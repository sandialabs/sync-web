import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import ObjectPane from './ObjectPane';
import { JournalService } from '../services/JournalService';

const apiResult = {
  operation: 'use!' as const,
  context: { route: ['peer-a'] },
  path: ['*state*', 'alice', 'counter'],
  readOnly: true,
  result: '(*name* *api* value)',
};

const service = {
  invokeObject: jest.fn(),
  getFederationContext: jest.fn(() => ({ route: ['peer-a'], historyIndexes: [4, 8] })),
} as unknown as JournalService;

describe('ObjectPane', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (service.getFederationContext as jest.Mock).mockReturnValue({
      route: ['peer-a'], historyIndexes: [4, 8],
    });
    (service.invokeObject as jest.Mock).mockResolvedValue({
      operation: 'use!',
      context: { route: ['peer-a'] },
      path: ['*state*', 'alice', 'counter'],
      readOnly: false,
      result: { '*type/byte-vector*': '35' },
    });
  });

  it('defaults Stage use to read only and permits an explicit persisting call', async () => {
    render(<ObjectPane
      journalService={service}
      path={['*state*', 'alice', 'counter']}
      historical={false}
      metadata="((class counter) (object-hash #u(1)) (code-hash #u(2)))"
      apiResult={apiResult}
    />);
    expect(await screen.findByText(/class counter/)).toBeInTheDocument();
    expect(screen.getByText('(*name* *api* value)')).toBeInTheDocument();
    expect(service.invokeObject).not.toHaveBeenCalled();
    expect(screen.getByText(/\(operation use!\)/)).toBeInTheDocument();
    expect(screen.getByText(/\(route \("peer-a"\)\)/)).toBeInTheDocument();
    expect(screen.getByText(/\(arguments \(\)\)/)).toBeInTheDocument();
    expect(screen.getByLabelText('Read only')).toBeChecked();
    fireEvent.change(screen.getByLabelText('Method symbol'), { target: { value: 'increment!' } });
    fireEvent.change(screen.getByLabelText('Complete Scheme arguments list'), { target: { value: '(2)' } });
    fireEvent.click(screen.getByLabelText('Read only'));
    fireEvent.click(screen.getByRole('button', { name: 'Execute use!' }));

    await waitFor(() => expect(service.invokeObject).toHaveBeenCalledWith({
      path: ['*state*', 'alice', 'counter'],
      method: 'increment!',
      argumentsExpression: '(2)',
      readOnly: false,
      historical: false,
    }));
  });

  it('makes Ledger retrieve visibly and immutably non-persisting', async () => {
    render(<ObjectPane
      journalService={service}
      path={[8, '*state*', 'alice', 'counter']}
      historical
      metadata="((class counter) (object-hash #u(1)) (code-hash #u(2)))"
      apiResult={{ ...apiResult, operation: 'retrieve' }}
    />);
    const control = screen.getByLabelText('Read only');
    expect(control).toBeChecked();
    expect(control).toBeDisabled();
    expect(screen.queryByText(/Historical retrieve is always non-persisting/)).not.toBeInTheDocument();
    expect(screen.getByText(/\(operation retrieve\)/)).toBeInTheDocument();
    expect(screen.getByText('retrieve')).toBeInTheDocument();
  });
});
