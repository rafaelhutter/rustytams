<script module lang="ts">
  // Module-level cache so tab switches don't re-fetch backends
  let _cachedBackends: any[] | null = null;
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage } from '../../lib/errors.js';
  import { apiGet, apiPost } from '../../lib/api.js';

  let { flowId = '' }: { flowId?: string } = $props();

  let storageMode: string = $state('limit');
  let storageLimit: string = $state('5');
  let storageObjectIds: string = $state('');
  let storageBackendId: string = $state('');
  let backends: any[] = $state([]);
  let allocating: boolean = $state(false);
  let allocError: string | null = $state(null);
  let allocResult: any = $state(null);

  onMount(() => {
    if (_cachedBackends) {
      backends = _cachedBackends;
    } else {
      apiGet('/service/storage-backends').then((r: any) => {
        _cachedBackends = r.data;
        backends = r.data;
      }).catch(() => {});
    }
  });

  async function allocateStorage(): Promise<void> {
    allocating = true;
    allocError = null;
    allocResult = null;
    try {
      const body: Record<string, any> = {};
      if (storageMode === 'limit') {
        const n: number = parseInt(storageLimit);
        if (!n || n < 1) { allocError = 'Limit must be at least 1'; return; }
        body.limit = n;
      } else {
        const ids: string[] = storageObjectIds.split('\n').map(s => s.trim()).filter(Boolean);
        if (ids.length === 0) { allocError = 'Provide at least one object ID'; return; }
        body.object_ids = ids;
      }
      if (storageBackendId) body.storage_id = storageBackendId;
      const result = await apiPost(`/flows/${flowId}/storage`, body);
      allocResult = result.data;
    } catch (e) {
      allocError = errorMessage(e);
    } finally {
      allocating = false;
    }
  }
</script>

<div class="panel" style="margin-bottom:1em">
  <h3>Allocate Storage</h3>
  {#if allocError}
    <p class="error-text" style="font-size:0.85em">{allocError}</p>
  {/if}

  <div class="form-row" style="margin-bottom:0.5em">
    <label class="storage-check">
      <input type="radio" name="storage-mode" value="limit" bind:group={storageMode} /> By count
    </label>
    <label class="storage-check">
      <input type="radio" name="storage-mode" value="object_ids" bind:group={storageMode} /> By object IDs
    </label>
  </div>

  {#if storageMode === 'limit'}
    <label style="display:flex;align-items:center;gap:0.5em;margin-bottom:0.5em">
      <span class="label-text">Limit</span>
      <input type="number" bind:value={storageLimit} min="1" style="max-width:6em" />
    </label>
  {:else}
    <label style="display:flex;flex-direction:column;gap:0.25em;margin-bottom:0.5em">
      <span class="label-text">Object IDs (one per line)</span>
      <textarea bind:value={storageObjectIds} rows="4" class="mono" style="font-size:0.85em;resize:vertical"></textarea>
    </label>
  {/if}

  {#if backends.length > 0}
    <label style="display:flex;align-items:center;gap:0.5em;margin-bottom:0.5em">
      <span class="label-text">Storage</span>
      <select bind:value={storageBackendId}>
        <option value="">Default</option>
        {#each backends as b}
          <option value={b.id}>{b.label || b.id?.slice(0, 8)} ({b.store_type || '--'})</option>
        {/each}
      </select>
    </label>
  {/if}

  <button class="primary" onclick={allocateStorage} disabled={allocating}>
    {allocating ? 'Allocating...' : 'Allocate'}
  </button>

  {#if allocResult?.media_objects?.length}
    <div style="margin-top:1em">
      <h3>Allocated Objects</h3>
      <table>
        <thead><tr><th>Object ID</th><th>PUT URL</th><th>Content-Type</th></tr></thead>
        <tbody>
          {#each allocResult.media_objects as mo}
            <tr>
              <td class="mono">{mo.object_id?.slice(0, 8) || '--'}</td>
              <td class="mono" style="font-size:0.75em;max-width:20em;overflow:hidden;text-overflow:ellipsis">
                {mo.put_url?.url || '--'}
              </td>
              <td class="mono">{mo.put_url?.['content-type'] || '--'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .form-row {
    display: flex;
    gap: 0.75em;
    flex-wrap: wrap;
  }
  .storage-check {
    display: flex;
    align-items: center;
    gap: 0.25em;
    font-size: 0.85em;
    color: var(--text-muted);
  }
</style>
