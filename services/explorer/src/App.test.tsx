import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import App, { isWritableRemoteStageSelection } from './App';
import { ExplorerSelection } from './types';
import { JournalService } from './services/JournalService';

const mockFetchResponse = (ok: boolean, status: number) =>
  Promise.resolve({ ok, status } as Response);

describe('remote Stage write scope', () => {
  const selection = (path: ExplorerSelection['path']): ExplorerSelection => ({
    path,
    type: 'directory',
  });

  it('keeps remote ancestors and public data read-only', () => {
    const route = ['journal-3'];
    expect(isWritableRemoteStageSelection(
      selection(['*state*', 'admin']), route, 'journal-1',
    )).toBe(false);
    expect(isWritableRemoteStageSelection(
      selection(['*state*', 'admin', 'data']), route, 'journal-1',
    )).toBe(false);
    expect(isWritableRemoteStageSelection(
      selection(['*state*', 'admin', 'data', 'public']), route, 'journal-1',
    )).toBe(false);
  });

  it('allows writes only beneath the bucket assigned to the routed origin', () => {
    expect(isWritableRemoteStageSelection(
      selection(['*state*', 'admin', 'data', 'journal-1']),
      ['journal-3'],
      'journal-1',
    )).toBe(true);
    expect(isWritableRemoteStageSelection(
      selection(['*state*', 'admin', 'data', 'journal-0:journal-1:journal-3', 'key-0']),
      ['journal-1', 'journal-3', 'journal-1'],
      'journal-0',
    )).toBe(true);
    expect(isWritableRemoteStageSelection(
      selection(['*state*', 'admin', 'data', 'journal-3']),
      ['journal-3'],
      'journal-1',
    )).toBe(false);
  });
});

describe('App session check', () => {
  let originalLocation: Location;

  beforeEach(() => {
    originalLocation = window.location;
    // jsdom does not allow direct assignment to window.location, so we delete and redefine.
    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: { ...originalLocation, href: 'http://localhost/explorer', hash: '' },
    });
  });

  afterEach(() => {
    // @ts-ignore test-only runtime configuration
    delete window._env_;
    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: originalLocation,
    });
    jest.clearAllMocks();
  });

  it('renders the app when whoami returns 200', async () => {
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ identity: { traits: { username: 'alice' } } }),
    } as unknown as Response);

    render(<App />);

    await waitFor(() => {
      expect(screen.queryByLabelText('Checking session…')).not.toBeInTheDocument();
    });
    expect(screen.getByText('Ledger')).toBeInTheDocument();
    expect(screen.getByText('alice')).toBeInTheDocument();
  });

  it('does not publish a synthetic Ledger index 0 before size discovery', async () => {
    const replaceState = jest.spyOn(window.history, 'replaceState');
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ identity: { traits: { username: 'alice' } } }),
    } as unknown as Response);

    render(<App />);

    await waitFor(() => {
      expect(screen.queryByLabelText('Checking session…')).not.toBeInTheDocument();
    });
    expect(replaceState.mock.calls.some((call) => String(call[2]).includes('#ledger/0/'))).toBe(false);
  });

  it('loads the routed namespace root when refreshing a Ledger deep link', async () => {
    // @ts-ignore test-only runtime configuration
    window._env_ = { SYNC_EXPLORER_ENDPOINT: '/api/v1' };
    window.location.hash = '#ledger/99/bridge/journal-1/state/bob/data/public/';
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ identity: { traits: { username: 'alice' } } }),
    } as unknown as Response);
    jest.spyOn(JournalService.prototype, 'getLocalJournalName').mockResolvedValue('journal-0');
    jest.spyOn(JournalService.prototype, 'ensureDirectory').mockResolvedValue(true);
    jest.spyOn(JournalService.prototype, 'getAdmins').mockRejectedValue(new Error('not admin'));
    jest.spyOn(JournalService.prototype, 'getSize').mockResolvedValue(100);
    jest.spyOn(JournalService.prototype, 'subscribeEvents').mockReturnValue(jest.fn());
    const directoryEntries = jest.spyOn(JournalService.prototype, 'getDirectoryEntries')
      .mockResolvedValue([
        { name: 'alice', type: 'directory' },
        { name: 'bob', type: 'directory' },
      ]);
    jest.spyOn(JournalService.prototype, 'get').mockResolvedValue({
      content: { '*type/byte-vector*': '' },
    });

    render(<App />);

    // The root load must be driven by the parsed route, not by a later user action.
    await waitFor(() => {
      expect(directoryEntries).toHaveBeenCalledWith([
        99, 'journal-1', -1, '*state*',
      ]);
      expect(directoryEntries.mock.calls.length).toBeGreaterThanOrEqual(2);
    }, { timeout: 1000 });
    await waitFor(() => {
      expect(screen.getAllByText('bob').some((node) => node.closest('.tree-node'))).toBe(true);
    });
    expect(screen.queryByText('No documents available for this ledger route.'))
      .not.toBeInTheDocument();
  });

  it('shows an in-place sign-in prompt when whoami returns 401', async () => {
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({ ok: false, status: 401 } as Response);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('Sign in to use Explorer')).toBeInTheDocument();
    });
    expect(screen.getByRole('link', { name: 'Sign in' })).toHaveAttribute(
      'href',
      '/auth/login?return_to=' + encodeURIComponent('http://localhost/explorer'),
    );
    expect(window.location.href).toBe('http://localhost/explorer');
  });

  it('shows an in-place retry prompt when whoami fetch rejects', async () => {
    jest.spyOn(global, 'fetch').mockRejectedValueOnce(new Error('Network error'));

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('Sign in to use Explorer')).toBeInTheDocument();
    });
    expect(screen.getByText('Could not check session')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(window.location.href).toBe('http://localhost/explorer');
  });
});
