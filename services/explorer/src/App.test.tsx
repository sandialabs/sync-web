import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import App, {
  accessibleRemoteSelection,
  resolveLedgerHopIndexes,
  terminalRootSelections,
  treeAncestorNodeIds,
} from './App';
import { ExplorerSelection, JournalPath, JournalResponse } from './types';
import { JournalService } from './services/JournalService';
import { retainedLedgerRootPath } from './utils/ledgerRoute';

const mockFetchResponse = (ok: boolean, status: number) =>
  Promise.resolve({ ok, status } as Response);

const mockReadyExplorer = (sizes: number[] = [10], defaultLedger = true) => {
  jest.spyOn(window.history, 'replaceState').mockImplementation((_data, _unused, url) => {
    const value = String(url ?? '');
    window.location.hash = value.includes('#') ? `#${value.split('#', 2)[1]}` : '';
  });
  // @ts-ignore test-only runtime configuration
  window._env_ = { SYNC_EXPLORER_ENDPOINT: '/api/v1' };
  jest.spyOn(global, 'fetch').mockResolvedValue({
    ok: true,
    status: 200,
    json: () => Promise.resolve({ identity: { traits: { username: 'alice' } } }),
  } as unknown as Response);
  jest.spyOn(JournalService.prototype, 'ensureDirectory').mockResolvedValue(true);
  jest.spyOn(JournalService.prototype, 'getAdmins').mockRejectedValue(new Error('not admin'));
  const getSize = jest.spyOn(JournalService.prototype, 'getSize');
  sizes.forEach((size) => getSize.mockResolvedValueOnce(size));
  getSize.mockResolvedValue(sizes[sizes.length - 1]);
  const getLocalSize = jest.spyOn(JournalService.prototype, 'getLocalSize');
  sizes.forEach((size) => getLocalSize.mockResolvedValueOnce(size));
  getLocalSize.mockResolvedValue(sizes[sizes.length - 1]);
  const getBridges = jest.spyOn(JournalService.prototype, 'getBridges').mockResolvedValue([
    { name: 'journal-1', endpoint: 'http://journal-1' },
  ]);
  jest.spyOn(JournalService.prototype, 'subscribeEvents').mockReturnValue(jest.fn());
  jest.spyOn(JournalService.prototype, 'probeObjectApi').mockRejectedValue(new Error('not an object'));
  jest.spyOn(JournalService.prototype, 'putResource').mockResolvedValue(true);
  const getDirectoryEntries = jest.spyOn(JournalService.prototype, 'getDirectoryEntries').mockResolvedValue([
    { name: 'bob', type: 'directory' },
    { name: 'alice', type: 'directory' },
  ]);
  const get = jest.spyOn(JournalService.prototype, 'get').mockResolvedValue({
    content: ['directory', { alice: 'directory', bob: 'directory' }, true],
  });
  return { get, getBridges, getDirectoryEntries };
};

const clickTreeNode = (label: string) => {
  const node = screen.getAllByText(label).find((candidate) =>
    candidate.closest('.tree-node:not(.tree-root-node)'),
  );
  if (!node) throw new Error(`Tree node not found: ${label}`);
  fireEvent.click(node);
};

const clickTreeRoot = () => {
  const root = document.querySelector('.tree-root-node .tree-node-label');
  if (!root) throw new Error('Tree root not found');
  fireEvent.click(root);
};

const clickContentItem = (label: string) => {
  const item = screen.getAllByText(label).find((candidate) =>
    candidate.closest('.directory-item'),
  );
  if (!item) throw new Error(`Content item not found: ${label}`);
  fireEvent.click(item.closest('button') ?? item);
};

