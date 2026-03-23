/**
 * TAMS upload pipeline utilities for the Record/Ingest page.
 * Pure functions with no Svelte or DOM dependencies — fully testable.
 *
 * Upload pipeline: allocateObject -> uploadObject -> registerSegment
 */
import { apiGet, apiPost, apiPut, parsePagination } from './api.js';
import { errorMessage } from './errors.js';
import { buildSegmentsQuery } from './query.js';
import { formatTimerangeStr, parseTimerange, NANOS_PER_SEC_NUM as NANOS_PER_SEC } from './timerange.js';
import { buildTimerangeFromNanos } from './rational.js';
import type {
  Flow,
  Segment,
  PaginationInfo,
  IngestSettings,
  FrameRateOption,
  VideoCodecOption,
  AudioCodecOption,
  VideoQualityPreset,
  AudioQualityPreset,
  AssemblyItem,
  AssemblyResult,
  FlowCollectionItem,
} from '../types/tams.js';

/** NMOS/TAMS format URNs — use these instead of raw strings. */
export const FORMAT_VIDEO = 'urn:x-nmos:format:video';
export const FORMAT_AUDIO = 'urn:x-nmos:format:audio';
export const FORMAT_DATA  = 'urn:x-nmos:format:data';
export const FORMAT_MULTI = 'urn:x-nmos:format:multi';
export const FORMAT_IMAGE = 'urn:x-tam:format:image';

/** Dropdown options for format selectors (label + value). */
export const FORMAT_OPTIONS: Array<{ value: string; label: string }> = [
  { value: FORMAT_VIDEO, label: 'video' },
  { value: FORMAT_AUDIO, label: 'audio' },
  { value: FORMAT_IMAGE, label: 'image' },
  { value: FORMAT_DATA,  label: 'data' },
  { value: FORMAT_MULTI, label: 'multi' },
];

/** Video codec options — TS-compatible only (hls.js constraint). */
export const VIDEO_CODEC_OPTIONS: VideoCodecOption[] = [
  { id: 'avc',  label: 'H.264', tamsCodec: 'video/H264', container: 'video/mp2t' },
  { id: 'hevc', label: 'H.265', tamsCodec: 'video/H265', container: 'video/mp2t' },
];

/** Audio codec options — TS-compatible only. */
export const AUDIO_CODEC_OPTIONS: AudioCodecOption[] = [
  { id: 'aac', label: 'AAC', tamsCodec: 'audio/AAC', container: 'video/mp2t' },
];

/** Video quality presets. iframeOnly: true -> every frame is a keyframe. */
export const VIDEO_QUALITY_PRESETS: VideoQualityPreset[] = [
  { id: 'low',       label: 'Low (1 Mbps)',          bitrate: 1_000_000 },
  { id: 'medium',    label: 'Medium (2 Mbps)',       bitrate: 2_000_000 },
  { id: 'high',      label: 'High (4 Mbps)',         bitrate: 4_000_000 },
  { id: 'hq',        label: 'HQ (8 Mbps)',           bitrate: 8_000_000 },
  { id: 'broadcast', label: 'Broadcast (15 Mbps)',   bitrate: 15_000_000 },
  { id: 'studio',    label: 'Studio (25 Mbps)',      bitrate: 25_000_000 },
  { id: 'max',       label: 'Maximum (50 Mbps)',     bitrate: 50_000_000 },
  { id: 'iframe',    label: 'I-frame Only (25 Mbps)', bitrate: 25_000_000, iframeOnly: true },
];

/** Audio quality presets. */
export const AUDIO_QUALITY_PRESETS: AudioQualityPreset[] = [
  { id: 'low',    label: 'Low (64 kbps)',    bitrate: 64_000 },
  { id: 'medium', label: 'Medium (128 kbps)', bitrate: 128_000 },
  { id: 'high',   label: 'High (192 kbps)',  bitrate: 192_000 },
  { id: 'hq',     label: 'HQ (320 kbps)',    bitrate: 320_000 },
];

