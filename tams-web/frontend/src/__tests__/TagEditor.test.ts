import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import type { Mock } from 'vitest';
import TagEditor from '../components/TagEditor.svelte';

// Mock the api module
vi.mock('../lib/api.js', () => ({
  apiPut: vi.fn(() => Promise.resolve({ data: null, status: 204 })),
  apiDelete: vi.fn(() => Promise.resolve({ data: null, status: 204 })),
}));

import { apiPut, apiDelete } from '../lib/api.js';

const mockApiPut = apiPut as Mock;
const mockApiDelete = apiDelete as Mock;

describe('TagEditor', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders "No tags" when tags is empty', () => {
    render(TagEditor, { props: { tags: {}, basePath: '/sources/123' } });
    expect(screen.getByText('No tags')).toBeInTheDocument();
  });

  it('renders existing tags in a table', () => {
    render(TagEditor, {
      props: {
        tags: { location: 'studio-a', dept: 'news' },
        basePath: '/sources/123',
      },
    });
    expect(screen.getByText('location')).toBeInTheDocument();
    expect(screen.getByText('studio-a')).toBeInTheDocument();
    expect(screen.getByText('dept')).toBeInTheDocument();
    expect(screen.getByText('news')).toBeInTheDocument();
  });

  it('renders array tag values as comma-separated', () => {
    render(TagEditor, {
      props: {
        tags: { topics: ['news', 'sport'] },
        basePath: '/sources/123',
      },
    });
    expect(screen.getByText('news, sport')).toBeInTheDocument();
  });

  it('add button disabled when tag name empty', () => {
    render(TagEditor, { props: { tags: {}, basePath: '/sources/123' } });
    const addBtn = screen.getByText('Add');
    expect(addBtn).toBeDisabled();
  });

  it('calls apiPut with correct path and value on add', async () => {
    const onUpdate = vi.fn<[Record<string, string | string[]>], void>();
    render(TagEditor, {
      props: { tags: {}, basePath: '/sources/123', onUpdate },
    });

    const inputs = screen.getAllByRole('textbox');
    await fireEvent.input(inputs[0], { target: { value: 'location' } });
    await fireEvent.input(inputs[1], { target: { value: 'studio-b' } });
    await fireEvent.click(screen.getByText('Add'));

    expect(mockApiPut).toHaveBeenCalledWith('/sources/123/tags/location', 'studio-b');
    expect(onUpdate).toHaveBeenCalledWith({ location: 'studio-b' });
  });

  it('splits comma-separated values into array on add', async () => {
    const onUpdate = vi.fn<[Record<string, string | string[]>], void>();
    render(TagEditor, {
      props: { tags: {}, basePath: '/sources/123', onUpdate },
    });

    const inputs = screen.getAllByRole('textbox');
    await fireEvent.input(inputs[0], { target: { value: 'topics' } });
    await fireEvent.input(inputs[1], { target: { value: 'news, sport' } });
    await fireEvent.click(screen.getByText('Add'));

    expect(mockApiPut).toHaveBeenCalledWith('/sources/123/tags/topics', ['news', 'sport']);
    expect(onUpdate).toHaveBeenCalledWith({ topics: ['news', 'sport'] });
  });

  it('calls apiDelete on tag removal', async () => {
    const onUpdate = vi.fn<[Record<string, string | string[]>], void>();
    render(TagEditor, {
      props: {
        tags: { location: 'studio-a' },
        basePath: '/sources/123',
        onUpdate,
      },
    });

    const deleteBtn = screen.getByText('X');
    await fireEvent.click(deleteBtn);

    expect(mockApiDelete).toHaveBeenCalledWith('/sources/123/tags/location');
    expect(onUpdate).toHaveBeenCalledWith({});
  });

  it('preserves existing tags when adding new one', async () => {
    const onUpdate = vi.fn<[Record<string, string | string[]>], void>();
    render(TagEditor, {
      props: {
        tags: { location: 'studio-a' },
        basePath: '/sources/123',
        onUpdate,
      },
    });

    const inputs = screen.getAllByRole('textbox');
    await fireEvent.input(inputs[0], { target: { value: 'dept' } });
    await fireEvent.input(inputs[1], { target: { value: 'news' } });
    await fireEvent.click(screen.getByText('Add'));

    expect(onUpdate).toHaveBeenCalledWith({ location: 'studio-a', dept: 'news' });
  });

  it('shows error message when apiPut rejects', async () => {
    mockApiPut.mockRejectedValueOnce(new Error('Network error'));
    render(TagEditor, {
      props: { tags: {}, basePath: '/sources/123' },
    });

    const inputs = screen.getAllByRole('textbox');
    await fireEvent.input(inputs[0], { target: { value: 'location' } });
    await fireEvent.input(inputs[1], { target: { value: 'studio-a' } });
    await fireEvent.click(screen.getByText('Add'));

    await waitFor(() => {
      expect(screen.getByText('Network error')).toBeInTheDocument();
    });
    const errorEl = screen.getByText('Network error');
    expect(errorEl.classList.contains('error-text')).toBe(true);
  });

  it('shows error message when apiDelete rejects', async () => {
    mockApiDelete.mockRejectedValueOnce(new Error('Delete failed'));
    render(TagEditor, {
      props: {
        tags: { location: 'studio-a' },
        basePath: '/sources/123',
      },
    });

    await fireEvent.click(screen.getByText('X'));

    await waitFor(() => {
      expect(screen.getByText('Delete failed')).toBeInTheDocument();
    });
    const errorEl = screen.getByText('Delete failed');
    expect(errorEl.classList.contains('error-text')).toBe(true);
  });

  it('add button disabled while saving', async () => {
    let resolveApiPut: (value: { data: null; status: number }) => void;
    mockApiPut.mockImplementationOnce(() => new Promise((resolve) => { resolveApiPut = resolve; }));
    render(TagEditor, {
      props: { tags: {}, basePath: '/sources/123' },
    });

    const inputs = screen.getAllByRole('textbox');
    await fireEvent.input(inputs[0], { target: { value: 'location' } });
    await fireEvent.input(inputs[1], { target: { value: 'studio-a' } });

    // Click Add but don't resolve yet -- saving should be in progress
    fireEvent.click(screen.getByText('Add'));

    await waitFor(() => {
      expect(screen.getByText('Add')).toBeDisabled();
    });

    // Resolve to clean up
    resolveApiPut!({ data: null, status: 204 });
  });

  it('delete button shows "..." while deleting that specific tag', async () => {
    let resolveApiDelete: (value: { data: null; status: number }) => void;
    mockApiDelete.mockImplementationOnce(() => new Promise((resolve) => { resolveApiDelete = resolve; }));
    render(TagEditor, {
      props: {
        tags: { location: 'studio-a', dept: 'news' },
        basePath: '/sources/123',
      },
    });

    const deleteButtons = screen.getAllByRole('button', { name: /X/ });
    // Click the first delete button (location)
    fireEvent.click(deleteButtons[0]);

    await waitFor(() => {
      expect(screen.getByText('...')).toBeInTheDocument();
    });
    // The other delete button should still show "X"
    expect(screen.getByText('X')).toBeInTheDocument();

    // Resolve to clean up
    resolveApiDelete!({ data: null, status: 204 });
  });

  it('handles empty value correctly -- adds as empty string, not array', async () => {
    const onUpdate = vi.fn<[Record<string, string | string[]>], void>();
    render(TagEditor, {
      props: { tags: {}, basePath: '/sources/123', onUpdate },
    });

    const inputs = screen.getAllByRole('textbox');
    await fireEvent.input(inputs[0], { target: { value: 'status' } });
    // Leave value empty (default is '')
    await fireEvent.click(screen.getByText('Add'));

    expect(mockApiPut).toHaveBeenCalledWith('/sources/123/tags/status', '');
    expect(onUpdate).toHaveBeenCalledWith({ status: '' });
  });
});
