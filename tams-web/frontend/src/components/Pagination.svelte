<script lang="ts">
  let { count = null, limit = null, nextKey = null, onPage = () => {} }: {
    count?: number | null;
    limit?: number | null;
    nextKey?: string | null;
    onPage?: (page: { key: string | null }) => void;
  } = $props();

  let currentPage: number = $state(1);
  let totalPages: number | null = $derived(
    count !== null && limit ? Math.ceil(count / limit) : null
  );
  let hasNext: boolean = $derived(nextKey !== null);
  let hasPrev: boolean = $derived(currentPage > 1);

  /** Keys for each page we've visited (index 0 = page 1 key = null). */
  let pageKeys: Array<string | null> = $state([null]);

  function goNext(): void {
    if (!hasNext) return;
    pageKeys[currentPage] = nextKey;
    currentPage++;
    onPage({ key: nextKey });
  }

  function goPrev(): void {
    if (!hasPrev) return;
    currentPage--;
    const key: string | null = pageKeys[currentPage - 1] ?? null;
    onPage({ key });
  }

  export function reset(): void {
    currentPage = 1;
    pageKeys = [null];
  }
</script>

{#if count !== null || hasNext || hasPrev}
  <div class="pagination">
    <button onclick={goPrev} disabled={!hasPrev}>&lt;</button>
    <span class="page-info">
      {#if totalPages !== null}
        {currentPage} of {totalPages}
      {:else}
        Page {currentPage}
      {/if}
    </span>
    <button onclick={goNext} disabled={!hasNext}>&gt;</button>
    {#if count !== null}
      <span class="muted count">{count} total</span>
    {/if}
  </div>
{/if}

<style>
  .pagination {
    display: flex;
    align-items: center;
    gap: 0.5em;
    margin-top: 0.75em;
    font-size: 0.85em;
  }
  .page-info {
    min-width: 5em;
    text-align: center;
  }
  .count {
    margin-left: 0.5em;
    font-size: 0.9em;
  }
</style>
