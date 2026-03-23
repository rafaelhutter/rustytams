import { describe, it, expect, vi, afterEach } from 'vitest';
import {
  segmentDuration,
  buildM3u8String,
  buildM3u8BlobUrl,
  buildMasterM3u8String,
  revokeManifest,
  segmentsTimespan,
  segmentStartOffset,
} from '../lib/hls.js';

interface TestSegment {
  timerange: string;
  object_id: string;
  get_urls: Array<{ url: string; presigned?: boolean }>;
}

// Helper: create a segment with timerange and presigned URL
function seg(tr: string, url: string = 'https://media.test/seg.ts', objectId: string = 'abc-123'): TestSegment {
  return {
    timerange: tr,
    object_id: objectId,
    get_urls: [{ url, presigned: true }],
  };
}

describe('segmentDuration', () => {
  it('computes duration from bounded timerange', () => {
    expect(segmentDuration('[0:0_10:0)')).toBeCloseTo(10, 5);
  });

  it('returns fallback for null/undefined', () => {
    expect(segmentDuration(null)).toBe(6);
    expect(segmentDuration(undefined)).toBe(6);
  });

  it('returns fallback for unbounded timerange', () => {
    expect(segmentDuration('_')).toBe(6); // eternity
  });

  it('returns fallback for never', () => {
    expect(segmentDuration('()')).toBe(6);
  });

  it('handles sub-second durations', () => {
    expect(segmentDuration('[0:0_0:500000000)')).toBeCloseTo(0.5, 5);
  });

  it('uses custom fallback', () => {
    expect(segmentDuration(null, 3)).toBe(3);
  });
});

describe('buildM3u8String', () => {
  it('returns null for empty segments', () => {
    expect(buildM3u8String([])).toBeNull();
  });

  it('returns null when no segments have URLs', () => {
    const noUrl = [{ timerange: '[0:0_5:0)', get_urls: [] as Array<{ url: string; presigned?: boolean }> }];
    expect(buildM3u8String(noUrl)).toBeNull();
  });

  it('builds valid m3u8 for single segment', () => {
    const segs = [seg('[0:0_10:0)')];
    const m3u8 = buildM3u8String(segs);
    expect(m3u8).toContain('#EXTM3U');
    expect(m3u8).toContain('#EXT-X-VERSION:3');
    expect(m3u8).toContain('#EXT-X-TARGETDURATION:10');
    expect(m3u8).toContain('#EXT-X-PLAYLIST-TYPE:VOD');
    expect(m3u8).toContain('#EXTINF:10.000000,');
    expect(m3u8).toContain('https://media.test/seg.ts');
    expect(m3u8).toContain('#EXT-X-ENDLIST');
  });

  it('builds m3u8 for multiple segments with discontinuity markers', () => {
    const segs = [
      seg('[0:0_5:0)', 'https://media.test/1.ts'),
      seg('[5:0_10:0)', 'https://media.test/2.ts'),
    ];
    const m3u8 = buildM3u8String(segs);
    // Every segment boundary gets a discontinuity marker because each
    // TAMS media object is independently encoded (separate PTS)
    expect(m3u8).toContain('#EXT-X-DISCONTINUITY');
    expect(m3u8).toContain('https://media.test/1.ts');
    expect(m3u8).toContain('https://media.test/2.ts');
  });

  it('adds discontinuity for non-contiguous segments', () => {
    const segs = [
      seg('[0:0_5:0)', 'https://media.test/1.ts'),
      seg('[20:0_25:0)', 'https://media.test/2.ts'),
    ];
    const m3u8 = buildM3u8String(segs);
    expect(m3u8).toContain('#EXT-X-DISCONTINUITY');
  });

  it('sets target duration to ceiling of max segment duration', () => {
    const segs = [
      seg('[0:0_3:500000000)', 'https://media.test/1.ts'),
      seg('[3:500000000_10:0)', 'https://media.test/2.ts'),
    ];
    const m3u8 = buildM3u8String(segs);
    // Max duration is 6.5s, ceiling = 7
    expect(m3u8).toContain('#EXT-X-TARGETDURATION:7');
  });

  it('prefers presigned URLs', () => {
    const segs = [{
      timerange: '[0:0_5:0)',
      get_urls: [
        { url: 'https://notsigned.test/s.ts', presigned: false },
        { url: 'https://signed.test/s.ts', presigned: true },
      ],
    }];
    const m3u8 = buildM3u8String(segs);
    expect(m3u8).toContain('https://signed.test/s.ts');
    expect(m3u8).not.toContain('https://notsigned.test/s.ts');
  });

  it('falls back to first URL if none presigned', () => {
    const segs = [{
      timerange: '[0:0_5:0)',
      get_urls: [{ url: 'https://first.test/s.ts' }],
    }];
    const m3u8 = buildM3u8String(segs);
    expect(m3u8).toContain('https://first.test/s.ts');
  });
});

