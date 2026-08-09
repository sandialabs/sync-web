import { ExplorerMode, ExplorerSelection, JournalPath, LedgerHop } from '../types';
import { LEDGER_LATEST, buildLedgerStateRootPath } from './ledgerRoute';

const encodeSegments = (segments: string[], isDirectory: boolean): string => {
  const encoded = segments.map((segment) => encodeURIComponent(segment)).join('/');
  if (isDirectory) {
    return `#${encoded}/`;
  }
  return `#${encoded}`;
};

const decodeHashSegments = (hash: string): { segments: string[]; isDirectory: boolean } => {
  const raw = hash.startsWith('#') ? hash.slice(1) : hash;
  const isDirectory = raw.endsWith('/');
  const trimmed = raw.replace(/^\/+/, '').replace(/\/+$/, '');
  if (!trimmed) {
    return { segments: [], isDirectory };
  }

  return {
    segments: trimmed.split('/').map((segment) => decodeURIComponent(segment)),
    isDirectory,
  };
};

const buildStagePath = (selection: ExplorerSelection | null): JournalPath => {
  if (!selection) {
    return ['*state*'];
  }
  return selection.path;
};

const normalizedSnapshot = (snapshot: string): string => {
  const trimmed = snapshot.trim().toLowerCase();
  return trimmed === '' || trimmed === LEDGER_LATEST ? LEDGER_LATEST : snapshot;
};

const buildStageFragment = (
  selection: ExplorerSelection | null,
  ledgerHops: LedgerHop[],
): string => {
  const path = buildStagePath(selection);
  const suffix = path[0] === '*state*' ? path.slice(1).map(String) : [];
  if (ledgerHops.length <= 1) {
    return encodeSegments(['stage', ...suffix], selection?.type !== 'file');
  }

  const segments = ['stage-route', normalizedSnapshot(ledgerHops[0].snapshot)];
  ledgerHops.slice(1).forEach((hop) => {
    segments.push('bridge', hop.name, normalizedSnapshot(hop.snapshot));
  });
  segments.push('state', ...suffix);
  return encodeSegments(segments, selection?.type !== 'file');
};

const getLedgerRootSnapshot = (hop: LedgerHop, rootIndex: number): string => {
  const trimmed = hop.snapshot.trim().toLowerCase();
  if (trimmed === '' || trimmed === 'latest') {
    return rootIndex >= 0 ? String(rootIndex) : '0';
  }
  return hop.snapshot;
};

const lastStateIndex = (path: JournalPath): number => path.lastIndexOf('*state*');

const buildLedgerStateSuffix = (selection: ExplorerSelection | null, ledgerRootPath: JournalPath): string[] => {
  const targetPath = selection?.path ?? ledgerRootPath;
  const targetState = lastStateIndex(targetPath);
  const rootState = lastStateIndex(ledgerRootPath);

  if (targetState < 0 || rootState < 0) {
    return [];
  }

  return targetPath.slice(targetState + 1).map(String);
};

const buildLedgerFragment = (
  selection: ExplorerSelection | null,
  ledgerRootPath: JournalPath,
  ledgerHops: LedgerHop[],
  rootIndex: number,
): string => {
  const segments = ['ledger', getLedgerRootSnapshot(ledgerHops[0], rootIndex)];

  for (const hop of ledgerHops.slice(1)) {
    segments.push('bridge', hop.name);
    const trimmed = hop.snapshot.trim().toLowerCase();
    if (trimmed !== '' && trimmed !== 'latest') {
      segments.push(hop.snapshot);
    }
  }

  segments.push('state', ...buildLedgerStateSuffix(selection, ledgerRootPath));
  return encodeSegments(segments, selection?.type !== 'file');
};

export const buildFragmentHash = (input: {
  mode: ExplorerMode;
  stageSelection: ExplorerSelection | null;
  ledgerSelection: ExplorerSelection | null;
  ledgerRootPath: JournalPath;
  ledgerHops: LedgerHop[];
  rootIndex: number;
}): string => {
  if (input.mode === 'admin') {
    return '#admin';
  }

  if (input.mode === 'access') {
    return '#access';
  }

  if (input.mode === 'stage') {
    return buildStageFragment(input.stageSelection, input.ledgerHops);
  }

  return buildLedgerFragment(
    input.ledgerSelection,
    input.ledgerRootPath,
    input.ledgerHops,
    input.rootIndex,
  );
};

export const buildProjectedPathDisplay = (input: {
  mode: ExplorerMode;
  stageSelection: ExplorerSelection | null;
  ledgerSelection: ExplorerSelection | null;
  ledgerRootPath: JournalPath;
  ledgerHops: LedgerHop[];
  rootIndex: number;
}): string => {
  const hash = buildFragmentHash(input);
  const raw = hash.startsWith('#') ? hash.slice(1) : hash;
  return raw ? `/${raw}` : '/';
};

