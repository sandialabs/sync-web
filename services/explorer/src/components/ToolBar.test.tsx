import React from 'react';
import { render, screen } from '@testing-library/react';
import ToolBar from './ToolBar';

describe('ToolBar', () => {
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
