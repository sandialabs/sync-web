import { executeQuery } from './api';

// Mock fetch globally
const mockFetch = jest.fn();
global.fetch = mockFetch;

describe('executeQuery', () => {
  beforeEach(() => {
    mockFetch.mockClear();
  });

  it('should execute a successful query', async () => {
    const mockResponse = { result: 42 };
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      statusText: 'OK',
      text: () => Promise.resolve(JSON.stringify(mockResponse)),
    });

    const result = await executeQuery('http://localhost:4096/interface', '(+ 1 2)');

    expect(result.error).toBeUndefined();
    expect(result.result).toEqual(mockResponse);
    expect(result.request).toContain('POST http://localhost:4096/interface');
    expect(result.request).toContain('(+ 1 2)');
    expect(result.response).toContain('HTTP 200 OK');
  });

  it('should classify a canonical top-level journal error without losing the exchange text', async () => {
    const journalError = `(error 'api-error "Interface does not implement API endpoint: no-such-function" ((data (no-such-function))))`;
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        statusText: 'OK',
        text: () => Promise.resolve(journalError),
      })
      .mockResolvedValueOnce({
        ok: true,
        text: () => Promise.resolve(JSON.stringify([
          'error', { '*type/quoted*': 'api-error' },
          { '*type/string*': 'Interface does not implement API endpoint: no-such-function' },
          { data: ['no-such-function'] },
        ])),
      });

    const result = await executeQuery('http://localhost:4096/interface', '((function no-such-function))');

    expect(result.error).toBe('Journal error: api-error');
    expect(result.result).toBe(journalError);
    expect(result.request).toContain('((function no-such-function))');
    expect(result.response).toContain(journalError);
  });

  it.each([
    ['"plain text containing error"', undefined],
    [`(prefix (error 'api-error "not top level"))`, ['prefix', ['error']]],
    ['(error-message ordinary-value)', ['error-message', 'ordinary-value']],
    ['{"error":"ordinary JSON value"}', undefined],
  ])('should not classify an ordinary response as a journal error: %s', async (responseText, converted) => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      statusText: 'OK',
      text: () => Promise.resolve(responseText),
    });
    if (converted !== undefined) {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        text: () => Promise.resolve(JSON.stringify(converted)),
      });
    }

    const result = await executeQuery('http://localhost:4096/interface', '(ordinary)');
    expect(result.error).toBeUndefined();
  });

  it('fails safely when the Journal codec cannot parse a non-JSON response', async () => {
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        statusText: 'OK',
        text: () => Promise.resolve('(unterminated'),
      })
      .mockResolvedValueOnce({ ok: false, text: () => Promise.resolve('') });

    const result = await executeQuery(
      'http://localhost:4096/mount/interface?ignored=1#fragment',
      '(ordinary)',
    );
    expect(result.error).toBe('Journal response could not be parsed');
    expect(mockFetch).toHaveBeenNthCalledWith(
      2,
      'http://localhost:4096/mount/interface/scheme-to-json',
      expect.objectContaining({ body: '(unterminated' }),
    );
  });

  it('should handle HTTP errors', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      text: () => Promise.resolve('Server error'),
    });

    const result = await executeQuery('http://localhost:4096/interface', '(invalid)');

    expect(result.error).toBe('Request failed: 500 Internal Server Error');
    expect(result.result).toBeNull();
    expect(result.response).toContain('HTTP 500');
  });

  it('should handle network errors', async () => {
    mockFetch.mockRejectedValueOnce(new Error('Network error'));

    const result = await executeQuery('http://localhost:4096/interface', '(+ 1 2)');

    expect(result.error).toBe('Network error');
    expect(result.result).toBeNull();
    expect(result.response).toContain('Error: Network error');
  });

  it('should handle non-JSON responses', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      statusText: 'OK',
      text: () => Promise.resolve('"plain text response"'),
    });

    const result = await executeQuery('http://localhost:4096/interface', '(display "hello")');

    expect(result.error).toBeUndefined();
    expect(result.result).toBe('plain text response');
  });

  it('should trim query whitespace', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      statusText: 'OK',
      text: () => Promise.resolve('42'),
    });

    await executeQuery('http://localhost:4096/interface', '  (+ 1 2)  \n');

    expect(mockFetch).toHaveBeenCalledWith(
      'http://localhost:4096/interface',
      expect.objectContaining({
        body: '(+ 1 2)',
      })
    );
  });

  it('should send correct headers', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      status: 200,
      statusText: 'OK',
      text: () => Promise.resolve('42'),
    });

    await executeQuery('http://localhost:4096/interface', '(+ 1 2)');

    expect(mockFetch).toHaveBeenCalledWith(
      'http://localhost:4096/interface',
      expect.objectContaining({
        method: 'POST',
        headers: {
          'Content-Type': 'text/plain',
        },
      })
    );
  });

  it('should handle unknown error types', async () => {
    mockFetch.mockRejectedValueOnce('string error');

    const result = await executeQuery('http://localhost:4096/interface', '(+ 1 2)');

    expect(result.error).toBe('Unknown error');
  });
});
