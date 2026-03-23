import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { toasts, addToast, removeToast } from '../lib/toast.js';
import type { ToastItem } from '../lib/toast.js';

describe('toast store', () => {
  beforeEach(() => {
    // Reset the store to empty before each test
    toasts.set([]);
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('starts empty', () => {
    expect(get(toasts)).toEqual([]);
  });

  it('addToast adds a toast to the store', () => {
    addToast('Something happened', 'info');
    const current: ToastItem[] = get(toasts);
    expect(current).toHaveLength(1);
    expect(current[0].message).toBe('Something happened');
    expect(current[0].type).toBe('info');
  });

  it('removeToast removes a toast by id', () => {
    addToast('First', 'info');
    addToast('Second', 'error');
    const before: ToastItem[] = get(toasts);
    expect(before).toHaveLength(2);

    const idToRemove = before[0].id;
    removeToast(idToRemove);

    const after: ToastItem[] = get(toasts);
    expect(after).toHaveLength(1);
    expect(after[0].message).toBe('Second');
  });

  it('addToast auto-generates incrementing ids', () => {
    addToast('A', 'info');
    addToast('B', 'success');
    addToast('C', 'error');
    const current: ToastItem[] = get(toasts);
    expect(current[0].id).toBeLessThan(current[1].id);
    expect(current[1].id).toBeLessThan(current[2].id);
  });

  it('multiple toasts can coexist', () => {
    addToast('Error toast', 'error');
    addToast('Success toast', 'success');
    addToast('Info toast', 'info');
    const current: ToastItem[] = get(toasts);
    expect(current).toHaveLength(3);
    expect(current.map((t) => t.message)).toEqual([
      'Error toast',
      'Success toast',
      'Info toast',
    ]);
  });
});
