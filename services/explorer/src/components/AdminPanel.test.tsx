import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import AdminPanel from './AdminPanel';
import { JournalService } from '../services/JournalService';

const createJournalService = (overrides: Partial<JournalService> = {}) => ({
  getAdminConfig: jest.fn().mockResolvedValue({
    admins: ['admin'],
    bridges: [],
    subscribers: [],
    localName: 'journal-0',
    windowSize: 4,
  }),
  saveBridge: jest.fn().mockResolvedValue(true),
  deleteBridge: jest.fn().mockResolvedValue(true),
  setAdmins: jest.fn().mockResolvedValue(true),
  setWindowSize: jest.fn().mockResolvedValue(true),
  ...overrides,
}) as unknown as JournalService;

describe('AdminPanel', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('prefills remote name from local config and creates an outgoing bridge', async () => {
    const journalService = createJournalService();
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    const bridgeNameInput = await screen.findByPlaceholderText('Bridge name');

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'outgoing' } });
    fireEvent.change(bridgeNameInput, { target: { value: 'beagle' } });
    fireEvent.change(screen.getByPlaceholderText('Remote endpoint'), {
      target: { value: 'https://beagle.sync-web.org/api/v1/journal/interface' },
    });

    expect(screen.getByPlaceholderText('Remote name')).toHaveValue('journal-0');

    fireEvent.click(screen.getByRole('button', { name: 'Create' }));

    await waitFor(() => {
      expect(journalService.saveBridge).toHaveBeenCalledWith({
        name: 'beagle',
        endpoint: 'https://beagle.sync-web.org/api/v1/journal/interface',
        direction: 'outgoing',
        policy: { publish: 'push', subscribe: 'pull' },
        remoteName: 'journal-0',
      });
    });
  });

  it('rejects non-http remote endpoint URLs before saving', async () => {
    const journalService = createJournalService();
    render(<AdminPanel journalService={journalService} currentUser="admin" refreshKey={0} />);

    const bridgeNameInput = await screen.findByPlaceholderText('Bridge name');

    fireEvent.change(bridgeNameInput, { target: { value: 'beagle' } });
    fireEvent.change(screen.getByPlaceholderText('Remote endpoint'), {
      target: { value: 'ftp://beagle.sync-web.org/api/v1/journal/interface' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Create' }));

    expect(await screen.findByText('Remote endpoint must be an http:// or https:// URL.')).toBeInTheDocument();
    expect(journalService.saveBridge).not.toHaveBeenCalled();
  });
});
