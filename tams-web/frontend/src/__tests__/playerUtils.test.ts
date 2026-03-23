import { describe, it, expect } from 'vitest';
import { detectOverlaps, validateExport, findOverlappingSegments, formatSeconds } from '../lib/playerUtils.js';

interface TestSegment {
  timerange: string;
  object_id: string;
}

describe('detectOverlaps', () => {
  it('returns empty set for empty array', () => {
    expect(detectOverlaps([])).toEqual(new Set());
  });

  it('returns empty set for single marker', () => {
    expect(detectOverlaps([{ id: 'a', start: 0, end: 5 }])).toEqual(new Set());
  });

  it('returns empty set for non-overlapping markers', () => {
    const markers = [
      { id: 'a', start: 0, end: 5 },
      { id: 'b', start: 5, end: 10 },
      { id: 'c', start: 15, end: 20 },
    ];
    expect(detectOverlaps(markers)).toEqual(new Set());
  });

  it('detects two overlapping markers', () => {
    const markers = [
      { id: 'a', start: 0, end: 6 },
      { id: 'b', start: 4, end: 10 },
    ];
    expect(detectOverlaps(markers)).toEqual(new Set(['a', 'b']));
  });

  it('detects one marker fully inside another', () => {
    const markers = [
      { id: 'outer', start: 0, end: 20 },
      { id: 'inner', start: 5, end: 10 },
    ];
    expect(detectOverlaps(markers)).toEqual(new Set(['outer', 'inner']));
  });

  it('does not count touching boundaries as overlap', () => {
    // [0-5) and [5-10) should NOT overlap
    const markers = [
      { id: 'a', start: 0, end: 5 },
      { id: 'b', start: 5, end: 10 },
    ];
    expect(detectOverlaps(markers)).toEqual(new Set());
  });

  it('detects cascading overlaps across three markers', () => {
    // a overlaps b, b overlaps c, but a does NOT overlap c
    const markers = [
      { id: 'a', start: 0, end: 6 },
      { id: 'b', start: 4, end: 12 },
      { id: 'c', start: 10, end: 15 },
    ];
    const result = detectOverlaps(markers);
    expect(result.has('a')).toBe(true);
    expect(result.has('b')).toBe(true);
    expect(result.has('c')).toBe(true);
  });

  it('only flags overlapping subset', () => {
    const markers = [
      { id: 'a', start: 0, end: 5 },
      { id: 'b', start: 10, end: 16 },
      { id: 'c', start: 14, end: 20 },
      { id: 'd', start: 30, end: 35 },
    ];
    const result = detectOverlaps(markers);
    expect(result).toEqual(new Set(['b', 'c']));
  });
});

describe('validateExport', () => {
  // Helper: create a segment with timerange
  function seg(tr: string): TestSegment {
    return { timerange: tr, object_id: 'test-obj' };
  }

  it('returns empty array for valid markers with covering segments', () => {
    const markers = [{ label: 'M1', start: 2, end: 8 }];
    const segments = [seg('[0:0_10:0)')];
    expect(validateExport(markers, 0, segments)).toEqual([]);
  });

  it('reports zero-duration marker', () => {
    const markers = [{ label: 'M1', start: 5, end: 5 }];
    const segments = [seg('[0:0_10:0)')];
    const errors = validateExport(markers, 0, segments);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain('zero or negative duration');
  });

  it('reports negative-duration marker', () => {
    const markers = [{ label: 'M1', start: 8, end: 3 }];
    const segments = [seg('[0:0_10:0)')];
    const errors = validateExport(markers, 0, segments);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain('zero or negative duration');
  });

  it('reports marker with no covering segments', () => {
    const markers = [{ label: 'M1', start: 20, end: 25 }];
    const segments = [seg('[0:0_10:0)')];
    const errors = validateExport(markers, 0, segments);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain('no segments in range');
  });

  it('handles startOffset correctly', () => {
    // Segment covers TAMS time [100:0 - 110:0)
    // Marker at video time [2 - 8] with offset=100 -> TAMS [102 - 108] -> overlaps
    const markers = [{ label: 'M1', start: 2, end: 8 }];
    const segments = [seg('[100:0_110:0)')];
    expect(validateExport(markers, 100, segments)).toEqual([]);
  });

  it('detects no coverage with offset mismatch', () => {
    // Segment at TAMS [0 - 10), marker at video [2 - 8] with offset=100 -> TAMS [102 - 108] -> no overlap
    const markers = [{ label: 'M1', start: 2, end: 8 }];
    const segments = [seg('[0:0_10:0)')];
    const errors = validateExport(markers, 100, segments);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain('no segments in range');
  });

  it('reports multiple errors from multiple markers', () => {
    const markers = [
      { label: 'M1', start: 5, end: 5 },  // zero duration
      { label: 'M2', start: 50, end: 55 }, // no segments
    ];
    const segments = [seg('[0:0_10:0)')];
    const errors = validateExport(markers, 0, segments);
    expect(errors).toHaveLength(2);
  });

  it('handles empty segments array', () => {
    const markers = [{ label: 'M1', start: 0, end: 5 }];
    expect(validateExport(markers, 0, [])).toEqual(['M1: no segments in range']);
  });

  it('skips unparseable segments', () => {
    const markers = [{ label: 'M1', start: 0, end: 5 }];
    const segments = [seg('()'), seg('[0:0_10:0)')]; // first is "never"
    expect(validateExport(markers, 0, segments)).toEqual([]);
  });
});

