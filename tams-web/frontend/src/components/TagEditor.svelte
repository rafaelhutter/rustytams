<script lang="ts">
  import { apiPut, apiDelete } from '../lib/api.js';
  import { errorMessage } from '../lib/errors.js';

  let { tags = {}, basePath = '', onUpdate = () => {} }: {
    tags?: Record<string, string | string[]>;
    basePath?: string;
    onUpdate?: (newTags: Record<string, string | string[]>) => void;
  } = $props();

  let newKey: string = $state('');
  let newValue: string = $state('');
  let saving: string | null = $state(null);
  let error: string | null = $state(null);

  async function addTag(): Promise<void> {
    if (!newKey.trim()) return;
    const key: string = newKey.trim();
    const val: string | string[] = newValue.includes(',')
      ? newValue.split(',').map(v => v.trim()).filter(Boolean)
      : newValue.trim();
    saving = key;
    error = null;
    try {
      await apiPut(`${basePath}/tags/${encodeURIComponent(key)}`, val);
      onUpdate({ ...tags, [key]: val });
      newKey = '';
      newValue = '';
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = null;
    }
  }

  async function deleteTag(key: string): Promise<void> {
    saving = key;
    error = null;
    try {
      await apiDelete(`${basePath}/tags/${encodeURIComponent(key)}`);
      const updated: Record<string, string | string[]> = { ...tags };
      delete updated[key];
      onUpdate(updated);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = null;
    }
  }
</script>

<div class="tag-editor">
  <h3>Tags</h3>
  {#if error}
    <p class="error-text" style="font-size:0.85em">{error}</p>
  {/if}

  {#if Object.keys(tags).length > 0}
    <table>
      <thead><tr><th>Key</th><th>Value</th><th></th></tr></thead>
      <tbody>
        {#each Object.entries(tags) as [key, value]}
          <tr>
            <td class="mono">{key}</td>
            <td>{Array.isArray(value) ? value.join(', ') : value}</td>
            <td>
              <button
                class="btn-small btn-danger"
                onclick={() => deleteTag(key)}
                disabled={saving === key}
              >{saving === key ? '...' : 'X'}</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="muted" style="font-size:0.85em">No tags</p>
  {/if}

  <div class="add-row">
    <input type="text" bind:value={newKey} placeholder="Tag name" class="input-sm" />
    <input type="text" bind:value={newValue} placeholder="Value (comma-sep for multiple)" class="input-sm" />
    <button class="btn-small" onclick={addTag} disabled={!newKey.trim() || saving !== null}>Add</button>
  </div>
</div>

<style>
  .tag-editor { margin-top: 0.5em; }
  .add-row {
    display: flex;
    gap: 0.5em;
    margin-top: 0.5em;
    align-items: center;
  }
  .input-sm {
    flex: 1;
    font-size: 0.85em;
    padding: 0.25em 0.5em;
  }
</style>
