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
          'use!': { description: 'read', template: 'use template', example: 'use example', permission: 'user' },
          'run!': { description: 'admin run', template: 'run template', example: 'run example', permission: 'admin' },
          '*eval*': { description: 'root eval', template: 'eval template', example: 'eval example', permission: 'root' },
        });
      }
      return response({});
    });
    render(<WorkbenchLeftPane onUseExample={jest.fn()} />);

    expect(await screen.findByText('run!')).toBeInTheDocument();
    expect(screen.getByText('admin')).toHaveClass('api-permission-admin');
    fireEvent.click(screen.getByRole('button', { name: 'Admin' }));
    expect(screen.getByText('run!')).toBeInTheDocument();
    expect(screen.queryByText('use!')).not.toBeInTheDocument();
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

  it('only returns copy and destructive truncate examples to the editor callback', async () => {
    const onUseExample = jest.fn();
    jest.spyOn(global, 'fetch').mockImplementation((input) => {
      if (String(input).endsWith('/help-api.json')) {
        return response({
          'copy!': { description: 'copy', template: 'copy template', example: 'copy example', permission: 'user' },
          'truncate!': { description: 'Destructive truncate', template: 'truncate template', example: 'truncate example', permission: 'admin' },
        });
      }
      return response({});
    });
    render(<WorkbenchLeftPane onUseExample={onUseExample} />);

    fireEvent.click(await screen.findByText('copy!'));
    fireEvent.click(screen.getByRole('button', { name: 'Use Example' }));
    expect(onUseExample).toHaveBeenCalledWith('copy example');
    expect(onUseExample).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByText('truncate!'));
    fireEvent.click(screen.getByRole('button', { name: 'Use Example' }));
    expect(onUseExample).toHaveBeenLastCalledWith('truncate example');
    expect(onUseExample).toHaveBeenCalledTimes(2);
  });

});
