import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import type { Mock } from 'vitest';
import {
  buildTimerange, checkWebCodecsSupport, buildIngestFlowParams,
  VIDEO_CODEC_OPTIONS, AUDIO_CODEC_OPTIONS,
  VIDEO_QUALITY_PRESETS, AUDIO_QUALITY_PRESETS,
  SEGMENT_DURATION_OPTIONS, KEYFRAME_INTERVAL_OPTIONS, FRAME_RATE_OPTIONS,
  resolveKeyFrameInterval,
  DEFAULT_INGEST_SETTINGS, loadIngestSettings, saveIngestSettings,
  FORMAT_VIDEO, FORMAT_AUDIO,
} from '../lib/ingest.js';
import type { IngestSettings, VideoQualityPreset } from '../types/tams.js';

// --- buildTimerange ---

describe('buildTimerange', () => {
  it('formats zero start', () => {
    expect(buildTimerange(0, 6)).toBe('[0:0_6:0)');
  });

  it('formats whole seconds', () => {
    expect(buildTimerange(6, 12)).toBe('[6:0_12:0)');
  });

  it('formats fractional seconds', () => {
    expect(buildTimerange(0, 6.5)).toBe('[0:0_6:500000000)');
  });

  it('formats large values', () => {
    expect(buildTimerange(100, 106)).toBe('[100:0_106:0)');
  });

  it('formats sub-second values', () => {
    expect(buildTimerange(0, 0.25)).toBe('[0:0_0:250000000)');
  });

  it('formats mixed seconds and nanoseconds', () => {
    expect(buildTimerange(1.5, 7.75)).toBe('[1:500000000_7:750000000)');
  });

  it('handles zero duration', () => {
    expect(buildTimerange(5, 5)).toBe('[5:0_5:0)');
  });

  it('round-trips through parseTimerange', async () => {
    const { parseTimerange, nanosToSeconds } = await import('../lib/timerange.js');
    const tr = buildTimerange(3.5, 9.25);
    const parsed = parseTimerange(tr);
    expect(parsed.type).toBe('range');
    expect(nanosToSeconds(parsed.start!.nanos)).toBeCloseTo(3.5, 5);
    expect(nanosToSeconds(parsed.end!.nanos)).toBeCloseTo(9.25, 5);
  });
});

// --- allocateObject, uploadObject, registerSegment, uploadSegment ---
// These depend on apiPost/apiFetch from api.js which requires mocking the module.

