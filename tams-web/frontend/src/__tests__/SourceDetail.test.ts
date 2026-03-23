import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import type { Mock } from 'vitest';
import SourceDetail from '../pages/SourceDetail.svelte';

vi.mock('../lib/api.js', () => ({
  apiGet: vi.fn(),
  apiPut: vi.fn(() => Promise.resolve({ data: null, status: 204 })),
  apiDelete: vi.fn(() => Promise.resolve({ data: null, status: 204 })),
  formatShortName: (urn: string | undefined) => urn?.split(':').pop() || '--',
}));

vi.mock('../lib/router.js', () => ({
  push: vi.fn(),
  link: () => ({ destroy() {} }),
  location: { subscribe: () => () => {} },
}));

vi.mock('../lib/toast.js', () => ({
  addToast: vi.fn(),
}));

import { apiGet, apiPut, apiDelete } from '../lib/api.js';
import { push } from '../lib/router.js';
import { addToast } from '../lib/toast.js';

const mockApiGet = apiGet as Mock;
const mockApiPut = apiPut as Mock;
const mockApiDelete = apiDelete as Mock;

interface MockSource {
  id: string;
  format: string;
  label: string;
  description: string;
  tags: Record<string, string>;
}

interface MockFlow {
  id: string;
  format: string;
  codec: string;
  label: string;
}

const mockSource: MockSource = {
  id: 'src-uuid-1234',
  format: 'urn:x-nmos:format:video',
  label: 'Camera 1',
  description: 'Main camera',
  tags: {},
};

const mockFlows: MockFlow[] = [
  { id: 'flow-1', format: 'urn:x-nmos:format:video', codec: 'video/h264', label: 'Flow 1' },
];

beforeEach(() => {
  vi.clearAllMocks();
  mockApiGet.mockImplementation((path: string) => {
    if (path.startsWith('/sources/'))
      return Promise.resolve({ data: mockSource, headers: new Headers() });
    if (path.startsWith('/flows?'))
      return Promise.resolve({ data: mockFlows, headers: new Headers() });
    return Promise.reject(new Error('Unknown path'));
  });
});

describe('SourceDetail', () => {
  it('shows loading state initially', () => {
    render(SourceDetail, { props: { params: { id: 'src-uuid-1234' } } });
    expect(screen.getByText(/Loading/)).toBeInTheDocument();
  });

  it('displays source properties after load', async () => {
    render(SourceDetail, { props: { params: { id: 'src-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Camera 1')).toBeInTheDocument();
    });
    expect(screen.getByText('src-uuid-1234')).toBeInTheDocument();
    // "video" appears in both source format badge and flow format badge -- use getAllByText
    expect(screen.getAllByText('video').length).toBeGreaterThanOrEqual(1);
  });

  it('shows linked flows in table', async () => {
    render(SourceDetail, { props: { params: { id: 'src-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Flow 1')).toBeInTheDocument();
    });
    expect(screen.getByText('video/h264')).toBeInTheDocument();
    expect(screen.getByText('Flows (1)')).toBeInTheDocument();
  });

  it('shows "No flows" when empty', async () => {
    mockApiGet.mockImplementation((path: string) => {
      if (path.startsWith('/sources/'))
        return Promise.resolve({ data: mockSource, headers: new Headers() });
      if (path.startsWith('/flows?'))
        return Promise.resolve({ data: [], headers: new Headers() });
      return Promise.reject(new Error('Unknown path'));
    });

    render(SourceDetail, { props: { params: { id: 'src-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('No flows for this source.')).toBeInTheDocument();
    });
  });

  it('save label calls apiPut with correct path', async () => {
    render(SourceDetail, { props: { params: { id: 'src-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Camera 1')).toBeInTheDocument();
    });

    const labelInput = screen.getByPlaceholderText('Source label');
    await fireEvent.input(labelInput, { target: { value: 'Camera 2' } });

    const saveButtons = screen.getAllByText('Save');
    await fireEvent.click(saveButtons[0]);

    expect(mockApiPut).toHaveBeenCalledWith('/sources/src-uuid-1234/label', 'Camera 2');
  });

  it('save empty label calls apiDelete to clear field', async () => {
    render(SourceDetail, { props: { params: { id: 'src-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Camera 1')).toBeInTheDocument();
    });

    const labelInput = screen.getByPlaceholderText('Source label');
    await fireEvent.input(labelInput, { target: { value: '' } });

    const saveButtons = screen.getAllByText('Save');
    await fireEvent.click(saveButtons[0]);

    expect(mockApiDelete).toHaveBeenCalledWith('/sources/src-uuid-1234/label');
  });

  it('shows error text when apiGet fails', async () => {
    mockApiGet.mockRejectedValue(new Error('Server unavailable'));

    render(SourceDetail, { props: { params: { id: 'src-uuid-1234' } } });

    await waitFor(() => {
      expect(screen.getByText('Server unavailable')).toBeInTheDocument();
    });
    const errorEl = screen.getByText('Server unavailable');
    expect(errorEl.classList.contains('error-text')).toBe(true);
  });

  // Note: no delete test — the TAMS spec does not define DELETE /sources/{sourceId}
});
