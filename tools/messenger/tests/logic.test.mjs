import assert from "node:assert/strict";
import test from "node:test";
import {
  assertContactRemovable, authorizationRuleKey, byteVectorToText, contactAuthorizationRule, contactRegistryDocument,
  conversationIdForContact, findReplyParent, incomingMailboxPath, isMessengerMailboxRule, makeEnvelope, mergeMessage, migrateRouteCanonicalRegistry, normalizeContacts,
  outgoingMailboxPath, parseDirectory, persistedMessages, readyGroupTargets, replyExcerpt, replyReferenceFor, textToByteVector, validateEnvelope,
} from "../public/logic.mjs";

const config = { localOwner: "thiennam", localJournal: "galactica" };
const contact = { contactId: "rocky", id: "rocky", handle: "Rocky", identity: "rocky", journal: "rocky", owner: "rocky", route: ["rocky"], incomingPrincipal: ["rocky", "*state*", "rocky"] };
const id = "123e4567-e89b-42d3-a456-426614174000";

test("UTF-8 message bytes round trip exactly", () => {
  const value = "hello, Rocky — 👋";
  assert.equal(byteVectorToText(textToByteVector(value)), value);
  assert.throws(() => byteVectorToText({ "*type/byte-vector*": "0" }), /Invalid/);
  assert.throws(() => byteVectorToText({ "*type/byte-vector*": "ff" }), /encoded data/);
});

test("directory parsing is complete and fail closed", () => {
  assert.deepEqual(parseDirectory(["nothing"]), []);
  assert.deepEqual(parseDirectory(["directory", { [id]: "value" }, true]), [{ name: id, kind: "value" }]);
  assert.throws(() => parseDirectory(["directory", { bad: "surprise" }, true]), /Invalid/);
  assert.throws(() => parseDirectory(["directory", { [id]: "value" }, true, "trailing"]), /complete/);
  assert.throws(() => parseDirectory(["directory", { [id]: "value" }, false]), /incomplete/);
  assert.throws(() => parseDirectory(["directory", [[id, "value"]], true]), /children/);
});

