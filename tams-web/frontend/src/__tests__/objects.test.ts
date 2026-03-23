import { describe, it, expect } from 'vitest';
import { buildObjectQuery } from '../lib/query.js';

describe('Object query building', () => {
  const id = 'abc-123';

  it('builds bare path with no options', () => {
    expect(buildObjectQuery(id)).toBe('/objects/abc-123');
  });

  it('builds bare path with empty options', () => {
    expect(buildObjectQuery(id, {})).toBe('/objects/abc-123');
  });

  it('includes verbose_storage when true', () => {
    expect(buildObjectQuery(id, { verboseStorage: true })).toBe(
      '/objects/abc-123?verbose_storage=true'
    );
  });

  it('omits verbose_storage when false', () => {
    expect(buildObjectQuery(id, { verboseStorage: false })).toBe('/objects/abc-123');
  });

  it('includes accept_get_urls', () => {
    expect(buildObjectQuery(id, { acceptGetUrls: 'local-1,remote-2' })).toBe(
      '/objects/abc-123?accept_get_urls=local-1%2Cremote-2'
    );
  });

  it('includes accept_storage_ids', () => {
    expect(buildObjectQuery(id, { acceptStorageIds: 'uuid1,uuid2' })).toBe(
      '/objects/abc-123?accept_storage_ids=uuid1%2Cuuid2'
    );
  });

  it('includes presigned=true', () => {
    expect(buildObjectQuery(id, { presigned: true })).toBe(
      '/objects/abc-123?presigned=true'
    );
  });

  it('includes presigned=false', () => {
    expect(buildObjectQuery(id, { presigned: false })).toBe(
      '/objects/abc-123?presigned=false'
    );
  });

  it('omits presigned when undefined', () => {
    expect(buildObjectQuery(id, { presigned: undefined })).toBe('/objects/abc-123');
  });

  it('includes flow_tag.name=value when both provided', () => {
    expect(buildObjectQuery(id, { flowTagName: 'env', flowTagValue: 'prod' })).toBe(
      '/objects/abc-123?flow_tag.env=prod'
    );
  });

  it('uses flow_tag_exists when only name provided', () => {
    expect(buildObjectQuery(id, { flowTagName: 'env' })).toBe(
      '/objects/abc-123?flow_tag_exists.env=true'
    );
  });

  it('ignores flow tag when only value provided', () => {
    expect(buildObjectQuery(id, { flowTagValue: 'prod' })).toBe('/objects/abc-123');
  });

  it('includes limit', () => {
    expect(buildObjectQuery(id, { limit: 10 })).toBe('/objects/abc-123?limit=10');
  });

  it('includes page cursor', () => {
    expect(buildObjectQuery(id, { page: 'cursor42' })).toBe(
      '/objects/abc-123?page=cursor42'
    );
  });

  it('combines multiple options', () => {
    const result = buildObjectQuery(id, {
      verboseStorage: true,
      presigned: true,
      flowTagName: 'env',
      flowTagValue: 'prod',
      page: 'next1',
    });
    expect(result).toContain('verbose_storage=true');
    expect(result).toContain('presigned=true');
    expect(result).toContain('flow_tag.env=prod');
    expect(result).toContain('page=next1');
  });

  it('trims whitespace from flow tag inputs', () => {
    expect(buildObjectQuery(id, { flowTagName: '  env  ', flowTagValue: '  prod  ' })).toBe(
      '/objects/abc-123?flow_tag.env=prod'
    );
  });

  it('handles special characters in object ID', () => {
    const specialId = 'obj/with/slashes';
    expect(buildObjectQuery(specialId)).toBe('/objects/obj/with/slashes');
  });
});
