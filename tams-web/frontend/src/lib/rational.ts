/**
 * Exact rational time arithmetic for media pipelines.
 *
 * All internal representations use BigInt nanoseconds to avoid float precision
 * loss. Use these functions instead of float multiplication when computing
 * segment boundaries, frame durations, or any time value that feeds into
 * TAMS timerange registration.
 *
 * Float-based buildTimerange() in ingest.ts is still fine for UI-derived
 * values (marker dragging, seeking) where floats are inherent.
 */
import { formatTimerangeStr, NANOS_PER_SEC, nanosToSeconds } from './timerange.js';

/**
 * Convert a rational number {num, den} to BigInt nanoseconds.
 * Result = (den * 1_000_000_000) / num (for "seconds per unit" rationals)
 * or = (num * 1_000_000_000) / den (for "units per second" rationals).
 *
 * For frame rates: frameDurationNanos({num: 30000, den: 1001}) = 1001e9/30000
 * BigInt division truncates towards zero. For 23.976fps (24000/1001):
 * exact = 41708333.33...ns, result = 41708333n (floor). Error < 1ns per frame,
 * < 1 microsecond per 1000 frames — well within TAMS precision requirements.
 */
export function frameDurationNanos(rate: { num: number; den: number }): bigint {
  return (BigInt(rate.den) * NANOS_PER_SEC) / BigInt(rate.num);
}

/**
 * Convert an integer number of seconds to BigInt nanoseconds.
 */
export function secondsToNanos(seconds: number): bigint {
  return BigInt(seconds) * NANOS_PER_SEC;
}

/**
 * Convert a float seconds value to BigInt nanoseconds (with rounding).
 * Use only when the input is inherently a float (e.g. mediabunny duration).
 * Prefer secondsToNanos() for integer values.
 */
export function floatSecondsToNanos(seconds: number): bigint {
  return BigInt(Math.round(seconds * Number(NANOS_PER_SEC)));
}

/**
 * Build a TAMS timerange string from BigInt nanosecond boundaries.
 * Inclusive start, exclusive end: [startNanos, endNanos)
 */
export function buildTimerangeFromNanos(startNanos: bigint, endNanos: bigint): string {
  return formatTimerangeStr({
    type: 'range',
    start: { nanos: startNanos, inclusive: true },
    end: { nanos: endNanos, inclusive: false },
  });
}

/**
 * Compute exact segment boundary nanoseconds for a given segment index.
 * segStart = index * segmentDurationNanos
 * segEnd   = (index + 1) * segmentDurationNanos, clamped to totalDurationNanos
 */
export function segmentBounds(
  index: number,
  segmentDurationNanos: bigint,
  totalDurationNanos?: bigint,
): { startNanos: bigint; endNanos: bigint } {
  const startNanos = BigInt(index) * segmentDurationNanos;
  let endNanos = BigInt(index + 1) * segmentDurationNanos;
  if (totalDurationNanos !== undefined && endNanos > totalDurationNanos) {
    endNanos = totalDurationNanos;
  }
  return { startNanos, endNanos };
}

/**
 * Convert BigInt nanoseconds to float seconds (for passing to mediabunny
 * or other APIs that expect float). Display only — do not round-trip.
 * Re-exported from timerange.js for convenience.
 */
export { nanosToSeconds as nanosToFloat } from './timerange.js';
