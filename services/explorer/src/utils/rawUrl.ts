import { JournalPath, JournalPathSegment } from '../types';

export const RAW_URL_LIMITS = {
  tokenBytes: 4096,
  jsonBytes: 3072,
  routeHops: 16,
  pathSegments: 256,
  segmentStringBytes: 1024,
} as const;

const textEncoder = new TextEncoder();

type EncodedSegment = ['i', number] | ['y', string] | ['s', string];

const validScalarString = (value: string): boolean => {
  if (textEncoder.encode(value).byteLength > RAW_URL_LIMITS.segmentStringBytes) return false;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      if (index + 1 >= value.length) return false;
      const next = value.charCodeAt(index + 1);
      if (next < 0xdc00 || next > 0xdfff) return false;
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      return false;
    }
  }
  return true;
};

const encodeSegment = (segment: JournalPathSegment): EncodedSegment | null => {
  if (typeof segment === 'number') {
    return Number.isSafeInteger(segment) ? ['i', segment] : null;
  }
  if (typeof segment === 'string') return validScalarString(segment) ? ['y', segment] : null;
  const value = segment['*type/string*'];
  return validScalarString(value) ? ['s', value] : null;
};

const encodeSegments = (path: JournalPath): EncodedSegment[] | null => {
  if (path.length === 0 || path.length > RAW_URL_LIMITS.pathSegments) return null;
  const encoded = path.map(encodeSegment);
  return encoded.every((segment): segment is EncodedSegment => segment !== null) ? encoded : null;
};

const base64Url = (bytes: Uint8Array): string => {
  let binary = '';
  bytes.forEach((byte) => { binary += String.fromCharCode(byte); });
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

const encodePayload = (payload: unknown): string | null => {
  const bytes = textEncoder.encode(JSON.stringify(payload));
  if (bytes.byteLength > RAW_URL_LIMITS.jsonBytes) return null;
  const token = base64Url(bytes);
  return token.length <= RAW_URL_LIMITS.tokenBytes ? token : null;
};

export const rawSelectionTokenWithinLimit = (token: string): boolean =>
  token.length > 0 && token.length <= RAW_URL_LIMITS.tokenBytes
  && /^[A-Za-z0-9_-]+$/.test(token);

export const encodeStageRawSelection = (route: string[], path: JournalPath): string | null => {
  if (route.length > RAW_URL_LIMITS.routeHops
      || !route.every((hop) => validScalarString(hop))
      || path[0] !== '*state*') {
    return null;
  }
  const encoded = encodeSegments(path);
  return encoded ? encodePayload(['v1', 'stage', route, encoded]) : null;
};

export const normalizeLedgerRawPath = (
  path: JournalPath,
  indexes: number[] | undefined,
): JournalPath | null => {
  if (!indexes || indexes.length === 0) return null;
  const stateIndex = path.lastIndexOf('*state*');
  if (stateIndex < 1 || stateIndex % 2 !== 1) return null;

  let cursor = 0;
  const normalized = path.map((segment, index) => {
    if (index >= stateIndex || typeof segment !== 'number') return segment;
    if (cursor >= indexes.length) return segment;
    return indexes[cursor++];
  });

  if (cursor !== indexes.length) return null;
  for (let index = 0; index < stateIndex; index += 1) {
    const segment = normalized[index];
    if (index % 2 === 0) {
      if (typeof segment !== 'number' || !Number.isSafeInteger(segment) || segment < 0) return null;
    } else if (typeof segment !== 'string' || !validScalarString(segment)) {
      return null;
    }
  }
  const selectors = normalized.slice(0, stateIndex).filter((segment) => typeof segment === 'number');
  return selectors.length === indexes.length ? normalized : null;
};

export const encodeLedgerRawSelection = (
  path: JournalPath,
  indexes: number[] | undefined,
): string | null => {
  const normalized = normalizeLedgerRawPath(path, indexes);
  const encoded = normalized ? encodeSegments(normalized) : null;
  return encoded ? encodePayload(['v1', 'ledger', encoded]) : null;
};
