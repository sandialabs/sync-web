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
    (JournalService.extractSchemeValue as jest.Mock).mockImplementation((value) => ({ value, schemeType: null }));
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue(null);
    (JournalService.parseDirectoryEntries as jest.Mock).mockReturnValue(null);
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

    expect(await screen.findByText('+ Document')).toBeInTheDocument();
    expect(screen.getByText('+ Directory')).toBeInTheDocument();
    expect(screen.getByText('Upload File')).toBeInTheDocument();
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
        selection={{ path: [-1, '*bridge*', 'journal-5', -1, '*state*', 'admin', 'data', 'key-0'], type: 'file' }}
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
