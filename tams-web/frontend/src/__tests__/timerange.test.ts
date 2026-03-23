import { describe, it, expect } from 'vitest';
import {
  parseTimestamp, formatTimestampStr, parseTimerange, formatTimerangeStr,
  nanosToSeconds, formatDuration, isEternity, isNever, formatTimerangeDisplay,
} from '../lib/timerange.js';

// -- parseTimestamp (returns BigInt nanoseconds or null) --

describe('parseTimestamp', () => {
  it('parses zero', () => {
    expect(parseTimestamp('0:0')).toBe(0n);
  });

  it('parses whole seconds', () => {
    expect(parseTimestamp('10:0')).toBe(10_000_000_000n);
  });

  it('parses seconds:nanoseconds', () => {
    expect(parseTimestamp('8:399999999')).toBe(8_399_999_999n);
  });

  it('parses sub-second only', () => {
    expect(parseTimestamp('0:500000000')).toBe(500_000_000n);
  });

  it('parses large TAI timestamp', () => {
    expect(parseTimestamp('1694429247:40000000')).toBe(1_694_429_247_040_000_000n);
  });

  it('parses negative timestamp', () => {
    // -1:500000000 means -1 second + 500000000 nanos = -500000000 nanos total
    expect(parseTimestamp('-1:500000000')).toBe(-500_000_000n);
  });

  it('parses negative large', () => {
    expect(parseTimestamp('-100:0')).toBe(-100_000_000_000n);
  });

  it('returns null for empty', () => {
    expect(parseTimestamp('')).toBeNull();
  });

  it('returns null for null', () => {
    expect(parseTimestamp(null as unknown as string)).toBeNull();
  });

  it('returns null for missing colon', () => {
    expect(parseTimestamp('12345')).toBeNull();
  });

  it('returns null for malformed', () => {
    expect(parseTimestamp('abc:0')).toBeNull();
    expect(parseTimestamp('0:abc')).toBeNull();
  });

  it('rejects leading zeros in seconds', () => {
    expect(parseTimestamp('01:0')).toBeNull();
  });

  it('rejects leading zeros in nanoseconds', () => {
    expect(parseTimestamp('0:01')).toBeNull();
  });

  it('rejects nanoseconds > 999999999', () => {
    expect(parseTimestamp('0:1000000000')).toBeNull();
  });

  it('rejects empty parts', () => {
    expect(parseTimestamp(':0')).toBeNull();
    expect(parseTimestamp('0:')).toBeNull();
  });
});

// -- formatTimestampStr (BigInt nanos -> string) --

describe('formatTimestampStr', () => {
  it('formats zero', () => {
    expect(formatTimestampStr(0n)).toBe('0:0');
  });

  it('formats positive', () => {
    expect(formatTimestampStr(8_399_999_999n)).toBe('8:399999999');
  });

  it('formats negative (euclidean division)', () => {
    expect(formatTimestampStr(-500_000_000n)).toBe('-1:500000000');
  });

  it('formats negative exact seconds', () => {
    expect(formatTimestampStr(-100_000_000_000n)).toBe('-100:0');
  });

  it('returns -- for non-bigint', () => {
    expect(formatTimestampStr(42 as unknown as bigint)).toBe('--');
    expect(formatTimestampStr(null as unknown as bigint)).toBe('--');
  });
});

// -- Round-trip: parseTimestamp -> formatTimestampStr --

describe('timestamp round-trip', () => {
  const cases = ['0:0', '8:399999999', '-1:500000000', '1694429247:40000000', '-100:0'];
  for (const ts of cases) {
    it(`round-trips "${ts}"`, () => {
      expect(formatTimestampStr(parseTimestamp(ts)!)).toBe(ts);
    });
  }
});

// -- parseTimerange --

