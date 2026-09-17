import React from 'react';
import { fireEvent, render, screen } from '@testing-library/react';
import ToolBar from './ToolBar';

describe('ToolBar', () => {
  it('orders Stage, Ledger, Access, and Admin without changing the active mode', () => {
    render(
      <ToolBar
        sessionName="alice"
        error={null}
        mode="ledger"
        isAdmin
        theme="light"
        onModeChange={jest.fn()}
        onThemeToggle={jest.fn()}
      />,
    );

    const modeButtons = screen.getAllByRole('button')
      .filter((button) => ['Stage', 'Ledger', 'Access', 'Admin'].includes(button.textContent ?? ''));
    expect(modeButtons.map((button) => button.textContent)).toEqual([
      'Stage', 'Ledger', 'Access', 'Admin',
    ]);
    expect(screen.getByRole('button', { name: 'Ledger' })).toHaveClass('active');
    expect(screen.getByRole('img', { name: 'Synchronic Web' }))
      .toHaveAttribute('src', '/explorer/logo.png');
  });

  it('closes transient navigation before starting sign out', () => {
    const onSignOut = jest.fn();
    const fetch = jest.spyOn(global, 'fetch').mockResolvedValue({ ok: false } as Response);
    render(
      <ToolBar
        sessionName="alice"
        error={null}
        mode="stage"
        isAdmin={false}
        theme="light"
        onModeChange={jest.fn()}
        onThemeToggle={jest.fn()}
        onSignOut={onSignOut}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Sign out' }));
    expect(onSignOut).toHaveBeenCalledTimes(1);
    fetch.mockRestore();
  });

  it('visibly labels Self-local controls while a remote route is selected', () => {
    render(
      <ToolBar
        sessionName="alice"
        error={null}
        mode="ledger"
        isAdmin
        localOnlyDisabled
        theme="light"
        onModeChange={jest.fn()}
        onThemeToggle={jest.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: 'Access · Self' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Admin · Self' })).toBeDisabled();
  });
});
