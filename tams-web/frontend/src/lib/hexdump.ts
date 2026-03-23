/**
 * Format a Uint8Array as a hex dump string.
 * Output follows the canonical hex dump format: offset  hex bytes  |ascii|
 */
export function formatHexDump(bytes: Uint8Array): string {
  const lines: string[] = [];
  for (let offset = 0; offset < bytes.length; offset += 16) {
    const chunk = bytes.slice(offset, offset + 16);
    const hex = Array.from(chunk, b => b.toString(16).padStart(2, '0')).join(' ');
    const ascii = Array.from(chunk, b => (b >= 0x20 && b <= 0x7e) ? String.fromCharCode(b) : '.').join('');
    const offsetStr = offset.toString(16).padStart(8, '0');
    lines.push(`${offsetStr}  ${hex.padEnd(47)}  |${ascii}|`);
  }
  return lines.join('\n');
}
