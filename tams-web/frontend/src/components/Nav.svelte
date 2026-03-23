<script lang="ts">
  import { link, location } from '../lib/router.js';

  interface NavLink {
    path: string;
    label: string;
    icon: string;
  }

  const links: NavLink[] = [
    { path: '/', label: 'Dashboard', icon: '\u25A3' },
    { path: '/sources', label: 'Sources', icon: '\u25C9' },
    { path: '/flows', label: 'Flows', icon: '\u25B6' },
    { path: '/webhooks', label: 'Webhooks', icon: '\u21C4' },
    { path: '/record', label: 'Record', icon: '\u25CF' },
    { path: '/gallery', label: 'Gallery', icon: '\u25A3' },
  ];
</script>

<nav class="sidebar">
  <div class="brand">
    <img src="/logo.png" alt="RustyTAMS" class="brand-logo" />
  </div>
  <ul>
    {#each links as { path, label, icon }}
      <li class:active={$location === path || ($location.startsWith(path + '/') && path !== '/')}>
        <a href={path} use:link>
          <span class="icon">{icon}</span>
          {label}
        </a>
      </li>
    {/each}
  </ul>
</nav>

<style>
  .sidebar {
    width: 180px;
    min-height: 100vh;
    background: var(--panel);
    border-right: 1px solid var(--border);
    padding: 1em 0;
    flex-shrink: 0;
  }
  .brand {
    padding: 1em;
    border-bottom: 1px solid var(--border);
    margin-bottom: 0.5em;
    display: flex;
    justify-content: center;
  }
  .brand-logo {
    width: 160px;
    height: 160px;
    object-fit: contain;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li a {
    display: block;
    padding: 0.5em 1em;
    color: var(--text-muted);
    text-decoration: none;
    transition: color 0.15s, background 0.15s;
  }
  li a:hover {
    color: var(--text);
    background: rgba(255,255,255,0.05);
  }
  li.active a {
    color: var(--accent);
    background: rgba(90,159,212,0.1);
    border-left: 3px solid var(--accent);
  }
  .icon {
    display: inline-block;
    width: 1.5em;
    text-align: center;
  }
</style>
