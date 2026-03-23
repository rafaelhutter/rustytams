/**
 * TAMS timerange utilities.
 *
 * TAMS timestamps use "seconds:nanoseconds" format (e.g. "8:399999999").
 * Internally stored as BigInt nanoseconds for precision — never converted to
 * float for storage or comparison.
 *
 * Timerange wire format:
 *   "_"              — eternity (all time)
 *   "()"             — never (empty)
 *   "[0:0_10:0)"     — standard range (inclusive start, exclusive end)
 *   "[10:0]"         — instantaneous (single point, both inclusive)
 *   "_10:0)"         — open-ended start (unbounded to a bound)
 *   "(5:0_"          — open-ended end (from a bound to unbounded)
 *   "-1:500000000"   — negative timestamps supported
 *
 * Brackets: [ ] = inclusive, ( ) = exclusive.
 * Exclusive markers on instantaneous → treated as never.
 * End before start → treated as never.
 */

import type { Bound, TimeRangeParsed } from '../types/tams.js';

export const NANOS_PER_SEC: bigint = 1_000_000_000n;
export const NANOS_PER_SEC_NUM: number = 1_000_000_000;

/**
 * Parse a TAMS timestamp "seconds:nanoseconds" to BigInt nanoseconds.
 */
export function parseTimestamp(ts: string): bigint | null {
  if (!ts || typeof ts !== 'string') return null;
  const colon = ts.indexOf(':');
  if (colon === -1) return null;
  const secsStr = ts.slice(0, colon);
  const nanosStr = ts.slice(colon + 1);
  if (secsStr === '' || nanosStr === '') return null;

  // Validate: no leading zeros (except "0" itself)
  const secsAbs = secsStr.startsWith('-') ? secsStr.slice(1) : secsStr;
  if (secsAbs === '' || (secsAbs.length > 1 && secsAbs[0] === '0')) return null;
  if (nanosStr.length > 1 && nanosStr[0] === '0') return null;

  // Validate digits
  if (!/^-?\d+$/.test(secsStr) || !/^\d+$/.test(nanosStr)) return null;

  try {
    const secs = BigInt(secsStr);
    const nanos = BigInt(nanosStr);
    if (nanos > 999_999_999n) return null;
    return secs * NANOS_PER_SEC + nanos;
  } catch {
    return null;
  }
}

/**
 * Format BigInt nanoseconds back to TAMS timestamp string.
 * Round-trips: formatTimestampStr(parseTimestamp("8:399999999")) === "8:399999999"
 * Uses Euclidean division so negative timestamps display correctly.
 */
export function formatTimestampStr(totalNanos: bigint): string {
  if (typeof totalNanos !== 'bigint') return '--';
  // Euclidean division: nanos always non-negative
  let secs = totalNanos / NANOS_PER_SEC;
  let nanos = totalNanos % NANOS_PER_SEC;
  if (nanos < 0n) {
    secs -= 1n;
    nanos += NANOS_PER_SEC;
  }
  return `${secs}:${nanos}`;
}

/**
 * Parse a TAMS timerange string into a structured object.
 */
export function parseTimerange(tr: string | undefined): TimeRangeParsed {
  if (!tr || tr === '()') return { type: 'never', start: null, end: null };
  if (tr === '_') return { type: 'range', start: null, end: null }; // eternity

  const len = tr.length;
  let pos = 0;

  // Parse optional start bracket
  let startInclusive: boolean | null = null;
  if (tr[0] === '[') { startInclusive = true; pos = 1; }
  else if (tr[0] === '(') { startInclusive = false; pos = 1; }

  // Parse optional end bracket
  let endInclusive: boolean | null = null;
  if (tr[len - 1] === ']') { endInclusive = true; }
  else if (tr[len - 1] === ')') { endInclusive = false; }

  const contentEnd = endInclusive !== null ? len - 1 : len;
  const content = tr.slice(pos, contentEnd);

  const sepIdx = content.indexOf('_');

  if (sepIdx !== -1) {
    // Range with separator
    const startStr = content.slice(0, sepIdx);
    const endStr = content.slice(sepIdx + 1);

    const start: Bound | null = startStr === '' ? null : {
      nanos: parseTimestamp(startStr)!,
      inclusive: startInclusive !== null ? startInclusive : true,
    };
    const end: Bound | null = endStr === '' ? null : {
      nanos: parseTimestamp(endStr)!,
      inclusive: endInclusive !== null ? endInclusive : true,
    };

    // Validate timestamps parsed
    if (start && start.nanos === null) return { type: 'never', start: null, end: null };
    if (end && end.nanos === null) return { type: 'never', start: null, end: null };

    // Degenerate: end before start → never
    if (start && end) {
      if (start.nanos > end.nanos) return { type: 'never', start: null, end: null };
      if (start.nanos === end.nanos && (!start.inclusive || !end.inclusive)) {
        return { type: 'never', start: null, end: null };
      }
    }

    return { type: 'range', start, end };
  } else {
    // No separator: instantaneous like "[10:0]"
    if (content === '') return { type: 'never', start: null, end: null };

    const nanos = parseTimestamp(content);
    if (nanos === null) return { type: 'never', start: null, end: null };

    const sInc = startInclusive !== null ? startInclusive : true;
    const eInc = endInclusive !== null ? endInclusive : true;

    // Exclusive markers on instantaneous = never
    if (!sInc || !eInc) return { type: 'never', start: null, end: null };

    return {
      type: 'range',
      start: { nanos, inclusive: true },
      end: { nanos, inclusive: true },
    };
  }
}

