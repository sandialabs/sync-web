/**
 * Service for interacting with the Synchronic Web Gateway API
 */

import { 
  AdminBridge,
  AdminConfig,
  JournalResponse, 
  JournalPath, 
  PeerInfo,
  SchemeString,
  DirectoryResult,
  DirectoryEntry,
  DirectoryEntryType,
  AuthorizationRule,
  FederationContext,
} from '../types';

export interface GatewayChangeEvent {
  id?: number;
  operation: string;
  path?: JournalPath;
  time?: string;
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

  static isReservedStateSegment(value: string): boolean {
    return value.startsWith('*') && value.endsWith('*');
  }

  static isIndexError(error: unknown): boolean {
    return typeof error === 'object' && error !== null
      && 'code' in error && (error as { code?: unknown }).code === 'index-error';
  }

  static isSnapshotUnavailable(error: unknown): boolean {
    if (JournalService.isIndexError(error)) {
      return true;
    }
    if (typeof error !== 'object' || error === null) {
      return false;
    }
    const value = error as { code?: unknown; message?: unknown };
    if (value.code !== 'bridge-error' || typeof value.message !== 'string') {
      return false;
    }
    const prefix = `${value.code}: `;
    const message = value.message.startsWith(prefix)
      ? value.message.slice(prefix.length)
      : value.message;
    return message.startsWith('Bridge is not committed at the selected local index:');
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
    if (!value) {
      return value;
    }
    if (JournalService.isR7RSIdentifier(value)) {
      return value.replace(/%/g, '%25');
    }
    let out = '';
    const initial = /^[A-Za-z!$&*/:<=>?^_~]$/;
    const subsequent = /^[A-Za-z!$&*/:<=>?^_~0-9+\-.@]$/;
    Array.from(value).forEach((char, index) => {
      if ((index === 0 ? initial : subsequent).test(char)) {
        out += char;
      } else if (char.charCodeAt(0) < 128) {
        out += `%${char.charCodeAt(0).toString(16).toUpperCase().padStart(2, '0')}`;
      } else {
        out += encodeURIComponent(char).replace(/%[0-9a-f]{2}/gi, (escape) => escape.toUpperCase());
      }
    });
    return out;
  }

  static decodePathSegment(value: string): string {
    return value.replace(/(?:%[0-9a-fA-F]{2})+/g, (escape) => {
      try {
        return decodeURIComponent(escape);
      } catch {
        return escape;
      }
    });
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
      if (value === 'directory' || value === 'value' || value === 'unknown') {
        return value;
      }
      return 'unknown';
    };

    const normalizeSegment = (value: unknown): { name: string; pathSegment: string } | null => {
      const { value: extracted } = JournalService.extractSchemeValue(value);
      if (typeof extracted !== 'string') {
        return null;
      }
      return {
        name: JournalService.decodePathSegment(extracted),
        pathSegment: extracted,
      };
    };

    const normalizeArrayEntry = (item: unknown): DirectoryEntry | null => {
      if (Array.isArray(item) && item.length >= 2) {
        const segment = normalizeSegment(item[0]);
        return segment ? {
          name: segment.name,
          pathSegment: segment.pathSegment,
          type: normalizeType(item[1]),
        } : null;
      }
      const segment = normalizeSegment(item);
      return segment ? {
        name: segment.name,
        pathSegment: segment.pathSegment,
        type: 'unknown' as const,
      } : null;
    };

    if (content[0] === 'directory') {
      if (content[1] && typeof content[1] === 'object' && !Array.isArray(content[1])) {
        return Object.entries(content[1]).map(([name, type]) => ({
          name: JournalService.decodePathSegment(name),
          pathSegment: name,
          type: normalizeType(type),
        }));
      }
      if (Array.isArray(content[1])) {
        return content[1].map(normalizeArrayEntry).filter((entry): entry is DirectoryEntry => entry !== null);
      }
      return null;
    }

    if (Array.isArray(content[0])) {
      return content[0].map(normalizeArrayEntry).filter((entry): entry is DirectoryEntry => entry !== null);
    }

