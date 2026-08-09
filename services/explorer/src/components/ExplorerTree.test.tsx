import React from 'react';
import { fireEvent, render, screen, waitForElementToBeRemoved } from '@testing-library/react';
import ExplorerTree from './ExplorerTree';
import { JournalService } from '../services/JournalService';

describe('ExplorerTree', () => {
  const mockJournalService = {
    getDirectoryEntries: jest.fn(),
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
    expect(labels).toEqual(['▣*state*', '▣bob', '▣alice', '▣zara']);
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
        rootPath={[-1, 'peer', -1, '*state*', 'admin']}
        selected={null}
        expandedNodes={new Set(['ledger/data'])}
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
      [-1, 'peer', -1, '*state*', 'admin', 'data'],
    );
  });

  it('shows request failures as a stable access error instead of a tree node', async () => {
    (mockJournalService.getDirectoryEntries as jest.Mock).mockRejectedValue(
      new Error('journal_error: raw authorization details'),
    );

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[-1, 'peer', -1, '*state*', 'alice']}
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
        rootPath={[257, 'journal-1', -2, '*state*']}
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
        new Error('bridge-error: Bridge is not committed at the selected local index: journal-1 -1'),
        { code: 'bridge-error' },
      ),
    );

    render(
      <ExplorerTree
        mode="ledger"
        rootPath={[257, 'journal-1', -2, '*state*']}
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
        rootPath={[-1, '*state*', 'alice']}
        selected={null}
        expandedNodes={new Set(['ledger/denied'])}
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

    const root = await screen.findByText('*state*');
    const alice = await screen.findByText('alice');
    expect(root.closest('.tree-node-content')).toHaveClass('selected');
    expect(root.closest('.tree-root-node')).toContainElement(alice);
    fireEvent.click(root);
    expect(onSelect).toHaveBeenCalledWith({ path: ['*state*'], type: 'directory' });
  });

  it('selects the exact routed Ledger namespace root including snapshot prefixes', async () => {
    const onSelect = jest.fn();
    const rootPath = [42, 'journal-1', -3, '*state*'];
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

    fireEvent.click(await screen.findByText('*state*'));
    expect(onSelect).toHaveBeenCalledWith({ path: rootPath, type: 'directory' });
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
