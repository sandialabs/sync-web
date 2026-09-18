const PROFILE_LIMIT = 4096;
const MAX_DEPTH = 8;
const MAX_NODES = 64;
const MAX_SYMBOL_BYTES = 128;
const SYMBOL_RE = /^[A-Za-z0-9_.*+!<>=?/-]+$/;

export class ProfileError extends Error {}

function utf8Length(value) { return new TextEncoder().encode(value).length; }

export function identityBytesFromBase64(value) {
  if (typeof value !== "string" || !/^[A-Za-z0-9+/]{43}=$/.test(value)) throw new ProfileError("identityIdBase64 must be canonical base64 for 32 bytes");
  const binary = atob(value);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  if (bytes.length !== 32 || btoa(binary) !== value) throw new ProfileError("identityIdBase64 must be canonical base64 for 32 bytes");
  return bytes;
}

function textField(value, name, limit, { allowLf = false, nonempty = false } = {}) {
  if (typeof value !== "string") throw new ProfileError(`${name} must be a string`);
  if (nonempty && !value.trim()) throw new ProfileError(`${name} must contain non-whitespace text`);
  for (const character of value) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw new ProfileError(`${name} is not valid UTF-8 text`);
    if (code === 0 || (code < 0x20 && !(allowLf && character === "\n")) || (code >= 0x7f && code <= 0x9f)
      || code === 0x2028 || code === 0x2029 || (code >= 0x202a && code <= 0x202e) || (code >= 0x2066 && code <= 0x2069)) {
      throw new ProfileError(`${name} contains a forbidden control`);
    }
  }
  if (utf8Length(value) > limit) throw new ProfileError(`${name} exceeds ${limit} UTF-8 bytes`);
  return value;
}

class Parser {
  constructor(bytes) {
    if (!(bytes instanceof Uint8Array) || bytes.length > PROFILE_LIMIT) throw new ProfileError(`profile exceeds ${PROFILE_LIMIT} bytes`);
    try { this.text = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
    catch { throw new ProfileError("profile is not valid UTF-8"); }
    this.position = 0;
    this.nodes = 0;
  }

  parse() {
    const value = this.datum(0);
    this.space();
    if (this.position !== this.text.length) throw new ProfileError("trailing profile data");
    return value;
  }

  space() { while (/[ \t\r\n]/.test(this.text[this.position] ?? "")) this.position += 1; }

  datum(depth) {
    this.space();
    if (this.position >= this.text.length) throw new ProfileError("unexpected end of profile");
    if (this.text.startsWith("#u(", this.position)) return this.byteVector();
    const character = this.text[this.position];
    if (character === "(") {
      if (depth >= MAX_DEPTH) throw new ProfileError(`profile exceeds nesting depth ${MAX_DEPTH}`);
      this.position += 1;
      const values = [];
      while (true) {
        this.space();
        if (this.position >= this.text.length) throw new ProfileError("unterminated list");
        if (this.text[this.position] === ")") { this.position += 1; return values; }
        this.nodes += 1;
        if (this.nodes > MAX_NODES) throw new ProfileError(`profile exceeds ${MAX_NODES} list elements`);
        values.push(this.datum(depth + 1));
      }
    }
    if (character === '"') return this.string();
    if ("'`;,#|".includes(character)) throw new ProfileError("forbidden Scheme reader form");
    return this.symbol();
  }

  string() {
    const start = this.position++;
    let escaped = false;
    while (this.position < this.text.length) {
      const character = this.text[this.position++];
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') {
        try { return { type: "string", value: JSON.parse(this.text.slice(start, this.position)) }; }
        catch { throw new ProfileError("invalid Scheme string"); }
      } else if (character.codePointAt(0) < 0x20) throw new ProfileError("literal control in string");
    }
    throw new ProfileError("unterminated string");
  }

