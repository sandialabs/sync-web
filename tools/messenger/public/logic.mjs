export const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
export const SYMBOL_RE = /^[A-Za-z0-9_.*+!<>=?/-]+$/;
const ROUTE_COMPONENT_RE = /^[A-Za-z0-9_.*+!<>=?-]+$/;
const CONTACT_ID_RE = /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/;

export function textToByteVector(value) {
  const bytes = new TextEncoder().encode(value);
  let hex = "";
  for (const byte of bytes) hex += byte.toString(16).padStart(2, "0");
  return { "*type/byte-vector*": hex };
}

export function byteVectorToText(value, maxBytes = 65_536) {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Expected a byte vector");
  const hex = value["*type/byte-vector*"];
  if (typeof hex !== "string" || hex.length % 2 || /[^0-9a-f]/i.test(hex)) throw new Error("Invalid byte vector");
  if (hex.length / 2 > maxBytes) throw new Error(`Message exceeds ${maxBytes} bytes`);
  const bytes = new Uint8Array(hex.length / 2);
  for (let index = 0; index < bytes.length; index += 1) bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
}

export function parseDirectory(value) {
  if (Array.isArray(value) && value.length === 1 && value[0] === "nothing") return [];
  if (!Array.isArray(value) || value[0] !== "directory" || value.length !== 3) throw new Error("Expected a complete directory response");
  const children = value[1];
  if (!children || typeof children !== "object" || Array.isArray(children)) throw new Error("Invalid directory children");
  const entries = Object.entries(children).map(([name, kind]) => {
    if (!name || !["value", "directory", "unknown", "nothing"].includes(kind)) throw new Error("Invalid directory entry");
    return { name, kind };
  });
  if (value[2] !== true) throw new Error("Mailbox directory response is incomplete");
  return entries;
}

function endpoint(owner, journal) { return `${owner}@${journal}`; }

function validEndpoint(value) {
  if (typeof value !== "string") return false;
  const parts = value.split("@");
  return parts.length === 2 && parts.every((part) => SYMBOL_RE.test(part) && part !== "." && part !== "..");
}

export function validateEnvelope(input, expected) {
  if (!input || ![1, 2].includes(input.version) || !UUID_RE.test(input.id)) throw new Error("Invalid message version or id");
  if (input.id.toLowerCase() !== expected.id.toLowerCase()) throw new Error("Mailbox key does not match envelope id");
  if (input.from !== endpoint(expected.identity, expected.journal)) throw new Error("Message sender does not match mailbox path");
  if (input.to !== endpoint(expected.localOwner, expected.localJournal)) throw new Error("Message recipient does not match this inbox");
  if (typeof input.body !== "string" || input.body.length === 0) throw new Error("Message body must be non-empty text");
  if (typeof input.createdAt !== "string" || new Date(input.createdAt).toISOString() !== input.createdAt) throw new Error("Invalid message timestamp");
  if (input.version === 1) {
    if (input.inReplyTo !== undefined && !UUID_RE.test(input.inReplyTo)) throw new Error("Invalid reply id");
    return {
      version: 1,
      id: input.id.toLowerCase(),
      from: input.from,
      to: input.to,
      createdAt: input.createdAt,
      body: input.body,
      ...(input.inReplyTo ? { inReplyTo: input.inReplyTo.toLowerCase() } : {}),
    };
  }
  if (!UUID_RE.test(input.conversationId)) throw new Error("Invalid group conversation id");
  if (!Array.isArray(input.participants) || input.participants.length < 3 || input.participants.length > 16 || !input.participants.every(validEndpoint)) throw new Error("Invalid group participants");
  const participants = [...input.participants];
  if (new Set(participants).size !== participants.length || participants.join("\0") !== [...participants].sort().join("\0")) throw new Error("Group participants must be unique and sorted");
  if (!participants.includes(input.from) || !participants.includes(input.to)) throw new Error("Group participants do not include sender and recipient");
  if (input.inReplyTo !== undefined && (!input.inReplyTo || !validEndpoint(input.inReplyTo.from) || !participants.includes(input.inReplyTo.from) || !UUID_RE.test(input.inReplyTo.id))) throw new Error("Invalid group reply id");
  return {
    version: 2,
    id: input.id.toLowerCase(),
    conversationId: input.conversationId.toLowerCase(),
    participants,
    from: input.from,
    to: input.to,
    createdAt: input.createdAt,
    body: input.body,
    ...(input.inReplyTo ? { inReplyTo: { from: input.inReplyTo.from, id: input.inReplyTo.id.toLowerCase() } } : {}),
  };
}

