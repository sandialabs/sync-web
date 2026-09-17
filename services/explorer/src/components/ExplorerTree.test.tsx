import React from 'react';
import { act, fireEvent, render, screen, waitFor, waitForElementToBeRemoved } from '@testing-library/react';
import ExplorerTree from './ExplorerTree';
import { JournalService } from '../services/JournalService';

describe('ExplorerTree', () => {
  const mockJournalService = {
    getDirectoryEntries: jest.fn(),
    getChainInventory: jest.fn(),
  } as unknown as JournalService;

  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('shows directories before files using segmented sorting', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockResolvedValue([
      { name: 'alpha', type: 'value' },
      { name: '10-foo', type: 'directory' },
      { name: '2-foo', type: 'directory' },
      { name: 'beta', type: 'value' },
    ]);

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    const twoFoo = await screen.findByText('2-foo');
    const tenFoo = screen.getByText('10-foo');
    const alpha = screen.getByText('alpha');
    const beta = screen.getByText('beta');

    const labels = [twoFoo, tenFoo, alpha, beta].map((node) => node.closest('button')?.textContent);
    expect(labels).toEqual(['▣2-foo', '▣10-foo', '▤alpha', '▤beta']);
  });

  it('renders object metadata as a non-expandable object leaf', async () => {
    const onSelect = jest.fn();
    (mockJournalService.getDirectoryEntries as jest.Mock).mockResolvedValue([
      { name: 'counter', type: 'object' },
    ]);

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={onSelect}
      />,
    );

    const label = await screen.findByText('counter');
    expect(label.closest('button')).toHaveTextContent('◆counter');
    const node = label.closest('.tree-node');
    expect(node?.querySelector('.tree-node-icon')).toHaveClass('disabled');
    fireEvent.click(label);
    expect(onSelect).toHaveBeenCalledWith({ path: ['*state*', 'counter'], type: 'object' });
    expect(mockJournalService.getDirectoryEntries).toHaveBeenCalledTimes(1);
  });

  it('sorts and emphasizes the signed-in user at the namespace root', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockResolvedValue([
      { name: 'zara', type: 'directory' },
      { name: 'bob', type: 'directory' },
      { name: 'alice', type: 'directory' },
    ]);

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        currentUser="bob"
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    const bob = await screen.findByText('bob');
    const labels = screen.getAllByRole('button')
      .filter((button) => button.classList.contains('tree-node-label'))
      .map((button) => button.textContent);
    expect(labels).toEqual(['▣State', '▣bob', '▣alice', '▣zara']);
    expect(bob).toHaveClass('tree-node-current-user');
  });

  it('shows an empty state for an empty stage tree', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockResolvedValue([]);

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    expect(await screen.findByText('No local documents yet.')).toBeInTheDocument();
  });

  it('navigates admitted ancestor directories without a direct-path control', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock)
      .mockResolvedValueOnce([{ name: 'data', type: 'unknown' }])
      .mockResolvedValueOnce([
        { name: 'public', type: 'unknown' },
        { name: 'journal-1', type: 'unknown' },
      ]);

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[12]}
        selected={null}
        expandedNodes={new Set(['ledger/state/symbol:"data"'])}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    expect(await screen.findByText('public')).toBeInTheDocument();
    expect(screen.getByText('journal-1')).toBeInTheDocument();
    expect(mockJournalService.getDirectoryEntries).toHaveBeenNthCalledWith(
      2,
      [12, '*state*', 'data'],
    );
  });

  it('expands only the typed node whose ID matches when a symbol contains an integer tag', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock)
      .mockResolvedValueOnce([
        { name: '123', type: 'directory', pathSegment: 123 },
        { name: 'integer:123', type: 'directory', pathSegment: 'integer:123' },
      ])
      .mockResolvedValueOnce([{ name: 'integer-child', type: 'value' }]);

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={null}
        expandedNodes={new Set(['stage/integer:123'])}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    expect(await screen.findByText('integer-child')).toBeInTheDocument();
    expect(mockJournalService.getDirectoryEntries).toHaveBeenCalledTimes(2);
    expect(mockJournalService.getDirectoryEntries).toHaveBeenNthCalledWith(
      2,
      ['*state*', 123],
    );
    expect(mockJournalService.getDirectoryEntries).not.toHaveBeenCalledWith(
      ['*state*', 'integer:123'],
    );
  });

  it('shows request failures as a stable access error instead of a tree node', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockRejectedValue(
      new Error('journal_error: raw authorization details'),
    );

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[-1]}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Check that the selected journal has granted access to this user and path.',
    );
    expect(screen.queryByText(/raw authorization details/)).not.toBeInTheDocument();
  });

  it('distinguishes a transient snapshot race from an access denial', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockRejectedValue(
      Object.assign(new Error('Index is out of bounds: 257'), { code: 'index-error' }),
    );

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[257]}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('selected ledger snapshot is unavailable');
    expect(alert).not.toHaveTextContent('granted access');
  });

  it('distinguishes an unretained historical route from an access denial', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockRejectedValue(
      Object.assign(
        new Error('bridge-index-error: Bridge is not committed at the selected local index: journal-1 -1'),
        { code: 'bridge-index-error' },
      ),
    );

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[257]}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('selected ledger snapshot is unavailable');
    expect(alert).not.toHaveTextContent('granted access');
  });

  it('localizes denied expansion beneath its row while preserving siblings', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock)
      .mockResolvedValueOnce([
        { name: 'denied', type: 'directory' },
        { name: 'public', type: 'directory' },
      ])
      .mockRejectedValueOnce(new Error('authorization-error: private details'));

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[-1]}
        selected={null}
        expandedNodes={new Set(['ledger/state/symbol:"denied"'])}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    const denied = await screen.findByText('denied');
    const allowedSibling = screen.getByText('public');
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Unable to load this location');
    expect(denied.closest('.tree-node')).toContainElement(alert);
    expect(denied.closest('.tree-node')?.querySelector('.tree-node-icon'))
      .toHaveAttribute('aria-describedby', alert.id);
    expect(allowedSibling).toBeEnabled();
    expect(screen.queryByText(/private details/)).not.toBeInTheDocument();
  });

  it('shows per-node loading feedback while expanding a directory', async () => {
    let resolveChildren: (value: Array<{ name: string; type: 'value' }>) => void = () => {};
    (mockJournalService.getDirectoryEntries as jest.Mock)
      .mockResolvedValueOnce([{ name: 'docs', type: 'directory' }])
      .mockImplementationOnce(() => new Promise((resolve) => {
        resolveChildren = resolve;
      }));

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    fireEvent.click(await screen.findByText('▶'));
    expect(screen.getByLabelText('Loading docs')).toHaveAttribute('aria-busy', 'true');

    resolveChildren([{ name: 'note.txt', type: 'value' }]);
    await waitForElementToBeRemoved(() => screen.queryByLabelText('Loading docs'));
  });

  it('renders a selected clickable Stage namespace root above its children', async () => {
    const onSelect = jest.fn();
    (mockJournalService.getDirectoryEntries as jest.Mock).mockResolvedValue([
      { name: 'alice', type: 'directory' },
    ]);

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={{ path: ['*state*'], type: 'directory' }}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        currentUser="alice"
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={onSelect}
      />,
    );

    const root = await screen.findByText('State');
    const alice = await screen.findByText('alice');
    expect(root.closest('.tree-node-content')).toHaveClass('selected');
    expect(root.closest('.tree-root-node')).toContainElement(alice);
    fireEvent.click(root);
    expect(onSelect).toHaveBeenCalledWith({ path: ['*state*'], type: 'directory' });
  });

  it('navigates retained Bridges through exact inventory indexes and State', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockImplementation(
      async (path: Array<string | number>) => (
        path[path.length - 1] === '*bridge*'
          ? [{ name: 'archive', type: 'directory' }]
          : []
      ),
    );
    (mockJournalService.getChainInventory as jest.Mock).mockResolvedValue({
      indexes: [2, 7], complete: false,
    });
    const rootPath = [9];
    const retainedRootPath = [4];

    const { rerender } = render(
      <ExplorerTree
        mode="ledger"
        rootPath={rootPath}
        retainedRootPath={retainedRootPath}
        selected={null}
        expandedNodes={new Set(['ledger/bridges'])}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    expect(await screen.findByText('archive')).toBeInTheDocument();
    rerender(
      <ExplorerTree
        mode="ledger"
        rootPath={rootPath}
        retainedRootPath={retainedRootPath}
        selected={null}
        expandedNodes={new Set([
          'ledger/bridges', 'ledger/bridges/symbol:"archive"',
          'ledger/bridges/symbol:"archive"/7',
        ])}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );
    expect(await screen.findByText('7')).toBeInTheDocument();
    expect(mockJournalService.getChainInventory).toHaveBeenCalledWith([
      4, '*bridge*', 'archive',
    ]);
    await waitFor(() => expect(mockJournalService.getDirectoryEntries).toHaveBeenCalledWith([
      4, '*bridge*', 'archive', 7, '*state*',
    ]));
    expect(mockJournalService.getDirectoryEntries).not.toHaveBeenCalledWith([
      9, '*bridge*', 'archive', 7, '*state*',
    ]);
  });

  it('omits retained indexes whose authenticated child loads are inaccessible or empty', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockImplementation(
      async (path: Array<string | number>) => {
        if (JSON.stringify(path) === JSON.stringify([9, '*bridge*'])) {
          return [{ name: 'archive', type: 'directory' }];
        }
        if (JSON.stringify(path) === JSON.stringify([9, '*bridge*', 'archive', 2, '*state*'])) {
          throw new Error('authorization-error: unavailable state');
        }
        if (JSON.stringify(path) === JSON.stringify([9, '*bridge*', 'archive', 7, '*state*'])) {
          return [{ name: 'public', type: 'directory' }];
        }
        return [];
      },
    );
    (mockJournalService.getChainInventory as jest.Mock).mockResolvedValue({
      indexes: [2, 7], complete: true,
    });

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[9]}
        selected={null}
        expandedNodes={new Set([
          'ledger/bridges', 'ledger/bridges/symbol:"archive"',
          'ledger/bridges/symbol:"archive"/2', 'ledger/bridges/symbol:"archive"/7',
        ])}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={jest.fn()}
      />,
    );

    expect(await screen.findByText('7')).toBeInTheDocument();
    expect(screen.queryByText('2')).not.toBeInTheDocument();
    expect(screen.queryByText('No accessible contents')).not.toBeInTheDocument();
    expect(mockJournalService.getDirectoryEntries).toHaveBeenCalledWith([
      9, '*bridge*', 'archive', 2, '*state*',
    ]);
    expect(mockJournalService.getDirectoryEntries).toHaveBeenCalledWith([
      9, '*bridge*', 'archive', 7, '*state*',
    ]);
  });

  it('renders a bridge with malformed index children as a selectable leaf', async () => {
    const onSelect = jest.fn();
    (mockJournalService.getDirectoryEntries as jest.Mock).mockImplementation(
      async (path: Array<string | number>) => {
        if (JSON.stringify(path) === JSON.stringify([9, '*bridge*'])) {
          return [{ name: 'archive', type: 'directory' }];
        }
        if (path.includes(2)) {
          throw new Error('malformed retained child');
        }
        return [];
      },
    );
    (mockJournalService.getChainInventory as jest.Mock).mockResolvedValue({
      indexes: [2], complete: true,
    });

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[9]}
        selected={null}
        expandedNodes={new Set(['ledger/bridges'])}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={onSelect}
      />,
    );

    const archive = await screen.findByText('archive');
    const icon = archive.closest('.tree-node')?.querySelector('.tree-node-icon');
    expect(icon).toHaveTextContent('•');
    expect(icon).toHaveClass('disabled');
    expect(screen.queryByText('2')).not.toBeInTheDocument();
    fireEvent.click(archive);
    expect(onSelect).toHaveBeenCalledWith({
      path: [9, '*bridge*', 'archive'], type: 'directory',
    });
  });

  it('discards a retained child load from an older route-edit context', async () => {
    let resolveInventory: (value: { indexes: number[]; complete: boolean }) => void = () => undefined;
    (mockJournalService.getChainInventory as jest.Mock).mockReturnValue(
      new Promise((resolve) => { resolveInventory = resolve; }),
    );
    (mockJournalService.getDirectoryEntries as jest.Mock).mockImplementation(
      async (path: Array<string | number>) => {
        if (JSON.stringify(path) === JSON.stringify([9, '*bridge*'])) {
          return [{ name: 'archive', type: 'directory' }];
        }
        if (path.includes(2) && path[path.length - 1] === '*state*') {
          return [{ name: 'public', type: 'directory' }];
        }
        return [];
      },
    );

    const props = {
      mode: 'ledger' as const,
      selected: null,
      expandedNodes: new Set<string>(),
      journalService: mockJournalService,
      onExpandedNodesChange: jest.fn(),
      onSelect: jest.fn(),
    };
    const { rerender } = render(
      <ExplorerTree {...props} rootPath={[9]} refreshKey={0} />,
    );
    const bridges = await screen.findByText('Bridges');
    const toggle = bridges.closest('.tree-node')?.querySelector('.tree-node-icon');
    if (!toggle) throw new Error('Bridges toggle not found');
    fireEvent.click(toggle);
    await waitFor(() => expect(mockJournalService.getChainInventory).toHaveBeenCalled());

    rerender(<ExplorerTree {...props} rootPath={[10]} refreshKey={1} />);
    await act(async () => resolveInventory({ indexes: [2], complete: true }));
    await act(async () => Promise.resolve());
    expect(screen.queryByText('archive')).not.toBeInTheDocument();
  });

  it('selects the responder-local Ledger State root', async () => {
    const onSelect = jest.fn();
    const rootPath = [42];
    (mockJournalService.getDirectoryEntries as jest.Mock).mockResolvedValue([]);

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={rootPath}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={onSelect}
      />,
    );

    fireEvent.click(await screen.findByText('State'));
    expect(onSelect).toHaveBeenCalledWith({ path: [42, '*state*'], type: 'directory' });
  });

  it('emits a selection when a node is clicked', async () => {
    const onSelect = jest.fn();
    (mockJournalService.getDirectoryEntries as jest.Mock).mockResolvedValue([
      { name: 'draft.txt', type: 'value' },
    ]);

    render(
      <ExplorerTree
        mode="stage"
        rootPath={['*state*']}
        selected={null}
        expandedNodes={new Set()}
        journalService={mockJournalService}
        refreshKey={0}
        onExpandedNodesChange={jest.fn()}
        onSelect={onSelect}
      />,
    );

    const node = await screen.findByText('draft.txt');
    fireEvent.click(node);

    expect(onSelect).toHaveBeenCalledWith({
      path: ['*state*', 'draft.txt'],
      type: 'file',
    });
  });
});
