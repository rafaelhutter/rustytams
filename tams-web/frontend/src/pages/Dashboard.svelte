<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage } from '../lib/errors.js';
  import { apiGet, apiPost, parsePagination, formatShortName } from '../lib/api.js';
  import { buildSourcesQuery, buildFlowsQuery, buildWebhooksQuery } from '../lib/query.js';
  import { formatTimerangeDisplay } from '../lib/timerange.js';
  import Spinner from '../components/Spinner.svelte';
  import type { Source, Flow, PaginationInfo } from '../types/tams.js';

  let service: any = $state(null);
  let sources: any[] = $state([]);
  let flows: any[] = $state([]);
  let backends: any[] = $state([]);
  let sourceCount: number = $state(0);
  let flowCount: number = $state(0);
  let webhookCount: number = $state(0);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);

  // Editable service fields
  let editName: string = $state('');
  let editDesc: string = $state('');
  let saving: boolean = $state(false);
  let saveMsg: { text: string; ok: boolean } | null = $state(null);

  onMount(async () => {
    try {
      const [svcResp, srcResp, flowResp, backendResp, whResp] = await Promise.all([
        apiGet('/service'),
        apiGet(buildSourcesQuery({ limit: 5 })),
        apiGet(buildFlowsQuery({ limit: 5 })),
        apiGet('/service/storage-backends').catch(() => ({ data: [] })),
        apiGet(buildWebhooksQuery({ limit: 1 })).catch(() => ({ data: [], headers: new Headers() })),
      ]);
      service = svcResp.data;
      sources = srcResp.data;
      flows = flowResp.data;
      backends = backendResp.data;
      sourceCount = parsePagination(srcResp.headers).count ?? sources.length;
      flowCount = parsePagination(flowResp.headers).count ?? flows.length;
      webhookCount = parsePagination(whResp.headers).count ?? whResp.data?.length ?? 0;
      editName = service.name || '';
      editDesc = service.description || '';
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  });

  async function saveService(): Promise<void> {
    saving = true;
    saveMsg = null;
    try {
      const body: Record<string, string> = {};
      if (editName !== (service.name || '')) body.name = editName;
      if (editDesc !== (service.description || '')) body.description = editDesc;
      if (Object.keys(body).length === 0) { saveMsg = { text: 'No changes', ok: true }; saving = false; return; }
      const { data } = await apiPost('/service', body);
      service = data;
      saveMsg = { text: 'Saved', ok: true };
    } catch (e) {
      saveMsg = { text: errorMessage(e), ok: false };
    } finally {
      saving = false;
    }
  }
</script>

<div class="page">
  <h1>Dashboard</h1>

  {#if loading}
    <p class="muted"><Spinner /> Loading...</p>
  {:else if error}
    <p class="error-text">{error}</p>
  {:else}
    <div class="grid">
      <div class="panel">
        <h3>Service</h3>
        {#if service}
          <dl class="detail-grid">
            <dt>Type</dt>
            <dd class="mono">{service.type || '--'}</dd>
            <dt>API Version</dt>
            <dd>{service.api_version || '--'}</dd>
            {#if service.service_version}
              <dt>Version</dt>
              <dd>{service.service_version}</dd>
            {/if}
            <dt>Min Object Timeout</dt>
            <dd class="mono">{service.min_object_timeout || '--'}</dd>
          </dl>

          <div class="edit-section">
            <label>
              <span class="label-text">Name</span>
              <input type="text" bind:value={editName} placeholder="Service name" />
            </label>
            <label>
              <span class="label-text">Description</span>
              <input type="text" bind:value={editDesc} placeholder="Service description" />
            </label>
            <div class="edit-actions">
              <button onclick={saveService} disabled={saving}>{saving ? 'Saving...' : 'Save'}</button>
              {#if saveMsg}
                <span class={saveMsg.ok ? 'success-text' : 'error-text'}>{saveMsg.text}</span>
              {/if}
            </div>
          </div>
        {:else}
          <p class="muted">No service info</p>
        {/if}
      </div>

      <div class="panel">
        <h3><a href="#/sources">Sources</a></h3>
        <div class="stat">{sourceCount}</div>
        {#if sources.length > 0}
          <table>
            <thead><tr><th>Label</th><th>Format</th></tr></thead>
            <tbody>
              {#each sources as src}
                <tr>
                  <td><a href="#/sources/{src.id}">{src.label || src.id.slice(0, 8)}</a></td>
                  <td class="mono badge">{formatShortName(src.format)}</td>
                </tr>
              {/each}
              {#if sourceCount > sources.length}
                <tr><td colspan="2" class="muted">+{sourceCount - sources.length} more</td></tr>
              {/if}
            </tbody>
          </table>
        {:else}
          <p class="muted">No sources</p>
        {/if}
      </div>

      <div class="panel">
        <h3><a href="#/flows">Flows</a></h3>
        <div class="stat">{flowCount}</div>
        {#if flows.length > 0}
          <table>
            <thead><tr><th>Label</th><th>Timerange</th></tr></thead>
            <tbody>
              {#each flows as flow}
                {@const tr = formatTimerangeDisplay(flow.timerange)}
                <tr>
                  <td><a href="#/flows/{flow.id}">{flow.label || flow.id.slice(0, 8)}</a></td>
                  <td class="mono" style="font-size:0.75em" title={tr.display}><span class="timerange-raw">{tr.raw}</span></td>
                </tr>
              {/each}
              {#if flowCount > flows.length}
                <tr><td colspan="2" class="muted">+{flowCount - flows.length} more</td></tr>
              {/if}
            </tbody>
          </table>
        {:else}
          <p class="muted">No flows</p>
        {/if}
      </div>

      <div class="panel">
        <h3><a href="#/webhooks">Webhooks</a></h3>
        <div class="stat">{webhookCount}</div>
      </div>
    </div>

    {#if sourceCount === 0 && flowCount === 0}
      <div class="panel get-started">
        <h3>Get Started</h3>
        <p>No media in the system yet. Record from your webcam or upload a video file to get started.</p>
        <div class="get-started-actions">
          <a href="#/record?mode=webcam" class="btn primary">Record from Webcam</a>
          <a href="#/record?mode=upload" class="btn">Upload Video</a>
        </div>
      </div>
    {/if}

    {#if backends.length > 0}
      <div class="panel" style="margin-top:1em">
        <h3>Storage Backends</h3>
        <table>
          <thead>
            <tr>
              <th>ID</th>
              <th>Type</th>
              <th>Provider</th>
              <th>Product</th>
              <th>Label</th>
              <th>Default</th>
            </tr>
          </thead>
          <tbody>
            {#each backends as b}
              <tr>
                <td class="mono">{b.id?.slice(0, 8) || '--'}</td>
                <td>{b.store_type || '--'}</td>
                <td>{b.provider || '--'}</td>
                <td>{b.store_product || '--'}</td>
                <td>{b.label || '--'}</td>
                <td>{b.default_storage ? 'Yes' : '--'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 1em;
  }
  .stat {
    font-size: 2em;
    font-weight: 600;
    color: var(--accent);
    margin-bottom: 0.5em;
  }
  .get-started {
    margin-top: 1em;
    text-align: center;
    padding: 2em;
  }
  .get-started p {
    color: var(--text-muted);
    margin: 0.75em 0 1.5em;
  }
  .get-started-actions {
    display: flex;
    gap: 1em;
    justify-content: center;
  }
  .get-started-actions .btn {
    display: inline-block;
    padding: 0.6em 1.5em;
    border-radius: 4px;
    text-decoration: none;
    font-size: 0.95em;
    border: 1px solid var(--border);
    color: var(--text);
    background: var(--panel);
  }
  .get-started-actions .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .get-started-actions .btn:hover {
    opacity: 0.85;
  }
  .grid .panel {
    min-width: 0;
    overflow: hidden;
  }
  .grid .panel td {
    overflow-wrap: anywhere;
  }
</style>