/** Allowed segment duration values in seconds. */
export const SEGMENT_DURATION_OPTIONS: number[] = [2, 4, 6, 8, 10, 15, 30];

/** Allowed keyframe interval values in seconds. */
export const KEYFRAME_INTERVAL_OPTIONS: number[] = [0.5, 1, 2, 4];

/**
 * Frame rate options as exact rationals {num, den}.
 * Float value = num/den. Stored as rationals to avoid precision loss
 * in keyframe interval and timerange calculations.
 */
export const FRAME_RATE_OPTIONS: FrameRateOption[] = [
  { num: 24000, den: 1001, label: '23.976' },
  { num: 24,    den: 1,    label: '24' },
  { num: 25,    den: 1,    label: '25' },
  { num: 30000, den: 1001, label: '29.97' },
  { num: 30,    den: 1,    label: '30' },
  { num: 50,    den: 1,    label: '50' },
  { num: 60000, den: 1001, label: '59.94' },
  { num: 60,    den: 1,    label: '60' },
];

const FRAME_RATE_MAP: Map<string, FrameRateOption> = new Map(FRAME_RATE_OPTIONS.map(f => [f.label, f]));

/**
 * Look up a FRAME_RATE_OPTIONS entry by its label string.
 */
export function getFrameRate(label: string): FrameRateOption {
  return FRAME_RATE_MAP.get(label) || FRAME_RATE_MAP.get('30')!;
}

/**
 * Resolve the effective keyFrameInterval for a quality preset + settings.
 * For I-frame only: den/num seconds (exact rational frame duration).
 * Otherwise: use the user's keyFrameInterval setting (default 1s).
 */
export function resolveKeyFrameInterval(preset: VideoQualityPreset, settings: IngestSettings): number {
  if (preset.iframeOnly) {
    const fr = getFrameRate(settings.frameRate);
    return fr.den / fr.num; // exact frame duration
  }
  return settings.keyFrameInterval;
}

// -- localStorage Settings ------------------------------------------------

const SETTINGS_KEY = 'tams-ingest-settings';

/** Default ingest settings. */
export const DEFAULT_INGEST_SETTINGS: IngestSettings = {
  videoCodec: 'avc',
  audioCodec: 'aac',
  videoQuality: 'medium',
  audioQuality: 'medium',
  segmentDuration: 6,
  frameRate: '30',      // label from FRAME_RATE_OPTIONS (rational lookup)
  keyFrameInterval: 1,
};

/** Load persisted ingest settings from localStorage, merged with defaults. */
export function loadIngestSettings(): IngestSettings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (!raw) return { ...DEFAULT_INGEST_SETTINGS };
    return { ...DEFAULT_INGEST_SETTINGS, ...JSON.parse(raw) };
  } catch {
    return { ...DEFAULT_INGEST_SETTINGS };
  }
}

/** Persist ingest settings to localStorage. */
export function saveIngestSettings(settings: IngestSettings): void {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch { /* quota exceeded or private mode */ }
}

const MAX_RETRIES = 3;
let _retryBaseDelayMs = 500;

/** Set the base delay for retry backoff (for testing). */
export function setRetryDelay(ms: number): void { _retryBaseDelayMs = ms; }

/** Default essence parameters and codecs per TAMS format. */
const FORMAT_DEFAULTS: Record<string, { essenceParameters: Record<string, number>; codec: string }> = {
  [FORMAT_VIDEO]: { essenceParameters: { frame_width: 1920, frame_height: 1080 }, codec: 'video/H264' },
  [FORMAT_AUDIO]: { essenceParameters: { sample_rate: 48000, channels: 2 }, codec: 'audio/AAC' },
};

export interface CodecConfig {
  tamsCodec: string;
  container: string;
}

export type SourceMode = 'new' | 'existing';

