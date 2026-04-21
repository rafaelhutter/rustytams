<script lang="ts">
  import { location, matchRoute, compileRoutes } from './lib/router.js';
  import Nav from './components/Nav.svelte';
  import Dashboard from './pages/Dashboard.svelte';
  import Sources from './pages/Sources.svelte';
  import SourceDetail from './pages/SourceDetail.svelte';
  import Flows from './pages/Flows.svelte';
  import FlowDetail from './pages/FlowDetail.svelte';
  import Webhooks from './pages/Webhooks.svelte';
  import MediaPreview from './pages/MediaPreview.svelte';
  import Player from './pages/Player.svelte';
  import Record from './pages/Record.svelte';
  import Gallery from './pages/Gallery.svelte';
  import Editor from './pages/Editor.svelte';
  import { setCredentials, clearCredentials, configure, getApiBase, authenticated, authError } from './lib/api.js';
  import ToastContainer from './components/ToastContainer.svelte';
  import type { CompiledRoute, RouteMatch } from './lib/router.js';

  const compiled: CompiledRoute[] = compileRoutes({
    '/': Dashboard,
    '/sources': Sources,
    '/sources/:id': SourceDetail,
    '/flows': Flows,
    '/flows/:id': FlowDetail,
    '/webhooks': Webhooks,
    '/media/:id': MediaPreview,
    '/player/:id': Player,
    '/record': Record,
    '/gallery': Gallery,
    '/editor': Editor,
  });

  let username: string = $state('');
  let password: string = $state('');
  let apiUrl: string = $state(
    (typeof window !== 'undefined' && window.location.origin !== 'null')
      ? window.location.origin
      : 'http://localhost:5800'
  );

  let currentRoute: RouteMatch | null = $derived(matchRoute($location, compiled));

  function login(): void {
    if (!username || !password) return;
    configure({ api: apiUrl });
    setCredentials(username, password);
  }

  function logout(): void {
    clearCredentials();
  }
</script>

<Nav />
<div class="main">
  {#if !$authenticated}
    <div class="login-container">
      <div class="panel login-form">
        <h2>Connect to TAMS</h2>
        {#if $authError}
          <p class="error-text">{$authError}</p>
        {/if}
        <form onsubmit={(e: SubmitEvent) => { e.preventDefault(); login(); }}>
          <label>
            API URL
            <input type="url" bind:value={apiUrl} />
          </label>
          <label>
            Username
            <input type="text" bind:value={username} autocomplete="username" />
          </label>
          <label>
            Password
            <input type="password" bind:value={password} autocomplete="current-password" />
          </label>
          <button type="submit" class="primary">Connect</button>
        </form>
      </div>
    </div>
  {:else}
    <div class="topbar">
      <span class="muted">Connected to {getApiBase()}</span>
      <button onclick={logout}>Logout</button>
    </div>
    {#if currentRoute}
      <currentRoute.component params={currentRoute.params} />
    {:else}
      <div style="padding:1.5em"><p class="muted">Page not found.</p></div>
    {/if}
  {/if}
</div>
<ToastContainer />

<style>
  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }
  .login-container {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
  }
  .login-form {
    width: 340px;
  }
  .login-form form {
    display: flex;
    flex-direction: column;
    gap: 0.75em;
  }
  .login-form label {
    display: flex;
    flex-direction: column;
    gap: 0.25em;
    font-size: 0.85em;
    color: var(--text-muted);
  }
  .login-form input {
    width: 100%;
  }
  .topbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5em 1.5em;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    font-size: 0.85em;
  }
</style>
