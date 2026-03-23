import { describe, it, expect } from 'vitest';
import { buildSegmentsQuery } from '../lib/query.js';

const FLOW_ID = 'f47ac10b-58cc-4372-a567-0e02b2c3d479';

describe('Segments query building', () => {
  it('builds bare path with no options', () => {
    expect(buildSegmentsQuery(FLOW_ID)).toBe(
      `/flows/${FLOW_ID}/segments`
    );
  });

  it('builds bare path with empty options object', () => {
    expect(buildSegmentsQuery(FLOW_ID, {})).toBe(
      `/flows/${FLOW_ID}/segments`
    );
  });

  it('includes timerange filter', () => {
    expect(buildSegmentsQuery(FLOW_ID, { timerange: '0:0_10:0' })).toBe(
      `/flows/${FLOW_ID}/segments?timerange=0%3A0_10%3A0`
    );
  });

  it('includes object_id filter', () => {
    expect(buildSegmentsQuery(FLOW_ID, { objectId: 'obj-abc-123' })).toBe(
      `/flows/${FLOW_ID}/segments?object_id=obj-abc-123`
    );
  });

  it('includes reverse_order when true', () => {
    expect(buildSegmentsQuery(FLOW_ID, { reverseOrder: true })).toBe(
      `/flows/${FLOW_ID}/segments?reverse_order=true`
    );
  });

  it('omits reverse_order when false', () => {
    expect(buildSegmentsQuery(FLOW_ID, { reverseOrder: false })).toBe(
      `/flows/${FLOW_ID}/segments`
    );
  });

  it('includes presigned=true', () => {
    expect(buildSegmentsQuery(FLOW_ID, { presigned: true })).toBe(
      `/flows/${FLOW_ID}/segments?presigned=true`
    );
  });

  it('includes presigned=false (means only non-presigned)', () => {
    expect(buildSegmentsQuery(FLOW_ID, { presigned: false })).toBe(
      `/flows/${FLOW_ID}/segments?presigned=false`
    );
  });

  it('omits presigned when undefined', () => {
    expect(buildSegmentsQuery(FLOW_ID, { presigned: undefined })).toBe(
      `/flows/${FLOW_ID}/segments`
    );
  });

  it('includes accept_get_urls', () => {
    expect(buildSegmentsQuery(FLOW_ID, { acceptGetUrls: 'media,thumb' })).toBe(
      `/flows/${FLOW_ID}/segments?accept_get_urls=media%2Cthumb`
    );
  });

  it('includes accept_storage_ids', () => {
    expect(buildSegmentsQuery(FLOW_ID, { acceptStorageIds: 's3-eu,s3-us' })).toBe(
      `/flows/${FLOW_ID}/segments?accept_storage_ids=s3-eu%2Cs3-us`
    );
  });

  it('includes verbose_storage when true', () => {
    expect(buildSegmentsQuery(FLOW_ID, { verboseStorage: true })).toBe(
      `/flows/${FLOW_ID}/segments?verbose_storage=true`
    );
  });

  it('omits verbose_storage when false', () => {
    expect(buildSegmentsQuery(FLOW_ID, { verboseStorage: false })).toBe(
      `/flows/${FLOW_ID}/segments`
    );
  });

  it('includes include_object_timerange when true', () => {
    expect(buildSegmentsQuery(FLOW_ID, { includeObjectTimerange: true })).toBe(
      `/flows/${FLOW_ID}/segments?include_object_timerange=true`
    );
  });

  it('omits include_object_timerange when false', () => {
    expect(buildSegmentsQuery(FLOW_ID, { includeObjectTimerange: false })).toBe(
      `/flows/${FLOW_ID}/segments`
    );
  });

  it('includes limit', () => {
    expect(buildSegmentsQuery(FLOW_ID, { limit: 25 })).toBe(
      `/flows/${FLOW_ID}/segments?limit=25`
    );
  });

  it('includes limit of 0', () => {
    expect(buildSegmentsQuery(FLOW_ID, { limit: 0 })).toBe(
      `/flows/${FLOW_ID}/segments?limit=0`
    );
  });

  it('includes page cursor', () => {
    expect(buildSegmentsQuery(FLOW_ID, { page: 'cursor_abc123' })).toBe(
      `/flows/${FLOW_ID}/segments?page=cursor_abc123`
    );
  });

  it('combines multiple filters', () => {
    const result = buildSegmentsQuery(FLOW_ID, {
      timerange: '0:0_10:0',
      objectId: 'obj-1',
      reverseOrder: true,
      presigned: true,
      limit: 50,
    });
    expect(result).toContain(`/flows/${FLOW_ID}/segments?`);
    expect(result).toContain('timerange=0%3A0_10%3A0');
    expect(result).toContain('object_id=obj-1');
    expect(result).toContain('reverse_order=true');
    expect(result).toContain('presigned=true');
    expect(result).toContain('limit=50');
  });

  it('combines pagination with filters', () => {
    const result = buildSegmentsQuery(FLOW_ID, {
      timerange: '0:0_10:0',
      limit: 10,
      page: 'next_page_token',
    });
    expect(result).toContain('timerange=0%3A0_10%3A0');
    expect(result).toContain('limit=10');
    expect(result).toContain('page=next_page_token');
  });

  it('combines presigned=false with other filters', () => {
    const result = buildSegmentsQuery(FLOW_ID, {
      presigned: false,
      verboseStorage: true,
    });
    expect(result).toContain('presigned=false');
    expect(result).toContain('verbose_storage=true');
  });
});
