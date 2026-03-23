/**
 * Compute min/max bounds across all segments for timeline visualization.
 */
import { parseTimerange, nanosToSeconds } from './timerange.js';

export interface TimelineBounds {
  min: number;
  max: number;
  range: number;
}

export function computeTimelineBounds(segs: Array<{ timerange?: string }>): TimelineBounds | null {
  if (!segs.length) return null;
  let minSec = Infinity, maxSec = -Infinity;
  for (const seg of segs) {
    const tr = parseTimerange(seg.timerange);
    if (tr.type === 'never') continue;
    if (tr.start) {
      const sec = nanosToSeconds(tr.start.nanos);
      if (sec < minSec) minSec = sec;
      if (sec > maxSec) maxSec = sec;
    }
    if (tr.end) {
      const sec = nanosToSeconds(tr.end.nanos);
      if (sec < minSec) minSec = sec;
      if (sec > maxSec) maxSec = sec;
    }
  }
  if (minSec === Infinity) return null;
  return { min: minSec, max: maxSec, range: maxSec - minSec || 1 };
}

/**
 * Compute CSS left% and width% for a segment bar within a timeline.
 */
export function segmentBarStyle(seg: { timerange?: string }, bounds: TimelineBounds | null): string {
  if (!bounds) return 'display:none';
  const tr = parseTimerange(seg.timerange);
  if (tr.type === 'never') return 'display:none';
  const startSec = tr.start ? nanosToSeconds(tr.start.nanos) : bounds.min;
  const endSec = tr.end ? nanosToSeconds(tr.end.nanos) : bounds.max;
  const left = ((startSec - bounds.min) / bounds.range * 100).toFixed(2);
  const width = (Math.max((endSec - startSec) / bounds.range * 100, 0.5)).toFixed(2);
  return `left:${left}%;width:${width}%`;
}
