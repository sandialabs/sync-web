import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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
  beforeEach(() => jest.clearAllMocks());

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

    const journalId = 'ab'.repeat(32);
    fireEvent.change(await screen.findByPlaceholderText('64 hexadecimal characters'), {
      target: { value: journalId },
    });
    const peerInputs = screen.getAllByPlaceholderText('peer');
    fireEvent.change(peerInputs[peerInputs.length - 1], { target: { value: 'beagle' } });
    fireEvent.click(screen.getByRole('button', { name: 'Allow bridge' }));

    await waitFor(() => expect(journalService.updateConfig).toHaveBeenCalledWith(
      ['private', 'bridge-preapproval', 'beagle'],
      { '*type/byte-vector*': journalId },
    ));
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
