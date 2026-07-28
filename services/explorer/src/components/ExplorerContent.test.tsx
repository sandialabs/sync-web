import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import ExplorerContent from './ExplorerContent';
import { JournalService } from '../services/JournalService';

jest.mock('../services/JournalService', () => ({
  JournalService: {
    extractSchemeValue: jest.fn((value) => ({ value, schemeType: null })),
    parseDirectoryResponse: jest.fn(),
    parseDirectoryEntries: jest.fn(),
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
      const value = error as { code?: unknown; message?: unknown };
      const message = typeof value?.message === 'string'
        ? value.message.replace(/^bridge-error: /, '')
        : value?.message;
      return value?.code === 'index-error'
        || (value?.code === 'bridge-error' && typeof message === 'string'
          && message.startsWith('Bridge is not committed at the selected local index:'));
    }),
    decodePathSegment: jest.fn((value: string) => value.replace(/%20/g, ' ')),
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
  } as unknown as JournalService;

  beforeEach(() => {
    jest.clearAllMocks();
    URL.createObjectURL = jest.fn(() => 'blob:test');
    URL.revokeObjectURL = jest.fn();
    (JournalService.extractSchemeValue as jest.Mock).mockImplementation((value) => ({ value, schemeType: null }));
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue(null);
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue(null);
    (JournalService.isIndexError as jest.Mock).mockImplementation((error: unknown) =>
      typeof error === 'object' && error !== null
        && 'code' in error && (error as { code?: unknown }).code === 'index-error');
    (JournalService.isSnapshotUnavailable as jest.Mock).mockImplementation((error: unknown) => {
      const value = error as { code?: unknown; message?: unknown };
      const message = typeof value?.message === 'string'
        ? value.message.replace(/^bridge-error: /, '')
        : value?.message;
      return value?.code === 'index-error'
        || (value?.code === 'bridge-error' && typeof message === 'string'
          && message.startsWith('Bridge is not committed at the selected local index:'));
    });
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

    expect(await screen.findByText('+ File')).toBeInTheDocument();
    expect(screen.getByText('+ Directory')).toBeInTheDocument();
    expect(screen.getByText('Upload File')).toBeInTheDocument();
  });

  it('shows exact stored text in a view-only Raw mode', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '3c7363726970743e616c6572742831293c2f7363726970743e' },
      proof: {},
    });

    const { unmount } = render(
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

    fireEvent.click(await screen.findByText('Raw'));
    expect(await screen.findByText('<script>alert(1)</script>')).toBeInTheDocument();
    expect(screen.queryByText('Edit')).not.toBeInTheDocument();
    expect(screen.queryByText('Delete')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Rename')).not.toBeInTheDocument();
    expect(mockJournalService.get).toHaveBeenCalledWith(
      ['*state*', 'alice', 'page.html'],
    );
    unmount();
    expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:test');
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
  });

  it('does not present an unretained historical route as an access denial', async () => {
    (mockJournalService.get as jest.Mock).mockRejectedValue(
      Object.assign(
        new Error('bridge-error: Bridge is not committed at the selected local index: journal-1 -1'),
        { code: 'bridge-error' },
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

  it('hides mutation controls for a remote public stage directory', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['directory', { 'key-0': 'value' }, false],
      'pinned?': false,
      proof: {},
    });
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue({
      items: ['key-0'],
      isComplete: false,
    });
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue([
      { name: 'key-0', type: 'value' },
    ]);

    render(
      <ExplorerContent
        mode="stage"
        stageReadOnly
        selection={{ path: ['*state*', 'admin', 'data', 'public'], type: 'directory' }}
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

    expect(await screen.findByText('key-0')).toBeInTheDocument();
    expect(screen.queryByText('+ Document')).not.toBeInTheDocument();
    expect(screen.queryByText('+ Directory')).not.toBeInTheDocument();
    expect(screen.queryByText('Upload File')).not.toBeInTheDocument();
    expect(screen.queryByTitle('Rename')).not.toBeInTheDocument();
    expect(screen.queryByText('Delete')).not.toBeInTheDocument();
  });

  it('shows stage file actions', async () => {
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
        selection={{ path: ['*state*', 'draft.txt'], type: 'file' }}
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
    expect(screen.getByText('Download')).toBeInTheDocument();
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
    expect(screen.queryByText('+ Document')).not.toBeInTheDocument();
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
    const textarea = await screen.findByRole('textbox');
    fireEvent.change(textarea, { target: { value: 'manual unsaved edit' } });

    rerender(<ExplorerContent {...baseProps} refreshKey={1} />);

    await waitFor(() => expect((mockJournalService.get as jest.Mock).mock.calls.length).toBeGreaterThanOrEqual(2));
    expect(screen.getByRole('textbox')).toHaveValue('manual unsaved edit');
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

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [42, '*state*', 'peer.txt'], type: 'file' }}
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

    expect(await screen.findByText('Proof')).toBeInTheDocument();
    expect(screen.getByText('Download')).toBeInTheDocument();
    expect(await screen.findByText('Unpin')).toBeInTheDocument();
  });

  it('shows bridged file as unpinned or pinned based on resolve response', async () => {
    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '7065657220636f6e74656e74' },
      'pinned?': true,
      proof: { hash: 'abc' },
    });

    render(
      <ExplorerContent
        mode="ledger"
        selection={{ path: [-1, 'journal-5', -1, '*state*', 'admin', 'data', 'key-0'], type: 'file' }}
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

    expect(await screen.findByText('Unpin')).toBeInTheDocument();
  });
});
