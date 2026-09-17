import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import ExplorerContent from './ExplorerContent';
import { JournalService, ObjectInvocationResult } from '../services/JournalService';
import { JournalResponse } from '../types';

const decodeRawHref = (href: string): unknown => {
  const token = new URL(href, 'http://explorer.test').searchParams.get('selection')!;
  const padded = token.replace(/-/g, '+').replace(/_/g, '/')
    + '='.repeat((4 - (token.length % 4)) % 4);
  return JSON.parse(decodeURIComponent(Array.from(atob(padded))
    .map((character) => `%${character.charCodeAt(0).toString(16).padStart(2, '0')}`).join('')));
};

jest.mock('../services/JournalService', () => ({
  JournalService: {
    extractSchemeValue: jest.fn((value) => ({ value, schemeType: null })),
    parseDirectoryResponse: jest.fn(),
    parseDirectoryEntries: jest.fn(),
    byteVectorToText: jest.fn(() => null),
    documentContentToText: jest.fn((value) => {
      if (value && typeof value === 'object' && '*type/byte-vector*' in value) {
        return value['*type/byte-vector*'];
      }
      return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    }),
    isReservedStateSegment: jest.fn((value: string) => value.startsWith('*') && value.endsWith('*')),
    isIndexError: jest.fn((error: unknown) =>
      typeof error === 'object' && error !== null
        && 'code' in error && (error as { code?: unknown }).code === 'index-error'),
    isSnapshotUnavailable: jest.fn((error: unknown) => {
      const value = error as { code?: unknown };
      return value?.code === 'index-error' || value?.code === 'bridge-index-error';
    }),
    encodePathSegment: jest.fn((value: string) => encodeURIComponent(value)),
    decodePathSegment: jest.fn((value: string) => decodeURIComponent(value)),
    pathToScheme: jest.fn((path: unknown[]) => `(${path.join(' ')})`),
  },
}));

