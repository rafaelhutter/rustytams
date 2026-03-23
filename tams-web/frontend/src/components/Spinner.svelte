<script lang="ts">
  let { size = '1.5em' }: { size?: string } = $props();
</script>

<span class="spinner" style:width={size} style:height={size}></span>

<style>
  .spinner {
    display: inline-block;
    vertical-align: middle;
    border: 2px solid transparent;
    border-top-color: #888;
    border-left-color: #888;
    border-radius: 50%;
    animation: spin 0.6s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
