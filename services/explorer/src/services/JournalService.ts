/**
 * Service for interacting with the Synchronic Web Gateway API
 */

import { 
  AdminBridge,
  AdminConfig,
  JournalResponse, 
  JournalPath,
  JournalPathSegment,
  PeerInfo,
  SchemeString,
  DirectoryResult,
  DirectoryEntry,
  DirectoryEntryType,
  AuthorizationRule,
  FederationContext,
} from '../types';
import { pathSegmentIdentity } from '../utils/pathUtils';
import { decodeSafeName, encodeSafeName } from '../utils/nameCodec';
import { rawSelectionTokenWithinLimit } from '../utils/rawUrl';

export interface GatewayChangeEvent {
  id?: number;
  operation: string;
  path?: JournalPath;
  time?: string;
}

export type PutStorageMode = 'string' | 'bytes' | 'expression' | 'object';

export interface ResourcePutInput {
  mode: PutStorageMode;
  textValue?: string;
  schemeValue?: string;
}

export interface ObjectInvocationResult {
  operation: 'use!' | 'retrieve';
  context: FederationContext;
  path: JournalPath;
  readOnly: boolean;
  result: unknown;
}

interface GatewayErrorPayload {
  error?: string;
  message?: string;
  details?: unknown;
  hints?: unknown;
  source?: string;
}

class GatewayRequestError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details?: unknown;
  readonly hints?: unknown;
  readonly source?: string;

  constructor(input: {
    status: number;
    code: string;
    message: string;
    details?: unknown;
    hints?: unknown;
    source?: string;
  }) {
    super(input.message);
    this.name = 'GatewayRequestError';
    this.status = input.status;
    this.code = input.code;
    this.details = input.details;
    this.hints = input.hints;
    this.source = input.source;
  }
}

export class JournalService {
  private endpointBase: string;
  private federation: FederationContext = { route: [] };

  constructor(endpoint: string) {
    this.endpointBase = endpoint.replace(/\/+$/, '');
  }

  setFederationContext(context: FederationContext): void {
    this.federation = {
      route: [...context.route],
      ...(context.historyIndexes ? { historyIndexes: [...context.historyIndexes] } : {}),
    };
  }

  getFederationContext(): FederationContext {
    return {
      route: [...this.federation.route],
      ...(this.federation.historyIndexes
        ? { historyIndexes: [...this.federation.historyIndexes] }
        : {}),
    };
  }

  /**
   * Extract the actual value from Scheme type wrappers
   * Returns the unwrapped value and the type name if it was wrapped
   */
  static extractSchemeValue(value: any): { value: any; schemeType: string | null } {
    if (value && typeof value === 'object' && !Array.isArray(value)) {
      if ('*type/string*' in value) {
        return { value: value['*type/string*'], schemeType: 'string' };
      }
      if ('*type/byte-vector*' in value) {
        return { value: value['*type/byte-vector*'], schemeType: 'byte-vector' };
      }
    }
    return { value, schemeType: null };
  }

  static textToByteVector(value: string): { '*type/byte-vector*': string } {
    const escaped = encodeURIComponent(value);
    let hex = '';
    for (let i = 0; i < escaped.length; i += 1) {
      if (escaped[i] === '%') {
        hex += escaped.slice(i + 1, i + 3).toLowerCase();
        i += 2;
      } else {
        hex += escaped.charCodeAt(i).toString(16).padStart(2, '0');
      }
    }
    return { '*type/byte-vector*': hex };
  }