/**
 * Format a parsed TimeRange back to wire format.
 * Round-trips: formatTimerangeStr(parseTimerange(s)) === s
 */
export function formatTimerangeStr(tr: TimeRangeParsed): string {
  if (tr.type === 'never') return '()';
  if (!tr.start && !tr.end) return '_'; // eternity

  if (tr.start && tr.end) {
    // Instantaneous: same timestamp, both inclusive
    if (tr.start.nanos === tr.end.nanos && tr.start.inclusive && tr.end.inclusive) {
      return `[${formatTimestampStr(tr.start.nanos)}]`;
    }
    const sb = tr.start.inclusive ? '[' : '(';
    const eb = tr.end.inclusive ? ']' : ')';
    return `${sb}${formatTimestampStr(tr.start.nanos)}_${formatTimestampStr(tr.end.nanos)}${eb}`;
  }

  if (tr.start && !tr.end) {
    const sb = tr.start.inclusive ? '[' : '(';
    return `${sb}${formatTimestampStr(tr.start.nanos)}_`;
  }

  // !tr.start && tr.end
  const eb = tr.end!.inclusive ? ']' : ')';
  return `_${formatTimestampStr(tr.end!.nanos)}${eb}`;
}

/**
 * Convert BigInt nanoseconds to float seconds (for display only, not storage).
 */
export function nanosToSeconds(nanos: bigint): number {
  if (typeof nanos !== 'bigint') return NaN;
  return Number(nanos) / 1_000_000_000;
}

const pad2 = (n: number): string => String(n).padStart(2, '0');
const pad3 = (n: number): string => String(n).padStart(3, '0');

/**
 * Format seconds as HH:MM:SS.mmm (for display only).
 */
export function formatDuration(totalSeconds: number): string {
  if (!isFinite(totalSeconds) || totalSeconds < 0) return '--:--:--';
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = Math.floor(totalSeconds % 60);
  const ms = Math.floor((totalSeconds % 1) * 1000);
  return `${pad2(h)}:${pad2(m)}:${pad2(s)}.${pad3(ms)}`;
}

/**
 * Checks if a parsed TimeRange represents eternity.
 */
export function isEternity(tr: TimeRangeParsed): boolean {
  return tr.type === 'range' && !tr.start && !tr.end;
}

/**
 * Checks if a parsed TimeRange represents never.
 */
export function isNever(tr: TimeRangeParsed): boolean {
  return tr.type === 'never';
}

/**
 * Format a timerange wire string for human display.
 * Shows raw rational string + human-readable duration.
 */
export function formatTimerangeDisplay(trStr: string | undefined): { raw: string; display: string } {
  if (!trStr) return { raw: '--', display: '--' };

  const tr = parseTimerange(trStr);
  if (isNever(tr)) return { raw: trStr, display: 'never' };
  if (isEternity(tr)) return { raw: trStr, display: 'eternity' };

  const parts: string[] = [];
  if (tr.start) {
    parts.push(formatDuration(nanosToSeconds(tr.start.nanos)));
  } else {
    parts.push('...');
  }
  parts.push(' - ');
  if (tr.end) {
    parts.push(formatDuration(nanosToSeconds(tr.end.nanos)));
  } else {
    parts.push('...');
  }

  // Duration if both bounds present
  if (tr.start && tr.end) {
    const dur = nanosToSeconds(tr.end.nanos - tr.start.nanos);
    parts.push(` (${formatDuration(dur)})`);
  }

  return { raw: trStr, display: parts.join('') };
}
