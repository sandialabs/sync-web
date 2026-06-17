import { JournalService } from './JournalService';

describe('JournalService', () => {
  describe('extractSchemeValue', () => {
    it('should extract string type values', () => {
      const input = { '*type/string*': 'hello world' };
      const result = JournalService.extractSchemeValue(input);
      expect(result).toEqual({ value: 'hello world', schemeType: 'string' });
    });

    it('should extract byte-vector type values', () => {
      const input = { '*type/byte-vector*': 'base64data' };
      const result = JournalService.extractSchemeValue(input);
      expect(result).toEqual({ value: 'base64data', schemeType: 'byte-vector' });
    });

    it('should return original value for non-scheme objects', () => {
      const input = { foo: 'bar' };
      const result = JournalService.extractSchemeValue(input);
      expect(result).toEqual({ value: { foo: 'bar' }, schemeType: null });
    });

    it('should return original value for arrays', () => {
      const input = ['directory', ['a', 'b']];
      const result = JournalService.extractSchemeValue(input);
      expect(result).toEqual({ value: ['directory', ['a', 'b']], schemeType: null });
    });

    it('should return original value for primitives', () => {
      expect(JournalService.extractSchemeValue('string')).toEqual({ value: 'string', schemeType: null });
      expect(JournalService.extractSchemeValue(123)).toEqual({ value: 123, schemeType: null });
      expect(JournalService.extractSchemeValue(true)).toEqual({ value: true, schemeType: null });
      expect(JournalService.extractSchemeValue(null)).toEqual({ value: null, schemeType: null });
    });
  });

  describe('parseDirectoryResponse', () => {
    it('should parse complete directory response in object-map format', () => {
      const input = ['directory', { file1: 'value', file2: 'directory', file3: 'unknown' }, true];
      const result = JournalService.parseDirectoryResponse(input);
      expect(result).toEqual({
        items: ['file1', 'file2', 'file3'],
        isComplete: true,
      });
    });

    it('should parse incomplete directory response in object-map format', () => {
      const input = ['directory', { file1: 'value', file2: 'directory' }, false];
      const result = JournalService.parseDirectoryResponse(input);
      expect(result).toEqual({
        items: ['file1', 'file2'],
        isComplete: false,
      });
    });

    it('should parse directory response without completeness flag in object-map format', () => {
      const input = ['directory', { file1: 'value' }];
      const result = JournalService.parseDirectoryResponse(input);
      expect(result).toEqual({
        items: ['file1'],
        isComplete: true,
      });
    });

    it('should parse legacy directory response in array format for backward compatibility', () => {
      const input = ['directory', ['file1', 'file2'], true];
      const result = JournalService.parseDirectoryResponse(input);
      expect(result).toEqual({
        items: ['file1', 'file2'],
        isComplete: true,
      });
    });

    it('should return null for non-directory content', () => {
      expect(JournalService.parseDirectoryResponse({ '*type/string*': 'hello' })).toBeNull();
      expect(JournalService.parseDirectoryResponse(['nothing'])).toBeNull();
      expect(JournalService.parseDirectoryResponse('string')).toBeNull();
      expect(JournalService.parseDirectoryResponse(null)).toBeNull();
    });

    it('should return null for malformed directory response', () => {
      expect(JournalService.parseDirectoryResponse(['directory'])).toBeNull();
      expect(JournalService.parseDirectoryResponse(['directory', 'not-an-object'])).toBeNull();
    });
  });

  describe('path segment codec', () => {
    it('encodes non-R7RS path names with percent escapes', () => {
      expect(JournalService.encodePathSegment('sync-node?')).toBe('sync-node?');
      expect(JournalService.encodePathSegment('*')).toBe('*');
      expect(JournalService.encodePathSegment('New folder')).toBe('New%20folder');
      expect(JournalService.encodePathSegment('a%b')).toBe('a%25b');
      expect(JournalService.encodePathSegment('a%20b')).toBe('a%2520b');
      expect(JournalService.encodePathSegment('é')).toBe('%C3%A9');
      expect(JournalService.encodePathSegment('123')).toBe('%3123');
    });

    it('decodes percent-escaped path names for display', () => {
      expect(JournalService.decodePathSegment('sync-node?')).toBe('sync-node?');
      expect(JournalService.decodePathSegment('New%20folder')).toBe('New folder');
      expect(JournalService.decodePathSegment('a%25b')).toBe('a%b');
      expect(JournalService.decodePathSegment('%C3%A9')).toBe('é');
      expect(JournalService.decodePathSegment('bad%escape')).toBe('bad%escape');
    });
  });

  describe('parseDirectoryEntries', () => {
    it('should parse entry types from object-map directory payload', () => {
      const input = ['directory', { folder: 'directory', doc: 'value', mystery: 'unknown' }, true];
      const result = JournalService.parseDirectoryEntries(input);
      expect(result).toEqual([
        { name: 'folder', pathSegment: 'folder', type: 'directory' },
        { name: 'doc', pathSegment: 'doc', type: 'value' },
        { name: 'mystery', pathSegment: 'mystery', type: 'unknown' },
      ]);
    });

    it('should parse pair-list directory payload with entry types', () => {
      const input = ['directory', [['folder', 'directory'], ['doc%20name.txt', 'value'], ['mystery', 'unknown']], true];
      const result = JournalService.parseDirectoryEntries(input);
      expect(result).toEqual([
        { name: 'folder', pathSegment: 'folder', type: 'directory' },
        { name: 'doc name.txt', pathSegment: 'doc%20name.txt', type: 'value' },
        { name: 'mystery', pathSegment: 'mystery', type: 'unknown' },
      ]);
    });

    it('should treat legacy array directory payload as unknown entry types', () => {
      const input = ['directory', [{ '*type/string*': 'a' }, 'b'], true];
      const result = JournalService.parseDirectoryEntries(input);
      expect(result).toEqual([
        { name: 'a', pathSegment: 'a', type: 'unknown' },
        { name: 'b', pathSegment: 'b', type: 'unknown' },
      ]);
    });

    it('skips unsupported constructed-symbol directory names', () => {
      const input = ['directory', [[['symbol', { '*type/string*': 'bad name' }], 'directory'], ['good%20name', 'value']], true];
      const result = JournalService.parseDirectoryEntries(input);
      expect(result).toEqual([
        { name: 'good name', pathSegment: 'good%20name', type: 'value' },
      ]);
    });

    it('should return null for non-directory content', () => {
      expect(JournalService.parseDirectoryEntries({ '*type/string*': 'hello' })).toBeNull();
    });
  });
});

