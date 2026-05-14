import React from 'react';
import { render, screen, waitFor } from '@testing-library/react';
import App from './App';

const mockFetchResponse = (ok: boolean, status: number) =>
  Promise.resolve({ ok, status } as Response);

describe('App session check', () => {
  let originalLocation: Location;

  beforeEach(() => {
    originalLocation = window.location;
    // jsdom does not allow direct assignment to window.location, so we delete and redefine.
    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: { ...originalLocation, href: 'http://localhost/explorer' },
    });
  });

  afterEach(() => {
    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: originalLocation,
    });
    jest.clearAllMocks();
  });

  it('renders the app when whoami returns 200', async () => {
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ identity: { traits: { username: 'alice' } } }),
    } as unknown as Response);

    render(<App />);

    await waitFor(() => {
      expect(screen.queryByLabelText('Checking session…')).not.toBeInTheDocument();
    });
    expect(screen.getByText('Ledger')).toBeInTheDocument();
    expect(screen.getByText('alice')).toBeInTheDocument();
  });

  it('shows an in-place sign-in prompt when whoami returns 401', async () => {
    jest.spyOn(global, 'fetch').mockResolvedValueOnce({ ok: false, status: 401 } as Response);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('Sign in to use Explorer')).toBeInTheDocument();
    });
    expect(screen.getByRole('link', { name: 'Sign in' })).toHaveAttribute(
      'href',
      '/auth/login?return_to=' + encodeURIComponent('http://localhost/explorer'),
    );
    expect(window.location.href).toBe('http://localhost/explorer');
  });

  it('shows an in-place retry prompt when whoami fetch rejects', async () => {
    jest.spyOn(global, 'fetch').mockRejectedValueOnce(new Error('Network error'));

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('Sign in to use Explorer')).toBeInTheDocument();
    });
    expect(screen.getByText('Could not check session')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(window.location.href).toBe('http://localhost/explorer');
  });
});
