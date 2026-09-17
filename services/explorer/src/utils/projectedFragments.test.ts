import { ExplorerSelection, LedgerHop } from '../types';
import { retainedLedgerRootPath } from './ledgerRoute';
import {
  buildFragmentHash,
  parseFragmentHash,
} from './projectedFragments';

describe('projectedFragments', () => {
  const ledgerHops: LedgerHop[] = [
    { key: 'local', kind: 'local', name: 'Self', snapshot: '42' },
    { key: 'alice-1', kind: 'bridge', name: 'alice', snapshot: 'latest' },
    { key: 'bob-2', kind: 'bridge', name: 'bob', snapshot: '-3' },
  ];

  it('builds a stage fragment with url-safe encoding', () => {
    const stageSelection: ExplorerSelection = {
      path: ['*state*', 'docs', 'hello world.txt'],
      type: 'file',
    };

    const hash = buildFragmentHash({
      mode: 'stage',
      stageSelection,
      ledgerSelection: null,
      ledgerRootPath: [42, '*state*'],
      ledgerHops: [ledgerHops[0]],
      rootIndex: 42,
    });

    expect(hash).toBe('#stage/docs/hello%20world.txt');
    expect(parseFragmentHash(hash)).toEqual({
      mode: 'stage',
      selection: stageSelection,
    });
  });

  it('round-trips an exact multi-hop remote Stage route and encoded path', () => {
    const stageSelection: ExplorerSelection = {
      path: ['*state*', 'design%20notes', 'draft%2Fone.txt'],
      type: 'file',
    };
    const encodedHops: LedgerHop[] = [
      ledgerHops[0],
      { key: 'alice-1', kind: 'bridge', name: 'alice west', snapshot: 'latest' },
      ledgerHops[2],
    ];

    const hash = buildFragmentHash({
      mode: 'stage',
      stageSelection,
      ledgerSelection: null,
      ledgerRootPath: [42, '*state*'],
      ledgerHops: encodedHops,
      rootIndex: 42,
    });

    expect(hash).toBe(
      '#stage-route/42/bridge/alice%20west/latest/bridge/bob/-3/state/design%2520notes/draft%252Fone.txt',
    );
    expect(parseFragmentHash(hash)).toEqual({
      mode: 'stage',
      ledgerHops: [
        { key: 'local', kind: 'local', name: 'Self', snapshot: '42' },
        { key: 'alice west-1', kind: 'bridge', name: 'alice west', snapshot: 'latest' },
        { key: 'bob-2', kind: 'bridge', name: 'bob', snapshot: '-3' },
      ],
      selection: stageSelection,
    });
  });

  it('keeps route-free Self Stage fragments backward compatible even for a route path', () => {
    expect(parseFragmentHash('#stage/route/bridge/state/')).toEqual({
      mode: 'stage',
      selection: {
        path: ['*state*', 'route', 'bridge', 'state'],
        type: 'directory',
      },
    });
  });

  it('rejects malformed and noncanonical remote Stage snapshots', () => {
    expect(parseFragmentHash('#stage-route/latest/state/')).toBeNull();
    expect(parseFragmentHash('#stage-route/latest/bridge/alice/nope/state/')).toBeNull();
    ['1e2', '1.5', '0x10', 'Infinity', '01', '-0', '-1'].forEach((snapshot) => {
      expect(parseFragmentHash(
        `#stage-route/${snapshot}/bridge/alice/latest/state/`,
      )).toBeNull();
    });
    ['1e2', '1.5', '0x10', 'Infinity', '01', '-0'].forEach((snapshot) => {
      expect(parseFragmentHash(
        `#stage-route/42/bridge/alice/${snapshot}/state/`,
      )).toBeNull();
    });
    expect(parseFragmentHash('#stage-route/42/bridge/alice/2/state/')).toMatchObject({
      ledgerHops: [{ snapshot: '42' }, { snapshot: '2' }],
    });
  });

  it('round-trips a ledger fragment with bridges and history', () => {
    const ledgerSelection: ExplorerSelection = {
      path: [42, '*state*', 'docs', 'readme.md'],
      type: 'file',
    };

    const hash = buildFragmentHash({
      mode: 'ledger',
      stageSelection: null,
      ledgerSelection,
      ledgerRootPath: [42],
      ledgerHops,
      rootIndex: 42,
    });

    expect(hash).toBe('#ledger/42/bridge/alice/bridge/bob/-3/state/docs/readme.md');

    expect(parseFragmentHash(hash)).toEqual({
      mode: 'ledger',
      ledgerHops: [
        { key: 'local', kind: 'local', name: 'Self', snapshot: '42' },
        { key: 'alice-1', kind: 'bridge', name: 'alice', snapshot: 'latest' },
        { key: 'bob-2', kind: 'bridge', name: 'bob', snapshot: '-3' },
      ],
      selection: ledgerSelection,
    });
  });

  it('round-trips a retained bridge path separately from the terminal route', () => {
    const selection: ExplorerSelection = {
      path: [42, '*bridge*', 'archive', 7, '*state*', 'docs'],
      type: 'directory',
    };
    const hash = buildFragmentHash({
      mode: 'ledger', stageSelection: null, ledgerSelection: selection,
      ledgerRootPath: [42], ledgerHops, rootIndex: 42,
    });
    expect(hash).toBe(
      '#ledger/42/bridge/alice/bridge/bob/-3/retained/bridge/archive/7/state/docs/',
    );
    expect(parseFragmentHash(hash)).toEqual({
      mode: 'ledger',
      ledgerHops: [
        { key: 'local', kind: 'local', name: 'Self', snapshot: '42' },
        { key: 'alice-1', kind: 'bridge', name: 'alice', snapshot: 'latest' },
        { key: 'bob-2', kind: 'bridge', name: 'bob', snapshot: '-3' },
      ],
      selection: {
        ...selection,
        path: [-3, '*bridge*', 'archive', 7, '*state*', 'docs'],
      },
    });
  });

  it('projects retained selections onto the selected terminal provider root', () => {
    const parsed = parseFragmentHash(
      '#ledger/7/bridge/journal-0/4/retained/bridge/journal-2/3/state/',
    );
    expect(parsed?.mode).toBe('ledger');
    if (!parsed || parsed.mode !== 'ledger') throw new Error('Expected Ledger deep link');
    expect(retainedLedgerRootPath(parsed.ledgerHops, 7)).toEqual([4]);
    expect(parsed.selection.path).toEqual([
      4, '*bridge*', 'journal-2', 3, '*state*',
    ]);

    const latest = parseFragmentHash(
      '#ledger/7/bridge/journal-0/retained/bridge/journal-2/3/state/',
    );
    expect(latest?.mode).toBe('ledger');
    if (!latest || latest.mode !== 'ledger') throw new Error('Expected Ledger deep link');
    expect(latest.selection.path).toEqual([
      -1, '*bridge*', 'journal-2', 3, '*state*',
    ]);
  });

  it('builds and parses the admin fragment', () => {
    const hash = buildFragmentHash({
      mode: 'admin',
      stageSelection: null,
      ledgerSelection: null,
      ledgerRootPath: [42, '*state*'],
      ledgerHops,
      rootIndex: 42,
    });

    expect(hash).toBe('#admin');
    expect(parseFragmentHash(hash)).toEqual({ mode: 'admin' });
  });
});