describe('allocateObject', () => {
  let allocateObject: (flowId: string) => Promise<{ objectId: string; putUrl: string; contentType: string }>;
  let mockApiPost: Mock;

  beforeEach(async () => {
    vi.resetModules();
    mockApiPost = vi.fn();
    vi.doMock('../lib/api.js', () => ({
      apiPost: mockApiPost,
      apiFetch: vi.fn(),
    }));
    const mod = await import('../lib/ingest.js');
    allocateObject = mod.allocateObject;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('parses media_objects response correctly', async () => {
    mockApiPost.mockResolvedValue({
      data: {
        media_objects: [{
          object_id: 'obj-123',
          put_url: {
            url: 'http://media:5801/objects/obj-123?access_token=tok',
            'content-type': 'video/mp2t',
          },
        }],
      },
    });

    const result = await allocateObject('flow-abc');
    expect(mockApiPost).toHaveBeenCalledWith('/flows/flow-abc/storage', { limit: 1 });
    expect(result).toEqual({
      objectId: 'obj-123',
      putUrl: 'http://media:5801/objects/obj-123?access_token=tok',
      contentType: 'video/mp2t',
    });
  });

  it('defaults content-type to video/mp2t when missing', async () => {
    mockApiPost.mockResolvedValue({
      data: {
        media_objects: [{
          object_id: 'obj-456',
          put_url: { url: 'http://media:5801/objects/obj-456' },
        }],
      },
    });

    const result = await allocateObject('flow-abc');
    expect(result.contentType).toBe('video/mp2t');
  });

  it('throws on API failure', async () => {
    mockApiPost.mockRejectedValue(new Error('POST /flows/x/storage failed: 404'));
    await expect(allocateObject('x')).rejects.toThrow('404');
  });
});

describe('uploadObject', () => {
  let uploadObject: (putUrl: string, data: Uint8Array, contentType: string) => Promise<void>;
  let originalFetch: typeof globalThis.fetch;

  beforeEach(async () => {
    vi.resetModules();
    vi.doMock('../lib/api.js', () => ({
      apiPost: vi.fn(),
      apiFetch: vi.fn(),
    }));
    const mod = await import('../lib/ingest.js');
    uploadObject = mod.uploadObject;
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it('sends PUT with correct content-type and body', async () => {
    const mockFetch = vi.fn<Parameters<typeof fetch>, ReturnType<typeof fetch>>()
      .mockResolvedValue({ ok: true, status: 201 } as Response);
    globalThis.fetch = mockFetch;

    const bytes = new Uint8Array([0x47, 0x00, 0x11]);
    await uploadObject('http://media/objects/abc', bytes, 'video/mp2t');

    expect(mockFetch).toHaveBeenCalledWith('http://media/objects/abc', {
      method: 'PUT',
      headers: { 'Content-Type': 'video/mp2t' },
      body: bytes,
    });
  });

  it('throws on non-ok response', async () => {
    globalThis.fetch = vi.fn<Parameters<typeof fetch>, ReturnType<typeof fetch>>()
      .mockResolvedValue({ ok: false, status: 403 } as Response);
    await expect(
      uploadObject('http://media/objects/abc', new Uint8Array(), 'video/mp2t'),
    ).rejects.toThrow('Upload failed: 403');
  });
});

describe('registerSegment', () => {
  let registerSegment: (flowId: string, objectId: string, timerange: string) => Promise<void>;
  let mockApiPost: Mock;

  beforeEach(async () => {
    vi.resetModules();
    mockApiPost = vi.fn().mockResolvedValue({ data: null, status: 201 });
    vi.doMock('../lib/api.js', () => ({
      apiPost: mockApiPost,
      apiFetch: vi.fn(),
    }));
    const mod = await import('../lib/ingest.js');
    registerSegment = mod.registerSegment;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('posts segment with object_id and timerange', async () => {
    await registerSegment('flow-1', 'obj-1', '[0:0_6:0)');
    expect(mockApiPost).toHaveBeenCalledWith('/flows/flow-1/segments', {
      object_id: 'obj-1',
      timerange: '[0:0_6:0)',
    });
  });
});

describe('uploadSegment', () => {
  let uploadSegment: (flowId: string, data: Uint8Array, start: number, end: number) => Promise<{ objectId: string }>;
  let mockApiPost: Mock;
  let originalFetch: typeof globalThis.fetch;

  beforeEach(async () => {
    vi.resetModules();
    mockApiPost = vi.fn();
    vi.doMock('../lib/api.js', () => ({
      apiPost: mockApiPost,
      apiFetch: vi.fn(),
    }));
    const mod = await import('../lib/ingest.js');
    uploadSegment = mod.uploadSegment;
    mod.setRetryDelay(0); // no delays in tests
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
    vi.restoreAllMocks();
  });

  it('orchestrates allocate -> upload -> register', async () => {
    // allocateObject call
    mockApiPost.mockResolvedValueOnce({
      data: {
        media_objects: [{
          object_id: 'obj-999',
          put_url: { url: 'http://media/objects/obj-999', 'content-type': 'video/mp2t' },
        }],
      },
    });
    // registerSegment call
    mockApiPost.mockResolvedValueOnce({ data: null, status: 201 });

    globalThis.fetch = vi.fn<Parameters<typeof fetch>, ReturnType<typeof fetch>>()
      .mockResolvedValue({ ok: true, status: 201 } as Response);

    const result = await uploadSegment('flow-1', new Uint8Array([1, 2, 3]), 0, 6);

    expect(result).toEqual({ objectId: 'obj-999' });
    // First call: allocate
    expect(mockApiPost).toHaveBeenNthCalledWith(1, '/flows/flow-1/storage', { limit: 1 });
    // Second call: register
    expect(mockApiPost).toHaveBeenNthCalledWith(2, '/flows/flow-1/segments', {
      object_id: 'obj-999',
      timerange: '[0:0_6:0)',
    });
    // PUT upload
    expect(globalThis.fetch).toHaveBeenCalledWith('http://media/objects/obj-999', expect.objectContaining({
      method: 'PUT',
    }));
  });

  it('retries on failure and succeeds', async () => {
    // First attempt: allocate fails
    mockApiPost.mockRejectedValueOnce(new Error('network error'));
    // Second attempt: all succeed
    mockApiPost.mockResolvedValueOnce({
      data: {
        media_objects: [{
          object_id: 'obj-retry',
          put_url: { url: 'http://media/objects/obj-retry', 'content-type': 'video/mp2t' },
        }],
      },
    });
    mockApiPost.mockResolvedValueOnce({ data: null, status: 201 });
    globalThis.fetch = vi.fn<Parameters<typeof fetch>, ReturnType<typeof fetch>>()
      .mockResolvedValue({ ok: true, status: 201 } as Response);

    const result = await uploadSegment('flow-1', new Uint8Array(), 0, 6);
    expect(result).toEqual({ objectId: 'obj-retry' });
  });

  it('throws after all retries exhausted', async () => {
    mockApiPost.mockImplementation(() => Promise.reject(new Error('persistent failure')));

    await expect(uploadSegment('flow-1', new Uint8Array(), 0, 6)).rejects.toThrow('persistent failure');
    // Should have attempted 3 times
    expect(mockApiPost).toHaveBeenCalledTimes(3);
  });
});

// --- createFlowWithSource ---

describe('createFlowWithSource', () => {
  let createFlowWithSource: (opts: Record<string, unknown>) => Promise<void>;
  let mockApiPut: Mock;

  beforeEach(async () => {
    vi.resetModules();
    mockApiPut = vi.fn().mockResolvedValue({ data: null });
    vi.doMock('../lib/api.js', () => ({
      apiPost: vi.fn(),
      apiPut: mockApiPut,
      apiFetch: vi.fn(),
    }));
    const mod = await import('../lib/ingest.js');
    createFlowWithSource = mod.createFlowWithSource;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('creates flow with default video essence_parameters and codec', async () => {
    await createFlowWithSource({
      sourceId: 'src-1',
      flowId: 'flow-1',
      format: 'urn:x-nmos:format:video',
      sourceLabel: 'Test',
    });

    expect(mockApiPut).toHaveBeenCalledWith('/flows/flow-1', expect.objectContaining({
      source_id: 'src-1',
      format: 'urn:x-nmos:format:video',
      codec: 'video/H264',
      essence_parameters: { frame_width: 1920, frame_height: 1080 },
    }));
    expect(mockApiPut).toHaveBeenCalledWith('/sources/src-1/label', 'Test');
  });

  it('creates flow with default audio essence_parameters and codec', async () => {
    await createFlowWithSource({
      sourceId: 'src-2',
      flowId: 'flow-2',
      format: 'urn:x-nmos:format:audio',
      sourceLabel: 'Audio',
    });

    expect(mockApiPut).toHaveBeenCalledWith('/flows/flow-2', expect.objectContaining({
      codec: 'audio/AAC',
      essence_parameters: { sample_rate: 48000, channels: 2 },
    }));
  });

  it('uses custom essenceParameters and codec when provided', async () => {
    await createFlowWithSource({
      sourceId: 'src-3',
      flowId: 'flow-3',
      format: 'urn:x-nmos:format:video',
      codec: 'video/VP9',
      essenceParameters: { frame_width: 640, frame_height: 480 },
      sourceLabel: 'Custom',
    });

    expect(mockApiPut).toHaveBeenCalledWith('/flows/flow-3', expect.objectContaining({
      codec: 'video/VP9',
      essence_parameters: { frame_width: 640, frame_height: 480 },
    }));
  });

  it('sets container and label on flow when provided', async () => {
    await createFlowWithSource({
      sourceId: 'src-4',
      flowId: 'flow-4',
      format: 'urn:x-nmos:format:video',
      container: 'video/mp2t',
      label: 'My Recording',
      setSourceMeta: false,
    });

    expect(mockApiPut).toHaveBeenCalledWith('/flows/flow-4', expect.objectContaining({
      container: 'video/mp2t',
      label: 'My Recording',
    }));
    // Should NOT set source label/description
    expect(mockApiPut).toHaveBeenCalledTimes(1);
  });

  it('sets source label and description in parallel', async () => {
    await createFlowWithSource({
      sourceId: 'src-5',
      flowId: 'flow-5',
      format: 'urn:x-nmos:format:video',
      sourceLabel: 'Label',
      sourceDescription: 'Desc',
    });

    expect(mockApiPut).toHaveBeenCalledWith('/sources/src-5/label', 'Label');
    expect(mockApiPut).toHaveBeenCalledWith('/sources/src-5/description', 'Desc');
  });

  it('skips source meta when setSourceMeta is false', async () => {
    await createFlowWithSource({
      sourceId: 'src-6',
      flowId: 'flow-6',
      format: 'urn:x-nmos:format:video',
      sourceLabel: 'Label',
      sourceDescription: 'Desc',
      setSourceMeta: false,
    });

    expect(mockApiPut).toHaveBeenCalledTimes(1); // only the flow PUT
  });

  it('uses label as sourceLabel fallback', async () => {
    await createFlowWithSource({
      sourceId: 'src-7',
      flowId: 'flow-7',
      format: 'urn:x-nmos:format:video',
      label: 'Flow Label',
    });

    expect(mockApiPut).toHaveBeenCalledWith('/sources/src-7/label', 'Flow Label');
  });
});

// --- checkWebCodecsSupport ---

describe('checkWebCodecsSupport', () => {
  it('reports missing APIs', () => {
    // In jsdom, VideoEncoder and MediaStreamTrackProcessor don't exist
    const result = checkWebCodecsSupport();
    expect(result.supported).toBe(false);
    expect(result.missing).toContain('VideoEncoder');
    expect(result.missing).toContain('MediaStreamTrackProcessor');
  });

  it('reports supported when APIs exist', () => {
    (globalThis as Record<string, unknown>).VideoEncoder = class {};
    (globalThis as Record<string, unknown>).MediaStreamTrackProcessor = class {};
    try {
      const result = checkWebCodecsSupport();
      expect(result.supported).toBe(true);
      expect(result.missing).toEqual([]);
    } finally {
      delete (globalThis as Record<string, unknown>).VideoEncoder;
      delete (globalThis as Record<string, unknown>).MediaStreamTrackProcessor;
    }
  });
});

// --- Codec/Quality/Duration Constants ---

describe('VIDEO_CODEC_OPTIONS', () => {
  it('contains only TS-compatible codecs', () => {
    for (const opt of VIDEO_CODEC_OPTIONS) {
      expect(opt).toHaveProperty('id');
      expect(opt).toHaveProperty('label');
      expect(opt).toHaveProperty('tamsCodec');
      expect(opt.container).toBe('video/mp2t');
    }
  });

  it('includes H.264 and H.265', () => {
    const ids = VIDEO_CODEC_OPTIONS.map((o) => o.id);
    expect(ids).toContain('avc');
    expect(ids).toContain('hevc');
  });
});

describe('AUDIO_CODEC_OPTIONS', () => {
  it('contains AAC with correct shape', () => {
    expect(AUDIO_CODEC_OPTIONS).toHaveLength(1);
    expect(AUDIO_CODEC_OPTIONS[0]).toEqual({
      id: 'aac', label: 'AAC', tamsCodec: 'audio/AAC', container: 'video/mp2t',
    });
  });
});

describe('VIDEO_QUALITY_PRESETS', () => {
  it('each preset has id, label, and bitrate', () => {
    for (const p of VIDEO_QUALITY_PRESETS) {
      expect(p).toHaveProperty('id');
      expect(p).toHaveProperty('label');
      expect(typeof p.bitrate).toBe('number');
      expect(p.bitrate).toBeGreaterThan(0);
    }
  });

  it('bitrates range from 1 Mbps to 50 Mbps', () => {
    const bitrates = VIDEO_QUALITY_PRESETS.map((p) => p.bitrate);
    expect(Math.min(...bitrates)).toBe(1_000_000);
    expect(Math.max(...bitrates)).toBe(50_000_000);
  });

  it('has exactly one iframe-only preset', () => {
    const iframePresets = VIDEO_QUALITY_PRESETS.filter((p) => p.iframeOnly);
    expect(iframePresets).toHaveLength(1);
    expect(iframePresets[0].id).toBe('iframe');
  });
});

describe('AUDIO_QUALITY_PRESETS', () => {
  it('each preset has id, label, and bitrate', () => {
    for (const p of AUDIO_QUALITY_PRESETS) {
      expect(p).toHaveProperty('id');
      expect(p).toHaveProperty('label');
      expect(typeof p.bitrate).toBe('number');
    }
  });
});

describe('SEGMENT_DURATION_OPTIONS', () => {
  it('contains expected values including default 6', () => {
    expect(SEGMENT_DURATION_OPTIONS).toContain(6);
    expect(SEGMENT_DURATION_OPTIONS).toContain(2);
    expect(SEGMENT_DURATION_OPTIONS).toContain(30);
  });

  it('is sorted ascending', () => {
    for (let i = 1; i < SEGMENT_DURATION_OPTIONS.length; i++) {
      expect(SEGMENT_DURATION_OPTIONS[i]).toBeGreaterThan(SEGMENT_DURATION_OPTIONS[i - 1]);
    }
  });
});

describe('KEYFRAME_INTERVAL_OPTIONS', () => {
  it('contains expected values', () => {
    expect(KEYFRAME_INTERVAL_OPTIONS).toEqual([0.5, 1, 2, 4]);
  });
});

describe('FRAME_RATE_OPTIONS', () => {
  it('each entry has num, den, and label', () => {
    for (const opt of FRAME_RATE_OPTIONS) {
      expect(opt).toHaveProperty('num');
      expect(opt).toHaveProperty('den');
      expect(opt).toHaveProperty('label');
      expect(typeof opt.num).toBe('number');
      expect(typeof opt.den).toBe('number');
      expect(opt.den).toBeGreaterThan(0);
    }
  });

  it('contains standard frame rates by label', () => {
    const labels = FRAME_RATE_OPTIONS.map((f) => f.label);
    expect(labels).toContain('24');
    expect(labels).toContain('25');
    expect(labels).toContain('30');
    expect(labels).toContain('50');
    expect(labels).toContain('60');
  });

  it('contains NTSC fractional frame rates as exact rationals', () => {
    const ntsc30 = FRAME_RATE_OPTIONS.find((f) => f.label === '29.97');
    expect(ntsc30).toEqual({ num: 30000, den: 1001, label: '29.97' });

    const ntsc24 = FRAME_RATE_OPTIONS.find((f) => f.label === '23.976');
    expect(ntsc24).toEqual({ num: 24000, den: 1001, label: '23.976' });

    const ntsc60 = FRAME_RATE_OPTIONS.find((f) => f.label === '59.94');
    expect(ntsc60).toEqual({ num: 60000, den: 1001, label: '59.94' });
  });
});

// --- resolveKeyFrameInterval ---

describe('resolveKeyFrameInterval', () => {
  it('returns settings.keyFrameInterval for normal presets', () => {
    const preset = { id: 'medium', bitrate: 2_000_000 } as VideoQualityPreset;
    const settings = { keyFrameInterval: 2, frameRate: '30' };
    expect(resolveKeyFrameInterval(preset, settings)).toBe(2);
  });

  it('returns exact rational frame duration for iframe-only at 30fps', () => {
    const preset = { id: 'iframe', bitrate: 25_000_000, iframeOnly: true } as VideoQualityPreset;
    const settings = { keyFrameInterval: 1, frameRate: '30' };
    // 1/30 = 0.0333...
    expect(resolveKeyFrameInterval(preset, settings)).toBeCloseTo(1 / 30, 8);
  });

  it('returns exact rational frame duration for iframe-only at 60fps', () => {
    const preset = { id: 'iframe', bitrate: 25_000_000, iframeOnly: true } as VideoQualityPreset;
    const settings = { keyFrameInterval: 1, frameRate: '60' };
    expect(resolveKeyFrameInterval(preset, settings)).toBeCloseTo(1 / 60, 8);
  });

  it('returns exact rational frame duration for iframe-only at 24fps', () => {
    const preset = { id: 'iframe', bitrate: 25_000_000, iframeOnly: true } as VideoQualityPreset;
    const settings = { keyFrameInterval: 1, frameRate: '24' };
    expect(resolveKeyFrameInterval(preset, settings)).toBeCloseTo(1 / 24, 8);
  });

  it('ignores settings.keyFrameInterval when iframeOnly', () => {
    const preset = { id: 'iframe', bitrate: 25_000_000, iframeOnly: true } as VideoQualityPreset;
    const settings = { keyFrameInterval: 4, frameRate: '30' };
    expect(resolveKeyFrameInterval(preset, settings)).toBeCloseTo(1 / 30, 8);
  });

  it('uses exact rational for NTSC 29.97fps iframe-only', () => {
    const preset = { id: 'iframe', bitrate: 25_000_000, iframeOnly: true } as VideoQualityPreset;
    const settings = { keyFrameInterval: 1, frameRate: '29.97' };
    // Exact: 1001/30000 = 0.033366666...
    expect(resolveKeyFrameInterval(preset, settings)).toBeCloseTo(1001 / 30000, 8);
  });

  it('uses exact rational for NTSC 23.976fps iframe-only', () => {
    const preset = { id: 'iframe', bitrate: 25_000_000, iframeOnly: true } as VideoQualityPreset;
    const settings = { keyFrameInterval: 1, frameRate: '23.976' };
    // Exact: 1001/24000 = 0.041708333...
    expect(resolveKeyFrameInterval(preset, settings)).toBeCloseTo(1001 / 24000, 8);
  });
});

// --- localStorage Settings ---

describe('loadIngestSettings', () => {
  let store: Record<string, string>;

  beforeEach(() => {
    store = {};
    vi.stubGlobal('localStorage', {
      getItem: (key: string): string | null => store[key] ?? null,
      setItem: (key: string, val: string): void => { store[key] = String(val); },
      removeItem: (key: string): void => { delete store[key]; },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('returns defaults when localStorage is empty', () => {
    expect(loadIngestSettings()).toEqual(DEFAULT_INGEST_SETTINGS);
  });

  it('merges partial saved values with defaults', () => {
    store['tams-ingest-settings'] = JSON.stringify({ videoCodec: 'hevc', segmentDuration: 10 });
    const result = loadIngestSettings();
    expect(result.videoCodec).toBe('hevc');
    expect(result.segmentDuration).toBe(10);
    expect(result.audioCodec).toBe('aac');
    expect(result.videoQuality).toBe('medium');
    expect(result.frameRate).toBe('30');
    expect(result.keyFrameInterval).toBe(1);
  });

  it('handles corrupt JSON gracefully', () => {
    store['tams-ingest-settings'] = '{not valid json!!!';
    expect(loadIngestSettings()).toEqual(DEFAULT_INGEST_SETTINGS);
  });

  it('handles empty string gracefully', () => {
    store['tams-ingest-settings'] = '';
    expect(loadIngestSettings()).toEqual(DEFAULT_INGEST_SETTINGS);
  });
});

describe('saveIngestSettings', () => {
  let store: Record<string, string>;

  beforeEach(() => {
    store = {};
    vi.stubGlobal('localStorage', {
      getItem: (key: string): string | null => store[key] ?? null,
      setItem: (key: string, val: string): void => { store[key] = String(val); },
      removeItem: (key: string): void => { delete store[key]; },
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('persists settings to localStorage', () => {
    const settings = { ...DEFAULT_INGEST_SETTINGS, videoCodec: 'hevc', frameRate: 60 } as IngestSettings;
    saveIngestSettings(settings);
    const raw = store['tams-ingest-settings'];
    expect(raw).toBeTruthy();
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    expect(parsed.videoCodec).toBe('hevc');
    expect(parsed.frameRate).toBe(60);
  });

  it('round-trips through loadIngestSettings', () => {
    const settings: IngestSettings = {
      ...DEFAULT_INGEST_SETTINGS,
      videoCodec: 'hevc',
      videoQuality: 'studio',
      segmentDuration: 15,
      frameRate: '50',
      keyFrameInterval: 2,
    };
    saveIngestSettings(settings);
    expect(loadIngestSettings()).toEqual(settings);
  });
});

// --- buildIngestFlowParams ---

describe('buildIngestFlowParams', () => {
  const videoCodec = { tamsCodec: 'video/H264', container: 'video/mp2t' };
  const audioCodec = { tamsCodec: 'audio/AAC', container: 'video/mp2t' };
  const defaults = { sourceDescription: 'Uploaded from test.mp4' };

  it('passes video codec for video files', () => {
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: 'audio-1', audioSourceId: 'asrc-1',
    });
    expect(primary.codec).toBe('video/H264');
    expect(primary.format).toBe(FORMAT_VIDEO);
    expect(primary.container).toBe('video/mp2t');
  });

  it('passes audio codec for audio-only files', () => {
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: true,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: null,
    });
    expect(primary.codec).toBe('audio/AAC');
    expect(primary.format).toBe(FORMAT_AUDIO);
    expect(primary.container).toBe('video/mp2t');
  });

  it('passes H.265 codec when selected', () => {
    const h265Codec = { tamsCodec: 'video/H265', container: 'video/mp2t' };
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec: h265Codec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: 'audio-1', audioSourceId: 'asrc-1',
    });
    expect(primary.codec).toBe('video/H265');
  });

  it('creates audio flow params with caller-provided IDs and descriptions', () => {
    const { audio } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'My Video', sourceDescription: 'From file',
      sourceMode: 'new', audioFlowId: 'audio-1', audioSourceId: 'asrc-1',
      audioSourceDescription: 'Audio from test.mp4',
    });
    expect(audio).not.toBeNull();
    expect(audio!.sourceId).toBe('asrc-1');
    expect(audio!.codec).toBe('audio/AAC');
    expect(audio!.format).toBe(FORMAT_AUDIO);
    expect(audio!.label).toBe('My Video — audio');
    expect(audio!.flowId).toBe('audio-1');
    expect(audio!.sourceDescription).toBe('Audio from test.mp4');
    expect(audio!.setSourceMeta).toBe(true);
  });

  it('defaults audio sourceDescription from label', () => {
    const { audio } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'My Video', ...defaults,
      sourceMode: 'new', audioFlowId: 'audio-1', audioSourceId: 'asrc-1',
    });
    expect(audio!.sourceDescription).toBe('Audio — My Video');
  });

  it('returns null audio for audio-only files', () => {
    const { audio } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: true,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: null,
    });
    expect(audio).toBeNull();
  });

  it('sets flow_collection when audioFlowId provided', () => {
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: 'audio-1', audioSourceId: 'asrc-1',
    });
    expect(primary.flowCollection).toEqual([
      { id: 'flow-1', role: 'video' },
      { id: 'audio-1', role: 'audio' },
    ]);
  });

  it('uses existingSourceId when sourceMode is existing', () => {
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-new', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'existing', existingSourceId: 'src-existing', audioFlowId: null,
    });
    expect(primary.sourceId).toBe('src-existing');
    expect(primary.setSourceMeta).toBe(false);
  });

  it('throws when sourceMode is existing but existingSourceId is missing', () => {
    expect(() => buildIngestFlowParams({
      sourceId: 'src-new', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'existing', audioFlowId: null,
    })).toThrow('existingSourceId is required');
  });

  it('throws when audioFlowId provided but audioSourceId is missing', () => {
    expect(() => buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: 'audio-1',
    })).toThrow('audioSourceId is required');
  });

  it('uses new sourceId when sourceMode is new', () => {
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-new', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: null,
    });
    expect(primary.sourceId).toBe('src-new');
    expect(primary.setSourceMeta).toBe(true);
  });

  it('passes sourceDescription through to primary flow', () => {
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', sourceDescription: 'Recorded from webcam on 2026-03-19',
      sourceMode: 'new', audioFlowId: null,
    });
    expect(primary.sourceDescription).toBe('Recorded from webcam on 2026-03-19');
  });

  it('passes audioEssenceParameters as primary essenceParameters for audio-only files', () => {
    const { primary } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: true,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: null,
      audioEssenceParameters: { sample_rate: 44100, channels: 1 },
    });
    expect(primary.essenceParameters).toEqual({ sample_rate: 44100, channels: 1 });
  });

  it('passes essenceParameters through to flows', () => {
    const { primary, audio } = buildIngestFlowParams({
      sourceId: 'src-1', flowId: 'flow-1', isAudioOnly: false,
      videoCodec, audioCodec, label: 'Test', ...defaults,
      sourceMode: 'new', audioFlowId: 'audio-1', audioSourceId: 'asrc-1',
      videoEssenceParameters: { frame_width: 1920, frame_height: 1080 },
      audioEssenceParameters: { sample_rate: 48000, channels: 2 },
    });
    expect(primary.essenceParameters).toEqual({ frame_width: 1920, frame_height: 1080 });
    expect(audio!.essenceParameters).toEqual({ sample_rate: 48000, channels: 2 });
  });
});