    return null;
  }

  private static isIndexedPath(path: JournalPath): boolean {
    return typeof path[0] === 'number';
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
        initiation: initiation === 'remote' ? 'remote' : 'local',
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
    federation?: 'working';
  }): Promise<T> {
    const { method, path, args, federation } = input;
    const url = this.buildGatewayUrl(path);
    const headers: Record<string, string> = {};
    let body: string | undefined;

    if (method === 'POST') {
      headers['Content-Type'] = 'application/json';
      body = JSON.stringify({
        ...(args ?? {}),
        ...(federation && this.federation.route.length > 0
          ? { $federation: { route: this.federation.route } }
          : {}),
      });
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
        console.error('Gateway request failed:', {
          status: response.status,
          statusText: response.statusText,
          payload: parsed,
        });
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
      args: { 'route-target': this.federation.route, index: -1 },
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
        name: input.name,
        interface: endpointStr,
        'remote-name': input.remoteName || input.name,
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
      path: '/general/set',
      args: { path, value },
      federation: 'working',
    });
  }

  async setText(path: JournalPath, value: string): Promise<boolean> {
    return this.set(path, JournalService.textToByteVector(value));
  }

  /**
   * Get a Tree-native value and optional Ledger proof/retention details.
   */
  async get(
    path: JournalPath,
    options: { pinned?: boolean; proof?: boolean } = {},
  ): Promise<JournalResponse> {
    const { pinned = true, proof = true } = options;
    const indexedPath = JournalService.isIndexedPath(path);
    if (indexedPath) {
      return this.retryIndexRead(() => this.request<JournalResponse>({
        method: 'POST',
        path: '/general/resolve',
        args: { path, 'pinned?': pinned, 'proof?': proof },
      }));
    }
    const raw = await this.request<unknown>({
      method: 'POST',
      path: '/general/get',
      args: { path },
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
      .filter((entry) => !JournalService.isReservedStateSegment(entry.name));
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
      for (const entry of directoryEntries.filter((item) => !JournalService.isReservedStateSegment(item.name))) {
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
      for (const entry of directoryEntries.filter((item) => !JournalService.isReservedStateSegment(item.name))) {
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
      path: '/general/get',
      args: { path: ['*bridge*'] },
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
    });
    if (!Array.isArray(admins)) {
      return [];
    }
    return admins
      .map((admin) => (Array.isArray(admin) && admin[0] === '*state*' ? String(admin[1]) : String(admin)))
      .sort((left, right) => left.localeCompare(right));
  }

  async setAdmins(admins: string[]): Promise<boolean> {
    return this.request<boolean>({
      method: 'POST',
      path: '/general/set-admins',
      args: { admins: admins.map((admin) => ['*state*', admin]) },
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
    const resolve = JournalService.ruleField(rule, 'resolve');
    const keyIndex = JournalService.ruleField(rule, 'key-index');
    const exactRange = (value: unknown): [number, number] | null => (
      Array.isArray(value)
      && value.length === 2
      && value.every((index) => typeof index === 'number' && Number.isSafeInteger(index))
        ? [value[0] as number, value[1] as number]
        : null
    );
    if (!Array.isArray(principal) || !Array.isArray(path)) return null;
    const resolveRange = exactRange(resolve);
    const authenticationRange = exactRange(keyIndex);
    if (Array.isArray(resolve) && !resolveRange) return null;
    if (keyIndex !== undefined && !authenticationRange) return null;
    return {
      principal: principal as JournalPath,
      ...(authenticationRange ? { 'key-index': authenticationRange } : {}),
      path: path as JournalPath,
      get: JournalService.ruleField(rule, 'get') === true,
      'set!': JournalService.ruleField(rule, 'set!') === true,
      resolve: resolveRange ?? resolve === true,
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
    const [admins, config] = await Promise.all([
      this.getAdmins(),
      this.request<unknown>({
        method: 'POST',
        path: '/general/config',
      }),
    ]);

    const publicConfig = JournalService.getPublicBlock(config);
    const privateConfig = JournalService.asRecord(JournalService.asRecord(config)?.private);
    const bridgeAccept = JournalService.extractSchemeValue(publicConfig?.['bridge-accept']).value;
    return {
      admins,
      bridges: JournalService.extractAdminBridges(config),
      localName: JournalService.extractLocalName(config),
      localEndpoint: JournalService.extractLocalEndpoint(config),
      windowSize: JournalService.extractWindowSize(config),
      bridgeAccept: bridgeAccept === 'preapproved' ? 'preapproved' : 'auto',
      bridgePreapprovals: JournalService.asRecord(privateConfig?.['bridge-preapproval']) ?? {},
    };
  }
}