describe('terminal route selection', () => {
  it('resolves only newly-latest hops unless Refresh explicitly renews the route', async () => {
    const localSize = jest.spyOn(JournalService.prototype, 'getLocalSize').mockResolvedValue(31);
    const setFederationContext = jest.spyOn(JournalService.prototype, 'setFederationContext');
    const routedSize = jest.spyOn(JournalService.prototype, 'getSize')
      .mockResolvedValueOnce(41)
      .mockResolvedValueOnce(51)
      .mockResolvedValueOnce(61)
      .mockResolvedValueOnce(71);
    const hops = [
      { key: 'local', kind: 'local' as const, name: 'Self', snapshot: '20', maximum: 30 },
      { key: 'one', kind: 'bridge' as const, name: 'one', snapshot: '30', maximum: 40 },
      { key: 'two', kind: 'bridge' as const, name: 'two', snapshot: 'latest' },
    ];

    await expect(resolveLedgerHopIndexes('/api/v1', hops, false)).resolves.toEqual([
      hops[0], hops[1], { ...hops[2], snapshot: '40', maximum: 40 },
    ]);
    expect(localSize).not.toHaveBeenCalled();
    expect(routedSize).toHaveBeenCalledTimes(1);
    expect(setFederationContext).toHaveBeenCalledWith({
      route: ['one', 'two'], historyIndexes: [20, 30, -1],
    });

    await expect(resolveLedgerHopIndexes('/api/v1', hops, true)).resolves.toEqual([
      { ...hops[0], maximum: 30 },
      { ...hops[1], maximum: 50 },
      { ...hops[2], snapshot: '60', maximum: 60 },
    ]);
  });
  it('uses the hop resolver as the sole local latest lookup', async () => {
    const localSize = jest.spyOn(JournalService.prototype, 'getLocalSize').mockResolvedValue(31);
    await expect(resolveLedgerHopIndexes('/api/v1', [
      { key: 'local', kind: 'local' as const, name: 'Self', snapshot: 'latest' },
    ], false)).resolves.toEqual([
      { key: 'local', kind: 'local', name: 'Self', snapshot: '30', maximum: 30 },
    ]);
    expect(localSize).toHaveBeenCalledTimes(1);
  });

  it('separates requester and terminal-provider retained roots', () => {
    expect(retainedLedgerRootPath([
      { key: 'local', kind: 'local', name: 'Self', snapshot: '7' },
      { key: 'journal-0', kind: 'bridge', name: 'journal-0', snapshot: '4' },
    ], 7)).toEqual([4]);
    expect(retainedLedgerRootPath([
      { key: 'local', kind: 'local', name: 'Self', snapshot: 'latest' },
    ], 7)).toEqual([7]);
    expect(retainedLedgerRootPath([
      { key: 'local', kind: 'local', name: 'Self', snapshot: '7' },
      { key: 'journal-0', kind: 'bridge', name: 'journal-0', snapshot: 'latest' },
    ], 7)).toEqual([-1]);
  });

  it('keeps requester Ledger admission separate from the proposed provider route', async () => {
    const providerGet = jest.fn().mockResolvedValue({ content: ['directory', [], true] });
    const requesterGet = jest.fn().mockResolvedValue({ content: ['directory', [], true] });
    const setFederationContext = jest.fn();
    const providerService = { get: providerGet, setFederationContext };
    const requesterService = { get: requesterGet };
    const hops = [
      { key: 'local', kind: 'local' as const, name: 'Self', snapshot: '20' },
      { key: 'peer', kind: 'bridge' as const, name: 'journal-1', snapshot: 'latest' },
    ];

    await expect(terminalRootSelections(
      providerService, requesterService, ['journal-1'], hops, 20,
    )).resolves.toEqual({
      stage: { path: ['*state*'], type: 'directory' },
      ledger: { path: [20, '*state*'], type: 'directory' },
    });
    expect(providerGet).toHaveBeenCalledWith(
      ['*state*'], { pinned: false, proof: false },
    );
    expect(requesterGet).toHaveBeenCalledWith(
      [20, '*state*'], { pinned: false, proof: false },
    );
    expect(setFederationContext).toHaveBeenCalledTimes(1);
    expect(setFederationContext).toHaveBeenCalledWith({
      route: ['journal-1'], historyIndexes: [20, -1],
    });
  });

  it('overlaps root admissions while preserving provider-first errors', async () => {
    let rejectProvider: (error: Error) => void = () => undefined;
    let rejectRequester: (error: Error) => void = () => undefined;
    const providerGet = jest.fn().mockReturnValue(new Promise((_resolve, reject) => {
      rejectProvider = reject;
    }));
    const requesterGet = jest.fn().mockReturnValue(new Promise((_resolve, reject) => {
      rejectRequester = reject;
    }));
    const setFederationContext = jest.fn();
    const hops = [{ key: 'local', kind: 'local' as const, name: 'Self', snapshot: '9' }];
    const admission = terminalRootSelections(
      { get: providerGet, setFederationContext }, { get: requesterGet }, [], hops, 9,
    );

    expect(providerGet).toHaveBeenCalledTimes(1);
    expect(requesterGet).toHaveBeenCalledTimes(1);
    rejectRequester(new Error('Requester denied'));
    rejectProvider(new Error('Provider denied'));
    await expect(admission).rejects.toThrow('Provider denied');
    expect(setFederationContext).toHaveBeenCalledWith({ route: [] });
  });


  const preserved: ExplorerSelection = {
    path: [20, 'journal-1', -1, '*state*', 'admin'],
    type: 'directory',
  };
  const fallbackPath = [20, 'journal-1', -1, '*state*'] as ExplorerSelection['path'];

  it('preserves a selection that remains accessible after synchronization or snapshot change', async () => {
    const get = jest.fn().mockResolvedValue({ content: ['directory', [], true] });
    await expect(accessibleRemoteSelection({ get }, preserved, fallbackPath))
      .resolves.toEqual(preserved);
    expect(get).toHaveBeenCalledTimes(1);
  });

  it('falls back from an inaccessible preserved path to the current terminal state root', async () => {
    const get = jest.fn()
      .mockRejectedValueOnce(new Error('Not authorized'))
      .mockResolvedValueOnce({ content: ['directory', [], true] });
    await expect(accessibleRemoteSelection({ get }, preserved, fallbackPath))
      .resolves.toEqual({ path: fallbackPath, type: 'directory' });
    expect(get).toHaveBeenNthCalledWith(1, preserved.path, { pinned: false, proof: false });
    expect(get).toHaveBeenNthCalledWith(2, fallbackPath, { pinned: false, proof: false });
  });

  it('rebases an inaccessible multi-hop selection without granting remote admin access', async () => {
    const multiHop: ExplorerSelection = {
      path: [20, 'journal-1', -1, 'journal-2', -1, '*state*', 'admin'],
      type: 'directory',
    };
    const multiHopRoot: ExplorerSelection['path'] = [
      20, 'journal-1', -1, 'journal-2', -1, '*state*',
    ];
    const get = jest.fn()
      .mockRejectedValueOnce(new Error('Not authorized'))
      .mockResolvedValueOnce({ content: ['directory', [], true] });
    await expect(accessibleRemoteSelection({ get }, multiHop, multiHopRoot))
      .resolves.toEqual({ path: multiHopRoot, type: 'directory' });
  });

  it('does not enter a route when neither the selection nor its root is accessible', async () => {
    const get = jest.fn().mockRejectedValue(new Error('Not authorized'));
    await expect(accessibleRemoteSelection({ get }, preserved, fallbackPath))
      .rejects.toThrow('Not authorized');
  });
});

