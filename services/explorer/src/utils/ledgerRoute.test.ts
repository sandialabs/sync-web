import { buildLedgerUserHomePath } from './ledgerRoute';
import { LedgerHop } from '../types';

const hops: LedgerHop[] = [
  { key: 'local', kind: 'local', name: 'Self', snapshot: 'latest' },
  { key: 'peer', kind: 'bridge', name: 'peer', snapshot: '-2' },
];

describe('buildLedgerUserHomePath', () => {
  it('starts a routed ledger view at the authenticated user namespace', () => {
    expect(buildLedgerUserHomePath(hops, 12, 'alice')).toEqual([
      12, 'peer', -2, '*state*', 'alice',
    ]);
  });

  it('keeps the state root while session discovery is pending', () => {
    expect(buildLedgerUserHomePath(hops.slice(0, 1), 12, '')).toEqual([
      12, '*state*',
    ]);
  });
});
