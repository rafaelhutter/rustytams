/**
 * Generic thumbnail extraction service.
 *
 * Extracts the first video frame from a TAMS media segment via mediabunny,
 * renders it to a canvas, and returns a blob URL. Results are cached with
 * LRU eviction and proper blob URL revocation.
 *
 * Designed to be reusable across Gallery, FlowDetail, Sources, etc.
 * Not coupled to any Svelte component — pure JS with async API.
 */
import { selectPresignedUrl } from './api.js';
import { errorMessage } from './errors.js';
import { segmentDuration } from './hls.js';
import type { Flow, Segment } from '../types/tams.js';

declare global {
  interface Window {
    TAMS_DEBUG?: boolean;
  }
}

/** Set `window.TAMS_DEBUG = true` in the browser console to enable verbose thumbnail logging. */
function dbg(...args: unknown[]): void {
  if (typeof window !== 'undefined' && window.TAMS_DEBUG) console.log('[thumbnail]', ...args);
}
function dbgWarn(...args: unknown[]): void {
  if (typeof window !== 'undefined' && window.TAMS_DEBUG) console.warn('[thumbnail]', ...args);
}

// -- Cache ----------------------------------------------------------------

const cache: Map<string, string> = new Map();       // key -> blob URL
const accessOrder: string[] = [];                     // keys in LRU order (oldest first)
const MAX_CACHE = 80;
const EVICT_COUNT = 20;
let cacheDisabled = false;     // set true after clearThumbnailCache to reject late arrivals

function cacheGet(key: string): string | null {
  const url = cache.get(key);
  if (url) {
    // Move to end (most recently used)
    const idx = accessOrder.indexOf(key);
    if (idx !== -1) accessOrder.splice(idx, 1);
    accessOrder.push(key);
  }
  return url || null;
}

function cacheSet(key: string, blobUrl: string): void {
  if (cacheDisabled) {
    URL.revokeObjectURL(blobUrl); // don't leak the blob
    return;
  }
  if (cache.has(key)) {
    URL.revokeObjectURL(cache.get(key)!);
    const idx = accessOrder.indexOf(key);
    if (idx !== -1) accessOrder.splice(idx, 1);
  }
  cache.set(key, blobUrl);
  accessOrder.push(key);
  // LRU eviction
  if (cache.size > MAX_CACHE) {
    const toEvict = accessOrder.splice(0, EVICT_COUNT);
    for (const k of toEvict) {
      const url = cache.get(k);
      if (url) URL.revokeObjectURL(url);
      cache.delete(k);
    }
  }
}

/** Revoke all cached blob URLs and disable further caching. */
export function clearThumbnailCache(): void {
  cacheDisabled = true;
  for (const url of cache.values()) {
    URL.revokeObjectURL(url);
  }
  cache.clear();
  accessOrder.length = 0;
}

/** Re-enable caching (call when Gallery mounts). */
export function enableThumbnailCache(): void {
  cacheDisabled = false;
}

/** Check if a thumbnail is already cached for a key. */
export function hasThumbnail(key: string): boolean {
  return cache.has(key);
}

/** Get a cached thumbnail blob URL without triggering extraction. */
export function getCachedThumbnail(key: string): string | null {
  return cacheGet(key);
}

// -- Concurrency ----------------------------------------------------------

const MAX_CONCURRENT = 6;
let availableSlots: number = MAX_CONCURRENT;
const slotQueue: Array<() => void> = [];

async function acquireSlot(): Promise<void> {
  if (availableSlots > 0) {
    availableSlots--;
    return;
  }
  await new Promise<void>(resolve => slotQueue.push(resolve));
  // Slot was already reserved for us by releaseSlot — don't decrement again
}

function releaseSlot(): void {
  if (slotQueue.length > 0) {
    // Hand the slot directly to the next waiter (no decrement/increment race)
    const next = slotQueue.shift()!;
    next();
  } else {
    availableSlots++;
  }
}

// -- Adaptive Fetch -------------------------------------------------------

/**
 * Estimate initial fetch size for a segment based on flow metadata.
 */
