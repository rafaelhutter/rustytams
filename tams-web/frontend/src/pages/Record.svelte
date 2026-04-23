<script lang="ts">
  import { onMount } from 'svelte';
  import { apiGet } from '../lib/api.js';
  import { buildSourcesQuery } from '../lib/query.js';
  import { push, getHashParams } from '../lib/router.js';
  import { addToast } from '../lib/toast.js';
  import { errorMessage } from '../lib/errors.js';
  import {
    checkWebCodecsSupport, uploadSegment, createFlowWithSource, buildIngestFlowParams, FORMAT_VIDEO, FORMAT_AUDIO, type SourceMode,
    VIDEO_CODEC_OPTIONS, AUDIO_CODEC_OPTIONS,
    VIDEO_QUALITY_PRESETS, AUDIO_QUALITY_PRESETS,
    SEGMENT_DURATION_OPTIONS, KEYFRAME_INTERVAL_OPTIONS, FRAME_RATE_OPTIONS,
    getFrameRate, resolveKeyFrameInterval,
    loadIngestSettings, saveIngestSettings, fetchDefaultSegmentDuration,
  } from '../lib/ingest.js';
  import { formatSeconds } from '../lib/playerUtils.js';
  import Spinner from '../components/Spinner.svelte';
  import type { IngestSettings, SegmentEntry } from '../types/tams.js';

  // --- State machine ---
  type RecordStep = 'select' | 'webcam-setup' | 'webcam-recording' | 'webcam-processing' | 'upload-setup' | 'upload-processing' | 'done';
  type MediaMode = 'video-audio' | 'video-only' | 'audio-only';

  let step: RecordStep = $state('select');

  // --- Browser support ---
  let webCodecsCheck: any = $state(null);

  // --- Webcam state ---
  let stream: MediaStream | null = $state(null);
  let videoPreviewEl: HTMLVideoElement | null = $state(null);
  let cameras: MediaDeviceInfo[] = $state([]);
  let mics: MediaDeviceInfo[] = $state([]);
  let selectedCamera: string = $state('');
  let selectedMic: string = $state('');
  let selectedResolution: string = $state('1280x720');
  let label: string = $state('');

  const RESOLUTIONS: Array<{ label: string; value: string }> = [
    { label: '1080p', value: '1920x1080' },
    { label: '720p', value: '1280x720' },
    { label: '480p', value: '640x480' },
    { label: '360p', value: '640x360' },
  ];

  // Stripped-down shape of mediabunny's DiscardedTrack for GC safety —
  // plain object avoids retaining InputTrack → Input → BlobSource references.
  type DiscardInfo = { track?: { type?: string; codec?: string }; reason: string };

  // Human-readable labels for mediabunny track discard reasons.
  const DISCARD_REASON_LABELS: Record<string, string> = {
    unknown_source_codec: 'unsupported codec',
    undecodable_source_codec: 'codec cannot be decoded in this browser',
    no_encodable_target_codec: 'no compatible output codec available',
    max_track_count_reached: 'too many tracks for output format',
    max_track_count_of_type_reached: 'output format does not support this track type',
  };

  /** Block upload if any audio/video track has an unsupported codec.
   *  Mode filtering is handled upstream: the test conversion only requests
   *  tracks relevant to the current mediaMode, so mediabunny only reports
   *  discards for tracks that were actually attempted. */
  function evaluateDiscardedTracks(
    discarded: DiscardInfo[],
    duration: number,
  ): { duration?: number; error?: string } {
    for (const d of discarded) {
      if (d.reason === 'discarded_by_user') continue;
      const trackType = d.track?.type ?? 'unknown';
      // Only block on audio/video discards — subtitle/data tracks are harmless
      if (trackType !== 'audio' && trackType !== 'video') continue;
      const codecName = d.track?.codec ?? 'unknown codec';
      const why = DISCARD_REASON_LABELS[d.reason] ?? d.reason;
      const hint = trackType === 'audio'
        ? 'ffmpeg -i input.mov -c:v copy -c:a aac output.mp4'
        : 'ffmpeg -i input.mov -c:v libx264 -c:a copy output.mp4';
      return {
        error: `The ${trackType} track (${codecName}) cannot be processed: ${why}. Convert the file first (e.g. ${hint}).`,
      };
    }
    return { duration };
  }

  // --- Recording state ---
  let recording: boolean = $state(false);
  let elapsedSec: number = $state(0);
  let segmentsProduced: number = $state(0);
  let segmentsUploaded: number = $state(0);
  let segmentsFailed: number = $state(0);
  let stopRequested: boolean = false;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let recordingStartTime: number = 0;

  // --- Upload file state ---
  let selectedFile: File | null = $state(null);
  let fileProbe: { duration?: number; error?: string } | null = $state(null);
  let probing: boolean = $state(false);
  let fileProgress: number = $state(0);
  let fileTotalSegments: number = $derived(fileProbe?.duration ? Math.ceil(fileProbe.duration / segmentDuration) : 0);
  let dragOver: boolean = $state(false);

  // --- Done state ---
  let resultFlowId: string | null = $state(null);
  let resultSourceId: string | null = $state(null);
  let resultAudioFlowId: string | null = $state(null);
  let resultAudioSourceId: string | null = $state(null);
  let resultDuration: number = $state(0);
  let resultSegmentCount: number = $state(0);

  // --- Source selection ---
  let existingSources: any[] = $state([]);
  let sourceMode: SourceMode = $state('new');
  let existingSourceId: string = $state('');

  // --- Media mode ---
  let mediaMode: MediaMode = $state('video-audio');

  // --- Encoding settings ---
  let settings: IngestSettings = $state(loadIngestSettings());
  let codecSupport: Record<string, boolean> = $state({}); // { codecId: bool } — populated async on mount

  // Derived from settings
  let videoCodecConfig = $derived(VIDEO_CODEC_OPTIONS.find(c => c.id === settings.videoCodec) || VIDEO_CODEC_OPTIONS[0]);
  let audioCodecConfig = $derived(AUDIO_CODEC_OPTIONS.find(c => c.id === settings.audioCodec) || AUDIO_CODEC_OPTIONS[0]);
  let videoQualityPreset = $derived(VIDEO_QUALITY_PRESETS.find(p => p.id === settings.videoQuality) || VIDEO_QUALITY_PRESETS[1]);
  let audioBitrateValue = $derived((AUDIO_QUALITY_PRESETS.find(p => p.id === settings.audioQuality) || AUDIO_QUALITY_PRESETS[1]).bitrate);
  let segmentDuration = $derived(settings.segmentDuration);
  let effectiveKeyFrameInterval = $derived(resolveKeyFrameInterval(videoQualityPreset, settings));
  let frameRateConfig = $derived(getFrameRate(settings.frameRate));

  function updateSetting(key: string, value: any): void {
    settings = { ...settings, [key]: value };
  }

  function handleSaveDefaults(): void {
    saveIngestSettings(settings);
    addToast('Settings saved as defaults', 'success');
  }

  // --- Segment tracking + upload stats ---
  let segmentList: SegmentEntry[] = $state([]);
  // Each: {index, flowType, timerange, status, bytes, objectId, startedAt}
  let totalBytesUploaded: number = $state(0);
  let uploadSpeedSamples: Array<{ time: number; totalBytes: number }> = $state([]); // [{time, totalBytes}] rolling 10s window

  let queueLength = $derived(segmentList.filter(s => s.status === 'pending').length);
  let uploadingCount = $derived(segmentList.filter(s => s.status === 'uploading').length);
  let uploadSpeed = $derived.by(() => {
    if (uploadSpeedSamples.length < 2) return 0;
    const first = uploadSpeedSamples[0];
    const last = uploadSpeedSamples[uploadSpeedSamples.length - 1];
    const dt = (last.time - first.time) / 1000;
    return dt > 0 ? (last.totalBytes - first.totalBytes) / dt : 0;
  });

  function addSegmentEntry(index: number, flowType: string, timerangeStr: string, bytes: number): number {
    const pos = segmentList.length;
    segmentList = [...segmentList, {
      index, flowType, timerange: timerangeStr,
      status: 'pending', bytes, objectId: null, startedAt: null,
    }];
    return pos;
  }

  function updateSegmentStatus(pos: number, status: SegmentEntry['status'], objectId: string | null = null): void {
    segmentList = segmentList.map((s, i) => i === pos ? { ...s, status, objectId } : s);
  }

  function recordUploadBytes(bytes: number): void {
    totalBytesUploaded += bytes;
    const now = Date.now();
    uploadSpeedSamples = [
      ...uploadSpeedSamples.filter(s => now - s.time < 10000),
      { time: now, totalBytes: totalBytesUploaded },
    ];
  }

  function resetSegmentTracking(): void {
    segmentList = [];
    totalBytesUploaded = 0;
    uploadSpeedSamples = [];
  }

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function formatSpeed(bytesPerSec: number): string {
    if (bytesPerSec <= 0) return '--';
    if (bytesPerSec < 1024) return `${Math.round(bytesPerSec)} B/s`;
    if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
    return `${(bytesPerSec / 1024 / 1024).toFixed(1)} MB/s`;
  }

  // --- Concurrency (event-driven semaphore, no busy-wait) ---
  const MAX_CONCURRENT_UPLOADS: number = 3;
  let inFlightUploads: number = 0;
  let uploadWaiters: Array<() => void> = [];

  async function acquireUploadSlot(): Promise<void> {
    while (inFlightUploads >= MAX_CONCURRENT_UPLOADS) {
      await new Promise(resolve => uploadWaiters.push(resolve));
    }
    inFlightUploads++;
  }

  function releaseUploadSlot(): void {
    inFlightUploads--;
    const pending = uploadWaiters;
    uploadWaiters = [];
    for (const r of pending) r();
  }

  async function waitForUploads(): Promise<void> {
    while (inFlightUploads > 0) {
      await new Promise(resolve => uploadWaiters.push(resolve));
    }
  }

  onMount(() => {
    webCodecsCheck = checkWebCodecsSupport();
    const mode = getHashParams().get('mode');
    if (webCodecsCheck?.supported && mode === 'webcam') selectWebcam();
    else if (webCodecsCheck?.supported && mode === 'upload') selectUpload();
    // Async codec capability check — non-blocking, populates codecSupport
    probeCodecSupport();
    // Apply server-configured default segment duration only if user has no saved preference
    const hasSaved = !!localStorage.getItem('tams-ingest-settings');
    if (!hasSaved) {
      fetchDefaultSegmentDuration().then(dur => {
        settings = { ...settings, segmentDuration: dur };
      });
    }
    return () => cleanup();
  });

  async function probeCodecSupport(): Promise<void> {
    try {
      const { canEncode } = await import('mediabunny');
      const allCodecs = [...VIDEO_CODEC_OPTIONS, ...AUDIO_CODEC_OPTIONS];
      const checks = await Promise.all(
        allCodecs.map(c => canEncode(c.id).then(ok => [c.id, ok]).catch(() => [c.id, true]))
      );
      codecSupport = Object.fromEntries(checks);
    } catch {
      // mediabunny not available — leave all enabled
    }
  }

  function cleanup(): void {
    if (stream) {
      stream.getTracks().forEach(t => t.stop());
      stream = null;
    }
    if (elapsedTimer) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
    inFlightUploads = 0;
    uploadWaiters = [];
  }

  // --- Mode selection ---

  function selectWebcam(): void {
    if (!webCodecsCheck?.supported) return;
    step = 'webcam-setup';
    startPreview();
  }

  function selectUpload(): void {
    if (!webCodecsCheck?.supported) return;
    step = 'upload-setup';
    loadSources();
  }

  function goBack(): void {
    cleanup();
    step = 'select';
    selectedFile = null;
    label = '';
  }

  // --- Source loading ---

  async function loadSources(): Promise<void> {
    try {
      const { data } = await apiGet(buildSourcesQuery());
      existingSources = Array.isArray(data) ? data : [];
    } catch { existingSources = []; }
  }

  // --- Webcam preview ---

  async function startPreview(): Promise<void> {
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      cameras = devices.filter(d => d.kind === 'videoinput');
      mics = devices.filter(d => d.kind === 'audioinput');
      if (cameras.length) selectedCamera = cameras[0].deviceId;
      if (mics.length) selectedMic = mics[0].deviceId;
      await loadSources();
      await requestStream();
    } catch (err) {
      addToast(`Camera access failed: ${errorMessage(err)}`, 'error');
    }
  }

  async function requestStream(): Promise<void> {
    if (stream) stream.getTracks().forEach(t => t.stop());
    const constraints = {};

    if (mediaMode !== 'audio-only') {
      const [w, h] = selectedResolution.split('x').map(Number);
      const fr = getFrameRate(settings.frameRate);
      const videoConstraints = {
        width: { ideal: w }, height: { ideal: h },
        frameRate: { ideal: fr.num / fr.den },
      };
      if (selectedCamera) videoConstraints.deviceId = { exact: selectedCamera };
      constraints.video = videoConstraints;
    }

    if (mediaMode !== 'video-only') {
      if (selectedMic) constraints.audio = { deviceId: { exact: selectedMic } };
      else constraints.audio = true;
    }

    try {
      stream = await navigator.mediaDevices.getUserMedia(constraints);
      if (videoPreviewEl) videoPreviewEl.srcObject = stream;
    } catch (err) {
      addToast(`Media access error: ${errorMessage(err)}`, 'error');
    }
  }

  function bindPreview(node: HTMLVideoElement): { destroy(): void } {
    videoPreviewEl = node;
    if (stream) node.srcObject = stream;
    return {
      destroy() { videoPreviewEl = null; },
    };
  }

  // --- Webcam recording ---

  async function startRecording(): Promise<void> {
    if (!stream || recording) return;
    resetSegmentTracking();

    try {
      const videoSourceId = crypto.randomUUID();
      const videoFlowId = crypto.randomUUID();
      const recLabel = label || `Recording ${new Date().toISOString().slice(0, 16).replace('T', ' ')}`;
      const today = new Date().toISOString().split('T')[0];

      // Get video/audio track settings for essence_parameters
      const videoTrack = stream.getVideoTracks()[0];
      const audioTrack = stream.getAudioTracks()?.[0] || null;
      const trackSettings = videoTrack?.getSettings() || {};
      const audioSettings = audioTrack?.getSettings?.() || {};

      const audioFlowId = audioTrack ? crypto.randomUUID() : null;
      const audioSourceId = audioFlowId ? crypto.randomUUID() : null;

      const { primary, audio } = buildIngestFlowParams({
        sourceId: videoSourceId,
        flowId: videoFlowId,
        isAudioOnly: false,
        videoCodec: videoCodecConfig,
        audioCodec: audioCodecConfig,
        label: recLabel,
        sourceDescription: `Recorded from webcam on ${today}`,
        sourceMode,
        existingSourceId,
        audioFlowId,
        audioSourceId,
        audioSourceDescription: `Audio from webcam recording on ${today}`,
        videoEssenceParameters: { frame_width: trackSettings.width || 1280, frame_height: trackSettings.height || 720 },
        audioEssenceParameters: { sample_rate: audioSettings.sampleRate || 48000, channels: audioSettings.channelCount || 2 },
      });

      if (audio) await createFlowWithSource(audio);
      await createFlowWithSource(primary);

      resultFlowId = videoFlowId;
      resultSourceId = primary.sourceId;
      resultAudioFlowId = audioFlowId;
      resultAudioSourceId = audio?.sourceId ?? null;
      recording = true;
      stopRequested = false;
      segmentsProduced = 0;
      segmentsUploaded = 0;
      segmentsFailed = 0;
      elapsedSec = 0;
      recordingStartTime = Date.now();
      step = 'webcam-recording';

      // Start elapsed timer
      elapsedTimer = setInterval(() => {
        elapsedSec = Math.floor((Date.now() - recordingStartTime) / 1000);
      }, 500);

      // Listen for track ending (device unplug)
      if (videoTrack) {
        videoTrack.addEventListener('ended', () => {
          if (recording) {
            addToast('Camera disconnected — stopping recording', 'warning');
            stopRecording();
          }
        }, { once: true });
      }

      // Start segment loop
      await recordingLoop(videoFlowId, audioFlowId, stream);
    } catch (err) {
      addToast(`Recording failed: ${errorMessage(err)}`, 'error');
      cleanup();
      recording = false;
      step = 'webcam-setup';
    }
  }

  async function recordingLoop(videoFlowId: string, audioFlowId: string | null, mediaStream: MediaStream): Promise<void> {
    const { Output, MpegTsOutputFormat, BufferTarget, MediaStreamVideoTrackSource, MediaStreamAudioTrackSource } = await import('mediabunny');

    const videoTrack = mediaStream.getVideoTracks()[0];
    const audioTrack = audioFlowId ? mediaStream.getAudioTracks()?.[0] || null : null;
    let segmentIndex = 0;

    while (!stopRequested) {
      const segStart = segmentIndex * segmentDuration;

      // Video output (video-only TS)
      const videoOutput = new Output({
        format: new MpegTsOutputFormat(),
        target: new BufferTarget(),
      });
      const videoSource = new MediaStreamVideoTrackSource(videoTrack, {
        codec: settings.videoCodec,
        bitrate: videoQualityPreset.bitrate,
        keyFrameInterval: effectiveKeyFrameInterval,
      });
      videoSource.errorPromise.catch(() => {});
      videoOutput.addVideoTrack(videoSource, { frameRate: frameRateConfig.num / frameRateConfig.den });

      // Audio output (audio-only TS, separate from video)
      let audioOutput: any = null;
      let audioSource: any = null;
      if (audioTrack) {
        audioOutput = new Output({
          format: new MpegTsOutputFormat(),
          target: new BufferTarget(),
        });
        audioSource = new MediaStreamAudioTrackSource(audioTrack, {
          codec: settings.audioCodec,
          bitrate: audioBitrateValue,
        });
        audioSource.errorPromise.catch(() => {});
        audioOutput.addAudioTrack(audioSource);
      }

      await videoOutput.start();
      if (audioOutput) await audioOutput.start();

      // Wait for segment duration or stop — track actual elapsed time
      const segWallStart = performance.now();
      await new Promise(resolve => {
        let check;
        const timeout = setTimeout(() => {
          clearInterval(check);
          resolve();
        }, segmentDuration * 1000);
        check = setInterval(() => {
          if (stopRequested) {
            clearTimeout(timeout);
            clearInterval(check);
            resolve();
          }
        }, 100);
      });
      const segActualDuration = (performance.now() - segWallStart) / 1000;

      // Close sources before finalize to stop frame capture and prevent VideoFrame GC warnings
      videoSource.close();
      if (audioSource) audioSource.close();
      await videoOutput.finalize();
      if (audioOutput) await audioOutput.finalize();

      // Use actual duration for the last segment (when stopped early)
      const segEnd = stopRequested
        ? segStart + Math.min(segActualDuration, segmentDuration)
        : segStart + segmentDuration;

      // Skip tiny segments (< 0.1s) from stop timing
      if (segEnd - segStart < 0.1) break;

      // Upload video segment
      const videoBytes = videoOutput.target.buffer;
      if (videoBytes && videoBytes.byteLength > 0) {
        segmentsProduced++;
        const tr = `[${segStart}s–${segEnd}s)`;
        const vPos = addSegmentEntry(segmentIndex, 'video', tr, videoBytes.byteLength);
        await acquireUploadSlot();
        updateSegmentStatus(vPos, 'uploading');
        uploadSegment(videoFlowId, new Uint8Array(videoBytes), segStart, segEnd)
          .then((result) => { segmentsUploaded++; updateSegmentStatus(vPos, 'done', result.objectId); recordUploadBytes(videoBytes.byteLength); })
          .catch(() => { segmentsFailed++; updateSegmentStatus(vPos, 'failed'); })
          .finally(() => { releaseUploadSlot(); });
      }

      // Upload audio segment
      if (audioOutput) {
        const audioBytes = audioOutput.target.buffer;
        if (audioBytes && audioBytes.byteLength > 0) {
          const tr = `[${segStart}s–${segEnd}s)`;
          const aPos = addSegmentEntry(segmentIndex, 'audio', tr, audioBytes.byteLength);
          await acquireUploadSlot();
          updateSegmentStatus(aPos, 'uploading');
          uploadSegment(audioFlowId, new Uint8Array(audioBytes), segStart, segEnd)
            .then((result) => { updateSegmentStatus(aPos, 'done', result.objectId); recordUploadBytes(audioBytes.byteLength); })
            .catch(() => { segmentsFailed++; updateSegmentStatus(aPos, 'failed'); })
            .finally(() => { releaseUploadSlot(); });
        }
      }

      segmentIndex++;
    }
  }

  async function stopRecording(): Promise<void> {
    stopRequested = true;
    recording = false;
    if (elapsedTimer) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
    step = 'webcam-processing';

    // Wait for all in-flight uploads
    await waitForUploads();

    resultDuration = elapsedSec;
    resultSegmentCount = segmentsProduced;
    step = 'done';
    cleanup();
  }

  // --- File upload ---

  function handleFileDrop(e: DragEvent): void {
    e.preventDefault();
    dragOver = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) selectFileForUpload(file);
  }

  function handleFileSelect(e: Event): void {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (file) selectFileForUpload(file);
  }

  function reprobeFile(): void {
    // Re-run probe when media mode changes — the test conversion
    // includes different tracks per mode, affecting discard detection.
    if (selectedFile) {
      selectFileForUpload(selectedFile);
    }
  }

  async function selectFileForUpload(file: File): Promise<void> {
    if (!file.type.startsWith('video/') && !file.type.startsWith('audio/')) {
      addToast('Please select a video or audio file', 'error');
      return;
    }
    selectedFile = file;
    fileProbe = null;
    label = label || file.name; // Preserves user-edited label on re-probe

    // Probe the file to check codec support and get duration.
    // Guard against stale results: if user selects another file mid-probe,
    // discard this probe's results.
    const probeFile = file;
    probing = true;
    let input;
    try {
      const { Input, BlobSource, ALL_FORMATS, Conversion, Output, MpegTsOutputFormat, BufferTarget } = await import('mediabunny');
      input = new Input({
        source: new BlobSource(file),
        formats: ALL_FORMATS,
      });

      // Helper: only set fileProbe if this probe is still current
      const setProbe = (value) => {
        if (selectedFile === probeFile) fileProbe = value;
      };

      let duration;
      try {
        duration = await input.computeDuration();
      } catch (err) {
        setProbe({ error: `Cannot read file: ${errorMessage(err)}. This format may not be supported.` });
        return;
      }

      if (selectedFile !== probeFile) return; // user selected a different file

      if (!duration || duration <= 0) {
        setProbe({ error: 'Could not determine file duration. The file may be corrupt or in an unsupported format.' });
        return;
      }

      // Try a short test conversion to check codec support
      let testConversion;
      try {
        const testOutput = new Output({
          format: new MpegTsOutputFormat(),
          target: new BufferTarget(),
        });
        testConversion = await Conversion.init({
          input,
          output: testOutput,
          trim: { start: 0, end: Math.min(0.5, duration) },
          video: mediaMode !== 'audio-only'
            ? { codec: settings.videoCodec, bitrate: videoQualityPreset.bitrate, keyFrameInterval: effectiveKeyFrameInterval }
            : { discard: true },
          audio: mediaMode !== 'video-only'
            ? { codec: settings.audioCodec, bitrate: audioBitrateValue }
            : { discard: true },
          showWarnings: false,
        });

        if (selectedFile !== probeFile) return;

        if (!testConversion.isValid) {
          setProbe({
            error: 'The video/audio codec in this file is not supported for browser-based conversion. Supported video codecs: H.264, VP8, VP9, AV1. Supported audio codecs: AAC, Opus.',
          });
          return;
        }

        // Check for unsupported tracks — block upload if any track was discarded
        const discarded: DiscardInfo[] = (testConversion.discardedTracks ?? []).map(d => ({
          track: d.track ? { type: d.track.type, codec: d.track.codec } : undefined,
          reason: d.reason,
        }));

        setProbe(evaluateDiscardedTracks(discarded, duration));
      } finally {
        try { testConversion?.dispose?.(); } catch { /* ignore */ }
      }
    } catch (err) {
      if (selectedFile === probeFile) fileProbe = { error: `Failed to analyze file: ${errorMessage(err)}` };
    } finally {
      try { input?.dispose(); } catch { /* ignore */ }
      if (selectedFile === probeFile) probing = false;
    }
  }

  async function startFileUpload(): Promise<void> {
    if (!selectedFile) return;
    step = 'upload-processing';
    resetSegmentTracking();

    try {
      const { Output, MpegTsOutputFormat, BufferTarget, Conversion, Input, BlobSource, ALL_FORMATS } = await import('mediabunny');

      const videoSourceId = crypto.randomUUID();
      const videoFlowId = crypto.randomUUID();
      const isAudioOnly = selectedFile.type.startsWith('audio/');
      const audioFlowId = isAudioOnly ? null : crypto.randomUUID();
      const audioSourceId = audioFlowId ? crypto.randomUUID() : null;

      const { primary, audio } = buildIngestFlowParams({
        sourceId: videoSourceId,
        flowId: videoFlowId,
        isAudioOnly,
        videoCodec: videoCodecConfig,
        audioCodec: audioCodecConfig,
        label: label || selectedFile.name,
        sourceDescription: `Uploaded from ${selectedFile.name}`,
        sourceMode,
        existingSourceId,
        audioFlowId,
        audioSourceId,
        audioSourceDescription: `Audio from ${selectedFile.name}`,
      });

      // Create audio flow first (if applicable) so flow_collection links work
      if (audio) await createFlowWithSource(audio);
      await createFlowWithSource(primary);

      resultFlowId = videoFlowId;
      resultSourceId = primary.sourceId;
      resultAudioFlowId = audioFlowId;
      resultAudioSourceId = audio?.sourceId ?? null;

      // Read file — duration already known from probe
      const input = new Input({
        source: new BlobSource(selectedFile),
        formats: ALL_FORMATS,
      });
      const duration = fileProbe?.duration ?? await input.computeDuration();
      const totalSegments = Math.ceil(duration / segmentDuration);
      fileProgress = 0;
      segmentsFailed = 0;
      let audioGaveUp = false; // set true if first audio segment fails

      // Convert each segment
      for (let i = 0; i < totalSegments; i++) {
        const trimStart = i * segmentDuration;
        const trimEnd = Math.min((i + 1) * segmentDuration, duration);

        // Video (or audio-only) segment
        const videoSegOutput = new Output({
          format: new MpegTsOutputFormat(),
          target: new BufferTarget(),
        });
        // Video segment: encode video, explicitly discard audio (prevents
        // mediabunny from trying to parse unsupported audio codecs like MP3).
        // Audio-only: encode audio, discard video.
        const videoConversion = await Conversion.init({
          input,
          output: videoSegOutput,
          trim: { start: trimStart, end: trimEnd },
          video: isAudioOnly
            ? { discard: true }
            : { codec: settings.videoCodec, bitrate: videoQualityPreset.bitrate, keyFrameInterval: effectiveKeyFrameInterval },
          audio: isAudioOnly
            ? { codec: settings.audioCodec, bitrate: audioBitrateValue }
            : { discard: true },
          showWarnings: false,
        });

        if (!videoConversion.isValid) {
          try { videoConversion.dispose?.(); } catch { /* ignore */ }
          addToast(`Segment ${i + 1}: conversion failed (unsupported codec?)`, 'warning');
          segmentsFailed++;
          fileProgress = i + 1;
          continue;
        }

        await videoConversion.execute();
        const videoBytes = videoSegOutput.target.buffer;
        try { videoConversion.dispose?.(); } catch { /* ignore */ }
        if (videoBytes && videoBytes.byteLength > 0) {
          const tr = `[${trimStart.toFixed(1)}s–${trimEnd.toFixed(1)}s)`;
          const vType = isAudioOnly ? 'audio' : 'video';
          const vPos = addSegmentEntry(i, vType, tr, videoBytes.byteLength);
          updateSegmentStatus(vPos, 'uploading');
          try {
            const result = await uploadSegment(videoFlowId, new Uint8Array(videoBytes), trimStart, trimEnd);
            updateSegmentStatus(vPos, 'done', result.objectId);
            recordUploadBytes(videoBytes.byteLength);
          } catch (err) {
            addToast(`Segment ${i + 1} upload failed: ${errorMessage(err)}`, 'error');
            segmentsFailed++;
            updateSegmentStatus(vPos, 'failed');
          }
        }

        // Audio segment (separate flow) — skip if previous attempts showed no audio
        if (audioFlowId && !audioGaveUp) {
          const audioSegOutput = new Output({
            format: new MpegTsOutputFormat(),
            target: new BufferTarget(),
          });
          const audioConversion = await Conversion.init({
            input,
            output: audioSegOutput,
            trim: { start: trimStart, end: trimEnd },
            video: { discard: true },
            audio: { codec: settings.audioCodec, bitrate: audioBitrateValue },
            showWarnings: false,
          });
          if (!audioConversion.isValid) {
            if (i === 0) {
              addToast('File has no compatible audio track — skipping audio flow', 'warning');
              audioGaveUp = true;
              resultAudioFlowId = null;
            } else {
              addToast(`Audio segment ${i + 1}: conversion invalid`, 'warning');
              segmentsFailed++;
            }
          } else {
            await audioConversion.execute();
            const audioBytes = audioSegOutput.target.buffer;
            if (!audioBytes || audioBytes.byteLength === 0) {
              if (i === 0) {
                addToast('Audio conversion produced empty output — skipping audio flow', 'warning');
                audioGaveUp = true;
                resultAudioFlowId = null;
              } else {
                addToast(`Audio segment ${i + 1}: empty output`, 'warning');
                segmentsFailed++;
              }
            } else {
              const aPos = addSegmentEntry(i, 'audio', `[${trimStart.toFixed(1)}s–${trimEnd.toFixed(1)}s)`, audioBytes.byteLength);
              updateSegmentStatus(aPos, 'uploading');
              try {
                const result = await uploadSegment(audioFlowId, new Uint8Array(audioBytes), trimStart, trimEnd);
                updateSegmentStatus(aPos, 'done', result.objectId);
                recordUploadBytes(audioBytes.byteLength);
              } catch (err) {
                addToast(`Audio segment ${i + 1} upload failed: ${errorMessage(err)}`, 'error');
                segmentsFailed++;
                updateSegmentStatus(aPos, 'failed');
              }
            }
          }
          try { audioConversion.dispose?.(); } catch { /* ignore */ }
        }

        fileProgress = i + 1;
      }

      input.dispose();
      resultDuration = Math.round(duration);
      resultSegmentCount = totalSegments;
      step = 'done';
    } catch (err) {
      addToast(`Upload failed: ${errorMessage(err)}`, 'error');
      inFlightUploads = 0;
      uploadWaiters = [];
      step = 'upload-setup';
    }
  }

  // --- Done actions ---

  function editInPlayer(): void {
    if (resultFlowId) push(`/player/${resultFlowId}`);
  }

  function viewFlow(): void {
    if (resultFlowId) push(`/flows/${resultFlowId}`);
  }

  function recordAgain(): void {
    resultFlowId = null;
    resultSourceId = null;
    resultAudioFlowId = null;
    resultAudioSourceId = null;
    resultDuration = 0;
    resultSegmentCount = 0;
    segmentsFailed = 0;
    stopRequested = false;
    inFlightUploads = 0;
    uploadWaiters = [];
    resetSegmentTracking();
    selectedFile = null;
    fileProbe = null;
    label = '';
    mediaMode = 'video-audio';
    step = 'select';
  }

  // --- Helpers ---

  function formatElapsed(sec: number): string { return formatSeconds(sec, 0); }
