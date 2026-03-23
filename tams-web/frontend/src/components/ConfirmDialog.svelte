<script lang="ts">
  let {
    open = false,
    title = 'Confirm',
    message = 'Are you sure?',
    confirmLabel = 'Confirm',
    danger = true,
    loading = false,
    onConfirm = () => {},
    onCancel = () => {},
  }: {
    open?: boolean;
    title?: string;
    message?: string;
    confirmLabel?: string;
    danger?: boolean;
    loading?: boolean;
    onConfirm?: () => void;
    onCancel?: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      onCancel();
    }
  }

  function handleBackdropClick(e: MouseEvent): void {
    if (e.target === e.currentTarget) {
      onCancel();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="overlay" role="dialog" aria-modal="true" aria-label={title}
       tabindex="-1" onkeydown={handleKeydown} onclick={handleBackdropClick}>
    <div class="dialog-panel">
      <h3 class="dialog-title">{title}</h3>
      <p class="dialog-message">{message}</p>
      <div class="dialog-actions">
        <button class="btn-cancel" onclick={onCancel} disabled={loading}>Cancel</button>
        <button
          class={danger ? 'btn-confirm danger' : 'btn-confirm'}
          onclick={onConfirm}
          disabled={loading}
        >
          {loading ? 'Working...' : confirmLabel}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
  }
  .dialog-panel {
    background: var(--panel, #333333);
    border: 1px solid var(--border, #444444);
    border-radius: 6px;
    padding: 1.5em;
    min-width: 320px;
    max-width: 480px;
    box-shadow: 0 4px 24px rgba(0, 0, 0, 0.4);
  }
  .dialog-title {
    margin: 0 0 0.5em;
    color: var(--text, #e0e0e0);
    font-size: 1.1em;
  }
  .dialog-message {
    margin: 0 0 1.25em;
    color: var(--text, #e0e0e0);
    font-size: 0.9em;
    line-height: 1.4;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5em;
  }
  .btn-cancel {
    background: transparent;
    border: 1px solid var(--border, #444444);
    color: var(--text, #e0e0e0);
    padding: 0.4em 1em;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.9em;
  }
  .btn-cancel:hover {
    background: var(--border, #444444);
  }
  .btn-confirm {
    background: var(--accent, #5a9fd4);
    border: 1px solid var(--accent, #5a9fd4);
    color: #fff;
    padding: 0.4em 1em;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.9em;
  }
  .btn-confirm:hover {
    opacity: 0.9;
  }
  .btn-confirm.danger {
    background: var(--danger, #d45a5a);
    border-color: var(--danger, #d45a5a);
  }
  .btn-confirm:disabled,
  .btn-cancel:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
