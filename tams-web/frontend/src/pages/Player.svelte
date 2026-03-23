<script lang="ts">
  import { untrack } from 'svelte';
  import { apiGet, apiPost, apiPut, formatShortName } from '../lib/api.js';
  import { buildFlowsQuery, buildFlowQuery } from '../lib/query.js';
  import { push } from '../lib/router.js';
  import { addToast } from '../lib/toast.js';
  import { errorMessage } from '../lib/errors.js';
  import { parseTimerange, nanosToSeconds } from '../lib/timerange.js';
  import { buildM3u8BlobUrl, buildMasterM3u8String, m3u8BlobUrl, revokeManifest, segmentStartOffset, segmentDuration } from '../lib/hls.js';
  import { detectOverlaps as computeOverlaps, validateExport, findOverlappingSegments, formatSeconds } from '../lib/playerUtils.js';
  import { FORMAT_VIDEO, FORMAT_AUDIO, fetchAllSegments } from '../lib/ingest.js';
  import { collectionFlowIds } from '../types/tams.js';
  import Spinner from '../components/Spinner.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import '@byomakase/omakase-player/dist/style.css';

  interface SegmentationMarker {
    id: string;
    start: number;
    end: number;
    label: string;
  }

  let { params = {} }: { params?: Record<string, string> } = $props();

  // Primary flow + segments
  let flow: any = $state(null);
  let segments: any[] = $state([]);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);
  let loadedId: string | null = null;

  // Related flows (from flow_collection or same source)
  let relatedFlows: any[] = $state([]);
  let relatedSegments: Record<string, any[]> = $state({}); // { flowId: [segments] }
  let selectedLanes: Set<string> = $state(new Set()); // flow IDs to show as lanes

  // Player state
  let playerError: string | null = $state(null);
  let playerReady: boolean = $state(false);
  let manifestUrl: string | null = null;
  let omakasePlayer: any = null;
  let omakaseModule: any = null;
  let currentTimeline: any = null;
  let subscriptions: any[] = []; // RxJS subscriptions for cleanup
  let primaryStartOffset: number = 0; // computed once in initTimeline
  let allBlobUrls: Set<string> = new Set(); // all blob URLs to revoke on cleanup
  let urlRefreshTimer: ReturnType<typeof setInterval> | null = null;
  let refreshInProgress: boolean = false;
  const URL_REFRESH_INTERVAL: number = 10 * 60 * 1000; // 10 minutes

  // VU meter
  let vuMeterInstance: any = null;
  let vuMeterModule: any = null;
  let vuMeterInitialized: boolean = false;
  let vuTriggerFn: (() => void) | null = null; // stored for cleanup

  function removeVuTriggerListeners(): void {
    if (vuTriggerFn) {
      window.removeEventListener('click', vuTriggerFn);
      window.removeEventListener('keydown', vuTriggerFn);
      vuTriggerFn = null;
    }
  }

  // Segmentation editing (8d)
  let segmentationLane: any = null;
  let segmentationMarkers: SegmentationMarker[] = $state([]); // { id, start, end, label }
  let nextMarkerId: number = 0;

  // Overlap detection (8d) — derived from markers, auto-updates on any marker change
  let overlappingIds: Set<string> = $derived(computeOverlaps(segmentationMarkers));

  // Export (8e)
  let showExport: boolean = $state(false);
  let exporting: boolean = $state(false);
  let exportError: string | null = $state(null);

  // Segment marker colors
  const LANE_COLORS: string[] = [
    '#5a9fd4', '#d4a05a', '#5ad47a', '#d45a8a',
    '#8a5ad4', '#5ad4c7', '#d4d45a', '#d47a5a',
  ];

  // ── Data Loading ──────────────────────────────────────────────

  async function fetchSegments(flowId: string) {
    return fetchAllSegments(flowId, { presigned: true });
  }

  // Audio flows pre-fetched for master playlist (set before player init)
  let audioFlowsForManifest: Array<{ flow: any; segments: any[] }> = []; // [{ flow, segments }]

  async function loadFlow(id: string): Promise<void> {
    loadedId = id;
    loading = true;
    error = null;
    flow = null;
    segments = [];
    relatedFlows = [];
    relatedSegments = {};
    selectedLanes = new Set();
    segmentationMarkers = [];
    playerReady = false;
    playerError = null;
    showExport = false;
    audioFlowsForManifest = [];
    destroyPlayer();

    try {
      const [flowResp, segs] = await Promise.all([
        apiGet(buildFlowQuery(id, { includeTimerange: true })),
        fetchSegments(id),
      ]);
      if (id !== loadedId) return;

      const fl = flowResp.data;

      // Fetch related flows + pre-fetch audio segments BEFORE setting flow/segments.
      // This ensures audioFlowsForManifest is populated when the player init fires.
      await loadRelatedFlows(fl);
      if (id !== loadedId) return;

      flow = fl;
      segments = segs;
    } catch (e) {
      if (id !== loadedId) return;
      error = errorMessage(e);
    } finally {
      if (id === loadedId) loading = false;
    }
  }

  // IDs of flows from flow_collection that should auto-enable once timeline is ready
  let autoEnableFlows: string[] = $state([]);

  async function loadRelatedFlows(primaryFlow: any): Promise<void> {
    const related = [];
    const seen = new Set([primaryFlow.id]);
    const fromCollection = new Set();

    // From flow_collection (parallel fetch)
    const collectionIds = collectionFlowIds(primaryFlow.flow_collection).filter(fid => {
      if (seen.has(fid)) return false;
      seen.add(fid);
      return true;
    });
    const results = await Promise.allSettled(
      collectionIds.map(fid => apiGet(buildFlowQuery(fid, { includeTimerange: true })))
    );
    for (const r of results) {
      if (r.status !== 'fulfilled') continue;
      const f = r.value.data;
      if (f.tags?.hls_exclude?.[0] === 'true') continue;
      related.push(f);
      fromCollection.add(f.id);
    }

    // From same source (if no collection and source exists)
    if (related.length === 0 && primaryFlow.source_id) {
      try {
        const resp = await apiGet(buildFlowsQuery({ sourceId: primaryFlow.source_id, limit: 20 }));
        for (const f of (resp.data || [])) {
          if (seen.has(f.id)) continue;
          seen.add(f.id);
          if (f.tags?.hls_exclude?.[0] === 'true') continue;
          related.push(f);
        }
      } catch (err) {
        console.warn('Failed to load related flows by source:', err);
      }
    }

    relatedFlows = related;
    // Auto-enable flows from flow_collection once timeline is ready
    autoEnableFlows = related.filter(f => fromCollection.has(f.id)).map(f => f.id);

    // Pre-fetch segments for audio flows in collection (for master HLS playlist).
    // This must complete before initPlayer runs so the master playlist includes audio.
    const audioFlows = related.filter(f => fromCollection.has(f.id) && f.format === FORMAT_AUDIO);
    if (audioFlows.length > 0) {
      const audioResults = await Promise.allSettled(
        audioFlows.map(async f => ({ flow: f, segments: await fetchSegments(f.id) }))
      );
      audioFlowsForManifest = audioResults
        .filter(r => r.status === 'fulfilled' && r.value.segments.length > 0)
        .map(r => r.value);
      // Cache segments so toggleLane won't re-fetch
      const updates = {};
      for (const af of audioFlowsForManifest) {
        updates[af.flow.id] = af.segments;
      }
      relatedSegments = { ...relatedSegments, ...updates };
    }
  }

  let togglingLanes: Set<string> = new Set();

  async function toggleLane(flowId: string): Promise<void> {
    if (togglingLanes.has(flowId)) return;
    togglingLanes.add(flowId);
    try {
    const next = new Set(selectedLanes);
    if (next.has(flowId)) {
      next.delete(flowId);
      // Remove lane from timeline and free cached segments
      if (currentTimeline) {
        try { currentTimeline.removeTimelineLane(`lane-${flowId}`); } catch (e) { console.warn(`Lane removal failed for ${flowId}:`, e); }
      }
      const { [flowId]: _, ...rest } = relatedSegments;
      relatedSegments = rest;
    } else {
      next.add(flowId);
      // Fetch segments if not cached, then add lane
      if (!relatedSegments[flowId]) {
        try {
          const segs = await fetchSegments(flowId);
          relatedSegments = { ...relatedSegments, [flowId]: segs };
        } catch {
          addToast(`Failed to load segments for flow`, 'error');
          return;
        }
      }
      if (!currentTimeline) {
        addToast('Timeline not ready yet', 'error');
        return;
      }
      if (relatedSegments[flowId]) {
        const rf = relatedFlows.find(f => f.id === flowId);
        addFlowLane(currentTimeline, flowId, rf, relatedSegments[flowId]);
      }
    }
    selectedLanes = next;
    } finally {
      togglingLanes.delete(flowId);
    }
  }

  $effect(() => {
    const id = params.id;
    untrack(() => {
      if (id && id !== loadedId) loadFlow(id);
    });
  });

  // Auto-enable flow_collection flows once player is ready
  $effect(() => {
    if (playerReady && autoEnableFlows.length > 0) {
      const pending = autoEnableFlows;
      untrack(() => {
        autoEnableFlows = [];
        for (const fid of pending) {
          if (!selectedLanes.has(fid)) toggleLane(fid);
        }
      });
    }
  });

  // Derived
  let canPlay = $derived(
    (flow?.format === FORMAT_VIDEO || flow?.format === FORMAT_AUDIO) && segments.length > 0
  );

  // ── Player Lifecycle ──────────────────────────────────────────

  function destroyPlayer(): void {
    for (const sub of subscriptions) {
      try { sub.unsubscribe(); } catch { /* ignore */ }
    }
    subscriptions = [];
    if (omakasePlayer) {
      try { omakasePlayer.destroy(); } catch { /* ignore */ }
      omakasePlayer = null;
    }
    currentTimeline = null;
    segmentationLane = null;
    primaryStartOffset = 0;
    if (highlightTimer) { clearTimeout(highlightTimer); highlightTimer = null; }
    highlightedRow = null;
    if (urlRefreshTimer) { clearInterval(urlRefreshTimer); urlRefreshTimer = null; }
    for (const url of allBlobUrls) revokeManifest(url);
    allBlobUrls.clear();
    manifestUrl = null;
    playerReady = false;
    // VU meter cleanup
    removeVuTriggerListeners();
    if (vuMeterInstance) {
      try { vuMeterInstance.destroy(); } catch { /* ignore */ }
      vuMeterInstance = null;
    }
    vuMeterInitialized = false;
  }

  async function initVuMeter(): Promise<void> {
    if (vuMeterInitialized || !omakasePlayer) return;
    vuMeterInitialized = true;
    try {
      // Activate Web Audio routing — required before peak processor works
      const audioCtx = omakasePlayer.video.getAudioContext();
      if (audioCtx?.state === 'suspended') {
        await audioCtx.resume();
      }
      // Ensure video element volume is up (Omakase may mute by default)
      const videoEl = document.querySelector('.omakase-video');
      if (videoEl) videoEl.volume = 1;

      if (!vuMeterModule) {
        vuMeterModule = await import('@byomakase/vu-meter');
      }
      const el = document.getElementById('vu-meter-container');
      if (!el) return;
      el.innerHTML = '';
      const channelCount = 2; // stereo
      vuMeterInstance = new vuMeterModule.VuMeter(channelCount, el, {
        backgroundColor: '#2b2b2b',
        tickColor: '#888888',
        labelColor: '#aaaaaa',
        fontSize: 10,
        dbRange: 48,
        dbTickSize: 6,
        vertical: true,
      });
      vuMeterInstance.attachSource(
        omakasePlayer.audio.createMainAudioPeakProcessor()
      );
      console.log('[Player] VU meter initialized');
    } catch (err) {
      console.warn('[Player] VU meter init failed:', err);
    }
  }

  function omakaseAction(node: HTMLElement, config: any): { update(newConfig: any): void; destroy(): void } {
    let actionFlowId: string | null = config?.flow?.id || null;
    if (config?.segments?.length) initPlayer(node, config);
    return {
      update(newConfig: any) {
        // Only reinitialize if the flow actually changed
        const newId = newConfig?.flow?.id || null;
        if (newId === actionFlowId) return;
        actionFlowId = newId;
        destroyPlayer();
        if (newConfig?.segments?.length) initPlayer(node, newConfig);
      },
      destroy() { destroyPlayer(); },
    };
  }

  async function initPlayer(container: HTMLElement, config: any): Promise<void> {
    const { segments: segs, flow: fl } = config;
    playerError = null;

    // Build video media playlist
    const videoPlaylistUrl = buildM3u8BlobUrl(segs);
    if (!videoPlaylistUrl) {
      playerError = 'No playable segments found (no presigned URLs)';
      return;
    }
    allBlobUrls.add(videoPlaylistUrl);

    // Build master playlist with audio renditions if applicable
    manifestUrl = buildManifestWithAudio(videoPlaylistUrl, audioFlowsForManifest, fl, allBlobUrls);

    try {
      if (!omakaseModule) {
        omakaseModule = await import('@byomakase/omakase-player');
      }
      if (!container.id) container.id = 'omakase-player-container';

      omakasePlayer = new omakaseModule.OmakasePlayer({
        playerHTMLElementId: container.id,
      });

      const audioOnly = fl?.format === FORMAT_AUDIO;
      subscriptions.push(omakasePlayer.loadVideo(manifestUrl, {
        protocol: audioOnly ? 'audio' : 'hls',
      }).subscribe({
        next: (video) => {
          addToast(`Loaded (${video.duration?.toFixed(1)}s)`, 'success');
          initTimeline(segs, video.duration);
          scheduleUrlRefresh();
          // VU meter requires user gesture for Web Audio API.
          // Init on first click/keydown after video loads.
          removeVuTriggerListeners();
          vuTriggerFn = () => {
            initVuMeter();
            removeVuTriggerListeners();
          };
          window.addEventListener('click', vuTriggerFn);
          window.addEventListener('keydown', vuTriggerFn);
        },
        error: (err) => {
          playerError = `Failed to load video: ${errorMessage(err)}`;
        },
      }));
    } catch (err) {
      playerError = `Failed to initialize player: ${errorMessage(err)}`;
      for (const url of allBlobUrls) revokeManifest(url);
      allBlobUrls.clear();
      manifestUrl = null;
    }
  }

  /**
   * Build a manifest URL from a video playlist, optionally wrapping it in a
   * master playlist that includes audio renditions.  Tracks all created blob
   * URLs in `blobUrlSet` for later revocation.
   */
  function buildManifestWithAudio(videoPlaylistUrl: string, audioFlows: Array<{ flow: any; segments: any[] }>, fl: any, blobUrlSet: Set<string>): string {
    const isVideoFlow = fl?.format === FORMAT_VIDEO;
    if (isVideoFlow && audioFlows.length > 0) {
      const audioTracks = [];
      for (const af of audioFlows) {
        const segs = af.segments;
        if (!segs?.length) continue;
        const audioUrl = buildM3u8BlobUrl(segs);
        if (audioUrl) {
          blobUrlSet.add(audioUrl);
          audioTracks.push({
            name: af.flow.label || formatShortName(af.flow.format),
            url: audioUrl,
          });
        }
      }
      if (audioTracks.length > 0) {
        const masterContent = buildMasterM3u8String(videoPlaylistUrl, audioTracks);
        const url = m3u8BlobUrl(masterContent);
        blobUrlSet.add(url);
        return url;
      }
    }
    return videoPlaylistUrl;
  }

  // ── Presigned URL Refresh ────────────────────────────────────

  function scheduleUrlRefresh(): void {
    if (urlRefreshTimer) clearInterval(urlRefreshTimer);
    urlRefreshTimer = setInterval(refreshPresignedUrls, URL_REFRESH_INTERVAL);
  }

  async function refreshPresignedUrls(): Promise<void> {
    if (refreshInProgress || !flow || !omakasePlayer) return;
    refreshInProgress = true;
    try {
      // Fetch primary + related segments in parallel
      const fids = Object.keys(relatedSegments);
      const [freshSegs, ...relatedResults] = await Promise.all([
        fetchSegments(flow.id),
        ...fids.map(fid => fetchSegments(fid).then(segs => ({ fid, segs })).catch(() => null)),
      ]);

      let freshRelated = relatedSegments;
      if (fids.length > 0) {
        const updated = { ...relatedSegments };
        for (const r of relatedResults) {
          if (r) updated[r.fid] = r.segs;
        }
        freshRelated = updated;
      }

      // Rebuild m3u8 manifests with new presigned URLs
      const newVideoUrl = buildM3u8BlobUrl(freshSegs);
      if (!newVideoUrl) {
        console.warn('[Player] URL refresh: no playable segments');
        return;
      }

      const newBlobUrls = new Set([newVideoUrl]);
      // Build copies with refreshed segments for manifest rebuild
      const refreshedAudioFlows = audioFlowsForManifest.map(af => ({
        ...af,
        segments: freshRelated[af.flow.id] || af.segments,
      }));
      const newManifestUrl = buildManifestWithAudio(newVideoUrl, refreshedAudioFlows, flow, newBlobUrls);

      // Save playback position, swap source, restore position
      const wasPlaying = omakasePlayer.video.isPlaying();
      const currentTime = omakasePlayer.video.getCurrentTime();

      // Revoke old blob URLs, swap in new ones
      for (const url of allBlobUrls) revokeManifest(url);
      allBlobUrls = newBlobUrls;
      manifestUrl = newManifestUrl;

      // Update state for UI
      segments = freshSegs;
      relatedSegments = freshRelated;

      // Reload the player with new manifest
      const audioOnly = flow?.format === FORMAT_AUDIO;
      subscriptions.push(omakasePlayer.loadVideo(newManifestUrl, {
        protocol: audioOnly ? 'audio' : 'hls',
      }).subscribe({
        next: () => {
          // Restore playback position
          if (currentTime > 0) {
            subscriptions.push(omakasePlayer.video.seekToTime(currentTime).subscribe({
              next: () => {
                if (wasPlaying) omakasePlayer.video.play();
              },
            }));
          } else if (wasPlaying) {
            omakasePlayer.video.play();
          }
          console.log('[Player] Presigned URLs refreshed, manifest reloaded');
        },
        error: (err) => {
          console.warn('[Player] Failed to reload after URL refresh:', err);
          addToast('Presigned URL refresh failed — playback may stop', 'warning');
        },
      }));
    } catch (err) {
      console.warn('URL refresh failed:', err);
      addToast('Presigned URL refresh failed — playback may stop', 'warning');
    } finally {
      refreshInProgress = false;
    }
  }

  // ── Timeline ──────────────────────────────────────────────────

  function initTimeline(segs: any[], videoDuration: number): void {
    if (!omakasePlayer) return;
    const el = document.getElementById('omakase-timeline');
    if (!el) return;

    try {
      subscriptions.push(omakasePlayer.createTimeline({
        timelineHTMLElementId: 'omakase-timeline',
        style: {
          stageMinWidth: 700,
          stageMinHeight: 200,
          backgroundFill: '#2b2b2b',
          backgroundOpacity: 1,
          headerBackgroundFill: '#222222',
          headerBackgroundOpacity: 1,
          headerHeight: 20,
          headerMarginBottom: 0,
          footerBackgroundFill: '#222222',
          footerBackgroundOpacity: 1,
          footerHeight: 8,
          footerMarginTop: 0,
          headerTextFill: '#cccccc',
          textFill: '#cccccc',
          leftPaneWidth: 150,
          playProgressBarHeight: 10,
          playheadVisible: true,
          playheadFill: '#5a9fd4',
          playheadLineWidth: 2,
          playheadSymbolHeight: 10,
          playheadBackgroundFill: '#5a9fd4',
          playheadBackgroundOpacity: 0.1,
          playheadTextFill: '#ffffff',
          playheadPlayProgressFill: '#5a9fd4',
          playheadPlayProgressOpacity: 0.2,
          playheadBufferedFill: '#5a9fd4',
          playheadBufferedOpacity: 0.08,
          scrubberFill: '#d4a05a',
          scrubberTextFill: '#ffffff',
          scrollbarHeight: 0,
          textFontFamily: 'system-ui, -apple-system, sans-serif',
          loadingAnimationTheme: 'dark',
        },
      }).subscribe({
        next: (timeline) => {
          currentTimeline = timeline;
          primaryStartOffset = segmentStartOffset(segs);

          // Style the scrubber lane — bright timecodes on dark background
          try {
            const scrubber = timeline.getScrubberLane();
            if (scrubber) {
              scrubber.style = {
                ...scrubber.style,
                backgroundFill: '#333333',
                leftBackgroundFill: '#2b2b2b',
                timecodeFill: '#e0e0e0',
                tickFill: '#888888',
                marginBottom: 2,
              };
            }
          } catch { /* ignore */ }

          // Primary flow segment markers
          addFlowLane(timeline, flow.id, flow, segs, true);
          // Segmentation editing lane
          addSegmentationLane(timeline, videoDuration);
          // Scrollbar lane at the bottom
          addScrollbarLane(timeline);
          // Signal ready AFTER timeline exists so auto-enable $effect
          // can add lanes without "Timeline not ready yet" errors
          playerReady = true;
        },
        error: (err) => {
          console.warn('Timeline creation failed:', err);
        },
      }));
    } catch (err) {
      console.warn('Timeline setup error:', err);
    }
  }

  function addFlowLane(timeline: any, flowId: string, flowData: any, segs: any[], isPrimary: boolean = false): void {
    if (!segs?.length || !omakaseModule) return;

    const startOffset = primaryStartOffset; // computed once in initTimeline
    const laneIdx = isPrimary ? 0 : (flowId.charCodeAt(0) + flowId.charCodeAt(flowId.length - 1)) % LANE_COLORS.length;
    const color = LANE_COLORS[laneIdx % LANE_COLORS.length];
    const fmt = formatShortName(flowData?.format || '');
    const codec = flowData?.codec || '';
    const label = isPrimary ? `${fmt} (primary)` : `${fmt} / ${codec}`;

    try {
      const lane = new omakaseModule.MarkerLane({
        id: isPrimary ? 'primary-lane' : `lane-${flowId}`,
        description: label,
        style: {
          backgroundFill: '#2e3340',
          leftBackgroundFill: '#252830',
          height: 30,
          descriptionTextFill: '#cccccc',
          marginBottom: 1,
        },
      });
      timeline.addTimelineLane(lane);

      // Map marker IDs to object IDs for single-subscription focus handler
      const markerToObject = new Map();

      for (let i = 0; i < segs.length; i++) {
        const seg = segs[i];
        const tr = parseTimerange(seg.timerange);
        if (tr.type === 'never' || !tr.start || !tr.end) continue;

        const start = Math.max(0, nanosToSeconds(tr.start.nanos) - startOffset);
        const end = nanosToSeconds(tr.end.nanos) - startOffset;
        const markerId = `${flowId}-seg-${i}`;

        try {
          lane.createPeriodMarker({
            id: markerId,
            timeObservation: { start, end },
            style: {
              color: LANE_COLORS[(laneIdx + i) % LANE_COLORS.length],
              renderType: 'lane',
              symbolType: 'none',
              selectedAreaOpacity: 0.6,
              lineOpacity: 0,
            },
          });
          if (seg.object_id) markerToObject.set(markerId, seg.object_id);
        } catch {
          // Markers outside video bounds — non-fatal
        }
      }

      // Single subscription for all markers in this lane
      if (markerToObject.size > 0) {
        subscriptions.push(lane.onMarkerFocus$.subscribe((evt) => {
          const objectId = markerToObject.get(evt?.marker?.id);
          if (objectId) highlightSegmentRow(objectId);
        }));
      }
    } catch (err) {
      console.warn(`Lane setup error for ${flowId}:`, err);
    }
  }

  let highlightTimer: ReturnType<typeof setTimeout> | null = null;
  let highlightedRow: HTMLElement | null = null;

  function highlightSegmentRow(objectId: string): void {
    if (highlightTimer) clearTimeout(highlightTimer);
    if (highlightedRow) { highlightedRow.classList.remove('highlight'); highlightedRow = null; }
    const row = document.querySelector(`[data-object-id="${objectId}"]`);
    if (row) {
      row.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      row.classList.add('highlight');
      highlightedRow = row;
      highlightTimer = setTimeout(() => { row.classList.remove('highlight'); highlightedRow = null; }, 1500);
    }
  }

  function addScrollbarLane(timeline: any): void {
    if (!omakaseModule) return;
    try {
      timeline.addTimelineLane(new omakaseModule.ScrollbarLane({
        description: '',
        style: {
          backgroundFill: '#252830',
          leftBackgroundFill: '#222222',
          height: 20,
        },
      }));
    } catch (err) {
      console.warn('Scrollbar lane error:', err);
    }
  }

  // ── Segmentation Editing (8d) ─────────────────────────────────

  function addSegmentationLane(timeline: any, videoDuration: number): void {
    if (!omakaseModule) return;
    try {
      segmentationLane = new omakaseModule.MarkerLane({
        id: 'segmentation-lane',
        description: 'Edit markers',
        style: {
          backgroundFill: '#332830',
          leftBackgroundFill: '#2a2228',
          height: 30,
          descriptionTextFill: '#cccccc',
          marginBottom: 1,
        },
      });
      timeline.addTimelineLane(segmentationLane);

      // Single subscription for all segmentation marker updates (mutate in place
      // to avoid allocating a new array on every drag frame)
      subscriptions.push(segmentationLane.onMarkerUpdate$.subscribe((evt) => {
        const mid = evt?.marker?.id;
        if (!mid) return;
        const idx = segmentationMarkers.findIndex(m => m.id === mid);
        if (idx !== -1) {
          const obs = evt.marker.timeObservation;
          segmentationMarkers[idx].start = obs.start;
          segmentationMarkers[idx].end = obs.end;
        }
      }));
    } catch (err) {
      console.warn('Segmentation lane error:', err);
    }
  }

  function addSegmentationMarker(): void {
    if (!segmentationLane || !omakasePlayer?.video) return;
    const currentTime = omakasePlayer.video.getCurrentTime();
    const duration = omakasePlayer.video.getDuration();
    const id = `edit-${nextMarkerId++}`;
    const start = currentTime;
    const end = Math.min(currentTime + 2, duration); // 2s default width

    try {
      segmentationLane.createPeriodMarker({
        id,
        editable: true,
        timeObservation: { start, end },
        style: {
          color: '#e05050',
          renderType: 'lane',
          symbolType: 'triangle',
          selectedAreaOpacity: 0.6,
        },
      });
      segmentationMarkers = [...segmentationMarkers, { id, start, end, label: `Marker ${segmentationMarkers.length + 1}` }];
    } catch (err) {
      addToast(`Failed to add marker: ${errorMessage(err)}`, 'error');
    }
  }

  function removeSegmentationMarker(id: string): void {
    if (!segmentationLane) return;
    try { segmentationLane.removeMarker(id); } catch { /* ignore */ }
    segmentationMarkers = segmentationMarkers.filter(m => m.id !== id);
  }

  function clearSegmentationMarkers(): void {
    if (!segmentationLane) return;
    try { segmentationLane.removeAllMarkers(); } catch { /* ignore */ }
    segmentationMarkers = [];
  }

  // ── Export (8e) ───────────────────────────────────────────────

  async function exportSegments(): Promise<void> {
    if (!segmentationMarkers.length || !flow) return;
    exporting = true;
    exportError = null;

    try {
      const startOffset = primaryStartOffset;

      // Create a new flow as a child of this flow's source
      const newFlowId = crypto.randomUUID();
      const flowBody = {
        source_id: flow.source_id,
        format: flow.format,
        codec: flow.codec,
        container: flow.container,
        label: `${flow.label || 'Flow'} — edit export`,
        description: `Exported from ${flow.id} using ${segmentationMarkers.length} marker(s)`,
      };
      if (flow.essence_parameters) flowBody.essence_parameters = flow.essence_parameters;
      await apiPut(`/flows/${newFlowId}`, flowBody);

      // Collect all overlapping segments across all markers, then register in parallel
      const toRegister = [];
      const registeredIds = new Set();
      for (const marker of segmentationMarkers) {
        for (const seg of findOverlappingSegments(segments, marker.start, marker.end, startOffset)) {
          const key = `${seg.object_id}:${seg.timerange}`;
          if (registeredIds.has(key)) continue;
          registeredIds.add(key);
          toRegister.push({ object_id: seg.object_id, timerange: seg.timerange });
        }
      }

      const results = await Promise.allSettled(
        toRegister.map(s => apiPost(`/flows/${newFlowId}/segments`, s))
      );
      const registered = results.filter(r => r.status === 'fulfilled').length;
      const failed = results.length - registered;

      if (failed > 0) {
        addToast(`Exported ${registered} segment(s), ${failed} failed`, 'warning');
      } else {
        addToast(`Exported ${registered} segment(s) to new flow`, 'success');
      }
      // Tag as edit export
      try { await apiPut(`/flows/${newFlowId}/tags/edit_export`, ['true']); } catch { /* non-critical */ }
      showExport = false;
      // Navigate to new flow
      push(`/flows/${newFlowId}`);
    } catch (e) {
      exportError = errorMessage(e);
    } finally {
      exporting = false;
    }
  }

