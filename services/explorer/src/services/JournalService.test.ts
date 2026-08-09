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

describe('snapshot error classification', () => {
  it('recognizes index and unretained-route errors without classifying authorization errors', () => {
    expect(JournalService.isSnapshotUnavailable({ code: 'index-error', message: 'out of bounds' }))
      .toBe(true);
    expect(JournalService.isSnapshotUnavailable({
      code: 'bridge-error',
      message: 'bridge-error: Bridge is not committed at the selected local index: journal-1 -1',
    })).toBe(true);
    expect(JournalService.isSnapshotUnavailable({
      code: 'authorization-error',
      message: 'authorization-error: Principal is not authorized',
    })).toBe(false);
    expect(JournalService.isSnapshotUnavailable({
      code: 'bridge-error',
      message: 'bridge-error: Bridge is not available: journal-1',
    })).toBe(false);
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
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({}),
        })
      );
    });
  });

  describe('getLocalJournalName', () => {
    it('reads the local public descriptor without federation context', async () => {
      service.setFederationContext({ route: ['journal-3'] });
      mockFetch.mockResolvedValueOnce(mockJsonResponse({
        name: { '*type/string*': 'journal-1' },
      }));

      await expect(service.getLocalJournalName()).resolves.toBe('journal-1');
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/info',
        expect.objectContaining({ method: 'GET', body: undefined }),
      );
    });
  });

  describe('federation context', () => {
    it('uses public route metadata instead of federating size', async () => {
      service.setFederationContext({ route: ['carol', 'bob'] });
      mockFetch.mockResolvedValueOnce(mockJsonResponse({ 'terminal-index': 7 }));

      await expect(service.getSize()).resolves.toBe(8);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/route',
        expect.objectContaining({
          body: JSON.stringify({ 'route-target': ['carol', 'bob'], index: -1 }),
        }),
      );
    });

    it('adds the working route to staged get and set requests', async () => {
      service.setFederationContext({ route: ['bob'] });
      mockFetch
        .mockResolvedValueOnce(mockJsonResponse({ '*type/byte-vector*': '01' }))
        .mockResolvedValueOnce(mockTextResponse('true'));

      await service.get(['*state*', 'doc']);
      await service.set(['*state*', 'doc'], { '*type/byte-vector*': '02' });

      expect(JSON.parse((mockFetch.mock.calls[0][1] as RequestInit).body as string)).toEqual({
        path: ['*state*', 'doc'],
        $federation: { route: ['bob'] },
      });
      expect(JSON.parse((mockFetch.mock.calls[1][1] as RequestInit).body as string)).toEqual({
        path: ['*state*', 'doc'], value: { '*type/byte-vector*': '02' },
        $federation: { route: ['bob'] },
      });
    });

    it('forwards a selected Ledger path without separate federation context', async () => {
      mockFetch.mockResolvedValueOnce(mockJsonResponse({ content: ['directory', { data: 'directory' }, true] }));

      await service.get([400, 'journal-3', -1, '*state*', 'admin'], {
        pinned: false,
        proof: false,
      });

      expect(JSON.parse((mockFetch.mock.calls[0][1] as RequestInit).body as string)).toEqual({
        path: [400, 'journal-3', -1, '*state*', 'admin'],
        'pinned?': false,
        'proof?': false,
      });
    });

    it('uses one canonical path for multihop resolve requests', async () => {
      service.setFederationContext({ route: ['carol', 'bob'], historyIndexes: [-1, 3, 7] });
      mockFetch.mockResolvedValueOnce(mockJsonResponse({ content: 'value' }));

      await service.get([-1, 'carol', 3, 'bob', 7, '*state*', 'doc']);

      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/resolve',
        expect.objectContaining({
          body: JSON.stringify({
            path: [-1, 'carol', 3, 'bob', 7, '*state*', 'doc'],
            'pinned?': true,
            'proof?': true,
          }),
        }),
      );
    });

    it('requests proof-backed Tree-native bytes over a selected route', async () => {
      const value = { '*type/byte-vector*': '255044462d' };
      mockFetch.mockResolvedValueOnce(mockJsonResponse({
        content: value, proof: { graph: true }, 'pinned?': false,
      }));

      const result = await service.get(
        [103, 'journal-2', -1, '*state*', 'alice', 'data', 'public', 'report.pdf'],
      );

      expect(result).toEqual({
        content: value, proof: { graph: true }, 'pinned?': false,
      });
      expect(JSON.parse((mockFetch.mock.calls[0][1] as RequestInit).body as string)).toEqual({
        path: [103, 'journal-2', -1, '*state*', 'alice', 'data', 'public', 'report.pdf'],
        'pinned?': true,
        'proof?': true,
      });
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

    it('returns staged Tree-native bytes without an envelope', async () => {
      const value = { '*type/byte-vector*': '00ff' };
      mockFetch.mockResolvedValueOnce(mockJsonResponse(value));

      const path = ['*state*', 'binary'];
      await expect(service.get(path)).resolves.toEqual({ content: value });
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/get',
        expect.objectContaining({ body: JSON.stringify({ path }) }),
      );
    });

    it('preserves historical content and proof details', async () => {
      const value = { '*type/byte-vector*': '255044462d' };
      mockFetch.mockResolvedValueOnce(mockJsonResponse({
        content: value, proof: { graph: true }, 'pinned?': false,
      }));

      const path = [-1, '*state*', 'report.pdf'];
      await expect(service.get(path)).resolves.toEqual({
        content: value, proof: { graph: true }, 'pinned?': false,
      });
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

    it('retries a newly published index against a briefly older read snapshot', async () => {
      const log = jest.spyOn(console, 'error').mockImplementation(() => undefined);
      mockFetch
        .mockResolvedValueOnce(mockJsonResponse({
          error: 'index-error', message: 'Index is out of bounds: 257', source: 'journal',
        }, false, 400, 'Bad Request'))
        .mockResolvedValueOnce(mockJsonResponse({
          error: 'index-error', message: 'Index is out of bounds: 257', source: 'journal',
        }, false, 400, 'Bad Request'))
        .mockResolvedValueOnce(mockJsonResponse({ content: ['directory', { alice: 'directory' }, true] }));

      await expect(service.get(
        [257, 'journal-1', -2, '*state*', 'alice'],
        { pinned: false, proof: false },
      )).resolves.toEqual({ content: ['directory', { alice: 'directory' }, true] });
      expect(mockFetch).toHaveBeenCalledTimes(3);
      log.mockRestore();
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

  describe('remote local retention', () => {
    it('uses the same full path for pin and unpin', async () => {
      const path = [-1, 'bob', -1, '*state*', 'doc'];
      mockFetch
        .mockResolvedValueOnce(mockTextResponse('true'))
        .mockResolvedValueOnce(mockTextResponse('true'));

      await expect(service.pin(path)).resolves.toBe(true);
      await expect(service.unpin(path)).resolves.toBe(true);

      expect(JSON.parse((mockFetch.mock.calls[0][1] as RequestInit).body as string)).toEqual({ path });
      expect(JSON.parse((mockFetch.mock.calls[1][1] as RequestInit).body as string)).toEqual({ path });
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
            interface: { '*type/string*': 'http://peer-endpoint.com' },
            'remote-name': 'peer-name',
          }),
        })
      );
    });
  });

  describe('getPeers', () => {
    it('should read the public bridge directory', async () => {
      mockFetch.mockResolvedValueOnce(
        mockJsonResponse(['directory', [['alice', 'value'], ['bob', 'value']], true])
      );
      const result = await service.getPeers();
      expect(result).toEqual([
        { name: 'alice', endpoint: '' },
        { name: 'bob', endpoint: '' },
      ]);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/get',
        expect.objectContaining({
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ path: ['*bridge*'] }),
        })
      );
    });

    it('should discover bridges at the terminal working-route peer', async () => {
      service.setFederationContext({ route: ['journal-2', 'journal-3'] });
      mockFetch.mockResolvedValueOnce(
        mockJsonResponse(['directory', [['journal-4', 'directory']], true])
      );

      await expect(service.getBridges('working')).resolves.toEqual([
        { name: 'journal-4', endpoint: '' },
      ]);
      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/get',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            path: ['*bridge*'],
            $federation: { route: ['journal-2', 'journal-3'] },
          }),
        })
      );
    });
  });

  describe('admin operations', () => {
    it('should save reciprocal bridge config', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      const result = await service.saveBridge({
        name: 'peer-name',
        endpoint: 'https://peer.example/api/v1/journal/interface',
        remoteName: 'local-journal',
      });

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
            interface: { '*type/string*': 'https://peer.example/api/v1/journal/interface' },
            'remote-name': 'local-journal',
          }),
        })
      );
    });

    it('should delete reciprocal bridge config', async () => {
      mockFetch.mockResolvedValueOnce(mockTextResponse('true'));

      await expect(service.deleteBridge('peer-name')).resolves.toBe(true);

      expect(mockFetch).toHaveBeenCalledWith(
        'http://test-endpoint.com/api/v1/general/delete-bridge',
        expect.objectContaining({
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ name: 'peer-name' }),
        })
      );
    });

    it('should read admin config from admin and config endpoints', async () => {
      mockFetch
        .mockResolvedValueOnce(mockJsonResponse([['*state*', 'alice'], ['*state*', 'admin']]))
        .mockResolvedValueOnce(
          mockJsonResponse({
            public: {
              window: 12,
              name: { '*type/string*': 'journal-0' },
              'bridge-accept': 'preapproved',
            },
            private: {
              bridge: {
                peer2: {
                  interface: { '*type/string*': 'http://peer2/api/v1/journal/interface' },
                  initiation: 'remote',
                  'remote-name': 'local-two',
                  'last-index': 4,
                },
                peer1: {
                  interface: { '*type/string*': 'http://peer1/api/v1/journal/interface' },
                  initiation: 'local',
                  'remote-name': 'local-one',
                  'last-index': 8,
                  'remote-index': 7,
                },
              },
              'bridge-preapproval': { peer3: { '*type/byte-vector*': 'aabb' } },
            },
          })
        );

      const result = await service.getAdminConfig();

      expect(result).toEqual({
        admins: ['admin', 'alice'],
        bridges: [
          {
            name: 'peer1',
            endpoint: 'http://peer1/api/v1/journal/interface',
            remoteName: 'local-one',
            initiation: 'local',
            lastIndex: 8,
            remoteIndex: 7,
          },
          {
            name: 'peer2',
            endpoint: 'http://peer2/api/v1/journal/interface',
            remoteName: 'local-two',
            initiation: 'remote',
            lastIndex: 4,
          },
        ],
        localName: 'journal-0',
        localEndpoint: null,
        windowSize: 12,
        bridgeAccept: 'preapproved',
        bridgePreapprovals: { peer3: { '*type/byte-vector*': 'aabb' } },
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
          body: JSON.stringify({ admins: [['*state*', 'admin'], ['*state*', 'alice']] }),
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

  describe('authorization rules', () => {
    it('preserves exact key-index and resolve ranges from object rules', async () => {
      mockFetch.mockResolvedValueOnce(mockJsonResponse([{
        principal: ['peer', '*state*', 'bob'],
        'key-index': [-32, -1],
        path: ['documents'],
        get: true,
        'set!': false,
        resolve: [0, -1],
      }]));

      await expect(service.getAuthorizations(['*state*', 'alice'])).resolves.toEqual([{
        principal: ['peer', '*state*', 'bob'],
        'key-index': [-32, -1],
        path: ['documents'],
        get: true,
        'set!': false,
        resolve: [0, -1],
      }]);
    });

    it('preserves key-index from alist rules and omits it from local rules', async () => {
      mockFetch.mockResolvedValueOnce(mockJsonResponse([
        [
          ['principal', ['peer', '*state*', 'bob']],
          ['key-index', [-20, -1]],
          ['path', []],
          ['get', true],
          ['set!', true],
          ['resolve', false],
        ],
        {
          principal: ['*state*', 'carol'], path: [], get: true, 'set!': false, resolve: false,
        },
      ]));

      const rules = await service.getAuthorizations('alice');
      expect(rules[0]['key-index']).toEqual([-20, -1]);
      expect(rules[1]).not.toHaveProperty('key-index');
    });

    it('sends the complete exact rule for add and delete', async () => {
      const rule = {
        principal: ['peer', '*state*', 'bob'],
        'key-index': [-32, -1] as [number, number],
        path: ['documents'], get: true, 'set!': false,
        resolve: [0, -1] as [number, number],
      };
      mockFetch
        .mockResolvedValueOnce(mockTextResponse('true'))
        .mockResolvedValueOnce(mockTextResponse('true'));

      await service.authorize(['*state*', 'alice'], rule);
      await service.deauthorize(['*state*', 'alice'], rule);

      const expected = { user: ['*state*', 'alice'], rule };
      expect(JSON.parse((mockFetch.mock.calls[0][1] as RequestInit).body as string)).toEqual(expected);
      expect(JSON.parse((mockFetch.mock.calls[1][1] as RequestInit).body as string)).toEqual(expected);
    });

    it('drops rules with malformed range shapes instead of changing their identity', async () => {
      mockFetch.mockResolvedValueOnce(mockJsonResponse([
        {
          principal: ['peer', '*state*', 'bob'], 'key-index': ['-32', -1],
          path: [], get: true, 'set!': false, resolve: false,
        },
        {
          principal: ['peer', '*state*', 'carol'], 'key-index': [-32, -1],
          path: [], get: true, 'set!': false, resolve: [0, -1],
        },
      ]));

      await expect(service.getAuthorizations('alice')).resolves.toEqual([expect.objectContaining({
        principal: ['peer', '*state*', 'carol'], 'key-index': [-32, -1], resolve: [0, -1],
      })]);
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
