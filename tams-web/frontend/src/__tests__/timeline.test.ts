import { describe, it, expect } from 'vitest';
import { computeTimelineBounds, segmentBarStyle } from '../lib/timeline.js';
import type { TimelineBounds } from '../lib/timeline.js';

describe('computeTimelineBounds', () => {
  it('returns null for empty array', () => {
    expect(computeTimelineBounds([])).toBeNull();
  });

  it('returns null when all segments have no timerange', () => {
    expect(computeTimelineBounds([{ timerange: null as unknown as string }, { timerange: '()' }])).toBeNull();
  });

  it('computes bounds for a single segment', () => {
    const bounds = computeTimelineBounds([{ timerange: '[0:0_10:0)' }]) as TimelineBounds;
    expect(bounds.min).toBe(0);
    expect(bounds.max).toBe(10);
    expect(bounds.range).toBe(10);
  });

  it('computes bounds across multiple segments', () => {
    const bounds = computeTimelineBounds([
      { timerange: '[5:0_10:0)' },
      { timerange: '[0:0_3:0)' },
      { timerange: '[8:0_20:0)' },
    ]) as TimelineBounds;
    expect(bounds.min).toBe(0);
    expect(bounds.max).toBe(20);
    expect(bounds.range).toBe(20);
  });

  it('handles nanoseconds in timestamps', () => {
    const bounds = computeTimelineBounds([{ timerange: '[0:500000000_1:500000000)' }]) as TimelineBounds;
    expect(bounds.min).toBeCloseTo(0.5, 5);
    expect(bounds.max).toBeCloseTo(1.5, 5);
    expect(bounds.range).toBeCloseTo(1.0, 5);
  });

  it('skips segments with empty or never timeranges', () => {
    const bounds = computeTimelineBounds([
      { timerange: '()' },
      { timerange: '[5:0_15:0)' },
      { timerange: null as unknown as string },
    ]) as TimelineBounds;
    expect(bounds.min).toBe(5);
    expect(bounds.max).toBe(15);
  });

  it('returns range=1 when all timestamps are equal', () => {
    const bounds = computeTimelineBounds([{ timerange: '[5:0_5:0]' }]) as TimelineBounds;
    expect(bounds.min).toBe(5);
    expect(bounds.max).toBe(5);
    expect(bounds.range).toBe(1);
  });
});

describe('segmentBarStyle', () => {
  const bounds: TimelineBounds = { min: 0, max: 100, range: 100 };

  it('returns display:none when no bounds', () => {
    expect(segmentBarStyle({ timerange: '[0:0_10:0)' }, null)).toBe('display:none');
  });

  it('returns display:none for never timerange', () => {
    expect(segmentBarStyle({ timerange: '()' }, bounds)).toBe('display:none');
  });

  it('returns display:none for null timerange', () => {
    expect(segmentBarStyle({ timerange: null as unknown as string }, bounds)).toBe('display:none');
  });

  it('computes correct left and width for full range', () => {
    const style = segmentBarStyle({ timerange: '[0:0_100:0)' }, bounds);
    expect(style).toBe('left:0.00%;width:100.00%');
  });

  it('computes correct left and width for partial range', () => {
    const style = segmentBarStyle({ timerange: '[25:0_75:0)' }, bounds);
    expect(style).toBe('left:25.00%;width:50.00%');
  });

  it('enforces minimum width of 0.5%', () => {
    const style = segmentBarStyle({ timerange: '[50:0_50:0]' }, bounds);
    expect(style).toContain('width:0.50%');
  });

  it('handles nanosecond precision', () => {
    const style = segmentBarStyle(
      { timerange: '[0:0_10:500000000)' },
      { min: 0, max: 100, range: 100 },
    );
    expect(style).toBe('left:0.00%;width:10.50%');
  });
});