interface IngestFlowInput {
  sourceId: string;
  flowId: string;
  isAudioOnly: boolean;
  videoCodec: CodecConfig;
  audioCodec: CodecConfig;
  label: string;
  sourceDescription: string;
  sourceMode: SourceMode;
  existingSourceId?: string;
  /** Pre-generated audio flow ID. Pass null to skip audio flow creation. */
  audioFlowId?: string | null;
  /** Pre-generated audio source ID. Required when audioFlowId is non-null. */
  audioSourceId?: string | null;
  /** Audio description (defaults to "Audio — {label}"). */
  audioSourceDescription?: string;
  /** Video essence parameters (frame_width, frame_height). Uses FORMAT_DEFAULTS if omitted. */
  videoEssenceParameters?: Record<string, unknown>;
  /** Audio essence parameters (sample_rate, channels). Uses FORMAT_DEFAULTS if omitted. */
  audioEssenceParameters?: Record<string, unknown>;
}

/**
 * Build CreateFlowOpts for media ingest (file upload or webcam recording).
 * Extracted from the Svelte component so the codec/format/container mapping
 * is testable independently. All IDs must be pre-generated by the caller —
 * this function is pure.
 */
export function buildIngestFlowParams(input: IngestFlowInput): { primary: CreateFlowOpts; audio: CreateFlowOpts | null } {
  const { sourceId, flowId, isAudioOnly, videoCodec, audioCodec, label,
    sourceDescription, sourceMode, existingSourceId, audioFlowId, audioSourceId,
    audioSourceDescription, videoEssenceParameters, audioEssenceParameters } = input;

  if (sourceMode === 'existing' && !existingSourceId) {
    throw new Error('existingSourceId is required when sourceMode is "existing"');
  }
  const primarySourceId = sourceMode === 'new' ? sourceId : existingSourceId!;

  const primary: CreateFlowOpts = {
    sourceId: primarySourceId,
    flowId,
    format: isAudioOnly ? FORMAT_AUDIO : FORMAT_VIDEO,
    codec: isAudioOnly ? audioCodec.tamsCodec : videoCodec.tamsCodec,
    container: isAudioOnly ? audioCodec.container : videoCodec.container,
    label,
    essenceParameters: isAudioOnly ? audioEssenceParameters : videoEssenceParameters,
    setSourceMeta: sourceMode === 'new',
    sourceDescription,
    flowCollection: audioFlowId ? [{ id: flowId, role: 'video' }, { id: audioFlowId, role: 'audio' }] : undefined,
  };

  let audio: CreateFlowOpts | null = null;
  if (!isAudioOnly && audioFlowId) {
    if (!audioSourceId) {
      throw new Error('audioSourceId is required when audioFlowId is provided');
    }
    audio = {
      sourceId: audioSourceId,
      flowId: audioFlowId,
      format: FORMAT_AUDIO,
      codec: audioCodec.tamsCodec,
      container: audioCodec.container,
      label: `${label} — audio`,
      essenceParameters: audioEssenceParameters,
      setSourceMeta: true,
      sourceDescription: audioSourceDescription || `Audio — ${label}`,
    };
  }

  return { primary, audio };
}

interface CreateFlowOpts {
  sourceId: string;
  flowId: string;
  format: string;
  codec?: string;
  container?: string;
  label?: string;
  essenceParameters?: Record<string, unknown>;
  sourceLabel?: string;
  sourceDescription?: string;
  setSourceMeta?: boolean;
  flowCollection?: FlowCollectionItem[];
}

/**
 * Create a TAMS flow (which implicitly creates its source), then optionally
 * set source label and description via sub-resource PUTs.
 */
export async function createFlowWithSource(opts: CreateFlowOpts): Promise<void> {
  const defaults = FORMAT_DEFAULTS[opts.format] || {};
  const flowBody: Record<string, unknown> = {
    source_id: opts.sourceId,
    format: opts.format,
    essence_parameters: opts.essenceParameters || defaults.essenceParameters || {},
  };
  const codec = opts.codec || defaults.codec;
  if (codec) flowBody.codec = codec;
  if (opts.container) flowBody.container = opts.container;
  if (opts.label) flowBody.label = opts.label;
  if (opts.flowCollection?.length) flowBody.flow_collection = opts.flowCollection;

  await apiPut(`/flows/${opts.flowId}`, flowBody);

  if (opts.setSourceMeta !== false) {
    const srcLabel = opts.sourceLabel || opts.label;
    const metaUpdates: Array<Promise<unknown>> = [];
    if (srcLabel) metaUpdates.push(apiPut(`/sources/${opts.sourceId}/label`, srcLabel));
    if (opts.sourceDescription) metaUpdates.push(apiPut(`/sources/${opts.sourceId}/description`, opts.sourceDescription));
    if (metaUpdates.length) await Promise.allSettled(metaUpdates);
  }
}

