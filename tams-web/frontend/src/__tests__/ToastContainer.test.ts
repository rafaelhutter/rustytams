import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { toasts, removeToast } from '../lib/toast.js';
import type { ToastItem } from '../lib/toast.js';
import ToastContainer from '../components/ToastContainer.svelte';

beforeEach(() => {
  // Reset the store between tests
  toasts.set([]);
});

describe('ToastContainer', () => {
  it('renders toasts from the store', () => {
    toasts.set([{ id: 1, message: 'Test error', type: 'error' }]);
    render(ToastContainer);

    expect(screen.getByText('Test error')).toBeInTheDocument();
  });

  it('applies the correct type class to toasts', () => {
    toasts.set([{ id: 1, message: 'Error toast', type: 'error' }]);
    const { container } = render(ToastContainer);

    const toast = container.querySelector('.toast') as HTMLElement;
    expect(toast.classList.contains('error')).toBe(true);
  });

  it('applies success class for success toasts', () => {
    toasts.set([{ id: 1, message: 'Success toast', type: 'success' }]);
    const { container } = render(ToastContainer);

    const toast = container.querySelector('.toast') as HTMLElement;
    expect(toast.classList.contains('success')).toBe(true);
  });

  it('applies info class for info toasts', () => {
    toasts.set([{ id: 1, message: 'Info toast', type: 'info' }]);
    const { container } = render(ToastContainer);

    const toast = container.querySelector('.toast') as HTMLElement;
    expect(toast.classList.contains('info')).toBe(true);
  });

  it('renders multiple toasts', () => {
    toasts.set([
      { id: 1, message: 'First error', type: 'error' },
      { id: 2, message: 'Second info', type: 'info' },
    ]);
    render(ToastContainer);

    expect(screen.getByText('First error')).toBeInTheDocument();
    expect(screen.getByText('Second info')).toBeInTheDocument();
  });

  it('dismiss button removes the toast', async () => {
    toasts.set([{ id: 1, message: 'Dismissable', type: 'info' }]);
    render(ToastContainer);

    expect(screen.getByText('Dismissable')).toBeInTheDocument();

    const dismissBtn = screen.getByRole('button', { name: 'Dismiss' });
    await fireEvent.click(dismissBtn);

    await waitFor(() => {
      expect(screen.queryByText('Dismissable')).not.toBeInTheDocument();
    });
  });

  it('renders nothing when toasts store is empty', () => {
    toasts.set([]);
    const { container } = render(ToastContainer);

    expect(container.querySelector('.toast-container')).toBeNull();
  });
});
