// jest-dom adds custom jest matchers for asserting on DOM nodes.
// allows you to do things like:
// expect(element).toHaveTextContent(/react/i)
// learn more: https://github.com/testing-library/jest-dom
import '@testing-library/jest-dom';
import { configure } from '@testing-library/react';
import React from 'react';

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