export function makeEnvelope({ id, localOwner, localJournal, contact, body, createdAt = new Date().toISOString(), inReplyTo, conversationId, participants }) {
  if (!UUID_RE.test(id)) throw new Error("Invalid message id");
  if (!contact || !contact.identity || !contact.journal) throw new Error("Invalid contact");
  if (typeof body !== "string" || !body.trim()) throw new Error("Message body must be non-empty text");
  const common = {
    id: id.toLowerCase(),
    from: endpoint(localOwner, localJournal),
    to: endpoint(contact.identity, contact.journal),
    createdAt,
    body,
  };
  if (conversationId !== undefined) {
    if (!UUID_RE.test(conversationId) || !Array.isArray(participants)) throw new Error("Invalid group conversation");
    const sorted = [...new Set(participants)].sort();
    if (sorted.length < 3 || sorted.length > 16 || !sorted.every(validEndpoint) || !sorted.includes(common.from) || !sorted.includes(common.to)) throw new Error("Invalid group participants");
    if (inReplyTo && (!validEndpoint(inReplyTo.from) || !sorted.includes(inReplyTo.from) || !UUID_RE.test(inReplyTo.id))) throw new Error("Invalid group reply id");
    return {
      version: 2, ...common, conversationId: conversationId.toLowerCase(), participants: sorted,
      ...(inReplyTo ? { inReplyTo: { from: inReplyTo.from, id: inReplyTo.id.toLowerCase() } } : {}),
    };
  }
  return { version: 1, ...common, ...(inReplyTo ? { inReplyTo } : {}) };
}

export function outgoingMailboxPath(config, contact, id) {
  return ["*state*", contact.owner, "mailbox", "inbox", config.localJournal, config.localOwner, ...(id ? [id] : [])];
}

export function incomingMailboxPath(config, contact, id) {
  return ["*state*", config.localOwner, "mailbox", "inbox", contact.journal, contact.identity, ...(id ? [id] : [])];
}

export function conversationIdForContact(contactId) { return `contact:${contactId}`; }
export function conversationIdForGroup(groupId) { return `group:${groupId}`; }

export function readyGroupTargets(group, contacts, blocked, grants) {
  if (group.contactIds.length < 2 || group.contactIds.length > 15) throw new Error("A group requires 2..15 contacts");
  const targets = group.contactIds.map((id) => contacts.find((contact) => contact.id === id));
  if (targets.some((contact) => !contact)) throw new Error("A fixed group participant is no longer configured");
  if (targets.some((contact) => blocked.includes(contact.id))) throw new Error("A fixed group participant is blocked");
  if (targets.some((contact) => grants[contact.id] !== "grant-ready")) throw new Error("Every recipient must have an exact incoming grant before sending");
  return targets;
}

export function assertContactRemovable(contactId, groups) {
  if (groups.some((group) => group.contactIds.includes(contactId))) throw new Error("Remove or close groups containing this contact first");
}

function validSymbol(value) {
  return typeof value === "string" && SYMBOL_RE.test(value) && value !== "." && value !== "..";
}

function validRouteComponent(value) {
  return typeof value === "string" && ROUTE_COMPONENT_RE.test(value) && value !== "." && value !== "..";
}