</script>

{#snippet encodingSettings(showFrameRate)}
  <details class="encoding-details">
    <summary>Encoding Settings</summary>
    {#if mediaMode !== 'audio-only'}
      <label>
        Video Codec
        <select value={settings.videoCodec} onchange={(e: Event) => updateSetting('videoCodec', (e.target as HTMLSelectElement).value)}>
          {#each VIDEO_CODEC_OPTIONS as opt}
            <option value={opt.id} disabled={codecSupport[opt.id] === false}>{opt.label}{codecSupport[opt.id] === false ? ' (unsupported)' : ''}</option>
          {/each}
        </select>
      </label>
      <label>
        Video Quality
        <select value={settings.videoQuality} onchange={(e: Event) => updateSetting('videoQuality', (e.target as HTMLSelectElement).value)}>
          {#each VIDEO_QUALITY_PRESETS as opt}
            <option value={opt.id}>{opt.label}</option>
          {/each}
        </select>
      </label>
      {#if showFrameRate}
        <label>
          Frame Rate
          <select value={settings.frameRate} onchange={(e: Event) => { updateSetting('frameRate', (e.target as HTMLSelectElement).value); requestStream(); }}>
            {#each FRAME_RATE_OPTIONS as opt}
              <option value={opt.label}>{opt.label} fps</option>
            {/each}
          </select>
        </label>
      {/if}
      {#if !videoQualityPreset.iframeOnly}
        <label>
          Keyframe Interval
          <select value={settings.keyFrameInterval} onchange={(e: Event) => updateSetting('keyFrameInterval', Number((e.target as HTMLSelectElement).value))}>
            {#each KEYFRAME_INTERVAL_OPTIONS as val}
              <option value={val}>{val}s</option>
            {/each}
          </select>
        </label>
      {:else}
        <span class="muted" style="font-size:0.8em">Keyframe: every frame (I-frame only)</span>
      {/if}
    {/if}
    {#if mediaMode !== 'video-only'}
      <label>
        Audio Quality
        <select value={settings.audioQuality} onchange={(e: Event) => updateSetting('audioQuality', (e.target as HTMLSelectElement).value)}>
          {#each AUDIO_QUALITY_PRESETS as opt}
            <option value={opt.id}>{opt.label}</option>
          {/each}
        </select>
      </label>
    {/if}
    <label>
      Segment Duration
      <select value={settings.segmentDuration} onchange={(e: Event) => updateSetting('segmentDuration', Number((e.target as HTMLSelectElement).value))}>
        {#each SEGMENT_DURATION_OPTIONS as val}
          <option value={val}>{val}s</option>
        {/each}
      </select>
    </label>
    <button class="btn-small" onclick={handleSaveDefaults} style="margin-top:0.5em">Save as Defaults</button>
  </details>
{/snippet}

{#snippet uploadStats()}
  <div class="stat-row">
    <span class="stat-label">Queue:</span>
    <span>{queueLength}</span>
  </div>
  <div class="stat-row">
    <span class="stat-label">Uploading:</span>
    <span>{uploadingCount}</span>
  </div>
  <div class="stat-row">
    <span class="stat-label">Uploaded:</span>
    <span>{formatBytes(totalBytesUploaded)}</span>
  </div>
  {#if uploadSpeed > 0}
    <div class="stat-row">
      <span class="stat-label">Speed:</span>
      <span>{formatSpeed(uploadSpeed)}</span>
    </div>
  {/if}
  {#if segmentList.length > 0}
    <div class="segment-scroll">
      <table class="segment-table">
        <thead>
          <tr><th>#</th><th>Type</th><th>ID</th><th>Timerange</th><th>Status</th><th>Size</th></tr>
        </thead>
        <tbody>
          {#each segmentList as seg}
            <tr class:error-text={seg.status === 'failed'}>
              <td>{seg.index + 1}</td>
              <td>{seg.flowType}</td>
              <td class="mono">{seg.objectId?.slice(0, 8) || '--'}</td>
              <td class="mono">{seg.timerange || '--'}</td>
              <td>{seg.status}</td>
              <td>{formatBytes(seg.bytes)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/snippet}

<div class="page">
  {#if step === 'select'}
    <!-- Mode Selection -->
    <h2>Record / Ingest</h2>

    {#if webCodecsCheck && !webCodecsCheck.supported}
      <div class="panel warning-panel">
        <strong>Unsupported Browser</strong>
        <p>Recording requires WebCodecs (Chrome or Edge). Missing: {webCodecsCheck.missing.join(', ')}.</p>
      </div>
    {/if}

    <div class="mode-cards">
      <button class="mode-card" onclick={selectWebcam} disabled={!webCodecsCheck?.supported}>
        <span class="mode-icon">&#9679;</span>
        <strong>Webcam</strong>
        <p>Record live from your camera and microphone</p>
      </button>
      <button class="mode-card" onclick={selectUpload} disabled={!webCodecsCheck?.supported}>
        <span class="mode-icon">&#8613;</span>
        <strong>Upload Video</strong>
        <p>Upload an existing video file from disk</p>
      </button>
    </div>

  {:else if step === 'webcam-setup'}
    <!-- Webcam Setup -->
    <div class="step-header">
      <h2>Record / Ingest &rsaquo; Webcam</h2>
      <button onclick={goBack}>&larr; Back</button>
    </div>

    <!-- Preview full-width -->
    <div class="preview-container" style="margin-bottom:1em">
      {#if mediaMode === 'audio-only'}
        <div class="audio-only-placeholder">Audio Only</div>
      {:else}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video use:bindPreview autoplay muted playsinline class="preview-video"></video>
      {/if}
    </div>

    <!-- Setup form below preview -->
    <div class="setup-form-below">
      <div class="form-columns">
        <div class="form-col panel">
          <h3>Source &amp; Flow</h3>
          <label>
            Source
            <select bind:value={sourceMode}>
              <option value="new">Create new</option>
              <option value="existing">Use existing</option>
            </select>
          </label>

          {#if sourceMode === 'existing'}
            <label>
              Existing Source
              <select bind:value={existingSourceId}>
                {#each existingSources as src}
                  <option value={src.id}>{src.label || src.id.slice(0, 8)}</option>
                {/each}
              </select>
            </label>
          {/if}

          <label>
            Label
            <input type="text" bind:value={label} placeholder="My Recording" />
          </label>

          <h3>Media</h3>
          <div class="radio-group">
            <label><input type="radio" bind:group={mediaMode} value="video-audio" onchange={requestStream} /> Video + Audio</label>
            <label><input type="radio" bind:group={mediaMode} value="video-only" onchange={requestStream} /> Video only</label>
            <label><input type="radio" bind:group={mediaMode} value="audio-only" onchange={requestStream} /> Audio only</label>
          </div>
        </div>

        <div class="form-col panel">
          <h3>Devices</h3>
          {#if mediaMode !== 'audio-only'}
            <label>
              Camera
              <select bind:value={selectedCamera} onchange={requestStream}>
                {#each cameras as cam}
                  <option value={cam.deviceId}>{cam.label || 'Camera'}</option>
                {/each}
              </select>
            </label>
            <label>
              Resolution
              <select bind:value={selectedResolution} onchange={requestStream}>
                {#each RESOLUTIONS as res}
                  <option value={res.value}>{res.label} ({res.value})</option>
                {/each}
              </select>
            </label>
          {/if}
          {#if mediaMode !== 'video-only'}
            <label>
              Mic
              <select bind:value={selectedMic} onchange={requestStream}>
                {#each mics as mic}
                  <option value={mic.deviceId}>{mic.label || 'Microphone'}</option>
                {/each}
              </select>
            </label>
          {/if}
        </div>
      </div>

      {@render encodingSettings(true)}

      <button class="primary" onclick={startRecording} disabled={!stream} style="margin-top:0.75em">
        Start Recording
      </button>
    </div>

  {:else if step === 'webcam-recording'}
    <!-- Recording In Progress -->
    <div class="step-header">
      <h2>Record / Ingest &rsaquo; Webcam</h2>
      <div style="display:flex;align-items:center;gap:1em">
        <div class="rec-indicator">
          <span class="rec-dot"></span> REC
          <span class="elapsed">{formatElapsed(elapsedSec)}</span>
        </div>
        <button class="danger" onclick={stopRecording}>Stop Recording</button>
      </div>
    </div>

    <!-- Preview full-width -->
    <div class="preview-container" style="margin-bottom:1em">
      {#if mediaMode === 'audio-only'}
        <div class="audio-only-placeholder">Audio Only</div>
      {:else}
        <!-- svelte-ignore a11y_media_has_caption -->
        <video use:bindPreview autoplay muted playsinline class="preview-video"></video>
      {/if}
    </div>

    <!-- Stats below preview -->
    <div class="recording-details panel">
      <div class="stats-row-inline">
        <div class="stat-row">
          <span class="stat-label">Produced:</span>
          <span>{segmentsProduced}</span>
        </div>
        <div class="stat-row">
          <span class="stat-label">Uploaded:</span>
          <span>{segmentsUploaded}</span>
        </div>
        {#if segmentsFailed > 0}
          <div class="stat-row error-text">
            <span class="stat-label">Failed:</span>
            <span>{segmentsFailed}</span>
          </div>
        {/if}
      </div>

      {#if segmentsProduced > 0}
        <div class="progress-bar">
          <div class="progress-fill" style="width: {Math.round(segmentsUploaded / segmentsProduced * 100)}%"></div>
        </div>
      {/if}

      {@render uploadStats()}
    </div>

  {:else if step === 'webcam-processing'}
    <!-- Processing / Uploading -->
    <div class="processing-container">
      <h2>Finishing upload...</h2>
      <Spinner />
      <div class="progress-bar" style="width: 300px; margin-top: 1em">
        <div class="progress-fill" style="width: {segmentsProduced > 0 ? Math.round(segmentsUploaded / segmentsProduced * 100) : 0}%"></div>
      </div>
      <p class="muted">Uploading segment {segmentsUploaded} of {segmentsProduced}</p>
    </div>

  {:else if step === 'upload-setup'}
    <!-- File Upload Setup -->
    <div class="step-header">
      <h2>Record / Ingest &rsaquo; Upload</h2>
      <button onclick={goBack}>&larr; Back</button>
    </div>

    <div class="upload-form panel">
      <h3>Source &amp; Flow</h3>
      <div class="form-row">
        <label>
          Source
          <select bind:value={sourceMode}>
            <option value="new">Create new</option>
            <option value="existing">Use existing</option>
          </select>
        </label>

        {#if sourceMode === 'existing'}
          <label>
            Existing Source
            <select bind:value={existingSourceId}>
              {#each existingSources as src}
                <option value={src.id}>{src.label || src.id.slice(0, 8)}</option>
              {/each}
            </select>
          </label>
        {/if}

        <label>
          Label
          <input type="text" bind:value={label} placeholder="Uploaded video" />
        </label>
      </div>

      <h3>Media</h3>
      <div class="radio-group">
        <label><input type="radio" bind:group={mediaMode} value="video-audio" onchange={reprobeFile} /> Video + Audio</label>
        <label><input type="radio" bind:group={mediaMode} value="video-only" onchange={reprobeFile} /> Video only</label>
        <label><input type="radio" bind:group={mediaMode} value="audio-only" onchange={reprobeFile} /> Audio only</label>
      </div>

      {@render encodingSettings(false)}

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="drop-zone"
        class:drag-over={dragOver}
        ondragover={(e) => { e.preventDefault(); dragOver = true; }}
        ondragleave={() => { dragOver = false; }}
        ondrop={handleFileDrop}
        onclick={() => document.getElementById('file-input')?.click()}
        onkeydown={(e) => { if (e.key === 'Enter') document.getElementById('file-input')?.click(); }}
      >
        <input id="file-input" type="file" accept="video/*,audio/*" onchange={handleFileSelect} onclick={(e) => e.stopPropagation()} hidden />
        {#if selectedFile}
          <p><strong>{selectedFile.name}</strong></p>
          <p class="muted">{(selectedFile.size / 1024 / 1024).toFixed(1)} MB</p>
        {:else}
          <p>Drag &amp; drop a video file here<br>or click to browse</p>
          <p class="muted">MP4 - WebM - MKV - MOV</p>
        {/if}
      </div>

      {#if probing}
        <div class="probe-status">
          <Spinner size="16" /> Analyzing file...
        </div>
      {:else if fileProbe?.error}
        <div class="panel error-panel">
          <strong>Unsupported Media</strong>
          <p>{fileProbe.error}</p>
          <button class="btn-small" onclick={() => { selectedFile = null; fileProbe = null; }}>Choose Different File</button>
        </div>
      {:else if fileProbe && !fileProbe.error}
        <div class="probe-info">
          <span>Duration: <strong>{formatElapsed(Math.round(fileProbe.duration))}</strong></span>
          <span>Segments: <strong>{fileTotalSegments}</strong></span>
        </div>
      {/if}

      <button class="primary" onclick={startFileUpload} disabled={!selectedFile || probing || !fileProbe || fileProbe.error}>
        Upload &amp; Ingest
      </button>
    </div>

  {:else if step === 'upload-processing'}
    <!-- File Upload Processing -->
    <div class="step-header">
      <h2>Converting &amp; uploading...</h2>
      {#if selectedFile}
        <span class="muted" style="font-size:0.85em">{selectedFile.name}</span>
      {/if}
    </div>

    <div class="recording-details panel">
      <div class="stats-row-inline">
        <span class="muted">Segment {fileProgress} of {fileTotalSegments} &mdash; {fileTotalSegments > 0 ? Math.round(fileProgress / fileTotalSegments * 100) : 0}%</span>
        {#if segmentsFailed > 0}
          <span class="error-text">{segmentsFailed} failed</span>
        {/if}
      </div>
      <div class="progress-bar">
        <div class="progress-fill" style="width: {fileTotalSegments > 0 ? Math.round(fileProgress / fileTotalSegments * 100) : 0}%"></div>
      </div>
      {@render uploadStats()}
    </div>

  {:else if step === 'done'}
    <!-- Done -->
    <div class="done-container">
      <h2>Recording Complete</h2>

      <div class="panel summary-panel">
        <div class="summary-grid">
          <span class="stat-label">Duration</span>
          <span>{formatElapsed(resultDuration)}</span>
          <span class="stat-label">Segments</span>
          <span>{resultSegmentCount}{resultAudioFlowId ? ` (x2 flows)` : ''}</span>
          <span class="stat-label">Video Source</span>
          <span class="mono">{resultSourceId?.slice(0, 8)}...</span>
          <span class="stat-label">Video Flow</span>
          <span class="mono">{resultFlowId?.slice(0, 8)}...</span>
          {#if resultAudioFlowId}
            <span class="stat-label">Audio Source</span>
            <span class="mono">{resultAudioSourceId?.slice(0, 8)}...</span>
            <span class="stat-label">Audio Flow</span>
            <span class="mono">{resultAudioFlowId?.slice(0, 8)}...</span>
          {/if}
        </div>
      </div>

      {#if segmentsFailed > 0}
        <div class="panel warning-panel">
          {segmentsFailed} segment(s) failed to upload. The recording may have gaps.
        </div>
      {/if}

      <p class="muted">
        {#if resultAudioFlowId}
          Stored as {resultSegmentCount} video + {resultSegmentCount} audio segments in separate TAMS flows,
          linked via flow_collection. Open in Player to see both tracks.
        {:else}
          Stored as {resultSegmentCount} media objects in TAMS. Each segment references
          an object by ID &mdash; no data is duplicated.
        {/if}
      </p>

      <div class="done-actions">
        <button class="primary" onclick={editInPlayer}>Edit in Player</button>
        <button onclick={viewFlow}>View Flow</button>
        <button onclick={recordAgain}>Record Again</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .page {
    padding: 1.5em;
    max-width: 900px;
  }

  /* Mode selection */
  .mode-cards {
    display: flex;
    gap: 1.5em;
    margin-top: 1.5em;
  }
  .mode-card {
    flex: 1;
    padding: 2em;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
    text-align: center;
    transition: border-color 0.15s, background 0.15s;
  }
  .mode-card:hover:not(:disabled) {
    border-color: var(--accent);
    background: rgba(90,159,212,0.05);
  }
  .mode-card:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .mode-icon {
    font-size: 2em;
    display: block;
    margin-bottom: 0.5em;
  }
  .mode-card p {
    color: var(--text-muted);
    font-size: 0.85em;
    margin-top: 0.5em;
  }

  /* Step header */
  .step-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1em;
  }

  /* Setup layout — vertical: preview on top, form below */
  .setup-form-below {
    display: flex;
    flex-direction: column;
    gap: 0.75em;
  }
  .form-columns {
    display: flex;
    gap: 1em;
  }
  .form-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  .preview-container {
    width: 100%;
  }
  .preview-video {
    width: 100%;
    border-radius: 6px;
    background: #1a1a1a;
    aspect-ratio: 16/9;
    object-fit: cover;
  }
  .audio-only-placeholder {
    width: 100%;
    aspect-ratio: 16/9;
    border-radius: 6px;
    background: #1a1a1a;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    font-size: 1.2em;
  }
  .form-col label, .setup-form-below label {
    display: flex;
    flex-direction: column;
    gap: 0.25em;
    font-size: 0.85em;
    color: var(--text-muted);
  }
  .form-col h3 {
    margin: 0.5em 0 0;
    font-size: 0.9em;
  }
  .recording-details {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  .stats-row-inline {
    display: flex;
    gap: 1.5em;
    flex-wrap: wrap;
  }
  .upload-form label {
    display: flex;
    flex-direction: column;
    gap: 0.25em;
    font-size: 0.85em;
    color: var(--text-muted);
  }
  .upload-form h3 {
    margin: 0.5em 0 0;
    font-size: 0.9em;
  }

  /* Recording stats */
  .rec-indicator {
    display: flex;
    align-items: center;
    gap: 0.5em;
    font-weight: 600;
    font-size: 1.1em;
    color: var(--error);
  }
  .rec-dot {
    display: inline-block;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--error);
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }
  .elapsed {
    margin-left: auto;
    color: var(--text);
    font-family: monospace;
  }
  .stat-row {
    display: flex;
    justify-content: space-between;
    font-size: 0.9em;
  }
  .stat-label {
    color: var(--text-muted);
  }

  /* Progress bar */
  .progress-bar {
    height: 6px;
    background: var(--border);
    border-radius: 3px;
    overflow: hidden;
    margin: 0.5em 0;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 0.3s ease;
  }

  /* Processing */
  .processing-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 300px;
    text-align: center;
  }

  /* Upload form */
  .upload-form {
    max-width: 600px;
    display: flex;
    flex-direction: column;
    gap: 1em;
  }
  .form-row {
    display: flex;
    gap: 1em;
    flex-wrap: wrap;
  }
  .form-row label {
    flex: 1;
    min-width: 150px;
  }
  .drop-zone {
    border: 2px dashed var(--border);
    border-radius: 6px;
    padding: 3em 2em;
    text-align: center;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
  }
  .drop-zone:hover, .drop-zone.drag-over {
    border-color: var(--accent);
    background: rgba(90,159,212,0.05);
  }
  .drop-zone p {
    margin: 0.25em 0;
  }

  /* Done */
  .done-container {
    max-width: 500px;
  }
  .summary-panel {
    margin: 1em 0;
  }
  .summary-grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.5em 1.5em;
    font-size: 0.95em;
  }
  .done-actions {
    display: flex;
    gap: 1em;
    margin-top: 1.5em;
  }

  /* Probe status */
  .probe-status {
    display: flex;
    align-items: center;
    gap: 0.5em;
    font-size: 0.9em;
    color: var(--text-muted);
    padding: 0.5em 0;
  }
  .probe-info {
    display: flex;
    gap: 2em;
    font-size: 0.9em;
    padding: 0.5em 0;
  }
  .error-panel {
    border-color: var(--error);
    margin: 0.5em 0;
  }
  .error-panel strong {
    color: var(--error);
  }
  .error-panel p {
    margin: 0.5em 0;
    font-size: 0.85em;
    color: var(--text);
  }

  /* Radio group for media mode */
  .radio-group {
    display: flex;
    flex-direction: column;
    gap: 0.3em;
    font-size: 0.85em;
  }
  .radio-group label {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 0.4em;
    cursor: pointer;
    color: var(--text);
  }

  /* Encoding settings collapsible */
  .encoding-details {
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5em;
    margin: 0.5em 0;
  }
  .encoding-details summary {
    cursor: pointer;
    font-size: 0.85em;
    color: var(--text-muted);
    font-weight: 500;
  }
  .encoding-details label {
    margin-top: 0.4em;
  }

  /* Segment tracking table */
  .segment-scroll {
    max-height: 200px;
    overflow-y: auto;
    margin-top: 0.5em;
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .segment-scroll::-webkit-scrollbar { width: 4px; }
  .segment-scroll::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }
  .segment-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.75em;
  }
  .segment-table th {
    text-align: left;
    color: var(--text-muted);
    font-weight: 500;
    padding: 0.2em 0.4em;
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--panel);
  }
  .segment-table td {
    padding: 0.2em 0.4em;
    border-bottom: 1px solid rgba(68, 68, 68, 0.3);
    white-space: nowrap;
  }

  /* Warning panel */
  .warning-panel {
    border-color: var(--warning);
    color: var(--warning);
    margin: 1em 0;
  }
  .warning-panel p {
    margin: 0.5em 0 0;
    font-size: 0.9em;
  }

  .mono {
    font-family: monospace;
  }
</style>