  static byteVectorToText(value: unknown): string | null {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      return null;
    }
    const encoded = (value as Record<string, unknown>)['*type/byte-vector*'];
    if (typeof encoded !== 'string') {
      return null;
    }
    if (encoded.length % 2 !== 0 || /[^0-9a-f]/i.test(encoded)) {
      return null;
    }
    const escaped = encoded.match(/.{1,2}/g)?.map((chunk) => `%${chunk}`).join('') ?? '';
    try {
      return decodeURIComponent(escaped);
    } catch {
      return null;
    }
  }

  static documentContentToText(value: unknown): string {
    const decoded = JournalService.byteVectorToText(value);
    if (decoded !== null) {
      return decoded;
    }
    const { value: extracted } = JournalService.extractSchemeValue(value);
    return typeof extracted === 'string' ? extracted : JSON.stringify(extracted, null, 2);
  }

  private static asciiJson(value: unknown): string {
    const json = JSON.stringify(value);
    let encoded = '';
    for (let index = 0; index < json.length; index += 1) {
      const code = json.charCodeAt(index);
      encoded += code > 0x7e
        ? `\\u${code.toString(16).padStart(4, '0')}`
        : json[index];
    }
    return encoded;
  }

  private static escapeSchemeString(value: string): string {
    return value.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  }

  static pathToScheme(path: JournalPath): string {
    const segments = path.map((segment) => {
      if (typeof segment === 'number') return String(segment);
      if (typeof segment === 'string') {
        if (!JournalService.isR7RSIdentifier(segment)) {
          throw new Error(`Path segment is not a Scheme identifier: ${segment}`);
        }
        return segment;
      }
      return `"${JournalService.escapeSchemeString(segment['*type/string*'])}"`;
    });
    return `(${segments.join(' ')})`;
  }

  static isReservedStateSegment(value: string): boolean {
    return value.startsWith('*') && value.endsWith('*');
  }

  private static isReservedStatePathSegment(value: JournalPathSegment | undefined): boolean {
    return typeof value === 'string' && JournalService.isReservedStateSegment(value);
  }

  static pathSegmentIdentity(value: JournalPathSegment): string {
    return pathSegmentIdentity(value);
  }

  static isIndexError(error: unknown): boolean {
    return typeof error === 'object' && error !== null
      && 'code' in error && (error as { code?: unknown }).code === 'index-error';
  }

  static isSnapshotUnavailable(error: unknown): boolean {
    if (typeof error !== 'object' || error === null || !('code' in error)) {
      return false;
    }
    const code = (error as { code?: unknown }).code;
    return code === 'index-error' || code === 'bridge-index-error';
  }

  static isR7RSIdentifier(value: string): boolean {
    const initial = /^[A-Za-z!$%&*/:<=>?^_~]$/;
    const subsequent = /^[A-Za-z!$%&*/:<=>?^_~0-9+\-.@]$/;
    if (value === '+' || value === '-' || value === '...') {
      return true;
    }
    if (value.startsWith('->')) {
      return Array.from(value.slice(2)).every((char) => subsequent.test(char));
    }
    if (!value || !initial.test(value[0])) {
      return false;
    }
    return Array.from(value.slice(1)).every((char) => subsequent.test(char));
  }

  static encodePathSegment(value: string): string {
    return encodeSafeName(value);
  }

  static decodePathSegment(value: string): string {
    return decodeSafeName(value);
  }

  /**
   * Parse a directory response into a normalized structure
   * Returns null if the content is not a directory
   */
  static parseDirectoryResponse(content: any): DirectoryResult | null {
    if (!Array.isArray(content) || content.length < 2) {
      return null;
    }

    let items: any[] = [];
    let isComplete = true;

    if (content[0] === 'directory') {
      // Current authoritative format: ["directory", { "name": "type" }, true]
      if (content[1] && typeof content[1] === 'object' && !Array.isArray(content[1])) {
        items = Object.keys(content[1]);
        isComplete = content[2] !== false;
      } else if (Array.isArray(content[1])) {
        // Backward compatibility for older format: ["directory", ["a", "b"], true]
        items = content[1];
        isComplete = content[2] !== false;
      } else {
        return null;
      }
    } else if (Array.isArray(content[0])) {
      // Format: [[items...], isComplete]
      items = content[0];
      isComplete = content[1] !== false;
    } else {
      return null;
    }

    // Extract string values from Scheme string objects
    const normalizedItems = items.map(item => {
      const { value } = JournalService.extractSchemeValue(item);
      return typeof value === 'string' ? value : String(value);
    });

    return { items: normalizedItems, isComplete };
  }

  /**
   * Parse directory entries with explicit child-type metadata.
   * Returns null when content is not a directory payload.
   */
  static parseDirectoryEntries(content: any): DirectoryEntry[] | null {
    if (!Array.isArray(content) || content.length < 2) {
      return null;
    }

    const normalizeType = (value: unknown): DirectoryEntryType => {
      if (value === 'directory' || value === 'object' || value === 'value' || value === 'unknown') {
        return value;
      }
      return 'unknown';
    };

    const normalizeSegment = (value: unknown): {
      name: string;
      pathSegment: JournalPathSegment;
      keyType: NonNullable<DirectoryEntry['keyType']>;
    } | null => {
      if (typeof value === 'number' && Number.isInteger(value)) {
        return { name: String(value), pathSegment: value, keyType: 'integer' };
      }
      if (typeof value === 'string') {
        return {
          name: JournalService.decodePathSegment(value),
          pathSegment: value,
          keyType: 'symbol',
        };
      }
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        const wrapped = (value as Record<string, unknown>)['*type/string*'];
        if (typeof wrapped === 'string') {
          return { name: wrapped, pathSegment: { '*type/string*': wrapped }, keyType: 'string' };
        }
      }
      return null;
    };

    const normalizeArrayEntry = (item: unknown): DirectoryEntry | null => {
      if (Array.isArray(item) && item.length >= 2) {
        const segment = normalizeSegment(item[0]);
        return segment ? { ...segment, type: normalizeType(item[1]) } : null;
      }
      const segment = normalizeSegment(item);
      return segment ? { ...segment, type: 'unknown' as const } : null;
    };

    let entries: DirectoryEntry[] | null = null;
    if (content[0] === 'directory') {
      if (content[1] && typeof content[1] === 'object' && !Array.isArray(content[1])) {
        entries = Object.entries(content[1]).map(([name, type]) => ({
          name: JournalService.decodePathSegment(name),
          pathSegment: name,
          keyType: 'symbol',
          type: normalizeType(type),
        }));
      } else if (Array.isArray(content[1])) {
        entries = content[1]
          .map(normalizeArrayEntry)
          .filter((entry): entry is DirectoryEntry => entry !== null);
      }
    } else if (Array.isArray(content[0])) {
      entries = content[0]
        .map(normalizeArrayEntry)
        .filter((entry): entry is DirectoryEntry => entry !== null);
    }
    if (!entries) {
      return null;
    }

    entries = entries.filter((entry) => !JournalService.isReservedStatePathSegment(entry.pathSegment));
    const nameCounts = new Map<string, number>();
    entries.forEach((entry) => nameCounts.set(entry.name, (nameCounts.get(entry.name) ?? 0) + 1));
    return entries.map((entry) => {
      if (nameCounts.get(entry.name)! <= 1) {
        return entry;
      }
      const name = entry.keyType === 'integer'
        ? `${entry.name} [integer]`
        : entry.keyType === 'string'
          ? JSON.stringify(entry.name)
          : entry.name;
      return { ...entry, name };
    });
  }

  private static isIndexedPath(path: JournalPath): boolean {
    return typeof path[0] === 'number';
  }

  private isRetainedProviderRead(path: JournalPath): boolean {
    if (this.federation.route.length === 0 || !JournalService.isIndexedPath(path)) {
      return false;
    }
    const firstResource = path[1];
    return firstResource === '*bridge*'
      || (typeof firstResource === 'string'
        && !JournalService.isReservedStatePathSegment(firstResource));
  }

  private static getBridgeBlock(config: unknown): Record<string, unknown> | null {
    if (!config || typeof config !== 'object' || Array.isArray(config)) {
      return null;
    }

    const rootObject = config as Record<string, unknown>;
    const bridgeBlock =
      rootObject.private && typeof rootObject.private === 'object' && !Array.isArray(rootObject.private)
        ? (rootObject.private as Record<string, unknown>).bridge
        : config;
    if (!bridgeBlock || typeof bridgeBlock !== 'object' || Array.isArray(bridgeBlock)) {
      return null;
    }

    return bridgeBlock as Record<string, unknown>;
  }

  private static asRecord(value: unknown): Record<string, unknown> | null {
    return value && typeof value === 'object' && !Array.isArray(value)
      ? value as Record<string, unknown>
      : null;
  }

  private static extractBridgeEndpoint(value: unknown): string {
    const bridge = JournalService.asRecord(value);
    const { value: endpoint } = JournalService.extractSchemeValue(bridge?.interface);
    return typeof endpoint === 'string' ? endpoint : '';
  }

  private static extractRemoteName(input: unknown): string | undefined {
    const bridge = JournalService.asRecord(input);
    const { value } = JournalService.extractSchemeValue(bridge?.['remote-name']);
    return typeof value === 'string' ? value : undefined;
  }

  private static extractAdminBridges(config: unknown): AdminBridge[] {
    const block = JournalService.getBridgeBlock(config);
    if (!block) return [];
    return Object.entries(block).map(([name, value]): AdminBridge => {
      const bridge = JournalService.asRecord(value);
      const initiation = JournalService.extractSchemeValue(bridge?.initiation).value;
      return {
        name,
        endpoint: JournalService.extractBridgeEndpoint(value),
        remoteName: JournalService.extractRemoteName(value),
        ...(initiation === 'local' || initiation === 'remote' ? { initiation } : {}),
        ...(typeof bridge?.['last-index'] === 'number'
          ? { lastIndex: bridge['last-index'] as number } : {}),
        ...(typeof bridge?.['remote-index'] === 'number'
          ? { remoteIndex: bridge['remote-index'] as number } : {}),
      };
    }).sort((left, right) => left.name.localeCompare(right.name));
  }

  private static getPublicBlock(config: unknown): Record<string, unknown> | null {
    const rootObject = JournalService.asRecord(config);
    return JournalService.asRecord(rootObject?.public);
  }

  private static extractLocalName(config: unknown): string | null {
    const { value } = JournalService.extractSchemeValue(JournalService.getPublicBlock(config)?.name);
    return typeof value === 'string' ? value : null;
  }

  private static extractLocalEndpoint(config: unknown): string | null {
    const publicConfig = JournalService.getPublicBlock(config);
    const interfaceConfig = JournalService.asRecord(publicConfig?.interface);
    const { value } = JournalService.extractSchemeValue(interfaceConfig?.endpoint);
    return typeof value === 'string' ? value : null;
  }

  private static extractWindowSize(config: unknown): number | null {
    const value = JournalService.getPublicBlock(config)?.window;
    return typeof value === 'number' ? value : null;
  }

  private buildGatewayUrl(path: string): string {
    const suffix = path.startsWith('/') ? path : `/${path}`;
    return `${this.endpointBase}${suffix}`;
  }

  rawUrl(selection: string): string | null {
    return rawSelectionTokenWithinLimit(selection)
      ? `${this.buildGatewayUrl('/raw')}?selection=${selection}`
      : null;
  }

  private parseGatewayError(status: number, payload: unknown): GatewayRequestError {
    const objectPayload =
      payload && typeof payload === 'object' && !Array.isArray(payload)
        ? (payload as GatewayErrorPayload)
        : undefined;
    const code = objectPayload?.error || `http_${status}`;
    const baseMessage =
      objectPayload?.message ||
      (typeof payload === 'string' ? payload : `Gateway request failed (${status})`);
    const message = `${code}: ${baseMessage}`;
    return new GatewayRequestError({
      status,
      code,
      message,
      details: objectPayload?.details,
      hints: objectPayload?.hints,
      source: objectPayload?.source,
    });
  }

  private async request<T = any>(input: {
    method: 'GET' | 'POST';
    path: string;
    args?: Record<string, any>;
    schemeArgs?: string;
    federation?: 'working';
    expectedErrorStatuses?: number[];
  }): Promise<T> {
    const { method, path, args, schemeArgs, federation, expectedErrorStatuses = [] } = input;
    const url = this.buildGatewayUrl(path);
    const headers: Record<string, string> = {};
    let body: string | undefined;

    if (method === 'POST') {
      if (schemeArgs !== undefined) {
        if (args !== undefined) throw new Error('A request cannot mix JSON and Scheme arguments');
        headers['Content-Type'] = 'application/scheme';
        if (federation && this.federation.route.length > 0) {
          headers['X-Sync-Web-Federation-Route'] = JournalService.asciiJson(this.federation.route);
        }
        body = schemeArgs;
      } else {
        headers['Content-Type'] = 'application/json';
        body = JSON.stringify({
          ...(args ?? {}),
          ...(federation && this.federation.route.length > 0
            ? { $federation: { route: this.federation.route } }
            : {}),
        });
      }
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 30000);

    try {
      const response = await fetch(url, {
        method,
        headers,
        body,
        signal: controller.signal,
      });

      const raw = await response.text();
      let parsed: unknown = raw;
      if (raw) {
        try {
          parsed = JSON.parse(raw);
        } catch {
          // Keep plain-text payloads as-is.
        }
      }

      clearTimeout(timeoutId);

      if (!response.ok) {
        if (!expectedErrorStatuses.includes(response.status)) {
          console.error('Gateway request failed:', {
            status: response.status,
            statusText: response.statusText,
            payload: parsed,
          });
        }
        throw this.parseGatewayError(response.status, parsed);
      }
      return parsed as T;
    } catch (error) {
      clearTimeout(timeoutId);
      if (error instanceof Error && error.name === 'AbortError') {
        throw new Error('Request timeout: The gateway did not respond in time');
      }
      throw error;
    }
  }

  private async retryIndexRead<T>(read: () => Promise<T>): Promise<T> {
    for (let attempt = 0; attempt < 3; attempt += 1) {
      try {
        return await read();
      } catch (error) {
        if (!JournalService.isIndexError(error) || attempt === 2) {
          throw error;
        }
        // Size and resolve use separate Journal snapshots. A just-published
        // latest index can briefly precede the snapshot seen by the next read.
        await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
      }
    }
    throw new Error('Indexed read retry exhausted');
  }

  subscribeEvents(input: {
    onChange: (event: GatewayChangeEvent) => void;
    onError?: (event: Event) => void;
  }): () => void {
    const source = new EventSource(this.buildGatewayUrl('/events'), { withCredentials: true });
    source.addEventListener('sync-web-change', (message) => {
      try {
        input.onChange(JSON.parse((message as MessageEvent).data) as GatewayChangeEvent);
      } catch {
        input.onChange({ operation: 'unknown' });
      }
    });
    if (input.onError) {
      source.addEventListener('error', input.onError);
    }
    return () => source.close();
  }

  async getLocalJournalName(): Promise<string> {
    const info = await this.request<Record<string, unknown>>({
      method: 'GET',
      path: '/general/info',
    });
    const name = JournalService.extractSchemeValue(info?.name).value;
    if (typeof name !== 'string' || name.length === 0) {
      throw new Error('Journal info did not return a name');
    }
    return name;
  }

  /**
   * Get current size of the ledger
   */
  async getLocalSize(): Promise<number> {
    return this.request<number>({ method: 'POST', path: '/general/size' });
  }

  async getSize(): Promise<number> {
    if (this.federation.route.length === 0) {
      return this.getLocalSize();
    }
    const routed = await this.request<Record<string, unknown>>({
      method: 'POST',
      path: '/general/route',
      args: {
        'route-target': this.federation.route,
        ...(this.federation.historyIndexes
          ? { 'history-indexes': this.federation.historyIndexes }
          : {}),
        index: -1,
      },
    });
    const terminalIndex = routed?.['terminal-index'];
    if (typeof terminalIndex !== 'number') {
      throw new Error('Federation route did not return a terminal index');
    }
    return terminalIndex + 1;
  }

  /**
   * Add a new bridge
   */
  async saveBridge(input: {
    name: string;
    endpoint: string;
    remoteName?: string;
  }): Promise<boolean> {
    const endpointStr: SchemeString = { '*type/string*': input.endpoint };
    return this.request<boolean>({
      method: 'POST',
      path: '/general/bridge',
      args: {
        name: encodeSafeName(input.name),
        interface: endpointStr,
        'remote-name': encodeSafeName(input.remoteName || input.name),
      },
    });
  }

  async addBridge(name: string, endpoint: string): Promise<boolean> {
    return this.saveBridge({ name, endpoint, remoteName: name });
  }

  async deleteBridge(name: string): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/delete-bridge',
      args: { name },
    });
  }

  async updateConfig(path: JournalPath, value: unknown): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/update-config',
      args: { path, value },
    });
  }

  /**
   * Set data at path to the new value
   */
  async set(path: JournalPath, value: any): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/put',
      args: { path, value },
      federation: 'working',
    });
  }

  async setText(path: JournalPath, value: string): Promise<boolean> {
    return this.set(path, JournalService.textToByteVector(value));
  }

  async putResource(path: JournalPath, input: ResourcePutInput): Promise<boolean> {
    if (input.mode === 'string') {
      return this.request<boolean>({
        method: 'POST',
        path: '/general/put',
        args: {
          path,
          value: JournalService.textToByteVector(input.textValue ?? ''),
          'expression?': false,
          'object?': false,
          expected: ['nothing'],
        },
        federation: 'working',
      });
    }

    if (!input.schemeValue?.trim()) throw new Error('Scheme body is required');
    const expression = input.mode !== 'bytes';
    const object = input.mode === 'object';
    const schemeArgs = `((path ${JournalService.pathToScheme(path)}) `
      + `(value ${input.schemeValue}) (expression? ${expression ? '#t' : '#f'}) `
      + `(object? ${object ? '#t' : '#f'}) (expected (nothing)))`;
    const result = await this.request<unknown>({
      method: 'POST',
      path: '/general/put',
      schemeArgs,
      federation: 'working',
    });
    if (result === true || result === '#t') return true;
    if (result === false || result === '#f') return false;
    throw new Error('Gateway returned an invalid create result');
  }

  async probeObjectApi(input: {
    path: JournalPath;
    historical: boolean;
  }): Promise<void> {
    await this.request<unknown>({
      method: 'POST',
      path: input.historical ? '/general/retrieve' : '/general/use',
      args: {
        path: input.path,
        method: '*api*',
        arguments: [],
        ...(input.historical
          ? { 'pinned?': true, 'proof?': false }
          : { 'read-only?': true }),
        'expression?': true,
      },
      federation: 'working',
      expectedErrorStatuses: [400],
    });
  }

  async invokeObject(input: {
    path: JournalPath;
    method: string;
    argumentsExpression: string;
    readOnly: boolean;
    historical: boolean;
  }): Promise<ObjectInvocationResult> {
    const blankCall = input.method === '';
    if (!blankCall && !JournalService.isR7RSIdentifier(input.method)) {
      throw new Error('Method must be one exact Scheme symbol');
    }
    if (!input.argumentsExpression.trim()) {
      throw new Error('Arguments must be one complete Scheme list');
    }
    const operation = input.historical ? 'retrieve' : 'use!';
    const schemeArgs = `((path ${JournalService.pathToScheme(input.path)}) `
      + `${blankCall ? '' : `(method ${input.method}) (arguments ${input.argumentsExpression}) `}`
      + `${input.historical ? '(pinned? #t) (proof? #f) ' : `(read-only? ${input.readOnly ? '#t' : '#f'}) `}`
      + '(expression? #t))';
    const result = await this.request<unknown>({
      method: 'POST',
      path: input.historical ? '/general/retrieve' : '/general/use',
      schemeArgs,
      federation: 'working',
    });
    return {
      operation,
      context: this.getFederationContext(),
      path: [...input.path],
      readOnly: input.historical || input.readOnly,
      result,
    };
  }

  /**
   * Get a Tree-native value and optional Ledger proof/retention details.
   */
  async get(
    path: JournalPath,
    options: { pinned?: boolean; proof?: boolean; selectedIndexes?: boolean } = {},
  ): Promise<JournalResponse> {
    const { pinned = true, proof = true, selectedIndexes = false } = options;
    const indexedPath = JournalService.isIndexedPath(path);
    if (indexedPath) {
      const read = () => this.request<JournalResponse>({
        method: 'POST',
        path: '/general/retrieve',
        args: {
          path,
          'pinned?': pinned,
          'proof?': proof,
          ...(selectedIndexes ? { 'index?': true } : {}),
        },
        federation: 'working',
      });
      return selectedIndexes || this.isRetainedProviderRead(path)
        ? read()
        : this.retryIndexRead(read);
    }
    const raw = await this.request<unknown>({
      method: 'POST',
      path: '/general/use',
      args: { path, 'read-only?': true },
      federation: 'working',
    });
    return { content: raw } as JournalResponse;
  }

  /**
   * Pin the value at the specified path
   */
  async pin(path: JournalPath): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/pin',
      args: { path },
    });
  }

  /**
   * Unpin the value at the specified path
   */
  async unpin(path: JournalPath): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/unpin',
      args: { path },
    });
  }

  /**
   * Delete a document by setting it to ["nothing"]
   */
  async delete(path: JournalPath): Promise<boolean> {
    return this.set(path, ['nothing']);
  }

  private requireStatePath(path: JournalPath): JournalPath {
    if (!path.includes('*state*')) {
      throw new Error('Expected a state path');
    }
    return path;
  }

  private buildStateChildPath(parentPath: JournalPath, childName: string): JournalPath {
    this.requireStatePath(parentPath);
    return [...parentPath, JournalService.encodePathSegment(childName)];
  }

  private buildStateChildPathFromEntry(parentPath: JournalPath, entry: DirectoryEntry): JournalPath {
    this.requireStatePath(parentPath);
    return [...parentPath, entry.pathSegment ?? JournalService.encodePathSegment(entry.name)];
  }

  private buildStateSiblingPath(path: JournalPath, siblingName: string): JournalPath {
    this.requireStatePath(path);
    return [...path.slice(0, -1), JournalService.encodePathSegment(siblingName)];
  }

  private buildDirectoryMarkerPath(path: JournalPath): JournalPath {
    this.requireStatePath(path);
    return [...path, '*directory*'];
  }

  async getDirectoryEntries(path: JournalPath): Promise<DirectoryEntry[]> {
    const response = await this.get(path, { pinned: false, proof: false });
    const content = response && typeof response === 'object' && !Array.isArray(response) && 'content' in response
      ? response.content
      : response;
    return (JournalService.parseDirectoryEntries(content) ?? [])
      .filter((entry) => !JournalService.isReservedStatePathSegment(entry.pathSegment));
  }

  async getChainInventory(path: JournalPath): Promise<{ indexes: number[]; complete: boolean }> {
    const response = await this.get(path, { pinned: false, proof: false });
    const content = response && typeof response === 'object' && !Array.isArray(response) && 'content' in response
      ? response.content
      : response;
    if (!Array.isArray(content) || content.length !== 3 || content[0] !== 'chain'
        || !Array.isArray(content[1]) || !content[1].every(Number.isInteger)
        || typeof content[2] !== 'boolean') {
      throw new Error('Journal did not return a structural Chain inventory');
    }
    return { indexes: content[1] as number[], complete: content[2] };
  }

  async verifyPinnedInventories(path: JournalPath): Promise<void> {
    const stateIndex = path.indexOf('*state*');
    if (stateIndex < 3 || typeof path[0] !== 'number') {
      throw new Error('Pinned route path does not contain a bridge lineage');
    }
    let inventoryPath: JournalPath = [path[0], '*bridge*'];
    let index = 1;
    while (index < stateIndex) {
      if (path[index] === '*bridge*') index += 1;
      const alias = path[index];
      const selectedIndex = path[index + 1];
      if (typeof alias !== 'string' || typeof selectedIndex !== 'number') {
        throw new Error('Pinned route path has an invalid bridge lineage');
      }
      inventoryPath = [...inventoryPath, alias];
      const inventory = await this.getChainInventory(inventoryPath);
      if (!inventory.indexes.includes(selectedIndex)) {
        throw new Error(`Pinned bridge inventory does not contain index ${selectedIndex}`);
      }
      inventoryPath = [...inventoryPath, selectedIndex, '*bridge*'];
      index += 2;
    }
  }

  async createFile(parentPath: JournalPath, fileName: string): Promise<boolean> {
    return this.set(this.buildStateChildPath(parentPath, fileName), JournalService.textToByteVector(''));
  }

  async createDirectory(parentPath: JournalPath, directoryName: string): Promise<boolean> {
    const dirPath = this.buildStateChildPath(parentPath, directoryName);
    return this.ensureDirectory(dirPath);
  }

  async ensureDirectory(path: JournalPath): Promise<boolean> {
    return this.set(this.buildDirectoryMarkerPath(path), JournalService.textToByteVector(''));
  }

  async uploadFile(parentPath: JournalPath, file: File): Promise<boolean> {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
    return this.set(
      this.buildStateChildPath(parentPath, file.name),
      { '*type/byte-vector*': hex },
    );
  }

  async renameStagePath(path: JournalPath, nextName: string): Promise<boolean> {
    const targetPath = this.buildStateSiblingPath(path, nextName);
    await this.copyStagePath(path, targetPath);
    await this.deleteStagePath(path);
    return true;
  }

  async deleteStagePath(path: JournalPath): Promise<boolean> {
    const response = await this.get(path);
    const directoryEntries = JournalService.parseDirectoryEntries(response.content);
    if (directoryEntries) {
      for (const entry of directoryEntries.filter((item) => !JournalService.isReservedStatePathSegment(item.pathSegment))) {
        await this.deleteStagePath(this.buildStateChildPathFromEntry(path, entry));
      }
      await this.delete(this.buildDirectoryMarkerPath(path));
      return true;
    }

    return this.delete(path);
  }

  async download(path: JournalPath): Promise<{ blob: Blob; filename: string }> {
    const response = await this.get(path);
    const stateBlock = this.requireStatePath(path);
    const fallbackName = JournalService.decodePathSegment(String(stateBlock[stateBlock.length - 1] || 'download'));

    if (
      response.content &&
      typeof response.content === 'object' &&
      !Array.isArray(response.content) &&
      '*type/byte-vector*' in response.content
    ) {
      const hex = String(response.content['*type/byte-vector*']);
      const bytes = new Uint8Array(
        hex.match(/.{1,2}/g)?.map((chunk) => Number.parseInt(chunk, 16)) ?? [],
      );
      return {
        blob: new Blob([bytes]),
        filename: fallbackName,
      };
    }

    const { value } = JournalService.extractSchemeValue(response.content);
    const serialized =
      typeof value === 'string' ? value : JSON.stringify(value, null, 2);
    return {
      blob: new Blob([serialized], { type: 'text/plain;charset=utf-8' }),
      filename: fallbackName,
    };
  }

  private async copyStagePath(sourcePath: JournalPath, targetPath: JournalPath): Promise<void> {
    const response = await this.get(sourcePath);
    const directoryEntries = JournalService.parseDirectoryEntries(response.content);

    if (directoryEntries) {
      await this.set(this.buildDirectoryMarkerPath(targetPath), JournalService.textToByteVector(''));
      for (const entry of directoryEntries.filter((item) => !JournalService.isReservedStatePathSegment(item.pathSegment))) {
        await this.copyStagePath(
          this.buildStateChildPathFromEntry(sourcePath, entry),
          this.buildStateChildPathFromEntry(targetPath, entry),
        );
      }
      return;
    }

    await this.set(targetPath, response.content);
  }

  /**
   * Get bridge info
   */
  async getBridges(federation?: 'working'): Promise<PeerInfo[]> {
    const directory = await this.request<unknown>({
      method: 'POST',
      path: '/general/use',
      args: { path: ['*bridge*'], 'read-only?': true },
      federation,
    });
    return (JournalService.parseDirectoryEntries(directory) ?? []).map((entry) => ({
      name: entry.name,
      endpoint: '',
    })).sort((left, right) => left.name.localeCompare(right.name));
  }

  async addPeer(name: string, endpoint: string): Promise<boolean> {
    return this.addBridge(name, endpoint);
  }

  async getPeers(): Promise<PeerInfo[]> {
    return this.getBridges();
  }

  async getAdmins(): Promise<string[]> {
    const admins = await this.request<unknown>({
      method: 'POST',
      path: '/general/admins',
      expectedErrorStatuses: [400],
    });
    if (admins === null) {
      return [];
    }
    if (typeof admins !== 'object' || Array.isArray(admins)) {
      throw new Error('Malformed Interface admin principals');
    }
    const principals = Object.entries(admins);
    if (principals.length === 0
        || !principals.every(([username, principal]) => (
          Array.isArray(principal)
          && principal.length === 2
          && principal[0] === '*state*'
          && typeof principal[1] === 'string'
          && principal[1] === username
        ))) {
      throw new Error('Malformed Interface admin principals');
    }
    return principals.map(([username]) => username);
  }

  async setAdmins(admins: string[]): Promise<boolean> {
    if (new Set(admins).size !== admins.length) {
      throw new Error('Interface admin usernames must be unique');
    }
    return this.request<boolean>({
      method: 'POST',
      path: '/general/set-admins',
      args: {
        admins: Object.fromEntries(
          admins.map((admin) => [admin, ['*state*', admin]]),
        ),
      },
    });
  }

  async setWindowSize(value: number): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/set-window',
      args: { value },
    });
  }

  private static ruleField(rule: unknown, name: string): unknown {
    if (rule && typeof rule === 'object' && !Array.isArray(rule)) {
      return (rule as Record<string, unknown>)[name];
    }
    if (Array.isArray(rule)) {
      const entry = rule.find((item) => Array.isArray(item) && item[0] === name);
      return Array.isArray(entry) ? entry[1] : undefined;
    }
    return undefined;
  }

  private static parseAuthorizationRule(rule: unknown): AuthorizationRule | null {
    const principal = JournalService.ruleField(rule, 'principal');
    const path = JournalService.ruleField(rule, 'path');
    const retrieve = JournalService.ruleField(rule, 'retrieve');
    const use = JournalService.ruleField(rule, 'use!');
    const readOnly = JournalService.ruleField(use, 'read-only?');
    const keyIndex = JournalService.ruleField(rule, 'key-index');
    const exactRange = (value: unknown): [number, number] | null => (
      Array.isArray(value)
      && value.length === 2
      && value.every((index) => typeof index === 'number' && Number.isSafeInteger(index))
        ? [value[0] as number, value[1] as number]
        : null
    );
    const normalizedPath = path === null ? [] : path;
    if (!Array.isArray(principal) || !Array.isArray(normalizedPath)) return null;
    const retrieveRange = exactRange(retrieve);
    const authenticationRange = exactRange(keyIndex);
    if (Array.isArray(retrieve) && !retrieveRange) return null;
    if (!(use === false || typeof readOnly === 'boolean')) return null;
    if (keyIndex !== undefined && !authenticationRange) return null;
    return {
      principal: principal as JournalPath,
      ...(authenticationRange ? { 'key-index': authenticationRange } : {}),
      path: normalizedPath as JournalPath,
      'put!': JournalService.ruleField(rule, 'put!') === true,
      'use!': use === false ? false : { 'read-only?': readOnly as boolean },
      'run!': JournalService.ruleField(rule, 'run!') === true,
      retrieve: retrieveRange ?? retrieve === true,
    };
  }

  private static normalizeAuthorizationUser(user: string | JournalPath): JournalPath {
    return Array.isArray(user) ? user : ['*state*', user];
  }

  async getAuthorizations(user: string | JournalPath): Promise<AuthorizationRule[]> {
    const rules = await this.request<unknown>({
      method: 'POST',
      path: '/general/authorizations',
      args: { user: JournalService.normalizeAuthorizationUser(user) },
    });
    if (!Array.isArray(rules)) return [];
    return rules.map(JournalService.parseAuthorizationRule).filter((rule): rule is AuthorizationRule => rule !== null);
  }

  async authorize(user: string | JournalPath, rule: AuthorizationRule): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/authorize',
      args: { user: JournalService.normalizeAuthorizationUser(user), rule },
    });
  }

  async deauthorize(user: string | JournalPath, rule: AuthorizationRule): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/deauthorize',
      args: { user: JournalService.normalizeAuthorizationUser(user), rule },
    });
  }

  async getAdminConfig(): Promise<AdminConfig> {
    const [admins, ledgerConfig, bridgeConfig, bridgePreapprovals] = await Promise.all([
      this.getAdmins(),
      this.request<unknown>({
        method: 'POST',
        path: '/general/config',
      }),
      this.request<unknown>({
        method: 'POST',
        path: '/general/config',
        args: { path: ['private', 'bridge'] },
      }),
      this.request<unknown>({
        method: 'POST',
        path: '/general/config',
        args: { path: ['private', 'bridge-preapproval'] },
      }),
    ]);

    const publicConfig = JournalService.getPublicBlock(ledgerConfig);
    const bridgeAccept = JournalService.extractSchemeValue(publicConfig?.['bridge-accept']).value;
    return {
      admins,
      bridges: JournalService.extractAdminBridges(bridgeConfig),
      localName: JournalService.extractLocalName(ledgerConfig),
      localEndpoint: JournalService.extractLocalEndpoint(ledgerConfig),
      windowSize: JournalService.extractWindowSize(ledgerConfig),
      bridgeAccept: bridgeAccept === 'preapproved' ? 'preapproved' : 'auto',
      bridgePreapprovals: JournalService.asRecord(bridgePreapprovals) ?? {},
    };
  }
}
