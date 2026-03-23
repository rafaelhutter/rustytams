/**
 * TAMS API client.
 *
 * Auth state is exposed as a Svelte store (`authenticated`).
 * When token refresh fails, credentials are cleared and `authenticated`
 * updates to false, which the UI reacts to by showing the login form.
 */
import { writable } from 'svelte/store';
import type { Writable } from 'svelte/store';
import { addToast } from './toast.js';
import type { PaginationInfo, Segment } from '../types/tams.js';

interface Credentials {
  username: string;
  password: string;
}

let credentials: Credentials | null = null;

let bearerToken: string | null = null;

let tokenExpiry: number = 0;

let refreshPromise: Promise<string> | null = null;

let apiBase: string = 'http://localhost:5800';

/** Svelte store: true when credentials are set, false otherwise. */
export const authenticated: Writable<boolean> = writable(false);

/** Last auth error message (set on token failure, cleared on login). */
export const authError: Writable<string | null> = writable(null);

/** Current API base URL. */
export function getApiBase(): string { return apiBase; }

/**
 * Configure API server URL.
 */
export function configure(urls: { api?: string }): void {
  if (urls.api) apiBase = urls.api.replace(/\/$/, '');
}

/**
 * Set Basic auth credentials and mark as authenticated.
 */
export function setCredentials(username: string, password: string): void {
  credentials = { username, password };
  bearerToken = null;
  tokenExpiry = 0;
  authError.set(null);
  authenticated.set(true);
}

/** Clear stored credentials, token, and mark as unauthenticated. */
export function clearCredentials(): void {
  credentials = null;
  bearerToken = null;
  tokenExpiry = 0;
  refreshPromise = null;
  authenticated.set(false);
}

/**
 * Fetch a new bearer token from the /token endpoint.
 * On failure, clears credentials so the UI returns to the login form.
 */
async function fetchNewToken(): Promise<string> {
  const basic = btoa(`${credentials!.username}:${credentials!.password}`);
  const resp = await fetch(`${apiBase}/token`, {
    method: 'POST',
    headers: {
      'Authorization': `Basic ${basic}`,
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: 'grant_type=client_credentials',
  });

  if (!resp.ok) {
    const err = await resp.json().catch(() => ({}));
    const msg = err.summary || `Authentication failed (${resp.status})`;
    authError.set(msg);
    addToast(msg, 'error');
    clearCredentials();
    throw new Error(msg);
  }

  const data = await resp.json();
  bearerToken = data.access_token;
  const expiresIn = typeof data.expires_in === 'number' ? data.expires_in : 3600;
  tokenExpiry = Date.now() + Math.max(expiresIn - 60, 0) * 1000;
  return bearerToken!;
}

/**
 * Obtain a bearer token, reusing cached token or deduplicating concurrent refreshes.
 */
async function ensureBearerToken(): Promise<string> {
  if (bearerToken && Date.now() < tokenExpiry) return bearerToken;
  if (!credentials) throw new Error('No credentials configured');
  if (!refreshPromise) {
    refreshPromise = fetchNewToken().finally(() => { refreshPromise = null; });
  }
  return refreshPromise;
}

/**
 * Handle auth-related HTTP status codes.
 * 401 -> clear credentials, re-show login (idempotent -- safe for concurrent requests).
 * 403 -> set error banner, keep credentials.
 */
function handleAuthStatus(resp: Response): void {
  if (resp.status === 401 && credentials) {
    authError.set('Session expired — please log in again');
    addToast('Session expired — please log in again', 'error');
    clearCredentials();
  } else if (resp.status === 403) {
    authError.set('Forbidden — you do not have permission for this action');
    addToast('Forbidden — you do not have permission for this action', 'error');
  }
}

/**
 * Make an authenticated request to the TAMS API.
 * Handles 401 (clear credentials) and 403 (error banner).
 */
export async function apiFetch(path: string, options: RequestInit = {}): Promise<Response> {
  const token = await ensureBearerToken();
  const url = path.startsWith('http') ? path : `${apiBase}${path}`;
  const headers: Record<string, string> = {
    'Authorization': `Bearer ${token}`,
    ...(options.headers as Record<string, string>),
  };
  const resp = await fetch(url, { ...options, headers });
  handleAuthStatus(resp);
  return resp;
}

/**
 * GET a JSON resource from the TAMS API.
 */
export async function apiGet(path: string): Promise<{ data: unknown; headers: Headers }> {
  const resp = await apiFetch(path);
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({}));
    throw new Error(err.summary || `GET ${path} failed: ${resp.status}`);
  }
  const data: unknown = await resp.json();
  return { data, headers: resp.headers };
}

/**
 * Send a mutating request (PUT, POST, DELETE) with optional JSON body.
 */
async function apiMutate(
  method: string,
  path: string,
  body?: unknown,
): Promise<{ data: unknown; status: number; headers: Headers }> {
  const options: RequestInit = { method };
  if (body !== undefined) {
    options.headers = { 'Content-Type': 'application/json' };
    options.body = JSON.stringify(body);
  }
  const resp = await apiFetch(path, options);
  const status = resp.status;
  const data: unknown = resp.headers.get('content-type')?.includes('json')
    ? await resp.json()
    : null;
  if (!resp.ok) {
    throw new Error((data as Record<string, string>)?.summary || `${method} ${path} failed: ${status}`);
  }
  return { data, status, headers: resp.headers };
}

/** PUT a JSON resource. */
export function apiPut(path: string, body?: unknown): Promise<{ data: unknown; status: number; headers: Headers }> {
  return apiMutate('PUT', path, body);
}

/** POST a JSON resource. */
export function apiPost(path: string, body?: unknown): Promise<{ data: unknown; status: number; headers: Headers }> {
  return apiMutate('POST', path, body);
}

/** DELETE a resource. */
export function apiDelete(path: string): Promise<{ data: unknown; status: number; headers: Headers }> {
  return apiMutate('DELETE', path);
}

/**
 * Parse pagination headers from a response.
 */
export function parsePagination(headers: Headers): PaginationInfo {
  return {
    limit: headers.get('x-paging-limit') ? parseInt(headers.get('x-paging-limit')!) : null,
    nextKey: headers.get('x-paging-nextkey') || null,
    count: headers.get('x-paging-count') ? parseInt(headers.get('x-paging-count')!) : null,
    timerange: headers.get('x-paging-timerange') || null,
  };
}

/**
 * Extract short format name from a TAMS format URN.
 * E.g. "urn:x-nmos:format:video" -> "video"
 */
export function formatShortName(urn: string | undefined): string {
  return urn?.split(':').pop() || '--';
}

/**
 * Pick the best download URL from a TAMS object's get_urls array.
 * Prefers presigned URLs; falls back to the first available URL.
 */
export function selectPresignedUrl(obj: { get_urls?: Array<{ url: string; presigned?: boolean }> }): string | null {
  const urls = obj?.get_urls;
  if (!urls?.length) return null;
  const presigned = urls.find(u => u.presigned);
  return presigned?.url || urls[0]?.url || null;
}