/**
 * Build a TAMS timerange string from start/end seconds.
 * Format: [s:ns_s:ns) — inclusive start, exclusive end.
 */
export function buildTimerange(startSec: number, endSec: number): string {
  const startNanos = BigInt(Math.round(startSec * NANOS_PER_SEC));
  const endNanos = BigInt(Math.round(endSec * NANOS_PER_SEC));
  return formatTimerangeStr({
    type: 'range',
    start: { nanos: startNanos, inclusive: true },
    end: { nanos: endNanos, inclusive: false },
  });
}

/**
 * Allocate a media object for a flow via POST /flows/{flowId}/storage.
 */
export async function allocateObject(flowId: string): Promise<{ objectId: string; putUrl: string; contentType: string }> {
  const { data } = await apiPost(`/flows/${flowId}/storage`, { limit: 1 });
  const obj = (data as { media_objects: Array<{ object_id: string; put_url: { url: string; 'content-type'?: string } }> }).media_objects[0];
  return {
    objectId: obj.object_id,
    putUrl: obj.put_url.url,
    contentType: obj.put_url['content-type'] || 'video/mp2t',
  };
}

/**
 * Upload binary data to a presigned PUT URL.
 * The PUT URL is a presigned S3 URL (not the TAMS API),
 * so we use raw fetch rather than apiFetch.
 */
export async function uploadObject(putUrl: string, bytes: ArrayBuffer | Uint8Array, contentType: string): Promise<void> {
  const resp = await fetch(putUrl, {
    method: 'PUT',
    headers: { 'Content-Type': contentType },
    body: bytes as BodyInit,
  });
  if (!resp.ok) {
    throw new Error(`Upload failed: ${resp.status}`);
  }
}

/**
 * Register a segment for a flow via POST /flows/{flowId}/segments.
 */
export async function registerSegment(flowId: string, objectId: string, timerange: string): Promise<void> {
  await apiPost(`/flows/${flowId}/segments`, { object_id: objectId, timerange });
}

/**
 * Sleep for a given number of milliseconds.
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Upload a TS segment to TAMS: allocate -> upload binary -> register.
 * Retries up to MAX_RETRIES times with exponential backoff.
 */
export async function uploadSegment(
  flowId: string,
  bytes: ArrayBuffer | Uint8Array,
  startSec: number,
  endSec: number,
): Promise<{ objectId: string }> {
  const timerange = buildTimerange(startSec, endSec);
  let lastError: unknown;

  for (let attempt = 0; attempt < MAX_RETRIES; attempt++) {
    try {
      const { objectId, putUrl, contentType } = await allocateObject(flowId);
      await uploadObject(putUrl, bytes, contentType);
      await registerSegment(flowId, objectId, timerange);
      return { objectId };
    } catch (err) {
      lastError = err;
      if (attempt < MAX_RETRIES - 1) {
        await sleep(_retryBaseDelayMs * Math.pow(2, attempt));
      }
    }
  }

  throw lastError;
}

/**
 * Check if the browser supports WebCodecs APIs required for recording.
 */
export function checkWebCodecsSupport(): { supported: boolean; missing: string[] } {
  const missing: string[] = [];
  if (typeof globalThis.VideoEncoder === 'undefined') missing.push('VideoEncoder');
  if (typeof globalThis.MediaStreamTrackProcessor === 'undefined') missing.push('MediaStreamTrackProcessor');
  return { supported: missing.length === 0, missing };
}