describe('parseTimerange', () => {
  it('parses never "()"', () => {
    const tr = parseTimerange('()');
    expect(tr.type).toBe('never');
  });

  it('parses eternity "_"', () => {
    const tr = parseTimerange('_');
    expect(tr.type).toBe('range');
    expect(tr.start).toBeNull();
    expect(tr.end).toBeNull();
  });

  it('parses standard range [0:0_10:0)', () => {
    const tr = parseTimerange('[0:0_10:0)');
    expect(tr.type).toBe('range');
    expect(tr.start!.nanos).toBe(0n);
    expect(tr.start!.inclusive).toBe(true);
    expect(tr.end!.nanos).toBe(10_000_000_000n);
    expect(tr.end!.inclusive).toBe(false);
  });

  it('parses inclusive-inclusive range [0:0_10:0]', () => {
    const tr = parseTimerange('[0:0_10:0]');
    expect(tr.start!.inclusive).toBe(true);
    expect(tr.end!.inclusive).toBe(true);
  });

  it('parses exclusive-exclusive range (0:0_10:0)', () => {
    const tr = parseTimerange('(0:0_10:0)');
    expect(tr.start!.inclusive).toBe(false);
    expect(tr.end!.inclusive).toBe(false);
  });

  it('parses instantaneous [10:0]', () => {
    const tr = parseTimerange('[10:0]');
    expect(tr.type).toBe('range');
    expect(tr.start!.nanos).toBe(10_000_000_000n);
    expect(tr.end!.nanos).toBe(10_000_000_000n);
    expect(tr.start!.inclusive).toBe(true);
    expect(tr.end!.inclusive).toBe(true);
  });

  it('parses instantaneous with separator [10:0_10:0]', () => {
    const tr = parseTimerange('[10:0_10:0]');
    expect(tr.type).toBe('range');
    expect(tr.start!.nanos).toBe(tr.end!.nanos);
    expect(tr.start!.inclusive).toBe(true);
    expect(tr.end!.inclusive).toBe(true);
  });

  it('parses open-ended start _10:0)', () => {
    const tr = parseTimerange('_10:0)');
    expect(tr.type).toBe('range');
    expect(tr.start).toBeNull();
    expect(tr.end!.nanos).toBe(10_000_000_000n);
    expect(tr.end!.inclusive).toBe(false);
  });

  it('parses open-ended start _20:0]', () => {
    const tr = parseTimerange('_20:0]');
    expect(tr.type).toBe('range');
    expect(tr.start).toBeNull();
    expect(tr.end!.nanos).toBe(20_000_000_000n);
    expect(tr.end!.inclusive).toBe(true);
  });

  it('parses open-ended end (5:0_', () => {
    const tr = parseTimerange('(5:0_');
    expect(tr.type).toBe('range');
    expect(tr.start!.nanos).toBe(5_000_000_000n);
    expect(tr.start!.inclusive).toBe(false);
    expect(tr.end).toBeNull();
  });

  it('parses open-ended end [5:0_', () => {
    const tr = parseTimerange('[5:0_');
    expect(tr.type).toBe('range');
    expect(tr.start!.inclusive).toBe(true);
    expect(tr.end).toBeNull();
  });

  it('parses negative timestamp', () => {
    const tr = parseTimerange('[-1:500000000_10:0)');
    expect(tr.type).toBe('range');
    expect(tr.start!.nanos).toBe(-500_000_000n);
  });

  // -- Degenerate cases -> never --

  it('exclusive instant (10:0) is never', () => {
    expect(parseTimerange('(10:0)').type).toBe('never');
  });

  it('half-exclusive instant [10:0) is never', () => {
    expect(parseTimerange('[10:0)').type).toBe('never');
  });

  it('half-exclusive instant (10:0] is never', () => {
    expect(parseTimerange('(10:0]').type).toBe('never');
  });

  it('end before start [10:0_5:0] is never', () => {
    expect(parseTimerange('[10:0_5:0]').type).toBe('never');
  });

  it('equal with exclusive end [10:0_10:0) is never', () => {
    expect(parseTimerange('[10:0_10:0)').type).toBe('never');
  });

  it('null input is never', () => {
    expect(parseTimerange(null as unknown as string).type).toBe('never');
  });

  it('empty string is never', () => {
    expect(parseTimerange('').type).toBe('never');
  });
});

// -- formatTimerangeStr (round-trip) --