describe('ExplorerContent', () => {
  const mockJournalService = {
    get: jest.fn(),
    set: jest.fn(),
    setText: jest.fn(),
    download: jest.fn(),
    pin: jest.fn(),
    unpin: jest.fn(),
    verifyPinnedInventories: jest.fn().mockResolvedValue(undefined),
    probeObjectApi: jest.fn(),
    invokeObject: jest.fn(),
    getFederationContext: jest.fn(() => ({ route: ['peer-a'] })),
    rawUrl: jest.fn((selection: string) => `/api/v1/raw?selection=${selection}`),
  } as unknown as JournalService;

  beforeEach(() => {
    jest.clearAllMocks();
    URL.createObjectURL = jest.fn(() => 'blob:test');
    URL.revokeObjectURL = jest.fn();
    (JournalService.extractSchemeValue as jest.Mock).mockImplementation((value) => ({ value, schemeType: null }));
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue(null);
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue(null);
    (JournalService.documentContentToText as jest.Mock).mockImplementation((value) => {
      if (value && typeof value === 'object' && '*type/byte-vector*' in value) {
        return value['*type/byte-vector*'];
      }
      return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    });
    (JournalService.encodePathSegment as jest.Mock)
      .mockImplementation((value: string) => encodeURIComponent(value));
    (JournalService.decodePathSegment as jest.Mock)
      .mockImplementation((value: string) => decodeURIComponent(value));
    (mockJournalService.verifyPinnedInventories as jest.Mock).mockResolvedValue(undefined);
    (mockJournalService.probeObjectApi as jest.Mock).mockRejectedValue(new Error('not an object'));
    (mockJournalService.invokeObject as jest.Mock).mockRejectedValue(new Error('not an object'));
    (mockJournalService.getFederationContext as jest.Mock).mockReturnValue({ route: ['peer-a'] });
    (mockJournalService.rawUrl as jest.Mock)
      .mockImplementation((selection: string) => `/api/v1/raw?selection=${selection}`);
    (JournalService.isIndexError as jest.Mock).mockImplementation((error: unknown) =>
      typeof error === 'object' && error !== null
        && 'code' in error && (error as { code?: unknown }).code === 'index-error');
    (JournalService.isSnapshotUnavailable as jest.Mock).mockImplementation((error: unknown) => {
      const value = error as { code?: unknown };
      return value?.code === 'index-error' || value?.code === 'bridge-index-error';
    });
  });

  it('labels the structural bridge row for people without changing its path', () => {
    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [4, '*bridge*'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    expect(screen.getByText('Bridges')).toBeInTheDocument();
    expect(screen.queryByText('*bridge*')).not.toBeInTheDocument();
    expect(mockJournalService.get).not.toHaveBeenCalled();
  });

  it('renders retained bridge and index structure without fetching raw content', () => {
    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [4, '*bridge*', 'archive', 7], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    expect(screen.getByText('No accessible contents')).toBeInTheDocument();
    expect(mockJournalService.get).not.toHaveBeenCalled();
    expect(screen.queryByRole('link', { name: 'Raw' })).not.toBeInTheDocument();
  });

  it('shows stage directory actions', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['directory', { docs: 'directory' }, true],
      'pinned?': false,
      proof: {},
    });
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue({
      items: ['docs'],
      isComplete: true,
    });
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue([
      { name: 'docs', type: 'directory' },
    ]);

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'docs'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={jest.fn()}
      />,
    );

    expect(await screen.findByText('+ Put')).toBeInTheDocument();
    expect(screen.getByText('+ Directory')).toBeInTheDocument();
    expect(screen.getByText('Upload Document')).toBeInTheDocument();
  });

  it('offers Stage Raw as a stable credential-free new-tab link', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '3c7363726970743e616c6572742831293c2f7363726970743e' },
      proof: {},
    });

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'alice', 'page.html'], type: 'file' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    const raw = await screen.findByRole('link', { name: 'Raw' });
    expect(raw).toHaveAttribute('target', '_blank');
    expect(raw).toHaveAttribute('rel', 'noopener noreferrer');
    expect(raw.getAttribute('href')).toMatch(/^\/api\/v1\/raw\?selection=[A-Za-z0-9_-]+$/);
    expect(screen.getByText('Edit')).toBeInTheDocument();
    expect(screen.getByText('Delete')).toBeInTheDocument();
    expect(mockJournalService.get).toHaveBeenCalledWith(
      ['*state*', 'alice', 'page.html'],
      { pinned: false, proof: false },
    );
  });

  it('offers Ledger Raw only with same-response proof-derived indexes', async () => {
    const selection = { path: [-1, 'peer-a', -2, '*state*', 'doc'], type: 'file' as const };
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '00ff' }, indexes: [12, 8],
    });
    const props = {
      mode: 'ledger' as const,
      selection,
      journalService: mockJournalService,
      refreshKey: 0,
      ledgerView: 'content' as const,
      onLedgerViewToggle: jest.fn(),
      onStageCreateFile: jest.fn(), onStageCreateDirectory: jest.fn(),
      onStageUploadFile: jest.fn(), onStageRename: jest.fn(),
      onStageDelete: jest.fn(), onSelectPath: jest.fn(),
    };
    const first = render(<ExplorerContent {...props} />);
    const raw = await screen.findByRole('link', { name: 'Raw' });
    expect(raw).toHaveAttribute('target', '_blank');
    expect(mockJournalService.get).toHaveBeenCalledTimes(1);
    expect(mockJournalService.get).toHaveBeenCalledWith(selection.path, {
      pinned: true, proof: false, selectedIndexes: true,
    });
    expect(decodeRawHref(raw.getAttribute('href')!)).toEqual([
      'v1', 'ledger',
      [['i', 12], ['y', 'peer-a'], ['i', 8], ['y', '*state*'], ['y', 'doc']],
    ]);
    first.unmount();

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '00ff' },
    });
    const second = render(<ExplorerContent {...props} />);
    await waitFor(() => expect(second.container.querySelector('.loading-spinner')).toBeNull());
    expect(screen.queryByRole('link', { name: 'Raw' })).not.toBeInTheDocument();
  });

  it('does not present a transient snapshot race as an access denial', async () => {
    (mockJournalService.get as jest.Mock).mockRejectedValue(
      Object.assign(new Error('Index is out of bounds: 257'), { code: 'index-error' }),
    );

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [257, 'journal-1', -2, '*state*', 'alice'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('selected ledger snapshot is unavailable');
    expect(alert).not.toHaveTextContent('granted access');
    expect(mockJournalService.get).toHaveBeenCalledTimes(1);
    expect(mockJournalService.get).toHaveBeenCalledWith(
      [257, 'journal-1', -2, '*state*', 'alice'],
      { pinned: true, proof: false },
    );
  });

  it('does not present an unretained historical route as an access denial', async () => {
    (mockJournalService.get as jest.Mock).mockRejectedValue(
      Object.assign(
        new Error('bridge-index-error: Bridge is not committed at the selected local index: journal-1 -1'),
        { code: 'bridge-index-error' },
      ),
    );

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [257, 'journal-1', -2, '*state*', 'alice'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('selected ledger snapshot is unavailable');
    expect(alert).not.toHaveTextContent('granted access');
  });

  it('hides mutation controls only when the selected Stage location is browsing-only', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['directory', { alice: 'directory' }, false],
      'pinned?': false,
      proof: {},
    });
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue({
      items: ['alice'],
      isComplete: false,
    });
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue([
      { name: 'alice', type: 'directory' },
    ]);

    render(
      <ExplorerContent
        mode="stage"
        stageReadOnly
        selection={{ path: ['*state*'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={jest.fn()}
      />,
    );

    expect(await screen.findByText('alice')).toBeInTheDocument();
    expect(screen.queryByText('+ Put')).not.toBeInTheDocument();
    expect(screen.queryByText('+ Directory')).not.toBeInTheDocument();
    expect(screen.queryByText('Upload Document')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Rename')).not.toBeInTheDocument();
    expect(screen.queryByText('Delete')).not.toBeInTheDocument();
  });

  it('shows edit, rename, and delete for a selected remote Stage document', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '68656c6c6f' },
      'pinned?': false,
      proof: {},
    });
    (JournalService.extractSchemeValue as jest.Mock).mockReturnValue({
      value: 'hello',
      schemeType: 'string',
    });

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'admin', 'data', 'public', 'draft.txt'], type: 'file' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={jest.fn()}
      />,
    );

    expect(await screen.findByText('Edit')).toBeInTheDocument();
    expect(screen.getByTitle('Rename')).toBeInTheDocument();
    expect(screen.getByText('Delete')).toBeInTheDocument();
    expect(screen.getByText('Download')).toBeInTheDocument();
  });

  it('renders successful primary content before optional object and pin enrichment', async () => {
    let resolvePrimary: (value: JournalResponse) => void = () => undefined;
    let resolveProbe: () => void = () => undefined;
    let resolveApi: (value: ObjectInvocationResult) => void = () => undefined;
    let resolvePin: (value: JournalResponse) => void = () => undefined;
    const primary = new Promise<JournalResponse>((resolve) => { resolvePrimary = resolve; });
    const probe = new Promise<void>((resolve) => { resolveProbe = resolve; });
    const api = new Promise<ObjectInvocationResult>((resolve) => { resolveApi = resolve; });
    const pin = new Promise<JournalResponse>((resolve) => { resolvePin = resolve; });
    const pinService = { ...mockJournalService, get: jest.fn().mockReturnValue(pin) } as unknown as JournalService;
    (mockJournalService.get as jest.Mock).mockReturnValue(primary);
    (mockJournalService.probeObjectApi as jest.Mock).mockReturnValue(probe);
    (mockJournalService.invokeObject as jest.Mock).mockReturnValue(api);
    (JournalService.documentContentToText as jest.Mock).mockImplementation((value) => String(value));

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [4, '*state*', 'alice', 'counter'], type: 'file' }}
        journalService={mockJournalService}
        pinJournalService={pinService}
        pinPath={[4, '*state*', 'alice', 'counter']}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    await act(async () => resolvePrimary({ content: 'primary content' }));
    expect(await screen.findByText('primary content')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pin status unavailable' })).toBeDisabled();
    expect(screen.queryByRole('heading', { name: 'Object' })).not.toBeInTheDocument();

    await act(async () => resolveProbe());
    await waitFor(() => expect(mockJournalService.invokeObject).toHaveBeenCalledTimes(1));
    expect(screen.getByText('primary content')).toBeInTheDocument();
    await act(async () => resolveApi({
      operation: 'retrieve', context: { route: [] },
      path: [4, '*state*', 'alice', 'counter'], readOnly: true, result: '(*api*)',
    }));
    expect(await screen.findByRole('heading', { name: 'Object' })).toBeInTheDocument();
    await act(async () => resolvePin({ content: 'primary content', 'pinned?': true }));
    expect(await screen.findByRole('button', { name: 'Unpin' })).toBeInTheDocument();
  });

  it('discards stale primary, object, and pin results from an older selection generation', async () => {
    let resolveOldPrimary: (value: JournalResponse) => void = () => undefined;
    let resolveOldProbe: () => void = () => undefined;
    let resolveOldPin: (value: JournalResponse) => void = () => undefined;
    const oldPrimary = new Promise<JournalResponse>((resolve) => { resolveOldPrimary = resolve; });
    const oldProbe = new Promise<void>((resolve) => { resolveOldProbe = resolve; });
    const oldPin = new Promise<JournalResponse>((resolve) => { resolveOldPin = resolve; });
    const pinService = {
      ...mockJournalService,
      get: jest.fn()
        .mockReturnValueOnce(oldPin)
        .mockResolvedValueOnce({ content: 'new content', 'pinned?': false }),
    } as unknown as JournalService;
    (mockJournalService.get as jest.Mock)
      .mockReturnValueOnce(oldPrimary)
      .mockResolvedValueOnce({ content: 'new content' });
    (mockJournalService.probeObjectApi as jest.Mock)
      .mockReturnValueOnce(oldProbe)
      .mockRejectedValueOnce(new Error('not an object'));
    (mockJournalService.invokeObject as jest.Mock).mockResolvedValue({
      operation: 'retrieve', context: { route: [] }, path: [], readOnly: true, result: 'old api',
    });
    (JournalService.documentContentToText as jest.Mock).mockImplementation((value) => String(value));

    const props = {
      mode: 'ledger' as const,
      journalService: mockJournalService,
      pinJournalService: pinService,
      refreshKey: 0,
      ledgerView: 'content' as const,
      onLedgerViewToggle: jest.fn(),
      onStageCreateFile: jest.fn(),
      onStageCreateDirectory: jest.fn(),
      onStageUploadFile: jest.fn(),
      onStageRename: jest.fn(),
      onStageDelete: jest.fn(),
      onSelectPath: jest.fn(),
    };
    const { rerender } = render(
      <ExplorerContent
        {...props}
        selection={{ path: [4, '*state*', 'alice', 'old'], type: 'file' }}
        pinPath={[4, '*state*', 'alice', 'old']}
      />,
    );
    rerender(
      <ExplorerContent
        {...props}
        selection={{ path: [4, '*state*', 'alice', 'new'], type: 'file' }}
        pinPath={[4, '*state*', 'alice', 'new']}
      />,
    );
    expect(await screen.findByText('new content')).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Pin' })).toBeInTheDocument();

    await act(async () => {
      resolveOldPrimary({ content: 'old content' });
      resolveOldProbe();
      resolveOldPin({ content: 'old content', 'pinned?': true });
    });
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByText('new content')).toBeInTheDocument();
    expect(screen.queryByText('old content')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Pin' })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Object' })).not.toBeInTheDocument();
  });

  it('renders raw metadata only after *api* establishes the Journal object boundary', async () => {
    const rawMetadata = '((class |opaque class|) (object-hash #u(1)) (code-hash #u(2)))';
    const resultBase = {
      operation: 'use!', context: { route: [] },
      path: ['*state*', 'alice', 'counter'], readOnly: true,
    };
    (mockJournalService.get as jest.Mock).mockRejectedValue(new Error('Expected byte-vector'));
    (mockJournalService.probeObjectApi as jest.Mock).mockResolvedValue(undefined);
    (mockJournalService.invokeObject as jest.Mock)
      .mockResolvedValueOnce({ ...resultBase, result: '(*name* *api* *class* value)' })
      .mockResolvedValueOnce({ ...resultBase, result: rawMetadata });

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'alice', 'counter'], type: 'file' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    expect(await screen.findByRole('heading', { name: 'Object' })).toBeInTheDocument();
    expect(screen.getAllByText(rawMetadata)).toHaveLength(1);
    expect(screen.queryByText('Edit')).not.toBeInTheDocument();
    expect(screen.queryByText(/Unable to load this location/)).not.toBeInTheDocument();
  });

  it('does not exercise or render object controls for inert file values', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({ content: 'plain value' });

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'alice', 'note'], type: 'file' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    await waitFor(() => expect(mockJournalService.get).toHaveBeenCalled());
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByText('plain value')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Object' })).not.toBeInTheDocument();
    expect(mockJournalService.probeObjectApi).toHaveBeenCalledTimes(1);
    expect(mockJournalService.invokeObject).not.toHaveBeenCalled();
  });

  it('does not classify printed metadata-like inert content as an object', async () => {
    const duplicate = '((class counter) (class forged) (object-hash #u(1)) (code-hash #u(2)))';
    (mockJournalService.get as jest.Mock).mockResolvedValue({ content: duplicate });

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'alice', 'forged'], type: 'file' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    await waitFor(() => expect(mockJournalService.get).toHaveBeenCalled());
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByText(duplicate)).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'Object' })).not.toBeInTheDocument();
  });

  it('shows a stable access error without exposing the raw journal response', async () => {
    (mockJournalService.get as jest.Mock).mockRejectedValue(
      new Error('journal_error: raw authorization details'),
    );

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'alice'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={jest.fn()}
      />,
    );

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Check that the selected journal has granted access to this user and path.',
    );
    expect(screen.queryByText(/raw authorization details/)).not.toBeInTheDocument();
    expect(screen.queryByText('+ Put')).not.toBeInTheDocument();
  });

  it('navigates when a directory entry is clicked', async () => {
    const onSelectPath = jest.fn();

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['directory', { docs: 'directory', 'readme.md': 'value' }, true],
      'pinned?': false,
      proof: {},
    });
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue({
      items: ['docs', 'readme.md'],
      isComplete: true,
    });
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue([
      { name: 'docs', type: 'directory' },
      { name: 'readme.md', type: 'value' },
    ]);

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [42, '*state*'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={onSelectPath}
      />,
    );

    const docsButton = await screen.findByRole('button', { name: /docs/i });
    fireEvent.click(docsButton);

    expect(onSelectPath).toHaveBeenCalledWith({
      path: [42, '*state*', 'docs'],
      type: 'directory',
    });
  });

  it('renders a clickable responsive path breadcrumb with a non-clickable current segment', async () => {
    const onSelectPath = jest.fn();
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': 'hello' },
    });

    render(
      <ExplorerContent
        mode="ledger"
        selection={{
          path: [42, 'journal-1', -3, '*state*', 'alice', 'docs', 'draft%20one.txt'],
          type: "file"
        }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={onSelectPath}
      />,
    );

    const breadcrumb = screen.getByRole('navigation', { name: 'Content path' });
    expect(breadcrumb).toHaveTextContent('State/alice/docs/draft one.txt');
    expect(screen.getByRole('button', { name: 'State' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'alice' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'docs' }));
    expect(onSelectPath).toHaveBeenCalledWith({
      path: [42, 'journal-1', -3, '*state*', 'alice', 'docs'],
      type: 'directory',
    });
    expect(breadcrumb.querySelector('[aria-current="page"]')).toHaveTextContent('draft one.txt');
    expect(screen.queryByRole('button', { name: 'draft one.txt' })).not.toBeInTheDocument();
  });

  it('uses encoded entry path segments so content and tree selections agree', async () => {
    const onSelectPath = jest.fn();
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['directory', { 'design notes': 'value' }, true],
    });
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue({
      items: ['design notes'],
      isComplete: true,
    });
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue([
      { name: 'design notes', pathSegment: 'design%20notes', type: 'value' },
    ]);

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'alice'], type: "directory" }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={onSelectPath}
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: /design notes/i }));
    expect(onSelectPath).toHaveBeenCalledWith({
      path: ['*state*', 'alice', 'design%20notes'],
      type: 'file',
    });
  });

  it('selects colliding typed directory entries with their raw segments', async () => {
    const onSelectPath = jest.fn();
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['directory', [], true],
    });
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue({
      items: ['123', '123', '123'],
      isComplete: true,
    });
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue([
      { name: '123 [integer]', pathSegment: 123, keyType: 'integer', type: 'value' },
      { name: '123', pathSegment: '123', keyType: 'symbol', type: 'value' },
      {
        name: '"123"', pathSegment: { '*type/string*': '123' },
        keyType: 'string', type: 'value',
      },
    ]);

    render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'typed'], type: 'directory' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={onSelectPath}
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: /123 \[integer\]/i }));
    fireEvent.click(screen.getByRole('button', { name: /^123$/i }));
    fireEvent.click(screen.getByRole('button', { name: /"123"/i }));
    expect(onSelectPath.mock.calls.map(([selection]) => selection.path)).toEqual([
      ['*state*', 'typed', 123],
      ['*state*', 'typed', '123'],
      ['*state*', 'typed', { '*type/string*': '123' }],
    ]);
  });

  it('preserves in-progress stage edits during same-selection refreshes', async () => {
    (mockJournalService.get as jest.Mock)
      .mockResolvedValueOnce({
        content: { '*type/byte-vector*': '6f726967696e616c' },
        'pinned?': false,
        proof: {},
      })
      .mockResolvedValueOnce({
        content: { '*type/byte-vector*': '726566726573686564' },
        'pinned?': false,
        proof: {},
      });

    const baseProps = {
      mode: 'stage' as const,
      selection: { path: ['*state*', 'draft.txt'], type: 'file' as const },
      journalService: mockJournalService,
      ledgerView: 'content' as const,
      onLedgerViewToggle: jest.fn(),
      onStageCreateFile: jest.fn().mockResolvedValue(undefined),
      onStageCreateDirectory: jest.fn().mockResolvedValue(undefined),
      onStageUploadFile: jest.fn().mockResolvedValue(undefined),
      onStageRename: jest.fn().mockResolvedValue(undefined),
      onStageDelete: jest.fn().mockResolvedValue(undefined),
      onSelectPath: jest.fn(),
    };

    const { container, rerender } = render(<ExplorerContent {...baseProps} refreshKey={0} />);

    await waitFor(() => expect(container.querySelector('.loading-spinner')).toBeNull());
    fireEvent.click(screen.getByText('Edit'));
    const textarea = await screen.findByLabelText('Document content');
    fireEvent.change(textarea, { target: { value: 'manual unsaved edit' } });

    rerender(<ExplorerContent {...baseProps} refreshKey={1} />);

    await waitFor(() => expect((mockJournalService.get as jest.Mock).mock.calls.length).toBeGreaterThanOrEqual(2));
    expect(screen.getByLabelText('Document content')).toHaveValue('manual unsaved edit');
  });

  it('preserves in-progress stage edits when a same-selection refresh fails', async () => {
    (mockJournalService.get as jest.Mock)
      .mockResolvedValueOnce({
        content: { '*type/byte-vector*': '6f726967696e616c' },
        'pinned?': false,
        proof: {},
      })
      .mockRejectedValueOnce(new Error('temporary refresh failure'));

    const baseProps = {
      mode: 'stage' as const,
      selection: { path: ['*state*', 'draft.txt'], type: 'file' as const },
      journalService: mockJournalService,
      ledgerView: 'content' as const,
      onLedgerViewToggle: jest.fn(),
      onStageCreateFile: jest.fn(),
      onStageCreateDirectory: jest.fn(),
      onStageUploadFile: jest.fn(),
      onStageRename: jest.fn(),
      onStageDelete: jest.fn(),
      onSelectPath: jest.fn(),
    };

    const { container, rerender } = render(<ExplorerContent {...baseProps} refreshKey={0} />);

    await waitFor(() => expect(container.querySelector('.loading-spinner')).toBeNull());
    fireEvent.click(screen.getByText('Edit'));
    fireEvent.change(screen.getByLabelText('Document content'), { target: { value: 'manual unsaved edit' } });
    rerender(<ExplorerContent {...baseProps} refreshKey={1} />);

    await waitFor(() => expect((mockJournalService.get as jest.Mock)).toHaveBeenCalledTimes(2));
    expect(screen.getByLabelText('Document content')).toHaveValue('manual unsaved edit');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('keeps denied document edits in place and shows a localized save error', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: 'original',
      'pinned?': false,
      proof: {},
    });
    (mockJournalService.setText as jest.Mock).mockRejectedValue(new Error('not authorized'));

    const { container } = render(
      <ExplorerContent
        mode="stage"
        selection={{ path: ['*state*', 'alice', 'draft.txt'], type: 'file' }}
        journalService={mockJournalService}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    await waitFor(() => expect(container.querySelector('.loading-spinner')).toBeNull());
    fireEvent.click(screen.getByText('Edit'));
    fireEvent.change(screen.getByLabelText('Document content'), { target: { value: 'unsaved denied edit' } });
    fireEvent.click(screen.getByText('Save'));

    expect(await screen.findByRole('alert')).toHaveTextContent('Save failed: not authorized');
    expect(screen.getByLabelText('Document content')).toHaveValue('unsaved denied edit');
    expect(screen.getByText('Save')).toBeInTheDocument();
  });

  it('shows ledger proof toggle and unpin state from boolean pinned response', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '7065657220636f6e74656e74' },
      'pinned?': true,
      proof: { hash: 'abc' },
    });
    (JournalService.extractSchemeValue as jest.Mock).mockReturnValue({
      value: 'peer content',
      schemeType: 'string',
    });

    const { rerender } = render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [42, '*state*', 'peer.txt'], type: 'file' }}
        journalService={mockJournalService}
        pinJournalService={mockJournalService}
        pinPath={[42, '*state*', 'peer.txt']}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={jest.fn()}
      />,
    );

    expect(await screen.findAllByRole('button', { name: 'Proof' })).toHaveLength(1);
    expect(screen.getByText('Download')).toBeInTheDocument();
    expect(await screen.findByText('Unpin')).toBeInTheDocument();
    expect(mockJournalService.get).toHaveBeenCalledWith(
      [42, '*state*', 'peer.txt'],
      { pinned: true, proof: false, selectedIndexes: true },
    );

    rerender(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [42, '*state*', 'peer.txt'], type: 'file' }}
        journalService={mockJournalService}
        pinJournalService={mockJournalService}
        pinPath={[42, '*state*', 'peer.txt']}
        refreshKey={0}
        ledgerView="proof"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={jest.fn()}
      />,
    );
    await waitFor(() =>
      expect(mockJournalService.get).toHaveBeenCalledWith(
        [42, '*state*', 'peer.txt'],
        { pinned: true, proof: true, selectedIndexes: true },
      ),
    );
    expect(screen.getAllByRole('button', { name: 'Content' })).toHaveLength(1);
    expect(screen.queryByRole('button', { name: 'Proof' })).not.toBeInTheDocument();
  });

  it('requires exact local and terminal-provider evidence before showing a new pin state', async () => {
    const content = { '*type/byte-vector*': '70696e6e6564' };
    const provider = {
      ...mockJournalService,
      get: jest.fn()
        .mockResolvedValueOnce({ content, 'pinned?': false, indexes: [4, 8] })
        .mockResolvedValueOnce({ content, 'pinned?': false, indexes: [4, 8] }),
    } as unknown as JournalService;
    const origin = {
      ...mockJournalService,
      get: jest.fn()
        .mockResolvedValueOnce({ content, 'pinned?': false })
        .mockResolvedValueOnce({ content, 'pinned?': true }),
      pin: jest.fn().mockResolvedValue(true),
    } as unknown as JournalService;
    const providerPath = [8, '*state*', 'alice', 'counter'];
    const originPath = [4, 'peer-a', 8, '*state*', 'alice', 'counter'];

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: providerPath, type: 'file' }}
        journalService={provider}
        pinJournalService={origin}
        pinPath={originPath}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Pin' }));
    expect(await screen.findByRole('button', { name: 'Unpin' })).toBeInTheDocument();
    expect(origin.pin).toHaveBeenCalledWith(originPath);
    expect(origin.get).toHaveBeenLastCalledWith(originPath, { pinned: true, proof: false });
    expect(origin.verifyPinnedInventories).toHaveBeenCalledWith(originPath);
    expect(provider.get).toHaveBeenLastCalledWith(providerPath, {
      pinned: true, proof: false, selectedIndexes: true,
    });
    expect(screen.getByText(/local and terminal-provider readback/)).toBeInTheDocument();
  });

  it('does not retain an optimistic pin state after indeterminate readback', async () => {
    const content = { '*type/byte-vector*': '70696e6e6564' };
    const provider = {
      ...mockJournalService,
      get: jest.fn().mockResolvedValue({ content, 'pinned?': false, indexes: [4, 8] }),
    } as unknown as JournalService;
    const origin = {
      ...mockJournalService,
      get: jest.fn()
        .mockResolvedValueOnce({ content, 'pinned?': false })
        .mockRejectedValueOnce(new Error('readback unavailable')),
      pin: jest.fn().mockResolvedValue(true),
    } as unknown as JournalService;

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [8, '*state*', 'alice', 'counter'], type: 'file' }}
        journalService={provider}
        pinJournalService={origin}
        pinPath={[4, 'peer-a', 8, '*state*', 'alice', 'counter']}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn()}
        onStageCreateDirectory={jest.fn()}
        onStageUploadFile={jest.fn()}
        onStageRename={jest.fn()}
        onStageDelete={jest.fn()}
        onSelectPath={jest.fn()}
      />,
    );

    fireEvent.click(await screen.findByRole('button', { name: 'Pin' }));
    expect(await screen.findByRole('button', { name: 'Pin status unavailable' })).toBeDisabled();
    expect(screen.queryByRole('button', { name: 'Unpin' })).not.toBeInTheDocument();
    expect(screen.getByText(/Pin state is indeterminate: readback unavailable/)).toBeInTheDocument();
    expect(origin.pin).toHaveBeenCalledTimes(1);
  });

  it('preserves absolute routed indexes while displaying pin state', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '7065657220636f6e74656e74' },
      'pinned?': true,
      proof: { hash: 'abc' },
    });

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [4, 'journal-5', 8, '*state*', 'admin', 'data', 'key-0'], type: 'file' }}
        journalService={mockJournalService}
        pinJournalService={mockJournalService}
        pinPath={[4, 'journal-5', 8, '*state*', 'admin', 'data', 'key-0']}
        refreshKey={0}
        ledgerView="content"
        onLedgerViewToggle={jest.fn()}
        onStageCreateFile={jest.fn().mockResolvedValue(undefined)}
        onStageCreateDirectory={jest.fn().mockResolvedValue(undefined)}
        onStageUploadFile={jest.fn().mockResolvedValue(undefined)}
        onStageRename={jest.fn().mockResolvedValue(undefined)}
        onStageDelete={jest.fn().mockResolvedValue(undefined)}
        onSelectPath={jest.fn()}
      />,
    );

    expect(await screen.findByText('Unpin')).toBeInTheDocument();
  });
});
