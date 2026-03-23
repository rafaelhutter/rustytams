import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import type { Mock } from 'vitest';
import FlowDetail from '../pages/FlowDetail.svelte';

// Mock dependencies
vi.mock('../lib/api.js', () => ({
  apiGet: vi.fn(),
  apiDelete: vi.fn(),
  apiPut: vi.fn(),
  apiPost: vi.fn(),
  parsePagination: vi.fn(() => ({ limit: null, nextKey: null, count: null })),
  formatShortName: vi.fn((urn: string | undefined) => urn?.split(':').pop() || '--'),
}));

vi.mock('../lib/router.js', () => ({
  push: vi.fn(),
  link: () => ({ destroy() {} }),
  location: { subscribe: () => () => {} },
  hashParams: { subscribe: () => () => {} },
  getHashParams: () => new URLSearchParams(),
  setHashParams: vi.fn(),
}));

vi.mock('../lib/toast.js', () => ({
  addToast: vi.fn(),
}));

vi.mock('../lib/timerange.js', () => ({
  formatTimerangeDisplay: vi.fn((tr: string | undefined) => tr || '--'),
}));

vi.mock('../lib/timeline.js', () => ({
  computeTimelineBounds: vi.fn(() => ({ startSec: 0, endSec: 10 })),
  segmentBarStyle: vi.fn(() => ''),
}));

vi.mock('../lib/query.js', () => ({
  buildFlowQuery: vi.fn((id: string) => `/flows/${id}`),
  buildSegmentsQuery: vi.fn(() => '/flows/flow-uuid-1234/segments'),
}));

import { apiGet, apiDelete } from '../lib/api.js';
import { push } from '../lib/router.js';
import { addToast } from '../lib/toast.js';

const mockApiGet = apiGet as Mock;
const mockApiDelete = apiDelete as Mock;
const mockPush = push as Mock;
const mockAddToast = addToast as Mock;

interface MockFlow {
  id: string;
  source_id: string;
  format: string;
  codec: string;
  label: string;
  timerange: string;
  tags: Record<string, string>;
}

const mockFlow: MockFlow = {
  id: 'flow-uuid-1234',
  source_id: 'src-uuid-1234',
  format: 'urn:x-nmos:format:video',
  codec: 'video/h264',
  label: 'Test Flow',
  timerange: '[0:0_10:0)',
  tags: {},
};

interface MockSegment {
  timerange: string;
  object_id: string;
}

const mockSegments: MockSegment[] = [];

describe('FlowDetail', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockApiGet.mockImplementation((path: string) => {
      if (path.includes('/segments'))
        return Promise.resolve({ data: mockSegments, headers: new Headers() });
      if (path.includes('/flows/'))
        return Promise.resolve({ data: mockFlow, headers: new Headers() });
      return Promise.reject(new Error('Unknown'));
    });
  });

  it('shows loading state initially', () => {
    // Make apiGet hang so loading stays visible
    mockApiGet.mockImplementation(() => new Promise(() => {}));
    render(FlowDetail, { props: { params: { id: 'flow-uuid-1234' } } });
    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('displays flow label after load', async () => {
    render(FlowDetail, { props: { params: { id: 'flow-uuid-1234' } } });
    await waitFor(() => {
      expect(screen.getByText('Test Flow')).toBeInTheDocument();
    });
  });

  it('shows tab buttons (Properties, Segments, Storage)', async () => {
    render(FlowDetail, { props: { params: { id: 'flow-uuid-1234' } } });
    await waitFor(() => {
      expect(screen.getByText('Properties')).toBeInTheDocument();
    });
    expect(screen.getByText('Segments')).toBeInTheDocument();
    expect(screen.getByText('Storage')).toBeInTheDocument();
  });

  it('delete with 204 navigates to /flows', async () => {
    mockApiDelete.mockResolvedValueOnce({
      data: null,
      status: 204,
      headers: new Headers(),
    });

    render(FlowDetail, { props: { params: { id: 'flow-uuid-1234' } } });

    // Wait for flow to load
    await waitFor(() => {
      expect(screen.getByText('Test Flow')).toBeInTheDocument();
    });

    // Click the Delete button to open the confirm dialog
    await fireEvent.click(screen.getByText('Delete'));

    // The confirm dialog should now be open; click the confirm button
    // ConfirmDialog uses confirmLabel="Delete" so there will be two "Delete" texts;
    // the confirm button inside the dialog has class btn-confirm
    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });
    const confirmBtn = screen.getByRole('dialog').querySelector('.btn-confirm') as HTMLElement;
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(mockApiDelete).toHaveBeenCalledWith('/flows/flow-uuid-1234');
      expect(mockPush).toHaveBeenCalledWith('/flows');
      expect(mockAddToast).toHaveBeenCalledWith('Flow deleted', 'success');
    });
  });

  it('delete with 202 shows deletion-in-progress panel', async () => {
    mockApiDelete.mockResolvedValueOnce({
      data: { id: 'del-req-123', status: 'created', timerange_to_delete: '_' },
      status: 202,
      headers: new Headers(),
    });

    render(FlowDetail, { props: { params: { id: 'flow-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Test Flow')).toBeInTheDocument();
    });

    await fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });
    const confirmBtn = screen.getByRole('dialog').querySelector('.btn-confirm') as HTMLElement;
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(screen.getByText('Flow Deletion In Progress')).toBeInTheDocument();
    });
    expect(screen.getByText('del-req-123')).toBeInTheDocument();
    expect(screen.getByText('created')).toBeInTheDocument();
    expect(mockAddToast).toHaveBeenCalledWith('Flow deletion in progress', 'info');
    // Should NOT navigate away
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('delete error shows error text', async () => {
    mockApiDelete.mockRejectedValueOnce(new Error('Server exploded'));

    render(FlowDetail, { props: { params: { id: 'flow-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Test Flow')).toBeInTheDocument();
    });

    await fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeInTheDocument();
    });
    const confirmBtn = screen.getByRole('dialog').querySelector('.btn-confirm') as HTMLElement;
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(screen.getByText('Server exploded')).toBeInTheDocument();
    });
    // Should not navigate
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('shows error text when apiGet fails', async () => {
    mockApiGet.mockRejectedValue(new Error('Network failure'));

    render(FlowDetail, { props: { params: { id: 'flow-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Network failure')).toBeInTheDocument();
    });
  });
});
