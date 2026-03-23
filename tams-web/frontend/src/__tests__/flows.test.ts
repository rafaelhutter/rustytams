import { describe, it, expect } from 'vitest';
import { buildFlowsQuery, buildFlowQuery } from '../lib/query.js';

describe('Flows query building', () => {
  it('builds bare /flows with no filters', () => {
    expect(buildFlowsQuery({})).toBe('/flows');
  });

  it('includes label filter', () => {
    expect(buildFlowsQuery({ label: 'Camera 1' })).toBe(
      '/flows?label=Camera+1'
    );
  });

  it('includes format filter', () => {
    expect(buildFlowsQuery({ format: 'urn:x-nmos:format:video' })).toBe(
      '/flows?format=urn%3Ax-nmos%3Aformat%3Avideo'
    );
  });

  it('includes codec filter', () => {
    expect(buildFlowsQuery({ codec: 'video/h264' })).toBe(
      '/flows?codec=video%2Fh264'
    );
  });

  it('includes source_id filter', () => {
    expect(buildFlowsQuery({ sourceId: 'abc-123' })).toBe(
      '/flows?source_id=abc-123'
    );
  });

  it('includes frame_width and frame_height filters', () => {
    const result = buildFlowsQuery({ frameWidth: '1920', frameHeight: '1080' });
    expect(result).toContain('frame_width=1920');
    expect(result).toContain('frame_height=1080');
  });

  it('includes tag.name=value when both provided', () => {
    expect(buildFlowsQuery({ tagName: 'location', tagValue: 'studio-a' })).toContain(
      'tag.location=studio-a'
    );
  });

  it('uses tag_exists when tag name provided without value', () => {
    expect(buildFlowsQuery({ tagName: 'location' })).toContain(
      'tag_exists.location=true'
    );
  });

  it('includes page key for pagination', () => {
    expect(buildFlowsQuery({}, 'abc123')).toBe(
      '/flows?page=abc123'
    );
  });

  it('combines multiple filters', () => {
    const result = buildFlowsQuery({
      label: 'Cam',
      format: 'urn:x-nmos:format:video',
      codec: 'video/h264',
      sourceId: 'src-1',
    });
    expect(result).toContain('label=Cam');
    expect(result).toContain('format=urn%3Ax-nmos%3Aformat%3Avideo');
    expect(result).toContain('codec=video%2Fh264');
    expect(result).toContain('source_id=src-1');
  });

  it('trims whitespace from inputs', () => {
    expect(buildFlowsQuery({ label: '  Camera 1  ' })).toContain('label=Camera+1');
    expect(buildFlowsQuery({ codec: '  video/h264  ' })).toContain('codec=video%2Fh264');
  });

  it('includes timerange filter', () => {
    expect(buildFlowsQuery({ timerange: '[0:0_10:0)' })).toContain(
      'timerange=%5B0%3A0_10%3A0%29'
    );
  });

  it('includes limit parameter', () => {
    expect(buildFlowsQuery({ limit: 30 })).toBe('/flows?limit=30');
  });

  it('includes include_timerange parameter', () => {
    expect(buildFlowsQuery({ includeTimerange: true })).toBe('/flows?include_timerange=true');
  });

  it('combines limit and includeTimerange', () => {
    const result = buildFlowsQuery({ limit: 30, includeTimerange: true });
    expect(result).toContain('limit=30');
    expect(result).toContain('include_timerange=true');
  });

  it('combines limit with other filters', () => {
    const result = buildFlowsQuery({ sourceId: 'src-1', limit: 20 });
    expect(result).toContain('source_id=src-1');
    expect(result).toContain('limit=20');
  });
});

describe('Single flow query building', () => {
  it('builds bare /flows/{id} with no options', () => {
    expect(buildFlowQuery('abc-123')).toBe('/flows/abc-123');
  });

  it('includes include_timerange when requested', () => {
    expect(buildFlowQuery('abc-123', { includeTimerange: true })).toBe(
      '/flows/abc-123?include_timerange=true'
    );
  });

  it('omits include_timerange when false or absent', () => {
    expect(buildFlowQuery('abc-123', { includeTimerange: false })).toBe('/flows/abc-123');
    expect(buildFlowQuery('abc-123', {})).toBe('/flows/abc-123');
  });

  it('includes timerange for clipping', () => {
    expect(buildFlowQuery('abc-123', { includeTimerange: true, timerange: '[0:0_10:0)' })).toBe(
      '/flows/abc-123?include_timerange=true&timerange=%5B0%3A0_10%3A0%29'
    );
  });

  it('includes timerange without includeTimerange', () => {
    expect(buildFlowQuery('abc-123', { timerange: '[5:0_15:0)' })).toBe(
      '/flows/abc-123?timerange=%5B5%3A0_15%3A0%29'
    );
  });
});