describe('findOverlappingSegments', () => {
  function seg(tr: string): TestSegment {
    return { timerange: tr, object_id: 'obj-1' };
  }

  it('returns matching segments with zero offset', () => {
    const segments = [seg('[0:0_6:0)'), seg('[6:0_12:0)'), seg('[12:0_18:0)')];
    const result = findOverlappingSegments(segments, 4, 8, 0);
    expect(result).toHaveLength(2);
    expect(result[0].timerange).toBe('[0:0_6:0)');
    expect(result[1].timerange).toBe('[6:0_12:0)');
  });

  it('applies startOffset to convert video time to TAMS time', () => {
    const segments = [seg('[100:0_106:0)'), seg('[106:0_112:0)')];
    // Video time 2-8 with offset 100 -> TAMS time 102-108
    const result = findOverlappingSegments(segments, 2, 8, 100);
    expect(result).toHaveLength(2);
  });

  it('returns empty for no overlap', () => {
    const segments = [seg('[0:0_6:0)')];
    const result = findOverlappingSegments(segments, 10, 15, 0);
    expect(result).toHaveLength(0);
  });

  it('excludes exact boundary touch (non-overlapping)', () => {
    // Segment ends at exactly 6:0, query starts at exactly 6:0
    const segments = [seg('[0:0_6:0)')];
    const result = findOverlappingSegments(segments, 6, 10, 0);
    expect(result).toHaveLength(0);
  });

  it('skips never/unbounded segments', () => {
    const segments = [seg('()'), seg('_'), seg('[0:0_6:0)')];
    const result = findOverlappingSegments(segments, 0, 5, 0);
    // '()' is never, '_' is eternity (unbounded -- no start/end), only [0:0_6:0) matches
    expect(result).toHaveLength(1);
  });

  it('handles fractional seconds correctly', () => {
    const segments = [seg('[0:0_6:500000000)')]; // 0 to 6.5s
    const result = findOverlappingSegments(segments, 6.0, 7.0, 0);
    expect(result).toHaveLength(1); // segment ends at 6.5, overlaps with 6.0-7.0
  });

  it('no overlap when query ends before segment starts', () => {
    const segments = [seg('[10:0_16:0)')];
    const result = findOverlappingSegments(segments, 0, 5, 0);
    expect(result).toHaveLength(0);
  });
});

describe('formatSeconds', () => {
  it('formats zero', () => {
    expect(formatSeconds(0)).toBe('0:00.00');
  });

  it('formats whole minutes', () => {
    expect(formatSeconds(300)).toBe('5:00.00');
  });

  it('formats mixed seconds and fractions', () => {
    expect(formatSeconds(154.57)).toBe('2:34.57');
  });

  it('formats sub-second values', () => {
    expect(formatSeconds(0.25)).toBe('0:00.25');
  });

  it('formats large values (hours)', () => {
    expect(formatSeconds(7234.57)).toBe('120:34.57');
  });

  it('returns -- for NaN', () => {
    expect(formatSeconds(NaN)).toBe('--');
  });

  it('returns -- for Infinity', () => {
    expect(formatSeconds(Infinity)).toBe('--');
  });

  it('formats exactly 60 seconds as 1:00.00', () => {
    expect(formatSeconds(60)).toBe('1:00.00');
  });

  it('pads single-digit seconds', () => {
    expect(formatSeconds(63.5)).toBe('1:03.50');
  });
});
