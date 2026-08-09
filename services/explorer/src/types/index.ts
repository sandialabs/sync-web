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

export type JournalPath = Array<number | string>;
export type ExplorerMode = 'stage' | 'ledger' | 'access' | 'admin';

export interface JournalResponse<T = any> {
  content: T;
  'pinned?'?: boolean | JournalPath | null;
  proof?: any;
}

export interface PeerInfo {
  name: string;
  endpoint: string;
}

export interface AdminBridge {
  name: string;
  endpoint: string;
  remoteName?: string;
  initiation: 'local' | 'remote';
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
  get: boolean;
  'set!': boolean;
  resolve: boolean | [number, number];
  'key-index'?: [number, number];
}

export interface TreeNode {
  id: string;
  label: string;
  type: 'peer' | 'directory' | 'file';
  valueType?: DirectoryEntryType;
  path: JournalPath;
  children?: TreeNode[];
  isPinned?: boolean;
  isLocal?: boolean;
}

export interface ExplorerSelection {
  path: JournalPath;
  type: 'directory' | 'file';
}

export interface LedgerHop {
  key: string;
  kind: 'local' | 'bridge';
  name: string;
  snapshot: string;
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

export interface SchemeString {
  '*type/string*': string;
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

export type DirectoryEntryType = 'directory' | 'value' | 'unknown';

export interface DirectoryEntry {
  name: string;
  type: DirectoryEntryType;
  pathSegment?: string;
}
