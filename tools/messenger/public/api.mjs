import {
  authorizationRuleKey, byteVectorToText, contactAuthorizationRule, incomingMailboxPath,
  isMessengerMailboxRule, makeEnvelope, outgoingMailboxPath, parseDirectory,
  textToByteVector, validateEnvelope,
} from "./logic.mjs";
import { buildEditedProfile, identityBytesFromBase64, validateProfile } from "./profile.mjs";

export class ProfileWriteAmbiguousError extends Error {
  constructor(message, options) {
    super(message, options);
    this.name = "ProfileWriteAmbiguousError";
  }
}

export class GatewayError extends Error {
  constructor(status, payload) {
    super(typeof payload?.message === "string" ? payload.message : `Gateway request failed (${status})`);
    this.name = "GatewayError";
    this.status = status;
    this.code = payload?.error;
    this.payload = payload;
  }
}

async function responsePayload(response) {
  const text = await response.text();
  if (!text) return undefined;
  try { return JSON.parse(text); } catch { return text; }
}

function ruleField(rule, name) {
  if (rule && typeof rule === "object" && !Array.isArray(rule)) return rule[name];
  if (Array.isArray(rule)) {
    const entry = rule.find((item) => Array.isArray(item) && item[0] === name);
    return entry?.[1];
  }
  return undefined;
}

function exactRange(value) {
  return Array.isArray(value) && value.length === 2 && value.every(Number.isSafeInteger) ? [...value] : undefined;
}

function byteVectorBytes(value, expectedLength) {
  const hex = value?.["*type/byte-vector*"];
  if (typeof hex !== "string" || hex.length !== expectedLength * 2 || /[^0-9a-f]/i.test(hex)) throw new Error(`Expected a ${expectedLength}-byte vector`);
  return Uint8Array.from(hex.match(/../g), (part) => Number.parseInt(part, 16));
}

function base64ForBytes(bytes) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

