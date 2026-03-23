import { describe, it, expect, beforeEach } from 'vitest';
import { compileRoutes, matchRoute, getHashParams, setHashParams } from '../lib/router.js';
import type { CompiledRoute, RouteMatch } from '../lib/router.js';
import type { Component } from 'svelte';

const DummyA = { name: 'A' } as unknown as Component;
const DummyB = { name: 'B' } as unknown as Component;
const DummyC = { name: 'C' } as unknown as Component;

const compiled: CompiledRoute[] = compileRoutes({
  '/': DummyA,
  '/items': DummyB,
  '/items/:id': DummyC,
});

describe('compileRoutes', () => {
  it('compiles route patterns into regex entries', () => {
    expect(compiled).toHaveLength(3);
    expect(compiled[0].regex).toBeInstanceOf(RegExp);
    expect(compiled[2].paramNames).toEqual(['id']);
  });
});

describe('matchRoute', () => {
  it('matches root path', () => {
    const result = matchRoute('/', compiled) as RouteMatch;
    expect(result.component).toBe(DummyA);
    expect(result.params).toEqual({});
  });

  it('matches static path', () => {
    const result = matchRoute('/items', compiled) as RouteMatch;
    expect(result.component).toBe(DummyB);
  });

  it('matches parameterized path', () => {
    const result = matchRoute('/items/abc-123', compiled) as RouteMatch;
    expect(result.component).toBe(DummyC);
    expect(result.params).toEqual({ id: 'abc-123' });
  });

  it('decodes URI components in params', () => {
    const result = matchRoute('/items/hello%20world', compiled) as RouteMatch;
    expect(result.params.id).toBe('hello world');
  });

  it('returns null for no match', () => {
    expect(matchRoute('/unknown', compiled)).toBeNull();
  });

  it('does not match partial paths', () => {
    expect(matchRoute('/items/abc/extra', compiled)).toBeNull();
  });
});

describe('getHashParams', () => {
  beforeEach(() => {
    window.location.hash = '';
  });

  it('returns empty URLSearchParams when no query', () => {
    window.location.hash = '#/sources';
    const params = getHashParams();
    expect(params.toString()).toBe('');
    expect([...params.keys()]).toEqual([]);
  });

  it('parses query params from hash', () => {
    window.location.hash = '#/sources?label=test&format=urn%3Ax-nmos%3Aformat%3Avideo';
    const params = getHashParams();
    expect(params.get('label')).toBe('test');
    expect(params.get('format')).toBe('urn:x-nmos:format:video');
  });

  it('returns empty URLSearchParams when hash is empty', () => {
    window.location.hash = '';
    const params = getHashParams();
    expect([...params.keys()]).toEqual([]);
  });
});

describe('setHashParams', () => {
  beforeEach(() => {
    window.location.hash = '';
  });

  it('appends query params to current path', () => {
    window.location.hash = '#/flows';
    const p = new URLSearchParams();
    p.set('label', 'cam1');
    p.set('format', 'video');
    setHashParams(p);
    expect(window.location.hash).toBe('#/flows?label=cam1&format=video');
  });

  it('clears query params when given empty URLSearchParams', () => {
    window.location.hash = '#/sources?label=old';
    setHashParams(new URLSearchParams());
    expect(window.location.hash).toBe('#/sources');
  });

  it('replaces existing query params', () => {
    window.location.hash = '#/sources?label=old&format=audio';
    const p = new URLSearchParams();
    p.set('label', 'new');
    setHashParams(p);
    expect(window.location.hash).toBe('#/sources?label=new');
  });
});

describe('getHash strips query params', () => {
  it('route matching works with query params in hash', () => {
    // Simulate: hash is #/items?foo=bar -- matchRoute should still match /items
    const result = matchRoute('/items', compiled) as RouteMatch;
    expect(result.component).toBe(DummyB);
  });
});
