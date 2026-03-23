<script lang="ts">
  import { untrack } from 'svelte';
  import { errorMessage } from '../lib/errors.js';
  import { apiGet, apiPut, apiDelete, formatShortName } from '../lib/api.js';
  import { buildFlowsQuery } from '../lib/query.js';
  import { push } from '../lib/router.js';
  import { addToast } from '../lib/toast.js';
  import TagEditor from '../components/TagEditor.svelte';
  import Spinner from '../components/Spinner.svelte';
  import type { Flow } from '../types/tams.js';

  let { params = {} }: { params?: Record<string, string> } = $props();
  let source: any = $state(null);
  let flows: any[] = $state([]);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);

  // Editable fields
  let editLabel: string = $state('');
  let editDesc: string = $state('');
  let saving: string | null = $state(null);
  let saveMsg: { text: string; ok: boolean } | null = $state(null);

  let loadedId: string | null = null;

  async function loadSource(id: string): Promise<void> {
    loadedId = id;
    loading = true;
    error = null;
    source = null;
    flows = [];
    try {
      const [srcResp, flowResp] = await Promise.all([
        apiGet(`/sources/${id}`),
        apiGet(buildFlowsQuery({ sourceId: id, limit: 100 })),
      ]);
      if (id !== loadedId) return;
      source = srcResp.data;
      flows = flowResp.data;
      editLabel = source.label || '';
      editDesc = source.description || '';
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
      if (id && id !== loadedId) loadSource(id);
    });
  });

  async function saveField(field: string): Promise<void> {
    saving = field;
    saveMsg = null;
    const path: string = `/sources/${params.id}/${field}`;
    const current: string = field === 'label' ? (source.label || '') : (source.description || '');
    const edited: string = field === 'label' ? editLabel : editDesc;
    try {
      if (edited.trim() === '' && current !== '') {
        await apiDelete(path);
        source = { ...source, [field]: undefined };
      } else if (edited !== current) {
        await apiPut(path, edited);
        source = { ...source, [field]: edited };
      } else {
        saveMsg = { text: 'No changes', ok: true };
        saving = null;
        return;
      }
      saveMsg = { text: 'Saved', ok: true };
      addToast('Saved', 'success');
    } catch (e) {
      saveMsg = { text: errorMessage(e), ok: false };
    } finally {
      saving = null;
    }
  }

  // Delete source

  function handleTagUpdate(newTags: Record<string, string | string[]>): void {
    source = { ...source, tags: newTags };
  }
</script>

<div class="page">
  {#if loading}
    <p class="muted"><Spinner /> Loading...</p>
  {:else if error}
    <p class="error-text">{error}</p>
  {:else if source}
    <div class="page-header">
      <h1>{source.label || 'Source'}</h1>
    </div>
    <div class="panel" style="margin-bottom:1em">
      <dl class="detail-grid">
        <dt>ID</dt>
        <dd class="mono">{source.id}</dd>
        <dt>Format</dt>
        <dd><span class="badge">{formatShortName(source.format)}</span></dd>
      </dl>

      <div class="edit-section">
        <label>
          <span class="label-text">Label</span>
          <input type="text" bind:value={editLabel} placeholder="Source label" />
          <button class="btn-small" onclick={() => saveField('label')} disabled={saving === 'label'}>
            {saving === 'label' ? '...' : 'Save'}
          </button>
        </label>
        <label>
          <span class="label-text">Description</span>
          <input type="text" bind:value={editDesc} placeholder="Source description" />
          <button class="btn-small" onclick={() => saveField('description')} disabled={saving === 'description'}>
            {saving === 'description' ? '...' : 'Save'}
          </button>
        </label>
        {#if saveMsg}
          <span class={saveMsg.ok ? 'success-text' : 'error-text'} style="font-size:0.85em">
            {saveMsg.text}
          </span>
        {/if}
      </div>
    </div>

    <div class="panel" style="margin-bottom:1em">
      <TagEditor tags={source.tags || {}} basePath={`/sources/${params.id}`} onUpdate={handleTagUpdate} />
    </div>

    <h2>Flows ({flows.length})</h2>
    {#if flows.length > 0}
      <table>
        <thead><tr><th>Label</th><th>Format</th><th>Codec</th></tr></thead>
        <tbody>
          {#each flows as flow}
            <tr>
              <td><a href="#/flows/{flow.id}">{flow.label || flow.id.slice(0, 8)}</a></td>
              <td><span class="badge">{formatShortName(flow.format)}</span></td>
              <td class="mono">{flow.codec || '--'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="muted">No flows for this source.</p>
    {/if}
  {/if}
</div>