function estimateInitialFetchSize(flow?: Flow): number {
  const ep = flow?.essence_parameters as Record<string, number> | undefined;
  if (ep?.frame_width && ep?.frame_height) {
    const pixels = ep.frame_width * ep.frame_height;
    if (pixels >= 3840 * 2160) return 4 * 1024 * 1024;  // 4K
    if (pixels >= 1920 * 1080) return 1.5 * 1024 * 1024; // 1080p
    if (pixels >= 1280 * 720) return 1024 * 1024;         // 720p
    return 512 * 1024;                                     // smaller
  }
  return 1024 * 1024; // default 1MB — partial fetches that are too small cause getCanvas to hang
}

const HARD_CAP = 4 * 1024 * 1024; // 4MB max fetch per attempt
const MAX_RANGE_RETRIES = 2;
const MAX_SEGMENT_ATTEMPTS = 3;

/**
 * Fetch partial segment data via Range request with adaptive sizing.
 */
async function fetchPartial(url: string, initialSize: number, signal?: AbortSignal): Promise<Uint8Array | null> {
  let size = Math.max(initialSize, 128 * 1024);

  for (let attempt = 0; attempt <= MAX_RANGE_RETRIES; attempt++) {
    const rangeEnd = Math.min(size, HARD_CAP) - 1;
    try {
      dbg(`fetch attempt=${attempt}, Range: bytes=0-${rangeEnd} (${Math.round(rangeEnd/1024)}KB)`);
      const resp = await fetch(url, {
        headers: { Range: `bytes=0-${rangeEnd}` },
        signal,
      });
      dbg(`fetch status=${resp.status}`);

      if (resp.status === 206) {
        const buf = await resp.arrayBuffer();
        dbg(`206 partial: ${buf.byteLength} bytes`);
        return new Uint8Array(buf);
      }

      if (resp.ok) {
        dbg(`${resp.status} full response — reading up to ${Math.round(size/1024)}KB`);
        // Server doesn't support Range — read up to our limit then abort
        const reader = resp.body!.getReader();
        const chunks: Uint8Array[] = [];
        let totalRead = 0;
        while (totalRead < size) {
          const { done, value } = await reader.read();
          if (done) break;
          chunks.push(value);
          totalRead += value.length;
          if (signal?.aborted) { reader.cancel(); return null; }
        }
        reader.cancel();
        const result = new Uint8Array(totalRead);
        let offset = 0;
        for (const chunk of chunks) {
          result.set(chunk, offset);
          offset += chunk.length;
        }
        return result;
      }

      return null; // HTTP error
    } catch {
      if (signal?.aborted) return null;
      if (attempt === MAX_RANGE_RETRIES) return null;
    }

    // Double the range for next attempt
    size = Math.min(size * 2, HARD_CAP);
  }
  return null;
}

// -- Frame Extraction -----------------------------------------------------

/**
 * Extract first video frame from segment data bytes using mediabunny CanvasSink.
 */
async function extractFrameFromBytes(bytes: Uint8Array, width: number, signal?: AbortSignal): Promise<Blob | null> {
  // mediabunny is a dynamic import — types are unknown
  const { Input, BlobSource, ALL_FORMATS, CanvasSink } = await import('mediabunny') as {
    Input: new (opts: { source: unknown; formats: unknown }) => { getTracks(): Promise<Array<{ type: string; getFirstTimestamp(): Promise<number | null> }>>; dispose(): void };
    BlobSource: new (blob: Blob) => unknown;
    ALL_FORMATS: unknown;
    CanvasSink: new (track: unknown, opts: { width: number }) => { getCanvas(ts: number): Promise<{ canvas: HTMLCanvasElement | OffscreenCanvas } | null> };
  };
  const blob = new Blob([bytes as BlobPart]);
  const input = new Input({
    source: new BlobSource(blob),
    formats: ALL_FORMATS,
  });

  try {
    if (signal?.aborted) return null;

    dbg(`extractFrame: ${bytes.length} bytes, width=${width}`);
    const tracks = await input.getTracks();
    dbg(`tracks: ${tracks.map(t => t.type).join(', ')}`);
    const videoTrack = tracks.find(t => t.type === 'video');
    if (!videoTrack) {
      dbgWarn('no video track in segment data');
      return null;
    }

    const sink = new CanvasSink(videoTrack, { width });
    const firstTimestamp = await videoTrack.getFirstTimestamp();
    dbg(`firstTimestamp: ${firstTimestamp}`);
    if (firstTimestamp === null || signal?.aborted) return null;
    let canvasTimer: ReturnType<typeof setTimeout>;
    const wrapped = await Promise.race([
      sink.getCanvas(firstTimestamp),
      new Promise<null>(resolve => {
        canvasTimer = setTimeout(() => {
          console.warn('[thumbnail] getCanvas timeout after 5s');
          resolve(null);
        }, 5000);
      }),
    ]);
    clearTimeout(canvasTimer!);
    if (!wrapped?.canvas || signal?.aborted) return null;

    const canvas = wrapped.canvas;

    // Export as JPEG blob — handle both OffscreenCanvas and HTMLCanvasElement
    if (typeof (canvas as OffscreenCanvas).convertToBlob === 'function') {
      return await (canvas as OffscreenCanvas).convertToBlob({ type: 'image/jpeg', quality: 0.7 });
    }
    if (typeof (canvas as HTMLCanvasElement).toBlob === 'function') {
      return await new Promise<Blob | null>(resolve => (canvas as HTMLCanvasElement).toBlob(resolve, 'image/jpeg', 0.7));
    }
    return null;
  } catch (err: unknown) {
    console.warn('[thumbnail] Frame extraction failed:', errorMessage(err));
    return null;
  } finally {
    try { input.dispose(); } catch { /* ignore */ }
  }
}

