/**
 * Client-side HLS manifest (m3u8) generation from TAMS segments.
 *
 * Builds a VOD playlist from segments with presigned URLs and
 * durations computed from TAMS timeranges. Returns a blob URL
 * that can be passed to hls.js / Omakase Player.
 */
import { parseTimerange, nanosToSeconds } from './timerange.js';
import { selectPresignedUrl } from './api.js';
import type { Segment } from '../types/tams.js';

/**
 * Compute segment duration in seconds from a TAMS timerange string.
 * Falls back to a default if the timerange is unbounded or unparseable.
 */
export function segmentDuration(trStr: string | undefined, fallback: number = 6): number {
  if (!trStr) return fallback;
  const tr = parseTimerange(trStr);
  if (tr.type === 'never') return fallback;
  if (!tr.start || !tr.end) return fallback;
  const dur = nanosToSeconds(tr.end.nanos - tr.start.nanos);
  return dur > 0 ? dur : fallback;
}

/**
 * Build an HLS VOD m3u8 playlist string from TAMS segments.
 * Segments must already be sorted by timerange (ascending).
 */
export function buildM3u8String(segments: Segment[]): string | null {
  // Single pass: filter to playable, compute duration via segmentDuration
  const playable: Array<{ url: string; dur: number }> = [];
  for (const seg of segments) {
    const url = selectPresignedUrl(seg);
    if (!url) continue;
    playable.push({ url, dur: segmentDuration(seg.timerange) });
  }
  if (playable.length === 0) return null;

  let maxDuration = 0;
  const entries: string[] = [];

  for (let i = 0; i < playable.length; i++) {
    const { url, dur } = playable[i];
    if (dur > maxDuration) maxDuration = dur;

    // Add discontinuity between every segment. Each TAMS media object is
    // independently encoded (separate PAT/PMT/PTS), so hls.js must reset
    // its PTS tracking at every boundary — even when TAMS timeranges are
    // contiguous.
    if (i > 0) {
      entries.push('#EXT-X-DISCONTINUITY');
    }

    entries.push(`#EXTINF:${dur.toFixed(6)},`);
    entries.push(url);
  }

  const lines: string[] = [
    '#EXTM3U',
    '#EXT-X-VERSION:3',
    `#EXT-X-TARGETDURATION:${Math.ceil(maxDuration)}`,
    '#EXT-X-PLAYLIST-TYPE:VOD',
    '#EXT-X-MEDIA-SEQUENCE:0',
    ...entries,
    '#EXT-X-ENDLIST',
  ];

  return lines.join('\n');
}

/**
 * Create a blob URL from an m3u8 string.
 */
export function m3u8BlobUrl(content: string): string {
  return URL.createObjectURL(new Blob([content], { type: 'application/vnd.apple.mpegurl' }));
}

/**
 * Build an m3u8 playlist and return a blob URL.
 */
export function buildM3u8BlobUrl(segments: Segment[]): string | null {
  const content = buildM3u8String(segments);
  if (!content) return null;
  return m3u8BlobUrl(content);
}

interface AudioTrack {
  name: string;
  url: string;
}

/**
 * Build an HLS master playlist (multivariant) that combines a video variant
 * with alternate audio renditions. All URLs must be absolute (blob: URLs work).
 *
 * HLS.js will parse this, fetch each media playlist, and sync audio/video
 * playback automatically — no sidecar API needed.
 */
export function buildMasterM3u8String(videoPlaylistUrl: string, audioTracks: AudioTrack[] = []): string {
  const lines: string[] = ['#EXTM3U', '#EXT-X-VERSION:3'];
  if (audioTracks.length > 0) {
    for (let i = 0; i < audioTracks.length; i++) {
      const t = audioTracks[i];
      const def = i === 0 ? 'YES' : 'NO';
      // Strip quotes from name to avoid breaking HLS attribute syntax
      const safeName = (t.name || 'Audio').replace(/"/g, '');
      lines.push(`#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="${safeName}",DEFAULT=${def},AUTOSELECT=${def},URI="${t.url}"`);
    }
    lines.push('#EXT-X-STREAM-INF:BANDWIDTH=2000000,AUDIO="audio"');
  } else {
    lines.push('#EXT-X-STREAM-INF:BANDWIDTH=2000000');
  }
  lines.push(videoPlaylistUrl);
  return lines.join('\n');
}

/**
 * Revoke a previously created manifest blob URL.
 */
export function revokeManifest(url: string | null): void {
  if (url && url.startsWith('blob:')) {
    URL.revokeObjectURL(url);
  }
}

/**
 * Compute the time span from the first segment's start to the last segment's end, in seconds.
 * Note: for non-contiguous segments this is larger than the sum of individual durations.
 */
export function segmentsTimespan(segments: Segment[]): number {
  if (!segments?.length) return 0;
  // Use overall timerange: first segment start to last segment end
  const first = parseTimerange(segments[0]?.timerange);
  const last = parseTimerange(segments[segments.length - 1]?.timerange);
  if (first.type === 'never' || last.type === 'never') return 0;
  if (!first.start || !last.end) return 0;
  return nanosToSeconds(last.end.nanos - first.start.nanos);
}

/**
 * Compute the start time offset (in seconds) of the first segment.
 * Used to align segment markers to the video timeline (marker positions
 * are relative to video time 0, but segments may start at a non-zero
 * TAMS timestamp).
 */
export function segmentStartOffset(segments: Segment[]): number {
  if (!segments?.length) return 0;
  const first = parseTimerange(segments[0]?.timerange);
  if (first.type === 'never' || !first.start) return 0;
  return nanosToSeconds(first.start.nanos);
}