// -- Assembly -------------------------------------------------------------

/** Segments per page when fetching paginated segment lists. */
const SEGMENT_PAGE_SIZE = 300;
/** Maximum segments to fetch before stopping (prevents unbounded memory use). */
const SEGMENT_MAX_ITEMS = 3000;

/**
 * Fetch all segments for a flow (paginated).
 * Set presigned=true to include presigned download URLs in the response.
 */
export async function fetchAllSegments(
  flowId: string,
  opts: { presigned?: boolean; maxItems?: number } = {},
): Promise<Segment[]> {
  const { presigned, maxItems = SEGMENT_MAX_ITEMS } = opts;
  const segs: Segment[] = [];
  let resp = await apiGet(buildSegmentsQuery(flowId, { limit: SEGMENT_PAGE_SIZE, presigned }));
  segs.push(...((resp.data as Segment[]) || []));
  let pag: PaginationInfo = parsePagination(resp.headers);
  while (pag.nextKey && segs.length < maxItems) {
    resp = await apiGet(buildSegmentsQuery(flowId, { limit: SEGMENT_PAGE_SIZE, presigned, page: pag.nextKey }));
    segs.push(...((resp.data as Segment[]) || []));
    pag = parsePagination(resp.headers);
  }
  if (pag.nextKey && segs.length >= maxItems) {
    console.warn(`[fetchAllSegments] Truncated at ${segs.length} segments for flow ${flowId}`);
  }
  return segs;
}

/**
 * Create an assembly flow from multiple source flows.
 * Registers all segments from each flow in sequence with contiguous timeranges.
 * Auto-creates a linked audio assembly flow if any source has audio.
 */