// -- Public API -----------------------------------------------------------

interface ExtractThumbnailOpts {
  key: string;
  segments: Segment[];
  flow?: Flow;
  width?: number;
  signal?: AbortSignal;
}

/**
 * Extract a thumbnail for a TAMS flow from its segments.
 *
 * Generic: works for any flow with segments. Caller provides the key
 * for caching (typically flowId), the segments to try, and optional
 * flow metadata for adaptive fetch sizing.
 */
export async function extractThumbnail({ key, segments, flow, width = 320, signal }: ExtractThumbnailOpts): Promise<string | null> {
  // Cache hit
  const cached = cacheGet(key);
  if (cached) return cached;

  if (!segments?.length) return null;
  if (signal?.aborted) return null;

  dbg(`key=${key.slice(0,8)}: waiting for slot (${MAX_CONCURRENT - availableSlots}/${MAX_CONCURRENT} in flight)`);
  await acquireSlot();
  dbg(`key=${key.slice(0,8)}: slot acquired`);
  try {
    // Re-check cache after acquiring slot (another extraction may have completed)
    const cached2 = cacheGet(key);
    if (cached2) return cached2;

    const initialSize = estimateInitialFetchSize(flow);

    // Try segments in order until we get a frame
    const attemptsLimit = Math.min(segments.length, MAX_SEGMENT_ATTEMPTS);
    for (let i = 0; i < attemptsLimit; i++) {
      if (signal?.aborted) return null;

      const url = selectPresignedUrl(segments[i]);
      if (!url) continue;

      dbg(`key=${key.slice(0,8)} seg=${i}: fetching ${Math.round(initialSize/1024)}KB`);
      const bytes = await fetchPartial(url, initialSize, signal);
      if (!bytes || signal?.aborted) {
        dbgWarn(`key=${key.slice(0,8)} seg=${i}: fetch returned ${bytes ? bytes.length : 'null'} bytes`);
        continue;
      }
      dbg(`key=${key.slice(0,8)} seg=${i}: got ${bytes.length} bytes, extracting...`);

      // Timeout the entire extraction (8s)
      let extractTimer: ReturnType<typeof setTimeout>;
      const jpegBlob = await Promise.race([
        extractFrameFromBytes(bytes, width, signal),
        new Promise<null>(resolve => {
          extractTimer = setTimeout(() => {
            console.warn(`[thumbnail] key=${key.slice(0,8)} seg=${i}: TIMEOUT after 8s`);
            resolve(null);
          }, 8000);
        }),
      ]);
      clearTimeout(extractTimer!);
      if (signal?.aborted) return null;

      if (jpegBlob) {
        dbg(`key=${key.slice(0,8)}: SUCCESS from seg=${i}, blob size=${jpegBlob.size}`);
        const blobUrl = URL.createObjectURL(jpegBlob);
        cacheSet(key, blobUrl);
        return blobUrl;
      }
      dbgWarn(`key=${key.slice(0,8)}: seg=${i} produced no frame, trying next`);
    }

    dbgWarn(`key=${key.slice(0,8)}: all ${attemptsLimit} segment attempts failed`);

    return null;
  } finally {
    releaseSlot();
  }
}
