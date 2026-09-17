import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import AdminPanel from './AdminPanel';
import { JournalService } from '../services/JournalService';

const createJournalService = (overrides: Partial<JournalService> = {}) => ({
  getAdminConfig: jest.fn().mockResolvedValue({
    admins: ['admin'],
    bridges: [],
    localName: 'journal-0',
    localEndpoint: 'http://journal-0/interface',
    windowSize: 4,
    bridgeAccept: 'auto',
    bridgePreapprovals: {},
  }),
  saveBridge: jest.fn().mockResolvedValue(true),
  deleteBridge: jest.fn().mockResolvedValue(true),
  updateConfig: jest.fn().mockResolvedValue(true),
  setAdmins: jest.fn().mockResolvedValue(true),
  setWindowSize: jest.fn().mockResolvedValue(true),
  ...overrides,
}) as unknown as JournalService;

describe('AdminPanel', () => {
  beforeEach(() => {
    jest.restoreAllMocks();
  });

  it('shows composed journal identity and truthful bridge metadata read-only', async () => {
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue({
        admins: ['admin'],
        bridges: [{ name: 'mystery', endpoint: 'https://peer/interface' }],
        localName: 'journal-0', localEndpoint: 'http://journal-0/interface',
        windowSize: 4, bridgeAccept: 'auto', bridgePreapprovals: {},
      }),
    } as Partial<JournalService>);
    const { container } = render(
      <AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />,
    );

    const identity = await screen.findByLabelText('Local journal identity');
    expect(identity).toHaveTextContent('Journal name');
    await waitFor(() => expect(identity).toHaveTextContent('journal-0'));
    expect(identity).toHaveTextContent('http://journal-0/interface');
    expect(identity.querySelector('input')).toBeNull();
    const bridge = screen.getByText('mystery').closest('.bridge-card');
    expect(bridge).toHaveTextContent('https://peer/interface');
    expect(bridge).toHaveTextContent('Name at peerNot reported');
    expect(bridge).toHaveTextContent('Direction not reported');
    expect(container.textContent).not.toContain('Synchronizes here');
  });

  it('prefills the reciprocal name and creates a bridge', async () => {
    const journalService = createJournalService();
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    const bridgeNameInput = await screen.findByPlaceholderText('peer');
    fireEvent.change(bridgeNameInput, { target: { value: 'beagle' } });
    fireEvent.change(screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface'), {
      target: { value: 'https://beagle.sync-web.org/api/v1/journal/interface' },
    });

    await waitFor(() => expect(
      screen.getByPlaceholderText('Name peer uses for this journal'),
    ).toHaveValue('journal-0'));
    fireEvent.click(screen.getByRole('button', { name: 'Add bridge' }));

    await waitFor(() => expect(journalService.saveBridge).toHaveBeenCalledWith({
      name: 'beagle',
      endpoint: 'https://beagle.sync-web.org/api/v1/journal/interface',
      remoteName: 'journal-0',
    }));
  });

  it('decodes the local name for display, reciprocal prefill, and cleared fallback', async () => {
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue({
        admins: ['admin'], bridges: [], localName: 'local%20journal',
        localEndpoint: 'http://local/interface', windowSize: 4,
        bridgeAccept: 'auto', bridgePreapprovals: {},
      }),
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    const identity = await screen.findByLabelText('Local journal identity');
    await waitFor(() => expect(identity).toHaveTextContent('local journal'));
    expect(identity).not.toHaveTextContent('local%20journal');
    const remoteName = screen.getByPlaceholderText('Name peer uses for this journal');
    await waitFor(() => expect(remoteName).toHaveValue('local journal'));

    fireEvent.change(screen.getByPlaceholderText('peer'), { target: { value: 'beagle' } });
    fireEvent.change(screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface'), {
      target: { value: 'https://beagle/interface' },
    });
    fireEvent.change(remoteName, { target: { value: '' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add bridge' }));

    await waitFor(() => expect(journalService.saveBridge).toHaveBeenCalledWith({
      name: 'beagle',
      endpoint: 'https://beagle/interface',
      remoteName: 'local journal',
    }));
  });

  it('shows and saves an allowed bridge when preapproval is required', async () => {
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue({
        admins: ['admin'],
        bridges: [],
        localName: 'journal-0',
        localEndpoint: 'http://journal-0/interface',
        windowSize: 4,
        bridgeAccept: 'preapproved',
        bridgePreapprovals: {},
      }),
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    const signingKeyHash = 'ab'.repeat(32);
    fireEvent.change(await screen.findByPlaceholderText('64 hexadecimal characters'), {
      target: { value: signingKeyHash },
    });
    const peerInputs = screen.getAllByPlaceholderText('peer');
    fireEvent.change(peerInputs[peerInputs.length - 1], { target: { value: 'beagle' } });
    fireEvent.click(screen.getByRole('button', { name: 'Allow bridge' }));

    await waitFor(() => expect(journalService.updateConfig).toHaveBeenCalledWith(
      ['private', 'bridge-preapproval', 'beagle'],
      { '*type/byte-vector*': signingKeyHash },
    ));
  });

  it('cancels destructive bridge and preapproval removals without requests', async () => {
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue({
        admins: ['admin'],
        bridges: [{
          name: 'beagle', endpoint: 'https://beagle/interface', remoteName: 'journal-0',
          initiation: 'local',
        }],
        localName: 'journal-0', localEndpoint: 'http://journal-0/interface', windowSize: 4,
        bridgeAccept: 'preapproved', bridgePreapprovals: { incoming: { '*type/byte-vector*': 'ab'.repeat(32) } },
      }),
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Delete bridge beagle? This removes its bridge configuration and cascades deletion of alias-scoped authorization state.',
    );
    expect(screen.getByRole('button', { name: 'Cancel' })).toHaveFocus();
    expect(screen.getByRole('radio', { name: /Require preapproval/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Update Window' })).toBeDisabled();
    fireEvent.click(screen.getByRole('radio', { name: /Require preapproval/ }));
    expect(journalService.updateConfig).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    const removePreapproval = screen.getAllByRole('button', { name: 'Remove' })
      .find((button) => !button.hasAttribute('disabled'));
    if (!removePreapproval) throw new Error('Preapproval remove button not found');
    fireEvent.click(removePreapproval);
    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Remove incoming bridge preapproval for incoming?',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(journalService.deleteBridge).not.toHaveBeenCalled();
    expect(journalService.updateConfig).not.toHaveBeenCalled();
  });

  it('runs confirmed bridge, preapproval, and retention-window mutations', async () => {
    const config = {
      admins: ['admin'],
      bridges: [{ name: 'beagle', endpoint: 'https://beagle/interface', initiation: 'local' as const }],
      localName: 'journal-0', localEndpoint: 'http://journal-0/interface', windowSize: 4,
      bridgeAccept: 'preapproved' as const,
      bridgePreapprovals: { incoming: { '*type/byte-vector*': 'ab'.repeat(32) } },
    };
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue(config),
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    fireEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(journalService.deleteBridge).toHaveBeenCalledWith('beagle'));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Delete' })).toBeEnabled());

    const preapprovalRemove = screen.getAllByRole('button', { name: 'Remove' })
      .find((button) => !button.hasAttribute('disabled'));
    if (!preapprovalRemove) throw new Error('Preapproval remove button not found');
    fireEvent.click(preapprovalRemove);
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(journalService.updateConfig).toHaveBeenCalledWith(
      ['private', 'bridge-preapproval', 'incoming'], [],
    ));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Update Window' })).toBeEnabled());

    const windowInput = screen.getByRole('spinbutton');
    fireEvent.change(windowInput, { target: { value: '2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Update Window' }));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(journalService.setWindowSize).toHaveBeenCalledWith(2));
  });

  it('refuses bridge deletion when authoritative state changes during confirmation', async () => {
    const base = {
      admins: ['admin'], localName: 'journal-0', localEndpoint: 'http://journal-0/interface',
      windowSize: 4, bridgeAccept: 'auto' as const, bridgePreapprovals: {},
    };
    const getAdminConfig = jest.fn()
      .mockResolvedValueOnce({
        ...base,
        bridges: [{ name: 'beagle', endpoint: 'https://old/interface', initiation: 'local' }],
      })
      .mockResolvedValue({
        ...base,
        bridges: [{ name: 'beagle', endpoint: 'https://new/interface', initiation: 'local' }],
      });
    const journalService = createJournalService({ getAdminConfig } as Partial<JournalService>);
    const { rerender } = render(
      <AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />,
    );
    fireEvent.click(await screen.findByRole('button', { name: 'Delete' }));
    await screen.findByRole('dialog');
    rerender(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={1} />);
    await waitFor(() => expect(getAdminConfig).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText(
      'Bridge state changed during confirmation; review the current bridge before deleting it.',
    )).toBeInTheDocument();
    expect(journalService.deleteBridge).not.toHaveBeenCalled();
  });

  it('preserves every failed Admin form and the dirty window value', async () => {
    const denied = jest.fn().mockRejectedValue(new Error('denied'));
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue({
        admins: ['admin'], bridges: [], localName: 'journal-0',
        localEndpoint: 'http://journal-0/interface', windowSize: 4,
        bridgeAccept: 'preapproved', bridgePreapprovals: {},
      }),
      saveBridge: denied,
      updateConfig: denied,
      setAdmins: denied,
      setWindowSize: denied,
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);
    await screen.findByPlaceholderText('64 hexadecimal characters');

    const bridgeName = screen.getAllByPlaceholderText('peer')[0];
    const endpoint = screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface');
    fireEvent.change(bridgeName, { target: { value: 'beagle' } });
    fireEvent.change(endpoint, { target: { value: 'https://beagle/interface' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add bridge' }));
    await screen.findByText('denied');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Add bridge' })).toBeEnabled());
    expect(bridgeName).toHaveValue('beagle');
    expect(endpoint).toHaveValue('https://beagle/interface');

    const peers = screen.getAllByPlaceholderText('peer');
    const preapprovalName = peers[peers.length - 1];
    const key = screen.getByPlaceholderText('64 hexadecimal characters');
    fireEvent.change(preapprovalName, { target: { value: 'incoming' } });
    fireEvent.change(key, { target: { value: 'ab'.repeat(32) } });
    fireEvent.click(screen.getByRole('button', { name: 'Allow bridge' }));
    await waitFor(() => expect(journalService.updateConfig).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByRole('button', { name: 'Allow bridge' })).toBeEnabled());
    expect(preapprovalName).toHaveValue('incoming');
    expect(key).toHaveValue('ab'.repeat(32));

    const window = screen.getByRole('spinbutton');
    fireEvent.change(window, { target: { value: '12' } });
    fireEvent.click(screen.getByRole('button', { name: 'Update Window' }));
    await waitFor(() => expect(journalService.setWindowSize).toHaveBeenCalledWith(12));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Update Window' })).toBeEnabled());
    expect(window).toHaveValue(12);

    const admin = screen.getByPlaceholderText('Username');
    fireEvent.change(admin, { target: { value: 'alice' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add Admin' }));
    await waitFor(() => expect(journalService.setAdmins).toHaveBeenCalled());
    expect(admin).toHaveValue('alice');
  });

  it('replaces zero, one, and two-admin lists through add and remove actions', async () => {
    const config = (admins: string[]) => ({
      admins, bridges: [], localName: 'journal-0',
      localEndpoint: 'http://journal-0/interface', windowSize: 4,
      bridgeAccept: 'auto' as const, bridgePreapprovals: {},
    });
    const getAdminConfig = jest.fn()
      .mockResolvedValueOnce(config([]))
      .mockResolvedValueOnce(config(['alice']))
      .mockResolvedValueOnce(config(['alice', 'bob']))
      .mockResolvedValueOnce(config(['bob']));
    const setAdmins = jest.fn().mockResolvedValue(true);
    const journalService = createJournalService({ getAdminConfig, setAdmins } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="root" refreshKey={0} />);

    const input = await screen.findByPlaceholderText('Username');
    fireEvent.change(input, { target: { value: 'alice' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add Admin' }));
    await waitFor(() => expect(setAdmins).toHaveBeenLastCalledWith(['alice']));
    await screen.findByText('Added administrator alice.');

    fireEvent.change(input, { target: { value: 'bob' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add Admin' }));
    await waitFor(() => expect(setAdmins).toHaveBeenLastCalledWith(['alice', 'bob']));
    await screen.findByText('Added administrator bob.');

    fireEvent.click(screen.getAllByRole('button', { name: 'Remove' })[0]);
    await waitFor(() => expect(setAdmins).toHaveBeenLastCalledWith(['bob']));
  });

  it('confirms self-removal when another administrator remains', async () => {
    const config = (admins: string[]) => ({
      admins, bridges: [], localName: 'journal-0',
      localEndpoint: 'http://journal-0/interface', windowSize: 4,
      bridgeAccept: 'auto' as const, bridgePreapprovals: {},
    });
    const getAdminConfig = jest.fn()
      .mockResolvedValueOnce(config(['alice', 'bob']))
      .mockResolvedValueOnce(config(['bob']));
    const setAdmins = jest.fn().mockResolvedValue(true);
    const journalService = createJournalService({ getAdminConfig, setAdmins } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="alice" refreshKey={0} />);

    const removeButtons = await screen.findAllByRole('button', { name: 'Remove' });
    fireEvent.click(removeButtons[0]);
    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Remove your own administrator access for alice? Another administrator will remain configured.',
    );
    expect(setAdmins).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(setAdmins).toHaveBeenCalledWith(['bob']));
    await screen.findByText('Removed administrator alice.');
  });

  it('prevents removal of the final administrator', async () => {
    const journalService = createJournalService();
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    const remove = await screen.findByRole('button', { name: 'Remove' });
    expect(remove).toBeDisabled();
    fireEvent.click(remove);
    expect(journalService.setAdmins).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('does not let a late successful refresh erase a localized mutation failure', async () => {
    let finishRefresh: (value: any) => void = () => undefined;
    const pendingRefresh = new Promise((resolve) => { finishRefresh = resolve; });
    const config = {
      admins: ['admin'], bridges: [], localName: 'journal-0',
      localEndpoint: 'http://journal-0/interface', windowSize: 4,
      bridgeAccept: 'auto' as const, bridgePreapprovals: {},
    };
    const getAdminConfig = jest.fn()
      .mockResolvedValueOnce(config)
      .mockReturnValueOnce(pendingRefresh);
    const journalService = createJournalService({
      getAdminConfig,
      saveBridge: jest.fn().mockRejectedValue(new Error('bridge denied')),
    } as Partial<JournalService>);
    const { rerender } = render(
      <AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />,
    );
    const name = await screen.findByPlaceholderText('peer');
    const endpoint = screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface');
    fireEvent.change(name, { target: { value: 'beagle' } });
    fireEvent.change(endpoint, { target: { value: 'https://beagle/interface' } });
    rerender(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={1} />);
    fireEvent.click(screen.getByRole('button', { name: 'Add bridge' }));
    expect(await screen.findByText('bridge denied')).toBeInTheDocument();
    await act(async () => finishRefresh(config));
    expect(screen.getByText('bridge denied')).toBeInTheDocument();
    expect(name).toHaveValue('beagle');
    expect(endpoint).toHaveValue('https://beagle/interface');
  });

  it('confirms retention-window decreases with authoritative sizes and cancels without a request', async () => {
    const journalService = createJournalService();
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);
    const windowInput = await screen.findByRole('spinbutton');
    await waitFor(() => expect(windowInput).toHaveValue(4));
    fireEvent.change(windowInput, { target: { value: '2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Update Window' }));

    expect(await screen.findByRole('dialog')).toHaveTextContent(
      'Decrease window size from 4 to 2? Unpinned history outside the new window may be pruned, and later widening will not resurrect it.',
    );
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(journalService.setWindowSize).not.toHaveBeenCalled();
    expect(windowInput).toHaveValue(2);
  });

  it('refuses a decrease when authoritative state changes during confirmation', async () => {
    const base = {
      admins: ['admin'], bridges: [], localName: 'journal-0',
      localEndpoint: 'http://journal-0/interface', bridgeAccept: 'auto' as const,
      bridgePreapprovals: {},
    };
    const getAdminConfig = jest.fn()
      .mockResolvedValueOnce({ ...base, windowSize: 4 })
      .mockResolvedValue({ ...base, windowSize: 3 });
    const journalService = createJournalService({ getAdminConfig } as Partial<JournalService>);
    const { rerender } = render(
      <AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />,
    );
    const windowInput = await screen.findByRole('spinbutton');
    await waitFor(() => expect(windowInput).toHaveValue(4));
    fireEvent.change(windowInput, { target: { value: '2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Update Window' }));
    await screen.findByRole('dialog');
    rerender(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={1} />);
    await waitFor(() => expect(getAdminConfig).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText(
      'Administration state changed during confirmation; reload before changing the window.',
    )).toBeInTheDocument();
    expect(journalService.setWindowSize).not.toHaveBeenCalled();
  });

  it('does not confirm equal or increased retention windows', async () => {
    const journalService = createJournalService();
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);
    const windowInput = await screen.findByRole('spinbutton');
    await waitFor(() => expect(windowInput).toHaveValue(4));
    fireEvent.click(screen.getByRole('button', { name: 'Update Window' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Update Window' })).toBeEnabled());
    expect(journalService.setWindowSize).toHaveBeenCalledWith(4);
    fireEvent.change(windowInput, { target: { value: '6' } });
    fireEvent.click(screen.getByRole('button', { name: 'Update Window' }));
    await waitFor(() => expect(journalService.setWindowSize).toHaveBeenCalledWith(6));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('refuses a window change when the authoritative current size is unknown', async () => {
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue({
        admins: ['admin'], bridges: [], localName: 'journal-0',
        localEndpoint: 'http://journal-0/interface', windowSize: null,
        bridgeAccept: 'auto', bridgePreapprovals: {},
      }),
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);
    const windowInput = await screen.findByRole('spinbutton');
    fireEvent.change(windowInput, { target: { value: '2' } });
    fireEvent.click(screen.getByRole('button', { name: 'Update Window' }));
    expect(await screen.findByText(
      'Current authoritative window size is unavailable; reload before changing it.',
    )).toBeInTheDocument();
    expect(journalService.setWindowSize).not.toHaveBeenCalled();
  });

  it('treats a false mutation result as failure and preserves its form', async () => {
    const journalService = createJournalService({
      saveBridge: jest.fn().mockResolvedValue(false),
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);
    const name = await screen.findByPlaceholderText('peer');
    const endpoint = screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface');
    fireEvent.change(name, { target: { value: 'beagle' } });
    fireEvent.change(endpoint, { target: { value: 'https://beagle/interface' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add bridge' }));
    expect(await screen.findByText('Administration request was rejected.')).toBeInTheDocument();
    expect(name).toHaveValue('beagle');
    expect(endpoint).toHaveValue('https://beagle/interface');
    expect(journalService.getAdminConfig).toHaveBeenCalledTimes(1);
  });

  it('keeps only the newest Admin refresh when loads complete out of order', async () => {
    let finishOld: (value: any) => void = () => undefined;
    let finishNew: (value: any) => void = () => undefined;
    const oldRefresh = new Promise((resolve) => { finishOld = resolve; });
    const newRefresh = new Promise((resolve) => { finishNew = resolve; });
    const base = {
      admins: ['admin'], bridges: [], localName: 'journal-0',
      localEndpoint: 'http://journal-0/interface', bridgeAccept: 'auto' as const,
      bridgePreapprovals: {},
    };
    const getAdminConfig = jest.fn()
      .mockResolvedValueOnce({ ...base, windowSize: 4 })
      .mockReturnValueOnce(oldRefresh)
      .mockReturnValueOnce(newRefresh);
    const journalService = createJournalService({ getAdminConfig } as Partial<JournalService>);
    const { rerender } = render(
      <AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />,
    );
    const windowInput = await screen.findByRole('spinbutton');
    await waitFor(() => expect(windowInput).toHaveValue(4));
    rerender(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={1} />);
    rerender(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={2} />);
    await act(async () => finishNew({ ...base, windowSize: 12 }));
    await waitFor(() => expect(windowInput).toHaveValue(12));
    await act(async () => finishOld({ ...base, windowSize: 8 }));
    expect(windowInput).toHaveValue(12);
  });

  it('preserves a successful submission when its authoritative reload fails', async () => {
    const getAdminConfig = jest.fn()
      .mockResolvedValueOnce({
        admins: ['admin'], bridges: [], localName: 'journal-0',
        localEndpoint: 'http://journal-0/interface', windowSize: 4,
        bridgeAccept: 'auto', bridgePreapprovals: {},
      })
      .mockRejectedValueOnce(new Error('reload failed'));
    const journalService = createJournalService({ getAdminConfig } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);
    const name = await screen.findByPlaceholderText('peer');
    const endpoint = screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface');
    fireEvent.change(name, { target: { value: 'beagle' } });
    fireEvent.change(endpoint, { target: { value: 'https://beagle/interface' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add bridge' }));

    expect(await screen.findByText('reload failed')).toBeInTheDocument();
    expect(name).toHaveValue('beagle');
    expect(endpoint).toHaveValue('https://beagle/interface');
  });

  it('serializes Admin mutations and disables every mutation control while pending', async () => {
    let finishSave: (value: boolean) => void = () => undefined;
    const pendingSave = new Promise<boolean>((resolve) => { finishSave = resolve; });
    const config = {
      admins: ['admin', 'alice'], bridges: [], localName: 'journal-0',
      localEndpoint: 'http://journal-0/interface', windowSize: 4,
      bridgeAccept: 'preapproved' as const, bridgePreapprovals: {},
    };
    const journalService = createJournalService({
      getAdminConfig: jest.fn().mockResolvedValue(config),
      saveBridge: jest.fn().mockReturnValue(pendingSave),
    } as Partial<JournalService>);
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);
    await screen.findByPlaceholderText('64 hexadecimal characters');
    const name = screen.getAllByPlaceholderText('peer');
    const endpoint = screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface');
    fireEvent.change(name[0], { target: { value: 'beagle' } });
    fireEvent.change(endpoint, { target: { value: 'https://beagle/interface' } });
    const form = screen.getByRole('button', { name: 'Add bridge' }).closest('form') as HTMLFormElement;
    fireEvent.submit(form);
    fireEvent.submit(form);

    expect(journalService.saveBridge).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('radio', { name: /Automatically accept/ })).toBeDisabled();
    expect(screen.getByRole('radio', { name: /Require preapproval/ })).toBeDisabled();
    expect(screen.getByPlaceholderText('64 hexadecimal characters')).toBeDisabled();
    expect(screen.getByRole('spinbutton')).toBeDisabled();
    expect(screen.getByPlaceholderText('Username')).toBeDisabled();
    await act(async () => finishSave(true));
    await waitFor(() => expect(screen.getByPlaceholderText('Username')).toBeEnabled());
  });

  it('rejects non-http remote endpoint URLs before saving', async () => {
    const journalService = createJournalService();
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    fireEvent.change(await screen.findByPlaceholderText('peer'), {
      target: { value: 'beagle' },
    });
    fireEvent.change(screen.getByPlaceholderText('https://peer.example/api/v1/journal/interface'), {
      target: { value: 'ftp://beagle.sync-web.org/api/v1/journal/interface' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add bridge' }));

    expect(await screen.findByText('Peer endpoint must be an HTTP or HTTPS URL.')).toBeInTheDocument();
    expect(journalService.saveBridge).not.toHaveBeenCalled();
  });
});
