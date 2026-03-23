/**
 * Minimal hash-based router for Svelte 5.
 *
 * Uses window.location.hash (#/path) for navigation.
 * Exports a reactive `location` store and a `link` action.
 */
import { writable } from 'svelte/store';
import type { Component } from 'svelte';

export interface CompiledRoute {
  regex: RegExp;
  paramNames: string[];
  component: Component;
}

export interface RouteMatch {
  component: Component;
  params: Record<string, string>;
}

/** Parse the current hash into path and query string. */
function parseHash(): { path: string; query: string } {
  const raw = window.location.hash.startsWith('#') ? window.location.hash.slice(1) : '/';
  const qIdx = raw.indexOf('?');
  return {
    path: qIdx >= 0 ? raw.slice(0, qIdx) : raw,
    query: qIdx >= 0 ? raw.slice(qIdx + 1) : '',
  };
}

function getHash(): string {
  return parseHash().path;
}

/** Get current query params from hash (portion after '?'). */
export function getHashParams(): URLSearchParams {
  return new URLSearchParams(parseHash().query);
}

/** Update hash query params, preserving the current path. */
export function setHashParams(params: URLSearchParams): void {
  const { path } = parseHash();
  const qs = params.toString();
  window.location.hash = '#' + path + (qs ? '?' + qs : '');
}

/** Svelte store with the current hash path (e.g. "/sources"). */
export const location = writable(getHash());

let prevHash: string = typeof window !== 'undefined' ? window.location.hash : '';

if (typeof window !== 'undefined') {
  window.addEventListener('hashchange', () => {
    const current = window.location.hash;
    if (current === prevHash) return;
    prevHash = current;
    location.update(prev => {
      const next = getHash();
      return next !== prev ? next : prev;
    });
  });
}

/**
 * Navigate programmatically.
 */
export function push(path: string): void {
  window.location.hash = '#' + path;
}

/**
 * Svelte action for links. Intercepts clicks and uses hash navigation.
 * Usage: <a href="/sources" use:link>
 */
export function link(node: HTMLAnchorElement): { destroy(): void } {
  function onClick(e: Event): void {
    e.preventDefault();
    const href = node.getAttribute('href');
    if (href) push(href);
  }
  node.addEventListener('click', onClick);
  return {
    destroy() {
      node.removeEventListener('click', onClick);
    },
  };
}

/**
 * Compile a route table into an array of { regex, paramNames, component } for fast matching.
 */
export function compileRoutes(routes: Record<string, Component>): CompiledRoute[] {
  return Object.entries(routes).map(([pattern, component]) => {
    const paramNames: string[] = [];
    const regexStr = pattern.replace(/:([^/]+)/g, (_, name: string) => {
      paramNames.push(name);
      return '([^/]+)';
    });
    return { regex: new RegExp('^' + regexStr + '$'), paramNames, component };
  });
}

/**
 * Match a path against precompiled routes.
 */
export function matchRoute(path: string, compiled: CompiledRoute[]): RouteMatch | null {
  for (const { regex, paramNames, component } of compiled) {
    const match = path.match(regex);
    if (match) {
      const params: Record<string, string> = {};
      paramNames.forEach((name, i) => {
        params[name] = decodeURIComponent(match[i + 1]);
      });
      return { component, params };
    }
  }
  return null;
}
