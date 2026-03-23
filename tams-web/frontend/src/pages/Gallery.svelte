<script lang="ts">
  import { onMount } from 'svelte';
  import { apiGet, parsePagination, formatShortName } from '../lib/api.js';
  import { parseTimerange, nanosToSeconds } from '../lib/timerange.js';
  import { FORMAT_VIDEO, FORMAT_AUDIO, createAssembly } from '../lib/ingest.js';
  import { buildFlowsQuery, buildFlowQuery, buildSegmentsQuery } from '../lib/query.js';
  import { push } from '../lib/router.js';
  import { addToast } from '../lib/toast.js';
  import { errorMessage } from '../lib/errors.js';
  import { extractThumbnail, clearThumbnailCache, enableThumbnailCache, getCachedThumbnail } from '../lib/thumbnail.js';
  import { formatSeconds } from '../lib/playerUtils.js';
  import Spinner from '../components/Spinner.svelte';
  import { collectionFlowIds } from '../types/tams.js';
  import type { Flow, AssemblyItem, AssemblyResult } from '../types/tams.js';

  // --- State ---
  let flows: any[] = $state([]);
  let thumbnails: Record<string, string> = $state({});    // flowId -> blob URL
  let thumbFailed: Record<string, boolean> = $state({});   // flowId -> true (no thumbnail possible)
  let loading: boolean = $state(true);
  let hasMore: boolean = $state(true);
  let loadingMore: boolean = $state(false);
  let nextKey: string | null = null;

  // --- Filters ---
  let filterFormat: string = $state(FORMAT_VIDEO);
  let filterSource: string = $state('');
  let filterLabel: string = $state('');
  let filterEditsOnly: boolean = $state(false);
  let sortBy: string = $state('newest');

  // --- Thumbnail progress ---
  let videoFlowCount: number = $derived(flows.filter(f => f.format === FORMAT_VIDEO).length);
  let thumbDoneCount: number = $derived(
    flows.filter(f => f.format === FORMAT_VIDEO && (thumbnails[f.id] || thumbFailed[f.id])).length
  );
  let thumbProgress: number = $derived(videoFlowCount > 0 ? Math.round(thumbDoneCount / videoFlowCount * 100) : 100);
  let thumbsLoading: boolean = $derived(thumbDoneCount < videoFlowCount);

  // --- Tile size (persisted) ---
  let tileSize: number = $state(220);

  onMount(() => {
    enableThumbnailCache();
    try {
      const saved = localStorage.getItem('tams-gallery-tilesize');
      if (saved) tileSize = Number(saved);
    } catch { /* private browsing or storage disabled */ }
    loadFlows();
    return () => clearThumbnailCache();
  });

  function saveTileSize(val: number): void {
    tileSize = val;
    try { localStorage.setItem('tams-gallery-tilesize', String(val)); } catch { /* ignore */ }
  }

  // --- Data loading ---

  function enrichFlows(rawFlows: any[]): any[] {
    return rawFlows.map(f => {
      const tr = parseTimerange(f.timerange);
      const dur: number = (tr.type !== 'never' && tr.start && tr.end)
        ? nanosToSeconds(tr.end.nanos - tr.start.nanos) : 0;
      return { ...f, _duration: dur };
    });
  }

  async function loadFlows(): Promise<void> {
    loading = true;
    try {
      const resp = await apiGet(buildFlowsQuery({ limit: 30, includeTimerange: true }));
      flows = enrichFlows(resp.data || []);
      const pag = parsePagination(resp.headers);
      nextKey = pag.nextKey;
      hasMore = !!nextKey;
    } catch (err) {
      flows = [];
      hasMore = false;
    } finally {
      loading = false;
    }
  }

  async function loadMore(): Promise<void> {
    if (loadingMore || !hasMore || !nextKey) return;
    loadingMore = true;
    try {
      const resp = await apiGet(buildFlowsQuery({ limit: 30, includeTimerange: true }, nextKey));
      const newFlows: any[] = enrichFlows(resp.data || []);
      if (newFlows.length === 0) {
        hasMore = false;
        return;
      }
      // Dedup by ID (pagination overlap)
      const existingIds: Set<string> = new Set(flows.map(f => f.id));
      const unique: any[] = newFlows.filter(f => !existingIds.has(f.id));
      if (unique.length === 0) {
        // All duplicates — no new data, stop paginating
        hasMore = false;
        return;
      }
      flows = [...flows, ...unique];
      const pag = parsePagination(resp.headers);
      nextKey = pag.nextKey;
      hasMore = !!nextKey && newFlows.length >= 30;
    } catch {
      hasMore = false;
    } finally {
      loadingMore = false;
    }
  }

  // --- Filtering & sorting ---

  let sources: string[] = $derived([...new Set(flows.map(f => f.source_id).filter(Boolean))]);

  let filteredFlows: any[] = $derived.by(() => {
    let result: any[] = flows;
    if (filterFormat) result = result.filter(f => f.format === filterFormat);
    if (filterSource) result = result.filter(f => f.source_id === filterSource);
    if (filterLabel) {
      const q: string = filterLabel.toLowerCase();
      result = result.filter(f => (f.label || '').toLowerCase().includes(q));
    }
    if (filterEditsOnly) result = result.filter(f => f.tags?.edit_export?.[0] === 'true');

    switch (sortBy) {
      case 'oldest': result = [...result].sort((a, b) => (a._duration || 0) - (b._duration || 0) || a.id.localeCompare(b.id)); break;
      case 'newest': result = [...result].sort((a, b) => b.id.localeCompare(a.id)); break;
      case 'longest': result = [...result].sort((a, b) => (b._duration || 0) - (a._duration || 0)); break;
      case 'shortest': result = [...result].sort((a, b) => (a._duration || 0) - (b._duration || 0)); break;
      case 'label-az': result = [...result].sort((a, b) => (a.label || '').localeCompare(b.label || '')); break;
    }
    return result;
  });

  // --- Thumbnail loading (lazy, per-card) ---

  async function loadThumbnail(flow: any, signal: AbortSignal): Promise<void> {
    if (flow.format !== FORMAT_VIDEO || thumbnails[flow.id] || thumbFailed[flow.id]) return;
    try {
      // Fetch first 3 segments for this flow (fallback if first has no IDR)
      const resp = await apiGet(buildSegmentsQuery(flow.id, { limit: 3, presigned: true }));
      if (signal?.aborted) return;
      const segments: any[] = resp.data || [];
      if (!segments.length) {
        console.warn(`[gallery] No segments for flow ${flow.id.slice(0, 8)}`);
        thumbFailed = { ...thumbFailed, [flow.id]: true };
        return;
      }

      const blobUrl: string | null = await extractThumbnail({
        key: flow.id,
        segments,
        flow,
        width: Math.min(tileSize * 2, 640),
        signal,
      });

      if (signal?.aborted) return;

      if (blobUrl) {
        thumbnails = { ...thumbnails, [flow.id]: blobUrl };
      } else {
        console.warn(`[gallery] Thumbnail extraction returned null for ${flow.id.slice(0, 8)}`);
        thumbFailed = { ...thumbFailed, [flow.id]: true };
      }
    } catch (err) {
      if (!signal?.aborted) {
        console.warn(`[gallery] Thumbnail failed for ${flow.id.slice(0, 8)}:`, errorMessage(err));
        thumbFailed = { ...thumbFailed, [flow.id]: true };
      }
    }
  }

  // --- Svelte actions ---

  function lazyThumb(node: HTMLElement, flow: any): void | { destroy(): void } {
    if (flow.format !== FORMAT_VIDEO) {
      return;
    }
    let abortCtrl = new AbortController();

    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && !thumbnails[flow.id] && !thumbFailed[flow.id]) {
        loadThumbnail(flow, abortCtrl.signal);
      } else if (!entry.isIntersecting) {
        abortCtrl.abort();
        abortCtrl = new AbortController();
      }
    }, { rootMargin: '200px' });

    observer.observe(node);
    return {
      destroy() {
        observer.disconnect();
        abortCtrl.abort();
      },
    };
  }

  function infiniteScroll(node: HTMLElement): { destroy(): void } {
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting) loadMore();
    }, { rootMargin: '400px' });

    observer.observe(node);
    return {
      destroy() { observer.disconnect(); },
    };
  }

  // --- Assembly ---

  let assemblyItems: AssemblyItem[] = $state([]); // [{ flow, audioFlows }]
  let assemblyLabel: string = $state('');
  let creatingAssembly: boolean = $state(false);
  let assemblyTotal: number = $derived(assemblyItems.reduce((sum, item) => sum + (item.flow._duration || 0), 0));
  let showAssembly: boolean = $derived(assemblyItems.length > 0);
  let assemblyIdSet: Set<string> = $derived(new Set(assemblyItems.map(i => i.flow.id)));
  let dragIdx: number | null = null; // for reorder drag
  let addingFlows: Set<string> = new Set(); // mutex for addToAssembly

  function computeDuration(flow: any): number {
    const tr = parseTimerange(flow.timerange);
    return (tr.type !== 'never' && tr.start && tr.end)
      ? nanosToSeconds(tr.end.nanos - tr.start.nanos) : 0;
  }

  async function addToAssembly(flow: any): Promise<void> {
    if (assemblyIdSet.has(flow.id)) return;
    if (flow.format !== FORMAT_VIDEO) return;
    if (addingFlows.has(flow.id)) return;
    addingFlows.add(flow.id);

    // Find linked audio flows from flow_collection
    let audioFlows: any[] = [];
    const collectionIds: string[] = collectionFlowIds(flow.flow_collection).filter(fid => fid !== flow.id);
    if (collectionIds.length > 0) {
      const results = await Promise.allSettled(
        collectionIds.map(fid => apiGet(buildFlowQuery(fid, { includeTimerange: true })))
      );
      audioFlows = results
        .filter((r): r is PromiseFulfilledResult<any> => r.status === 'fulfilled' && r.value.data?.format === FORMAT_AUDIO)
        .map(r => {
          const f = r.value.data;
          return { ...f, _duration: computeDuration(f) };
        });
    }

    assemblyItems = [...assemblyItems, { flow, audioFlows }];
    addingFlows.delete(flow.id);
  }

  function removeFromAssembly(flowId: string): void {
    assemblyItems = assemblyItems.filter(i => i.flow.id !== flowId);
  }

  function onAssemblyDragStart(e: DragEvent, idx: number): void {
    dragIdx = idx;
    e.dataTransfer!.effectAllowed = 'move';
  }

  function onAssemblyDrop(e: DragEvent, targetIdx: number): void {
    e.preventDefault();
    if (dragIdx === null || dragIdx === targetIdx) return;
    const items: AssemblyItem[] = [...assemblyItems];
    const [moved] = items.splice(dragIdx, 1);
    items.splice(targetIdx, 0, moved);
    assemblyItems = items;
    dragIdx = null;
  }

  async function handleCreateAssembly(): Promise<void> {
    if (assemblyItems.length === 0) return;
    creatingAssembly = true;
    try {
      const result: AssemblyResult = await createAssembly({
        items: assemblyItems,
        label: assemblyLabel || `Assembly ${new Date().toISOString().slice(0, 16).replace('T', ' ')}`,
      });
      const msg: string = `Assembly created: ${result.totalSegments} segments` +
        (result.failed ? ` (${result.failed} failed)` : '');
      addToast(msg, result.failed ? 'warning' : 'success');
      assemblyItems = [];
      assemblyLabel = '';
      push(`/player/${result.videoFlowId}`);
    } catch (err) {
      addToast(`Assembly failed: ${errorMessage(err)}`, 'error');
    } finally {
      creatingAssembly = false;
    }
  }

  // --- Helpers ---

  function formatDuration(dur: number | undefined): string {
    if (!dur || dur <= 0) return '--';
    return formatSeconds(dur, 0);
  }
