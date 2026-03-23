<script lang="ts">
  import { untrack } from 'svelte';
  import { errorMessage } from '../lib/errors.js';
  import { apiGet, apiPost, apiDelete, formatShortName, selectPresignedUrl } from '../lib/api.js';
  import { buildObjectQuery, buildFlowQuery } from '../lib/query.js';
  import { formatTimerangeDisplay } from '../lib/timerange.js';
  import { formatHexDump } from '../lib/hexdump.js';
  import { addToast } from '../lib/toast.js';
  import { FORMAT_VIDEO, FORMAT_AUDIO, FORMAT_IMAGE } from '../lib/ingest.js';
  import ConfirmDialog from '../components/ConfirmDialog.svelte';
  import Spinner from '../components/Spinner.svelte';

  let { params = {} }: { params?: Record<string, string> } = $props();
  let obj: any = $state(null);
  let flow: any = $state(null);
  let error: string | null = $state(null);
  let loading: boolean = $state(true);
  let loadedId: string | null = null;

  // Query options
  let verboseStorage: boolean = $state(false);
  let presignedFilter: string = $state('');
  let acceptGetUrls: string = $state('');
  let acceptStorageIds: string = $state('');

  // Instance form
  let showAddInstance: boolean = $state(false);
  let instanceMode: string = $state('controlled');
  let instanceStorageId: string = $state('');
  let instanceUrl: string = $state('');
  let instanceLabel: string = $state('');
  let addingInstance: boolean = $state(false);
  let instanceError: string | null = $state(null);

  // Delete instance
  let pendingDeleteInstance: any = $state(null);
  let deletingInstance: boolean = $state(false);
  let deleteInstanceError: string | null = $state(null);

  // Storage backends (for controlled copy dropdown)
  let backends: any[] = $state([]);
  let backendsLoaded: boolean = $state(false);

  // Player
  let mpegtsPlayer: any = null;
  let playerError: string | null = $state(null);

  // Hex dump for unknown media types
  let hexDump: string | null = $state(null);
  let hexLoading: boolean = $state(false);
  let hexError: boolean = $state(false);

  // Derived values (idiomatic Svelte 5 — compute once per reactive update)
  let presignedUrl: string | null = $derived(selectPresignedUrl(obj));

  let mediaType: string = $derived.by(() => {
    if (!flow) return 'unknown';
    const fmt: string = flow.format;
    if (fmt === FORMAT_VIDEO) return 'video';
    if (fmt === FORMAT_AUDIO) return 'audio';
    if (fmt === FORMAT_IMAGE) return 'image';
    return 'unknown';
  });

  let mpegTs: boolean = $derived.by(() => {
    if (!flow) return false;
    const container: string = (flow.container || '').toLowerCase();
    const codec: string = (flow.codec || '').toLowerCase();
    return container.includes('mp2t') || container.includes('mpeg-ts') || container.includes('mpegts')
      || codec.includes('mp2t');
  });

  let objTimerange = $derived(formatTimerangeDisplay(obj?.timerange));

  async function loadObject(id: string): Promise<void> {
    loadedId = id;
    loading = true;
    error = null;
    obj = null;
    flow = null;
    playerError = null;
    deleteInstanceError = null;
    hexDump = null;
    hexLoading = false;
    hexError = false;
    destroyPlayer();
    try {
      const opts: Record<string, any> = {};
      if (verboseStorage) opts.verboseStorage = true;
      if (presignedFilter === 'true') opts.presigned = true;
      else if (presignedFilter === 'false') opts.presigned = false;
      if (acceptGetUrls.trim()) opts.acceptGetUrls = acceptGetUrls.trim();
      if (acceptStorageIds.trim()) opts.acceptStorageIds = acceptStorageIds.trim();

      const path: string = buildObjectQuery(id, opts);
      const resp = await apiGet(path);
      if (id !== loadedId) return;
      obj = resp.data;

      // Fetch first referenced flow for format/codec/container detection
      if (obj.referenced_by_flows?.length) {
        try {
          const flowResp = await apiGet(buildFlowQuery(obj.referenced_by_flows[0]));
          if (id === loadedId) flow = flowResp.data;
        } catch (flowErr) {
          if (id === loadedId) {
            playerError = `Failed to fetch flow metadata: ${errorMessage(flowErr)}. Format detection may be inaccurate.`;
          }
        }
      }
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
      if (id && id !== loadedId) loadObject(id);
    });
  });

  function refetchObject(): void {
    if (params.id) loadObject(params.id);
  }

  function clearFilters(): void {
    verboseStorage = false;
    presignedFilter = '';
    acceptGetUrls = '';
    acceptStorageIds = '';
    refetchObject();
  }

  // mpegts.js player lifecycle (loaded via CDN script tag, not Vite bundle)
  function initMpegtsPlayer(element: HTMLVideoElement, url: string): void {
    destroyPlayer();
    const mpegts: any = (window as any).mpegts;
    if (!mpegts || typeof mpegts.createPlayer !== 'function') {
      playerError = 'mpegts.js not loaded — check script tag in index.html';
      return;
    }
    if (!mpegts.isSupported()) {
      playerError = 'mpegts.js: MSE not supported in this browser';
      return;
    }
    try {
      const player = mpegts.createPlayer({
        type: 'mpegts',
        isLive: false,
        url,
      }, {
        enableWorker: false,
      });
      player.on(mpegts.Events.ERROR, (errorType: string, errorDetail: string, errorInfo: any) => {
        playerError = `mpegts.js error: ${errorType} - ${errorDetail}${errorInfo ? ' (' + JSON.stringify(errorInfo) + ')' : ''}`;
      });
      player.attachMediaElement(element);
      player.load();
      mpegtsPlayer = player;
    } catch (e) {
      playerError = `Player init error: ${errorMessage(e)}`;
    }
  }

  function destroyPlayer(): void {
    if (mpegtsPlayer) {
      try {
        mpegtsPlayer.off((window as any).mpegts.Events.ERROR);
        mpegtsPlayer.pause();
        mpegtsPlayer.unload();
        mpegtsPlayer.detach();
        mpegtsPlayer.destroy();
      } catch { /* best effort cleanup */ }
      mpegtsPlayer = null;
    }
  }

  // Svelte action for mpegts.js player — fires on mount/destroy, no $effect needed
  function mpegtsAction(node: HTMLVideoElement, url: string): { update(newUrl: string): void; destroy(): void } {
    if (url) initMpegtsPlayer(node, url);
    return {
      update(newUrl: string) {
        destroyPlayer();
        if (newUrl) initMpegtsPlayer(node, newUrl);
      },
      destroy() {
        destroyPlayer();
      },
    };
  }

  // Auto-fetch hex dump for unknown media types
  $effect(() => {
    const url: string | null = presignedUrl;
    const type: string = mediaType;
    if (url && type === 'unknown') {
      untrack(() => fetchHexDump(url));
    }
  });

  // Instance management
  async function loadBackends(): Promise<void> {
    if (backendsLoaded) return;
    backendsLoaded = true;
    try {
      const resp = await apiGet('/service/storage-backends');
      backends = Array.isArray(resp.data) ? resp.data : [];
    } catch (e) {
      backendsLoaded = false;
      instanceError = `Failed to load storage backends: ${errorMessage(e)}`;
    }
  }

  function resetInstanceForm(): void {
    instanceStorageId = '';
    instanceUrl = '';
    instanceLabel = '';
  }

  function toggleAddInstance(): void {
    showAddInstance = !showAddInstance;
    instanceError = null;
    if (showAddInstance) {
      loadBackends();
    } else {
      resetInstanceForm();
    }
  }

  async function addInstance(): Promise<void> {
    addingInstance = true;
    instanceError = null;
    try {
      const body: Record<string, string> = instanceMode === 'controlled'
        ? { storage_id: instanceStorageId }
        : { url: instanceUrl.trim(), label: instanceLabel.trim() };
      await apiPost(`/objects/${params.id}/instances`, body);
      showAddInstance = false;
      resetInstanceForm();
      addToast('Instance registered', 'success');
      refetchObject();
    } catch (e) {
      instanceError = errorMessage(e);
    } finally {
      addingInstance = false;
    }
  }

  async function deleteInstance(): Promise<void> {
    const identifier: any = pendingDeleteInstance;
    if (!identifier) return;
    deletingInstance = true;
    deleteInstanceError = null;
    try {
      const qs = new URLSearchParams();
      if (identifier.storage_id) qs.set('storage_id', identifier.storage_id);
      else qs.set('label', identifier.label);
      await apiDelete(`/objects/${params.id}/instances?${qs}`);
      pendingDeleteInstance = null;
      addToast('Instance deleted', 'success');
      refetchObject();
    } catch (e) {
      deleteInstanceError = errorMessage(e);
      pendingDeleteInstance = null;
    } finally {
      deletingInstance = false;
    }
  }

  async function fetchHexDump(url: string): Promise<void> {
    hexLoading = true;
    hexError = false;
    hexDump = null;
    try {
      const resp: Response = await fetch(url, { headers: { 'Range': 'bytes=0-511' } });
      if (!resp.ok && resp.status !== 206) throw new Error(`HTTP ${resp.status}`);
      const buf: ArrayBuffer = await resp.arrayBuffer();
      hexDump = formatHexDump(new Uint8Array(buf));
    } catch {
      hexError = true;
    } finally {
      hexLoading = false;
    }
  }

  function instanceKey(u: any): string {
    return u.storage_id || u.label || u.url;
  }
