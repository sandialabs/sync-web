import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import AccessPanel from './AccessPanel';
import { JournalService } from '../services/JournalService';

const createJournalService = (overrides: Partial<JournalService> = {}) => ({
  getAuthorizations: jest.fn().mockResolvedValue([]),
  authorize: jest.fn().mockResolvedValue(true),
  deauthorize: jest.fn().mockResolvedValue(true),
  ...overrides,
}) as unknown as JournalService;

describe('AccessPanel', () => {
  beforeEach(() => {
    jest.restoreAllMocks();
  });

  it('uses the current user namespace for non-admin users', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await waitFor(() => {
      expect(journalService.getAuthorizations).toHaveBeenCalledWith(['*state*', 'alice']);
    });
    expect(screen.queryByLabelText('Manage namespace')).not.toBeInTheDocument();
  });

  it('keeps rendered rules mounted during background refreshes', async () => {
    let finishRefresh: ((value: unknown[]) => void) | undefined;
    const pendingRefresh = new Promise<unknown[]>((resolve) => { finishRefresh = resolve; });
    const getAuthorizations = jest.fn()
      .mockResolvedValueOnce([{
        principal: ['*state*', 'bob'], path: ['docs'], get: true, 'set!': false, resolve: false,
      }])
      .mockReturnValueOnce(pendingRefresh);
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    const { rerender } = render(
      <AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />,
    );

    expect(await screen.findByText('*state* bob')).toBeInTheDocument();
    rerender(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={1} />);

    expect(screen.getByText('*state* bob')).toBeInTheDocument();
    expect(screen.queryByText('Loading access rules…')).not.toBeInTheDocument();
    finishRefresh?.([]);
  });

  it('preserves and displays the complete remote rule when deleting it', async () => {
    const rule = {
      principal: ['peer', '*state*', 'bob'], 'key-index': [-32, -1] as [number, number],
      path: ['a%2520b', 'caf%C3%A9'], get: true, 'set!': true,
      resolve: [0, -1] as [number, number],
    };
    const journalService = createJournalService({
      getAuthorizations: jest.fn().mockResolvedValue([rule]),
    } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    expect(await screen.findByText('peer *state* bob')).toBeInTheDocument();
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    expect(screen.getByText('Document history: 0 … -1')).toBeInTheDocument();
    expect(screen.getByText('a%20b / café')).toBeInTheDocument();
    expect(screen.getAllByText('set!')).toHaveLength(2);
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Remove access rule for peer *state* bob at a%20b / café?',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));

    await waitFor(() => {
      expect(journalService.deauthorize).toHaveBeenCalledWith(['*state*', 'alice'], rule);
    });
  });

  it('adds a direct remote rule with fixed hidden key-index and distinct document history', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    expect(screen.getByLabelText('Resolve Start')).toHaveValue(0);
    expect(screen.getByLabelText('Resolve End')).toHaveValue(-1);
    fireEvent.change(screen.getByLabelText('Share with principal'), {
      target: { value: 'peer *state* bob' },
    });
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('resolve').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByText('get').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(journalService.authorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      {
        principal: ['peer', '*state*', 'bob'],
        'key-index': [-32, -1],
        path: [],
        get: true,
        'set!': false,
        resolve: [0, -1],
      },
    ));
    expect(screen.getByLabelText('Resolve Start')).toHaveValue(0);
    expect(screen.getByLabelText('Resolve End')).toHaveValue(-1);
    fireEvent.change(screen.getByLabelText('Share with principal'), {
      target: { value: 'other *state* carol' },
    });
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
  });

  it('never exposes Authentication window fields while editing any principal shape', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    const input = screen.getByLabelText('Share with principal');
    for (const value of ['peer', 'peer *state*', '*state*', 'peer bob', '*public* extra']) {
      fireEvent.change(input, { target: { value } });
      expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    }
    fireEvent.change(input, { target: { value: 'peer *state* bob' } });
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
  });

  it('rejects a malformed principal clearly without clearing the input', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    fireEvent.change(screen.getByLabelText('Share with principal'), {
      target: { value: 'peer *state*' },
    });
    fireEvent.click(screen.getByText('get').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    expect(await screen.findByText(
      'Share with must be exact “*state* USER”, “*public*”, or one or more route segments followed by “*state* USER”.',
    )).toBeInTheDocument();
    expect(screen.getByLabelText('Share with principal')).toHaveValue('peer *state*');
    expect(journalService.authorize).not.toHaveBeenCalled();
  });

  it('recognizes a multi-hop remote principal and preserves its exact route', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    fireEvent.change(screen.getByLabelText('Share with principal'), {
      target: { value: 'peer archive *state* bob' },
    });
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('get').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(journalService.authorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      expect.objectContaining({
        principal: ['peer', 'archive', '*state*', 'bob'],
        'key-index': [-32, -1],
      }),
    ));
  });

  it.each([
    ['local', '*state* bob'],
    ['public', '*public*'],
  ])('omits key-index for an exact %s principal', async (_kind, principal) => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    fireEvent.change(screen.getByLabelText('Share with principal'), { target: { value: principal } });
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByText('get').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(journalService.authorize).toHaveBeenCalled());
    expect(journalService.authorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      expect.not.objectContaining({ 'key-index': expect.anything() }),
    );
  });

  it('rejects a malformed resolve range before submission', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    fireEvent.change(screen.getByLabelText('Share with principal'), {
      target: { value: 'peer *state* bob' },
    });
    fireEvent.click(screen.getByText('resolve').querySelector('input') as HTMLInputElement);
    fireEvent.change(screen.getByLabelText('Resolve Start'), { target: { value: '-2' } });
    fireEvent.change(screen.getByLabelText('Resolve End'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText('Resolve window cannot use a relative start with an absolute end.'))
      .toBeInTheDocument();
    expect(journalService.authorize).not.toHaveBeenCalled();
  });

  it('does not delete a rule when confirmation is cancelled', async () => {
    const rule = {
      principal: ['*state*', 'bob'], path: ['private%20documents'], get: true,
      'set!': false, resolve: false,
    };
    const journalService = createJournalService({
      getAuthorizations: jest.fn().mockResolvedValue([rule]),
    } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    expect(await screen.findByText('private documents')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Remove access rule for *state* bob at private documents?',
    );
    expect(screen.getByRole('button', { name: 'Add' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(journalService.deauthorize).not.toHaveBeenCalled();
  });

  it('refuses stale Access deletion after rules refresh during confirmation', async () => {
    const rule = {
      principal: ['*state*', 'bob'], path: ['private'], get: true, 'set!': false, resolve: false,
    };
    const getAuthorizations = jest.fn()
      .mockResolvedValueOnce([rule])
      .mockResolvedValueOnce([]);
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    const { rerender } = render(
      <AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />,
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    await screen.findByRole('dialog');
    rerender(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={1} />);
    await waitFor(() => expect(getAuthorizations).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText(
      'Access rules changed during confirmation; review the current rule before deleting it.',
    )).toBeInTheDocument();
    expect(journalService.deauthorize).not.toHaveBeenCalled();
  });

  it('encodes quoted human path segments exactly once while preserving simple paths', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');

    fireEvent.change(screen.getByLabelText('Share with principal'), { target: { value: '*state* bob' } });
    fireEvent.change(screen.getByLabelText('Path under your namespace'), {
      target: { value: 'data a%20b "private documents" "50% café" "say \\"hi\\"" "back\\\\slash"' },
    });
    fireEvent.click(screen.getByText('get').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(journalService.authorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      expect.objectContaining({
        path: ['data', 'a%2520b', 'private%20documents', '%350%25%20caf%C3%A9', 'say%20%22hi%22', 'back%5Cslash'],
      }),
    ));
  });

  it.each([
    ['"unclosed', 'Path has an unclosed quote.'],
    ['value\\', 'Path cannot end with an incomplete backslash escape.'],
    ['value\\q', 'Path backslash escapes support only whitespace, quote, or backslash.'],
    ['"two"joined', 'A closing quote must be followed by whitespace.'],
  ])('rejects malformed quoted path input %s', async (path, message) => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    fireEvent.change(screen.getByLabelText('Share with principal'), { target: { value: '*state* bob' } });
    fireEvent.change(screen.getByLabelText('Path under your namespace'), { target: { value: path } });
    fireEvent.click(screen.getByText('get').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText(message)).toBeInTheDocument();
    expect(journalService.authorize).not.toHaveBeenCalled();
  });

  it('applies only the newest admin namespace load when successes complete out of order', async () => {
    let resolveBob: (rules: any[]) => void = () => undefined;
    const bob = new Promise<any[]>((resolve) => { resolveBob = resolve; });
    const getAuthorizations = jest.fn().mockImplementation((namespace: string[]) => {
      if (namespace[1] === 'bob') return bob;
      if (namespace[1] === 'carol') return Promise.resolve([{
        principal: ['*state*', 'carol'], path: ['current'], get: true, 'set!': false, resolve: false,
      }]);
      return Promise.resolve([]);
    });
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');

    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'bob' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: '*state* carol' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(await screen.findByText('current')).toBeInTheDocument();
    await act(async () => resolveBob([{
      principal: ['*state*', 'bob'], path: ['stale'], get: true, 'set!': false, resolve: false,
    }]));
    expect(screen.queryByText('stale')).not.toBeInTheDocument();
    expect(screen.getAllByText('(*state* carol)')).toHaveLength(2);
  });

  it('ignores a stale failed namespace load while retaining the active target', async () => {
    let rejectBob: (error: Error) => void = () => undefined;
    const bob = new Promise<any[]>((_resolve, reject) => { rejectBob = reject; });
    const getAuthorizations = jest.fn().mockImplementation((namespace: string[]) => {
      if (namespace[1] === 'admin') return Promise.resolve([{
        principal: ['*state*', 'admin'], path: ['old'], get: true, 'set!': false, resolve: false,
      }]);
      if (namespace[1] === 'bob') return bob;
      return Promise.resolve([{
        principal: ['*state*', 'carol'], path: ['new'], get: true, 'set!': false, resolve: false,
      }]);
    });
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);
    expect(await screen.findByText('old')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'bob' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(screen.getByText('old')).toBeInTheDocument();
    expect(screen.getAllByText('(*state* admin)')).toHaveLength(2);
    expect(screen.getByRole('button', { name: 'Add' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeDisabled();
    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'carol' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(await screen.findByText('new')).toBeInTheDocument();
    await act(async () => rejectBob(new Error('stale failure')));
    expect(screen.queryByText('stale failure')).not.toBeInTheDocument();
    expect(screen.getByText('new')).toBeInTheDocument();
  });

  it('keeps active rules and identity after the newest target load fails', async () => {
    let rejectBob: (error: Error) => void = () => undefined;
    const bob = new Promise<any[]>((_resolve, reject) => { rejectBob = reject; });
    const getAuthorizations = jest.fn().mockImplementation((namespace: string[]) => (
      namespace[1] === 'bob'
        ? bob
        : Promise.resolve([{
          principal: ['*state*', 'admin'], path: ['active'], get: true, 'set!': false, resolve: false,
        }])
    ));
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);
    expect(await screen.findByText('active')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'bob' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(screen.getByRole('button', { name: 'Add' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(journalService.authorize).not.toHaveBeenCalled();
    expect(journalService.deauthorize).not.toHaveBeenCalled();
    await act(async () => rejectBob(new Error('target unavailable')));
    expect(await screen.findByText('target unavailable')).toBeInTheDocument();
    expect(screen.getByText('active')).toBeInTheDocument();
    expect(screen.getAllByText('(*state* admin)')).toHaveLength(2);
  });

  it('rejects non-user managed namespace shapes without loading them', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);
    await screen.findByLabelText('Manage namespace');
    fireEvent.change(screen.getByLabelText('Manage namespace'), {
      target: { value: '*state* bob extra' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(await screen.findByText('Managed namespace must be a username or exact “*state* USER”.'))
      .toBeInTheDocument();
    expect(journalService.getAuthorizations).toHaveBeenCalledTimes(1);
  });

  it('lets admins manage another local namespace', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);

    await screen.findByLabelText('Manage namespace');
    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'bob' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));

    await waitFor(() => {
      expect(journalService.getAuthorizations).toHaveBeenLastCalledWith(['*state*', 'bob']);
      expect(screen.getByRole('button', { name: 'Add' })).toBeEnabled();
    });

    fireEvent.change(screen.getByLabelText('Share with principal'), { target: { value: '*state* carol' } });
    fireEvent.click(screen.getByText('get').querySelector('input') as HTMLInputElement);
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => {
      expect(journalService.authorize).toHaveBeenCalledWith(
        ['*state*', 'bob'],
        expect.objectContaining({ principal: ['*state*', 'carol'], path: [], get: true }),
      );
    });
  });
});