describe('buildM3u8BlobUrl', () => {
  afterEach(() => {
    // Vitest jsdom may not have full URL.createObjectURL support
    vi.restoreAllMocks();
  });

  it('returns null for empty segments', () => {
    expect(buildM3u8BlobUrl([])).toBeNull();
  });

  it('creates a blob URL for valid segments', () => {
    // Mock URL.createObjectURL in jsdom
    const mockUrl = 'blob:http://localhost/fake-uuid';
    vi.spyOn(URL, 'createObjectURL').mockReturnValue(mockUrl);
    const result = buildM3u8BlobUrl([seg('[0:0_5:0)')]);
    expect(result).toBe(mockUrl);
    expect(URL.createObjectURL).toHaveBeenCalledWith(expect.any(Blob));
  });
});

describe('revokeManifest', () => {
  it('revokes blob URLs', () => {
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    revokeManifest('blob:http://localhost/fake');
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:http://localhost/fake');
    vi.restoreAllMocks();
  });

  it('ignores non-blob URLs', () => {
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    revokeManifest('https://example.com');
    revokeManifest(null);
    expect(URL.revokeObjectURL).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });
});

describe('segmentsTimespan', () => {
  it('returns 0 for empty array', () => {
    expect(segmentsTimespan([])).toBe(0);
  });

  it('computes total from first start to last end', () => {
    const segs = [
      seg('[0:0_5:0)'),
      seg('[5:0_10:0)'),
      seg('[10:0_20:0)'),
    ];
    expect(segmentsTimespan(segs)).toBeCloseTo(20, 5);
  });

  it('handles non-contiguous segments', () => {
    const segs = [
      seg('[0:0_5:0)'),
      seg('[100:0_110:0)'),
    ];
    // Total span: 0 to 110 = 110
    expect(segmentsTimespan(segs)).toBeCloseTo(110, 5);
  });
});

describe('buildMasterM3u8String', () => {
  it('builds a simple variant playlist without audio', () => {
    const m3u8 = buildMasterM3u8String('blob:video-url');
    expect(m3u8).toContain('#EXTM3U');
    expect(m3u8).toContain('#EXT-X-STREAM-INF:BANDWIDTH=2000000');
    expect(m3u8).toContain('blob:video-url');
    expect(m3u8).not.toContain('#EXT-X-MEDIA');
  });

  it('includes audio renditions in master playlist', () => {
    const m3u8 = buildMasterM3u8String('blob:video-url', [
      { name: 'English', url: 'blob:audio-en' },
      { name: 'French', url: 'blob:audio-fr' },
    ]);
    expect(m3u8).toContain('#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="English",DEFAULT=YES,AUTOSELECT=YES,URI="blob:audio-en"');
    expect(m3u8).toContain('#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="French",DEFAULT=NO,AUTOSELECT=NO,URI="blob:audio-fr"');
    expect(m3u8).toContain('#EXT-X-STREAM-INF:BANDWIDTH=2000000,AUDIO="audio"');
    expect(m3u8).toContain('blob:video-url');
  });

  it('first audio track is default', () => {
    const m3u8 = buildMasterM3u8String('blob:v', [
      { name: 'A', url: 'blob:a1' },
      { name: 'B', url: 'blob:a2' },
    ]);
    const lines = m3u8.split('\n');
    const mediaLines = lines.filter((l: string) => l.startsWith('#EXT-X-MEDIA'));
    expect(mediaLines[0]).toContain('DEFAULT=YES');
    expect(mediaLines[1]).toContain('DEFAULT=NO');
  });

  it('strips quotes from audio track names', () => {
    const m3u8 = buildMasterM3u8String('blob:v', [
      { name: 'My "special" track', url: 'blob:a' },
    ]);
    expect(m3u8).toContain('NAME="My special track"');
    expect(m3u8).not.toContain('NAME="My "special" track"');
  });

  it('handles empty audioTracks array', () => {
    const m3u8 = buildMasterM3u8String('blob:v', []);
    expect(m3u8).not.toContain('#EXT-X-MEDIA');
    expect(m3u8).toContain('#EXT-X-STREAM-INF:BANDWIDTH=2000000');
  });
});

describe('segmentStartOffset', () => {
  it('returns 0 for empty array', () => {
    expect(segmentStartOffset([])).toBe(0);
  });

  it('returns start time of first segment', () => {
    const segs = [seg('[100:0_110:0)')];
    expect(segmentStartOffset(segs)).toBeCloseTo(100, 5);
  });

  it('returns 0 for segments starting at 0', () => {
    const segs = [seg('[0:0_5:0)')];
    expect(segmentStartOffset(segs)).toBe(0);
  });
});
