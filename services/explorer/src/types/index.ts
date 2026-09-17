/**
 * Core types for the Synchronic Web Explorer application
 */

export interface AppState {
  endpoint: string;
  rootIndex: number;
  selectedPath: JournalPath | null;
  expandedNodes: Set<string>;
  isLoading: boolean;
  error: string | null;
}

export interface SchemeString {
  '*type/string*': string;
}

export type JournalPathSegment = number | string | SchemeString;
export type JournalPath = JournalPathSegment[];
export type ExplorerMode = 'stage' | 'ledger' | 'access' | 'admin';

export interface JournalResponse<T = any> {
  content: T;
  'pinned?'?: boolean | JournalPath | null;
  proof?: any;
  indexes?: number[];
}

export interface PeerInfo {
  name: string;
  endpoint: string;
}

export interface AdminBridge {
  name: string;
  endpoint: string;
  remoteName?: string;
  initiation?: 'local' | 'remote';
  lastIndex?: number;
  remoteIndex?: number;
}

export interface AdminConfig {
  admins: string[];
  bridges: AdminBridge[];
  localName: string | null;
  localEndpoint: string | null;
  windowSize: number | null;
  bridgeAccept: 'auto' | 'preapproved';
  bridgePreapprovals: Record<string, unknown>;
}

export interface FederationContext {
  route: string[];
  historyIndexes?: number[];
}

export interface AuthorizationRule {
  principal: JournalPath;
  path: JournalPath;
  'put!': boolean;
  'use!': false | { 'read-only?': boolean };
  'run!': boolean;
  retrieve: boolean | [number, number];
  'key-index'?: [number, number];
}

export interface TreeNode {
  id: string;
  label: string;
  type: 'peer' | 'directory' | 'file' | 'object';
  valueType?: DirectoryEntryType;
  path: JournalPath;
  children?: TreeNode[];
  childrenLoaded?: boolean;
  isPinned?: boolean;
  isLocal?: boolean;
  error?: boolean;
  navigationKind?: 'state' | 'bridges' | 'bridge' | 'index';
}

export interface ExplorerSelection {
  path: JournalPath;
  type: 'directory' | 'file' | 'object';
}

export interface LedgerHop {
  key: string;
  kind: 'local' | 'bridge';
  name: string;
  snapshot: string;
  maximum?: number;
}

export interface HistoryEntry {
  index: number;
  content: any;
  path: JournalPath;
  timestamp?: string;
}

// Journal API types
export interface JournalRequest {
  function: string;
  arguments?: Record<string, any> | any[];
}

export interface SchemeByteVector {
  '*type/byte-vector*': string;
}

// Union type for Scheme wrapped values
export type SchemeValue = SchemeString | SchemeByteVector;

// Directory response structure
export interface DirectoryResult {
  items: string[];
  isComplete: boolean;
}

export type DirectoryEntryType = 'directory' | 'object' | 'value' | 'unknown';

export interface DirectoryEntry {
  name: string;
  type: DirectoryEntryType;
  pathSegment?: JournalPathSegment;
  keyType?: 'integer' | 'symbol' | 'string';
}
