// jest-dom adds custom jest matchers for asserting on DOM nodes.
// allows you to do things like:
// expect(element).toHaveTextContent(/react/i)
// learn more: https://github.com/testing-library/jest-dom
import '@testing-library/jest-dom';
import { configure } from '@testing-library/react';
import React from 'react';
import { TextDecoder, TextEncoder } from 'util';

Object.assign(global, { TextDecoder, TextEncoder });

class MockEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  readonly CONNECTING = 0;
  readonly OPEN = 1;
  readonly CLOSED = 2;
  readonly url: string;
  readonly withCredentials: boolean;
  readyState = MockEventSource.CONNECTING;
  onopen = null;
  onmessage = null;
  onerror = null;

  constructor(url: string | URL, init?: EventSourceInit) {
    this.url = String(url);
    this.withCredentials = init?.withCredentials ?? false;
  }

  addEventListener() {}
  removeEventListener() {}
  dispatchEvent() { return false; }
  close() { this.readyState = MockEventSource.CLOSED; }
}

Object.defineProperty(global, 'EventSource', {
  writable: true,
  configurable: true,
  value: MockEventSource,
});

// Configure @testing-library/react to use React.act
configure({
  asyncUtilTimeout: 5000,
});

// Suppress the ReactDOMTestUtils.act deprecation warning
const originalError = console.error;
console.error = (...args) => {
  if (
    typeof args[0] === 'string' &&
    (
      args[0].includes('ReactDOMTestUtils.act is deprecated') ||
      args[0].includes('`ReactDOMTestUtils.act` is deprecated') ||
      args[0].includes('not wrapped in act')
    )
  ) {
    return;
  }
  originalError.call(console, ...args);
};

// Mock window.matchMedia — plain function so jest.resetAllMocks() cannot clear it
Object.defineProperty(window, 'matchMedia', {
  writable: true,
  configurable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }),
});

// Mock localStorage
const localStorageMock = {
  getItem: jest.fn(),
  setItem: jest.fn(),
  removeItem: jest.fn(),
  clear: jest.fn(),
  length: 0,
  key: jest.fn(),
};
Object.defineProperty(window, 'localStorage', { value: localStorageMock });

// Set up global React.act for testing-library
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
