<script lang="ts">
  import { untrack } from 'svelte';
  import { errorMessage } from '../lib/errors.js';
  import { apiGet, apiDelete, parsePagination } from '../lib/api.js';
  import { buildFlowQuery, buildSegmentsQuery } from '../lib/query.js';
  import { push } from '../lib/router.js';
  import { addToast } from '../lib/toast.js';
  import { FORMAT_VIDEO, FORMAT_AUDIO } from '../lib/ingest.js';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import Spinner from '../components/Spinner.svelte';
  import PropertiesTab from './flow/PropertiesTab.svelte';
  import SegmentsTab from './flow/SegmentsTab.svelte';
  import StorageTab from './flow/StorageTab.svelte';
  import type { PaginationInfo } from '../types/tams.js';

  let { params = {} }: { params?: Record<string, string> } = $props();
  let flow: any = $state(null);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);
  let activeTab: string = $state('properties');

  // Delete state
  let confirmDelete: boolean = $state(false);
  let deleting: boolean = $state(false);
  let deleteResult: any = $state(null);
  let deleteError: string | null = $state(null);

  // Initial segments (loaded with flow, passed to SegmentsTab)
  let initialSegments: any[] = $state([]);
  let initialSegPagination: Partial<PaginationInfo> = $state({});

  let loadedId: string | null = null;

  async function loadFlow(id: string): Promise<void> {
    loadedId = id;
    loading = true;
    error = null;
    flow = null;
    initialSegments = [];
    initialSegPagination = {};
    confirmDelete = false;
    deleteResult = null;
    deleteError = null;
    activeTab = 'properties';
    try {
      const [flowResp, segResp] = await Promise.all([
        apiGet(buildFlowQuery(id, { includeTimerange: true })),
        apiGet(buildSegmentsQuery(id, { limit: 50 })),
      ]);
      if (id !== loadedId) return;
      flow = flowResp.data;
      initialSegments = segResp.data;
      initialSegPagination = parsePagination(segResp.headers);
    } catch (e) {
      if (id !== loadedId) return;
      error = errorMessage(e);
    } finally {
      if (id === loadedId) loading = false;
    }
  }

  $effect(() => {
    const id: string | undefined = params.id;
    untrack(() => {
      if (id && id !== loadedId) loadFlow(id);
    });
  });

  async function deleteFlow(): Promise<void> {
    deleting = true;
    try {
      const result = await apiDelete(`/flows/${params.id}`);
      if (result.status === 202) {
        deleteResult = result.data;
        addToast('Flow deletion in progress', 'info');
      } else {
        addToast('Flow deleted', 'success');
        push('/flows');
      }
    } catch (e) {
      deleteError = errorMessage(e);
      confirmDelete = false;
    } finally {
      deleting = false;
    }
  }

  function handleFlowUpdate(updated: any): void {
    flow = updated;
  }
</script>

<div class="page">
  {#if loading}
    <p class="muted"><Spinner /> Loading...</p>
  {:else if error}
    <p class="error-text">{error}</p>
  {:else if deleteResult}
    <div class="panel">
      <h2>Flow Deletion In Progress</h2>
      <p class="muted">The flow has segments and is being deleted asynchronously.</p>
      <dl class="detail-grid">
        <dt>Request ID</dt>
        <dd class="mono">{deleteResult.id}</dd>
        <dt>Status</dt>
        <dd>{deleteResult.status || 'created'}</dd>
        {#if deleteResult.timerange_to_delete}
          <dt>Timerange to Delete</dt>
          <dd class="mono">{deleteResult.timerange_to_delete}</dd>
        {/if}
      </dl>
      <div style="margin-top:1em">
        <a href="#/flows">Back to Flows</a>
      </div>
    </div>
  {:else if flow}
    <div class="page-header">
      <h1>{flow.label || 'Flow'}</h1>
      <div style="display:flex;gap:0.5em">
        {#if flow.format === FORMAT_VIDEO || flow.format === FORMAT_AUDIO}
          <a href="#/player/{params.id}" class="btn-play">Play</a>
        {/if}
        <button class="danger" onclick={() => confirmDelete = true}>Delete</button>
      </div>
    </div>
    <ConfirmDialog
      open={confirmDelete}
      title="Delete Flow"
      message="Delete this flow?"
      confirmLabel="Delete"
      danger={true}
      loading={deleting}
      onConfirm={deleteFlow}
      onCancel={() => confirmDelete = false}
    />
    {#if deleteError}
      <p class="error-text" style="font-size:0.85em">{deleteError}</p>
    {/if}

    <div class="tabs">
      <button class:active={activeTab === 'properties'} onclick={() => activeTab = 'properties'}>Properties</button>
      <button class:active={activeTab === 'segments'} onclick={() => activeTab = 'segments'}>Segments</button>
      <button class:active={activeTab === 'storage'} onclick={() => activeTab = 'storage'}>Storage</button>
    </div>

    {#if activeTab === 'properties'}
      <PropertiesTab {flow} flowId={params.id} onFlowUpdate={handleFlowUpdate} />
    {:else if activeTab === 'segments'}
      <SegmentsTab flowId={loadedId} {initialSegments} initialPagination={initialSegPagination} />
    {:else if activeTab === 'storage'}
      <StorageTab flowId={loadedId} />
    {/if}
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    gap: 0;
    margin-bottom: 1em;
    border-bottom: 1px solid var(--border);
  }
  .tabs button {
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    color: var(--text-muted);
    padding: 0.5em 1em;
    font-size: 0.9em;
    cursor: pointer;
  }
  .tabs button.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
  .tabs button:hover {
    color: var(--text);
    background: transparent;
  }
  .btn-play {
    display: inline-block;
    padding: 0.4em 0.8em;
    background: var(--accent);
    color: #fff;
    text-decoration: none;
    border-radius: 3px;
    font-size: 0.85em;
  }
  .btn-play:hover {
    opacity: 0.85;
  }
</style>
