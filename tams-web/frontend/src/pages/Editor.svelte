<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { apiGet, apiPost, apiPut, formatShortName } from '../lib/api.js';
  import { buildFlowsQuery, buildFlowQuery, buildSegmentsQuery } from '../lib/query.js';
  import { push, getHashParams } from '../lib/router.js';
  import { addToast } from '../lib/toast.js';
  import { errorMessage } from '../lib/errors.js';
  import { parseTimerange, nanosToSeconds } from '../lib/timerange.js';
  import { buildM3u8BlobUrl, revokeManifest, segmentDuration } from '../lib/hls.js';
  import { FORMAT_VIDEO, fetchAllSegments, createFlowWithSource } from '../lib/ingest.js';
  import { buildTimerangeFromNanos } from '../lib/rational.js';
  import { extractThumbnail, clearThumbnailCache, enableThumbnailCache } from '../lib/thumbnail.js';
  import { parsePagination } from '../lib/api.js';
  import Spinner from '../components/Spinner.svelte';
  import '@byomakase/omakase-player/dist/style.css';

  import type { Segment } from '../types/tams.js';

  // ── Types ────────────────────────────────────────────────────────────────

  interface ClipEntry {
    id: string;
    flowId: string;
    flowLabel: string;
    segments: Segment[];
    duration: number; // seconds (sum of segment durations)
  }

  // ── State ────────────────────────────────────────────────────────────────

  // Bin – all flows
  let allFlows: any[] = $state([]);
  let binLoading: boolean = $state(false);
  let binNextKey: string | null = null;
  let binHasMore: boolean = $state(false);
  let binLoadingMore: boolean = $state(false);
  let binSearch: string = $state('');
  let binThumbnails: Record<string, string> = $state({});
  let binThumbFailed: Record<string, boolean> = $state({});

  let filteredBinFlows: any[] = $derived.by(() => {
    if (!binSearch.trim()) return allFlows;
    const q = binSearch.trim().toLowerCase();
    return allFlows.filter(f => (f.label || '').toLowerCase().includes(q) || f.id.toLowerCase().includes(q));
  });

  // Source monitor
  let activeFlow: any = $state(null);
  let activeSegments: Segment[] = $state([]);
  let sourceLoading: boolean = $state(false);
  let sourceError: string | null = $state(null);
  let sourcePlayerReady: boolean = $state(false);
  let sourceCurrentTime: number = $state(0);
  // Cumulative video times for each segment (index = segment index)
  let segVideoTimes: number[] = $state([]);
  let inSegIdx: number | null = $state(null);
  let outSegIdx: number | null = $state(null);

  // Internal player handles
  let sourcePlayer: any = null;
  let sourceModule: any = null;
  let sourceSubs: any[] = [];
  let sourceBlobUrls: Set<string> = new Set();

  // Program monitor
  let programPlayerReady: boolean = $state(false);
  let programBuilding: boolean = $state(false);
  let programCurrentTime: number = $state(0);

  let programPlayer: any = null;
  let programSubs: any[] = [];
  let programBlobUrls: Set<string> = new Set();

  // Timeline playhead
  let timelineScrollEl: HTMLElement | null = $state(null);
  let playheadDragging: boolean = false;

  // Timeline
  let timeline: ClipEntry[] = $state([]);
  let dragSrcIdx: number | null = null;
  let totalDuration: number = $derived(timeline.reduce((s, c) => s + c.duration, 0));

  /** Total pixel width of all clips + gaps (gap=4px) */
  let totalTrackPx: number = $derived.by(() =>
    timeline.reduce((s: number, c: ClipEntry, i: number) =>
      s + Math.max(80, Math.min(300, c.duration * 8)) + (i > 0 ? 4 : 0), 0)
  );

  /** Playhead X: non-linear, accounts for clamped clip widths */
  let playheadX: number = $derived(timeToPixel(programCurrentTime));

  /** Ruler tick marks */
  let rulerTicks: Array<{px: number, label: string}> = $derived.by(() => {
    if (!totalDuration || !totalTrackPx) return [];
    const candidates = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600];
    let interval = candidates[0];
    for (const c of candidates) {
      interval = c;
      if ((c / totalDuration) * totalTrackPx >= 80) break;
    }
    const ticks: Array<{px: number, label: string}> = [];
    for (let t = 0; t <= totalDuration + 0.001; t += interval) {
      const clamped = Math.min(t, totalDuration);
      ticks.push({ px: timeToPixel(clamped), label: formatSecs(clamped) });
      if (clamped >= totalDuration) break;
    }
    return ticks;
  });

  // Export
  let exporting: boolean = $state(false);
  let exportLabel: string = $state('');

  // Focus tracking for keyboard shortcuts
  type PanelId = 'bin' | 'source' | 'program' | 'timeline';
  let focusedPanel: PanelId | null = $state(null);

  function panelBlur(e: FocusEvent): void {
    const curr = e.currentTarget as HTMLElement;
    const next = e.relatedTarget as Node | null;
    if (!next || !curr.contains(next)) focusedPanel = null;
  }

  // ── Helpers ──────────────────────────────────────────────────────────────

  function formatSecs(s: number): string {
    if (!s || s < 0) return '0:00';
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = Math.floor(s % 60);
    if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(sec).padStart(2, '0')}`;
    return `${m}:${String(sec).padStart(2, '0')}`;
  }

  /** Build cumulative segment video-start times (seconds) from segment list. */
  function buildSegVideoTimes(segs: Segment[]): number[] {
    const times: number[] = [];
    let t = 0;
    for (const seg of segs) {
      times.push(t);
      t += segmentDuration(seg.timerange);
    }
    return times;
  }

  /** Find the segment index that the given video time falls in. */
  function segIdxAtTime(time: number, times: number[], segs: Segment[]): number {
    let idx = 0;
    for (let i = 0; i < times.length; i++) {
      const dur = segmentDuration(segs[i].timerange);
      if (time >= times[i] && time < times[i] + dur) { idx = i; break; }
      if (i === times.length - 1) idx = i;
    }
    return idx;
  }

  /** Compute sum of segment durations for an array of segments. */
  function sumDuration(segs: Segment[]): number {
    return segs.reduce((s, seg) => s + segmentDuration(seg.timerange), 0);
  }

  // ── Bin ──────────────────────────────────────────────────────────────────

  async function loadBinFlows(): Promise<void> {
    binLoading = true;
    try {
      const resp = await apiGet(buildFlowsQuery({ format: FORMAT_VIDEO, limit: 30, includeTimerange: true }));
      allFlows = resp.data || [];
      const pag = parsePagination(resp.headers);
      binNextKey = pag.nextKey;
      binHasMore = !!binNextKey;
    } catch { /* ignore */ } finally {
      binLoading = false;
    }
  }

  async function loadMoreBinFlows(): Promise<void> {
    if (binLoadingMore || !binHasMore || !binNextKey) return;
    binLoadingMore = true;
    try {
      const resp = await apiGet(buildFlowsQuery({ format: FORMAT_VIDEO, limit: 30, includeTimerange: true }, binNextKey));
      const newFlows: any[] = resp.data || [];
      const existing = new Set(allFlows.map((f: any) => f.id));
      allFlows = [...allFlows, ...newFlows.filter(f => !existing.has(f.id))];
      const pag = parsePagination(resp.headers);
      binNextKey = pag.nextKey;
      binHasMore = !!binNextKey && newFlows.length >= 30;
    } catch { /* ignore */ } finally {
      binLoadingMore = false;
    }
  }

  async function loadBinThumbnail(flow: any, signal: AbortSignal): Promise<void> {
    if (binThumbnails[flow.id] || binThumbFailed[flow.id]) return;
    try {
      const resp = await apiGet(buildSegmentsQuery(flow.id, { limit: 3, presigned: true }));
      if (signal?.aborted) return;
      const segs: any[] = resp.data || [];
      if (!segs.length) { binThumbFailed = { ...binThumbFailed, [flow.id]: true }; return; }
      const url = await extractThumbnail({ key: flow.id, segments: segs, flow, width: 320, signal });
      if (signal?.aborted) return;
      if (url) binThumbnails = { ...binThumbnails, [flow.id]: url };
      else binThumbFailed = { ...binThumbFailed, [flow.id]: true };
    } catch (e) {
      if (!signal?.aborted) binThumbFailed = { ...binThumbFailed, [flow.id]: true };
    }
  }

  /** Trigger thumbnail load for a flowId if not yet cached (used in timeline) */
  function ensureThumb(flowId: string): void {
    if (binThumbnails[flowId] || binThumbFailed[flowId]) return;
    const flow = allFlows.find((f: any) => f.id === flowId);
    if (flow) loadBinThumbnail(flow, new AbortController().signal);
  }

  function binLazyThumb(node: HTMLElement, flow: any): void | { destroy(): void } {
    let ctrl = new AbortController();
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && !binThumbnails[flow.id] && !binThumbFailed[flow.id]) {
        loadBinThumbnail(flow, ctrl.signal);
      } else if (!entry.isIntersecting) {
        ctrl.abort(); ctrl = new AbortController();
      }
    }, { rootMargin: '100px' });
    observer.observe(node);
    return { destroy() { observer.disconnect(); ctrl.abort(); } };
  }

  function binInfiniteScroll(node: HTMLElement): { destroy(): void } {
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) loadMoreBinFlows();
    }, { rootMargin: '200px' });
    observer.observe(node);
    return { destroy() { observer.disconnect(); } };
  }

  function addFlowToBin(_flow: any): void { /* no-op – all flows are always shown */ }

  function removeFlowFromBin(_flowId: string): void { /* no-op */ }


  async function selectBinFlow(flow: any): Promise<void> {
    if (activeFlow?.id === flow.id) return;
    destroySourcePlayer();
    inSegIdx = null;
    outSegIdx = null;
    activeFlow = flow;
    activeSegments = [];
    sourceLoading = true;
    sourceError = null;
    sourcePlayerReady = false;
    try {
      const segs = await fetchAllSegments(flow.id, { presigned: true });
      activeSegments = segs;
      segVideoTimes = buildSegVideoTimes(segs);
    } catch (e) {
      sourceError = errorMessage(e);
    } finally {
      sourceLoading = false;
    }
  }

  // ── Source Player ────────────────────────────────────────────────────────

  function destroySourcePlayer(): void {
    for (const s of sourceSubs) { try { s.unsubscribe(); } catch { /* */ } }
    sourceSubs = [];
    if (sourcePlayer) { try { sourcePlayer.destroy(); } catch { /* */ } sourcePlayer = null; }
    for (const u of sourceBlobUrls) revokeManifest(u);
    sourceBlobUrls.clear();
    sourcePlayerReady = false;
    sourceCurrentTime = 0;
  }

  async function initSourcePlayer(container: HTMLElement): Promise<void> {
    if (!activeSegments.length) return;
    const manifestUrl = buildM3u8BlobUrl(activeSegments);
    if (!manifestUrl) { sourceError = 'No playable segments (missing presigned URLs)'; return; }
    sourceBlobUrls.add(manifestUrl);

    try {
      if (!sourceModule) sourceModule = await import('@byomakase/omakase-player');
      container.id = 'omakase-source-monitor';

      sourcePlayer = new sourceModule.OmakasePlayer({ playerHTMLElementId: container.id });
      sourceSubs.push(sourcePlayer.loadVideo(manifestUrl, { protocol: 'hls' }).subscribe({
        next: () => {
          sourcePlayerReady = true;
          // Track current time via observable
          try {
            sourceSubs.push(sourcePlayer.video.onVideoTimeChange$.subscribe((evt: any) => {
              sourceCurrentTime = evt?.currentTime ?? 0;
            }));

          } catch { /* observable may not be available */ }
        },
        error: (err: any) => {
          sourceError = `Player error: ${errorMessage(err)}`;
        },
      }));
    } catch (err) {
      sourceError = `Init failed: ${errorMessage(err)}`;
      for (const u of sourceBlobUrls) revokeManifest(u);
      sourceBlobUrls.clear();
    }
  }

  function sourcePlayerAction(node: HTMLElement, segs: Segment[]): { update(s: Segment[]): void; destroy(): void } {
    if (segs.length) initSourcePlayer(node);
    return {
      update(newSegs: Segment[]) {
        destroySourcePlayer();
        if (newSegs.length) initSourcePlayer(node);
      },
      destroy() { destroySourcePlayer(); },
    };
  }

  // ── Mark In / Out ────────────────────────────────────────────────────────

  function markIn(): void {
    if (!activeSegments.length || !segVideoTimes.length) return;
    const idx = segIdxAtTime(sourceCurrentTime, segVideoTimes, activeSegments);
    inSegIdx = idx;
    if (outSegIdx !== null && outSegIdx < idx) outSegIdx = idx;
    addToast(`Mark In: segment ${idx + 1}/${activeSegments.length}`, 'info');
  }

  function markOut(): void {
    if (!activeSegments.length || !segVideoTimes.length) return;
    const idx = segIdxAtTime(sourceCurrentTime, segVideoTimes, activeSegments);
    outSegIdx = idx;
    if (inSegIdx !== null && inSegIdx > idx) inSegIdx = idx;
    addToast(`Mark Out: segment ${idx + 1}/${activeSegments.length}`, 'info');
  }

  function addToTimeline(): void {
    if (!activeFlow || !activeSegments.length) return;
    const from = inSegIdx ?? 0;
    const to = outSegIdx ?? activeSegments.length - 1;
    if (from > to) { addToast('In point must be before Out point', 'error'); return; }
    const clipSegs = activeSegments.slice(from, to + 1);
    const dur = sumDuration(clipSegs);
    const clip: ClipEntry = {
      id: crypto.randomUUID(),
      flowId: activeFlow.id,
      flowLabel: activeFlow.label || activeFlow.id.slice(0, 8),
      segments: clipSegs,
      duration: dur,
    };
    timeline = [...timeline, clip];
    addToast(`Added clip (${formatSecs(dur)}) to timeline`, 'success');
  }

  function removeClip(clipId: string): void {
    timeline = timeline.filter(c => c.id !== clipId);
  }

  // ── Timeline Drag & Drop ─────────────────────────────────────────────────

  function onClipDragStart(e: DragEvent, idx: number): void {
    dragSrcIdx = idx;
    e.dataTransfer!.effectAllowed = 'move';
  }

  function onClipDragOver(e: DragEvent): void {
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'move';
  }

  function onClipDrop(e: DragEvent, targetIdx: number): void {
    e.preventDefault();
    if (dragSrcIdx === null || dragSrcIdx === targetIdx) return;
    const items = [...timeline];
    const [moved] = items.splice(dragSrcIdx, 1);
    items.splice(targetIdx, 0, moved);
    timeline = items;
    dragSrcIdx = null;
  }

  // ── Program Monitor ──────────────────────────────────────────────────────

  function destroyProgramPlayer(): void {
    for (const s of programSubs) { try { s.unsubscribe(); } catch { /* */ } }
    programSubs = [];
    if (programPlayer) { try { programPlayer.destroy(); } catch { /* */ } programPlayer = null; }
    for (const u of programBlobUrls) revokeManifest(u);
    programBlobUrls.clear();
    programPlayerReady = false;
    programCurrentTime = 0;
  }

  async function buildProgramPreview(): Promise<void> {
    if (!timeline.length) { addToast('Timeline is empty', 'error'); return; }
    destroyProgramPlayer();
    programBuilding = true;

    const container = document.getElementById('omakase-program-monitor');
    if (!container) { programBuilding = false; return; }

    // Flat segment list from all clips in order
    const allSegs: Segment[] = timeline.flatMap(c => c.segments);
    const manifestUrl = buildM3u8BlobUrl(allSegs);
    if (!manifestUrl) {
      addToast('No playable segments in timeline (missing presigned URLs)', 'error');
      programBuilding = false;
      return;
    }
    programBlobUrls.add(manifestUrl);

    try {
      if (!sourceModule) sourceModule = await import('@byomakase/omakase-player');
      // Program player shares module with source player

      programPlayer = new sourceModule.OmakasePlayer({ playerHTMLElementId: 'omakase-program-monitor' });

      // Subscribe to time changes immediately (before loadVideo) so playback updates are captured
      programSubs.push(programPlayer.video.onVideoTimeChange$.subscribe({
        next: (ev: any) => {
          programCurrentTime = ev.currentTime;
          autoScrollPlayhead();
        }
      }));


      programSubs.push(programPlayer.loadVideo(manifestUrl, { protocol: 'hls' }).subscribe({
        next: () => { programPlayerReady = true; },
        error: (err: any) => { addToast(`Program player error: ${errorMessage(err)}`, 'error'); },
      }));
    } catch (err) {
      addToast(`Program player init failed: ${errorMessage(err)}`, 'error');
      for (const u of programBlobUrls) revokeManifest(u);
      programBlobUrls.clear();
    } finally {
      programBuilding = false;
    }
  }

  // ── Export ───────────────────────────────────────────────────────────────

  async function exportRoughCut(): Promise<void> {
    if (!timeline.length) { addToast('Timeline is empty', 'error'); return; }
    exporting = true;
    const label = exportLabel.trim() || `Rough Cut ${new Date().toISOString().slice(0, 16).replace('T', ' ')}`;
    try {
      const videoFlowId = crypto.randomUUID();
      const videoSourceId = crypto.randomUUID();

      // Pick codec/container from first clip's source flow
      const templateFlowId = timeline[0].flowId;
      let templateFlow: any = null;
      try {
        const resp = await apiGet(buildFlowQuery(templateFlowId, {}));
        templateFlow = resp.data;
      } catch { /* use defaults */ }

      await createFlowWithSource({
        sourceId: videoSourceId,
        flowId: videoFlowId,
        format: FORMAT_VIDEO,
        codec: templateFlow?.codec,
        container: templateFlow?.container,
        essenceParameters: templateFlow?.essence_parameters,
        label,
        sourceLabel: label,
        sourceDescription: `Rough cut from ${timeline.length} clip(s)`,
      });

      await apiPut(`/flows/${videoFlowId}/tags/edit_export`, ['true']);
      await apiPut(`/flows/${videoFlowId}/tags/rough_cut`, ['true']);

      // Register segments with re-based contiguous timeranges starting at 0
      const NANOS = 1_000_000_000n;
      let offsetNanos = 0n;
      let failed = 0;

      for (const clip of timeline) {
        for (const seg of clip.segments) {
          const tr = parseTimerange(seg.timerange);
          let durNanos: bigint;
          if (tr.type !== 'never' && tr.start && tr.end) {
            durNanos = tr.end.nanos - tr.start.nanos;
          } else {
            durNanos = BigInt(Math.round(segmentDuration(seg.timerange) * Number(NANOS)));
          }
          const newTimerange = buildTimerangeFromNanos(offsetNanos, offsetNanos + durNanos);
          try {
            await apiPost(`/flows/${videoFlowId}/segments`, {
              object_id: seg.object_id,
              timerange: newTimerange,
            });
            offsetNanos += durNanos;
          } catch (err) {
            console.warn('[export] segment failed:', err);
            failed++;
            offsetNanos += durNanos;
          }
        }
      }

      const msg = `Rough cut exported: ${timeline.reduce((s, c) => s + c.segments.length, 0)} segments` +
        (failed ? ` (${failed} failed)` : '');
      addToast(msg, failed ? 'warning' : 'success');
      push(`/player/${videoFlowId}`);
    } catch (err) {
      addToast(`Export failed: ${errorMessage(err)}`, 'error');
    } finally {
      exporting = false;
    }
  }

  // ── Lifecycle ────────────────────────────────────────────────────────────

  onMount(() => {
    enableThumbnailCache();
    loadBinFlows();

    // If launched from Gallery with a flowId param, load it into the source monitor
    const params = getHashParams();
    const flowId = params.get('flowId');
    if (flowId) {
      apiGet(buildFlowQuery(flowId, { includeTimerange: true }))
        .then((resp: any) => {
          const flow = resp.data;
          if (flow) selectBinFlow(flow);
        })
        .catch(() => { /* ignore */ });
    }
  });

  onDestroy(() => {
    destroySourcePlayer();
    destroyProgramPlayer();
    clearThumbnailCache();
  });

  // Derived: current segment index under playhead
  let currentSegIdx: number = $derived.by(() => {
    if (!activeSegments.length || !segVideoTimes.length) return 0;
    return segIdxAtTime(sourceCurrentTime, segVideoTimes, activeSegments);
  });

  // Labels for in/out points
  let inLabel: string = $derived(inSegIdx !== null ? `Seg ${inSegIdx + 1}` : '--');
  let outLabel: string = $derived(outSegIdx !== null ? `Seg ${outSegIdx + 1}` : '--');
  let clipDuration: number = $derived.by(() => {
    if (!activeSegments.length) return 0;
    const from = inSegIdx ?? 0;
    const to = outSegIdx ?? activeSegments.length - 1;
    if (from > to) return 0;
    return sumDuration(activeSegments.slice(from, to + 1));
  });

  // Auto-rebuild program preview whenever timeline changes (debounced 300ms)
  let _previewDebounce: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const t = timeline; // track reactively
    if (_previewDebounce) clearTimeout(_previewDebounce);
    if (!t.length) { destroyProgramPlayer(); return; }
    _previewDebounce = setTimeout(() => buildProgramPreview(), 300);
  });

  // Keyboard shortcuts
  function autoScrollPlayhead(): void {
    if (!timelineScrollEl) return;
    const el = timelineScrollEl;
    const x = playheadX;
    const margin = 80;
    const viewportX = x - el.scrollLeft;
    if (viewportX < margin) {
      el.scrollLeft = Math.max(0, x - margin);
    } else if (viewportX > el.clientWidth - margin) {
      el.scrollLeft = x - (el.clientWidth - margin);
    }
  }

  function onPlayheadMouseDown(e: MouseEvent): void {
    e.preventDefault();
    e.stopPropagation();
    playheadDragging = true;
    window.addEventListener('mousemove', onPlayheadMouseMove);
    window.addEventListener('mouseup', onPlayheadMouseUp, { once: true });
  }

  function onPlayheadMouseMove(e: MouseEvent): void {
    if (!playheadDragging) return;
    seekProgramToX(e.clientX);
  }

  function onPlayheadMouseUp(): void {
    playheadDragging = false;
    window.removeEventListener('mousemove', onPlayheadMouseMove);
  }

  function onTrackClick(e: MouseEvent): void {
    const target = e.target as HTMLElement;
    if (target.closest('.timeline-clip') || target.closest('.playhead')) return;
    seekProgramToX(e.clientX);
  }

  function seekProgramToX(clientX: number): void {
    if (!timelineScrollEl || !totalDuration) return;
    const rect = timelineScrollEl.getBoundingClientRect();
    const contentX = clientX - rect.left + timelineScrollEl.scrollLeft;
    const t = Math.max(0, Math.min(totalDuration, pixelToTime(contentX)));
    programCurrentTime = t;
    if (programPlayer?.video) {
      programPlayer.video.seekToTime(t).subscribe();
    }
  }

  /** Convert timeline time (seconds) → absolute pixel X, accounting for clamped clip widths */
  function timeToPixel(t: number): number {
    let px = 0;
    let elapsed = 0;
    for (let i = 0; i < timeline.length; i++) {
      const dur = timeline[i].duration;
      const w = Math.max(80, Math.min(300, dur * 8));
      if (elapsed + dur >= t || i === timeline.length - 1) {
        const frac = dur > 0 ? Math.min(1, Math.max(0, (t - elapsed) / dur)) : 0;
        return px + frac * w;
      }
      px += w + 4;
      elapsed += dur;
    }
    return px;
  }

  /** Convert absolute pixel X → timeline time (seconds) */
  function pixelToTime(px: number): number {
    let pxOffset = 0;
    let elapsed = 0;
    for (let i = 0; i < timeline.length; i++) {
      const dur = timeline[i].duration;
      const w = Math.max(80, Math.min(300, dur * 8));
      if (px <= pxOffset + w || i === timeline.length - 1) {
        const frac = w > 0 ? Math.max(0, Math.min(1, (px - pxOffset) / w)) : 0;
        return elapsed + frac * dur;
      }
      pxOffset += w + 4;
      elapsed += dur;
    }
    return totalDuration;
  }

  function handleKeydown(e: KeyboardEvent): void {
    // Don't fire when typing in an input/textarea
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

    switch (e.key) {
      case 'i':
      case 'I':
        e.preventDefault();
        markIn();
        break;
      case 'o':
      case 'O':
        e.preventDefault();
        markOut();
        break;
      case '.':
        e.preventDefault();
        addToTimeline();
        break;
      case ' ':
        e.preventDefault();
        if (focusedPanel === 'program' || focusedPanel === 'timeline') {
          try {
            programPlayer?.video?.togglePlayPause();
          } catch { /* ignore */ }
        } else {
          try {
            sourcePlayer?.video?.togglePlayPause();
          } catch { /* ignore */ }
        }
        break;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="editor-page">
  <div class="editor-header">
    <h2>✂ Editor</h2>
    <span class="muted" style="font-size:0.85em">Segment-accurate rough cut editor</span>
    <span class="kbd-hint muted">
      <kbd>I</kbd> Mark In &nbsp;
      <kbd>O</kbd> Mark Out &nbsp;
      <kbd>.</kbd> Add to Timeline &nbsp;
      <kbd>Space</kbd> Play/Pause
    </span>
  </div>

  <!-- ── Top row: Bin + Source Monitor + Program Monitor ───────────────── -->
  <div class="editor-top">

    <!-- Bin Panel -->
    <div
      class="bin-panel panel"
      class:focused={focusedPanel === 'bin'}
      tabindex="-1"
      onfocus={() => focusedPanel = 'bin'}
      onblur={panelBlur}
      role="region"
      aria-label="Bin"
    >
      <h3 class="panel-title">Bin</h3>

      <input
        type="text"
        bind:value={binSearch}
        placeholder="Filter flows…"
        class="bin-filter-input"
      />

      <div class="bin-grid">
        {#if binLoading}
          <div class="bin-empty"><Spinner size="1em" /> Loading…</div>
        {:else if filteredBinFlows.length === 0}
          <p class="bin-empty muted">No video flows found.</p>
        {:else}
          {#each filteredBinFlows as flow (flow.id)}
            <button
              class="bin-card"
              class:active={activeFlow?.id === flow.id}
              onclick={() => selectBinFlow(flow)}
              title={flow.label || flow.id}
              use:binLazyThumb={flow}
            >
              <div class="bin-card-thumb">
                {#if binThumbnails[flow.id]}
                  <img src={binThumbnails[flow.id]} alt="" />
                {:else if binThumbFailed[flow.id]}
                  <span class="thumb-icon">▶</span>
                {:else}
                  <span class="thumb-icon"><Spinner size="0.8em" /></span>
                {/if}
              </div>
              <div class="bin-card-label">{flow.label || flow.id.slice(0, 8)}</div>
            </button>
          {/each}
          {#if binHasMore}
            <div class="bin-sentinel" use:binInfiniteScroll></div>
          {/if}
        {/if}
      </div>
    </div>

    <!-- Source Monitor -->
    <div
      class="monitor-panel panel"
      class:focused={focusedPanel === 'source'}
      tabindex="-1"
      onfocus={() => focusedPanel = 'source'}
      onblur={panelBlur}
      role="region"
      aria-label="Source Monitor"
    >
      <h3 class="panel-title">
        Source Monitor
        {#if activeFlow}
          <span class="muted"> — {activeFlow.label || activeFlow.id.slice(0, 8)}</span>
        {/if}
      </h3>

      <div class="player-wrapper">
        {#if sourceLoading}
          <div class="player-placeholder"><Spinner /> Loading segments…</div>
        {:else if sourceError}
          <div class="player-placeholder error-text">{sourceError}</div>
        {:else if !activeFlow}
          <div class="player-placeholder muted">Select a flow from the Bin</div>
        {:else if activeSegments.length === 0}
          <div class="player-placeholder muted">No segments found</div>
        {:else}
          <div
            class="omakase-container"
            use:sourcePlayerAction={activeSegments}
          ></div>
        {/if}
      </div>

      <!-- Mark In / Out controls -->
      <div class="monitor-controls">
        <div class="mark-points">
          <button class="btn-mark" onclick={markIn} disabled={!sourcePlayerReady} title="Mark In at current segment">
            ◀ Mark In
          </button>
          <span class="mark-label">{inLabel}</span>
          <span class="mark-sep">→</span>
          <span class="mark-label">{outLabel}</span>
          <button class="btn-mark" onclick={markOut} disabled={!sourcePlayerReady} title="Mark Out at current segment">
            Mark Out ▶
          </button>
        </div>
        <div class="add-row">
          {#if clipDuration > 0}
            <span class="muted" style="font-size:0.8em">{formatSecs(clipDuration)}</span>
          {/if}
          <button
            class="primary btn-add-clip"
            onclick={addToTimeline}
            disabled={!sourcePlayerReady || !activeSegments.length}
            title={inSegIdx === null && outSegIdx === null ? 'Add entire clip to timeline' : 'Add marked range to timeline'}
          >
            ➕ Add to Timeline
          </button>
          <button
            class="btn-small"
            onclick={() => { inSegIdx = null; outSegIdx = null; }}
            disabled={inSegIdx === null && outSegIdx === null}
            title="Clear in/out points"
          >
            Clear marks
          </button>
        </div>
      </div>

      {#if sourcePlayerReady && activeSegments.length > 0}
        <div class="segment-info muted">
          Segment {currentSegIdx + 1} / {activeSegments.length}
          {#if inSegIdx !== null && outSegIdx !== null && inSegIdx <= outSegIdx}
            · Selection: {outSegIdx - inSegIdx + 1} segment(s)
          {/if}
        </div>
      {/if}
    </div>

    <!-- Program Monitor -->
    <div
      class="monitor-panel panel"
      class:focused={focusedPanel === 'program'}
      tabindex="-1"
      onfocus={() => focusedPanel = 'program'}
      onblur={panelBlur}
      role="region"
      aria-label="Program Monitor"
    >
      <h3 class="panel-title">Program Monitor</h3>

      <div class="player-wrapper">
        {#if programBuilding}
          <div class="player-placeholder"><Spinner /> Building preview…</div>
        {:else if timeline.length === 0}
          <div class="player-placeholder muted">Add clips to the timeline to preview</div>
        {:else}
          <div id="omakase-program-monitor" class="omakase-container"></div>
        {/if}
      </div>

      <div class="monitor-controls">
        <span class="muted" style="font-size:0.8em">
          {timeline.length} clip(s) · {formatSecs(totalDuration)}
          {#if programBuilding}&nbsp;<Spinner size="0.8em" />{/if}
        </span>
      </div>

      <!-- Export -->
      <div class="export-panel">
        <input
          type="text"
          bind:value={exportLabel}
          placeholder="Rough cut label (optional)"
          class="export-label-input"
        />
        <button
          class="btn-export"
          onclick={exportRoughCut}
          disabled={exporting || timeline.length === 0}
        >
          {exporting ? 'Exporting…' : '💾 Export Rough Cut'}
        </button>
      </div>
    </div>
  </div>

  <!-- ── Timeline ──────────────────────────────────────────────────────── -->
  <div
    class="timeline-panel panel"
    class:focused={focusedPanel === 'timeline'}
    tabindex="-1"
    onfocus={() => focusedPanel = 'timeline'}
    onblur={panelBlur}
    role="region"
    aria-label="Timeline"
  >
    <div class="timeline-header">
      <h3 class="panel-title" style="margin:0">Timeline</h3>
      <span class="muted" style="font-size:0.85em">
        {timeline.length} clip(s) · {formatSecs(totalDuration)}
      </span>
      {#if timeline.length > 0}
        <button class="btn-small danger" onclick={() => timeline = []}>Clear all</button>
      {/if}
    </div>

    {#if timeline.length === 0}
      <p class="muted" style="padding:1em 0;font-size:0.85em">
        Mark In/Out in the Source Monitor and click "Add to Timeline".
        Drag clips to reorder.
      </p>
    {:else}
      <div
        class="timeline-scroll-area"
        bind:this={timelineScrollEl}
        onclick={onTrackClick}
        role="presentation"
      >
        <div class="timeline-inner" style="min-width:{totalTrackPx}px">
          <!-- Ruler with timecodes -->
          <div class="ruler-bar">
            {#each rulerTicks as tick}
              <div class="ruler-tick" style="left:{tick.px}px">
                <span>{tick.label}</span>
              </div>
            {/each}
          </div>

          <!-- Clips -->
          <div class="clips-row">
            {#each timeline as clip, idx}
              {@const w = Math.max(80, Math.min(300, clip.duration * 8))}
              {@const thumb = binThumbnails[clip.flowId] ?? null}
              {#if !thumb}{ensureThumb(clip.flowId)}{/if}
              <div
                class="timeline-clip"
                style="width:{w}px"
                draggable="true"
                ondragstart={(e) => onClipDragStart(e, idx)}
                ondragover={onClipDragOver}
                ondrop={(e) => onClipDrop(e, idx)}
                role="listitem"
              >
                {#if thumb}
                  <div class="clip-thumb" style="background-image:url('{thumb}')"></div>
                {/if}
                <div class="clip-body">
                  <div class="clip-label" title="{clip.flowLabel} ({clip.segments.length} segs)">
                    {clip.flowLabel}
                  </div>
                  <div class="clip-meta">
                    {formatSecs(clip.duration)} · {clip.segments.length} seg{clip.segments.length !== 1 ? 's' : ''}
                  </div>
                </div>
                <button
                  class="clip-remove"
                  onclick={() => removeClip(clip.id)}
                  title="Remove clip"
                >✕</button>
              </div>
            {/each}
          </div>

          <!-- Playhead -->
          <div
            class="playhead"
            style="left:{playheadX}px"
            onmousedown={onPlayheadMouseDown}
            role="slider"
            aria-label="Playhead"
            aria-valuenow={programCurrentTime}
            aria-valuemin={0}
            aria-valuemax={totalDuration}
            tabindex="0"
          ></div>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .editor-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    padding: 0;
  }

  .editor-header {
    display: flex;
    align-items: baseline;
    gap: 1em;
    padding: 0.75em 1.5em 0.5em;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .editor-header h2 {
    margin: 0;
    font-size: 1.1em;
  }

  .kbd-hint {
    margin-left: auto;
    font-size: 0.75em;
    display: flex;
    align-items: center;
    gap: 0.4em;
    flex-wrap: wrap;
    color: var(--text-muted, #888);
  }

  .kbd-hint span {
    display: flex;
    align-items: center;
    gap: 0.2em;
    margin-right: 0.6em;
  }

  kbd {
    display: inline-block;
    background: var(--bg-alt, #2a2a2a);
    border: 1px solid var(--border, #444);
    border-radius: 3px;
    padding: 0.1em 0.35em;
    font-family: monospace;
    font-size: 0.95em;
    color: var(--text, #e0e0e0);
    line-height: 1.4;
  }

  .editor-top {
    display: grid;
    grid-template-columns: 220px 1fr 1fr;
    gap: 0.75em;
    padding: 0.75em;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .panel {
    background: var(--panel);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 6px;
    padding: 0.75em;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    outline: none;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }

  .panel.focused {
    border-color: rgba(90, 159, 212, 0.55);
    box-shadow: 0 0 0 1px rgba(90, 159, 212, 0.18);
  }

  .panel-title {
    margin: 0 0 0.5em;
    font-size: 0.9em;
    font-weight: 600;
    color: var(--text-muted, #aaa);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex-shrink: 0;
  }

  /* Bin */
  .bin-panel {
    overflow: hidden;
    gap: 0.4em;
  }

  .bin-filter-input {
    font-size: 0.82em;
    flex-shrink: 0;
    margin-bottom: 0.4em;
    width: 100%;
    box-sizing: border-box;
  }

  .bin-grid {
    flex: 1;
    overflow-y: auto;
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px;
    align-content: start;
  }

  .bin-empty {
    grid-column: 1 / -1;
    padding: 1em 0;
    text-align: center;
    font-size: 0.82em;
    color: var(--text-muted, #888);
  }

  .bin-card {
    background: var(--bg, #1e1e1e);
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    text-align: left;
    padding: 0;
    transition: border-color 0.15s;
  }

  .bin-card:hover { border-color: var(--accent, #5a9fd4); }
  .bin-card.active { border-color: var(--accent, #5a9fd4); box-shadow: 0 0 0 1px var(--accent, #5a9fd4); }

  .bin-card-thumb {
    width: 100%;
    aspect-ratio: 16 / 9;
    background: #111;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .bin-card-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .thumb-icon {
    color: var(--text-muted, #666);
    font-size: 0.9em;
  }

  .bin-card-label {
    font-size: 0.72em;
    padding: 0.25em 0.35em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text, #e0e0e0);
  }

  .bin-sentinel {
    grid-column: 1 / -1;
    height: 1px;
  }

  /* Monitors */
  .monitor-panel {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .player-wrapper {
    width: 100%;
    aspect-ratio: 16 / 9;
    background: #000;
    border-radius: 4px;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    margin-bottom: 0.5em;
    flex-shrink: 0;
  }

  .omakase-container {
    width: 100%;
    height: 100%;
    min-height: 200px;
  }

  .player-placeholder {
    padding: 1em;
    text-align: center;
    font-size: 0.85em;
  }

  /* Controls */
  .monitor-controls {
    display: flex;
    flex-direction: column;
    gap: 0.4em;
    flex-shrink: 0;
  }

  .mark-points {
    display: flex;
    align-items: center;
    gap: 0.4em;
    flex-wrap: wrap;
  }

  .btn-mark {
    background: var(--panel, #333);
    border: 1px solid var(--border);
    color: var(--text, #e0e0e0);
    border-radius: 4px;
    padding: 0.25em 0.6em;
    font-size: 0.8em;
    cursor: pointer;
  }

  .btn-mark:hover:not(:disabled) { border-color: var(--accent, #5a9fd4); }
  .btn-mark:disabled { opacity: 0.4; cursor: not-allowed; }

  .mark-label {
    font-family: monospace;
    font-size: 0.8em;
    color: var(--accent, #5a9fd4);
    min-width: 3em;
    text-align: center;
  }

  .mark-sep { color: var(--text-muted, #888); font-size: 0.8em; }

  .add-row {
    display: flex;
    align-items: center;
    gap: 0.4em;
  }

  .btn-add-clip {
    font-size: 0.82em;
    padding: 0.3em 0.7em;
  }

  .segment-info {
    font-size: 0.75em;
    margin-top: 0.3em;
    flex-shrink: 0;
  }

  /* Export */
  .export-panel {
    display: flex;
    gap: 0.4em;
    margin-top: 0.5em;
    flex-shrink: 0;
  }

  .export-label-input {
    flex: 1;
    font-size: 0.82em;
  }

  .btn-export {
    background: var(--accent, #5a9fd4);
    color: #fff;
    border: none;
    border-radius: 4px;
    padding: 0.35em 0.8em;
    font-size: 0.82em;
    cursor: pointer;
    white-space: nowrap;
  }

  .btn-export:hover:not(:disabled) { filter: brightness(1.15); }
  .btn-export:disabled { opacity: 0.4; cursor: not-allowed; }

  /* Timeline */
  .timeline-panel {
    flex-shrink: 0;
    margin: 0 0.75em 0.75em;
    min-height: 120px;
    max-height: 160px;
  }

  .timeline-header {
    display: flex;
    align-items: center;
    gap: 1em;
    margin-bottom: 0.5em;
    flex-shrink: 0;
  }

  .btn-small {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text, #e0e0e0);
    border-radius: 4px;
    padding: 0.2em 0.5em;
    font-size: 0.78em;
    cursor: pointer;
  }

  .btn-small:hover:not(:disabled) { border-color: var(--accent, #5a9fd4); }
  .btn-small:disabled { opacity: 0.4; cursor: not-allowed; }

  .btn-small.danger:hover { border-color: var(--error, #c0392b); color: var(--error, #c0392b); }

  /* Timeline scroll container */
  .timeline-scroll-area {
    flex: 1;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: thin;
    scrollbar-color: #3a5a7a #1a2530;
    cursor: default;
  }

  .timeline-scroll-area::-webkit-scrollbar { height: 5px; }
  .timeline-scroll-area::-webkit-scrollbar-track { background: #1a2530; border-radius: 3px; }
  .timeline-scroll-area::-webkit-scrollbar-thumb { background: #3a5a7a; border-radius: 3px; }

  /* Inner content — full track width, position:relative for playhead */
  .timeline-inner {
    position: relative;
    display: flex;
    flex-direction: column;
  }

  /* Ruler */
  .ruler-bar {
    position: relative;
    height: 20px;
    flex-shrink: 0;
    background: #0f1c27;
    border-bottom: 1px solid #2a4560;
    user-select: none;
    cursor: pointer;
  }

  .ruler-tick {
    position: absolute;
    top: 0;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    pointer-events: none;
  }

  .ruler-tick::before {
    content: '';
    width: 1px;
    height: 5px;
    background: #4a7a9a;
    flex-shrink: 0;
  }

  .ruler-tick span {
    font-size: 9px;
    color: #6a9aba;
    white-space: nowrap;
    padding-left: 2px;
    line-height: 1.4;
  }

  /* Clips row */
  .clips-row {
    display: flex;
    gap: 4px;
    padding: 4px 0 6px;
    align-items: stretch;
  }

  .timeline-clip {
    flex-shrink: 0;
    background: #1e3248;
    border: 1px solid var(--accent, #5a9fd4);
    border-radius: 4px;
    padding: 0;
    position: relative;
    display: flex;
    flex-direction: row;
    cursor: grab;
    user-select: none;
    height: 70px;
    overflow: hidden;
  }

  .timeline-clip:active { cursor: grabbing; }

  /* Thumbnail: fixed 16:9 box on the left */
  .clip-thumb {
    flex-shrink: 0;
    width: 124px; /* 70px * 16/9 ≈ 124px */
    height: 70px;
    background-size: contain;
    background-repeat: no-repeat;
    background-position: center;
    background-color: #0d1a26;
    border-right: 1px solid rgba(90,159,212,0.25);
  }

  /* Body: text fills remaining space with matching dark-blue bg */
  .clip-body {
    flex: 1;
    min-width: 0;
    background: #1e3248;
    display: flex;
    flex-direction: column;
    justify-content: center;
    padding: 0.3em 1.4em 0.3em 0.5em;
    gap: 0.15em;
  }

  .clip-label {
    font-size: 0.78em;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text, #e0e0e0);
  }

  .clip-meta {
    font-size: 0.68em;
    color: var(--text-muted, #7a9aba);
  }

  .clip-remove {
    position: absolute;
    top: 2px;
    right: 3px;
    background: transparent;
    border: none;
    color: var(--text-muted, #888);
    cursor: pointer;
    font-size: 0.7em;
    line-height: 1;
    padding: 0;
  }

  .clip-remove:hover { color: var(--error, #c0392b); }

  /* Playhead */
  .playhead {
    position: absolute;
    top: 0;
    width: 2px;
    height: 100%;
    background: #e74c3c;
    transform: translateX(-1px);
    cursor: ew-resize;
    z-index: 10;
    pointer-events: all;
  }

  /* Triangle handle at top of ruler */
  .playhead::before {
    content: '';
    position: absolute;
    top: 0;
    left: 50%;
    transform: translateX(-50%);
    width: 0;
    height: 0;
    border-left: 6px solid transparent;
    border-right: 6px solid transparent;
    border-top: 10px solid #e74c3c;
  }

  /* Shared badge */
  :global(.badge) {
    display: inline-block;
    font-size: 0.72em;
    padding: 0.1em 0.4em;
    border-radius: 3px;
    background: var(--accent-dim, #2a4a6a);
    color: var(--accent, #5a9fd4);
    white-space: nowrap;
  }
</style>
