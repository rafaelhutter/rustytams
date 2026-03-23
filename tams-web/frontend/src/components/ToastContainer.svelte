<script lang="ts">
  import { toasts, removeToast } from '../lib/toast.js';
  import type { ToastItem } from '../lib/toast.js';
</script>

{#if $toasts.length > 0}
  <div class="toast-container">
    {#each $toasts as toast (toast.id)}
      <div class="toast {toast.type}">
        <span class="toast-msg">{toast.message}</span>
        <button class="toast-close" onclick={() => removeToast(toast.id)} aria-label="Dismiss">&times;</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-container {
    position: fixed;
    top: 1em;
    right: 1em;
    z-index: 2000;
    display: flex;
    flex-direction: column;
    gap: 0.5em;
    max-width: 400px;
  }
  .toast {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.75em 1em;
    display: flex;
    align-items: center;
    gap: 0.5em;
    font-size: 0.85em;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
    animation: toast-in 0.2s ease-out;
  }
  .toast.error {
    border-left: 3px solid var(--error);
  }
  .toast.success {
    border-left: 3px solid var(--success);
  }
  .toast.info {
    border-left: 3px solid var(--accent);
  }
  .toast-msg {
    flex: 1;
  }
  .toast-close {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 1.2em;
    cursor: pointer;
    padding: 0 0.2em;
    line-height: 1;
  }
  .toast-close:hover {
    color: var(--text);
    background: transparent;
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateX(1em);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
</style>
