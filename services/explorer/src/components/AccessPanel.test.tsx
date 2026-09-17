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

const setUserPrincipal = (journal: string, user: string) => {
  fireEvent.change(screen.getByLabelText('Principal kind'), {
    target: { value: journal ? 'remote' : 'local' },
  });
  if (journal) {
    fireEvent.change(screen.getByLabelText('Journal location'), { target: { value: journal } });
  }
  fireEvent.change(screen.getByLabelText('User'), { target: { value: user } });
};

const setPublicPrincipal = () => {
  fireEvent.change(screen.getByLabelText('Principal kind'), { target: { value: 'public' } });
};

describe('AccessPanel', () => {
  beforeEach(() => {
    jest.restoreAllMocks();
  });


  it('shows exactly conditional principal fields and preserves exact durable shapes', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');

    expect(screen.getByRole('option', { name: 'Local user' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'Remote user' })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: 'Public' })).toBeInTheDocument();
    expect(screen.queryByLabelText('Journal location')).not.toBeInTheDocument();
    expect(screen.getByLabelText('User')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Principal kind'), { target: { value: 'remote' } });
    expect(screen.getByLabelText('Journal location')).toBeInTheDocument();
    expect(screen.getByLabelText('User')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Principal kind'), { target: { value: 'public' } });
    expect(screen.queryByLabelText('Journal location')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('User')).not.toBeInTheDocument();
  });

  it('orders progressive permission pills without Scheme punctuation', async () => {
    const journalService = createJournalService();
    const { container } = render(
      <AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />,
    );
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');

    expect(Array.from(container.querySelectorAll('.access-permission-pill > .access-switch-row'))
      .map((node) => node.textContent?.trim())).toEqual(['put', 'use', 'retrieve', 'run']);
    expect(screen.queryByLabelText('read-only')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('index start')).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('use'));
    expect(screen.getByLabelText('read-only')).not.toBeChecked();
    fireEvent.click(screen.getByLabelText('retrieve'));
    expect(screen.getByLabelText('index start')).toHaveValue(0);
    expect(screen.getByLabelText('index end')).toHaveValue(-1);
    expect(container.textContent).not.toMatch(/(?:put|use|run)!/);
  });

  it('confirms an exact home-directory rule before success and clearing', async () => {
    const stored: any[] = [];
    const getAuthorizations = jest.fn().mockImplementation(async () => [...stored]);
    const authorize = jest.fn().mockImplementation(async (_namespace, rule) => {
      stored.push(rule);
      return true;
    });
    const journalService = createJournalService({ getAuthorizations, authorize } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setUserPrincipal('', 'bob');
    fireEvent.change(screen.getByLabelText('Path under your namespace'), { target: { value: '   ' } });
    fireEvent.click(screen.getByLabelText('run'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    expect(await screen.findByText('Access rule added.')).toBeInTheDocument();
    expect(authorize).toHaveBeenCalledWith(['*state*', 'alice'], expect.objectContaining({
      principal: ['*state*', 'bob'], path: [], 'use!': false, 'run!': true,
    }));
    expect(screen.getByText('(home)')).toBeInTheDocument();
    expect(screen.getByLabelText('User')).toHaveValue('');
  });

  it('preserves actionable input and rejects false or absent postconditions', async () => {
    const journalService = createJournalService({
      authorize: jest.fn().mockResolvedValue(false),
    } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setUserPrincipal('', 'bob');
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    expect(await screen.findByText('Access rule was not added; Journal reported no change.')).toBeInTheDocument();
    expect(screen.getByLabelText('User')).toHaveValue('bob');
    expect(screen.queryByText('Access rule added.')).not.toBeInTheDocument();
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
        principal: ['*state*', 'bob'], path: ['docs'], 'use!': { 'read-only?': true }, 'put!': false, retrieve: false,
      }])
      .mockReturnValueOnce(pendingRefresh);
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    const { rerender } = render(
      <AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />,
    );

    expect(await screen.findByText('User: bob')).toBeInTheDocument();
    rerender(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={1} />);

    expect(screen.getByText('User: bob')).toBeInTheDocument();
    expect(screen.queryByText('Loading access rules…')).not.toBeInTheDocument();
    finishRefresh?.([]);
  });

  it('preserves and displays the complete remote rule when deleting it', async () => {
    const rule = {
      principal: ['peer', '*state*', 'bob'], 'key-index': [-32, -1] as [number, number],
      path: ['a%2520b', 'caf%C3%A9'], 'use!': { 'read-only?': true }, 'put!': true,
      retrieve: [0, -1] as [number, number], 'run!': true,
    };
    const journalService = createJournalService({
      getAuthorizations: jest.fn().mockResolvedValue([rule]),
    } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    expect(await screen.findByText('Journal: peer')).toBeInTheDocument();
    expect(screen.getByText('User: bob')).toBeInTheDocument();
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    expect(screen.getByText('retrieve · 0 … -1')).toBeInTheDocument();
    expect(screen.getByText('a%20b / café')).toBeInTheDocument();
    expect(Array.from(document.querySelectorAll('.access-rule-permissions span'))
      .map((node) => node.textContent)).toEqual([
        'put', 'use · read-only', 'retrieve · 0 … -1', 'run',
      ]);
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Remove access rule for Remote user bob at peer at a%20b / café?',
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
    expect(screen.queryByLabelText('index start')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('index end')).not.toBeInTheDocument();
    setUserPrincipal('peer', 'bob');
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('retrieve'));
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(journalService.authorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      {
        principal: ['peer', '*state*', 'bob'],
        'key-index': [-32, -1],
        path: [],
        'put!': false,
        'use!': { 'read-only?': true },
        'run!': false,
        retrieve: [0, -1],
      },
    ));
    expect(screen.getByLabelText('index start')).toHaveValue(0);
    expect(screen.getByLabelText('index end')).toHaveValue(-1);
    setUserPrincipal('other', 'carol');
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
  });

  it('adds, reloads, displays, and removes an exact public-principal rule', async () => {
    const storedRules: any[] = [];
    const getAuthorizations = jest.fn().mockImplementation(async () => [...storedRules]);
    const authorize = jest.fn().mockImplementation(async (_namespace, rule) => {
      storedRules.push(rule);
      return true;
    });
    const deauthorize = jest.fn().mockImplementation(async (_namespace, rule) => {
      const index = storedRules.findIndex((stored) => JSON.stringify(stored) === JSON.stringify(rule));
      if (index >= 0) storedRules.splice(index, 1);
      return true;
    });
    const journalService = createJournalService({
      getAuthorizations, authorize, deauthorize,
    } as Partial<JournalService>);

    const first = render(
      <AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />,
    );
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setPublicPrincipal();
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByRole('button', { name: 'Delete' })).toBeInTheDocument();
    expect(screen.getByText('Public', { selector: '.access-rule-principal' })).toBeInTheDocument();
    expect(authorize).toHaveBeenCalledWith(['*state*', 'alice'], expect.objectContaining({
      principal: ['*public*'], 'use!': { 'read-only?': true },
    }));

    first.unmount();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={1} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(deauthorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      expect.objectContaining({ principal: ['*public*'] }),
    ));
    expect(await screen.findByText(
      'No explicit access rules yet. This namespace is private except to its owner and admins.',
    )).toBeInTheDocument();
  });

  it('never exposes Authentication window fields while editing any principal shape', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setUserPrincipal('peer archive', 'bob');
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    setPublicPrincipal();
    expect(screen.queryByLabelText('Journal location')).not.toBeInTheDocument();
    expect(screen.queryByLabelText('User')).not.toBeInTheDocument();
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
  });

  it('rejects a malformed principal clearly without clearing the input', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setUserPrincipal('peer *state*', '');
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    expect(await screen.findByText(
      'Journal location must contain only route aliases, and User must be one local username.',
    )).toBeInTheDocument();
    expect(screen.getByLabelText('Journal location')).toHaveValue('peer *state*');
    expect(screen.getByLabelText('User')).toHaveValue('');
    expect(journalService.authorize).not.toHaveBeenCalled();
  });

  it.each([
    ['local', '', 'alice bob'],
    ['remote', 'peer archive', 'bob carol'],
  ])('rejects a multi-token %s User without clearing the input', async (_kind, journal, user) => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setUserPrincipal(journal, user);
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    expect(await screen.findByText(
      'Journal location must contain only route aliases, and User must be one local username.',
    )).toBeInTheDocument();
    if (journal) expect(screen.getByLabelText('Journal location')).toHaveValue(journal);
    else expect(screen.queryByLabelText('Journal location')).not.toBeInTheDocument();
    expect(screen.getByLabelText('User')).toHaveValue(user);
    expect(journalService.authorize).not.toHaveBeenCalled();
  });

  it('recognizes a multi-hop remote principal and preserves its exact route', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setUserPrincipal('peer archive', 'bob');
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(journalService.authorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      expect.objectContaining({
        principal: ['peer', 'archive', '*state*', 'bob'],
        'key-index': [-32, -1],
      }),
    ));
  });

  it.each(['local', 'public'])('omits key-index for an exact %s principal', async (kind) => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    if (kind === 'public') setPublicPrincipal();
    else setUserPrincipal('', 'bob');
    expect(screen.queryByText(/Authentication window/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => expect(journalService.authorize).toHaveBeenCalled());
    expect(journalService.authorize).toHaveBeenCalledWith(
      ['*state*', 'alice'],
      expect.not.objectContaining({ 'key-index': expect.anything() }),
    );
  });

  it('rejects a malformed retrieve range before submission', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');
    setUserPrincipal('peer', 'bob');
    fireEvent.click(screen.getByLabelText('retrieve'));
    fireEvent.change(screen.getByLabelText('index start'), { target: { value: '-2' } });
    fireEvent.change(screen.getByLabelText('index end'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(await screen.findByText('Retrieve window cannot use a relative start with an absolute end.'))
      .toBeInTheDocument();
    expect(journalService.authorize).not.toHaveBeenCalled();
  });

  it('does not delete a rule when confirmation is cancelled', async () => {
    const rule = {
      principal: ['*state*', 'bob'], path: ['private%20documents'], 'use!': { 'read-only?': true },
      'put!': false, retrieve: false,
    };
    const journalService = createJournalService({
      getAuthorizations: jest.fn().mockResolvedValue([rule]),
    } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    expect(await screen.findByText('private documents')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Remove access rule for Local user bob at private documents?',
    );
    expect(screen.getByRole('button', { name: 'Add' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(journalService.deauthorize).not.toHaveBeenCalled();
  });

  it('refuses stale Access deletion after rules refresh during confirmation', async () => {
    const rule = {
      principal: ['*state*', 'bob'], path: ['private'], 'use!': { 'read-only?': true }, 'put!': false, retrieve: false,
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

    setUserPrincipal('', 'bob');
    fireEvent.change(screen.getByLabelText('Path under your namespace'), {
      target: { value: 'data a%20b "private documents" "50% café" "say \\"hi\\"" "back\\\\slash"' },
    });
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
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
    setUserPrincipal('', 'bob');
    fireEvent.change(screen.getByLabelText('Path under your namespace'), { target: { value: path } });
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
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
        principal: ['*state*', 'carol'], path: ['current'], 'use!': { 'read-only?': true }, 'put!': false, retrieve: false,
      }]);
      return Promise.resolve([]);
    });
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);
    await screen.findByText('No explicit access rules yet. This namespace is private except to its owner and admins.');

    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'bob' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'carol' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(await screen.findByText('current')).toBeInTheDocument();
    await act(async () => resolveBob([{
      principal: ['*state*', 'bob'], path: ['stale'], 'use!': { 'read-only?': true }, 'put!': false, retrieve: false,
    }]));
    expect(screen.queryByText('stale')).not.toBeInTheDocument();
    expect(screen.getAllByText('carol')).toHaveLength(2);
  });

  it('ignores a stale failed namespace load while retaining the active target', async () => {
    let rejectBob: (error: Error) => void = () => undefined;
    const bob = new Promise<any[]>((_resolve, reject) => { rejectBob = reject; });
    const getAuthorizations = jest.fn().mockImplementation((namespace: string[]) => {
      if (namespace[1] === 'admin') return Promise.resolve([{
        principal: ['*state*', 'admin'], path: ['old'], 'use!': { 'read-only?': true }, 'put!': false, retrieve: false,
      }]);
      if (namespace[1] === 'bob') return bob;
      return Promise.resolve([{
        principal: ['*state*', 'carol'], path: ['new'], 'use!': { 'read-only?': true }, 'put!': false, retrieve: false,
      }]);
    });
    const journalService = createJournalService({ getAuthorizations } as Partial<JournalService>);
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);
    expect(await screen.findByText('old')).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'bob' } });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(screen.getByText('old')).toBeInTheDocument();
    expect(screen.getAllByText('admin')).toHaveLength(2);
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
          principal: ['*state*', 'admin'], path: ['active'], 'use!': { 'read-only?': true }, 'put!': false, retrieve: false,
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
    expect(screen.getAllByText('admin')).toHaveLength(2);
  });

  it('rejects non-user managed namespace shapes without loading them', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);
    await screen.findByLabelText('Manage namespace');
    fireEvent.change(screen.getByLabelText('Manage namespace'), {
      target: { value: '*state* bob extra' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Load namespace' }));
    expect(await screen.findByText('Managed namespace must be one local username.'))
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

    setUserPrincipal('', 'carol');
    fireEvent.click(screen.getByLabelText('use'));
    fireEvent.click(screen.getByLabelText('read-only'));
    fireEvent.click(screen.getByRole('button', { name: 'Add' }));

    await waitFor(() => {
      expect(journalService.authorize).toHaveBeenCalledWith(
        ['*state*', 'bob'],
        expect.objectContaining({ principal: ['*state*', 'carol'], path: [], 'use!': { 'read-only?': true } }),
      );
    });
  });
});
