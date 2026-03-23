<script lang="ts">
  import { untrack } from 'svelte';
  import { errorMessage } from '../../lib/errors.js';
  import { apiPut, apiDelete, formatShortName } from '../../lib/api.js';
  import { formatTimerangeDisplay } from '../../lib/timerange.js';
  import TagEditor from '../../components/TagEditor.svelte';
  import type { Flow } from '../../types/tams.js';

  let { flow = null, flowId = '', onFlowUpdate = () => {} }: {
    flow?: any;
    flowId?: string;
    onFlowUpdate?: (updated: any) => void;
  } = $props();
  let flowTr = $derived(formatTimerangeDisplay(flow?.timerange));

  let editLabel: string = $state('');
  let editDesc: string = $state('');
  let editReadOnly: boolean = $state(false);
  let editMaxBitRate: string = $state('');
  let editAvgBitRate: string = $state('');
  let editFlowCollection: string = $state('');
  let saving: string | null = $state(null);
  let saveMsg: { text: string; ok: boolean } | null = $state(null);

  // Sync edit fields only when a new flow is loaded (not on field edits)
  let syncedFlowId: string | null = null;
  $effect(() => {
    const id: string | undefined = flow?.id;
    if (id && id !== syncedFlowId) {
      syncedFlowId = id;
      untrack(() => {
        editLabel = flow.label || '';
        editDesc = flow.description || '';
        editReadOnly = flow.read_only || false;
        editMaxBitRate = flow.max_bit_rate != null ? String(flow.max_bit_rate) : '';
        editAvgBitRate = flow.avg_bit_rate != null ? String(flow.avg_bit_rate) : '';
        editFlowCollection = (flow.flow_collection || []).map((item: any) => `${item.id}:${item.role}`).join(', ');
      });
    }
  });

  async function saveField(field: string): Promise<void> {
    saving = field;
    saveMsg = null;
    const path: string = `/flows/${flowId}/${field}`;
    try {
      let current: any, edited: any, value: any;
      switch (field) {
        case 'label':
          current = flow.label || '';
          edited = editLabel;
          value = edited;
          break;
        case 'description':
          current = flow.description || '';
          edited = editDesc;
          value = edited;
          break;
        case 'read_only':
          current = flow.read_only || false;
          edited = editReadOnly;
          value = edited;
          if (edited === current) { saveMsg = { text: 'No changes', ok: true }; return; }
          await apiPut(path, value);
          onFlowUpdate({ ...flow, [field]: edited });
          saveMsg = { text: 'Saved', ok: true };
          return;
        case 'max_bit_rate':
        case 'avg_bit_rate':
          current = flow[field] != null ? String(flow[field]) : '';
          edited = field === 'max_bit_rate' ? editMaxBitRate : editAvgBitRate;
          value = edited.trim() ? parseInt(edited) : null;
          if (edited.trim() === '' && current !== '') {
            await apiDelete(path);
            onFlowUpdate({ ...flow, [field]: undefined });
            saveMsg = { text: 'Saved', ok: true };
            return;
          }
          if (edited === current) { saveMsg = { text: 'No changes', ok: true }; return; }
          await apiPut(path, value);
          onFlowUpdate({ ...flow, [field]: value });
          saveMsg = { text: 'Saved', ok: true };
          return;
        case 'flow_collection':
          current = (flow.flow_collection || []).map((item: any) => `${item.id}:${item.role}`).join(', ');
          edited = editFlowCollection;
          value = edited.trim() ? edited.split(',').map((s: string) => {
            const [id, role] = s.trim().split(':');
            return { id: id?.trim(), role: (role?.trim()) || 'video' };
          }).filter((item: any) => item.id) : [];
          if (value.length === 0 && (flow.flow_collection || []).length > 0) {
            await apiDelete(path);
            onFlowUpdate({ ...flow, flow_collection: undefined });
            saveMsg = { text: 'Saved', ok: true };
            return;
          }
          if (edited === current) { saveMsg = { text: 'No changes', ok: true }; return; }
          await apiPut(path, value);
          onFlowUpdate({ ...flow, flow_collection: value });
          saveMsg = { text: 'Saved', ok: true };
          return;
        default:
          return;
      }
      // String fields: label, description
      if (edited.trim() === '' && current !== '') {
        await apiDelete(path);
        onFlowUpdate({ ...flow, [field]: undefined });
      } else if (edited !== current) {
        await apiPut(path, value);
        onFlowUpdate({ ...flow, [field]: edited });
      } else {
        saveMsg = { text: 'No changes', ok: true };
        return;
      }
      saveMsg = { text: 'Saved', ok: true };
    } catch (e) {
      saveMsg = { text: errorMessage(e), ok: false };
    } finally {
      saving = null;
    }
  }

  function handleTagUpdate(newTags: Record<string, string | string[]>): void {
    onFlowUpdate({ ...flow, tags: newTags });
  }
