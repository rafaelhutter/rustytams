import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import type { Mock } from 'vitest';

// Mock dependencies shared by all page components.
vi.mock('../lib/api.js', () => ({
  apiGet: vi.fn(),
  apiPut: vi.fn(),
  apiPost: vi.fn(),
  apiDelete: vi.fn(),
  parsePagination: vi.fn(() => ({ limit: null, nextKey: null, count: null })),
  formatShortName: (urn: string | undefined) => urn?.split(':').pop() || '--',
}));

vi.mock('../lib/router.js', () => ({
  push: vi.fn(),
  link: () => ({ destroy() {} }),
  location: { subscribe: () => () => {} },
  getHashParams: () => new URLSearchParams(),
  setHashParams: vi.fn(),
  hashParams: { subscribe: () => () => {} },
}));

vi.mock('../lib/toast.js', () => ({
  addToast: vi.fn(),
  toasts: { subscribe: () => () => {} },
  removeToast: vi.fn(),
}));

vi.mock('../lib/query.js', () => ({
  buildSourcesQuery: vi.fn(() => '/sources'),
  buildFlowsQuery: vi.fn(() => '/flows'),
  buildWebhooksQuery: vi.fn(() => '/service/webhooks'),
  WEBHOOK_EVENTS: ['flows/created', 'flows/updated', 'flows/deleted',
    'flows/segments_added', 'flows/segments_deleted',
    'sources/created', 'sources/updated', 'sources/deleted'],
}));

vi.mock('../lib/timerange.js', () => ({
  formatTimerangeDisplay: vi.fn(() => ({ raw: '--', display: '--' })),
}));

import { apiGet, apiPut, apiPost, apiDelete } from '../lib/api.js';
import Sources from '../pages/Sources.svelte';
import Flows from '../pages/Flows.svelte';
import Webhooks from '../pages/Webhooks.svelte';

const mockApiGet = apiGet as Mock;
const mockApiPut = apiPut as Mock;
const mockApiPost = apiPost as Mock;
const mockApiDelete = apiDelete as Mock;

beforeEach(() => {
  vi.clearAllMocks();
});

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------
describe('Sources page — error states', () => {
  it('shows fetch error when apiGet rejects', async () => {
    mockApiGet.mockRejectedValue(new Error('Network timeout'));
    render(Sources);

    await waitFor(() => {
      expect(screen.getByText('Network timeout')).toBeInTheDocument();
    });
    const el = screen.getByText('Network timeout');
    expect(el.classList.contains('error-text')).toBe(true);
  });

  it('shows create error when apiPut rejects', async () => {
    mockApiGet.mockResolvedValue({ data: [], headers: new Headers() });
    mockApiPut.mockRejectedValue(new Error('409 Conflict'));
    render(Sources);

    // Wait for initial load to complete
    await waitFor(() => {
      expect(screen.queryByText('Loading...')).not.toBeInTheDocument();
    });

    // Open the create form
    await fireEvent.click(screen.getByText('+ Create Source'));

    // The ID field is auto-populated via crypto.randomUUID(); fill format
    // is already defaulted to video. Submit the form.
    await fireEvent.click(screen.getByText('Create'));

    await waitFor(() => {
      expect(screen.getByText('409 Conflict')).toBeInTheDocument();
    });
    const el = screen.getByText('409 Conflict');
    expect(el.classList.contains('error-text')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Flows
// ---------------------------------------------------------------------------
describe('Flows page — error states', () => {
  it('shows fetch error when apiGet rejects', async () => {
    mockApiGet.mockRejectedValue(new Error('Network timeout'));
    render(Flows);

    await waitFor(() => {
      expect(screen.getByText('Network timeout')).toBeInTheDocument();
    });
    const el = screen.getByText('Network timeout');
    expect(el.classList.contains('error-text')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Webhooks
// ---------------------------------------------------------------------------
describe('Webhooks page — error states', () => {
  it('shows fetch error when apiGet rejects', async () => {
    mockApiGet.mockRejectedValue(new Error('Network timeout'));
    render(Webhooks);

    await waitFor(() => {
      expect(screen.getByText('Network timeout')).toBeInTheDocument();
    });
    const el = screen.getByText('Network timeout');
    expect(el.classList.contains('error-text')).toBe(true);
  });

  it('shows create error when apiPost rejects', async () => {
    mockApiGet.mockResolvedValue({
      data: [],
      headers: new Headers({ 'x-paging-count': '0' }),
    });
    mockApiPost.mockRejectedValue(new Error('Bad Request'));
    render(Webhooks);

    // Wait for list load
    await waitFor(() => {
      expect(screen.queryByText('Loading...')).not.toBeInTheDocument();
    });

    // Open create form
    await fireEvent.click(screen.getByText('+ New Webhook'));

    // Fill URL
    const urlInput = screen.getByPlaceholderText('https://example.com/callback');
    await fireEvent.input(urlInput, { target: { value: 'https://example.com/hook' } });

    // Select at least one event checkbox
    const firstCheckbox = screen.getAllByRole('checkbox')[0];
    await fireEvent.change(firstCheckbox);

    // Submit
    await fireEvent.click(screen.getByText('Create'));

    await waitFor(() => {
      expect(screen.getByText('Bad Request')).toBeInTheDocument();
    });
    const el = screen.getByText('Bad Request');
    expect(el.classList.contains('error-text')).toBe(true);
  });

  it('shows delete error when apiDelete rejects', async () => {
    mockApiGet.mockResolvedValue({
      data: [{
        id: 'wh-1',
        url: 'https://example.com',
        events: ['flows/created'],
        status: 'created',
      }],
      headers: new Headers(),
    });
    mockApiDelete.mockRejectedValue(new Error('Server Error'));
    render(Webhooks);

    // Wait for list to render
    await waitFor(() => {
      expect(screen.getByText('https://example.com')).toBeInTheDocument();
    });

    // Click Delete to show confirmation
    await fireEvent.click(screen.getByText('Delete'));

    // Confirm the deletion
    await fireEvent.click(screen.getByText('Yes'));

    await waitFor(() => {
      expect(screen.getByText('Server Error')).toBeInTheDocument();
    });
    const el = screen.getByText('Server Error');
    expect(el.classList.contains('error-text')).toBe(true);
  });
});
