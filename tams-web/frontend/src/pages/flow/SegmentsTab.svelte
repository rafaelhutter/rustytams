<script lang="ts">
  import { untrack } from 'svelte';
  import { errorMessage } from '../../lib/errors.js';
  import { apiGet, apiPost, apiDelete, parsePagination } from '../../lib/api.js';
  import { formatTimerangeDisplay } from '../../lib/timerange.js';
  import { buildSegmentsQuery } from '../../lib/query.js';
  import { computeTimelineBounds, segmentBarStyle } from '../../lib/timeline.js';
  import { addToast } from '../../lib/toast.js';
  import type { PaginationInfo, Segment } from '../../types/tams.js';

  let { flowId = '', initialSegments = [], initialPagination = {} }: {
    flowId?: string;
    initialSegments?: any[];
    initialPagination?: Partial<PaginationInfo>;
  } = $props();

  let segments: any[] = $state([]);
  let segLoading: boolean = $state(false);
  let segError: string | null = $state(null);
  let segPagination: PaginationInfo = $state({ limit: null, nextKey: null, count: null, timerange: null });

  // Query controls
  let segReverseOrder: boolean = $state(false);
  let segPresigned: string = $state('');
  let segVerboseStorage: boolean = $state(false);
  let segIncludeObjectTr: boolean = $state(false);
  let segAcceptGetUrls: string = $state('');
  let segAcceptStorageIds: string = $state('');
  let segTimerangeFilter: string = $state('');
  let segObjectIdFilter: string = $state('');

  // Registration
  let showSegRegister: boolean = $state(false);
  let segRegForm: { objectId: string; timerange: string; tsOffset: string } = $state({ objectId: '', timerange: '', tsOffset: '0:0' });
  let segRegistering: boolean = $state(false);
  let segRegError: string | null = $state(null);
  let segRegSuccess: string | null = $state(null);

  // Deletion
  let showSegDelete: boolean = $state(false);
  let segDelTimerange: string = $state('');
  let segDelObjectId: string = $state('');
  let segDeleting: boolean = $state(false);
  let segDelError: string | null = $state(null);
  let segDelResult: any = $state(null);

  let timelineBounds = $derived(computeTimelineBounds(segments));

  // Sync initial data (untrack segPagination to avoid self-triggering)
  $effect(() => {
    segments = initialSegments;
    untrack(() => {
      segPagination = { ...segPagination, ...initialPagination };
    });
  });

  async function fetchSegments(pageKey: string | null = null): Promise<void> {
    segLoading = true;
    segError = null;
    try {
      const path: string = buildSegmentsQuery(flowId, {
        limit: 50,
        timerange: segTimerangeFilter.trim() || undefined,
        objectId: segObjectIdFilter.trim() || undefined,
        reverseOrder: segReverseOrder || undefined,
        presigned: segPresigned === 'true' ? true : segPresigned === 'false' ? false : undefined,
        verboseStorage: segVerboseStorage || undefined,
        includeObjectTimerange: segIncludeObjectTr || undefined,
        acceptGetUrls: segAcceptGetUrls.trim() || undefined,
        acceptStorageIds: segAcceptStorageIds.trim() || undefined,
        page: pageKey || undefined,
      });
      const { data, headers } = await apiGet(path);
      segments = data;
      segPagination = parsePagination(headers);
    } catch (e) {
      segError = errorMessage(e);
    } finally {
      segLoading = false;
    }
  }

  function clearSegFilters(): void {
    segTimerangeFilter = '';
    segObjectIdFilter = '';
    segReverseOrder = false;
    segPresigned = '';
    segVerboseStorage = false;
    segIncludeObjectTr = false;
    segAcceptGetUrls = '';
    segAcceptStorageIds = '';
    fetchSegments();
  }

  async function registerSegment(): Promise<void> {
    if (!segRegForm.objectId.trim() || !segRegForm.timerange.trim()) return;
    segRegistering = true;
    segRegError = null;
    segRegSuccess = null;
    try {
      const body: Record<string, string> = {
        object_id: segRegForm.objectId.trim(),
        timerange: segRegForm.timerange.trim(),
      };
      if (segRegForm.tsOffset.trim() && segRegForm.tsOffset.trim() !== '0:0') {
        body.ts_offset = segRegForm.tsOffset.trim();
      }
      const result = await apiPost(`/flows/${flowId}/segments`, body);
      if (result.status === 200 && result.data) {
        segRegError = 'Partial failure: ' + JSON.stringify(result.data);
      } else {
        segRegSuccess = 'Segment registered';
        segRegForm = { objectId: '', timerange: '', tsOffset: '0:0' };
        showSegRegister = false;
        addToast('Segment registered', 'success');
        fetchSegments();
      }
    } catch (e) {
      segRegError = errorMessage(e);
    } finally {
      segRegistering = false;
    }
  }

  async function deleteSegments(): Promise<void> {
    segDeleting = true;
    segDelError = null;
    segDelResult = null;
    try {
      const searchParams = new URLSearchParams();
      if (segDelTimerange.trim()) searchParams.set('timerange', segDelTimerange.trim());
      if (segDelObjectId.trim()) searchParams.set('object_id', segDelObjectId.trim());
      const qs: string = searchParams.toString();
      const path: string = `/flows/${flowId}/segments${qs ? '?' + qs : ''}`;
      const result = await apiDelete(path);
      if (result.status === 202) {
        segDelResult = result.data;
        addToast('Segment deletion in progress', 'info');
      } else {
        showSegDelete = false;
        segDelTimerange = '';
        segDelObjectId = '';
        addToast('Segments deleted', 'success');
        fetchSegments();
      }
    } catch (e) {
      segDelError = errorMessage(e);
    } finally {
      segDeleting = false;
    }
  }