export function normalizeContacts(document) {
  if (!document || document.version !== 1 || !Array.isArray(document.contacts)) throw new Error("Contacts document version must be 1");
  const ids = new Set();
  return document.contacts.map((contact) => {
    if (!contact || typeof contact.contactId !== "string" || !CONTACT_ID_RE.test(contact.contactId) || ids.has(contact.contactId)) throw new Error("Invalid or duplicate contact id");
    if (typeof contact.handle !== "string" || !contact.handle.trim()) throw new Error(`Invalid contact ${contact.contactId}`);
    if (![contact.identity, contact.journal, contact.owner].every(validSymbol)) throw new Error(`Invalid transport symbols for ${contact.contactId}`);
    if (!Array.isArray(contact.route) || contact.route.length === 0 || !contact.route.every(validRouteComponent)
      || contact.route.join("/").split("/").join("/") !== contact.route.join("/")) throw new Error(`Invalid route for ${contact.contactId}`);
    if (!Array.isArray(contact.incomingPrincipal) || contact.incomingPrincipal.length < 3
      || !contact.incomingPrincipal.every(validSymbol)
      || contact.incomingPrincipal.at(-2) !== "*state*" || contact.incomingPrincipal.at(-1) !== contact.identity) {
      throw new Error(`Invalid incoming messaging principal for ${contact.contactId}`);
    }
    if (contact.enabled !== undefined) throw new Error(`Contact ${contact.contactId} uses the removed enabled field`);
    const { identityIdBase64: _obsoleteIdentityBinding, id: _legacyRuntimeId, ...routeCanonicalContact } = contact;
    ids.add(contact.contactId);
    return { ...routeCanonicalContact, id: contact.contactId, incomingPrincipal: [...contact.incomingPrincipal], status: contact.status ?? "configured" };
  });
}

export function contactRegistryDocument(contacts) {
  return {
    version: 1,
    contacts: contacts.map(({
      identityIdBase64: _obsoleteIdentityBinding,
      status: _runtimeStatus,
      id: _runtimeId,
      ...contact
    }) => {
      if (!Array.isArray(contact.route) || contact.route.length === 0 || !contact.route.every(validRouteComponent)
        || contact.route.join("/").split("/").join("/") !== contact.route.join("/")) {
        throw new Error(`Invalid route for ${contact.contactId ?? _runtimeId}`);
      }
      return {
        ...contact,
        route: [...contact.route],
        contactId: contact.contactId ?? _runtimeId,
      };
    }),
  };
}

