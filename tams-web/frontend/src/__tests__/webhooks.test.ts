import { describe, it, expect } from 'vitest';
import { buildWebhooksQuery, WEBHOOK_EVENTS } from '../lib/query.js';

describe('WEBHOOK_EVENTS', () => {
  it('contains exactly 8 event types', () => {
    expect(WEBHOOK_EVENTS).toHaveLength(8);
  });

  it('includes all flow events', () => {
    expect(WEBHOOK_EVENTS).toContain('flows/created');
    expect(WEBHOOK_EVENTS).toContain('flows/updated');
    expect(WEBHOOK_EVENTS).toContain('flows/deleted');
    expect(WEBHOOK_EVENTS).toContain('flows/segments_added');
    expect(WEBHOOK_EVENTS).toContain('flows/segments_deleted');
  });

  it('includes all source events', () => {
    expect(WEBHOOK_EVENTS).toContain('sources/created');
    expect(WEBHOOK_EVENTS).toContain('sources/updated');
    expect(WEBHOOK_EVENTS).toContain('sources/deleted');
  });
});

describe('Webhooks query building', () => {
  it('builds bare /service/webhooks with no filters', () => {
    expect(buildWebhooksQuery({})).toBe('/service/webhooks');
  });

  it('builds bare path with empty defaults', () => {
    expect(buildWebhooksQuery()).toBe('/service/webhooks');
  });

  it('includes tag.name=value when both provided', () => {
    expect(buildWebhooksQuery({ tagName: 'env', tagValue: 'prod' })).toBe(
      '/service/webhooks?tag.env=prod'
    );
  });

  it('uses tag_exists when tag name provided without value', () => {
    expect(buildWebhooksQuery({ tagName: 'env' })).toBe(
      '/service/webhooks?tag_exists.env=true'
    );
  });

  it('ignores tag when only value is provided', () => {
    expect(buildWebhooksQuery({ tagValue: 'prod' })).toBe('/service/webhooks');
  });

  it('includes page key for pagination', () => {
    expect(buildWebhooksQuery({}, 'cursor123')).toBe(
      '/service/webhooks?page=cursor123'
    );
  });

  it('combines tag filter and pagination', () => {
    const result = buildWebhooksQuery({ tagName: 'env', tagValue: 'prod' }, 'next1');
    expect(result).toContain('tag.env=prod');
    expect(result).toContain('page=next1');
  });

  it('trims whitespace from tag inputs', () => {
    expect(buildWebhooksQuery({ tagName: '  env  ', tagValue: '  prod  ' })).toBe(
      '/service/webhooks?tag.env=prod'
    );
  });

  it('includes limit parameter', () => {
    expect(buildWebhooksQuery({ limit: 1 })).toBe('/service/webhooks?limit=1');
  });
});
