import { describe, it, expect } from 'vitest';
import {
  frameDurationNanos,
  secondsToNanos,
  floatSecondsToNanos,
  buildTimerangeFromNanos,
  segmentBounds,
  nanosToFloat,
} from '../lib/rational.js';

describe('secondsToNanos', () => {
  it('converts integer seconds exactly', () => {
    expect(secondsToNanos(0)).toBe(0n);
    expect(secondsToNanos(1)).toBe(1_000_000_000n);
    expect(secondsToNanos(6)).toBe(6_000_000_000n);
    expect(secondsToNanos(30)).toBe(30_000_000_000n);
  });
});

describe('floatSecondsToNanos', () => {
  it('converts whole seconds', () => {
    expect(floatSecondsToNanos(6)).toBe(6_000_000_000n);
  });

  it('converts fractional seconds', () => {
    expect(floatSecondsToNanos(0.5)).toBe(500_000_000n);
    expect(floatSecondsToNanos(18.5)).toBe(18_500_000_000n);
  });

  it('rounds to nearest nanosecond', () => {
    const result = floatSecondsToNanos(1 / 3);
    // 0.333... * 1e9 = 333333333.333... -> rounds to 333333333
    expect(result).toBe(333_333_333n);
  });
});

describe('frameDurationNanos', () => {
  it('computes exact frame duration at 30fps', () => {
    const nanos = frameDurationNanos({ num: 30, den: 1 });
    expect(nanos).toBe(33_333_333n); // 1/30 sec = 33.333... ms, truncated
  });

  it('computes exact frame duration at 29.97fps (30000/1001)', () => {
    const nanos = frameDurationNanos({ num: 30000, den: 1001 });
    // 1001 * 1_000_000_000 / 30000 = 33366666.666... -> 33366666n (truncated)
    expect(nanos).toBe(33_366_666n);
  });

  it('computes exact frame duration at 24fps', () => {
    const nanos = frameDurationNanos({ num: 24, den: 1 });
    expect(nanos).toBe(41_666_666n); // 1/24 sec truncated
  });

  it('computes exact frame duration at 23.976fps (24000/1001)', () => {
    const nanos = frameDurationNanos({ num: 24000, den: 1001 });
    // 1001 * 1_000_000_000 / 24000 = 41708333.333... -> 41708333n
    expect(nanos).toBe(41_708_333n);
  });

  it('computes exact frame duration at 25fps', () => {
    const nanos = frameDurationNanos({ num: 25, den: 1 });
    expect(nanos).toBe(40_000_000n); // 1/25 sec = exactly 40ms
  });

  it('computes exact frame duration at 60fps', () => {
    const nanos = frameDurationNanos({ num: 60, den: 1 });
    expect(nanos).toBe(16_666_666n);
  });

  it('computes exact frame duration at 59.94fps (60000/1001)', () => {
    const nanos = frameDurationNanos({ num: 60000, den: 1001 });
    expect(nanos).toBe(16_683_333n);
  });
});

describe('segmentBounds', () => {
  it('computes contiguous segments for integer duration', () => {
    const segDur = secondsToNanos(6);
    const seg0 = segmentBounds(0, segDur);
    const seg1 = segmentBounds(1, segDur);
    const seg2 = segmentBounds(2, segDur);

    expect(seg0.startNanos).toBe(0n);
    expect(seg0.endNanos).toBe(6_000_000_000n);
    expect(seg1.startNanos).toBe(6_000_000_000n);
    expect(seg1.endNanos).toBe(12_000_000_000n);
    expect(seg2.startNanos).toBe(12_000_000_000n);
    expect(seg2.endNanos).toBe(18_000_000_000n);

    // Contiguous: each segment's end equals next segment's start
    expect(seg0.endNanos).toBe(seg1.startNanos);
    expect(seg1.endNanos).toBe(seg2.startNanos);
  });

  it('clamps last segment to total duration', () => {
    const segDur = secondsToNanos(6);
    const totalDur = floatSecondsToNanos(18.5);
    const seg3 = segmentBounds(3, segDur, totalDur);

    expect(seg3.startNanos).toBe(18_000_000_000n);
    expect(seg3.endNanos).toBe(18_500_000_000n); // clamped
  });

  it('handles zero index', () => {
    const segDur = secondsToNanos(10);
    const seg = segmentBounds(0, segDur);
    expect(seg.startNanos).toBe(0n);
    expect(seg.endNanos).toBe(10_000_000_000n);
  });

  it('preserves exact contiguity over many segments', () => {
    const segDur = secondsToNanos(6);
    // After 100 segments, the 100th segment should start at exactly 600s
    const seg100 = segmentBounds(100, segDur);
    expect(seg100.startNanos).toBe(600_000_000_000n);
    // No float drift -- pure BigInt multiplication
  });

  it('preserves contiguity with frame-rate-based duration', () => {
    // 100 frames at 29.97fps, each segment = 1 frame
    const frameDur = frameDurationNanos({ num: 30000, den: 1001 });
    const bounds: Array<{ startNanos: bigint; endNanos: bigint }> = [];
    for (let i = 0; i < 100; i++) {
      bounds.push(segmentBounds(i, frameDur));
    }
    // Check contiguity
    for (let i = 1; i < bounds.length; i++) {
      expect(bounds[i].startNanos).toBe(bounds[i - 1].endNanos);
    }
    // After 100 frames at 29.97fps: 100 * 33366666n = 3336666600n
    expect(bounds[99].endNanos).toBe(100n * 33_366_666n);
  });
});

describe('buildTimerangeFromNanos', () => {
  it('formats a range from BigInt nanoseconds', () => {
    const tr = buildTimerangeFromNanos(0n, 6_000_000_000n);
    expect(tr).toBe('[0:0_6:0)');
  });

  it('formats fractional nanoseconds', () => {
    const tr = buildTimerangeFromNanos(6_000_000_000n, 12_500_000_000n);
    expect(tr).toBe('[6:0_12:500000000)');
  });

  it('formats frame-rate-derived boundaries', () => {
    const frameDur = frameDurationNanos({ num: 30000, den: 1001 });
    const tr = buildTimerangeFromNanos(0n, frameDur);
    expect(tr).toBe('[0:0_0:33366666)');
  });
});

describe('nanosToFloat', () => {
  it('converts BigInt nanos to float seconds', () => {
    expect(nanosToFloat(0n)).toBe(0);
    expect(nanosToFloat(1_000_000_000n)).toBe(1);
    expect(nanosToFloat(6_000_000_000n)).toBe(6);
    expect(nanosToFloat(500_000_000n)).toBe(0.5);
  });

  it('converts frame duration nanos to approximate float', () => {
    const nanos = frameDurationNanos({ num: 30000, den: 1001 });
    expect(nanosToFloat(nanos)).toBeCloseTo(1001 / 30000, 6);
  });
});
