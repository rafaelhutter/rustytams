<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage } from '../lib/errors.js';
  import { apiGet, apiDelete, parsePagination, formatShortName } from '../lib/api.js';
  import { createFlowWithSource, FORMAT_VIDEO, FORMAT_OPTIONS } from '../lib/ingest.js';
  import { link, getHashParams, setHashParams } from '../lib/router.js';
  import { buildSourcesQuery } from '../lib/query.js';
  import { addToast } from '../lib/toast.js';
  import Pagination from '../components/Pagination.svelte';
  import Spinner from '../components/Spinner.svelte';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import type { PaginationInfo } from '../types/tams.js';

  let sources: any[] = $state([]);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);
  let pagination: PaginationInfo = $state({ limit: null, nextKey: null, count: null, timerange: null });

  // Delete confirmation
  let deleteTarget: { id: string; label: string } | null = $state(null);
  let deleting: boolean = $state(false);

  async function confirmDelete(): Promise<void> {
    if (!deleteTarget) return;
    deleting = true;
    try {
      await apiDelete(`/sources/${deleteTarget.id}`);
      addToast(`Source "${deleteTarget.label || deleteTarget.id}" deleted`, 'success');
      deleteTarget = null;
      fetchSources();
    } catch (e) {
      addToast(`Delete failed: ${errorMessage(e)}`, 'error');
    } finally {
      deleting = false;
    }
  }

  // Filters
  let filterLabel: string = $state('');
  let filterFormat: string = $state('');
  let filterTagName: string = $state('');
  let filterTagValue: string = $state('');

  // Create source form
  let showCreate: boolean = $state(false);
  const defaultForm: { id: string; format: string; label: string; desc: string } = { id: '', format: FORMAT_VIDEO, label: '', desc: '' };
  let form: { id: string; format: string; label: string; desc: string } = $state({ ...defaultForm });
  let creating: boolean = $state(false);
  let createError: string | null = $state(null);

  function generateId(): void {
    form.id = crypto.randomUUID();
  }

  async function createSource(): Promise<void> {
    if (!form.id.trim() || !form.format) return;
    creating = true;
    createError = null;
    try {
      const sourceId: string = form.id.trim();
      await createFlowWithSource({
        sourceId,
        flowId: crypto.randomUUID(),
        format: form.format,
        sourceLabel: form.label.trim() || undefined,
        sourceDescription: form.desc.trim() || undefined,
      });
      showCreate = false;
      form = { ...defaultForm };
      createError = null;
      addToast('Source created', 'success');
      fetchSources();
    } catch (e) {
      createError = errorMessage(e);
    } finally {
      creating = false;
    }
  }

  let paginationRef: Pagination | undefined = $state();

  async function fetchSources(pageKey: string | null = null): Promise<void> {
    loading = true;
    error = null;
    try {
      const query: string = buildSourcesQuery(
        { label: filterLabel, format: filterFormat, tagName: filterTagName, tagValue: filterTagValue },
        pageKey,
      );
      const { data, headers } = await apiGet(query);
      sources = data;
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
    if (filterTagName) p.set('tagName', filterTagName);
    if (filterTagValue) p.set('tagValue', filterTagValue);
    setHashParams(p);
    paginationRef?.reset();
    fetchSources();
  }

  function clearFilters(): void {
    filterLabel = '';
    filterFormat = '';
    filterTagName = '';
    filterTagValue = '';
    setHashParams(new URLSearchParams());
    paginationRef?.reset();
    fetchSources();
  }

  function handlePage({ key }: { key: string | null }): void {
    fetchSources(key);
  }

  onMount(() => {
    const p: URLSearchParams = getHashParams();
    if (p.has('label')) filterLabel = p.get('label')!;
    if (p.has('format')) filterFormat = p.get('format')!;
    if (p.has('tagName')) filterTagName = p.get('tagName')!;
    if (p.has('tagValue')) filterTagValue = p.get('tagValue')!;
    fetchSources();
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
    <h1>Sources</h1>
    <button class="primary" onclick={() => { showCreate = !showCreate; createError = null; if (showCreate && !form.id) generateId(); }}>
      {showCreate ? 'Cancel' : '+ Create Source'}
    </button>
  </div>

  {#if showCreate}
    <div class="panel" style="margin-bottom:1em">
      <h3>Create Source</h3>
      {#if createError}
        <p class="error-text" style="font-size:0.85em">{createError}</p>
      {/if}
      <div class="create-form">
        <label>
          <span class="label-text">ID</span>
          <div style="display:flex;gap:0.5em">
            <input type="text" bind:value={form.id} placeholder="UUID" style="flex:1" />
            <button class="btn-small" onclick={generateId}>Generate</button>
          </div>
        </label>
        <label>
          <span class="label-text">Format</span>
          <select bind:value={form.format}>
            {#each FORMAT_OPTIONS as opt}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </label>
        <label>
          <span class="label-text">Label</span>
          <input type="text" bind:value={form.label} placeholder="Optional label" />
        </label>
        <label>
          <span class="label-text">Description</span>
          <input type="text" bind:value={form.desc} placeholder="Optional description" />
        </label>
        <button class="primary" onclick={createSource} disabled={creating || !form.id.trim()}>
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
    <input type="text" bind:value={filterTagName} placeholder="Tag name" class="filter-input filter-sm" />
    <input type="text" bind:value={filterTagValue} placeholder="Tag value" class="filter-input filter-sm" />
    <button onclick={applyFilters}>Apply</button>
    <button onclick={clearFilters} class="btn-secondary">Clear</button>
  </div>

  {#if loading}
    <p class="muted"><Spinner /> Loading...</p>
  {:else if error}
    <p class="error-text">{error}</p>
  {:else if sources.length === 0}
    <p class="muted">No sources found.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Label</th>
          <th>ID</th>
          <th>Format</th>
          <th>Description</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each sources as src}
          <tr>
            <td><a href="/sources/{src.id}" use:link>{src.label || '--'}</a></td>
            <td class="mono">{src.id.slice(0, 8)}</td>
            <td><span class="badge">{formatShortName(src.format)}</span></td>
            <td class="muted">{src.description || '--'}</td>
            <td>
              <button class="btn-danger-sm" onclick={() => deleteTarget = { id: src.id, label: src.label || '' }} title="Delete source">✕</button>
            </td>
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

{#if deleteTarget}
  <ConfirmDialog
    title="Delete Source"
    message="Delete source &quot;{deleteTarget.label || deleteTarget.id}&quot; and all its flows? This cannot be undone."
    confirmLabel={deleting ? 'Deleting...' : 'Delete'}
    disabled={deleting}
    onConfirm={confirmDelete}
    onCancel={() => deleteTarget = null}
  />
{/if}

<style>
  .create-form {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
    margin-top: 0.75em;
  }
  .create-form label {
    display: flex;
    flex-direction: column;
    gap: 0.2em;
  }
  :global(.btn-danger-sm) {
    background: transparent;
    border: 1px solid var(--error, #c0392b);
    color: var(--error, #c0392b);
    border-radius: 4px;
    padding: 0.15em 0.45em;
    font-size: 0.8em;
    cursor: pointer;
    line-height: 1.2;
  }
  :global(.btn-danger-sm:hover) {
    background: var(--error, #c0392b);
    color: #fff;
  }
</style>
