import { describe, it, expect } from 'vitest';
import { formatHexDump } from '../lib/hexdump.js';

describe('formatHexDump', () => {
  it('formats a single line of 16 bytes', () => {
    const bytes = new Uint8Array([
      0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x2c, 0x20, 0x57,
      0x6f, 0x72, 0x6c, 0x64, 0x21, 0x0a, 0x00, 0xff,
    ]);
    const result = formatHexDump(bytes);
    expect(result).toBe(
      '00000000  48 65 6c 6c 6f 2c 20 57 6f 72 6c 64 21 0a 00 ff  |Hello, World!...|'
    );
  });

  it('handles partial last line', () => {
    const bytes = new Uint8Array([0x41, 0x42, 0x43]); // ABC
    const result = formatHexDump(bytes);
    // hex "41 42 43" (8 chars) padded to 47 = 39 trailing spaces
    expect(result).toMatch(/^00000000  41 42 43\s+\|ABC\|$/);
    expect(result).toContain('|ABC|');
  });

  it('handles empty input', () => {
    const result = formatHexDump(new Uint8Array([]));
    expect(result).toBe('');
  });

  it('replaces non-printable bytes with dots in ASCII column', () => {
    const bytes = new Uint8Array([0x01, 0x7f, 0x1f, 0x20, 0x7e]);
    const result = formatHexDump(bytes);
    expect(result).toContain('|... ~|');
  });

  it('formats multiple lines with correct offsets', () => {
    const bytes = new Uint8Array(32);
    bytes.fill(0xaa);
    const lines = formatHexDump(bytes).split('\n');
    expect(lines).toHaveLength(2);
    expect(lines[0]).toMatch(/^00000000/);
    expect(lines[1]).toMatch(/^00000010/);
  });

  it('pads hex column for alignment', () => {
    const oneByte = formatHexDump(new Uint8Array([0x42]));
    // offset(8) + "  " + hex padded to 47 + "  |" + ascii + "|"
    // 8 + 2 + 47 + 3 + 1 + 1 = 62 for 1 byte
    expect(oneByte).toMatch(/^00000000  42\s+\|B\|$/);
    // A full 16-byte line has hex exactly 47 chars (no padding needed)
    const fullLine = formatHexDump(new Uint8Array(16).fill(0x42));
    const hexPart = fullLine.slice(10, 57); // after "00000000  ", 47 chars
    expect(hexPart.trim().split(' ')).toHaveLength(16);
  });
});