export async function migrateRouteCanonicalRegistry(registry, previousValue, save) {
  if (!registry || registry.version !== 1 || !Array.isArray(registry.contacts) || typeof save !== "function") {
    throw new Error("Route-canonical registry migration input is invalid");
  }
  const invalidRoute = registry.contacts.find((contact) => !contact || !Array.isArray(contact.route) || contact.route.length === 0
    || !contact.route.every(validRouteComponent) || contact.route.join("/").split("/").join("/") !== contact.route.join("/"));
  const migrationContactIds = new Set();
  const invalidRelationship = registry.contacts.find((contact) => {
    const contactId = contact?.contactId ?? contact?.id;
    const invalid = !contact || typeof contactId !== "string" || !CONTACT_ID_RE.test(contactId) || migrationContactIds.has(contactId)
      || (contact.contactId !== undefined && contact.id !== undefined && contact.contactId !== contact.id)
      || ![contact.identity, contact.journal, contact.owner].every(validSymbol)
      || !Array.isArray(contact.incomingPrincipal) || contact.incomingPrincipal.length < 3
      || !contact.incomingPrincipal.every(validSymbol)
      || contact.incomingPrincipal.at(-2) !== "*state*" || contact.incomingPrincipal.at(-1) !== contact.identity;
    migrationContactIds.add(contactId);
    return invalid;
  });
  const required = registry.contacts.some((contact) => Object.hasOwn(contact, "identityIdBase64"));
  const retiredEvidence = { document: structuredClone(registry), value: structuredClone(previousValue) };
  if (invalidRoute) {
    return {
      ready: false, migrated: false, document: structuredClone(registry), retiredEvidence,
      error: `Configured route is ambiguous for ${invalidRoute.id}`,
    };
  }
  if (invalidRelationship) {
    return {
      ready: false, migrated: false, document: structuredClone(registry), retiredEvidence,
      error: `Configured relationship symbols are invalid for ${invalidRelationship?.id ?? "contact"}`,
    };
  }
  const document = required ? {
    version: 1,
    contacts: registry.contacts.map(({ identityIdBase64: _obsoleteIdentityBinding, ...contact }) => structuredClone(contact)),
  } : structuredClone(registry);
  if (!required) return { ready: true, migrated: false, document };
  try {
    await save(document);
    return { ready: true, migrated: true, document, retiredEvidence };
  } catch (error) {
    return {
      ready: false, migrated: false, document, retiredEvidence,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export function contactAuthorizationRule(contact) {
  return {
    principal: [...contact.incomingPrincipal],
    "key-index": [-32, -1],
    path: ["mailbox", "inbox", contact.journal, contact.identity],
    "put!": true,
    "use!": { "read-only?": true },
  };
}

export function authorizationRuleKey(rule) {
  return JSON.stringify({
    principal: rule.principal,
    "key-index": rule["key-index"],
    path: rule.path,
    "put!": rule["put!"],
    "use!": rule["use!"],
  });
}

export function isMessengerMailboxRule(rule) {
  return Array.isArray(rule?.principal) && rule.principal.length >= 3
    && rule.principal.at(-2) === "*state*" && validSymbol(rule.principal.at(-1))
    && Array.isArray(rule.path) && rule.path.length === 4
    && rule.path[0] === "mailbox" && rule.path[1] === "inbox"
    && validSymbol(rule.path[2]) && validSymbol(rule.path[3])
    && rule.path[3] === rule.principal.at(-1);
}

export function mergeMessage(messages, message) {
  const existing = messages.find((item) => item.sourceKey === message.sourceKey);
  if (!existing) return [...messages, message];
  if (existing.contentHex && message.contentHex && existing.contentHex !== message.contentHex) throw new Error(`Immutable message changed: ${message.sourceKey}`);
  return messages.map((item) => item.sourceKey === message.sourceKey ? { ...item, ...message } : item);
}

export function replyReferenceFor(message, group = false) {
  if (!message || !UUID_RE.test(message.id)) throw new Error("Reply target has an invalid message id");
  if (!group) return message.id;
  if (!validEndpoint(message.from)) throw new Error("Group reply target has an invalid sender");
  return { from: message.from, id: message.id };
}

export function findReplyParent(messages, message) {
  const reference = message?.inReplyTo;
  if (!reference) return undefined;
  const id = typeof reference === "string" ? reference : reference.id;
  if (!UUID_RE.test(id)) return undefined;
  return messages.find((candidate) => candidate.conversationId === message.conversationId
    && typeof candidate.id === "string" && candidate.id.toLowerCase() === id.toLowerCase()
    && (typeof reference === "string" || candidate.from === reference.from));
}

export function replyExcerpt(body, limit = 96) {
  if (!Number.isInteger(limit) || limit < 2) throw new Error("Reply excerpt limit must be at least 2");
  const text = typeof body === "string" ? body.replace(/\s+/g, " ").trim() : "";
  return text.length <= limit ? text : `${text.slice(0, limit - 1)}…`;
}

function messageRevisionTime(message) {
  return Date.parse(message.updatedAt || message.observedAt || message.createdAt) || 0;
}

function messageCreatedTime(message) {
  return Date.parse(message.createdAt || message.observedAt || message.updatedAt) || 0;
}

export function persistedMessages(stored, current, limit = 3000) {
  if (!Number.isInteger(limit) || limit < 1) throw new Error("Message history limit must be positive");
  const merged = new Map();
  for (const message of [...stored, ...current]) {
    if (!message || typeof message.sourceKey !== "string") continue;
    const compact = { ...message };
    delete compact.prepared;
    const existing = merged.get(compact.sourceKey);
    if (existing?.contentHex && compact.contentHex && existing.contentHex !== compact.contentHex) {
      throw new Error(`Immutable message changed: ${compact.sourceKey}`);
    }
    if (!existing || messageRevisionTime(compact) >= messageRevisionTime(existing)) {
      merged.set(compact.sourceKey, { ...existing, ...compact });
    }
  }
  return [...merged.values()]
    .sort((left, right) => messageCreatedTime(left) - messageCreatedTime(right) || left.sourceKey.localeCompare(right.sourceKey))
    .slice(-limit);
}

export function sortConversations(conversations, messages) {
  const latest = new Map();
  for (const message of messages) {
    const time = Date.parse(message.observedAt || message.createdAt) || 0;
    latest.set(message.conversationId, Math.max(latest.get(message.conversationId) || 0, time));
  }
  return [...conversations].sort((left, right) => (latest.get(right.id) || 0) - (latest.get(left.id) || 0) || left.name.localeCompare(right.name));
}
