import {
  buildLedgerUserHomePath, normalizePublicSnapshotInput,
  normalizeSnapshotInput, stepSnapshotValue,
} from './ledgerRoute';
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

describe('absolute contextual snapshots', () => {
  it.each([
    ['160', '160'], ['999', '160'], ['-1', '160'], ['-2', '159'],
    ['-10', '151'], ['-999', '0'], ['latest', '160'], ['bad', '160'],
  ])('normalizes %s against maximum 160', (input, expected) => {
    expect(normalizeSnapshotInput(input, 160)).toBe(expected);
  });

  it.each([
    ['160', '160'], ['999', '160'], ['-1', '160'], ['-2', '159'], ['-999', '0'],
  ])('accepts public integer %s as %s', (input, expected) => {
    expect(normalizePublicSnapshotInput(input, 160)).toBe(expected);
  });

  it.each(['latest', 'bad', '9007199254740992', '-9007199254740992'])
    ('rejects public input %s', (input) => {
      expect(normalizePublicSnapshotInput(input, 160)).toBeNull();
    });

  it('steps only within zero and the contextual maximum', () => {
    expect(stepSnapshotValue('0', 160, 'older')).toBe('0');
    expect(stepSnapshotValue('0', 160, 'newer')).toBe('1');
    expect(stepSnapshotValue('160', 160, 'newer')).toBe('160');
    expect(stepSnapshotValue('160', 160, 'older')).toBe('159');
  });
});
