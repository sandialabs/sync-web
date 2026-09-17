import { JournalPath, LedgerHop } from '../types';

export const LEDGER_LATEST = 'latest';

export const normalizeSnapshotInput = (value: string, maximum: number): string => {
  const safeMaximum = Math.max(0, maximum);
  const trimmed = value.trim().toLowerCase();
  if (!/^-?\d+$/.test(trimmed)) return String(safeMaximum);
  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isSafeInteger(parsed)) return String(safeMaximum);
  if (parsed >= 0) return String(Math.min(parsed, safeMaximum));
  return String(Math.max(0, safeMaximum + parsed + 1));
};

export const normalizePublicSnapshotInput = (
  value: string,
  maximum: number,
): string | null => {
  const trimmed = value.trim();
  if (!/^-?\d+$/.test(trimmed)) return null;
  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isSafeInteger(parsed)) return null;
  return normalizeSnapshotInput(trimmed, maximum);
};

export const stepSnapshotValue = (
  value: string,
  maximum: number,
  direction: 'older' | 'newer',
): string => {
  const current = Number.parseInt(normalizeSnapshotInput(value, maximum), 10);
  return String(direction === 'older'
    ? Math.max(0, current - 1)
    : Math.min(Math.max(0, maximum), current + 1));
};

export const retainedLedgerRootPath = (
  hops: LedgerHop[],
  rootIndex: number,
): JournalPath => {
  if (hops.length <= 1) {
    return [rootIndex >= 0 ? rootIndex : 0];
  }
  const terminal = hops[hops.length - 1];
  const snapshot = terminal.snapshot.trim().toLowerCase();
  if (snapshot === '' || snapshot === LEDGER_LATEST) {
    return [terminal.maximum ?? -1];
  }
  const parsed = Number.parseInt(snapshot, 10);
  return [Number.isNaN(parsed) ? -1 : parsed];
};

const firstHopToRootIndex = (snapshot: string, rootIndex: number): number => {
  const trimmed = snapshot.trim().toLowerCase();
  if (trimmed === '' || trimmed === LEDGER_LATEST) {
    return rootIndex;
  }

  const parsed = Number.parseInt(trimmed, 10);
  if (Number.isNaN(parsed)) {
    return rootIndex;
  }

  return parsed;
};

const bridgeHopToIndex = (hop: LedgerHop): number => {
  const trimmed = hop.snapshot.trim().toLowerCase();
  if (trimmed === '' || trimmed === LEDGER_LATEST) return hop.maximum ?? -1;
  return Number.parseInt(trimmed, 10);
};

export const buildLedgerRouteBasePath = (
  hops: LedgerHop[],
  rootIndex: number,
): JournalPath => {
  if (hops.length === 0) {
    return [rootIndex];
  }

  const [first, ...rest] = hops;
  const path: JournalPath = [firstHopToRootIndex(first.snapshot, rootIndex)];

  for (const hop of rest) {
    path.push(hop.name, bridgeHopToIndex(hop));
  }

  return path;
};

export const buildLedgerStateRootPath = (
  hops: LedgerHop[],
  rootIndex: number,
): JournalPath => [...buildLedgerRouteBasePath(hops, rootIndex), '*state*'];

export const buildLedgerUserHomePath = (
  hops: LedgerHop[],
  rootIndex: number,
  username: string,
): JournalPath => [
  ...buildLedgerStateRootPath(hops, rootIndex),
  ...(username ? [username] : []),
];

export const buildLedgerBridgesPath = (
  hops: LedgerHop[],
  rootIndex: number,
): JournalPath => [...buildLedgerRouteBasePath(hops, rootIndex), '*bridge*'];
