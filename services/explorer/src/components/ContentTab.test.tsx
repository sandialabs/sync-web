import React from 'react';
import { act, render, screen, fireEvent, waitFor } from '@testing-library/react';
import ContentTab from './ContentTab';
import { JournalService } from '../services/JournalService';
import { AppState } from '../types';

// Mock JournalService
jest.mock('../services/JournalService', () => ({
  JournalService: {
    parseDirectoryResponse: jest.fn(),
    extractSchemeValue: jest.fn((value) => ({ value, schemeType: null })),
    documentContentToText: jest.fn((value) => {
      if (value && typeof value === 'object' && '*type/byte-vector*' in value) {
        return value['*type/byte-vector*'] === '48656c6c6f20576f726c64' ? 'Hello World' : 'content';
      }
      return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    }),
  },
}));

const mockJournalService = {
  get: jest.fn(),
  set: jest.fn(),
  setText: jest.fn(),
  pin: jest.fn(),
  unpin: jest.fn(),
} as unknown as JournalService;

const createAppState = (overrides: Partial<AppState> = {}): AppState => ({
  endpoint: 'http://test.com',
  rootIndex: 100,
  selectedPath: null,
  expandedNodes: new Set(),
  isLoading: false,
  error: null,
  ...overrides,
});

describe('ContentTab', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    // Reset the mock to default behavior
    (JournalService.extractSchemeValue as jest.Mock).mockImplementation((value) => ({ value, schemeType: null }));
    (JournalService.documentContentToText as jest.Mock).mockImplementation((value) => {
      if (value && typeof value === 'object' && '*type/byte-vector*' in value) {
        return value['*type/byte-vector*'] === '48656c6c6f20576f726c64' ? 'Hello World' : 'content';
      }
      return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    });
    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue(null);
  });

  it('should show empty state when no path is selected', async () => {
    const appState = createAppState();
    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Select a document or directory to view its contents')).toBeInTheDocument();
    });
  });

  it('should load and display content when path is selected', async () => {
    const appState = createAppState({
      selectedPath: ['*state*', 'test'],
    });

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '48656c6c6f20576f726c64' },
      'pinned?': null,
      proof: {},
    });

    (JournalService.extractSchemeValue as jest.Mock).mockReturnValue({
      value: 'Hello World',
      schemeType: 'string',
    });

    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByText('Hello World')).toBeInTheDocument();
  });

  it('should display directory contents', async () => {
    const appState = createAppState({
      selectedPath: ['*state*'],
    });

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['directory', ['file1', 'file2', 'file3'], true],
      'pinned?': null,
      proof: {},
    });

    (JournalService.parseDirectoryResponse as jest.Mock).mockReturnValue({
      items: ['file1', 'file2', 'file3'],
      isComplete: true,
    });

    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Directory Contents:')).toBeInTheDocument();
      expect(screen.getByText('file1')).toBeInTheDocument();
      expect(screen.getByText('file2')).toBeInTheDocument();
      expect(screen.getByText('file3')).toBeInTheDocument();
    });
  });

  it('should show edit button for local paths', async () => {
    const appState = createAppState({
      selectedPath: ['*state*', 'test'],
    });

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '6564697461626c6520636f6e74656e74' },
      'pinned?': null,
      proof: {},
    });

    (JournalService.extractSchemeValue as jest.Mock).mockReturnValue({
      value: 'editable content',
      schemeType: 'string',
    });

    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Edit')).toBeInTheDocument();
    });
  });

  it('should show pin button for non-local versioned paths', async () => {
    const appState = createAppState({
      selectedPath: [-1, '*bridge*', 'alice', -1, '*state*', 'test'],
    });

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '7065657220636f6e74656e74' },
      'pinned?': null,
      proof: {},
    });

    (JournalService.extractSchemeValue as jest.Mock).mockReturnValue({
      value: 'peer content',
      schemeType: 'string',
    });

    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('📌 Pin')).toBeInTheDocument();
    });
  });

  it('should handle pin action', async () => {
    const appState = createAppState({
      selectedPath: [-1, '*state*', 'test'],
    });

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: { '*type/byte-vector*': '636f6e74656e74' },
      'pinned?': null,
      proof: {},
    });
    (mockJournalService.pin as jest.Mock).mockResolvedValue(true);

    (JournalService.extractSchemeValue as jest.Mock).mockReturnValue({
      value: 'content',
      schemeType: 'string',
    });

    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('📌 Pin')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('📌 Pin'));

    await waitFor(() => {
      expect(mockJournalService.pin).toHaveBeenCalledWith([-1, '*state*', 'test']);
    });
  });

  it('should display empty document message', async () => {
    const appState = createAppState({
      selectedPath: ['*state*', 'empty'],
    });

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['nothing'],
      'pinned?': null,
      proof: {},
    });

    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Empty document')).toBeInTheDocument();
    });
  });

  it('should display pruned document message', async () => {
    const appState = createAppState({
      selectedPath: [-100, '*state*', 'old'],
    });

    (mockJournalService.get as jest.Mock).mockResolvedValue({
      content: ['unknown'],
      'pinned?': null,
      proof: {},
    });

    render(
      <ContentTab
        appState={appState}
        journalService={mockJournalService}
        onContentUpdate={jest.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByText('Document has been pruned')).toBeInTheDocument();
    });
  });
});
