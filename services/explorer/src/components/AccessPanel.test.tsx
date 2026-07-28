import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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
    jest.clearAllMocks();
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

  it('lets admins manage another local namespace', async () => {
    const journalService = createJournalService();
    render(<AccessPanel journalService={journalService} currentUser="admin" refreshKey={0} isAdmin />);

    await screen.findByLabelText('Manage namespace');
    fireEvent.change(screen.getByLabelText('Manage namespace'), { target: { value: 'bob' } });

    await waitFor(() => {
      expect(journalService.getAuthorizations).toHaveBeenLastCalledWith(['*state*', 'bob']);
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