export async function createAssembly({ items, label }: { items: AssemblyItem[]; label: string }): Promise<AssemblyResult> {
  const template = items[0].flow;
  const videoFlowId = crypto.randomUUID();
  const videoSourceId = crypto.randomUUID();

  // Only create an assembly audio flow if ALL items have audio.
  // Mixing video-only and video+audio items produces an audio timeline
  // that doesn't cover the full video, which HLS cannot represent cleanly.
  const hasAudio = items.every(i => i.audioFlows.length > 0);
  let audioFlowId: string | null = null;
  let audioSourceId: string | null = null;

  // Create audio assembly flow first (if needed) so we can link via flow_collection
  if (hasAudio) {
    audioSourceId = crypto.randomUUID();
    audioFlowId = crypto.randomUUID();
    const audioTemplate = items.find(i => i.audioFlows.length > 0)!.audioFlows[0];
    await createFlowWithSource({
      sourceId: audioSourceId,
      flowId: audioFlowId,
      format: FORMAT_AUDIO,
      codec: audioTemplate.codec,
      container: audioTemplate.container,
      label: `${label} — audio`,
      sourceLabel: `${label} — audio`,
    });
  }

  // Create video assembly flow (with flow_collection linking to audio if present)
  await createFlowWithSource({
    sourceId: videoSourceId,
    flowId: videoFlowId,
    format: FORMAT_VIDEO,
    codec: template.codec,
    container: template.container,
    label,
    essenceParameters: template.essence_parameters,
    sourceLabel: label,
    sourceDescription: `Assembly of ${items.length} flows`,
    flowCollection: audioFlowId ? [{ id: videoFlowId, role: 'video' }, { id: audioFlowId, role: 'audio' }] : undefined,
  });

  // Tag as assembly
  await apiPut(`/flows/${videoFlowId}/tags/assembly`, ['true']);
  await apiPut(`/flows/${videoFlowId}/tags/edit_export`, ['true']);

  // Fetch all segments in parallel (one fetch per flow, video + audio)
  const allFetched = await Promise.all(
    items.map(async (item) => ({
      videoSegs: await fetchAllSegments(item.flow.id),
      audioSegs: (audioFlowId && item.audioFlows.length > 0)
        ? await fetchAllSegments(item.audioFlows[0].id)
        : [],
    }))
  );

  // Build registration list with shifted timeranges.
  // Compute duration from actual segments (flow.timerange may be absent).
  let videoOffset = 0n;
  let audioOffset = 0n;
  const toRegister: Array<{ targetFlowId: string; object_id: string; timerange: string }> = [];

  for (let idx = 0; idx < items.length; idx++) {
    const { videoSegs, audioSegs } = allFetched[idx];
    console.log(`[assembly] Item ${idx}: ${items[idx].flow.label || items[idx].flow.id.slice(0,8)}, ${videoSegs.length} video segs, ${audioSegs.length} audio segs`);

    // Single pass: compute span + collect shifted segments for video
    let flowMinNanos: bigint | null = null;
    let flowMaxNanos: bigint | null = null;
    const parsedVideoSegs: Array<{ object_id: string; startNanos: bigint; endNanos: bigint }> = [];
    for (const seg of videoSegs) {
      const tr = parseTimerange(seg.timerange);
      if (tr.type === 'never' || !tr.start || !tr.end) continue;
      if (flowMinNanos === null || tr.start.nanos < flowMinNanos) flowMinNanos = tr.start.nanos;
      if (flowMaxNanos === null || tr.end.nanos > flowMaxNanos) flowMaxNanos = tr.end.nanos;
      parsedVideoSegs.push({ object_id: seg.object_id, startNanos: tr.start.nanos, endNanos: tr.end.nanos });
    }
    const flowStart = flowMinNanos ?? 0n;
    const flowDuration = (flowMinNanos !== null && flowMaxNanos !== null) ? flowMaxNanos - flowMinNanos : 0n;
    console.log(`[assembly] Item ${idx}: flowStart=${flowStart}, flowDuration=${flowDuration}, videoOffset=${videoOffset}`);

    for (const seg of parsedVideoSegs) {
      toRegister.push({
        targetFlowId: videoFlowId,
        object_id: seg.object_id,
        timerange: buildTimerangeFromNanos(seg.startNanos - flowStart + videoOffset, seg.endNanos - flowStart + videoOffset),
      });
    }
    videoOffset += flowDuration;

    // Single pass for audio segments
    if (audioFlowId && audioSegs.length > 0) {
      let audioMin: bigint | null = null;
      let audioMax: bigint | null = null;
      const parsedAudioSegs: Array<{ object_id: string; startNanos: bigint; endNanos: bigint }> = [];
      for (const seg of audioSegs) {
        const tr = parseTimerange(seg.timerange);
        if (tr.type === 'never' || !tr.start || !tr.end) continue;
        if (audioMin === null || tr.start.nanos < audioMin) audioMin = tr.start.nanos;
        if (audioMax === null || tr.end.nanos > audioMax) audioMax = tr.end.nanos;
        parsedAudioSegs.push({ object_id: seg.object_id, startNanos: tr.start.nanos, endNanos: tr.end.nanos });
      }
      const audioFlowStart = audioMin ?? 0n;

      for (const seg of parsedAudioSegs) {
        toRegister.push({
          targetFlowId: audioFlowId,
          object_id: seg.object_id,
          timerange: buildTimerangeFromNanos(seg.startNanos - audioFlowStart + audioOffset, seg.endNanos - audioFlowStart + audioOffset),
        });
      }
      const audioDuration = (audioMin !== null && audioMax !== null) ? audioMax - audioMin : flowDuration;
      audioOffset += audioDuration;
    } else {
      audioOffset += flowDuration;
    }
  }

  // Register segments sequentially
  let failed = 0;
  for (const reg of toRegister) {
    try {
      await apiPost(`/flows/${reg.targetFlowId}/segments`, {
        object_id: reg.object_id,
        timerange: reg.timerange,
      });
    } catch (err: unknown) {
      console.warn(`[assembly] Failed: ${reg.timerange} obj:${reg.object_id.slice(0,8)} on flow:${reg.targetFlowId.slice(0,8)}`, errorMessage(err));
      failed++;
    }
  }

  return { videoFlowId, audioFlowId, totalSegments: toRegister.length, failed };
}
