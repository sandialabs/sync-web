import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import { WorkbenchLeftPane } from './WorkbenchLeftPane';

const response = (value: unknown) => Promise.resolve({
  ok: true,
  json: () => Promise.resolve(value),
} as Response);

describe('WorkbenchLeftPane API permissions', () => {
  afterEach(() => jest.restoreAllMocks());

  it('shows and filters the separate admin permission class', async () => {
    jest.spyOn(global, 'fetch').mockImplementation((input) => {
      const url = String(input);
      if (url.endsWith('/help-api.json')) {
        return response({
          get: { description: 'read', template: 'get template', example: 'get example', permission: 'user' },
          'call!': { description: 'admin call', template: 'call template', example: 'call example', permission: 'admin' },
          '*eval*': { description: 'root eval', template: 'eval template', example: 'eval example', permission: 'root' },
        });
      }
      return response({});
    });
    render(<WorkbenchLeftPane onUseExample={jest.fn()} />);

    expect(await screen.findByText('call!')).toBeInTheDocument();
    expect(screen.getByText('admin')).toHaveClass('api-permission-admin');
    fireEvent.click(screen.getByRole('button', { name: 'Admin' }));
    expect(screen.getByText('call!')).toBeInTheDocument();
    expect(screen.queryByText('get')).not.toBeInTheDocument();
    expect(screen.queryByText('*eval*')).not.toBeInTheDocument();
  });

  it('rejects an unknown catalog permission instead of silently reclassifying it', async () => {
    jest.spyOn(global, 'fetch').mockImplementation((input) => {
      if (String(input).endsWith('/help-api.json')) {
        return response({
          unsafe: { description: 'bad', template: 'bad', example: 'bad', permission: 'operator' },
        });
      }
      return response({});
    });
    render(<WorkbenchLeftPane onUseExample={jest.fn()} />);

    expect(await screen.findByText('Error: Invalid API catalog entry: unsafe')).toBeInTheDocument();
  });
});
