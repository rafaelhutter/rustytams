import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
  formatShortName, parsePagination, authenticated, authError,
  setCredentials, clearCredentials, configure, getApiBase,
  apiFetch,
} from '../lib/api.js';

beforeEach(() => {
  clearCredentials();
});

describe('formatShortName', () => {
  it('extracts last segment from URN', () => {
    expect(formatShortName('urn:x-nmos:format:video')).toBe('video');
  });

  it('extracts from audio URN', () => {
    expect(formatShortName('urn:x-nmos:format:audio')).toBe('audio');
  });

  it('returns -- for undefined', () => {
    expect(formatShortName(undefined)).toBe('--');
  });

  it('returns -- for null', () => {
    expect(formatShortName(null)).toBe('--');
  });

  it('returns the string itself if no colon', () => {
    expect(formatShortName('video')).toBe('video');
  });
});

describe('parsePagination', () => {
  it('parses all pagination headers', () => {
    const headers = new Headers({
      'x-paging-limit': '25',
      'x-paging-nextkey': 'abc123',
      'x-paging-count': '100',
      'x-paging-timerange': '[0:0_10:0)',
    });
    const result = parsePagination(headers);
    expect(result.limit).toBe(25);
    expect(result.nextKey).toBe('abc123');
    expect(result.count).toBe(100);
    expect(result.timerange).toBe('[0:0_10:0)');
  });

  it('returns nulls for missing headers', () => {
    const headers = new Headers();
    const result = parsePagination(headers);
    expect(result.limit).toBeNull();
    expect(result.nextKey).toBeNull();
    expect(result.count).toBeNull();
    expect(result.timerange).toBeNull();
  });
});

describe('authenticated store', () => {
  it('starts as false', () => {
    expect(get(authenticated)).toBe(false);
  });

  it('becomes true after setCredentials', () => {
    setCredentials('user', 'pass');
    expect(get(authenticated)).toBe(true);
  });

  it('becomes false after clearCredentials', () => {
    setCredentials('user', 'pass');
    clearCredentials();
    expect(get(authenticated)).toBe(false);
  });

  it('clears authError on setCredentials', () => {
    authError.set('some error');
    setCredentials('user', 'pass');
    expect(get(authError)).toBeNull();
  });
});

describe('configure', () => {
  it('sets API base URL', () => {
    configure({ api: 'http://example.com:9000/' });
    expect(getApiBase()).toBe('http://example.com:9000');
  });

  it('strips trailing slash', () => {
    configure({ api: 'http://localhost:5800/' });
    expect(getApiBase()).toBe('http://localhost:5800');
  });
});

describe('token handling', () => {
  let fetchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    configure({ api: 'http://localhost:5800' });
    setCredentials('test', 'password');
  });

  afterEach(() => {
    fetchSpy?.mockRestore();
    clearCredentials();
  });

  it('does not re-fetch token when expires_in is missing (defaults to 3600s)', async () => {
    let tokenFetchCount = 0;
    fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const url = typeof input === 'string' ? input : (input as Request).url;
      if (url.endsWith('/token')) {
        tokenFetchCount++;
        return new Response(JSON.stringify({ access_token: 'tok-123' }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }
      // API call
      return new Response(JSON.stringify({ id: 'test' }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    // First call: fetches token + makes API call
    await apiFetch('/service');
    expect(tokenFetchCount).toBe(1);

    // Second call: should reuse cached token (not re-fetch)
    await apiFetch('/service');
    expect(tokenFetchCount).toBe(1);
  });

  it('re-fetches token when expires_in is 0', async () => {
    let tokenFetchCount = 0;
    fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const url = typeof input === 'string' ? input : (input as Request).url;
      if (url.endsWith('/token')) {
        tokenFetchCount++;
        return new Response(JSON.stringify({ access_token: `tok-${tokenFetchCount}`, expires_in: 0 }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }
      return new Response(JSON.stringify({}), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    await apiFetch('/service');
    expect(tokenFetchCount).toBe(1);

    // Token expired immediately, should re-fetch
    await apiFetch('/service');
    expect(tokenFetchCount).toBe(2);
  });
});