async function sha256Hex(bytes) {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function normalizeAuthorizationRule(rule) {
  const principal = ruleField(rule, "principal");
  const path = ruleField(rule, "path");
  const keyIndexValue = ruleField(rule, "key-index");
  const useValue = ruleField(rule, "use!");
  const keyIndex = exactRange(keyIndexValue);
  const readOnly = ruleField(useValue, "read-only?");
  if (!Array.isArray(principal) || !principal.every((item) => typeof item === "string")
    || !Array.isArray(path) || !path.every((item) => typeof item === "string")
    || (keyIndexValue !== undefined && !keyIndex)
    || (useValue !== undefined && useValue !== false && typeof readOnly !== "boolean")) return undefined;
  return {
    principal: [...principal],
    ...(keyIndex ? { "key-index": keyIndex } : {}),
    path: [...path],
    "put!": ruleField(rule, "put!") === true,
    "use!": readOnly === true ? { "read-only?": true } : false,
  };
}

export class MessengerApi {
  constructor(config, fetchImpl) {
    this.config = config;
    this.fetchImpl = fetchImpl ?? globalThis.fetch.bind(globalThis);
    this.localOwner = undefined;
    this.localJournal = undefined;
    this.localProfileEditBases = new WeakSet();
    this.consumedProfileEditBases = new WeakSet();
  }

  setLocalIdentity(owner, journal) {
    if (typeof owner !== "string" || !owner || typeof journal !== "string" || !journal) throw new Error("Invalid local Messenger identity");
    this.localOwner = owner;
    this.localJournal = journal;
  }

  requireLocalIdentity() {
    if (!this.localOwner || !this.localJournal) throw new Error("Local Messenger identity has not been discovered");
  }

  localPathConfig() {
    this.requireLocalIdentity();
    return { localOwner: this.localOwner, localJournal: this.localJournal };
  }

  async request(path, body) {
    const response = await this.fetchImpl(`${this.config.gatewayBase}${path}`, {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json", Accept: "application/json", "X-Sync-Messenger-Request": "v1" },
      body: JSON.stringify(body),
    });
    const payload = await responsePayload(response);
    if (!response.ok) throw new GatewayError(response.status, payload);
    return payload;
  }

  readCurrent(body) {
    return this.request("/general/use", { ...body, "read-only?": true, "expression?": false });
  }

  putCurrent(body) {
    return this.request("/general/put", { ...body, "expression?": false });
  }

  async whoami() {
    const response = await this.fetchImpl(`${this.config.authBase}/.ory/sessions/whoami`, {
      method: "GET",
      credentials: "include",
      headers: { Accept: "application/json" },
      cache: "no-store",
    });
    const payload = await responsePayload(response);
    if (!response.ok) throw new GatewayError(response.status, payload);
    return payload;
  }

  async getLocalJournalInfo() {
    const response = await this.fetchImpl(`${this.config.gatewayBase}/general/info`, {
      method: "GET", credentials: "include", headers: { Accept: "application/json" }, cache: "no-store",
    });
    const payload = await responsePayload(response);
    if (!response.ok) throw new GatewayError(response.status, payload);
    const name = payload?.name?.["*type/string*"] ?? payload?.name;
    if (typeof name !== "string" || !name) throw new Error("Journal info did not return a name");
    let identityIdBase64;
    try { identityIdBase64 = base64ForBytes(byteVectorBytes(payload?.identity?.id, 32)); }
    catch { /* Profile binding is optional and must not block messaging startup. */ }
    return { name, ...(identityIdBase64 ? { identityIdBase64 } : {}) };
  }

  async getLocalJournalName() { return (await this.getLocalJournalInfo()).name; }

  async getAuthorizations() {
    this.requireLocalIdentity();
    const rules = await this.request("/general/authorizations", { user: ["*state*", this.localOwner] });
    const encodedRules = rules === null ? [] : rules;
    if (!Array.isArray(encodedRules)) throw new Error("Gateway returned an invalid authorization table");
    const normalized = encodedRules.map(normalizeAuthorizationRule);
    if (normalized.some((rule) => !rule)) throw new Error("Gateway returned an authorization rule Messenger cannot preserve exactly");
    return normalized;
  }

  authorize(rule) {
    this.requireLocalIdentity();
    return this.request("/general/authorize", { user: ["*state*", this.localOwner], rule });
  }

  deauthorize(rule) {
    this.requireLocalIdentity();
    return this.request("/general/deauthorize", { user: ["*state*", this.localOwner], rule });
  }

  profilePath(owner) { return ["*state*", owner, "profile.scm"]; }

  async readProfile({ owner, journal, route }) {
    return this.readProfileCurrent({ owner, journal, route });
  }

  async readLocalProfile({ owner, journal, identityIdBase64 }) {
    identityBytesFromBase64(identityIdBase64);
    if (owner !== this.localOwner || journal !== this.localJournal) throw new Error("Local profile source does not match authenticated Journal info");
    const value = await this.readCurrent({ path: this.profilePath(owner) });
    let current;
    if (Array.isArray(value) && value.length === 1 && value[0] === "nothing") {
      current = {
        profile: null, value, identityIdBase64,
        observedAt: new Date().toISOString(), stale: false, rawSha256: null,
      };
    } else {
      const bytes = new TextEncoder().encode(byteVectorToText(value, 4096));
      const profile = validateProfile(bytes, { owner, journal, identityIdBase64 });
      current = {
        profile, value, identityIdBase64,
        observedAt: new Date().toISOString(), stale: false, rawSha256: await sha256Hex(bytes),
      };
    }
    this.localProfileEditBases.add(current);
    return current;
  }

  async readProfileCurrent({ owner, journal, identityIdBase64, route }) {
    const value = await this.readCurrent({
      path: this.profilePath(owner),
      ...(route ? { $federation: { route: [...route] } } : {}),
    });
    const bytes = new TextEncoder().encode(byteVectorToText(value, 4096));
    const profile = validateProfile(bytes, { owner, journal, ...(identityIdBase64 ? { identityIdBase64 } : {}) });
    return {
      profile, value, ...(identityIdBase64 ? { identityIdBase64 } : {}),
      observedAt: new Date().toISOString(), stale: false, rawSha256: await sha256Hex(bytes),
    };
  }

  isProfileEditBaseConsumed(current) { return Boolean(current && this.consumedProfileEditBases.has(current)); }

  async saveLocalProfile(current, fields) {
    this.requireLocalIdentity();
    if (!current || !this.localProfileEditBases.has(current) || this.isProfileEditBaseConsumed(current)) {
      throw new Error("Profile edit requires the exact fresh local current-get object");
    }
    const expected = { owner: this.localOwner, journal: this.localJournal, identityIdBase64: current.identityIdBase64 };
    const bytes = buildEditedProfile(current, fields, expected);
    const value = textToByteVector(new TextDecoder().decode(bytes));

    // Any dispatch attempt consumes this exact edit base, regardless of outcome.
    this.localProfileEditBases.delete(current);
    this.consumedProfileEditBases.add(current);
    try {
      const result = await this.putCurrent({
        path: this.profilePath(this.localOwner), value, expected: current.value, "expression?": false,
      });
      if (result === false) return { status: "conflict" };
      if (result !== true) throw new Error("Gateway returned an ambiguous profile write result");
      const readback = await this.readLocalProfile(expected);
      if (readback.value["*type/byte-vector*"] !== value["*type/byte-vector*"]) throw new Error("Profile readback did not match the proposed exact bytes");
      return { status: "saved-current", current: readback };
    } catch (error) {
      throw new ProfileWriteAmbiguousError("Profile write failed or is ambiguous; a fresh current readback is required", { cause: error });
    }
  }

  contactRegistryPath() {
    this.requireLocalIdentity();
    return ["*state*", this.localOwner, "messenger", "contacts.json"];
  }

  async loadContactRegistry(seedDocument) {
    let current = await this.readCurrent({ path: this.contactRegistryPath() });
    if (Array.isArray(current) && current.length === 1 && current[0] === "nothing") {
      const initial = textToByteVector(JSON.stringify(seedDocument));
      const created = await this.putCurrent({
        path: this.contactRegistryPath(), value: initial, expected: current, "expression?": false,
      });
      current = created === true ? initial : await this.readCurrent({ path: this.contactRegistryPath() });
    }
    const document = JSON.parse(byteVectorToText(current, this.config.maxMessageBytes));
    this.contactRegistryValue = current;
    return document;
  }

  contactRegistryEvidence() {
    if (!this.contactRegistryValue) throw new Error("Contact registry has not been loaded");
    return structuredClone(this.contactRegistryValue);
  }

  async saveContactRegistry(document) {
    if (!this.contactRegistryValue) throw new Error("Contact registry has not been loaded");
    const value = textToByteVector(JSON.stringify(document));
    const updated = await this.putCurrent({
      path: this.contactRegistryPath(), value, expected: this.contactRegistryValue, "expression?": false,
    });
    if (updated !== true) throw new Error("Contact registry changed in another session; reload before editing");
    this.contactRegistryValue = value;
  }

  async reconcileContactAuthorizations(contacts) {
    const desired = contacts.map(contactAuthorizationRule);
    const desiredKeys = new Set(desired.map(authorizationRuleKey));
    const existing = await this.getAuthorizations();
    const existingKeys = new Set(existing.map(authorizationRuleKey));

    // Publish desired rules before contracting stale Messenger mailbox authority.
    for (const rule of desired) {
      if (!existingKeys.has(authorizationRuleKey(rule))) {
        if (await this.authorize(rule) !== true) throw new Error("Gateway did not accept a contact authorization");
      }
    }
    for (const rule of existing) {
      if (isMessengerMailboxRule(rule) && !desiredKeys.has(authorizationRuleKey(rule))) {
        if (await this.deauthorize(rule) !== true) throw new Error("Gateway did not remove a stale contact authorization");
      }
    }

    const settled = await this.getAuthorizations();
    const settledOwned = settled.filter(isMessengerMailboxRule).map(authorizationRuleKey).sort();
    const expectedOwned = [...desiredKeys].sort();
    if (JSON.stringify(settledOwned) !== JSON.stringify(expectedOwned)) {
      throw new Error("Contact authorization reconciliation did not settle exactly");
    }
    return Object.fromEntries(contacts.map((contact) => [contact.id, "grant-ready"]));
  }

  prepare(contact, body, options = {}) {
    const id = options.id ?? crypto.randomUUID();
    this.requireLocalIdentity();
    const envelope = makeEnvelope({
      id,
      localOwner: this.localOwner,
      localJournal: this.localJournal,
      contact,
      body,
      createdAt: options.createdAt,
      inReplyTo: options.inReplyTo,
      conversationId: options.conversationId,
      participants: options.participants,
    });
    const bytes = JSON.stringify(envelope);
    if (new TextEncoder().encode(bytes).length > this.config.maxMessageBytes) throw new Error(`Envelope exceeds ${this.config.maxMessageBytes} bytes`);
    const value = textToByteVector(bytes);
    return {
      contactId: contact.id,
      envelope,
      contentHex: value["*type/byte-vector*"],
      request: {
        path: outgoingMailboxPath(this.localPathConfig(), contact, id),
        value,
        "expression?": false,
        $federation: { route: [...contact.route] },
      },
    };
  }

  async sendPrepared(prepared) {
    const result = await this.putCurrent(prepared.request);
    if (result !== true) throw new Error("Gateway did not accept the mailbox write");
    return { envelope: prepared.envelope, contentHex: prepared.contentHex };
  }

  async send(contact, body, options = {}) {
    return this.sendPrepared(this.prepare(contact, body, options));
  }

  async listOutgoingCopies(contact) {
    const result = await this.readCurrent({
      path: outgoingMailboxPath(this.localPathConfig(), contact),
      $federation: { route: [...contact.route] },
    });
    return parseDirectory(result).filter((entry) => entry.kind === "value");
  }

  async readOutgoingCopy(contact, id) {
    const result = await this.readCurrent({
      path: outgoingMailboxPath(this.localPathConfig(), contact, id),
      $federation: { route: [...contact.route] },
    });
    const text = byteVectorToText(result, this.config.maxMessageBytes);
    this.requireLocalIdentity();
    const envelope = validateEnvelope(JSON.parse(text), {
      id,
      identity: this.localOwner,
      journal: this.localJournal,
      localOwner: contact.identity,
      localJournal: contact.journal,
    });
    return { envelope, contentHex: result["*type/byte-vector*"] };
  }

  async listIncoming(contact) {
    const result = await this.readCurrent({ path: incomingMailboxPath(this.localPathConfig(), contact) });
    return parseDirectory(result).filter((entry) => entry.kind === "value");
  }

  async readIncoming(contact, id) {
    const result = await this.readCurrent({ path: incomingMailboxPath(this.localPathConfig(), contact, id) });
    const text = byteVectorToText(result, this.config.maxMessageBytes);
    this.requireLocalIdentity();
    const envelope = validateEnvelope(JSON.parse(text), {
      id,
      identity: contact.identity,
      journal: contact.journal,
      localOwner: this.localOwner,
      localJournal: this.localJournal,
    });
    return { envelope, contentHex: result["*type/byte-vector*"] };
  }
}