  byteVector() {
    this.position += 3;
    const values = [];
    while (true) {
      this.space();
      if (this.position >= this.text.length) throw new ProfileError("unterminated byte vector");
      if (this.text[this.position] === ")") { this.position += 1; return { type: "bytes", value: Uint8Array.from(values) }; }
      const match = /^(?:0|[1-9][0-9]*)/.exec(this.text.slice(this.position));
      if (!match) throw new ProfileError("invalid byte vector token");
      this.position += match[0].length;
      if (!/[ \t\r\n)]/.test(this.text[this.position] ?? "")) throw new ProfileError("invalid byte vector separator");
      const value = Number(match[0]);
      if (value > 255) throw new ProfileError("byte vector value exceeds 255");
      values.push(value);
      if (values.length > 32) throw new ProfileError("identity byte vector exceeds 32 bytes");
    }
  }

  symbol() {
    const start = this.position;
    while (this.position < this.text.length && !/[ \t\r\n()]/.test(this.text[this.position])) this.position += 1;
    const value = this.text.slice(start, this.position);
    if (/^(?:0|[1-9][0-9]*)$/.test(value)) return { type: "integer", value: Number(value) };
    if (!SYMBOL_RE.test(value) || [".", "..", "#t", "#f"].includes(value) || /^[+-]?[0-9]+$/.test(value)) throw new ProfileError(`invalid or forbidden symbol: ${JSON.stringify(value)}`);
    if (utf8Length(value) > MAX_SYMBOL_BYTES) throw new ProfileError(`symbol exceeds ${MAX_SYMBOL_BYTES} bytes`);
    return { type: "symbol", value };
  }
}

function equalBytes(left, right) { return left.length === right.length && left.every((value, index) => value === right[index]); }
function isNode(value, type) { return value && !Array.isArray(value) && value.type === type; }
function stringNode(value, name) {
  if (!isNode(value, "string")) throw new ProfileError(`${name} must be a string`);
  return value.value;
}

export function validateProfile(bytes, expected) {
  const expectedIdentity = expected.identityIdBase64 === undefined ? undefined : identityBytesFromBase64(expected.identityIdBase64);
  const datum = new Parser(bytes).parse();
  if (!Array.isArray(datum)) throw new ProfileError("profile must be a proper association list");
  const rows = new Map();
  for (const row of datum) {
    if (!Array.isArray(row) || row.length !== 2 || !isNode(row[0], "symbol")) throw new ProfileError("profile rows must be two-element symbol-keyed lists");
    if (rows.has(row[0].value)) throw new ProfileError(`duplicate profile key: ${row[0].value}`);
    rows.set(row[0].value, row[1]);
  }
  const schemaNode = rows.get("schema");
  if (!isNode(schemaNode, "symbol")) throw new ProfileError("schema must be a symbol");
  const schema = schemaNode.value;
  const common = ["schema", "identity-id", "journal", "owner", "display-name"];
  const version = schema === "sync-agent-profile-v0.experimental" ? 0 : schema === "sync-agent-profile-v1.experimental" ? 1 : undefined;
  if (version === undefined) throw new ProfileError("schema mismatch");
  const required = new Set(version ? [...common, "revision", "bio"] : common);
  const allowed = new Set([...required, "pronouns"]);
  const missing = [...required].filter((key) => !rows.has(key));
  const unknown = [...rows.keys()].filter((key) => !allowed.has(key));
  if (missing.length) throw new ProfileError(`missing profile keys: ${missing.sort().join(", ")}`);
  if (unknown.length) throw new ProfileError(`unknown profile keys: ${unknown.sort().join(", ")}`);
  const embeddedIdentity = rows.get("identity-id");
  if (!isNode(embeddedIdentity, "bytes") || embeddedIdentity.value.length !== 32) {
    throw new ProfileError("identity-id must contain exactly 32 public metadata bytes");
  }
  if (expectedIdentity && !equalBytes(embeddedIdentity.value, expectedIdentity)) throw new ProfileError("identity-id mismatch");
  if (rows.get("journal")?.type !== "symbol" || rows.get("journal").value !== expected.journal) throw new ProfileError("journal mismatch");
  if (rows.get("owner")?.type !== "symbol" || rows.get("owner").value !== expected.owner) throw new ProfileError("owner mismatch");
  const displayName = textField(stringNode(rows.get("display-name"), "display-name"), "display-name", 128);
  let pronouns = null;
  if (rows.has("pronouns")) {
    const values = rows.get("pronouns");
    if (!Array.isArray(values) || values.length < 1 || values.length > 4) throw new ProfileError("pronouns must contain one through four strings");
    pronouns = values.map((item) => textField(stringNode(item, "pronoun"), "pronoun", 64));
  }
  let revision = 0;
  let bio = null;
  if (version === 1) {
    const item = rows.get("revision");
    if (!isNode(item, "integer") || !Number.isSafeInteger(item.value) || item.value < 1) throw new ProfileError("revision must be a positive integer");
    revision = item.value;
    bio = textField(stringNode(rows.get("bio"), "bio"), "bio", 2048, { allowLf: true, nonempty: true });
  }
  const identityIdBase64 = btoa(String.fromCharCode(...embeddedIdentity.value));
  return { schemaVersion: version, revision, identityIdBase64, displayName, pronouns, bio };
}

