import {
  RAW_URL_LIMITS,
  encodeLedgerRawSelection,
  encodeStageRawSelection,
  normalizeLedgerRawPath,
  rawSelectionTokenWithinLimit,
} from './rawUrl';

const stageAtJsonBytes = (target: number): string | null => {
  for (let segments = 1; segments <= 8; segments += 1) {
    const path = ['*state*', ...Array(segments).fill('')];
    const encodedPath = [['y', '*state*'], ...Array.from({ length: segments }, () => ['y', ''])];
    let remaining = target - new TextEncoder().encode(JSON.stringify([
      'v1', 'stage', [], encodedPath,
    ])).byteLength;
    if (remaining < 0 || remaining > segments * RAW_URL_LIMITS.segmentStringBytes) continue;
    for (let index = 1; index < path.length && remaining > 0; index += 1) {
      const length = Math.min(remaining, RAW_URL_LIMITS.segmentStringBytes);
      path[index] = 'a'.repeat(length);
      remaining -= length;
    }
    return encodeStageRawSelection([], path);
  }
  throw new Error(`cannot construct ${target}-byte Raw payload`);
};

const decodeToken = (token: string): unknown => {
  const padded = token.replace(/-/g, '+').replace(/_/g, '/')
    + '='.repeat((4 - (token.length % 4)) % 4);
  return JSON.parse(decodeURIComponent(Array.from(atob(padded))
    .map((character) => `%${character.charCodeAt(0).toString(16).padStart(2, '0')}`).join('')));
};

describe('Raw URL selections', () => {
  it('injectively tags Stage integer, symbol, and Scheme-string path segments', () => {
    const token = encodeStageRawSelection(['peer-a'], [
      '*state*', 1, '1', { '*type/string*': '1' },
    ]);
    expect(decodeToken(token!)).toEqual([
      'v1', 'stage', ['peer-a'],
      [['y', '*state*'], ['i', 1], ['y', '1'], ['s', '1']],
    ]);
  });

  it('normalizes every Ledger history selector from one response index list', () => {
    const path = [-1, 'peer-a', -2, '*state*', -7, { '*type/string*': 'value' }];
    expect(normalizeLedgerRawPath(path, [12, 8])).toEqual([
      12, 'peer-a', 8, '*state*', -7, { '*type/string*': 'value' },
    ]);
    expect(decodeToken(encodeLedgerRawSelection(path, [12, 8])!)).toEqual([
      'v1', 'ledger',
      [['i', 12], ['y', 'peer-a'], ['i', 8], ['y', '*state*'],
        ['i', -7], ['s', 'value']],
    ]);
  });

  it('suppresses Stage links outside route, path, string, and payload bounds', () => {
    expect(encodeStageRawSelection(
      Array(RAW_URL_LIMITS.routeHops + 1).fill('peer'), ['*state*'],
    )).toBeNull();
    expect(encodeStageRawSelection([], [
      '*state*', ...Array(RAW_URL_LIMITS.pathSegments).fill('a'),
    ])).toBeNull();
    expect(encodeStageRawSelection([], [
      '*state*', 'a'.repeat(RAW_URL_LIMITS.segmentStringBytes + 1),
    ])).toBeNull();
    expect(encodeStageRawSelection([], [
      '*state*', ...Array(4).fill('a'.repeat(RAW_URL_LIMITS.segmentStringBytes)),
    ])).toBeNull();
  });

  it('applies the exact client token max and max+1 boundary', () => {
    const max = stageAtJsonBytes(RAW_URL_LIMITS.jsonBytes);
    expect(max).toHaveLength(RAW_URL_LIMITS.tokenBytes);
    expect(stageAtJsonBytes(RAW_URL_LIMITS.jsonBytes + 1)).toBeNull();
    expect(rawSelectionTokenWithinLimit('a'.repeat(RAW_URL_LIMITS.tokenBytes))).toBe(true);
    expect(rawSelectionTokenWithinLimit('a'.repeat(RAW_URL_LIMITS.tokenBytes + 1))).toBe(false);
  });

  it('fails closed when proof-derived indexes are absent or incomplete', () => {
    const path = [-1, 'peer-a', -1, '*state*', 'value'];
    expect(encodeLedgerRawSelection(path, undefined)).toBeNull();
    expect(encodeLedgerRawSelection(path, [4])).toBeNull();
    expect(encodeLedgerRawSelection(path, [4, -1])).toBeNull();
    expect(encodeLedgerRawSelection(path, [4, 3, 2])).toBeNull();
  });
});