</script>

<div class="panel" style="margin-bottom:1em">
  <dl class="detail-grid">
    <dt>ID</dt>
    <dd class="mono">{flow.id}</dd>
    <dt>Source</dt>
    <dd class="mono"><a href="#/sources/{flow.source_id}">{flow.source_id}</a></dd>
    <dt>Format</dt>
    <dd><span class="badge">{formatShortName(flow.format)}</span></dd>
    <dt>Codec</dt>
    <dd class="mono">{flow.codec || '--'}</dd>
    <dt>Timerange</dt>
    <dd class="mono"><span class="timerange-raw">{flowTr.raw}</span><br><span class="muted" style="font-size:0.8em">{flowTr.display}</span></dd>
    {#if flow.container}
      <dt>Container</dt>
      <dd class="mono">{flow.container}</dd>
    {/if}
    {#if flow.generation != null}
      <dt>Generation</dt>
      <dd>{flow.generation}</dd>
    {/if}
    {#if flow.flow_collection?.length}
      <dt>Flow Collection</dt>
      <dd>
        {#each flow.flow_collection as item, i}
          <a href="#/flows/{item.id}" class="mono">{item.id.slice(0, 8)}</a> <span class="muted">({item.role})</span>{#if i < flow.flow_collection.length - 1}, {/if}
        {/each}
      </dd>
    {/if}
    {#if flow.collected_by?.length}
      <dt>Collected By</dt>
      <dd>
        {#each flow.collected_by as cid, i}
          <a href="#/flows/{cid}" class="mono">{cid.slice(0, 8)}</a>{#if i < flow.collected_by.length - 1}, {/if}
        {/each}
      </dd>
    {/if}
  </dl>

  <div class="edit-section">
    <label>
      <span class="label-text">Label</span>
      <input type="text" bind:value={editLabel} placeholder="Flow label" />
      <button class="btn-small" onclick={() => saveField('label')} disabled={saving === 'label'}>
        {saving === 'label' ? '...' : 'Save'}
      </button>
    </label>
    <label>
      <span class="label-text">Description</span>
      <input type="text" bind:value={editDesc} placeholder="Flow description" />
      <button class="btn-small" onclick={() => saveField('description')} disabled={saving === 'description'}>
        {saving === 'description' ? '...' : 'Save'}
      </button>
    </label>
    <label>
      <span class="label-text">Read Only</span>
      <div class="checkbox-row">
        <input type="checkbox" bind:checked={editReadOnly} />
        <button class="btn-small" onclick={() => saveField('read_only')} disabled={saving === 'read_only'}>
          {saving === 'read_only' ? '...' : 'Save'}
        </button>
      </div>
    </label>
    <label>
      <span class="label-text">Max Bit Rate</span>
      <input type="number" bind:value={editMaxBitRate} placeholder="kbps (1000 bits/s)" min="0" />
      <button class="btn-small" onclick={() => saveField('max_bit_rate')} disabled={saving === 'max_bit_rate'}>
        {saving === 'max_bit_rate' ? '...' : 'Save'}
      </button>
    </label>
    <label>
      <span class="label-text">Avg Bit Rate</span>
      <input type="number" bind:value={editAvgBitRate} placeholder="kbps (1000 bits/s)" min="0" />
      <button class="btn-small" onclick={() => saveField('avg_bit_rate')} disabled={saving === 'avg_bit_rate'}>
        {saving === 'avg_bit_rate' ? '...' : 'Save'}
      </button>
    </label>
    <label>
      <span class="label-text">Flow Collection</span>
      <input type="text" bind:value={editFlowCollection} placeholder="Comma-separated flow UUIDs" />
      <button class="btn-small" onclick={() => saveField('flow_collection')} disabled={saving === 'flow_collection'}>
        {saving === 'flow_collection' ? '...' : 'Save'}
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
  <TagEditor tags={flow.tags || {}} basePath={`/flows/${flowId}`} onUpdate={handleTagUpdate} />
</div>

{#if flow.essence_parameters && Object.keys(flow.essence_parameters).length > 0}
  <div class="panel" style="margin-bottom:1em">
    <h3>Essence Parameters</h3>
    <pre class="json-display">{JSON.stringify(flow.essence_parameters, null, 2)}</pre>
  </div>
{/if}

{#if flow.container_mapping && Object.keys(flow.container_mapping).length > 0}
  <div class="panel" style="margin-bottom:1em">
    <h3>Container Mapping</h3>
    <pre class="json-display">{JSON.stringify(flow.container_mapping, null, 2)}</pre>
  </div>
{/if}

<style>
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 0.5em;
  }
  .json-display {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.75em;
    font-family: var(--mono);
    font-size: 0.8em;
    overflow-x: auto;
    margin: 0;
  }
</style>
