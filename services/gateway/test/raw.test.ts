import assert from 'node:assert/strict';
import test from 'node:test';
import {
  RAW_LIMITS,
  classifyRawBytes,
  decodeRawSelection,
  extractRawBytes,
} from '../src/raw';

const token = (payload: unknown): string => Buffer.from(JSON.stringify(payload)).toString('base64url');

const stage = ['v1', 'stage', ['peer-a'], [
  ['y', '*state*'], ['i', 1], ['y', '1'], ['s', '1'],
]];
const ledger = ['v1', 'ledger', [
  ['i', 12], ['y', 'peer-a'], ['i', 8], ['y', '*state*'], ['i', -7],
]];

test('decodes canonical typed Stage and concrete Ledger selections', () => {
  assert.deepEqual(decodeRawSelection(token(stage)), {
    mode: 'stage', route: ['peer-a'],
    path: ['*state*', 1, '1', { '*type/string*': '1' }],
  });
  assert.deepEqual(decodeRawSelection(token(ledger)), {
    mode: 'ledger', path: [12, 'peer-a', 8, '*state*', -7],
  });
});

test('rejects relative Ledger selectors and noncanonical encodings', () => {
  assert.throws(() => decodeRawSelection(token([
    'v1', 'ledger', [['i', -1], ['y', '*state*'], ['y', 'value']],
  ])), /relative/);
  assert.throws(() => decodeRawSelection(`${token(stage)}=`), /invalid/);
  assert.throws(() => decodeRawSelection(Buffer.from(JSON.stringify(stage) + ' ').toString('base64url')), /noncanonical/);
  assert.throws(() => decodeRawSelection(Buffer.from(
    '["v1","stage",[],[["y","*state*"],["y","\\ud800"]]]',
  ).toString('base64url')), /string/);
});

test('enforces token, route, path, and segment bounds before use', () => {
  assert.throws(() => decodeRawSelection('a'.repeat(RAW_LIMITS.tokenBytes + 1)), /token/);
  assert.throws(() => decodeRawSelection(token([
    'v1', 'stage', Array(RAW_LIMITS.routeHops + 1).fill('peer'), [['y', '*state*']],
  ])), /route/);
  assert.throws(() => decodeRawSelection(token([
    'v1', 'stage', [], Array(RAW_LIMITS.pathSegments + 1).fill(['y', 'a']),
  ])), /path/);
  assert.throws(() => decodeRawSelection(token([
    'v1', 'stage', [], [['y', '*state*'], ['y', 'a'.repeat(RAW_LIMITS.segmentStringBytes + 1)]],
  ])), /string/);
});

test('extracts exact bytes and preserves missing/unavailable classes', () => {
  assert.deepEqual(extractRawBytes({ '*type/byte-vector*': '000d0aff80' }), {
    kind: 'bytes', bytes: Buffer.from([0, 13, 10, 255, 128]),
  });
  assert.deepEqual(extractRawBytes({ content: { '*type/byte-vector*': 'ff00' }, indexes: [4] }), {
    kind: 'bytes', bytes: Buffer.from([255, 0]),
  });
  assert.deepEqual(extractRawBytes(['nothing']), { kind: 'missing' });
  assert.deepEqual(extractRawBytes({ content: ['unknown'] }), { kind: 'unavailable' });
  assert.deepEqual(extractRawBytes({ content: 'not bytes' }), { kind: 'unsupported' });
  assert.deepEqual(extractRawBytes({ '*type/byte-vector*': 'ff', extra: true }), { kind: 'unsupported' });
  assert.deepEqual(extractRawBytes({
    content: { '*type/byte-vector*': 'ff', extra: true }, indexes: [4],
  }), { kind: 'unsupported' });
  assert.deepEqual(extractRawBytes(Object.create({ '*type/byte-vector*': 'ff' })), {
    kind: 'unsupported',
  });
  assert.deepEqual(extractRawBytes({ '*type/byte-vector*': 'f' }), { kind: 'unsupported' });
  assert.deepEqual(extractRawBytes({ '*type/byte-vector*': 'fg' }), { kind: 'unsupported' });
});

test('keeps active UTF-8 inert and ambiguous binary downloadable', () => {
  assert.deepEqual(classifyRawBytes(Buffer.from('<script>fetch("/api")</script>')), {
    contentType: 'text/plain; charset=utf-8', disposition: 'inline',
  });
  assert.deepEqual(classifyRawBytes(Buffer.from('<svg onload="alert(1)"/>')), {
    contentType: 'text/plain; charset=utf-8', disposition: 'inline',
  });
  assert.deepEqual(classifyRawBytes(Buffer.from([0, 255, 0, 254])), {
    contentType: 'application/octet-stream', disposition: 'attachment',
  });
  assert.deepEqual(classifyRawBytes(Buffer.from('%PDF-1.7\n')), {
    contentType: 'application/octet-stream', disposition: 'attachment',
  });
  assert.deepEqual(classifyRawBytes(Buffer.from([0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10])), {
    contentType: 'image/png', disposition: 'inline',
  });
});
