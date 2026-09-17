export const RAW_LIMITS = {
  // The full query request line remains below Nginx's 8 KiB single-buffer boundary.
  tokenBytes: 4096,
  jsonBytes: 3072,
  routeHops: 16,
  pathSegments: 256,
  segmentStringBytes: 1024,
} as const;

type RawPathSegment = number | string | { "*type/string*": string };

export type RawSelection =
  | { mode: "stage"; route: string[]; path: RawPathSegment[] }
  | { mode: "ledger"; path: RawPathSegment[] };

export interface RawClassification {
  contentType: string;
  disposition: "inline" | "attachment";
}

const utf8 = new TextEncoder();
const utf8Fatal = new TextDecoder("utf-8", { fatal: true });

const validScalarString = (value: string): boolean => {
  if (utf8.encode(value).byteLength > RAW_LIMITS.segmentStringBytes) return false;
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

const decodeSegment = (value: unknown): RawPathSegment => {
  if (!Array.isArray(value) || value.length !== 2 || typeof value[0] !== "string") {
    throw new Error("invalid raw path segment");
  }
  if (value[0] === "i") {
    if (!Number.isSafeInteger(value[1])) throw new Error("invalid raw integer segment");
    return value[1] as number;
  }
  if ((value[0] === "y" || value[0] === "s")
      && typeof value[1] === "string" && validScalarString(value[1])) {
    return value[0] === "y" ? value[1] : { "*type/string*": value[1] };
  }
  throw new Error("invalid raw string segment");
};

const isSymbol = (value: RawPathSegment, symbol: string): boolean =>
  typeof value === "string" && value === symbol;

const validateStage = (routeValue: unknown, pathValue: unknown): RawSelection => {
  if (!Array.isArray(routeValue) || routeValue.length > RAW_LIMITS.routeHops
      || !routeValue.every((hop) => typeof hop === "string" && validScalarString(hop))) {
    throw new Error("invalid raw route");
  }
  if (!Array.isArray(pathValue) || pathValue.length === 0
      || pathValue.length > RAW_LIMITS.pathSegments) {
    throw new Error("invalid raw path");
  }
  const path = pathValue.map(decodeSegment);
  if (!isSymbol(path[0], "*state*")) throw new Error("invalid raw Stage path");
  return { mode: "stage", route: routeValue as string[], path };
};

const validateLedger = (pathValue: unknown): RawSelection => {
  if (!Array.isArray(pathValue) || pathValue.length === 0
      || pathValue.length > RAW_LIMITS.pathSegments) {
    throw new Error("invalid raw path");
  }
  const path = pathValue.map(decodeSegment);
  const stateIndex = path.findIndex((segment) => isSymbol(segment, "*state*"));
  if (stateIndex < 1 || typeof path[0] !== "number") throw new Error("invalid raw Ledger path");

  for (let index = 0; index < stateIndex; index += 1) {
    const segment = path[index];
    if (index % 2 === 0) {
      if (typeof segment !== "number" || segment < 0) {
        throw new Error("relative raw Ledger selector");
      }
    } else if (typeof segment !== "string") {
      throw new Error("invalid raw Ledger bridge");
    }
  }
  if (stateIndex % 2 !== 1) throw new Error("invalid raw Ledger route shape");
  return { mode: "ledger", path };
};

export const decodeRawSelection = (token: string): RawSelection => {
  if (!token || Buffer.byteLength(token, "ascii") > RAW_LIMITS.tokenBytes
      || !/^[A-Za-z0-9_-]+$/.test(token)) {
    throw new Error("invalid raw token");
  }
  const bytes = Buffer.from(token, "base64url");
  if (bytes.byteLength > RAW_LIMITS.jsonBytes || bytes.toString("base64url") !== token) {
    throw new Error("noncanonical raw token");
  }
  let text: string;
  try {
    text = utf8Fatal.decode(bytes);
  } catch {
    throw new Error("invalid raw token encoding");
  }
  let payload: unknown;
  try {
    payload = JSON.parse(text);
  } catch {
    throw new Error("invalid raw token JSON");
  }
  if (!Array.isArray(payload) || JSON.stringify(payload) !== text || payload[0] !== "v1") {
    throw new Error("noncanonical raw payload");
  }

  let selection: RawSelection;
  if (payload.length === 4 && payload[1] === "stage") {
    selection = validateStage(payload[2], payload[3]);
  } else if (payload.length === 3 && payload[1] === "ledger") {
    selection = validateLedger(payload[2]);
  } else {
    throw new Error("invalid raw payload shape");
  }
  if (Buffer.from(JSON.stringify(payload), "utf8").toString("base64url") !== token) {
    throw new Error("noncanonical raw payload");
  }
  return selection;
};

export const extractRawBytes = (result: unknown):
  | { kind: "bytes"; bytes: Buffer }
  | { kind: "missing" }
  | { kind: "unavailable" }
  | { kind: "unsupported" } => {
  const record = result && typeof result === "object" && !Array.isArray(result)
    ? result as Record<string, unknown> : null;
  const content = record && Object.prototype.hasOwnProperty.call(record, "content")
    ? record.content : result;
  if (Array.isArray(content) && content.length === 1 && content[0] === "nothing") {
    return { kind: "missing" };
  }
  if (Array.isArray(content) && content.length === 1 && content[0] === "unknown") {
    return { kind: "unavailable" };
  }
  if (!content || typeof content !== "object" || Array.isArray(content)) {
    return { kind: "unsupported" };
  }
  const prototype = Object.getPrototypeOf(content);
  const keys = Object.keys(content);
  if ((prototype !== Object.prototype && prototype !== null)
      || keys.length !== 1 || keys[0] !== "*type/byte-vector*"
      || !Object.prototype.hasOwnProperty.call(content, "*type/byte-vector*")) {
    return { kind: "unsupported" };
  }
  const hex = (content as Record<string, unknown>)["*type/byte-vector*"];
  if (typeof hex !== "string" || hex.length % 2 !== 0 || /[^0-9a-f]/i.test(hex)) {
    return { kind: "unsupported" };
  }
  return { kind: "bytes", bytes: Buffer.from(hex, "hex") };
};

const ascii = (bytes: Buffer, start: number, length: number): string =>
  bytes.subarray(start, start + length).toString("ascii");
const startsWith = (bytes: Buffer, signature: number[]): boolean =>
  signature.every((byte, index) => bytes[index] === byte);

export const classifyRawBytes = (bytes: Buffer): RawClassification => {
  if (startsWith(bytes, [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) {
    return { contentType: "image/png", disposition: "inline" };
  }
  if (startsWith(bytes, [0xff, 0xd8, 0xff])) return { contentType: "image/jpeg", disposition: "inline" };
  if (ascii(bytes, 0, 6) === "GIF87a" || ascii(bytes, 0, 6) === "GIF89a") {
    return { contentType: "image/gif", disposition: "inline" };
  }
  if (ascii(bytes, 0, 4) === "RIFF" && ascii(bytes, 8, 4) === "WEBP") {
    return { contentType: "image/webp", disposition: "inline" };
  }
  if (ascii(bytes, 4, 4) === "ftyp" && ["avif", "avis"].includes(ascii(bytes, 8, 4))) {
    return { contentType: "image/avif", disposition: "inline" };
  }
  if (ascii(bytes, 0, 2) === "BM") return { contentType: "image/bmp", disposition: "inline" };
  if (startsWith(bytes, [0x00, 0x00, 0x01, 0x00])) {
    return { contentType: "image/x-icon", disposition: "inline" };
  }

  const unprovenMedia = ascii(bytes, 0, 5) === "%PDF-"
    || (ascii(bytes, 0, 4) === "RIFF" && ascii(bytes, 8, 4) === "WAVE")
    || ascii(bytes, 0, 4) === "OggS"
    || ascii(bytes, 0, 3) === "ID3"
    || startsWith(bytes, [0x1a, 0x45, 0xdf, 0xa3])
    || ascii(bytes, 4, 4) === "ftyp";
  if (unprovenMedia) {
    return { contentType: "application/octet-stream", disposition: "attachment" };
  }

  try {
    utf8Fatal.decode(bytes);
    return { contentType: "text/plain; charset=utf-8", disposition: "inline" };
  } catch {
    // Binary formats require either a proven safe inline class or download.
  }
  return { contentType: "application/octet-stream", disposition: "attachment" };
};

export const RAW_RESPONSE_HEADERS = {
  "x-content-type-options": "nosniff",
  "content-security-policy": "sandbox; default-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
  "referrer-policy": "no-referrer",
  "cache-control": "private, no-store, no-transform",
  "cross-origin-resource-policy": "same-origin",
} as const;
