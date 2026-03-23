import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  clearThumbnailCache,
  hasThumbnail,
  getCachedThumbnail,
} from '../lib/thumbnail.js';

// These tests cover the cache logic. Actual frame extraction requires
// browser APIs (OffscreenCanvas, VideoFrame, mediabunny) and is tested
// manually.

describe('thumbnail cache', () => {
  beforeEach(() => {
    clearThumbnailCache();
  });

  it('hasThumbnail returns false for unknown key', () => {
    expect(hasThumbnail('unknown')).toBe(false);
  });

  it('getCachedThumbnail returns null for unknown key', () => {
    expect(getCachedThumbnail('unknown')).toBeNull();
  });

  it('clearThumbnailCache empties the cache', () => {
    // We can't easily populate the cache without extractThumbnail
    // (which needs browser APIs), but we can verify clear doesn't throw
    clearThumbnailCache();
    expect(hasThumbnail('anything')).toBe(false);
  });
});

describe('extractThumbnail', () => {
  it('returns null for empty segments array', async () => {
    const { extractThumbnail } = await import('../lib/thumbnail.js');
    const result = await extractThumbnail({ key: 'test', segments: [] });
    expect(result).toBeNull();
  });

  it('returns null for null segments', async () => {
    const { extractThumbnail } = await import('../lib/thumbnail.js');
    const result = await extractThumbnail({ key: 'test', segments: null as unknown as [] });
    expect(result).toBeNull();
  });

  it('returns null when aborted before start', async () => {
    const { extractThumbnail } = await import('../lib/thumbnail.js');
    const controller = new AbortController();
    controller.abort();
    const result = await extractThumbnail({
      key: 'test',
      segments: [{ get_urls: [{ url: 'http://example.com/seg.ts', presigned: true }] }],
      signal: controller.signal,
    });
    expect(result).toBeNull();
  });

  it('returns null for segments without presigned URLs', async () => {
    const { extractThumbnail } = await import('../lib/thumbnail.js');
    const result = await extractThumbnail({
      key: 'test',
      segments: [{ get_urls: [] }],
    });
    expect(result).toBeNull();
  });
});