const canonicalInteger = (value: string): number | null => {
  if (!/^-?(0|[1-9]\d*)$/.test(value) || value === '-0') return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : null;
};

const isRootSnapshot = (value: string): boolean =>
  value === LEDGER_LATEST || (canonicalInteger(value) ?? -1) >= 0;

const isBridgeSnapshot = (value: string): boolean =>
  value === LEDGER_LATEST || (canonicalInteger(value) ?? 0) < 0;

const parseStageFragment = (segments: string[], isDirectory: boolean) => {
  if (segments[0] === 'stage') {
    return {
      mode: 'stage' as const,
      selection: {
        path: ['*state*', ...segments.slice(1)] as JournalPath,
        type: isDirectory || segments.length === 1 ? 'directory' as const : 'file' as const,
      },
    };
  }

  if (segments.length < 6 || !isRootSnapshot(segments[1])) return null;
  const hops: LedgerHop[] = [{
    key: 'local',
    kind: 'local',
    name: 'Self',
    snapshot: segments[1],
  }];
  let cursor = 2;

  while (cursor < segments.length && segments[cursor] === 'bridge') {
    if (cursor + 2 >= segments.length || !isBridgeSnapshot(segments[cursor + 2])) return null;
    const name = segments[cursor + 1];
    hops.push({
      key: `${name}-${hops.length}`,
      kind: 'bridge',
      name,
      snapshot: segments[cursor + 2],
    });
    cursor += 3;
  }

  if (hops.length === 1 || segments[cursor] !== 'state') return null;
  const suffix = segments.slice(cursor + 1);
  return {
    mode: 'stage' as const,
    ledgerHops: hops,
    selection: {
      path: ['*state*', ...suffix] as JournalPath,
      type: isDirectory || suffix.length === 0 ? 'directory' as const : 'file' as const,
    },
  };
};

const parseLedgerFragment = (segments: string[], isDirectory: boolean) => {
  let cursor = 1;
  let rootSnapshot = '0';
  const hops: LedgerHop[] = [];

  if (cursor < segments.length && !Number.isNaN(Number(segments[cursor]))) {
    rootSnapshot = segments[cursor];
    cursor++;
  }

  hops.push({
    key: 'local',
    kind: 'local',
    name: 'Self',
    snapshot: rootSnapshot,
  });

  const path: JournalPath = [Number.parseInt(rootSnapshot, 10)];

  while (cursor < segments.length) {
    const segment = segments[cursor];
    if (segment === 'state') {
      const suffix = segments.slice(cursor + 1);
      path.push('*state*', ...suffix);
      return {
        mode: 'ledger' as const,
        ledgerHops: hops,
        selection: {
          path,
          type: isDirectory || suffix.length === 0 ? 'directory' as const : 'file' as const,
        },
      };
    }

    if (segment !== 'bridge' || cursor + 1 >= segments.length) {
      return null;
    }

    const bridgeName = segments[cursor + 1];
    cursor += 2;
    let snapshot = 'latest';
    if (cursor < segments.length && !Number.isNaN(Number(segments[cursor]))) {
      snapshot = segments[cursor];
      cursor++;
    }

    hops.push({
      key: `${bridgeName}-${hops.length}`,
      kind: 'bridge',
      name: bridgeName,
      snapshot,
    });
    path.push(bridgeName, snapshot === 'latest' ? -1 : Number.parseInt(snapshot, 10));
  }

  return null;
};

export const parseProjectedFragment = (hash: string) => {
  const { segments, isDirectory } = decodeHashSegments(hash);
  if (segments.length === 0) {
    return null;
  }

  if (segments[0] === 'admin') {
    return { mode: 'admin' as const };
  }

  if (segments[0] === 'access') {
    return { mode: 'access' as const };
  }

  if (segments[0] === 'stage' || segments[0] === 'stage-route') {
    return parseStageFragment(segments, isDirectory);
  }

  if (segments[0] === 'ledger') {
    return parseLedgerFragment(segments, isDirectory);
  }

  return null;
};

export const defaultLedgerHops = (_rootIndex: number): LedgerHop[] => [{
  key: 'local',
  kind: 'local',
  name: 'Self',
  snapshot: LEDGER_LATEST,
}];

export const getInitialLedgerHops = defaultLedgerHops;

export const defaultLedgerRootPath = (rootIndex: number): JournalPath =>
  buildLedgerStateRootPath(defaultLedgerHops(rootIndex), rootIndex);

export const parseFragmentHash = parseProjectedFragment;
