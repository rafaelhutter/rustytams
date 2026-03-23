<script lang="ts">
  import { errorMessage } from '../lib/errors.js';

  let { value = '{}', label = 'JSON', onSave = () => {}, disabled = false }: {
    value?: string;
    label?: string;
    onSave?: (parsed: unknown) => void;
    disabled?: boolean;
  } = $props();

  let text: string = $state(value);
  let parseError: string | null = $state(null);

  // Sync external value changes into the textarea
  $effect(() => {
    text = value;
    parseError = null;
  });

  function handleSave(): void {
    parseError = null;
    try {
      const parsed: unknown = JSON.parse(text);
      onSave(parsed);
    } catch (e) {
      parseError = errorMessage(e);
    }
  }
</script>

<div class="json-editor">
  <label>
    <span class="label-text">{label}</span>
    <textarea
      bind:value={text}
      {disabled}
      rows="8"
      spellcheck="false"
      class="json-textarea"
    ></textarea>
  </label>
  {#if parseError}
    <p class="error-text parse-error">{parseError}</p>
  {/if}
  <button class="btn-small primary" onclick={handleSave} {disabled}>Save</button>
</div>

<style>
  .json-editor {
    display: flex;
    flex-direction: column;
    gap: 0.5em;
  }
  .json-editor label {
    display: flex;
    flex-direction: column;
    gap: 0.25em;
  }
  .json-textarea {
    font-family: var(--mono);
    font-size: 0.85em;
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5em;
    resize: vertical;
    tab-size: 2;
    line-height: 1.4;
  }
  .json-textarea:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .parse-error {
    font-size: 0.8em;
    margin: 0;
  }
</style>
