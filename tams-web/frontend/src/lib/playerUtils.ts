/**
 * Pure utility functions for the Player page.
 * Extracted for testability — no Svelte or DOM dependencies.
 */
import { parseTimerange, nanosToSeconds, NANOS_PER_SEC_NUM } from './timerange.js';
import type { Segment } from '../types/tams.js';

interface Marker {
  id: string;
  start: number;
  end: number;
}

interface LabeledMarker {
  label: string;
  start: number;
  end: number;
}

/**
 * Find all marker IDs that overlap with at least one other marker.
 * Two markers overlap when a.start < b.end && b.start < a.end (exclusive boundaries).
 */
export function detectOverlaps(markers: Marker[]): Set<string> {
  const ids = new Set<string>();
  for (let i = 0; i < markers.length; i++) {
    for (let j = i + 1; j < markers.length; j++) {
      const a = markers[i];
      const b = markers[j];
      if (a.start < b.end && b.start < a.end) {
        ids.add(a.id);
        ids.add(b.id);
      }
    }
  }
  return ids;
}

/**
 * Find segments that overlap with a time range (in seconds, with TAMS offset).
 */
export function findOverlappingSegments(
  segments: Segment[],
  startSec: number,
  endSec: number,
  startOffset: number,
): Segment[] {
  const startNanos = BigInt(Math.round((startSec + startOffset) * NANOS_PER_SEC_NUM));
  const endNanos = BigInt(Math.round((endSec + startOffset) * NANOS_PER_SEC_NUM));
  return segments.filter(seg => {
    const tr = parseTimerange(seg.timerange);
    if (tr.type === 'never' || !tr.start || !tr.end) return false;
    return tr.end.nanos > startNanos && tr.start.nanos < endNanos;
  });
}

/**
 * Validate segmentation markers before export.
 * Checks for zero/negative duration and missing segment coverage.
 */
export function validateExport(
  markers: LabeledMarker[],
  startOffset: number,
  segments: Segment[],
): string[] {
  const errors: string[] = [];

  for (const marker of markers) {
    const dur = marker.end - marker.start;
    if (dur <= 0) {
      errors.push(`${marker.label}: zero or negative duration`);
      continue;
    }
    if (findOverlappingSegments(segments, marker.start, marker.end, startOffset).length === 0) {
      errors.push(`${marker.label}: no segments in range`);
    }
  }

  return errors;
}

/**
 * Format seconds as M:SS.SS for timecode display.
 */
export function formatSeconds(s: number, decimals: number = 2): string {
  if (!isFinite(s)) return '--';
  const m = Math.floor(s / 60);
  const sec = (s % 60).toFixed(decimals);
  const padLen = decimals > 0 ? 3 + decimals : 2;
  return `${m}:${sec.padStart(padLen, '0')}`;
}
