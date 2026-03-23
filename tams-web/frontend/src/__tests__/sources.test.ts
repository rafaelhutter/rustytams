import { describe, it, expect } from 'vitest';
import { buildSourcesQuery } from '../lib/query.js';

describe('Sources query building', () => {
  it('builds bare /sources with no filters', () => {
    expect(buildSourcesQuery({})).toBe('/sources');
  });

  it('includes label filter', () => {
    expect(buildSourcesQuery({ label: 'Camera 1' })).toBe('/sources?label=Camera+1');
  });

  it('includes format filter', () => {
    expect(buildSourcesQuery({ format: 'urn:x-nmos:format:video' }))
      .toBe('/sources?format=urn%3Ax-nmos%3Aformat%3Avideo');
  });

  it('includes tag.name=value when both provided', () => {
    expect(buildSourcesQuery({ tagName: 'location', tagValue: 'studio-a' }))
      .toBe('/sources?tag.location=studio-a');
  });

  it('uses tag_exists when tag name provided without value', () => {
    expect(buildSourcesQuery({ tagName: 'location' }))
      .toBe('/sources?tag_exists.location=true');
  });

  it('ignores empty tag name even with value', () => {
    expect(buildSourcesQuery({ tagValue: 'studio-a' })).toBe('/sources');
  });

  it('includes page key for pagination', () => {
    expect(buildSourcesQuery({}, 'abc123')).toBe('/sources?page=abc123');
  });

  it('combines multiple filters', () => {
    const result = buildSourcesQuery({
      label: 'Cam',
      format: 'urn:x-nmos:format:video',
      tagName: 'location',
      tagValue: 'studio-a',
    });
    expect(result).toContain('label=Cam');
    expect(result).toContain('format=urn%3Ax-nmos%3Aformat%3Avideo');
    expect(result).toContain('tag.location=studio-a');
  });

  it('trims whitespace from inputs', () => {
    expect(buildSourcesQuery({ label: '  Camera 1  ' })).toBe('/sources?label=Camera+1');
    expect(buildSourcesQuery({ tagName: ' location ', tagValue: ' studio-a ' }))
      .toBe('/sources?tag.location=studio-a');
  });

  it('includes limit parameter', () => {
    expect(buildSourcesQuery({ limit: 5 })).toBe('/sources?limit=5');
  });
});
