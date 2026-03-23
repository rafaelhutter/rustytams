import { writable } from 'svelte/store';
import type { Writable } from 'svelte/store';

const MAX_TOASTS = 5;

export interface ToastItem {
  id: number;
  message: string;
  type: string;
}

export type ToastType = 'error' | 'success' | 'info' | 'warning';

export const toasts: Writable<ToastItem[]> = writable([]);

const timers: Map<number, ReturnType<typeof setTimeout>> = new Map();

let nextId = 0;

/**
 * Add a toast notification.
 */
export function addToast(message: string, type: ToastType = 'error', duration?: number): void {
  const logFn = type === 'error' ? console.error : type === 'warning' ? console.warn : console.log;
  logFn(`[toast:${type}] ${message}`);
  const id = ++nextId;
  const timeout = duration ?? (type === 'error' ? 6000 : 3000);
  toasts.update(t => {
    const next = [...t, { id, message, type }];
    // Evict oldest if over cap
    while (next.length > MAX_TOASTS) {
      const evicted = next.shift();
      if (evicted) {
        const tid = timers.get(evicted.id);
        if (tid) { clearTimeout(tid); timers.delete(evicted.id); }
      }
    }
    return next;
  });
  timers.set(id, setTimeout(() => removeToast(id), timeout));
}

/**
 * Remove a toast by id, cancelling its auto-dismiss timer.
 */
export function removeToast(id: number): void {
  const tid = timers.get(id);
  if (tid) { clearTimeout(tid); timers.delete(id); }
  toasts.update(t => t.filter(toast => toast.id !== id));
}