</script>

<div class="page">
  {#if loading}
    <p class="muted"><Spinner /> Loading...</p>
  {:else if error}
    <p class="error-text">{error}</p>
  {:else if obj}
    <h1>Object</h1>

    <!-- Metadata -->
    <div class="panel" style="margin-bottom:1em">
      <dl class="detail-grid">
        <dt>ID</dt>
        <dd class="mono">{obj.id}</dd>
        <dt>Timerange</dt>
        <dd class="mono">
          <span class="timerange-raw">{objTimerange.raw}</span>
          {#if objTimerange.display !== objTimerange.raw && objTimerange.display !== '--'}
            <br><span class="muted" style="font-size:0.8em">{objTimerange.display}</span>
          {/if}
        </dd>
        {#if obj.key_frame_count !== undefined}
          <dt>Key Frames</dt>
          <dd>{obj.key_frame_count}</dd>
        {/if}
      </dl>

      {#if flow}
        <div style="margin-top:0.75em">
          <span class="label-text">Format:</span>
          <span class="badge">{formatShortName(flow.format)}</span>
          {#if flow.codec}
            <span class="label-text" style="margin-left:0.5em">Codec:</span>
            <span class="mono">{flow.codec}</span>
          {/if}
        </div>
      {/if}

      {#if obj.referenced_by_flows?.length}
        <div style="margin-top:0.75em">
          <span class="label-text">Referenced by flows:</span>
          <div class="flow-links">
            {#each obj.referenced_by_flows as flowId}
              <a href="#/flows/{flowId}" class="mono">{flowId.slice(0, 8)}</a>
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <!-- Player -->
    {#if presignedUrl}
      <div class="panel" style="margin-bottom:1em">
        <h3>Preview</h3>
        {#if playerError}
          <p class="error-text" style="font-size:0.85em">{playerError}</p>
        {/if}
        {#if mediaType === 'video'}
          {#if mpegTs}
            <video
              use:mpegtsAction={presignedUrl}
              controls
              class="media-player"
              onerror={() => playerError = 'Video playback failed'}
            >
              <track kind="captions" />
            </video>
          {:else}
            <video
              controls
              class="media-player"
              src={presignedUrl}
              onerror={() => playerError = 'Video playback failed'}
            >
              <track kind="captions" />
            </video>
          {/if}
        {:else if mediaType === 'audio'}
          <audio
            controls
            class="media-player"
            src={presignedUrl}
            onerror={() => playerError = 'Audio playback failed'}
          >
          </audio>
        {:else if mediaType === 'image'}
          <img
            src={presignedUrl}
            alt="Media object {obj.id}"
            class="media-player"
            onerror={() => playerError = `Image load failed from ${presignedUrl}`}
          />
        {:else}
          {#if hexLoading}
            <p class="muted">Loading hex dump...</p>
          {:else if hexDump}
            <pre class="hex-dump">{hexDump}</pre>
          {:else if hexError}
            <p class="muted">Binary content — download to view</p>
          {:else}
            <p class="muted">No preview available for this format.</p>
          {/if}
        {/if}
        <div style="margin-top:0.5em">
          <a href={presignedUrl} download class="btn-small primary">Download</a>
        </div>
      </div>
    {/if}

    <!-- get_urls with filters -->
    <div class="panel" style="margin-bottom:1em">
      <h3>URLs</h3>
      <div class="filter-bar" style="margin-bottom:0.5em">
        <label class="inline-label">
          <input type="checkbox" bind:checked={verboseStorage} />
          Verbose storage
        </label>
        <select bind:value={presignedFilter} class="filter-input" style="width:auto">
          <option value="">Presigned: any</option>
          <option value="true">Presigned only</option>
          <option value="false">Non-presigned only</option>
        </select>
        <input type="text" bind:value={acceptGetUrls} placeholder="accept_get_urls" class="filter-input" />
        <input type="text" bind:value={acceptStorageIds} placeholder="accept_storage_ids" class="filter-input" />
        <button class="btn-small" onclick={refetchObject}>Apply</button>
        <button class="btn-small btn-secondary" onclick={clearFilters}>Clear</button>
      </div>

      {#if obj.get_urls?.length}
        <table>
          <thead>
            <tr>
              <th>Label</th>
              <th>URL</th>
              <th>Presigned</th>
              {#if verboseStorage}
                <th>Storage ID</th>
                <th>Type</th>
                <th>Provider</th>
              {/if}
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each obj.get_urls as u}
              <tr>
                <td>{u.label || '--'}</td>
                <td class="url-cell" title={u.url}>
                  <a href={u.url} target="_blank" rel="noopener">{u.url}</a>
                </td>
                <td>{u.presigned ? 'Yes' : 'No'}</td>
                {#if verboseStorage}
                  <td class="mono">{u.storage_id?.slice(0, 8) || '--'}</td>
                  <td>{u.store_type || '--'}</td>
                  <td>{u.provider || '--'}</td>
                {/if}
                <td class="actions-cell">
                  <a href={u.url} download class="btn-small">DL</a>
                  <button class="btn-small btn-danger" onclick={() => pendingDeleteInstance = u}>Del</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="muted">No URLs available.</p>
      {/if}
      {#if deleteInstanceError}
        <p class="error-text" style="font-size:0.85em">{deleteInstanceError}</p>
      {/if}
    </div>

    <!-- Register Instance -->
    <div class="panel">
      <div style="display:flex;justify-content:space-between;align-items:center">
        <h3>Instances</h3>
        <button class="btn-small primary" onclick={toggleAddInstance}>
          {showAddInstance ? 'Cancel' : '+ Register Instance'}
        </button>
      </div>

      {#if showAddInstance}
        <div class="instance-form">
          {#if instanceError}
            <p class="error-text" style="font-size:0.85em">{instanceError}</p>
          {/if}
          <div class="form-row">
            <label class="inline-label">
              <input type="radio" bind:group={instanceMode} value="controlled" /> Controlled (copy to storage)
            </label>
            <label class="inline-label">
              <input type="radio" bind:group={instanceMode} value="uncontrolled" /> Uncontrolled (external URL)
            </label>
          </div>
          {#if instanceMode === 'controlled'}
            <label>
              <span class="label-text">Storage Backend</span>
              <select bind:value={instanceStorageId}>
                <option value="">-- select --</option>
                {#each backends as b}
                  <option value={b.id}>{b.label || b.id?.slice(0, 8)} ({b.store_type || '--'})</option>
                {/each}
              </select>
            </label>
            <button class="primary" onclick={addInstance} disabled={addingInstance || !instanceStorageId}>
              {addingInstance ? 'Registering...' : 'Register'}
            </button>
          {:else}
            <label>
              <span class="label-text">URL</span>
              <input type="url" bind:value={instanceUrl} placeholder="https://example.com/object" />
            </label>
            <label>
              <span class="label-text">Label</span>
              <input type="text" bind:value={instanceLabel} placeholder="Instance label" />
            </label>
            <button class="primary" onclick={addInstance} disabled={addingInstance || !instanceUrl.trim() || !instanceLabel.trim()}>
              {addingInstance ? 'Registering...' : 'Register'}
            </button>
          {/if}
        </div>
      {/if}
    </div>

    <ConfirmDialog
      open={pendingDeleteInstance !== null}
      title="Delete Instance"
      message={`Delete instance "${pendingDeleteInstance ? instanceKey(pendingDeleteInstance) : ''}"?`}
      confirmLabel="Delete"
      danger={true}
      loading={deletingInstance}
      onConfirm={deleteInstance}
      onCancel={() => pendingDeleteInstance = null}
    />
  {/if}
</div>

<style>
  .media-player {
    max-width: 100%;
    max-height: 480px;
    border-radius: 4px;
    background: #000;
  }
  .hex-dump {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.75em;
    font-family: var(--mono);
    font-size: 0.8em;
    overflow-x: auto;
    margin: 0;
    white-space: pre;
    line-height: 1.4;
  }
  .url-cell {
    max-width: 25em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .flow-links {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5em;
    margin-top: 0.25em;
  }
  .instance-form {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
    margin-top: 0.75em;
    padding-top: 0.75em;
    border-top: 1px solid var(--border);
  }
  .instance-form label {
    display: flex;
    flex-direction: column;
    gap: 0.2em;
  }
  .inline-label {
    display: flex;
    align-items: center;
    gap: 0.3em;
    font-size: 0.85em;
    color: var(--text-muted);
    cursor: pointer;
  }
  .form-row {
    display: flex;
    gap: 1em;
    flex-wrap: wrap;
  }
  .actions-cell {
    white-space: nowrap;
    display: flex;
    gap: 0.25em;
    align-items: center;
  }
</style>