</script>

<div class="page">
  <div class="gallery-header">
    <h2>Gallery</h2>
    <div class="tile-slider">
      <span class="tile-icon" style="font-size:0.7em">&#9632;</span>
      <input type="range" min="120" max="400" step="20"
        value={tileSize}
        oninput={(e: Event) => saveTileSize(Number((e.target as HTMLInputElement).value))} />
      <span class="tile-icon" style="font-size:1.2em">&#9632;</span>
    </div>
  </div>

  <!-- Thumbnail progress -->
  {#if thumbsLoading}
    <div class="thumb-progress">
      <div class="thumb-progress-bar" style="width: {thumbProgress}%"></div>
    </div>
    <span class="thumb-progress-label">{thumbDoneCount}/{videoFlowCount} thumbnails</span>
  {/if}

  <!-- Filter bar -->
  <div class="filter-bar">
    <select bind:value={filterFormat} class="filter-input">
      <option value="">All formats</option>
      <option value={FORMAT_VIDEO}>Video</option>
      <option value={FORMAT_AUDIO}>Audio</option>
    </select>

    <select bind:value={filterSource} class="filter-input">
      <option value="">All sources</option>
      {#each sources as sid}
        <option value={sid}>{sid.slice(0, 8)}</option>
      {/each}
    </select>

    <input type="text" bind:value={filterLabel} placeholder="Search label..." class="filter-input" />

    <label class="filter-check">
      <input type="checkbox" bind:checked={filterEditsOnly} />
      Edits only
    </label>

    <select bind:value={sortBy} class="filter-input">
      <option value="newest">Newest</option>
      <option value="oldest">Oldest</option>
      <option value="longest">Longest</option>
      <option value="shortest">Shortest</option>
      <option value="label-az">Label A-Z</option>
    </select>
  </div>

  <!-- Assembly bar — always visible -->
  <div class="assembly-bar" class:assembly-has-items={showAssembly}>
    <div class="assembly-header">
      {#if showAssembly}
        <strong>Assembly ({assemblyItems.length} clip{assemblyItems.length !== 1 ? 's' : ''}, {formatDuration(assemblyTotal)})</strong>
        <div class="assembly-actions">
          <input type="text" bind:value={assemblyLabel} placeholder="Assembly name..." class="assembly-name-input" />
          <button class="primary btn-small" onclick={handleCreateAssembly} disabled={creatingAssembly}>
            {creatingAssembly ? 'Creating...' : 'Create Assembly'}
          </button>
          <button class="btn-small" onclick={() => { assemblyItems = []; assemblyLabel = ''; }}>Clear</button>
        </div>
      {:else}
        <span class="assembly-hint">Assembly — click + on video cards to build an edit list</span>
      {/if}
    </div>
    {#if showAssembly}
      <div class="assembly-strip" role="list">
        {#each assemblyItems as item, i (item.flow.id)}
          <div class="assembly-item" role="listitem"
            draggable="true"
            ondragstart={(e: DragEvent) => onAssemblyDragStart(e, i)}
            ondragover={(e: DragEvent) => e.preventDefault()}
            ondrop={(e: DragEvent) => onAssemblyDrop(e, i)}
            ondragend={() => { dragIdx = null; }}
          >
            <div class="assembly-thumb">
              {#if thumbnails[item.flow.id]}
                <img src={thumbnails[item.flow.id]} alt="" />
              {:else}
                <span class="assembly-thumb-placeholder">{formatShortName(item.flow.format)}</span>
              {/if}
            </div>
            <div class="assembly-item-info">
              <span class="assembly-item-label">{item.flow.label || item.flow.id.slice(0, 8)}</span>
              <span class="assembly-item-dur">{formatDuration(item.flow._duration)}</span>
              {#if item.audioFlows.length > 0}
                <span class="assembly-audio-badge">+ audio</span>
              {/if}
            </div>
            <button class="assembly-remove" onclick={() => removeFromAssembly(item.flow.id)}>x</button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  {#if loading}
    <div class="gallery-loading">
      <Spinner size="1.5em" /> Loading flows...
    </div>
  {:else if filteredFlows.length === 0}
    <div class="gallery-empty">
      <p class="muted">{flows.length === 0 ? 'No flows in the system yet.' : 'No flows match your filters.'}</p>
    </div>
  {:else}
    <div class="gallery-grid" style="--tile-size: {tileSize}px">
      {#each filteredFlows as flow (flow.id)}
        <a href="#/player/{flow.id}" class="gallery-card" use:lazyThumb={flow}>
          {#if flow.format === FORMAT_VIDEO}
            <button class="card-add-btn"
              class:added={assemblyIdSet.has(flow.id)}
              onclick={(e: MouseEvent) => { e.preventDefault(); e.stopPropagation(); addToAssembly(flow); }}
            >{assemblyIdSet.has(flow.id) ? '\u2713' : '+'}</button>
          {/if}
          <div class="card-thumb">
            {#if thumbnails[flow.id]}
              <img src={thumbnails[flow.id]} alt="" />
            {:else if thumbFailed[flow.id]}
              <div class="thumb-placeholder">{formatShortName(flow.format)}</div>
            {:else if flow.format === FORMAT_VIDEO}
              <div class="thumb-placeholder"><Spinner size="1em" /></div>
            {:else}
              <div class="thumb-placeholder thumb-audio">{formatShortName(flow.format)}</div>
            {/if}
          </div>
          <div class="card-info">
            <span class="card-label">{flow.label || flow.id.slice(0, 8)}</span>
            <span class="card-meta">
              {formatShortName(flow.format)}
              {#if flow.codec} / {flow.codec}{/if}
            </span>
            <span class="card-duration">{formatDuration(flow._duration)}</span>
          </div>
        </a>
      {/each}
    </div>

    {#if hasMore}
      <div class="sentinel" use:infiniteScroll>
        <Spinner size="1em" /> Loading more...
      </div>
    {/if}
  {/if}
</div>

<style>
  .page {
    padding: 0;
  }
  .gallery-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1em 1.5em 0;
  }
  .gallery-header h2 {
    margin: 0;
  }
  .tile-slider {
    display: flex;
    align-items: center;
    gap: 0.4em;
  }
  .tile-icon {
    color: var(--text-muted);
  }
  .tile-slider input[type="range"] {
    width: 100px;
    accent-color: var(--accent);
  }

  /* Thumbnail progress */
  .thumb-progress {
    height: 3px;
    background: var(--border);
    margin: 0 1.5em;
    border-radius: 2px;
    overflow: hidden;
  }
  .thumb-progress-bar {
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
    transition: width 0.3s ease;
  }
  .thumb-progress-label {
    display: block;
    text-align: right;
    padding: 0.15em 1.5em 0;
    font-size: 0.7em;
    color: var(--text-muted);
  }

  /* Filter bar */
  .filter-bar {
    display: flex;
    gap: 0.5em;
    padding: 0.75em 1.5em;
    flex-wrap: wrap;
    align-items: center;
  }
  .filter-input {
    padding: 0.3em 0.5em;
    font-size: 0.85em;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--panel);
    color: var(--text);
  }
  .filter-check {
    display: flex;
    align-items: center;
    gap: 0.3em;
    font-size: 0.85em;
    color: var(--text-muted);
    cursor: pointer;
  }

  /* Grid */
  .gallery-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(var(--tile-size, 220px), 1fr));
    gap: 1em;
    padding: 0.5em 1.5em 1.5em;
  }

  /* Card */
  .gallery-card {
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    background: var(--panel);
    text-decoration: none;
    color: var(--text);
    transition: border-color 0.15s, transform 0.1s;
  }
  .gallery-card:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .card-thumb {
    aspect-ratio: 16/9;
    background: #1a1a1a;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .card-thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb-placeholder {
    color: var(--text-muted);
    font-size: 0.85em;
  }
  .thumb-audio {
    text-transform: uppercase;
    font-size: 0.75em;
    letter-spacing: 0.05em;
  }
  .card-info {
    padding: 0.5em 0.6em;
    display: flex;
    flex-direction: column;
    gap: 0.15em;
  }
  .card-label {
    font-weight: 600;
    font-size: 0.85em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .card-meta {
    font-size: 0.7em;
    color: var(--text-muted);
  }
  .card-duration {
    font-size: 0.7em;
    color: var(--text-muted);
    font-family: monospace;
  }

  /* Loading / empty states */
  .gallery-loading, .gallery-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5em;
    padding: 3em;
    color: var(--text-muted);
  }
  .sentinel {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5em;
    padding: 1.5em;
    color: var(--text-muted);
    font-size: 0.85em;
  }

  /* Card add button */
  .card-add-btn {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 1px solid rgba(255,255,255,0.3);
    background: rgba(0,0,0,0.6);
    color: #fff;
    font-size: 1em;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    opacity: 0;
    transition: opacity 0.15s;
    z-index: 2;
    padding: 0;
    line-height: 1;
  }
  .gallery-card:hover .card-add-btn { opacity: 1; }
  .card-add-btn.added {
    opacity: 1;
    background: var(--accent);
    border-color: var(--accent);
  }
  .gallery-card { position: relative; }

  /* Assembly bar */
  .assembly-bar {
    margin: 0 1.5em 0.5em;
    border: 1px dashed var(--border);
    border-radius: 6px;
    background: transparent;
    padding: 0.5em 0.6em;
    transition: border-color 0.15s, background 0.15s;
  }
  .assembly-bar.assembly-has-items {
    border: 2px solid var(--accent);
    background: rgba(90,159,212,0.05);
    padding: 0.6em;
  }
  .assembly-hint {
    color: var(--text-muted);
    font-size: 0.8em;
  }
  .assembly-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5em;
    flex-wrap: wrap;
    gap: 0.5em;
    font-size: 0.85em;
  }
  .assembly-actions {
    display: flex;
    gap: 0.4em;
    align-items: center;
  }
  .assembly-name-input {
    padding: 0.25em 0.5em;
    font-size: 0.85em;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--panel);
    color: var(--text);
    width: 180px;
  }
  .assembly-strip {
    display: flex;
    gap: 0.5em;
    overflow-x: auto;
    padding: 0.25em 0;
  }
  .assembly-strip::-webkit-scrollbar { height: 4px; }
  .assembly-strip::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }
  .assembly-item {
    flex-shrink: 0;
    width: 100px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--panel);
    cursor: grab;
    position: relative;
    overflow: hidden;
  }
  .assembly-item:active { cursor: grabbing; }
  .assembly-thumb {
    aspect-ratio: 16/9;
    background: #1a1a1a;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .assembly-thumb img { width: 100%; height: 100%; object-fit: cover; }
  .assembly-thumb-placeholder { color: var(--text-muted); font-size: 0.7em; }
  .assembly-item-info {
    padding: 0.25em 0.3em;
    display: flex;
    flex-direction: column;
    gap: 0.1em;
  }
  .assembly-item-label {
    font-size: 0.65em;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .assembly-item-dur {
    font-size: 0.6em;
    color: var(--text-muted);
    font-family: monospace;
  }
  .assembly-audio-badge {
    font-size: 0.55em;
    color: var(--accent);
  }
  .assembly-remove {
    position: absolute;
    top: 2px;
    right: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: none;
    background: rgba(0,0,0,0.6);
    color: #fff;
    font-size: 0.6em;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    line-height: 1;
  }
  .assembly-remove:hover { background: var(--error); }
  .btn-small {
    padding: 0.25em 0.55em;
    font-size: 0.75em;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--panel);
    color: var(--text);
    cursor: pointer;
  }
  .btn-small.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  /* Responsive */
  @media (max-width: 600px) {
    .gallery-grid {
      grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
      gap: 0.5em;
      padding: 0.5em;
    }
    .filter-bar {
      padding: 0.5em;
    }
  }
</style>