test("v1 envelope remains path-derived and ordinary", () => {
  const envelope = makeEnvelope({ id, localOwner: "thiennam", localJournal: "galactica", contact, body: "test", createdAt: "2026-08-16T12:00:00.000Z" });
  assert.equal(envelope.from, "thiennam@galactica");
  assert.equal(envelope.to, "rocky@rocky");
  assert.equal(validateEnvelope(envelope, { id, identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky" }).body, "test");
  assert.throws(() => validateEnvelope({ ...envelope, from: "someone@elsewhere" }, { id, identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky" }), /sender/);
  assert.throws(() => validateEnvelope({ ...envelope, id: crypto.randomUUID() }, { id, identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky" }), /key/);
});

test("v2 group envelope adds only stable conversation and participant context", () => {
  const conversationId = "223e4567-e89b-42d3-a456-426614174000";
  const participants = ["thiennam@galactica", "rocky@rocky", "grace@grace"];
  const envelope = makeEnvelope({
    id, localOwner: "thiennam", localJournal: "galactica", contact, body: "group hello",
    createdAt: "2026-08-16T12:00:00.000Z", conversationId, participants,
    inReplyTo: { from: "grace@grace", id: "323e4567-e89b-42d3-a456-426614174000" },
  });
  assert.deepEqual(envelope.participants, ["grace@grace", "rocky@rocky", "thiennam@galactica"]);
  const normalized = validateEnvelope(envelope, {
    id, identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky",
  });
  assert.equal(normalized.version, 2);
  assert.equal(normalized.conversationId, conversationId);
  assert.deepEqual(normalized.inReplyTo, { from: "grace@grace", id: "323e4567-e89b-42d3-a456-426614174000" });
  assert.throws(() => validateEnvelope({ ...envelope, participants: [...envelope.participants].reverse() }, {
    id, identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky",
  }), /unique and sorted/);
  assert.throws(() => validateEnvelope({ ...envelope, participants: ["rocky@rocky", "thiennam@galactica"] }, {
    id, identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky",
  }), /participants/);
  const outsiderParent = { from: "outsider@elsewhere", id: "423e4567-e89b-42d3-a456-426614174000" };
  assert.throws(() => validateEnvelope({ ...envelope, inReplyTo: outsiderParent }, {
    id, identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky",
  }), /reply id/);
  assert.throws(() => makeEnvelope({
    id, localOwner: "thiennam", localJournal: "galactica", contact, body: "bad parent",
    conversationId, participants, inReplyTo: outsiderParent,
  }), /reply id/);
  assert.throws(() => makeEnvelope({
    id, localOwner: "thiennam", localJournal: "galactica", contact, body: "bad endpoint",
    conversationId, participants: [".@invalid", "rocky@rocky", "thiennam@galactica"],
  }), /participants/);
  const tooMany = Array.from({ length: 17 }, (_, index) => `peer${index}@journal${index}`);
  assert.throws(() => makeEnvelope({
    id, localOwner: "peer0", localJournal: "journal0",
    contact: { identity: "peer1", journal: "journal1" }, body: "too many", conversationId, participants: tooMany,
  }), /participants/);
});

test("reply references resolve locally with an unavailable-parent fallback", () => {
  const conversationId = "contact:rocky";
  const parent = { sourceKey: "rocky/root", id, from: "rocky@rocky", conversationId, body: "A long\nparent   message" };
  const direct = { id: crypto.randomUUID(), from: "thiennam@galactica", conversationId, inReplyTo: id.toUpperCase() };
  assert.equal(replyReferenceFor(parent), id);
  assert.equal(findReplyParent([parent], direct), parent);
  assert.equal(findReplyParent([], direct), undefined);
  assert.equal(replyExcerpt(parent.body), "A long parent message");
  assert.equal(replyExcerpt("123456", 5), "1234…");
});

test("group replies use source-qualified parents", () => {
  const conversationId = "group:223e4567-e89b-42d3-a456-426614174000";
  const rocky = { sourceKey: "rocky/root", id, from: "rocky@rocky", conversationId, body: "Rocky" };
  const grace = { sourceKey: "grace/root", id, from: "grace@grace", conversationId, body: "Grace" };
  const reference = replyReferenceFor(grace, true);
  assert.deepEqual(reference, { from: "grace@grace", id });
  assert.equal(findReplyParent([rocky, grace], { conversationId, inReplyTo: reference }), grace);
  assert.throws(() => replyReferenceFor({ id }, true), /sender/);
});

test("fixed groups fail before sending when any participant is unavailable", () => {
  const grace = { ...contact, id: "grace", identity: "grace", journal: "grace" };
  const group = { id: "group", contactIds: ["rocky", "grace"] };
  const contacts = [contact, grace];
  const grants = { rocky: "grant-ready", grace: "grant-ready" };
  assert.deepEqual(readyGroupTargets(group, contacts, [], grants), contacts);
  assert.throws(() => readyGroupTargets(group, contacts, ["grace"], grants), /blocked/);
  assert.throws(() => readyGroupTargets(group, [contact], [], grants), /no longer configured/);
  assert.throws(() => readyGroupTargets(group, contacts, [], { ...grants, grace: "grant-pending" }), /exact incoming grant/);
  assert.throws(() => readyGroupTargets({ contactIds: Array(16).fill("rocky") }, contacts, [], grants), /2\.\.15 contacts/);
});

test("contacts referenced by fixed groups cannot be removed", () => {
  assert.doesNotThrow(() => assertContactRemovable("grace", [{ contactIds: ["rocky", "thiennam"] }]));
  assert.throws(() => assertContactRemovable("rocky", [{ contactIds: ["rocky", "thiennam"] }]), /Remove or close groups/);
});

test("v1 receive remains field-order independent and drops unknown fields", () => {
  const parsed = JSON.parse(`{"body":"first","unknown":"ignored","version":1,"to":"rocky@rocky","createdAt":"2026-08-16T12:00:00.000Z","from":"thiennam@galactica","id":"${id}","body":"last"}`);
  const normalized = validateEnvelope(parsed, { id: id.toUpperCase(), identity: "thiennam", journal: "galactica", localOwner: "rocky", localJournal: "rocky" });
  assert.equal(normalized.body, "last");
  assert.equal("unknown" in normalized, false);
  assert.equal(normalized.id, id);
});

test("mailbox paths separate local sender and remote owner", () => {
  assert.deepEqual(outgoingMailboxPath(config, contact), ["*state*", "rocky", "mailbox", "inbox", "galactica", "thiennam"]);
  assert.deepEqual(outgoingMailboxPath(config, contact, id), ["*state*", "rocky", "mailbox", "inbox", "galactica", "thiennam", id]);
  assert.deepEqual(incomingMailboxPath(config, contact), ["*state*", "thiennam", "mailbox", "inbox", "rocky", "rocky"]);
  assert.deepEqual(incomingMailboxPath(config, contact, id), ["*state*", "thiennam", "mailbox", "inbox", "rocky", "rocky", id]);
  assert.equal(conversationIdForContact("rocky"), "contact:rocky");
});

test("contact configuration rejects duplicates and missing routes", () => {
  const normalized = normalizeContacts({ version: 1, contacts: [contact] })[0];
  assert.equal(normalized.status, "configured");
  assert.equal("enabled" in normalized, false);
  assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, enabled: false }] }), /removed enabled field/);
  assert.throws(() => normalizeContacts({ version: 1, contacts: [contact, contact] }), /duplicate/);
  for (const contactId of [undefined, "", "-bad", "bad.id", "bad id", "x".repeat(129)]) {
    assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, contactId }] }), /contact id/);
  }
  for (const route of ["rocky", {}, 1, true, null, [], [""], ["."], [".."], ["galactica/rocky"], ["grace@galactica"], ["white space"], ["colon:"], ["comma,"], ["percent%"], ["tilde~"]]) {
    assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, route }] }), /route/);
    assert.throws(() => contactRegistryDocument([{ ...contact, route }]), /route/);
  }
  const multi = { ...contact, route: ["galactica", "rocky"] };
  assert.deepEqual(normalizeContacts(contactRegistryDocument([multi]))[0].route, multi.route);
  assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, incomingPrincipal: ["rocky"] }] }), /incoming.*principal/);
  assert.deepEqual(normalizeContacts({ version: 1, contacts: [{ ...contact, incomingPrincipal: ["other/path", "*state*", "rocky"] }] })[0].incomingPrincipal,
    ["other/path", "*state*", "rocky"]);
  for (const value of ["bad,id", "bad:colon", "bad\"quote", "bad'quote", "snowman☃", ".", "..", "bad space"]) {
    assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, identity: value, incomingPrincipal: ["rocky", "*state*", value] }] }), /symbols/);
    assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, owner: value }] }), /symbols/);
    assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, journal: value }] }), /symbols/);
    assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, incomingPrincipal: [value, "*state*", "rocky"] }] }), /incoming.*principal/);
  }
  assert.throws(() => normalizeContacts({ version: 1, contacts: [{ ...contact, identity: "rocky@example" }] }), /symbols/);
  assert.equal("identityIdBase64" in normalizeContacts({ version: 1, contacts: [{ ...contact, identityIdBase64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" }] })[0], false);
  assert.equal("identityIdBase64" in normalizeContacts({ version: 1, contacts: [{ ...contact, identityIdBase64: "self-attested" }] })[0], false);
});

test("route-canonical registry migration strips only the old identity pin with one exact save", async () => {
  const { id: _runtimeId, ...persistedContact } = contact;
  const registry = { version: 1, contacts: [{ ...persistedContact, identityIdBase64: "public-old-binding" }] };
  const previousValue = { "*type/byte-vector*": "0102" };
  const saved = [];
  const result = await migrateRouteCanonicalRegistry(registry, previousValue, async (document) => saved.push(document));
  assert.equal(result.ready, true);
  assert.equal(result.migrated, true);
  assert.equal(saved.length, 1);
  assert.deepEqual(saved[0], { version: 1, contacts: [persistedContact] });
  assert.deepEqual(result.retiredEvidence, { document: registry, value: previousValue });
});

test("noncanonical route components block migration before CAS", async () => {
  let calls = 0;
  for (const route of ["rocky", {}, 1, true, null, [], [""], ["."], [".."], ["galactica/rocky"], ["grace@galactica"], ["white space"], ["colon:"], ["comma,"], ["percent%"], ["tilde~"]]) {
    const registry = { version: 1, contacts: [{ ...contact, route }] };
    const result = await migrateRouteCanonicalRegistry(registry, { "*type/byte-vector*": "0102" }, async () => { calls += 1; });
    assert.equal(result.ready, false);
    assert.match(result.error, /ambiguous/);
  }
  assert.equal(calls, 0);
});

test("asymmetric incoming messaging principal is preserved and not a profile migration trigger", async () => {
  const { id: _runtimeId, ...persistedContact } = contact;
  const asymmetric = { ...persistedContact, incomingPrincipal: ["other", "*state*", "rocky"] };
  let calls = 0;
  const result = await migrateRouteCanonicalRegistry({ version: 1, contacts: [asymmetric] }, { "*type/byte-vector*": "0102" }, async () => { calls += 1; });
  assert.equal(calls, 0);
  assert.equal(result.ready, true);
  assert.equal(result.migrated, false);
  assert.deepEqual(result.document.contacts[0].incomingPrincipal, asymmetric.incomingPrincipal);
  assert.deepEqual(normalizeContacts(result.document)[0].incomingPrincipal, asymmetric.incomingPrincipal);
});

test("stale concurrent identity-pin stripping loses exact CAS without retry", async () => {
  const registry = { version: 1, contacts: [{ ...contact, identityIdBase64: "public-old-binding" }] };
  let committed = false;
  let calls = 0;
  const save = async () => {
    calls += 1;
    if (committed) throw new Error("exact CAS conflict");
    committed = true;
  };
  const winner = await migrateRouteCanonicalRegistry(registry, { "*type/byte-vector*": "0102" }, save);
  const stale = await migrateRouteCanonicalRegistry(registry, { "*type/byte-vector*": "0102" }, save);
  assert.equal(winner.ready, true);
  assert.equal("identityIdBase64" in winner.document.contacts[0], false);
  assert.equal(stale.ready, false);
  assert.match(stale.error, /exact CAS conflict/);
  assert.equal(calls, 2);
});

test("invalid full relationship symbols fail before migration CAS", async () => {
  let calls = 0;
  for (const value of ["bad,id", "bad:colon", "bad\"quote", "bad'quote", "snowman☃", ".", "..", "bad space"]) {
    for (const candidate of [
      { ...contact, identity: value, incomingPrincipal: ["rocky", "*state*", value] },
      { ...contact, owner: value },
      { ...contact, journal: value },
      { ...contact, incomingPrincipal: [value, "*state*", "rocky"] },
    ]) {
      const result = await migrateRouteCanonicalRegistry({ version: 1, contacts: [candidate] }, { "*type/byte-vector*": "0102" }, async () => { calls += 1; });
      assert.equal(result.ready, false);
      assert.match(result.error, /relationship symbols are invalid/);
    }
  }
  assert.equal(calls, 0);
});

test("duplicate stable contact IDs fail before migration CAS", async () => {
  let calls = 0;
  const result = await migrateRouteCanonicalRegistry(
    { version: 1, contacts: [contact, { ...contact, identity: "other", incomingPrincipal: ["other", "*state*", "other"] }] },
    { "*type/byte-vector*": "0102" }, async () => { calls += 1; },
  );
  assert.equal(result.ready, false);
  assert.match(result.error, /relationship symbols are invalid/);
  assert.equal(calls, 0);
});

test("route-canonical migration conflict fails closed without retry or messaging mutation", async () => {
  const registry = { version: 1, contacts: [{ ...contact, identityIdBase64: "public-old-binding" }] };
  let calls = 0;
  const result = await migrateRouteCanonicalRegistry(registry, { "*type/byte-vector*": "0102" }, async () => {
    calls += 1;
    throw new Error("exact CAS conflict");
  });
  assert.equal(calls, 1);
  assert.equal(result.ready, false);
  assert.equal(result.migrated, false);
  assert.match(result.error, /conflict/);
  assert.equal("identityIdBase64" in result.document.contacts[0], false);
  assert.equal(contact.identity, "rocky");
  assert.deepEqual(contact.route, ["rocky"]);
});

test("contact presence maps to one exact incoming mailbox authorization", () => {
  const rule = contactAuthorizationRule(contact);
  assert.deepEqual(rule, {
    principal: ["rocky", "*state*", "rocky"],
    "key-index": [-32, -1],
    path: ["mailbox", "inbox", "rocky", "rocky"],
    "put!": true,
    "use!": { "read-only?": true },
  });
  assert.equal(isMessengerMailboxRule(rule), true);
  assert.match(authorizationRuleKey(rule), /mailbox/);
});

test("source-qualified message merge rejects changed bytes", () => {
  const first = { sourceKey: "galactica/thiennam/id", contentHex: "00", status: "seen" };
  assert.equal(mergeMessage([], first).length, 1);
  assert.equal(mergeMessage([first], { ...first, status: "validated" })[0].status, "validated");
  assert.throws(() => mergeMessage([first], { ...first, contentHex: "01" }), /changed/);
});

test("persisted history retains newest messages by creation time rather than poll insertion order", () => {
  const outgoing = { sourceKey: "outbound/root", direction: "outbound", body: "hello, world!", createdAt: "2026-08-22T00:00:10.000Z" };
  const olderInbound = Array.from({ length: 3 }, (_, index) => ({
    sourceKey: `peer/old-${index}`, direction: "inbound", body: "old",
    createdAt: `2026-08-21T00:00:0${index}.000Z`, observedAt: "2026-08-22T00:00:20.000Z",
  }));
  const retained = persistedMessages([], [outgoing, ...olderInbound], 3);
  assert.equal(retained.some((message) => message.sourceKey === outgoing.sourceKey), true);
  assert.equal(retained.at(-1).body, "hello, world!");
});

test("persisted history merges tabs, keeps the latest lifecycle, and removes prepared wire copies", () => {
  const stored = [{
    sourceKey: "outbound/root", status: "prepared", createdAt: "2026-08-22T00:00:00.000Z",
    updatedAt: "2026-08-22T00:00:01.000Z", prepared: { peer: { contentHex: "00" } },
  }];
  const current = [{
    sourceKey: "outbound/root", status: "write-accepted", createdAt: "2026-08-22T00:00:00.000Z",
    updatedAt: "2026-08-22T00:00:02.000Z", outcomes: { peer: "write-accepted" },
  }, { sourceKey: "peer/reply", status: "validated", createdAt: "2026-08-22T00:00:03.000Z" }];
  const retained = persistedMessages(stored, current);
  assert.equal(retained.length, 2);
  assert.equal(retained[0].status, "write-accepted");
  assert.equal("prepared" in retained[0], false);
});
