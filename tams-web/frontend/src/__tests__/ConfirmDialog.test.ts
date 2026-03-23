import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import ConfirmDialog from '../components/ConfirmDialog.svelte';
import Spinner from '../components/Spinner.svelte';

describe('ConfirmDialog', () => {
  it('renders nothing when open=false', () => {
    const { container } = render(ConfirmDialog, { props: { open: false } });
    expect(container.querySelector('.overlay')).toBeNull();
  });

  it('renders dialog content when open=true', () => {
    render(ConfirmDialog, {
      props: { open: true, title: 'Delete Item', message: 'This cannot be undone.' },
    });
    expect(screen.getByText('Delete Item')).toBeInTheDocument();
    expect(screen.getByText('This cannot be undone.')).toBeInTheDocument();
    expect(screen.getByText('Cancel')).toBeInTheDocument();
    expect(screen.getByText('Confirm')).toBeInTheDocument();
  });

  it('confirm button calls onConfirm', async () => {
    const onConfirm = vi.fn<[], void>();
    render(ConfirmDialog, {
      props: { open: true, title: 'Delete Item', onConfirm },
    });
    await fireEvent.click(screen.getByText('Confirm'));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('cancel button calls onCancel', async () => {
    const onCancel = vi.fn<[], void>();
    render(ConfirmDialog, {
      props: { open: true, onCancel },
    });
    await fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('buttons disabled when loading=true', () => {
    render(ConfirmDialog, {
      props: { open: true, loading: true },
    });
    expect(screen.getByText('Cancel')).toBeDisabled();
    expect(screen.getByText('Working...')).toBeDisabled();
  });

  it('shows "Working..." when loading=true', () => {
    render(ConfirmDialog, {
      props: { open: true, loading: true, confirmLabel: 'Delete' },
    });
    expect(screen.getByText('Working...')).toBeInTheDocument();
    expect(screen.queryByText('Delete')).toBeNull();
  });

  it('confirm button has danger class when danger=true', () => {
    const { container } = render(ConfirmDialog, {
      props: { open: true, danger: true, title: 'Delete' },
    });
    const confirmBtn = container.querySelector('.btn-confirm') as HTMLElement;
    expect(confirmBtn.className).toContain('danger');
  });

  it('confirm button lacks danger class when danger=false', () => {
    const { container } = render(ConfirmDialog, {
      props: { open: true, danger: false, title: 'Action' },
    });
    const confirmBtn = container.querySelector('.btn-confirm') as HTMLElement;
    expect(confirmBtn.className).not.toContain('danger');
  });

  it('renders custom confirmLabel', () => {
    render(ConfirmDialog, {
      props: { open: true, confirmLabel: 'Yes, delete it' },
    });
    expect(screen.getByText('Yes, delete it')).toBeInTheDocument();
  });

  it('ESC key triggers onCancel', async () => {
    const onCancel = vi.fn<[], void>();
    render(ConfirmDialog, {
      props: { open: true, onCancel },
    });
    const overlay = screen.getByRole('dialog');
    await fireEvent.keyDown(overlay, { key: 'Escape' });
    expect(onCancel).toHaveBeenCalledOnce();
  });
});

describe('Spinner', () => {
  it('renders a span with class "spinner"', () => {
    const { container } = render(Spinner);
    const span = container.querySelector('span.spinner');
    expect(span).toBeInTheDocument();
  });

  it('applies default size of 1.5em', () => {
    const { container } = render(Spinner);
    const span = container.querySelector('span.spinner') as HTMLElement;
    expect(span.style.width).toBe('1.5em');
    expect(span.style.height).toBe('1.5em');
  });

  it('applies custom size', () => {
    const { container } = render(Spinner, { props: { size: '3em' } });
    const span = container.querySelector('span.spinner') as HTMLElement;
    expect(span.style.width).toBe('3em');
    expect(span.style.height).toBe('3em');
  });
});
