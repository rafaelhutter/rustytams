<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage } from '../lib/errors.js';
  import { apiGet, apiPut, parsePagination, formatShortName } from '../lib/api.js';
  import { buildFlowsQuery } from '../lib/query.js';
  import { formatTimerangeDisplay } from '../lib/timerange.js';
  import { link, getHashParams, setHashParams } from '../lib/router.js';
  import { addToast } from '../lib/toast.js';
  import { FORMAT_VIDEO, FORMAT_AUDIO, FORMAT_DATA, FORMAT_MULTI, FORMAT_IMAGE, FORMAT_OPTIONS } from '../lib/ingest.js';
  import Pagination from '../components/Pagination.svelte';
  import Spinner from '../components/Spinner.svelte';
  import type { PaginationInfo } from '../types/tams.js';

  let flows: any[] = $state([]);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);
  let pagination: PaginationInfo = $state({ limit: null, nextKey: null, count: null, timerange: null });

  // Filters
  let filterLabel: string = $state('');
  let filterFormat: string = $state('');
  let filterCodec: string = $state('');
  let filterSourceId: string = $state('');
  let filterTimerange: string = $state('');
  let filterFrameWidth: string = $state('');
  let filterFrameHeight: string = $state('');
  let filterTagName: string = $state('');
  let filterTagValue: string = $state('');

  let paginationRef: Pagination | undefined = $state();

  // Create flow form
  let showCreate: boolean = $state(false);
  const defaultForm: {
    id: string; sourceId: string; format: string;
    codec: string; label: string; desc: string;
    frameWidth: string; frameHeight: string; sampleRate: string; channels: string;
  } = {
    id: '', sourceId: '', format: FORMAT_VIDEO,
    codec: '', label: '', desc: '',
    frameWidth: '', frameHeight: '', sampleRate: '', channels: '',
  };
  let form = $state({ ...defaultForm });
  let creating: boolean = $state(false);
  let createError: string | null = $state(null);

  function generateId(): void {
    form.id = crypto.randomUUID();
  }

  let needsCodec: boolean = $derived(form.format !== FORMAT_MULTI);
  let needsVideo: boolean = $derived(form.format === FORMAT_VIDEO || form.format === FORMAT_IMAGE);
  let needsAudio: boolean = $derived(form.format === FORMAT_AUDIO);

  async function createFlow(): Promise<void> {
    if (!form.id.trim() || !form.sourceId.trim() || !form.format) return;
    if (needsCodec && !form.codec.trim()) return;
    creating = true;
    createError = null;
    try {
      const body: Record<string, any> = {
        id: form.id.trim(),
        source_id: form.sourceId.trim(),
        format: form.format,
      };
      if (needsCodec) body.codec = form.codec.trim();
      if (form.label.trim()) body.label = form.label.trim();
      if (form.desc.trim()) body.description = form.desc.trim();

      if (needsVideo) {
        const w: number = parseInt(form.frameWidth);
        const h: number = parseInt(form.frameHeight);
        if (!w || !h) { createError = 'Frame width and height required'; creating = false; return; }
        body.essence_parameters = { frame_width: w, frame_height: h };
      } else if (needsAudio) {
        const sr: number = parseInt(form.sampleRate);
        const ch: number = parseInt(form.channels);
        if (!sr || !ch) { createError = 'Sample rate and channels required'; creating = false; return; }
        body.essence_parameters = { sample_rate: sr, channels: ch };
      } else if (form.format === FORMAT_DATA) {
        body.essence_parameters = {};
      }

      await apiPut(`/flows/${body.id}`, body);
      showCreate = false;
      form = { ...defaultForm };
      createError = null;
      addToast('Flow created', 'success');
      fetchFlows();
    } catch (e) {
      createError = errorMessage(e);
    } finally {
      creating = false;
    }
  }

  async function fetchFlows(pageKey: string | null = null): Promise<void> {
    loading = true;
    error = null;
    try {
      const query: string = buildFlowsQuery({
        label: filterLabel, format: filterFormat, codec: filterCodec,
        sourceId: filterSourceId, timerange: filterTimerange,
        frameWidth: filterFrameWidth, frameHeight: filterFrameHeight,
        tagName: filterTagName, tagValue: filterTagValue,
      }, pageKey);
      const { data, headers } = await apiGet(query);
      flows = data;
      pagination = parsePagination(headers);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  function applyFilters(): void {
    const p = new URLSearchParams();
    if (filterLabel) p.set('label', filterLabel);
    if (filterFormat) p.set('format', filterFormat);
    if (filterCodec) p.set('codec', filterCodec);
    if (filterSourceId) p.set('sourceId', filterSourceId);
    if (filterTimerange) p.set('timerange', filterTimerange);
    if (filterFrameWidth) p.set('frameWidth', filterFrameWidth);
    if (filterFrameHeight) p.set('frameHeight', filterFrameHeight);
    if (filterTagName) p.set('tagName', filterTagName);
    if (filterTagValue) p.set('tagValue', filterTagValue);
    setHashParams(p);
    paginationRef?.reset();
    fetchFlows();
  }

  function clearFilters(): void {
    filterLabel = '';
    filterFormat = '';
    filterCodec = '';
    filterSourceId = '';
    filterTimerange = '';
    filterFrameWidth = '';
    filterFrameHeight = '';
    filterTagName = '';
    filterTagValue = '';
    setHashParams(new URLSearchParams());
    paginationRef?.reset();
    fetchFlows();
  }

  function handlePage({ key }: { key: string | null }): void {
    fetchFlows(key);
  }

  onMount(() => {
    const p: URLSearchParams = getHashParams();
    if (p.has('label')) filterLabel = p.get('label')!;
    if (p.has('format')) filterFormat = p.get('format')!;
    if (p.has('codec')) filterCodec = p.get('codec')!;
    if (p.has('sourceId')) filterSourceId = p.get('sourceId')!;
    if (p.has('timerange')) filterTimerange = p.get('timerange')!;
    if (p.has('frameWidth')) filterFrameWidth = p.get('frameWidth')!;
    if (p.has('frameHeight')) filterFrameHeight = p.get('frameHeight')!;
    if (p.has('tagName')) filterTagName = p.get('tagName')!;
    if (p.has('tagValue')) filterTagValue = p.get('tagValue')!;
    fetchFlows();
  });

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      if (showCreate) { showCreate = false; createError = null; }
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="page">
  <div class="page-header">
    <h1>Flows</h1>
    <button class="primary" onclick={() => { showCreate = !showCreate; if (showCreate && !form.id) generateId(); }}>
      {showCreate ? 'Cancel' : '+ New Flow'}
    </button>
  </div>

  {#if showCreate}
    <div class="panel create-form">
      <h3>Create Flow</h3>
      {#if createError}
        <p class="error-text" style="font-size:0.85em">{createError}</p>
      {/if}
      <div class="form-grid">
        <label>
          <span class="label-text">ID</span>
          <div class="id-row">
            <input type="text" bind:value={form.id} placeholder="UUID" class="mono" />
            <button onclick={generateId} class="btn-small">Generate</button>
          </div>
        </label>
        <label>
          <span class="label-text">Source ID</span>
          <input type="text" bind:value={form.sourceId} placeholder="Source UUID" class="mono" />
        </label>
        <label>
          <span class="label-text">Format</span>
          <select bind:value={form.format}>
            {#each FORMAT_OPTIONS as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </label>
        {#if needsCodec}
          <label>
            <span class="label-text">Codec</span>
            <input type="text" bind:value={form.codec} placeholder="e.g. video/h264" />
          </label>
        {/if}
        <label>
          <span class="label-text">Label</span>
          <input type="text" bind:value={form.label} placeholder="Optional" />
        </label>
        <label>
          <span class="label-text">Description</span>
          <input type="text" bind:value={form.desc} placeholder="Optional" />
        </label>
        {#if needsVideo}
          <label>
            <span class="label-text">Frame Width</span>
            <input type="number" bind:value={form.frameWidth} placeholder="e.g. 1920" min="1" />
          </label>
          <label>
            <span class="label-text">Frame Height</span>
            <input type="number" bind:value={form.frameHeight} placeholder="e.g. 1080" min="1" />
          </label>
        {/if}
        {#if needsAudio}
          <label>
            <span class="label-text">Sample Rate</span>
            <input type="number" bind:value={form.sampleRate} placeholder="e.g. 48000" min="1" />
          </label>
          <label>
            <span class="label-text">Channels</span>
            <input type="number" bind:value={form.channels} placeholder="e.g. 2" min="1" />
          </label>
        {/if}
      </div>
      <div style="margin-top:0.75em">
        <button class="primary" onclick={createFlow} disabled={creating || !form.id.trim() || !form.sourceId.trim() || (needsCodec && !form.codec.trim())}>
          {creating ? 'Creating...' : 'Create'}
        </button>
      </div>
    </div>
  {/if}

  <div class="filter-bar">
    <input type="text" bind:value={filterLabel} placeholder="Label" class="filter-input" />
    <select bind:value={filterFormat} class="filter-input">
      <option value="">Any format</option>
      {#each FORMAT_OPTIONS as opt}
        <option value={opt.value}>{opt.label}</option>
      {/each}
    </select>
    <input type="text" bind:value={filterCodec} placeholder="Codec" class="filter-input filter-sm" />
    <input type="text" bind:value={filterSourceId} placeholder="Source ID" class="filter-input filter-sm" />
    <input type="text" bind:value={filterTimerange} placeholder="Timerange" class="filter-input filter-sm" />
    <input type="text" bind:value={filterFrameWidth} placeholder="Width" class="filter-input filter-xs" />
    <input type="text" bind:value={filterFrameHeight} placeholder="Height" class="filter-input filter-xs" />
    <input type="text" bind:value={filterTagName} placeholder="Tag name" class="filter-input filter-sm" />
    <input type="text" bind:value={filterTagValue} placeholder="Tag value" class="filter-input filter-sm" />
    <button onclick={applyFilters}>Apply</button>
    <button onclick={clearFilters} class="btn-secondary">Clear</button>
  </div>

  {#if loading}
    <p class="muted"><Spinner /> Loading...</p>
  {:else if error}
    <p class="error-text">{error}</p>
  {:else if flows.length === 0}
    <p class="muted">No flows found.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Label</th>
          <th>ID</th>
          <th>Source</th>
          <th>Format</th>
          <th>Codec</th>
          <th>Timerange</th>
        </tr>
      </thead>
      <tbody>
        {#each flows as flow}
          {@const tr = formatTimerangeDisplay(flow.timerange)}
          <tr>
            <td><a href="/flows/{flow.id}" use:link>{flow.label || '--'}</a></td>
            <td class="mono">{flow.id.slice(0, 8)}</td>
            <td class="mono">{flow.source_id?.slice(0, 8) || '--'}</td>
            <td><span class="badge">{formatShortName(flow.format)}</span></td>
            <td class="mono">{flow.codec || '--'}</td>
            <td class="mono" style="font-size:0.75em" title={tr.display}><span class="timerange-raw">{tr.raw}</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
    <Pagination
      bind:this={paginationRef}
      count={pagination.count}
      limit={pagination.limit}
      nextKey={pagination.nextKey}
      onPage={handlePage}
    />
  {/if}
</div>

<style>
  .create-form {
    margin-bottom: 1em;
  }
  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5em 1em;
  }
  .form-grid label {
    display: flex;
    flex-direction: column;
    gap: 0.25em;
  }
  .id-row {
    display: flex;
    gap: 0.5em;
  }
  .id-row input {
    flex: 1;
  }
</style>