</script>

<div class="panel" style="margin-bottom:1em">
  <div class="seg-header">
    <h3>Segments {segPagination.count != null ? `(${segPagination.count})` : `(${segments.length})`}</h3>
    <div class="seg-actions">
      <button class="btn-small" onclick={() => { showSegRegister = !showSegRegister; segRegError = null; segRegSuccess = null; }}>
        {showSegRegister ? 'Cancel' : '+ Register'}
      </button>
      <button class="btn-small btn-danger" onclick={() => { showSegDelete = !showSegDelete; segDelError = null; segDelResult = null; }}>
        {showSegDelete ? 'Cancel' : 'Delete...'}
      </button>
    </div>
  </div>

  <!-- Register segment form -->
  {#if showSegRegister}
    <div class="seg-form">
      {#if segRegError}
        <p class="error-text" style="font-size:0.85em">{segRegError}</p>
      {/if}
      {#if segRegSuccess}
        <p class="success-text" style="font-size:0.85em">{segRegSuccess}</p>
      {/if}
      <div class="form-row">
        <label>
          <span class="label-text">Object ID</span>
          <input type="text" bind:value={segRegForm.objectId} placeholder="UUID" class="mono" />
        </label>
        <label>
          <span class="label-text">Timerange</span>
          <input type="text" bind:value={segRegForm.timerange} placeholder="[0:0_10:0)" class="mono" />
        </label>
        <label>
          <span class="label-text">TS Offset</span>
          <input type="text" bind:value={segRegForm.tsOffset} placeholder="0:0" class="mono" style="max-width:8em" />
        </label>
      </div>
      <button class="primary" onclick={registerSegment} disabled={segRegistering || !segRegForm.objectId.trim() || !segRegForm.timerange.trim()}>
        {segRegistering ? 'Registering...' : 'Register'}
      </button>
    </div>
  {/if}

  <!-- Delete segments form -->
  {#if showSegDelete}
    <div class="seg-form">
      {#if segDelError}
        <p class="error-text" style="font-size:0.85em">{segDelError}</p>
      {/if}
      {#if segDelResult}
        <div style="margin-bottom:0.5em">
          <p class="muted" style="font-size:0.85em">Deletion in progress (async).</p>
          <dl class="detail-grid">
            <dt>Request ID</dt><dd class="mono">{segDelResult.id}</dd>
            <dt>Status</dt><dd>{segDelResult.status || 'created'}</dd>
          </dl>
        </div>
      {:else}
        <div class="form-row">
          <label>
            <span class="label-text">Timerange</span>
            <input type="text" bind:value={segDelTimerange} placeholder="_ (all)" class="mono" />
          </label>
          <label>
            <span class="label-text">Object ID</span>
            <input type="text" bind:value={segDelObjectId} placeholder="Optional" class="mono" />
          </label>
        </div>
        <button class="danger" onclick={deleteSegments} disabled={segDeleting}>
          {segDeleting ? 'Deleting...' : 'Delete Segments'}
        </button>
      {/if}
    </div>
  {/if}

  <!-- Query controls -->
  <div class="seg-controls">
    <label class="seg-check"><input type="checkbox" bind:checked={segReverseOrder} /> Reverse</label>
    <label class="seg-check">
      Presigned:
      <select bind:value={segPresigned} style="font-size:0.85em">
        <option value="">any</option>
        <option value="true">yes</option>
        <option value="false">no</option>
      </select>
    </label>
    <label class="seg-check"><input type="checkbox" bind:checked={segVerboseStorage} /> Verbose</label>
    <label class="seg-check"><input type="checkbox" bind:checked={segIncludeObjectTr} /> Obj TR</label>
    <input type="text" bind:value={segTimerangeFilter} placeholder="Timerange" class="filter-input filter-sm" />
    <input type="text" bind:value={segObjectIdFilter} placeholder="Object ID" class="filter-input filter-sm" />
    <input type="text" bind:value={segAcceptGetUrls} placeholder="Get URLs" class="filter-input filter-sm" title="Comma-separated URL labels" />
    <input type="text" bind:value={segAcceptStorageIds} placeholder="Storage IDs" class="filter-input filter-sm" title="Comma-separated storage IDs" />
    <button class="btn-small" onclick={() => fetchSegments()}>Apply</button>
    <button class="btn-small btn-secondary" onclick={clearSegFilters}>Clear</button>
  </div>

  {#if segPagination.timerange}
    <p class="muted" style="font-size:0.8em;margin:0.25em 0">
      Paging timerange: <span class="mono timerange-raw">{segPagination.timerange}</span>
      {#if segPagination.count != null} | Count: {segPagination.count}{/if}
    </p>
  {/if}

  <!-- Timeline bar -->
  {#if segments.length > 0 && timelineBounds}
    <div class="timeline">
      {#each segments as seg}
        <div class="timeline-seg" style={segmentBarStyle(seg, timelineBounds)} title={seg.timerange}></div>
      {/each}
    </div>
  {/if}

  <!-- Segments table -->
  {#if segLoading}
    <p class="muted">Loading segments...</p>
  {:else if segError}
    <p class="error-text" style="font-size:0.85em">{segError}</p>
  {:else if segments.length === 0}
    <p class="muted">No segments.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Timerange</th>
          <th>Object ID</th>
          <th>TS Offset</th>
          <th>Key Frames</th>
          {#if segIncludeObjectTr}<th>Object TR</th>{/if}
        </tr>
      </thead>
      <tbody>
        {#each segments as seg}
          {@const segTr = formatTimerangeDisplay(seg.timerange)}
          <tr>
            <td class="mono" style="font-size:0.8em" title={segTr.display}><span class="timerange-raw">{segTr.raw}</span></td>
            <td class="mono"><a href="#/media/{seg.object_id}">{seg.object_id?.slice(0, 8) || '--'}</a></td>
            <td class="mono">{seg.ts_offset || '0:0'}</td>
            <td>{seg.key_frame_count ?? '--'}</td>
            {#if segIncludeObjectTr}<td class="mono" style="font-size:0.8em">{seg.object_timerange || '--'}</td>{/if}
          </tr>
        {/each}
      </tbody>
    </table>

    {#if segPagination.nextKey}
      <div style="margin-top:0.5em;text-align:center">
        <button class="btn-small" onclick={() => fetchSegments(segPagination.nextKey)}>Next Page</button>
      </div>
    {/if}
  {/if}

  <!-- get_urls display -->
  {#if segments.some(s => s.get_urls?.length)}
    <div style="margin-top:1em">
      <h3>Segment URLs</h3>
      <table>
        <thead><tr><th>Object</th><th>Label</th><th>URL</th><th>Presigned</th></tr></thead>
        <tbody>
          {#each segments as seg}
            {#each seg.get_urls || [] as gu}
              <tr>
                <td class="mono">{seg.object_id?.slice(0, 8)}</td>
                <td>{gu.label || '--'}</td>
                <td class="mono" style="font-size:0.75em;max-width:20em;overflow:hidden;text-overflow:ellipsis">
                  <a href={gu.url} target="_blank" rel="noopener">{gu.url}</a>
                </td>
                <td>{gu.presigned ? 'Yes' : 'No'}</td>
              </tr>
            {/each}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .seg-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5em;
  }
  .seg-header h3 {
    margin: 0;
  }
  .seg-actions {
    display: flex;
    gap: 0.5em;
  }
  .seg-controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5em;
    align-items: center;
    margin-bottom: 0.5em;
    font-size: 0.85em;
  }
  .seg-check {
    display: flex;
    align-items: center;
    gap: 0.25em;
    font-size: 0.85em;
    color: var(--text-muted);
  }
  .seg-form {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.75em;
    margin-bottom: 0.75em;
  }
  .form-row {
    display: flex;
    gap: 0.75em;
    flex-wrap: wrap;
    margin-bottom: 0.5em;
  }
  .form-row label {
    display: flex;
    flex-direction: column;
    gap: 0.2em;
    flex: 1;
    min-width: 10em;
  }
  .timeline {
    position: relative;
    height: 1.5em;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 3px;
    margin-bottom: 0.5em;
    overflow: hidden;
  }
  .timeline-seg {
    position: absolute;
    top: 2px;
    bottom: 2px;
    background: var(--accent);
    opacity: 0.7;
    border-radius: 2px;
  }
</style>