describe('content-to-tree ancestor reveal', () => {
  it('derives only ancestor IDs relative to the active routed root', () => {
    expect(Array.from(treeAncestorNodeIds(
      'ledger',
      [42],
      [42, '*state*', 'alice', 'docs', 'draft%20one.txt'],
    ))).toEqual([
      'ledger/state',
      'ledger/state/symbol:"alice"',
      'ledger/state/symbol:"alice"/symbol:"docs"',
    ]);
    expect(treeAncestorNodeIds(
      'ledger',
      [42],
      [41, '*state*', 'alice'],
    ).size).toBe(0);
  });

  it('preserves typed segment identity in ancestor node IDs', () => {
    const root = ['*state*'];
    const integerId = Array.from(treeAncestorNodeIds('stage', root, [
      '*state*', 123, 'child',
    ]))[0];
    const taggedSymbolId = Array.from(treeAncestorNodeIds('stage', root, [
      '*state*', 'integer:123', 'child',
    ]))[0];
    const schemeStringId = Array.from(treeAncestorNodeIds('stage', root, [
      '*state*', { '*type/string*': 'abc' }, 'child',
    ]))[0];
    const stringTaggedSymbolId = Array.from(treeAncestorNodeIds('stage', root, [
      '*state*', 'string:"abc"', 'child',
    ]))[0];

    expect(integerId).not.toBe(taggedSymbolId);
    expect(schemeStringId).not.toBe(stringTaggedSymbolId);
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
    jest.restoreAllMocks();
  });

  it('lands both modes at the terminal state root and keeps that Stage root browsing-only', async () => {
    mockReadyExplorer([10], false);
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    expect(screen.queryByText('+ Put')).not.toBeInTheDocument();
    expect(screen.queryByText('+ Directory')).not.toBeInTheDocument();
    expect(screen.queryByText('Upload Document')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Rename')).not.toBeInTheDocument();
    expect(screen.queryByText('Delete')).not.toBeInTheDocument();
    expect(screen.queryByText('Home')).not.toBeInTheDocument();
  });

  it('closes the bridge picker on every primary-mode and fragment navigation', async () => {
    mockReadyExplorer();
    (JournalService.prototype.getAdmins as jest.Mock).mockResolvedValue(['alice']);
    jest.spyOn(JournalService.prototype, 'getAuthorizations').mockResolvedValue([]);
    jest.spyOn(JournalService.prototype, 'getAdminConfig').mockResolvedValue({
      admins: ['alice'], bridges: [], localName: 'Self', localEndpoint: 'http://self',
      windowSize: 32, bridgeAccept: 'auto', bridgePreapprovals: {},
    });
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    for (const modeName of ['Stage', 'Ledger', 'Access', 'Admin']) {
      fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
      await waitFor(() => expect(window.location.hash).toBe('#stage/'));
      fireEvent.click(screen.getByTitle('Open a bridge'));
      expect(await screen.findByRole('button', { name: 'journal-1' })).toBeInTheDocument();
      fireEvent.click(await screen.findByRole('button', { name: modeName }));
      expect(screen.queryByRole('button', { name: 'journal-1' })).not.toBeInTheDocument();
    }

    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    expect(await screen.findByRole('button', { name: 'journal-1' })).toBeInTheDocument();
    window.location.hash = '#stage/';
    window.dispatchEvent(new Event('hashchange'));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'journal-1' }))
      .not.toBeInTheDocument());
  });

  it('does not repeat the local admin capability probe when the working route changes', async () => {
    mockReadyExplorer();
    const getAdmins = JournalService.prototype.getAdmins as jest.Mock;
    render(<App />);

    await waitFor(() => expect(getAdmins).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));
    expect(getAdmins).toHaveBeenCalledTimes(1);
  });

  it('does not reopen a closed picker when an older bridge lookup completes', async () => {
    const { getBridges } = mockReadyExplorer();
    let resolveBridges: (value: Array<{ name: string; endpoint: string }>) => void = () => undefined;
    getBridges.mockReturnValueOnce(new Promise((resolve) => { resolveBridges = resolve; }));
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await act(async () => resolveBridges([{ name: 'stale-peer', endpoint: 'http://stale' }]));

    expect(screen.queryByRole('button', { name: 'stale-peer' })).not.toBeInTheDocument();
    expect(screen.getByTitle('Open a bridge')).toBeInTheDocument();
  });

  it('exposes the create-only Put modal outside the browsing-only namespace root', async () => {
    mockReadyExplorer();
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));

    expect(document.querySelector('.content-path')).toHaveTextContent('State');
    expect(screen.queryByText('+ Put')).not.toBeInTheDocument();
    let alice: HTMLElement | undefined;
    await waitFor(() => {
      alice = screen.getAllByText('alice')
        .find((node) => node.closest('.directory-item'));
      expect(alice).toBeDefined();
    });
    fireEvent.click(alice!);

    expect(await screen.findByText('+ Put')).toBeInTheDocument();
    expect(screen.getByText('+ Directory')).toBeInTheDocument();
    expect(screen.getByText('Upload Document')).toBeInTheDocument();
    expect(screen.getByTitle('Rename')).toBeInTheDocument();
    expect(screen.getByText('Delete')).toBeInTheDocument();

    fireEvent.click(screen.getByText('+ Put'));
    expect(await screen.findByRole('dialog', { name: 'Put resource' })).toBeInTheDocument();
    expect(screen.getByLabelText('String')).toBeChecked();
    expect(screen.queryByLabelText('JSON')).not.toBeInTheDocument();
    expect(screen.getByText(/target must be absent/)).toBeInTheDocument();
  });

  it('keeps failed Put input in the modal and reports the exact error inline', async () => {
    mockReadyExplorer();
    jest.spyOn(JournalService.prototype, 'putResource').mockRejectedValue(new Error('denied'));
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    let alice: HTMLElement | undefined;
    await waitFor(() => {
      alice = screen.getAllByText('alice')
        .find((node) => node.closest('.directory-item'));
      expect(alice).toBeDefined();
    });
    fireEvent.click(alice!);
    fireEvent.click(await screen.findByText('+ Put'));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'draft' } });
    fireEvent.click(screen.getByRole('button', { name: 'Put' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('denied');
    expect(screen.getByLabelText('Name')).toHaveValue('draft');
  });

  it('keeps the Put modal and selection unchanged when create-only comparison returns false', async () => {
    mockReadyExplorer();
    jest.spyOn(JournalService.prototype, 'putResource').mockResolvedValue(false);
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    let alice: HTMLElement | undefined;
    await waitFor(() => {
      alice = screen.getAllByText('alice').find((node) => node.closest('.directory-item'));
      expect(alice).toBeDefined();
    });
    fireEvent.click(alice!);
    const selectionBefore = window.location.hash;
    fireEvent.click(await screen.findByText('+ Put'));
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'existing' } });
    fireEvent.click(screen.getByRole('button', { name: 'Put' }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Create conflict');
    expect(screen.getByRole('dialog', { name: 'Put resource' })).toBeInTheDocument();
    expect(screen.getByLabelText('Name')).toHaveValue('existing');
    expect(window.location.hash).toBe(selectionBefore);
  });

  it('hard-resets Stage and Ledger routes and selections on every toolbar change', async () => {
    mockReadyExplorer();
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/9/state/'));
    clickTreeNode('alice');

    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']);
    expect(document.querySelector('.content-path')).toHaveTextContent('State');

    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']);
    expect(document.querySelector('.content-path')).toHaveTextContent('State');
  });

  it('reveals content selections in the tree without collapsing unrelated branches', async () => {
    const { get } = mockReadyExplorer();
    const entries = (path: JournalPath) => {
      const leaf = path[path.length - 1];
      if (leaf === '*state*') return [
        { name: 'alice', type: 'directory' as const },
        { name: 'bob', type: 'directory' as const },
      ];
      if (leaf === 'alice') return [{ name: 'docs', type: 'directory' as const }];
      if (leaf === 'docs') return [{ name: 'draft.txt', type: 'value' as const }];
      if (leaf === 'bob') return [{ name: 'kept-open', type: 'directory' as const }];
      return [];
    };
    (JournalService.prototype.getDirectoryEntries as jest.Mock)
      .mockImplementation(async (path) => entries(path));
    get.mockImplementation(async (path) => ({
      content: ['directory', Object.fromEntries(entries(path).map((entry) => [
        entry.name,
        entry.type === 'value' ? 'value' : 'directory',
      ])), true],
    }));
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    const bob = screen.getAllByText('bob').find((candidate) =>
      candidate.closest('.tree-node:not(.tree-root-node)'),
    );
    const bobNode = bob?.closest('.tree-node');
    const bobToggle = bobNode?.querySelector<HTMLButtonElement>('.tree-node-icon');
    if (!bobToggle) throw new Error('Bob tree toggle not found');
    fireEvent.click(bobToggle);
    await waitFor(() => expect(bobToggle).toHaveTextContent('▼'));

    clickContentItem('alice');
    await waitFor(() => expect(document.querySelector('.directory-item')).toHaveTextContent('docs'));
    clickContentItem('docs');
    await waitFor(() => expect(document.querySelector('.directory-item')).toHaveTextContent('draft.txt'));
    clickContentItem('draft.txt');

    await waitFor(() => {
      const selected = document.querySelector('.tree-node-content.selected');
      expect(selected).toHaveTextContent('draft.txt');
    });
    const expandedLabels = Array.from(document.querySelectorAll('.tree-node-icon'))
      .filter((node) => node.textContent === '▼')
      .map((node) => node.parentElement?.textContent);
    expect(expandedLabels.some((label) => label?.includes('bob'))).toBe(true);
    expect(expandedLabels.some((label) => label?.includes('alice'))).toBe(true);
    expect(expandedLabels.some((label) => label?.includes('docs'))).toBe(true);
  });

  it('keeps Access and Admin local and hard-resets routed browsing state', async () => {
    mockReadyExplorer();
    (JournalService.prototype.getAdmins as jest.Mock).mockResolvedValue(['alice']);
    jest.spyOn(JournalService.prototype, 'getAuthorizations').mockResolvedValue([]);
    jest.spyOn(JournalService.prototype, 'getAdminConfig').mockResolvedValue({
      admins: ['alice'], bridges: [], localName: 'Self', localEndpoint: 'http://self',
      windowSize: 32, bridgeAccept: 'auto', bridgePreapprovals: {},
    });
    render(<App />);

    fireEvent.click(await screen.findByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));

    fireEvent.click(screen.getByRole('button', { name: 'Access' }));
    expect(await screen.findByText('Add rule')).toBeInTheDocument();
    expect(document.querySelector('.route-builder')).toBeNull();
    expect(screen.getByRole('button', { name: 'Access' })).not.toBeDisabled();

    fireEvent.click(await screen.findByRole('button', { name: 'Admin' }));
    expect(await screen.findByText('Admin Users')).toBeInTheDocument();
    expect(document.querySelector('.route-builder')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']);
  });

  it('resets to verified terminal roots for Stage- and Ledger-initiated route changes', async () => {
    mockReadyExplorer();
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    await waitFor(() => expect(screen.getAllByText('bob')
      .some((node) => node.closest('.tree-node:not(.tree-root-node)'))).toBe(true));
    clickTreeNode('bob');
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));
    clickTreeNode('alice');
    fireEvent.click(screen.getByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('State'));
    fireEvent.click(screen.getByTitle('Move back one journal'));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']));

    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Self' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
  });

  it('returns to the synthetic root in local and routed Stage and Ledger views', async () => {
    mockReadyExplorer();
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    clickTreeNode('alice');
    clickTreeRoot();
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    await waitFor(() => expect(screen.getAllByText('bob')
      .some((node) => node.closest('.tree-node:not(.tree-root-node)'))).toBe(true));
    clickTreeNode('bob');
    clickTreeRoot();
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));

    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));
    clickTreeNode('alice');
    clickTreeRoot();
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('State'));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/9/state/'));
    clickTreeNode('bob');
    await waitFor(() => expect(document.querySelector('.tree-root-node .tree-node-label'))
      .not.toBeNull());
    clickTreeRoot();
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/9/state/'));
  });

  it('uses one coherent routed Ledger bar for every hop and snapshot control', async () => {
    const { getBridges } = mockReadyExplorer();
    const setFederationContext = jest.spyOn(JournalService.prototype, 'setFederationContext');
    getBridges.mockResolvedValue([
      { name: 'journal-1', endpoint: 'http://journal-1' },
      { name: 'journal-2', endpoint: 'http://journal-2' },
    ]);
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.change(screen.getByLabelText('Self snapshot'), { target: { value: '8' } });
    fireEvent.blur(screen.getByLabelText('Self snapshot'));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/bridge/journal-1/9/state/'));
    expect(document.querySelectorAll('.route-builder')).toHaveLength(1);
    expect(screen.getByLabelText('journal-1 snapshot')).toHaveValue('9');
    fireEvent.change(screen.getByLabelText('journal-1 snapshot'), { target: { value: '-2' } });
    fireEvent.blur(screen.getByLabelText('journal-1 snapshot'));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/bridge/journal-1/8/state/'));
    expect(setFederationContext).toHaveBeenCalledWith({
      route: ['journal-1'], historyIndexes: [8, 8],
    });

    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-2' }));
    await waitFor(() => expect(screen.getByLabelText('journal-2 snapshot')).toBeInTheDocument());
    expect(screen.getByLabelText('Self snapshot')).toHaveValue('8');
    expect(screen.getByLabelText('journal-1 snapshot')).toHaveValue('8');
    fireEvent.click(screen.getByRole('button', { name: 'Older journal-2 snapshot' }));
    await waitFor(() => expect(window.location.hash)
      .toBe('#ledger/8/bridge/journal-1/8/bridge/journal-2/8/state/'));
    expect(setFederationContext).toHaveBeenCalledWith({
      route: ['journal-1', 'journal-2'], historyIndexes: [8, 8, 8],
    });
    fireEvent.click(screen.getByRole('button', { name: 'journal-1' }));

    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/bridge/journal-1/8/state/'));
    expect(screen.queryByLabelText('journal-2 snapshot')).not.toBeInTheDocument();
    expect(screen.getByLabelText('journal-1 snapshot')).toHaveValue('8');
  });

  it('preserves contextual maxima while extending and shortening a route', async () => {
    const { getBridges } = mockReadyExplorer([10]);
    getBridges.mockResolvedValue([
      { name: 'journal-1', endpoint: 'http://journal-1' },
      { name: 'journal-2', endpoint: 'http://journal-2' },
    ]);
    const getSize = JournalService.prototype.getSize as jest.Mock;
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash)
      .toBe('#ledger/9/bridge/journal-1/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Older journal-1 snapshot' }));
    await waitFor(() => expect(screen.getByLabelText('journal-1 snapshot')).toHaveValue('8'));

    getSize.mockResolvedValue(15);
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-2' }));
    await waitFor(() => expect(screen.getByLabelText('journal-2 snapshot')).toHaveValue('14'));
    expect(screen.getByLabelText('journal-1 snapshot')).toHaveValue('8');
    expect(screen.getByRole('button', { name: 'Newer journal-1 snapshot' })).not.toBeDisabled();
    expect(getSize).toHaveBeenCalledTimes(2);

    fireEvent.click(screen.getByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(screen.queryByLabelText('journal-2 snapshot')).not.toBeInTheDocument());
    expect(screen.getByLabelText('journal-1 snapshot')).toHaveValue('8');
    expect(screen.getByRole('button', { name: 'Newer journal-1 snapshot' })).not.toBeDisabled();
    expect(getSize).toHaveBeenCalledTimes(2);
  });

  it('pins once with exact requester lineage and the fixed terminal-provider snapshot', async () => {
    const { get, getDirectoryEntries } = mockReadyExplorer([122]);
    (JournalService.prototype.getSize as jest.Mock).mockReset().mockResolvedValue(120);
    getDirectoryEntries.mockResolvedValue([{ name: 'worker', type: 'object' }]);
    const requests: Array<{ path: JournalPath; route: string[] }> = [];
    let pinned = false;
    get.mockImplementation(async function mockGet(this: JournalService, path) {
      requests.push({ path, route: this.getFederationContext().route });
      return { content: { '*type/string*': 'value' }, 'pinned?': pinned };
    });
    const probeObjectApi = JournalService.prototype.probeObjectApi as jest.Mock;
    probeObjectApi.mockRejectedValue(new Error('not an object'));
    const pin = jest.spyOn(JournalService.prototype, 'pin').mockImplementation(async () => {
      pinned = true;
      return true;
    });
    const verifyPinnedInventories = jest.spyOn(
      JournalService.prototype, 'verifyPinnedInventories',
    ).mockResolvedValue(undefined);
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/121/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash)
      .toBe('#ledger/121/bridge/journal-1/119/state/'));
    clickTreeNode('worker');

    const pinPath: JournalPath = [
      121, 'journal-1', 119, '*state*', 'worker',
    ];
    const providerPath: JournalPath = [119, '*state*', 'worker'];
    await waitFor(() => expect(probeObjectApi).toHaveBeenCalledWith({
      path: providerPath, historical: true,
    }));
    await waitFor(() => expect(requests).toContainEqual({ path: pinPath, route: [] }));
    await waitFor(() => expect(requests).toContainEqual({
      path: providerPath, route: ['journal-1'],
    }));

    fireEvent.click(await screen.findByRole('button', { name: 'Pin' }));
    await waitFor(() => expect(screen.getByText(
      'Pinned with local and terminal-provider readback',
    )).toBeInTheDocument());
    expect(pin).toHaveBeenCalledTimes(1);
    expect(pin).toHaveBeenCalledWith(pinPath);
    expect(verifyPinnedInventories).toHaveBeenCalledWith(pinPath);
    expect(requests.filter((request) => JSON.stringify(request) === JSON.stringify({
      path: providerPath, route: ['journal-1'],
    })).length).toBeGreaterThanOrEqual(2);
  });

  it('preserves an exact provider snapshot through mounted retained navigation', async () => {
    window.location.hash = '#ledger/8/bridge/journal-0/4/retained/bridge/journal-2/3/state/';
    const { get } = mockReadyExplorer([9]);
    const getRequests: Array<{ path: JournalPath; route: string[] }> = [];
    get.mockImplementation(async function mockGet(this: JournalService, path) {
      getRequests.push({ path, route: this.getFederationContext().route });
      return { content: ['directory', { alice: 'directory' }, true] };
    });
    const directoryRequests: Array<{ path: JournalPath; route: string[] }> = [];
    const directoryEntries = jest.spyOn(JournalService.prototype, 'getDirectoryEntries')
      .mockImplementation(async function mockDirectory(this: JournalService, path) {
        directoryRequests.push({ path, route: this.getFederationContext().route });
        if (JSON.stringify(path) === JSON.stringify([4, '*bridge*'])) {
          return [{ name: 'journal-2', type: 'directory' }];
        }
        if (JSON.stringify(path) === JSON.stringify([
          4, '*bridge*', 'journal-2', 3, '*state*',
        ])) {
          return [{ name: 'alice', type: 'directory' }];
        }
        return [];
      });
    const inventoryRequests: Array<{ path: JournalPath; route: string[] }> = [];
    const inventory = jest.spyOn(JournalService.prototype, 'getChainInventory')
      .mockImplementation(async function mockInventory(this: JournalService, path) {
        inventoryRequests.push({ path, route: this.getFederationContext().route });
        return { indexes: [3], complete: true };
      });

    render(<App />);

    const retainedStatePath: JournalPath = [
      4, '*bridge*', 'journal-2', 3, '*state*',
    ];
    await waitFor(() => expect(getRequests).toContainEqual({
      path: retainedStatePath, route: ['journal-0'],
    }), { timeout: 1000 });
    await waitFor(() => expect(directoryRequests).toContainEqual({
      path: [8, '*state*'], route: [],
    }), { timeout: 1000 });
    expect(directoryRequests).not.toContainEqual({
      path: [8, '*state*'], route: ['journal-0'],
    });
    expect(directoryRequests.some(({ path }) =>
      JSON.stringify(path) === JSON.stringify([9, '*state*']),
    )).toBe(false);
    expect(get.mock.calls.some(([path]) => JSON.stringify(path) === JSON.stringify([
      -1, '*bridge*', 'journal-2', 3, '*state*',
    ]))).toBe(false);
    expect(get.mock.calls.some(([path]) => JSON.stringify(path) === JSON.stringify([
      8, '*bridge*', 'journal-2', 3, '*state*',
    ]))).toBe(false);

    let bridges: HTMLElement | undefined;
    await waitFor(() => {
      bridges = screen.getAllByText('Bridges').find((node) =>
        node.closest('.retained-tree'),
      );
      expect(bridges).toBeDefined();
    }, { timeout: 1000 });
    const bridgeToggle = bridges!.closest('.tree-node')?.querySelector('.tree-node-icon');
    if (!bridgeToggle) throw new Error('Bridges toggle not found');
    fireEvent.click(bridgeToggle);
    const journal2 = await screen.findByText('journal-2');
    const journal2Toggle = journal2.closest('.tree-node')?.querySelector('.tree-node-icon');
    if (!journal2Toggle) throw new Error('journal-2 toggle not found');
    fireEvent.click(journal2Toggle);
    const retainedIndex = await screen.findByText('3');
    const retainedIndexToggle = retainedIndex.closest('.tree-node')?.querySelector('.tree-node-icon');
    if (!retainedIndexToggle) throw new Error('retained index toggle not found');
    fireEvent.click(retainedIndexToggle);
    expect(inventory).toHaveBeenCalledWith([4, '*bridge*', 'journal-2']);
    expect(inventoryRequests).toContainEqual({
      path: [4, '*bridge*', 'journal-2'], route: ['journal-0'],
    });
    expect(directoryEntries).toHaveBeenCalledWith([
      4, '*bridge*', 'journal-2', 3, '*state*',
    ]);
    expect(directoryRequests).toContainEqual({
      path: [4, '*bridge*', 'journal-2', 3, '*state*'], route: ['journal-0'],
    });
    expect(directoryEntries).not.toHaveBeenCalledWith([
      -1, '*bridge*', 'journal-2', 3, '*state*',
    ]);
    expect(directoryEntries).not.toHaveBeenCalledWith([
      8, '*bridge*', 'journal-2', 3, '*state*',
    ]);
    expect(document.querySelector('.tree-load-error')).toBeNull();
    expect(window.location.hash)
      .toBe('#ledger/8/bridge/journal-0/4/retained/bridge/journal-2/3/state/');
  });

  it('enters a route with divergent histories without probing the requester index remotely', async () => {
    const { get } = mockReadyExplorer([141]);
    (JournalService.prototype.getSize as jest.Mock).mockReset().mockResolvedValue(120);
    const admissions: Array<{ path: JournalPath; route: string[] }> = [];
    get.mockImplementation(async function mockGet(this: JournalService, path) {
      const route = this.getFederationContext().route;
      if ((path.length === 1 && path[0] === '*state*')
          || (path.length === 2 && (path[0] === 140 || path[0] === 119)
            && path[1] === '*state*')) {
        admissions.push({ path, route });
      }
      if (route.length > 0 && path[0] === 140) {
        throw new Error('Index is out of bounds: 140');
      }
      return { content: ['directory', { alice: 'directory' }, true] };
    });
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/140/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/140/bridge/journal-1/119/state/'));

    expect(admissions).toContainEqual({ path: ['*state*'], route: ['journal-1'] });
    expect(admissions).toContainEqual({ path: [140, '*state*'], route: [] });
    expect(admissions).toContainEqual({ path: [119, '*state*'], route: ['journal-1'] });
    expect(admissions).not.toContainEqual({ path: [140, '*state*'], route: ['journal-1'] });
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('commits the sole bridge-entry latest result over older pending latest requests', async () => {
    mockReadyExplorer([10]);
    const getLocalSize = JournalService.prototype.getLocalSize as jest.Mock;
    let resolveInitial: (size: number) => void = () => undefined;
    let resolveLedgerEntry: (size: number) => void = () => undefined;
    getLocalSize.mockReset();
    getLocalSize
      .mockReturnValueOnce(new Promise<number>((resolve) => { resolveInitial = resolve; }))
      .mockReturnValueOnce(new Promise<number>((resolve) => { resolveLedgerEntry = resolve; }))
      .mockResolvedValue(15);
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    expect(await screen.findByLabelText('Self snapshot')).toHaveValue('0');
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash)
      .toBe('#ledger/14/bridge/journal-1/9/state/'));
    expect(screen.getByLabelText('Self snapshot')).toHaveValue('14');

    act(() => {
      resolveInitial(10);
      resolveLedgerEntry(11);
    });
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    fireEvent.change(screen.getByLabelText('Self snapshot'), { target: { value: 'invalid' } });
    fireEvent.blur(screen.getByLabelText('Self snapshot'));

    await waitFor(() => expect(screen.getByLabelText('Self snapshot')).toHaveValue('14'));
    expect(window.location.hash).toBe('#ledger/14/bridge/journal-1/9/state/');
    expect(getLocalSize).toHaveBeenCalledTimes(3);
  });

  it('ignores an older route preflight that completes after a newer peer choice', async () => {
    const { get, getBridges } = mockReadyExplorer();
    getBridges.mockResolvedValue([
      { name: 'journal-1', endpoint: 'http://journal-1' },
      { name: 'journal-2', endpoint: 'http://journal-2' },
    ]);
    let resolveFirst: (value: JournalResponse) => void = () => undefined;
    let resolveSecond: (value: JournalResponse) => void = () => undefined;
    const first = new Promise<JournalResponse>((resolve) => { resolveFirst = resolve; });
    const second = new Promise<JournalResponse>((resolve) => { resolveSecond = resolve; });
    get.mockImplementation(async function mockGet(this: JournalService, path) {
      const route = this.getFederationContext().route;
      if (path.length === 1 && path[0] === '*state*' && route[0] === 'journal-1') return first;
      if (path.length === 1 && path[0] === '*state*' && route[0] === 'journal-2') return second;
      return { content: ['directory', { alice: 'directory' }, true] };
    });
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-2' }));
    await act(async () => resolveSecond({ content: ['directory', {}, true] }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-2/9/state/'));
    await act(async () => resolveFirst({ content: ['directory', {}, true] }));
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    expect(window.location.hash).toBe('#ledger/9/bridge/journal-2/9/state/');
    expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-2']);
  });

  it('keeps a newer snapshot coherent when an older route preflight completes later', async () => {
    const { get } = mockReadyExplorer();
    let resolveRoute: (value: JournalResponse) => void = () => undefined;
    const pendingRoute = new Promise<JournalResponse>((resolve) => { resolveRoute = resolve; });
    get.mockImplementation(async function mockGet(this: JournalService, path) {
      const route = this.getFederationContext().route;
      if (path.length === 1 && path[0] === '*state*' && route[0] === 'journal-1') {
        return pendingRoute;
      }
      return { content: ['directory', { alice: 'directory' }, true] };
    });
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    fireEvent.change(screen.getByLabelText('Self snapshot'), { target: { value: '8' } });
    fireEvent.blur(screen.getByLabelText('Self snapshot'));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/state/'));
    await act(async () => resolveRoute({ content: ['directory', {}, true] }));
    await new Promise((resolve) => window.setTimeout(resolve, 0));

    expect(window.location.hash).toBe('#ledger/8/state/');
    expect(screen.getByLabelText('Self snapshot')).toHaveValue('8');
    expect(document.querySelector('.content-path')).toHaveTextContent('State');
    expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']);
  });

  it('does not enter a route whose terminal namespace root is inaccessible', async () => {
    const { get } = mockReadyExplorer();
    get.mockImplementation(async function mockGet(this: JournalService, _path, _options) {
      if (this.getFederationContext().route.length > 0) {
        throw new Error('Not authorized');
      }
      return { content: ['directory', { alice: 'directory' }, true] };
    });
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Remote route is not accessible');
    expect(window.location.hash).toBe('#ledger/9/state/');
    expect(document.querySelector('.content-path')).toHaveTextContent('State');
    expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']);
  });

  it('resolves fresh Ledger indexes after leaving and re-entering Ledger', async () => {
    mockReadyExplorer([10]);
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    (JournalService.prototype.getLocalSize as jest.Mock).mockResolvedValue(13);
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));

    await waitFor(() => expect(window.location.hash).toBe('#ledger/12/state/'));
    expect(screen.getByLabelText('Self snapshot')).toHaveValue('12');
  });

  it('preserves the selected Ledger suffix across snapshot changes and synchronization', async () => {
    const { get } = mockReadyExplorer([10]);
    const reads: Array<{ path: JournalPath; route: string[] }> = [];
    get.mockImplementation(async function mockGet(this: JournalService, path) {
      reads.push({ path, route: this.getFederationContext().route });
      return { content: ['directory', { alice: 'directory' }, true] };
    });
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    clickTreeNode('alice');
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/alice/'));
    fireEvent.change(screen.getByLabelText('Self snapshot'), { target: { value: '8' } });
    fireEvent.blur(screen.getByLabelText('Self snapshot'));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/state/alice/'));
    (JournalService.prototype.getLocalSize as jest.Mock).mockResolvedValue(11);
    fireEvent.click(screen.getByTitle('Synchronize latest committed root'));
    await act(async () => new Promise((resolve) => window.setTimeout(resolve, 0)));
    expect(window.location.hash).toBe('#ledger/8/state/alice/');
    expect(reads).toContainEqual({ path: [8, '*state*', 'alice'], route: [] });
    expect(screen.getByRole('button', { name: 'Newer Self snapshot' })).not.toBeDisabled();
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

  it('defaults empty and unsupported fragments to the canonical Stage root', async () => {
    window.location.hash = '#unsupported/deep-link';
    mockReadyExplorer([10], false);
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    expect(screen.getByRole('button', { name: 'Stage' })).toHaveClass('active');
  });

  it('preserves an explicit Stage namespace-root deep link without scoping it to the user', async () => {
    window.location.hash = '#stage/';
    mockReadyExplorer();
    render(<App />);

    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('State'));
    expect(window.location.hash).toBe('#stage/');
    expect(screen.queryByText('+ Put')).not.toBeInTheDocument();
  });

  it('restores exact remote Stage routes from direct, copied, and back/forward fragments', async () => {
    jest.spyOn(window.history, 'replaceState').mockImplementation((_data, _unused, url) => {
      const value = String(url ?? '');
      window.location.hash = value.includes('#') ? `#${value.split('#', 2)[1]}` : '';
    });
    // @ts-ignore test-only runtime configuration
    window._env_ = { SYNC_EXPLORER_ENDPOINT: '/api/v1' };
    const remoteHash = '#stage-route/99/bridge/journal-1/-2/bridge/journal%20two/latest/state/bob/data/';
    window.location.hash = remoteHash;
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ identity: { traits: { username: 'alice' } } }),
    } as unknown as Response);
    jest.spyOn(JournalService.prototype, 'ensureDirectory').mockResolvedValue(true);
    jest.spyOn(JournalService.prototype, 'getAdmins').mockRejectedValue(new Error('not admin'));
    jest.spyOn(JournalService.prototype, 'getSize').mockResolvedValue(100);
    jest.spyOn(JournalService.prototype, 'getLocalSize').mockResolvedValue(100);
    jest.spyOn(JournalService.prototype, 'subscribeEvents').mockReturnValue(jest.fn());
    const setFederationContext = jest.spyOn(JournalService.prototype, 'setFederationContext');
    jest.spyOn(JournalService.prototype, 'getDirectoryEntries')
      .mockImplementation(async function mockStageRouteDirectory(this: JournalService) {
        return this.getFederationContext().route.length > 0
          ? [{ name: 'remote-root', type: 'directory' }]
          : [{ name: 'self-root', type: 'directory' }];
      });
    const get = jest.spyOn(JournalService.prototype, 'get').mockResolvedValue({
      content: ['directory', {}, true],
    });

    render(<App />);

    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1', 'journal two']));
    expect(window.location.hash).toBe(remoteHash);
    expect(screen.getByRole('button', { name: 'Access' })).toBeEnabled();
    expect(await screen.findByText('remote-root')).toBeInTheDocument();
    expect(setFederationContext).toHaveBeenCalledWith({
      route: ['journal-1', 'journal two'],
      historyIndexes: [99, -2, -1],
    });
    expect(get).toHaveBeenCalledWith(
      ['*state*', 'bob', 'data'],
      { pinned: false, proof: false },
    );

    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/99/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));

    window.location.hash = '#stage/';
    window.dispatchEvent(new Event('hashchange'));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']));
    expect(screen.getByRole('button', { name: 'Access' })).toBeEnabled();
    await waitFor(() => expect(screen.getByText('self-root')).toBeInTheDocument());
    expect(screen.queryByText('remote-root')).not.toBeInTheDocument();
    await act(async () => new Promise((resolve) => window.setTimeout(resolve, 0)));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/99/state/'));

    window.location.hash = remoteHash;
    window.dispatchEvent(new Event('hashchange'));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.route-builder .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1', 'journal two']));
    expect(screen.getByRole('button', { name: 'Access' })).toBeEnabled();
    expect(window.location.hash).toBe(remoteHash);
    await waitFor(() => expect(screen.getByText('remote-root')).toBeInTheDocument());
    expect(screen.queryByText('self-root')).not.toBeInTheDocument();
    await act(async () => new Promise((resolve) => window.setTimeout(resolve, 0)));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/99/state/'));
  });

  it('loads and preserves the exact routed Ledger deep link', async () => {
    // @ts-ignore test-only runtime configuration
    window._env_ = { SYNC_EXPLORER_ENDPOINT: '/api/v1' };
    window.location.hash = '#ledger/99/bridge/journal-1/state/bob/data/public/';
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ identity: { traits: { username: 'alice' } } }),
    } as unknown as Response);
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
      expect(directoryEntries).toHaveBeenCalledWith([99, '*state*']);
      expect(directoryEntries.mock.calls.length).toBeGreaterThanOrEqual(2);
    }, { timeout: 1000 });
    await waitFor(() => {
      expect(screen.getAllByText('bob').some((node) => node.closest('.tree-node'))).toBe(true);
    });
    expect(screen.queryByText('No documents available for this ledger route.'))
      .not.toBeInTheDocument();
    expect(window.location.hash).toBe('#ledger/99/bridge/journal-1/state/bob/data/public/');
  });

  it('does not reread a settled Stage document for an unrelated change hint', async () => {
    window.location.hash = '#stage/alice/settled.txt';
    const { get, getDirectoryEntries } = mockReadyExplorer();
    get.mockResolvedValue({ content: { '*type/byte-vector*': '736574746c6564' } });
    let onChange: ((event: {
      id: number;
      operation: string;
      path: JournalPath;
      time: string;
    }) => void) | undefined;
    jest.spyOn(JournalService.prototype, 'subscribeEvents').mockImplementation((input) => {
      onChange = input.onChange;
      return jest.fn();
    });
    const documentPath: JournalPath = ['*state*', 'alice', 'settled.txt'];
    const documentReads = () => get.mock.calls.filter(([path]) =>
      JSON.stringify(path) === JSON.stringify(documentPath)).length;

    render(<App />);

    await waitFor(() => expect(documentReads()).toBeGreaterThan(0));
    await act(async () => new Promise((resolve) => window.setTimeout(resolve, 400)));
    const settledReads = documentReads();
    const settledTreeReads = getDirectoryEntries.mock.calls.length;
    expect(onChange).toBeDefined();

    act(() => onChange?.({
      id: 1,
      operation: 'put!',
      path: ['*state*', 'alice', 'social-agent.txt'],
      time: new Date().toISOString(),
    }));
    await waitFor(() => expect(getDirectoryEntries.mock.calls.length)
      .toBeGreaterThan(settledTreeReads));
    expect(documentReads()).toBe(settledReads);

    const malformedPaths = [
      [{}],
      [{ '*type/string*': 'alice', extra: true }],
      [1.5],
      [null],
    ];
    for (let index = 0; index < malformedPaths.length; index++) {
      const path = malformedPaths[index];
      const readsBeforeEvent = documentReads();
      act(() => onChange?.({
        id: index + 2,
        operation: 'put!',
        path: path as unknown as JournalPath,
        time: new Date().toISOString(),
      }));
      await waitFor(() => expect(documentReads()).toBe(readsBeforeEvent + 1));
    }

    const readsBeforeSelectedChange = documentReads();
    act(() => onChange?.({
      id: 6,
      operation: 'put!',
      path: documentPath,
      time: new Date().toISOString(),
    }));
    await waitFor(() => expect(documentReads()).toBe(readsBeforeSelectedChange + 1));
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