function schemeString(value) {
  textField(value, "profile text", 2048, { allowLf: true });
  return JSON.stringify(value);
}

export function profileProvenanceLabel(qualified, current) {
  if (typeof qualified !== "string" || !qualified.includes("@") || !current?.profile
    || !Number.isSafeInteger(current.profile.schemaVersion) || !Number.isSafeInteger(current.profile.revision)
    || typeof current.observedAt !== "string" || new Date(current.observedAt).toISOString() !== current.observedAt
    || current.authenticationScope !== "route-owner-journal-path-current" || current.terminalJournalContinuity !== "unproven"
    || !/^[0-9a-f]{64}$/.test(current.rawSha256)) throw new ProfileError("Invalid profile provenance");
  const freshness = current.stale === true ? "stale/unavailable" : "fresh at observation";
  return `${qualified} · schema v${current.profile.schemaVersion} · revision ${current.profile.revision} · current-get · ${freshness} · observed ${current.observedAt} · raw SHA-256 ${current.rawSha256} · authenticationScope=route-owner-journal-path-current · terminalJournalContinuity=unproven · non-authoritative descriptive metadata · current-state observation, not historical commitment`;
}

export function buildEditedProfile(current, fields, expected) {
  const nextRevision = current.profile == null ? 1 : current.profile.schemaVersion === 0 ? 1 : current.profile.revision + 1;
  const identity = identityBytesFromBase64(expected.identityIdBase64);
  const displayName = textField(fields.displayName, "display-name", 128);
  const bio = textField(fields.bio, "bio", 2048, { allowLf: true, nonempty: true });
  const pronouns = fields.pronouns == null || fields.pronouns.length === 0 ? null
    : fields.pronouns.map((value) => textField(value, "pronoun", 64));
  if (pronouns && pronouns.length > 4) throw new ProfileError("pronouns must contain one through four strings");
  const rows = [
    "((schema sync-agent-profile-v1.experimental)",
    ` (identity-id #u(${[...identity].join(" ")}))`,
    ` (journal ${expected.journal})`,
    ` (owner ${expected.owner})`,
    ` (revision ${nextRevision})`,
    ` (display-name ${schemeString(displayName)})`,
    ...(pronouns ? [` (pronouns (${pronouns.map(schemeString).join(" ")}))`] : []),
    ` (bio ${schemeString(bio)}))\n`,
  ];
  const bytes = new TextEncoder().encode(rows.join("\n"));
  if (bytes.length > PROFILE_LIMIT) throw new ProfileError(`profile exceeds ${PROFILE_LIMIT} bytes`);
  validateProfile(bytes, expected);
  return bytes;
}