</script>

<div class="page">
  {#if loading}
    <div style="padding:1.5em;display:flex;align-items:center;gap:0.5em">
      <Spinner size="1.2em" /> Loading flow...
    </div>
  {:else if error}
    <div style="padding:1.5em">
      <p class="error-text">{error}</p>
      <button onclick={() => loadFlow(params.id)}>Retry</button>
    </div>
  {:else if flow}
    <div class="page-header">
      <div>
        <h1>{flow.label || 'Player'}</h1>
        <span class="muted" style="font-size:0.85em">
          {formatShortName(flow.format)} / {flow.codec || '--'}
          {#if segments.length}
            &mdash; {segments.length} segment{segments.length !== 1 ? 's' : ''}
          {/if}
        </span>
      </div>
      <div style="display:flex;gap:0.5em">
        <a href="#/flows/{flow.id}" class="btn">Flow Details</a>
      </div>
    </div>

    {#if !canPlay}
      <div class="panel" style="margin:1em 1.5em">
        <p class="muted">
          {#if segments.length === 0}
            No segments found for this flow.
          {:else}
            This flow format ({formatShortName(flow.format)}) is not playable.
          {/if}
        </p>
      </div>
    {:else}
      {#if playerError}
        <div style="padding:0 1.5em">
          <p class="error-text" style="font-size:0.85em">{playerError}</p>
        </div>
      {/if}

      <!-- Player + VU Meter -->
      <div class="player-wrapper">
        <div
          id="omakase-player-container"
          class="player-container"
          use:omakaseAction={{ segments, flow }}
        ></div>
        <div id="vu-meter-container" class="vu-meter-container"></div>
      </div>

      <!-- Timeline -->
      <div class="timeline-wrapper">
        <div id="omakase-timeline" class="timeline-container"></div>
      </div>

      <!-- Controls + Panels below timeline -->
      {#if playerReady}
        <div class="controls-bar">
          <button class="btn-small" onclick={addSegmentationMarker}>+ Marker</button>
          {#if segmentationMarkers.length > 0}
            <button class="btn-small" onclick={clearSegmentationMarkers}>Clear</button>
            <button class="btn-small primary" onclick={() => {
              const errors = validateExport(segmentationMarkers, primaryStartOffset, segments);
              exportError = errors.length ? errors.join('; ') : null;
              showExport = true;
            }}>
              Export ({segmentationMarkers.length})
            </button>
          {/if}
        </div>
      {/if}

      <!-- Two-column layout for panels -->
      <div class="panels-row">
        <!-- Left: Markers + Related Flows -->
        <div class="panels-left">
          {#if segmentationMarkers.length > 0}
            <div class="panel compact-panel marker-list">
              <div class="panel-header">
                <h4>Edit Markers</h4>
              </div>
              {#if overlappingIds.size > 0}
                <p class="overlap-warning">Overlapping markers — may duplicate segments in export.</p>
              {/if}
              <table class="compact-table">
                <thead>
                  <tr><th>Label</th><th>In</th><th>Out</th><th>Dur</th><th></th></tr>
                </thead>
                <tbody>
                  {#each segmentationMarkers as m}
                    <tr class:overlap={overlappingIds.has(m.id)}>
                      <td>{m.label}</td>
                      <td class="mono">{formatSeconds(m.start)}</td>
                      <td class="mono">{formatSeconds(m.end)}</td>
                      <td class="mono">{(m.end - m.start).toFixed(1)}s</td>
                      <td><button class="btn-tiny danger" onclick={() => removeSegmentationMarker(m.id)}>x</button></td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}

          {#if relatedFlows.length > 0}
            <div class="panel compact-panel">
              <h4>Related Flows</h4>
              <div class="flow-selector">
                {#each relatedFlows as rf}
                  <label class="flow-check">
                    <input
                      type="checkbox"
                      checked={selectedLanes.has(rf.id)}
                      onchange={() => toggleLane(rf.id)}
                    />
                    <span class="mono">{rf.id.slice(0, 8)}</span>
                    <span class="muted">{formatShortName(rf.format)}</span>
                    {#if rf.label}<span>{rf.label}</span>{/if}
                  </label>
                {/each}
              </div>
            </div>
          {/if}
        </div>

        <!-- Right: Segment list (scrollable) -->
        <div class="panels-right">
          <div class="panel compact-panel segment-list">
            <h4>Segments <span class="muted">({segments.length})</span></h4>
            <div class="segment-scroll">
              <table class="compact-table">
                <thead>
                  <tr>
                    <th>Object</th>
                    <th>Timerange</th>
                    <th>Dur</th>
                  </tr>
                </thead>
                <tbody>
                  {#each segments as seg, i}
                    <tr data-object-id={seg.object_id}>
                      <td class="mono">
                        <a href="#/media/{seg.object_id}">{seg.object_id?.slice(0, 8) || '--'}</a>
                      </td>
                      <td class="mono">{seg.timerange || '--'}</td>
                      <td class="mono">{segmentDuration(seg.timerange).toFixed(1)}s</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>

      <!-- Export Modal (8e) -->
      {#if exportError}
        <p class="error-text" style="font-size:0.85em;padding:0 1.5em">{exportError}</p>
      {/if}
      <ConfirmDialog
        open={showExport}
        title="Export Segments"
        message={`Create a new flow containing the segments covered by ${segmentationMarkers.length} marker(s). Segments that overlap with any marker will be registered in the new flow.`}
        confirmLabel="Export"
        danger={false}
        loading={exporting}
        onConfirm={exportSegments}
        onCancel={() => { showExport = false; exportError = null; }}
      />
    {/if}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: 0.75em 1.5em;
    gap: 1em;
    flex-wrap: wrap;
  }
  .page-header h1 {
    margin: 0 0 0.1em 0;
    font-size: 1.2em;
  }
  .btn {
    display: inline-block;
    padding: 0.35em 0.7em;
    background: var(--panel);
    border: 1px solid var(--border);
    color: var(--text);
    text-decoration: none;
    border-radius: 3px;
    font-size: 0.8em;
  }
  .btn:hover {
    background: var(--border);
  }
  .btn-small {
    display: inline-block;
    padding: 0.25em 0.55em;
    background: var(--panel);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 3px;
    font-size: 0.75em;
    cursor: pointer;
  }
  .btn-small:hover {
    background: var(--border);
  }
  .btn-small.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn-small.primary:hover {
    opacity: 0.85;
  }
  .btn-tiny {
    display: inline-block;
    padding: 0.1em 0.35em;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-muted);
    border-radius: 2px;
    font-size: 0.7em;
    cursor: pointer;
    line-height: 1;
  }
  .btn-tiny:hover {
    border-color: var(--text-muted);
    color: var(--text);
  }
  .btn-tiny.danger:hover {
    border-color: var(--error);
    color: var(--error);
  }
  .player-wrapper {
    padding: 0 1.5em;
    display: flex;
    gap: 0;
    align-items: stretch;
  }
  .player-container {
    flex: 1 1 auto;
    min-height: 400px;
    background: #1a1a1a;
    border: 1px solid var(--border);
    border-radius: 3px 0 0 3px;
  }
  .vu-meter-container {
    flex: 0 0 160px;
    height: 400px;
    background: #2b2b2b;
    border: 1px solid var(--border);
    border-left: none;
    border-radius: 0 3px 3px 0;
  }
  .timeline-wrapper {
    padding: 0.4em 1.5em;
  }
  .timeline-container {
    width: 100%;
    min-height: 120px;
    background: #333333;
    border: 1px solid var(--border);
    border-radius: 3px;
  }
  .controls-bar {
    display: flex;
    align-items: center;
    gap: 0.4em;
    padding: 0.25em 1.5em 0.4em;
    flex-wrap: wrap;
  }

  /* Two-column layout below controls */
  .panels-row {
    display: flex;
    gap: 0.75em;
    padding: 0 1.5em 1em;
    align-items: flex-start;
  }
  .panels-left {
    flex: 0 0 340px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  .panels-right {
    flex: 1 1 auto;
    min-width: 0;
  }

  /* Compact panels */
  .compact-panel {
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--panel);
    padding: 0.5em 0.6em;
  }
  .compact-panel h4 {
    margin: 0 0 0.35em 0;
    font-size: 0.8em;
    font-weight: 600;
    color: var(--text);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .panel-header h4 {
    margin-bottom: 0;
  }

  /* Compact tables */
  .compact-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.75em;
  }
  .compact-table th {
    text-align: left;
    color: #bbb;
    font-weight: 500;
    padding: 0.2em 0.4em;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .compact-table td {
    padding: 0.2em 0.4em;
    border-bottom: 1px solid rgba(68, 68, 68, 0.4);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }

  /* Scrollable segment list */
  .segment-scroll {
    max-height: 280px;
    overflow-y: auto;
  }
  .segment-scroll::-webkit-scrollbar {
    width: 5px;
  }
  .segment-scroll::-webkit-scrollbar-track {
    background: var(--panel);
  }
  .segment-scroll::-webkit-scrollbar-thumb {
    background: var(--border);
    border-radius: 3px;
  }

  .flow-selector {
    display: flex;
    flex-direction: column;
    gap: 0.25em;
  }
  .flow-check {
    display: flex;
    align-items: center;
    gap: 0.4em;
    font-size: 0.75em;
    color: #ccc;
    cursor: pointer;
  }
  .flow-check:hover {
    color: var(--text);
  }
  .overlap-warning {
    color: var(--warning);
    font-size: 0.75em;
    margin: 0 0 0.3em;
  }
  .overlap td {
    color: var(--warning);
  }
  :global(tr[data-object-id].highlight td) {
    background: rgba(90, 159, 212, 0.15);
    transition: background 0.3s;
  }

  /* Responsive: stack on narrow viewports */
  @media (max-width: 800px) {
    .panels-row {
      flex-direction: column;
    }
    .panels-left {
      flex: none;
      width: 100%;
    }
  }
</style>