describe('formatTimerangeStr', () => {
  it('formats never', () => {
    expect(formatTimerangeStr(parseTimerange('()'))).toBe('()');
  });

  it('formats eternity', () => {
    expect(formatTimerangeStr(parseTimerange('_'))).toBe('_');
  });

  it('formats standard range', () => {
    expect(formatTimerangeStr(parseTimerange('[0:0_10:0)'))).toBe('[0:0_10:0)');
  });

  it('formats instantaneous', () => {
    expect(formatTimerangeStr(parseTimerange('[10:0]'))).toBe('[10:0]');
  });

  it('formats unbounded start', () => {
    expect(formatTimerangeStr(parseTimerange('_20:0]'))).toBe('_20:0]');
  });

  it('formats unbounded end', () => {
    expect(formatTimerangeStr(parseTimerange('(5:0_'))).toBe('(5:0_');
  });
});

// -- Timerange round-trip --

describe('timerange round-trip', () => {
  const cases = ['_', '()', '[0:0_10:0)', '(5:0_', '_20:0]', '[10:0]', '[100:0_200:0]', '(0:0_1:0)'];
  for (const c of cases) {
    it(`round-trips "${c}"`, () => {
      expect(formatTimerangeStr(parseTimerange(c))).toBe(c);
    });
  }
});

// -- isEternity / isNever helpers --

describe('isEternity / isNever', () => {
  it('eternity', () => {
    expect(isEternity(parseTimerange('_'))).toBe(true);
    expect(isNever(parseTimerange('_'))).toBe(false);
  });

  it('never', () => {
    expect(isNever(parseTimerange('()'))).toBe(true);
    expect(isEternity(parseTimerange('()'))).toBe(false);
  });

  it('standard range is neither', () => {
    const tr = parseTimerange('[0:0_10:0)');
    expect(isEternity(tr)).toBe(false);
    expect(isNever(tr)).toBe(false);
  });
});

// -- nanosToSeconds (for display only) --

describe('nanosToSeconds', () => {
  it('converts zero', () => {
    expect(nanosToSeconds(0n)).toBe(0);
  });

  it('converts to float seconds', () => {
    expect(nanosToSeconds(8_400_000_000n)).toBeCloseTo(8.4, 5);
  });

  it('returns NaN for non-bigint', () => {
    expect(nanosToSeconds(42 as unknown as bigint)).toBeNaN();
  });
});

// -- formatDuration --

describe('formatDuration', () => {
  it('formats zero', () => {
    expect(formatDuration(0)).toBe('00:00:00.000');
  });

  it('formats seconds with milliseconds', () => {
    expect(formatDuration(8.4)).toBe('00:00:08.400');
  });

  it('formats hours', () => {
    expect(formatDuration(3661.5)).toBe('01:01:01.500');
  });

  it('handles NaN', () => {
    expect(formatDuration(NaN)).toBe('--:--:--');
  });

  it('handles negative', () => {
    expect(formatDuration(-1)).toBe('--:--:--');
  });
});

// -- formatTimerangeDisplay --

describe('formatTimerangeDisplay', () => {
  it('returns -- for undefined', () => {
    const result = formatTimerangeDisplay(undefined);
    expect(result.raw).toBe('--');
    expect(result.display).toBe('--');
  });

  it('shows raw wire format for a standard range', () => {
    const result = formatTimerangeDisplay('[0:0_10:0)');
    expect(result.raw).toBe('[0:0_10:0)');
    expect(result.display).toContain('00:00:00.000');
    expect(result.display).toContain('00:00:10.000');
  });

  it('shows "never" display for "()"', () => {
    const result = formatTimerangeDisplay('()');
    expect(result.raw).toBe('()');
    expect(result.display).toBe('never');
  });

  it('shows "eternity" display for "_"', () => {
    const result = formatTimerangeDisplay('_');
    expect(result.raw).toBe('_');
    expect(result.display).toBe('eternity');
  });

  it('includes duration in display', () => {
    const result = formatTimerangeDisplay('[0:0_10:0)');
    expect(result.display).toContain('(00:00:10.000)');
  });

  it('shows ... for open-ended start', () => {
    const result = formatTimerangeDisplay('_10:0)');
    expect(result.display).toContain('...');
    expect(result.display).toContain('00:00:10.000');
  });

  it('shows ... for open-ended end', () => {
    const result = formatTimerangeDisplay('[5:0_');
    expect(result.display).toContain('00:00:05.000');
    expect(result.display).toContain('...');
  });
});
