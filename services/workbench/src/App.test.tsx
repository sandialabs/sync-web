import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import App from './App';
import { executeQuery } from './utils/api';

jest.mock('./utils/api', () => ({ executeQuery: jest.fn() }));
jest.mock('./components/WorkbenchLeftPane', () => ({
  WorkbenchLeftPane: () => <div>Left pane</div>,
}));
jest.mock('./components/QueryPane', () => ({
  QueryPane: (props: any) => (
    <div>
      <button onClick={() => props.onTabContentChange(props.activeTabId, '((function no-such-function))')}>
        Set query
      </button>
      <button onClick={props.onRunQuery}>Run query</button>
    </div>
  ),
}));
jest.mock('./components/OutputPane', () => ({
  OutputPane: () => <div>Output pane</div>,
}));
jest.mock('./components/WorkbenchHelpModal', () => ({
  WorkbenchHelpModal: () => <div>Help</div>,
}));

const mockExecuteQuery = executeQuery as jest.MockedFunction<typeof executeQuery>;

describe('App query status', () => {
  beforeEach(() => {
    mockExecuteQuery.mockReset();
    (window.matchMedia as jest.Mock).mockImplementation((query: string) => ({
      matches: false,
      media: query,
      addListener: jest.fn(),
      removeListener: jest.fn(),
      addEventListener: jest.fn(),
      removeEventListener: jest.fn(),
      dispatchEvent: jest.fn(),
    }));
  });

  it('shows a canonical journal error in the toolbar and history', async () => {
    mockExecuteQuery.mockResolvedValue({
      result: `(error 'api-error "unknown endpoint")`,
      request: 'POST /interface\n\n((function no-such-function))',
      response: `HTTP 200 OK\n\n(error 'api-error "unknown endpoint")`,
      error: 'Journal error: api-error',
    });

    const { container } = render(<App />);
    fireEvent.click(screen.getByText('Set query'));
    fireEvent.click(screen.getByText('Run query'));

    expect(await screen.findByText('Error: Journal error: api-error')).toBeInTheDocument();
    await waitFor(() => expect(container.querySelector('.history-entry.error')).toBeInTheDocument());
    expect(screen.getByText('✗')).toBeInTheDocument();
    expect(screen.queryByText('Ready')).not.toBeInTheDocument();
  });
});
