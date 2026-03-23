<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage } from '../lib/errors.js';
  import { apiGet, apiPost, apiPut, apiDelete, parsePagination } from '../lib/api.js';
  import { buildWebhooksQuery, WEBHOOK_EVENTS } from '../lib/query.js';
  import { addToast } from '../lib/toast.js';
  import Pagination from '../components/Pagination.svelte';
  import TagEditor from '../components/TagEditor.svelte';
  import Spinner from '../components/Spinner.svelte';
  import { getHashParams, setHashParams } from '../lib/router.js';
  import type { PaginationInfo } from '../types/tams.js';

  interface WebhookForm {
    url: string;
    events: string[];
    status: string;
    apiKeyName: string;
    apiKeyValue: string;
    flowIds: string;
    sourceIds: string;
    flowCollectedByIds: string;
    sourceCollectedByIds: string;
  }

  let webhooks: any[] = $state([]);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);
  let pagination: PaginationInfo = $state({ limit: null, nextKey: null, count: null, timerange: null });
  let paginationRef: Pagination | undefined = $state();

  // Filters
  let tagName: string = $state('');
  let tagValue: string = $state('');

  // Create form
  let showCreate: boolean = $state(false);
  let createForm: WebhookForm = $state(defaultCreateForm());
  let creating: boolean = $state(false);
  let createError: string | null = $state(null);

  // Edit state
  let editingId: string | null = $state(null);
  let editForm: WebhookForm | null = $state(null);
  let saving: boolean = $state(false);
  let saveError: string | null = $state(null);

  // Delete state
  let confirmDeleteId: string | null = $state(null);
  let deletingId: string | null = $state(null);
  let deleteError: string | null = $state(null);

  function defaultCreateForm(): WebhookForm {
    return {
      url: '',
      events: [],
      status: 'created',
      apiKeyName: '',
      apiKeyValue: '',
      flowIds: '',
      sourceIds: '',
      flowCollectedByIds: '',
      sourceCollectedByIds: '',
    };
  }

  async function fetchWebhooks(pageKey: string | null = null): Promise<void> {
    loading = true;
    error = null;
    deleteError = null;
    try {
      const path: string = buildWebhooksQuery({ tagName, tagValue }, pageKey);
      const resp = await apiGet(path);
      webhooks = resp.data;
      pagination = parsePagination(resp.headers);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      loading = false;
    }
  }

  function applyFilters(): void {
    const p = new URLSearchParams();
    if (tagName) p.set('tagName', tagName);
    if (tagValue) p.set('tagValue', tagValue);
    setHashParams(p);
    paginationRef?.reset();
    fetchWebhooks();
  }

  function clearFilters(): void {
    tagName = '';
    tagValue = '';
    setHashParams(new URLSearchParams());
    paginationRef?.reset();
    fetchWebhooks();
  }

  function handlePage({ key }: { key: string | null }): void {
    fetchWebhooks(key);
  }

  // Initial load
  onMount(() => {
    const p: URLSearchParams = getHashParams();
    if (p.has('tagName')) tagName = p.get('tagName')!;
    if (p.has('tagValue')) tagValue = p.get('tagValue')!;
    fetchWebhooks();
  });

  // -- Toggle event in an events array --
  function toggleEventIn(form: WebhookForm, evt: string): void {
    form.events = form.events.includes(evt)
      ? form.events.filter(e => e !== evt)
      : [...form.events, evt];
  }

  // -- Split newline/comma-separated UUIDs --
  function splitIds(str: string): string[] {
    return str.split(/[\n,]/).map(s => s.trim()).filter(Boolean);
  }

  // -- Build webhook request body from form state --
  function buildWebhookBody(form: WebhookForm): Record<string, any> {
    const body: Record<string, any> = {
      url: form.url.trim(),
      events: form.events,
      status: form.status,
    };
    if (form.apiKeyName.trim()) body.api_key_name = form.apiKeyName.trim();
    if (form.apiKeyValue.trim()) body.api_key_value = form.apiKeyValue.trim();
    for (const [formKey, bodyKey] of [
      ['flowIds', 'flow_ids'],
      ['sourceIds', 'source_ids'],
      ['flowCollectedByIds', 'flow_collected_by_ids'],
      ['sourceCollectedByIds', 'source_collected_by_ids'],
    ] as const) {
      const ids: string[] = splitIds(form[formKey]);
      if (ids.length) body[bodyKey] = ids;
    }
    return body;
  }

  // -- Create --
  async function createWebhook(): Promise<void> {
    if (!createForm.url.trim() || createForm.events.length === 0) return;
    creating = true;
    createError = null;
    try {
      const body: Record<string, any> = buildWebhookBody(createForm);
      await apiPost('/service/webhooks', body);
      showCreate = false;
      createForm = defaultCreateForm();
      addToast('Webhook created', 'success');
      fetchWebhooks();
    } catch (e) {
      createError = errorMessage(e);
    } finally {
      creating = false;
    }
  }

  // -- Edit --
  function startEdit(wh: any): void {
    editingId = wh.id;
    saveError = null;
    editForm = {
      url: wh.url || '',
      events: [...(wh.events || [])],
      status: (wh.status === 'error' || wh.status === 'started') ? 'created' : wh.status,
      apiKeyName: wh.api_key_name || '',
      apiKeyValue: '',
      flowIds: (wh.flow_ids || []).join('\n'),
      sourceIds: (wh.source_ids || []).join('\n'),
      flowCollectedByIds: (wh.flow_collected_by_ids || []).join('\n'),
      sourceCollectedByIds: (wh.source_collected_by_ids || []).join('\n'),
    };
  }

  function cancelEdit(): void {
    editingId = null;
    editForm = null;
    saveError = null;
  }

  async function saveEdit(): Promise<void> {
    if (!editForm || !editForm.url.trim() || editForm.events.length === 0) return;
    saving = true;
    saveError = null;
    try {
      const body: Record<string, any> = buildWebhookBody(editForm);
      body.id = editingId;
      await apiPut(`/service/webhooks/${editingId}`, body);
      editingId = null;
      editForm = null;
      addToast('Webhook saved', 'success');
      fetchWebhooks();
    } catch (e) {
      saveError = errorMessage(e);
    } finally {
      saving = false;
    }
  }

  // -- Delete --
  async function deleteWebhook(id: string): Promise<void> {
    deletingId = id;
    deleteError = null;
    try {
      await apiDelete(`/service/webhooks/${id}`);
      confirmDeleteId = null;
      addToast('Webhook deleted', 'success');
      fetchWebhooks();
    } catch (e) {
      deleteError = errorMessage(e);
      confirmDeleteId = null;
    } finally {
      deletingId = null;
    }
  }

  function statusClass(status: string | undefined): string {
    switch (status) {
      case 'started': return 'success-text';
      case 'error': return 'error-text';
      case 'disabled': return 'muted';
      default: return '';
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      if (editingId) { cancelEdit(); }
      else if (confirmDeleteId) { confirmDeleteId = null; }
      else if (showCreate) { showCreate = false; createError = null; }
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="page">
  <div class="page-header">
    <h1>Webhooks</h1>
    <button class="primary" onclick={() => { showCreate = !showCreate; createError = null; }}>
      {showCreate ? 'Cancel' : '+ New Webhook'}
    </button>
  </div>

  {#if deleteError}
    <p class="error-text" style="font-size:0.85em">{deleteError}</p>
  {/if}

  <!-- Create form -->
  {#if showCreate}
    <div class="panel" style="margin-bottom:1em">
      <h3>Register Webhook</h3>
      {#if createError}
        <p class="error-text" style="font-size:0.85em">{createError}</p>
      {/if}
      <div class="wh-form">
        <label>
          <span class="label-text">URL *</span>
          <input type="url" bind:value={createForm.url} placeholder="https://example.com/callback" />
        </label>
        <label>
          <span class="label-text">Status</span>
          <select bind:value={createForm.status}>
            <option value="created">Created (active)</option>
            <option value="disabled">Disabled</option>
          </select>
        </label>
        <fieldset class="events-fieldset">
          <legend class="label-text">Events *</legend>
          <div class="events-grid">
            {#each WEBHOOK_EVENTS as evt}
              <label class="event-check">
                <input type="checkbox" checked={createForm.events.includes(evt)} onchange={() => toggleEventIn(createForm, evt)} />
                {evt}
              </label>
            {/each}
          </div>
        </fieldset>
        <div class="form-row">
          <label>
            <span class="label-text">API Key Header</span>
            <input type="text" bind:value={createForm.apiKeyName} placeholder="X-Api-Key" />
          </label>
          <label>
            <span class="label-text">API Key Value</span>
            <input type="password" bind:value={createForm.apiKeyValue} placeholder="secret" />
          </label>
        </div>
        <details class="scope-details">
          <summary class="label-text">Scope Filters (optional)</summary>
          <div class="form-row">
            <label>
              <span class="label-text">Flow IDs</span>
              <textarea bind:value={createForm.flowIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
            </label>
            <label>
              <span class="label-text">Source IDs</span>
              <textarea bind:value={createForm.sourceIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
            </label>
          </div>
          <div class="form-row">
            <label>
              <span class="label-text">Flow Collected By IDs</span>
              <textarea bind:value={createForm.flowCollectedByIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
            </label>
            <label>
              <span class="label-text">Source Collected By IDs</span>
              <textarea bind:value={createForm.sourceCollectedByIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
            </label>
          </div>
        </details>
        <button class="primary" onclick={createWebhook} disabled={creating || !createForm.url.trim() || createForm.events.length === 0}>
          {creating ? 'Creating...' : 'Create'}
        </button>
      </div>
    </div>
  {/if}

  <!-- Filters -->
  <div class="filter-bar">
    <input type="text" bind:value={tagName} placeholder="Tag name" class="filter-input" />
    <input type="text" bind:value={tagValue} placeholder="Tag value" class="filter-input" />
    <button class="btn-small" onclick={applyFilters}>Apply</button>
    <button class="btn-small btn-secondary" onclick={clearFilters}>Clear</button>
  </div>

  {#if loading}
    <p class="muted"><Spinner /> Loading...</p>
  {:else if error}
    <p class="error-text">{error}</p>
  {:else if webhooks.length === 0}
    <p class="muted">No webhooks configured.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>ID</th>
          <th>URL</th>
          <th>Events</th>
          <th>Tags</th>
          <th>Status</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each webhooks as wh}
          {#if editingId === wh.id}
            <!-- Edit row -->
            <tr>
              <td class="mono">{wh.id?.slice(0, 8)}</td>
              <td colspan="5">
                <div class="edit-panel">
                  {#if saveError}
                    <p class="error-text" style="font-size:0.85em">{saveError}</p>
                  {/if}
                  <label>
                    <span class="label-text">URL *</span>
                    <input type="url" bind:value={editForm.url} />
                  </label>
                  <label>
                    <span class="label-text">Status</span>
                    <select bind:value={editForm.status}>
                      <option value="created">Created (active)</option>
                      <option value="disabled">Disabled</option>
                    </select>
                  </label>
                  <fieldset class="events-fieldset">
                    <legend class="label-text">Events *</legend>
                    <div class="events-grid">
                      {#each WEBHOOK_EVENTS as evt}
                        <label class="event-check">
                          <input type="checkbox" checked={editForm.events.includes(evt)} onchange={() => toggleEventIn(editForm, evt)} />
                          {evt}
                        </label>
                      {/each}
                    </div>
                  </fieldset>
                  <div class="form-row">
                    <label>
                      <span class="label-text">API Key Header</span>
                      <input type="text" bind:value={editForm.apiKeyName} placeholder="X-Api-Key" />
                    </label>
                    <label>
                      <span class="label-text">API Key Value</span>
                      <input type="password" bind:value={editForm.apiKeyValue} placeholder="(unchanged if empty)" />
                    </label>
                  </div>
                  <details class="scope-details">
                    <summary class="label-text">Scope Filters</summary>
                    <div class="form-row">
                      <label>
                        <span class="label-text">Flow IDs</span>
                        <textarea bind:value={editForm.flowIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
                      </label>
                      <label>
                        <span class="label-text">Source IDs</span>
                        <textarea bind:value={editForm.sourceIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
                      </label>
                    </div>
                    <div class="form-row">
                      <label>
                        <span class="label-text">Flow Collected By IDs</span>
                        <textarea bind:value={editForm.flowCollectedByIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
                      </label>
                      <label>
                        <span class="label-text">Source Collected By IDs</span>
                        <textarea bind:value={editForm.sourceCollectedByIds} rows="2" class="mono" placeholder="One UUID per line"></textarea>
                      </label>
                    </div>
                  </details>
                  <TagEditor
                    tags={wh.tags || {}}
                    basePath={`/service/webhooks/${wh.id}`}
                    onUpdate={(newTags) => { wh.tags = newTags; }}
                  />
                  <div style="display:flex;gap:0.5em;margin-top:0.5em">
                    <button class="primary" onclick={saveEdit} disabled={saving || !editForm.url.trim() || editForm.events.length === 0}>
                      {saving ? 'Saving...' : 'Save'}
                    </button>
                    <button onclick={cancelEdit}>Cancel</button>
                  </div>
                </div>
              </td>
            </tr>
          {:else}
            <!-- Display row -->
            <tr>
              <td class="mono">{wh.id?.slice(0, 8) || '--'}</td>
              <td style="max-width:20em;overflow:hidden;text-overflow:ellipsis" title={wh.url}>{wh.url || '--'}</td>
              <td>
                {#if wh.events?.length}
                  {#each wh.events as evt}
                    <span class="badge">{evt}</span>{' '}
                  {/each}
                {:else}
                  <span class="muted">--</span>
                {/if}
              </td>
              <td>
                {#if wh.tags && Object.keys(wh.tags).length}
                  {#each Object.entries(wh.tags) as [k, v]}
                    <span class="badge">{k}={Array.isArray(v) ? v.join(',') : v}</span>{' '}
                  {/each}
                {:else}
                  <span class="muted">--</span>
                {/if}
              </td>
              <td><span class={statusClass(wh.status)}>{wh.status || '--'}</span></td>
              <td class="actions-cell">
                {#if confirmDeleteId === wh.id}
                  <span class="error-text" style="font-size:0.85em">Delete?</span>
                  <button class="btn-small btn-danger" onclick={() => deleteWebhook(wh.id)} disabled={deletingId === wh.id}>
                    {deletingId === wh.id ? '...' : 'Yes'}
                  </button>
                  <button class="btn-small" onclick={() => confirmDeleteId = null}>No</button>
                {:else}
                  <button class="btn-small" onclick={() => startEdit(wh)}>Edit</button>
                  <button class="btn-small btn-danger" onclick={() => confirmDeleteId = wh.id}>Delete</button>
                {/if}
              </td>
            </tr>
          {/if}
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
  .wh-form {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  .wh-form label {
    display: flex;
    flex-direction: column;
    gap: 0.2em;
  }
  .events-fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5em;
    margin: 0;
  }
  .events-fieldset legend {
    padding: 0 0.25em;
  }
  .events-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(12em, 1fr));
    gap: 0.25em;
  }
  .event-check {
    display: flex;
    align-items: center;
    gap: 0.25em;
    font-size: 0.85em;
    color: var(--text-muted);
    cursor: pointer;
  }
  .form-row {
    display: flex;
    gap: 0.75em;
    flex-wrap: wrap;
  }
  .form-row label {
    flex: 1;
    min-width: 12em;
  }
  .scope-details {
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5em;
  }
  .scope-details summary {
    cursor: pointer;
  }
  .edit-panel {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
    padding: 0.5em;
    background: var(--bg);
    border-radius: 4px;
  }
  .edit-panel label {
    display: flex;
    flex-direction: column;
    gap: 0.2em;
  }
  .actions-cell {
    white-space: nowrap;
    display: flex;
    gap: 0.25em;
    align-items: center;
  }
</style>
