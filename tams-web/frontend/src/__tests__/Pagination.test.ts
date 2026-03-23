import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import Pagination from '../components/Pagination.svelte';

describe('Pagination', () => {
  it('renders nothing when no pagination info', () => {
    const { container } = render(Pagination);
    expect(container.querySelector('.pagination')).toBeNull();
  });

  it('renders page info when count and limit provided', () => {
    render(Pagination, { props: { count: 25, limit: 10, nextKey: 'abc' } });
    expect(screen.getByText('1 of 3')).toBeInTheDocument();
    expect(screen.getByText('25 total')).toBeInTheDocument();
  });

  it('renders "Page N" when count is unknown', () => {
    render(Pagination, { props: { nextKey: 'abc' } });
    expect(screen.getByText('Page 1')).toBeInTheDocument();
  });

  it('prev button disabled on first page', () => {
    render(Pagination, { props: { count: 20, limit: 10, nextKey: 'key2' } });
    const [prev] = screen.getAllByRole('button');
    expect(prev).toBeDisabled();
  });

  it('next button disabled when no nextKey', () => {
    render(Pagination, { props: { count: 5, limit: 10 } });
    const buttons = screen.getAllByRole('button');
    const next = buttons[buttons.length - 1];
    expect(next).toBeDisabled();
  });

  it('calls onPage with nextKey when clicking next', async () => {
    const onPage = vi.fn<[{ key: string | null }], void>();
    render(Pagination, { props: { count: 20, limit: 10, nextKey: 'page2key', onPage } });
    const buttons = screen.getAllByRole('button');
    const next = buttons[buttons.length - 1];
    await fireEvent.click(next);
    expect(onPage).toHaveBeenCalledWith({ key: 'page2key' });
  });

  it('advances page number on next click', async () => {
    const onPage = vi.fn<[{ key: string | null }], void>();
    render(Pagination, { props: { count: 30, limit: 10, nextKey: 'page2key', onPage } });
    const buttons = screen.getAllByRole('button');
    await fireEvent.click(buttons[buttons.length - 1]);
    expect(screen.getByText('2 of 3')).toBeInTheDocument();
  });

  it('calls onPage with null key when going back to first page', async () => {
    const onPage = vi.fn<[{ key: string | null }], void>();
    const { rerender } = render(Pagination, {
      props: { count: 30, limit: 10, nextKey: 'page2key', onPage },
    });
    const buttons = screen.getAllByRole('button');
    // Go to page 2
    await fireEvent.click(buttons[buttons.length - 1]);
    // Update nextKey for page 2
    await rerender({ count: 30, limit: 10, nextKey: 'page3key', onPage });
    // Go back to page 1
    const updatedButtons = screen.getAllByRole('button');
    await fireEvent.click(updatedButtons[0]);
    expect(onPage).toHaveBeenLastCalledWith({ key: null });
    expect(screen.getByText('1 of 3')).toBeInTheDocument();
  });

  it('shows total and disables next when count provided but nextKey is null', () => {
    render(Pagination, { props: { count: 15, limit: 10, nextKey: null } });
    expect(screen.getByText('15 total')).toBeInTheDocument();
    expect(screen.getByText('1 of 2')).toBeInTheDocument();
    const buttons = screen.getAllByRole('button');
    const next = buttons[buttons.length - 1];
    expect(next).toBeDisabled();
  });

  it('shows count total even on last page with no nextKey', async () => {
    const onPage = vi.fn<[{ key: string | null }], void>();
    const { rerender } = render(Pagination, {
      props: { count: 15, limit: 10, nextKey: 'page2key', onPage },
    });
    // Advance to page 2
    const buttons = screen.getAllByRole('button');
    await fireEvent.click(buttons[buttons.length - 1]);
    expect(screen.getByText('2 of 2')).toBeInTheDocument();
    // Simulate last page: nextKey becomes null
    await rerender({ count: 15, limit: 10, nextKey: null, onPage });
    const updatedButtons = screen.getAllByRole('button');
    const next = updatedButtons[updatedButtons.length - 1];
    expect(next).toBeDisabled();
    expect(screen.getByText('15 total')).toBeInTheDocument();
  });

  it('reset() returns to page 1 after advancing', async () => {
    const onPage = vi.fn<[{ key: string | null }], void>();
    const { component } = render(Pagination, {
      props: { count: 30, limit: 10, nextKey: 'page2key', onPage },
    });
    // Advance to page 2
    const buttons = screen.getAllByRole('button');
    await fireEvent.click(buttons[buttons.length - 1]);
    expect(screen.getByText('2 of 3')).toBeInTheDocument();
    // Call the exported reset function
    flushSync(() => {
      (component as unknown as { reset: () => void }).reset();
    });
    expect(screen.getByText('1 of 3')).toBeInTheDocument();
  });
});