describe('reserved state segments', () => {
  it('treats star-wrapped names as reserved but leaves ordinary names alone', () => {
    expect(JournalService.isReservedStateSegment('*time*')).toBe(true);
    expect(JournalService.isReservedStateSegment('*directory*')).toBe(true);
    expect(JournalService.isReservedStateSegment('alice')).toBe(false);
    expect(JournalService.isReservedStateSegment('*draft')).toBe(false);
  });
});

describe('JournalService API', () => {
  let service: JournalService;
  let mockFetch: jest.Mock;
  const mockJsonResponse = (payload: unknown, ok = true, status = 200, statusText = 'OK') => ({
    ok,
    status,
    statusText,
    text: () => Promise.resolve(JSON.stringify(payload)),
  });
  const mockTextResponse = (payload: string, ok = true, status = 200, statusText = 'OK') => ({
    ok,
    status,
    statusText,
    text: () => Promise.resolve(payload),
  });

  beforeEach(() => {
    service = new JournalService('http://test-endpoint.com/api/v1');
    mockFetch = jest.fn();
    global.fetch = mockFetch;
  });

  afterEach(() => {
    jest.resetAllMocks();
  });

  describe('subscribeEvents', () => {
    it('subscribes to gateway event stream and closes on unsubscribe', () => {
      const listeners: Record<string, (event: MessageEvent) => void> = {};
      const close = jest.fn();
      const eventSourceMock = jest.fn().mockImplementation(() => ({
        addEventListener: jest.fn((type: string, listener: (event: MessageEvent) => void) => {
          listeners[type] = listener;
        }),
        close,
      }));
      const originalEventSource = global.EventSource;
      (global as any).EventSource = eventSourceMock;
      const onChange = jest.fn();

      try {
        const unsubscribe = service.subscribeEvents({ onChange });
        expect(eventSourceMock).toHaveBeenCalledWith(
          'http://test-endpoint.com/api/v1/events',
          { withCredentials: true },
        );

        listeners['sync-web-change']?.({
          data: JSON.stringify({ operation: 'set!', path: ['*state*', 'alice'], time: 'now' }),
        } as MessageEvent);
        expect(onChange).toHaveBeenCalledWith({ operation: 'set!', path: ['*state*', 'alice'], time: 'now' });

        unsubscribe();
        expect(close).toHaveBeenCalled();
      } finally {
        (global as any).EventSource = originalEventSource;
      }
    });
  });

  describe('getSize', () => {
    it('should call size endpoint and return result', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('42'));

      const result = await service.getSize();

      expect(result).toBe(42);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/size',
        expect.objectContaining({
          method: 'GET',
          headers: {},
        })
      );
    });
  });

  describe('get', () => {
    it('should call get endpoint for staged paths', async () => {
      const rawValue = { '*type/string*': 'test content' };
      mockFetch.mockResolvedValueOnce(mockJsonResponse(rawValue));

      const path = ['*state*', 'test'];
      const result = await service.get(path);

      expect(result).toEqual({ content: rawValue });
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/get',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path }),
        })
      );
    });

    it('should call resolve endpoint for indexed paths', async () => {
      const mockResponse = 'value';
      mockFetch.mockResolvedValueOnce(mockJsonResponse(mockResponse));

      const path = [-1, '*state*', 'test'];
      const result = await service.get(path, { pinned: false, proof: false });

      expect(result).toEqual(mockResponse);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/resolve',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path, 'pinned?': false, 'proof?': false }),
        })
      );
    });
  });

  describe('getDirectoryEntries', () => {
    it('uses content-only resolve for indexed directory discovery', async () => {
      mockFetch.mockResolvedValueOnce(mockJsonResponse(['directory', {
        file: 'value',
        folder: 'directory',
        '*time*': 'value',
      }, true]));

      const path = [-1, '*bridge*'];
      const result = await service.getDirectoryEntries(path);

      expect(result).toEqual([
        { name: 'file', pathSegment: 'file', type: 'value' },
        { name: 'folder', pathSegment: 'folder', type: 'directory' },
      ]);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/resolve',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path, 'pinned?': false, 'proof?': false }),
        })
      );
    });
  });

  describe('set', () => {
    it('should call set endpoint with value as-is', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const path = ['*state*', 'test'];
      const result = await service.set(path, { '*type/byte-vector*': '0102' });

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/set',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path, value: { '*type/byte-vector*': '0102' } }),
        })
      );
    });

    it('should encode text values as byte-vectors', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const path = ['*state*', 'test'];
      const result = await service.setText(path, 'test value');

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/set',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path, value: { '*type/byte-vector*': '746573742076616c7565' } }),
        })
      );
    });

    it('should call set endpoint with non-string value as-is', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const path = ['*state*', 'test'];
      const result = await service.set(path, true);

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/set',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path, value: true }),
        })
      );
    });
  });

  describe('pin', () => {
    it('should call pin endpoint with path', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const path = [-1, '*state*', 'test'];
      const result = await service.pin(path);

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/pin',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path }),
        })
      );
    });
  });

  describe('unpin', () => {
    it('should call unpin endpoint with path', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const path = [-1, '*state*', 'test'];
      const result = await service.unpin(path);

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/unpin',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path }),
        })
      );
    });
  });

  describe('delete', () => {
    it('should call set with nothing value', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const path = ['*state*', 'test'];
      const result = await service.delete(path);

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/set',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path, value: ['nothing'] }),
        })
      );
    });
  });

  describe('addPeer', () => {
    it('should call bridge with name and local bridge info', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const result = await service.addPeer('peer-name', 'http://peer-endpoint.com');

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/bridge',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            name: 'peer-name',
            'info-local': {
              interface: { '*type/string*': 'http://peer-endpoint.com' },
              policy: { publish: 'push', subscribe: 'pull' },
              role: false,
              'remote-name': 'peer-name',
            },
          }),
        })
      );
    });
  });

  describe('getPeers', () => {
    it('should call config and normalize bridge names', async () => {
      mockFetch.mockResolvedValueOnce(
        mockJsonResponse({
          alice: {},
          bob: {},
        })
      );
      const result = await service.getPeers();
      expect(result).toEqual([
        { name: 'alice', endpoint: '' },
        { name: 'bob', endpoint: '' },
      ]);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/config',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path: ['private', 'bridge'] }),
        })
      );
    });
  });

  describe('admin operations', () => {
    it('should read admin config from admin and config endpoints', async () => {
      mockFetch
        .mockResolvedValueOnce(mockJsonResponse(['alice', 'admin']))
        .mockResolvedValueOnce(
          mockJsonResponse({
            public: { window: 12 },
            private: {
              bridge: {
                peer2: { interface: { '*type/string*': 'http://peer2/api/v1/journal/interface' } },
                peer1: { interface: { '*type/string*': 'http://peer1/api/v1/journal/interface' } },
              },
            },
          })
        );

      const result = await service.getAdminConfig();

      expect(result).toEqual({
        admins: ['admin', 'alice'],
        bridges: [
          { name: 'peer1', endpoint: 'http://peer1/api/v1/journal/interface' },
          { name: 'peer2', endpoint: 'http://peer2/api/v1/journal/interface' },
        ],
        windowSize: 12,
      });
      expect(mockFetch).toHaveBeenNthCalledWith(
        1,
        'http://test-endpoint.com/api/v1/general/admins',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({}),
        })
      );
      expect(mockFetch).toHaveBeenNthCalledWith(
        2,
        'http://test-endpoint.com/api/v1/general/config',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({}),
        })
      );
    });

    it('should replace admins through the admin endpoint', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const result = await service.setAdmins(['admin', 'alice']);

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/set-admins',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ admins: ['admin', 'alice'] }),
        })
      );
    });

    it('should set window size through the admin endpoint', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const result = await service.setWindowSize(32);

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/set-window',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ value: 32 }),
        })
      );
    });
  });

  describe('error handling', () => {
    it('should surface gateway error message on non-ok response', async () => {
      mockFetch.mockResolvedValueOnce(
        mockJsonResponse(
          {
            error: 'authentication-error',
            message: 'Could not authenticate restricted interface call',
          },
          false,
          401,
          'Unauthorized'
        )
      );
      await expect(service.getPeers()).rejects.toThrow(
        'authentication-error: Could not authenticate restricted interface call'
      );
    });

    it('should throw error on network failure', async () => {
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      await expect(service.getSize()).rejects.toThrow('Network error');
    });
  });
});
