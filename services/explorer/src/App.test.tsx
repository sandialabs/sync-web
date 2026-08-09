import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import App, {
  accessibleRemoteSelection,
  terminalRootSelections,
  treeAncestorNodeIds,
} from './App';
import { ExplorerSelection, JournalResponse } from './types';
import { JournalService } from './services/JournalService';

const mockFetchResponse = (ok: boolean, status: number) =>
  Promise.resolve({ ok, status } as Response);

const mockReadyExplorer = (sizes: number[] = [10]) => {
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
  jest.spyOn(JournalService.prototype, 'getLocalSize').mockResolvedValue(sizes[sizes.length - 1]);
  const getBridges = jest.spyOn(JournalService.prototype, 'getBridges').mockResolvedValue([
    { name: 'journal-1', endpoint: 'http://journal-1' },
  ]);
  jest.spyOn(JournalService.prototype, 'subscribeEvents').mockReturnValue(jest.fn());
  jest.spyOn(JournalService.prototype, 'getDirectoryEntries').mockResolvedValue([
    { name: 'bob', type: 'directory' },
    { name: 'alice', type: 'directory' },
  ]);
  const get = jest.spyOn(JournalService.prototype, 'get').mockResolvedValue({
    content: ['directory', { alice: 'directory', bob: 'directory' }, true],
  });
  return { get, getBridges };
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
  it('verifies both roots using an isolated proposed-route context', async () => {
    const get = jest.fn().mockResolvedValue({ content: ['directory', [], true] });
    const setFederationContext = jest.fn();
    const service = { get, setFederationContext };
    const hops = [
      { key: 'local', kind: 'local' as const, name: 'Self', snapshot: '20' },
      { key: 'peer', kind: 'bridge' as const, name: 'journal-1', snapshot: 'latest' },
    ];

    await expect(terminalRootSelections(service, ['journal-1'], hops, 20)).resolves.toEqual({
      stage: { path: ['*state*'], type: 'directory' },
      ledger: { path: [20, 'journal-1', -1, '*state*'], type: 'directory' },
    });
    expect(get).toHaveBeenNthCalledWith(1, ['*state*'], { pinned: false, proof: false });
    expect(get).toHaveBeenNthCalledWith(
      2, [20, 'journal-1', -1, '*state*'], { pinned: false, proof: false },
    );
    expect(setFederationContext).toHaveBeenCalledTimes(1);
    expect(setFederationContext).toHaveBeenCalledWith({
      route: ['journal-1'], historyIndexes: [20, -1],
    });
  });

  it('fails an inaccessible isolated target probe before route entry', async () => {
    const get = jest.fn().mockRejectedValue(new Error('Not authorized'));
    const setFederationContext = jest.fn();
    const service = { get, setFederationContext };
    const hops = [{ key: 'local', kind: 'local' as const, name: 'Self', snapshot: '9' }];

    await expect(terminalRootSelections(service, [], hops, 9)).rejects.toThrow('Not authorized');
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
      [42, 'journal-1', -3, '*state*'],
      [42, 'journal-1', -3, '*state*', 'alice', 'docs', 'draft%20one.txt'],
    ))).toEqual(['ledger/alice', 'ledger/alice/docs']);
    expect(treeAncestorNodeIds(
      'ledger',
      [42, 'journal-1', -3, '*state*'],
      [42, '*state*', 'alice'],
    ).size).toBe(0);
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
    mockReadyExplorer();
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    expect(screen.queryByText('+ Document')).not.toBeInTheDocument();
    expect(screen.queryByText('+ Directory')).not.toBeInTheDocument();
    expect(screen.queryByText('Upload Document')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Rename')).not.toBeInTheDocument();
    expect(screen.queryByText('Delete')).not.toBeInTheDocument();
    const currentUserNodes = screen.getAllByText('alice')
      .filter((node) => node.closest('.tree-node'));
    expect(currentUserNodes[0]).toHaveClass('tree-node-current-user');
    expect(screen.queryByText('Home')).not.toBeInTheDocument();
  });

  it('optimistically exposes remote Stage mutations outside the browsing-only namespace root', async () => {
    mockReadyExplorer();
    const prompt = jest.spyOn(window, 'prompt').mockReturnValue('denied.txt');
    const createFile = jest.spyOn(JournalService.prototype, 'createFile')
      .mockRejectedValue(new Error('not authorized'));
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));

    expect(document.querySelector('.content-path')).toHaveTextContent('*state*');
    expect(screen.queryByText('+ Document')).not.toBeInTheDocument();
    let alice: HTMLElement | undefined;
    await waitFor(() => {
      alice = screen.getAllByText('alice')
        .find((node) => node.closest('.directory-item'));
      expect(alice).toBeDefined();
    });
    fireEvent.click(alice!);

    expect(await screen.findByText('+ Document')).toBeInTheDocument();
    expect(screen.getByText('+ Directory')).toBeInTheDocument();
    expect(screen.getByText('Upload Document')).toBeInTheDocument();
    expect(screen.getByTitle('Rename')).toBeInTheDocument();
    expect(screen.getByText('Delete')).toBeInTheDocument();

    fireEvent.click(screen.getByText('+ Document'));
    expect(prompt).toHaveBeenCalledWith('Enter document name:');
    expect(createFile).toHaveBeenCalledWith(['*state*', 'alice'], 'denied.txt');
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Create document failed: not authorized',
    );
    expect(document.querySelector('.content-path')).toHaveTextContent('alice');
    expect(screen.getByText('+ Document')).toBeInTheDocument();
  });

  it('uses document terminology for Stage creation prompts and errors', async () => {
    mockReadyExplorer();
    const prompt = jest.spyOn(window, 'prompt').mockReturnValue('draft.txt');
    jest.spyOn(JournalService.prototype, 'createFile').mockRejectedValue(new Error('denied'));
    render(<App />);

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
    fireEvent.click(await screen.findByText('+ Document'));

    expect(prompt).toHaveBeenCalledWith('Enter document name:');
    expect(await screen.findByRole('alert')).toHaveTextContent('Create document failed: denied');
  });

  it('preserves the route and per-tab selections across Stage and Ledger changes', async () => {
    mockReadyExplorer();
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/state/'));
    clickTreeNode('alice');
    await waitFor(() => expect(window.location.hash)
      .toBe('#ledger/9/bridge/journal-1/state/alice/'));

    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash)
      .toBe('#stage-route/latest/bridge/journal-1/latest/state/'));
    expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']);
    expect(screen.getByRole('button', { name: 'Access · Self' })).toBeDisabled();
    await waitFor(() => expect(screen.getAllByText('bob')
      .some((node) => node.closest('.tree-node:not(.tree-root-node)'))).toBe(true));
    clickTreeNode('bob');
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('bob'));

    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash)
      .toBe('#ledger/9/bridge/journal-1/state/alice/'));
    expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']);

    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('bob'));
    expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']);
  });

  it('reveals content selections in the tree without collapsing unrelated branches', async () => {
    const { get } = mockReadyExplorer();
    const entries = (path: Array<string | number>) => {
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

  it('preserves local Stage and Ledger selections while visiting Self-local Access and Admin', async () => {
    mockReadyExplorer();
    (JournalService.prototype.getAdmins as jest.Mock).mockResolvedValue(['alice']);
    jest.spyOn(JournalService.prototype, 'getAuthorizations').mockResolvedValue([]);
    jest.spyOn(JournalService.prototype, 'getAdminConfig').mockResolvedValue({
      admins: ['alice'],
      bridges: [],
      localName: 'Self',
      localEndpoint: 'http://self',
      windowSize: 32,
      bridgeAccept: 'auto',
      bridgePreapprovals: {},
    });
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    clickTreeNode('alice');
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/alice/'));
    fireEvent.click(await screen.findByRole('button', { name: 'Access' }));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/alice/'));

    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    await waitFor(() => expect(screen.getAllByText('bob')
      .some((node) => node.closest('.tree-node:not(.tree-root-node)'))).toBe(true));
    clickTreeNode('bob');
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('bob'));
    fireEvent.click(await screen.findByRole('button', { name: 'Admin' }));
    expect(await screen.findByText('Admin Users')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('bob'));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/alice/'));
  });

  it('resets to verified terminal roots for Stage- and Ledger-initiated route changes', async () => {
    mockReadyExplorer();
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe('#stage/'));
    await waitFor(() => expect(screen.getAllByText('bob')
      .some((node) => node.closest('.tree-node:not(.tree-root-node)'))).toBe(true));
    clickTreeNode('bob');
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));
    clickTreeNode('alice');
    fireEvent.click(screen.getByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('*state*'));
    fireEvent.click(screen.getByTitle('Move back one journal'));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']));

    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/state/'));
    fireEvent.click(screen.getByRole('button', { name: 'Self' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
  });

  it('returns to the synthetic root in local and routed Stage and Ledger views', async () => {
    mockReadyExplorer();
    render(<App />);

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
    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']));
    clickTreeNode('alice');
    clickTreeRoot();
    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('*state*'));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/state/'));
    clickTreeNode('bob');
    clickTreeRoot();
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-1/state/'));
  });

  it('truncates a breadcrumb route while preserving prefix Ledger snapshots', async () => {
    const { getBridges } = mockReadyExplorer();
    getBridges.mockResolvedValue([
      { name: 'journal-1', endpoint: 'http://journal-1' },
      { name: 'journal-2', endpoint: 'http://journal-2' },
    ]);
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.change(screen.getByLabelText('Self snapshot'), { target: { value: '8' } });
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/bridge/journal-1/state/'));
    fireEvent.change(screen.getByLabelText('journal-1 snapshot'), { target: { value: '-3' } });
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/bridge/journal-1/-3/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-2' }));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1', 'journal-2']));
    fireEvent.click(screen.getByRole('button', { name: 'journal-1' }));

    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/bridge/journal-1/-3/state/'));
    expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1']);
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

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    fireEvent.click(screen.getByRole('button', { name: 'journal-2' }));
    await act(async () => resolveSecond({ content: ['directory', {}, true] }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/bridge/journal-2/state/'));
    await act(async () => resolveFirst({ content: ['directory', {}, true] }));
    await new Promise((resolve) => window.setTimeout(resolve, 0));
    expect(window.location.hash).toBe('#ledger/9/bridge/journal-2/state/');
    expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
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

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    fireEvent.change(screen.getByLabelText('Self snapshot'), { target: { value: '8' } });
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/state/'));
    await act(async () => resolveRoute({ content: ['directory', {}, true] }));
    await new Promise((resolve) => window.setTimeout(resolve, 0));

    expect(window.location.hash).toBe('#ledger/8/state/');
    expect(screen.getByLabelText('Self snapshot')).toHaveValue('8');
    expect(document.querySelector('.content-path')).toHaveTextContent('*state*');
    expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
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

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    fireEvent.click(screen.getByTitle('Open a bridge'));
    fireEvent.click(await screen.findByRole('button', { name: 'journal-1' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Remote route is not accessible');
    expect(window.location.hash).toBe('#ledger/9/state/');
    expect(document.querySelector('.content-path')).toHaveTextContent('*state*');
    expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']);
  });

  it('preserves the selected Ledger suffix across snapshot changes and synchronization', async () => {
    mockReadyExplorer([10, 11]);
    render(<App />);

    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/'));
    clickTreeNode('alice');
    await waitFor(() => expect(window.location.hash).toBe('#ledger/9/state/alice/'));
    fireEvent.change(screen.getByLabelText('Self snapshot'), { target: { value: '8' } });
    await waitFor(() => expect(window.location.hash).toBe('#ledger/8/state/alice/'));
    fireEvent.click(screen.getByTitle('Synchronize latest committed root'));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/10/state/alice/'));
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

  it('preserves an explicit Stage namespace-root deep link without scoping it to the user', async () => {
    window.location.hash = '#stage/';
    mockReadyExplorer();
    render(<App />);

    await waitFor(() => expect(document.querySelector('.content-path')).toHaveTextContent('*state*'));
    expect(window.location.hash).toBe('#stage/');
    expect(screen.queryByText('+ Document')).not.toBeInTheDocument();
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

    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1', 'journal two']));
    expect(window.location.hash).toBe(remoteHash);
    expect(screen.getByRole('button', { name: 'Access · Self' })).toBeDisabled();
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
    await waitFor(() => expect(window.location.hash).toBe(
      '#ledger/99/bridge/journal-1/-2/bridge/journal%20two/state/',
    ));
    fireEvent.click(screen.getByRole('button', { name: 'Stage' }));
    await waitFor(() => expect(window.location.hash).toBe(remoteHash));

    window.location.hash = '#stage/';
    window.dispatchEvent(new Event('hashchange'));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self']));
    expect(screen.getByRole('button', { name: 'Access' })).toBeEnabled();
    await waitFor(() => expect(screen.getByText('self-root')).toBeInTheDocument());
    expect(screen.queryByText('remote-root')).not.toBeInTheDocument();
    await act(async () => new Promise((resolve) => window.setTimeout(resolve, 0)));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe('#ledger/99/state/'));

    window.location.hash = remoteHash;
    window.dispatchEvent(new Event('hashchange'));
    await waitFor(() => expect(Array.from(document.querySelectorAll('.working-route .hop-tag'))
      .map((node) => node.textContent)).toEqual(['Self', 'journal-1', 'journal two']));
    expect(screen.getByRole('button', { name: 'Access · Self' })).toBeDisabled();
    expect(window.location.hash).toBe(remoteHash);
    await waitFor(() => expect(screen.getByText('remote-root')).toBeInTheDocument());
    expect(screen.queryByText('self-root')).not.toBeInTheDocument();
    await act(async () => new Promise((resolve) => window.setTimeout(resolve, 0)));
    fireEvent.click(screen.getByRole('button', { name: 'Ledger' }));
    await waitFor(() => expect(window.location.hash).toBe(
      '#ledger/99/bridge/journal-1/-2/bridge/journal%20two/state/',
    ));
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
    expect(window.location.hash).toBe('#ledger/99/bridge/journal-1/state/bob/data/public/');
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
