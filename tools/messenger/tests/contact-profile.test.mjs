import assert from "node:assert/strict";
import test from "node:test";
import { webcrypto } from "node:crypto";
import {
  contactRelationshipFingerprint, contactSnapshot, createContactProfileViewCache, createMemoryContactProfiles, failedContactStatus, mergeContactProfileState, restoreContactSnapshot, serializedContactProfileCommit,
  statusMatchesContact, successfulContactStatus, validatedContactStatus,
} from "../public/contact-profile.mjs";

if (!globalThis.crypto) globalThis.crypto = webcrypto;
if (!globalThis.atob) globalThis.atob = (value) => Buffer.from(value, "base64").toString("binary");
if (!globalThis.btoa) globalThis.btoa = (value) => Buffer.from(value, "binary").toString("base64");

const identityIdBase64 = "48ULmmW9TeDTbLCWLPFzW83h4tAwYZEC+z799UU9NJ8=";
const contact = {
  contactId: "rocky", id: "rocky", identity: "rocky", journal: "rocky", owner: "rocky",
  incomingPrincipal: ["rocky", "*state*", "rocky"], route: ["rocky"],
};
const source = `((schema sync-agent-profile-v1.experimental)
 (identity-id #u(227 197 11 154 101 189 77 224 211 108 176 150 44 241 115 91 205 225 226 208 48 97 145 2 251 62 253 245 69 61 52 159))
 (journal rocky)
 (owner rocky)
 (revision 1)
 (display-name "Rocky")
 (pronouns ("he/him"))
 (bio "Lead Synchronic Web implementation and engineering agent."))
`;
const raw = new TextEncoder().encode(source);
const rawHex = Buffer.from(raw).toString("hex");
const rawSha256 = "894f091c7ff64812d96855869dbce19dae417b9923e801726342def6c1fea5c9";
const profile = {
  schemaVersion: 1, revision: 1, identityIdBase64, displayName: "Rocky", pronouns: ["he/him"],
  bio: "Lead Synchronic Web implementation and engineering agent.",
};
const current = {
  profile, value: { "*type/byte-vector*": rawHex },
  observedAt: "2026-08-24T04:00:00.000Z", stale: false, rawSha256,
};

function serializedMemoryStore() {
  let value;
  let tail = Promise.resolve();
  const lock = (operation) => {
    const result = tail.then(operation);
    tail = result.catch(() => {});
    return result;
  };
  return { lock, read: () => structuredClone(value), write: (next) => { value = structuredClone(next); }, current: () => structuredClone(value) };
}

async function currentForSource(value, observedAt) {
  const bytes = new TextEncoder().encode(value);
  const digest = Buffer.from(await crypto.subtle.digest("SHA-256", bytes)).toString("hex");
  let parsed = value.includes('(display-name "Rooky")') ? { ...profile, displayName: "Rooky" }
    : value.includes("(revision 2)") ? { ...profile, revision: 2 } : profile;
  if (value.includes(`identity-id #u(${"0 ".repeat(31)}0)`)) {
    parsed = { ...parsed, identityIdBase64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" };
  }
  return {
    profile: parsed, value: { "*type/byte-vector*": Buffer.from(bytes).toString("hex") },
    observedAt, stale: false, rawSha256: digest,
  };
}

test("contact snapshots retain and reconstruct exact source bytes", async () => {
  const snapshot = await contactSnapshot(contact, current);
  assert.equal(snapshot.source.rawSha256, rawSha256);
  assert.equal(snapshot.source.rawBytes, raw.length);
  assert.equal(snapshot.rawHex, rawHex);
  const status = successfulContactStatus(contact, snapshot, undefined, "2026-08-24T04:01:00.000Z");
  const restored = await restoreContactSnapshot(contact, structuredClone(snapshot), structuredClone(status));
  assert.deepEqual(restored.profile, profile);
  assert.equal(restored.value["*type/byte-vector*"], rawHex);
  assert.equal(restored.stale, false);
});

test("asymmetric incoming mailbox principal does not affect profile fingerprint or source evidence", async () => {
  const asymmetric = { ...contact, incomingPrincipal: ["other/path", "*state*", "rocky"] };
  let authoritative = contact;
  let fetches = 0;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => authoritative,
    fetchCurrent: async () => { fetches += 1; return current; },
  });
  assert.equal((await adapter.refresh(contact)).kind, "accepted");
  authoritative = asymmetric;
  assert.equal((await adapter.current(asymmetric)).profile.revision, 1);
  assert.equal((await adapter.renderCurrent(asymmetric)).source.authenticationScope, "route-owner-journal-path-current");
  assert.equal("incomingPrincipal" in (await adapter.renderCurrent(asymmetric)).source, false);
  assert.equal("principal" in (await adapter.renderCurrent(asymmetric)).source, false);
  assert.equal(fetches, 1);
});

test("asymmetric incoming mailbox principal change preserves rollback continuity", async () => {
  const asymmetric = { ...contact, incomingPrincipal: ["other/path", "*state*", "rocky"] };
  let authoritative = contact;
  let fetched = await currentForSource(source.replace("(revision 1)", "(revision 2)"), "2026-08-24T04:09:00.000Z");
  let attempt = 0;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => authoritative,
    fetchCurrent: async () => fetched,
    clock: () => `2026-08-24T04:${10 + attempt++}:00.000Z`,
    randomId: () => `attempt-asymmetric-${attempt}`,
  });
  assert.equal((await adapter.refresh(contact)).current.profile.revision, 2);
  authoritative = asymmetric;
  fetched = current;
  const rollback = await adapter.refresh(asymmetric);
  assert.equal(rollback.kind, "rejected");
  assert.equal(rollback.error, "rollback");
  assert.equal(rollback.current.profile.revision, 2);
  assert.equal((await adapter.current(asymmetric)).profile.revision, 2);
});

test("noncanonical route components reject before fetch or acceptance", async () => {
  let authoritative;
  let fetches = 0;
  const adapter = createMemoryContactProfiles({ resolveContact: () => authoritative, fetchCurrent: async () => { fetches += 1; return current; } });
  for (const route of [[], [""], ["."], [".."], ["galactica/rocky"], ["grace@galactica"], ["white space"], ["colon:"], ["comma,"], ["percent%"], ["tilde~"]]) {
    authoritative = { ...contact, route, incomingPrincipal: [...route, "*state*", "rocky"] };
    await assert.rejects(() => adapter.refresh(authoritative), /canonically bound/);
    await assert.rejects(() => contactSnapshot(authoritative, current), /canonically bound/);
  }
  assert.equal(fetches, 0);
});

test("invalid relationship symbols reject every profile operation before fetch", async () => {
  let authoritative;
  let fetches = 0;
  const adapter = createMemoryContactProfiles({ resolveContact: () => authoritative, fetchCurrent: async () => { fetches += 1; return current; } });
  const invalid = [
    { ...contact, contactId: undefined }, { ...contact, contactId: "" }, { ...contact, contactId: "-bad" },
    { ...contact, contactId: "bad.id" }, { ...contact, contactId: "bad id" }, { ...contact, contactId: "x".repeat(129) },
  ];
  for (const value of ["bad,id", "bad:colon", "bad\"quote", "bad'quote", "snowman☃", ".", "..", "bad space"]) {
    invalid.push(
      { ...contact, identity: value, incomingPrincipal: ["rocky", "*state*", value] },
      { ...contact, owner: value },
      { ...contact, journal: value },
      { ...contact, incomingPrincipal: [value, "*state*", "rocky"] },
    );
  }
  for (authoritative of invalid) {
    await assert.rejects(() => adapter.refresh(authoritative), /canonically bound/);
    await assert.rejects(() => adapter.current(authoritative), /canonically bound/);
    assert.throws(() => adapter.status(authoritative), /canonically bound/);
    await assert.rejects(() => adapter.renderCurrent(authoritative), /canonically bound/);
    await assert.rejects(() => contactSnapshot(authoritative, current), /canonically bound/);
  }
  assert.equal(fetches, 0);
});

test("injected memory adapter exposes refresh current status and bounded rendering without lookup network", async () => {
  let fetches = 0;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => contact,
    fetchCurrent: async () => { fetches += 1; return current; },
    clock: () => "2026-08-24T04:10:00.000Z",
    randomId: () => "attempt-memory",
  });
  const refreshed = await adapter.refresh(contact);
  assert.equal(refreshed.kind, "accepted");
  assert.equal(refreshed.current.profile.displayName, "Rocky");
  assert.equal(adapter.status(contact).attemptId, "attempt-memory");
  assert.equal((await adapter.current(contact)).rawSha256, rawSha256);
  const bounded = await adapter.renderCurrent(contact);
  assert.equal(bounded.profile.bio, null);
  assert.equal(bounded.instructions, false);
  assert.equal(bounded.source.authenticationScope, "route-owner-journal-path-current");
  assert.equal(bounded.source.terminalJournalContinuity, "unproven");
  assert.equal((await adapter.renderCurrent(contact, { includeBio: true })).profile.bio, profile.bio);
  assert.equal(fetches, 1);
});

test("a new browser-session consumer starts empty without profile network", async () => {
  let fetches = 0;
  const create = () => createMemoryContactProfiles({
    resolveContact: () => contact,
    fetchCurrent: async () => { fetches += 1; return current; },
  });
  const firstSession = create();
  await firstSession.refresh(contact);
  assert.equal((await firstSession.current(contact)).profile.displayName, "Rocky");
  const reloadedSession = create();
  assert.equal(await reloadedSession.current(contact), null);
  assert.equal(reloadedSession.status(contact), null);
  assert.equal(fetches, 1);
});

test("view cache discards an old completion after remove and same-id relationship rebind", async () => {
  let release;
  const waiting = new Promise((resolve) => { release = resolve; });
  const viewContact = { ...contact, id: "rocky" };
  let configured = viewContact;
  const oldValue = { profile, observedAt: current.observedAt, stale: false, rawSha256 };
  const consumer = {
    async refresh() { await waiting; return { kind: "accepted" }; },
    async current() { return oldValue; },
    status() { return { lastAttempt: "accepted" }; },
    async renderCurrent() { return { profile, source: { stale: false } }; },
  };
  const views = createContactProfileViewCache({
    consumer,
    resolveContact: (id) => configured?.id === id ? configured : undefined,
  });
  const pending = views.refresh(viewContact);
  views.remove(viewContact.id);
  configured = { ...viewContact, owner: "different-owner" };
  release();
  assert.deepEqual(await pending, { discarded: true });
  assert.equal(views.read(configured), undefined);
});

test("audit-only binding evidence change preserves fingerprint and rejects lower revision", async () => {
  const evidenceA = { ...contact, bindingEvidence: { sha256: "a".repeat(64) } };
  let authoritative = evidenceA;
  let fetched = await currentForSource(source.replace("(revision 1)", "(revision 2)"), "2026-08-24T04:09:00.000Z");
  let attempt = 0;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => authoritative,
    fetchCurrent: async () => fetched,
    clock: () => `2026-08-24T04:${10 + attempt++}:00.000Z`,
    randomId: () => `attempt-evidence-${attempt}`,
  });
  assert.equal((await adapter.refresh(evidenceA)).current.profile.revision, 2);
  authoritative = { ...evidenceA, bindingEvidence: { sha256: "b".repeat(64) } };
  fetched = current;
  const rollback = await adapter.refresh(authoritative);
  assert.equal(rollback.kind, "rejected");
  assert.equal(rollback.error, "rollback");
  assert.equal(rollback.current.profile.revision, 2);
  assert.equal((await adapter.current(authoritative)).profile.revision, 2);
});

test("memory current and render reject an outbound route change without refresh", async () => {
  let authoritative = contact;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => authoritative,
    fetchCurrent: async () => current,
    clock: () => "2026-08-24T04:10:00.000Z",
    randomId: () => "attempt-route",
  });
  await adapter.refresh(contact);
  const changedRoute = {
    ...contact, route: ["galactica", "rocky"], incomingPrincipal: ["galactica", "rocky", "*state*", "rocky"],
  };
  authoritative = changedRoute;
  await assert.rejects(() => adapter.current(contact), /not authoritative/);
  await assert.rejects(() => adapter.renderCurrent(contact), /not authoritative/);
  assert.throws(() => adapter.status(contact), /not authoritative/);
  assert.equal(await adapter.current(changedRoute), null);
  assert.equal(await adapter.renderCurrent(changedRoute), null);
  assert.equal(adapter.status(changedRoute), null);
});

test("relationship change during fetch cannot commit an ambiguous old-relationship attempt", async () => {
  let authoritative = contact;
  let release;
  const waiting = new Promise((resolve) => { release = resolve; });
  const adapter = createMemoryContactProfiles({
    resolveContact: () => authoritative,
    fetchCurrent: async () => { await waiting; return current; },
  });
  const pending = adapter.refresh(contact);
  authoritative = {
    ...contact, route: ["galactica", "rocky"], incomingPrincipal: ["galactica", "rocky", "*state*", "rocky"],
  };
  release();
  await assert.rejects(() => pending, /not authoritative/);
  await assert.rejects(() => adapter.current(contact), /not authoritative/);
  assert.equal(await adapter.current(authoritative), null);
  assert.equal(adapter.status(authoritative), null);
});

test("render lookup rejects a cached view after relationship binding changes", async () => {
  const viewContact = { ...contact, id: "rocky" };
  let configured = viewContact;
  const consumer = {
    async refresh() { return {}; },
    async current() { return { profile, observedAt: current.observedAt, stale: false, rawSha256 }; },
    status() { return { lastAttempt: "accepted" }; },
    async renderCurrent() { return { profile, source: { stale: false } }; },
  };
  const views = createContactProfileViewCache({ consumer, resolveContact: () => configured });
  await views.refresh(viewContact);
  assert.equal(views.read(viewContact).current.profile.displayName, "Rocky");
  configured = {
    ...viewContact, route: ["galactica", "rocky"], incomingPrincipal: ["galactica", "rocky", "*state*", "rocky"],
  };
  assert.equal(views.read(configured), undefined);
});

test("injected adapter preserves current on fetch failure and keeps messaging independent", async () => {
  let fail = false;
  let counter = 0;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => contact,
    fetchCurrent: async () => { if (fail) throw new TypeError("offline"); return current; },
    clock: () => `2026-08-24T04:1${counter++}:00.000Z`,
    randomId: () => `attempt-${counter}`,
  });
  await adapter.refresh(contact);
  fail = true;
  const unavailable = await adapter.refresh(contact);
  assert.equal(unavailable.kind, "unavailable");
  assert.equal(unavailable.current.profile.displayName, "Rocky");
  assert.equal(unavailable.current.stale, true);
  assert.equal(contact.identity, "rocky");
  assert.deepEqual(contact.route, ["rocky"]);
});

test("bounded timeout records one unavailable attempt with no fresh claims or retry", async () => {
  let fireTimeout;
  let fetches = 0;
  let releaseFetch;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => contact,
    fetchCurrent: async (_contact, { signal }) => {
      fetches += 1;
      return new Promise((resolve, reject) => {
        releaseFetch = resolve;
        signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")), { once: true });
      });
    },
    fetchTimeoutMs: 25,
    scheduleTimeout: (callback) => { fireTimeout = callback; return 1; },
    cancelTimeout: () => {},
    clock: () => "2026-08-24T04:20:00.000Z",
    randomId: () => "attempt-timeout-fresh",
  });
  const pending = adapter.refresh(contact);
  fireTimeout();
  const result = await pending;
  assert.equal(result.kind, "unavailable");
  assert.equal(result.current, null);
  assert.equal(result.status.lastAttempt, "unavailable");
  assert.equal(result.status.errorCode, "ProfileFetchTimeout");
  assert.equal(result.status.stale, false);
  assert.equal(result.status.snapshotRawSha256, null);
  assert.equal(await adapter.renderCurrent(contact), null);
  assert.deepEqual(adapter.renderStatus(contact), {
    version: 1,
    classification: "owner-local-profile-attempt-status",
    instructions: false,
    trustedForAuthority: false,
    remoteClaims: false,
    contactId: "rocky",
    attemptedAt: "2026-08-24T04:20:00.000Z",
    outcome: "unavailable",
    code: "ProfileFetchTimeout",
    currentRawSha256: null,
  });
  const reloadedViews = createContactProfileViewCache({ consumer: adapter, resolveContact: () => contact });
  const reloaded = await reloadedViews.reload(contact);
  assert.equal(reloaded.discarded, false);
  assert.equal(reloaded.value.current, undefined);
  assert.equal(reloaded.value.statusProjection.currentRawSha256, null);
  assert.equal(reloadedViews.read(contact).statusProjection.code, "ProfileFetchTimeout");
  assert.equal(fetches, 1);
  const status = structuredClone(adapter.status(contact));
  releaseFetch(current);
  await Promise.resolve();
  assert.deepEqual(adapter.status(contact), status);
  assert.equal(fetches, 1);
});

test("bounded timeout preserves incumbent stale and records exactly one attempt", async () => {
  let fireTimeout;
  let mode = "accept";
  let fetches = 0;
  let releaseFetch;
  const ignoredFetch = new Promise((resolve) => { releaseFetch = resolve; });
  let attempt = 0;
  const adapter = createMemoryContactProfiles({
    resolveContact: () => contact,
    fetchCurrent: async () => { fetches += 1; return mode === "accept" ? current : ignoredFetch; },
    fetchTimeoutMs: 25,
    scheduleTimeout: (callback) => { fireTimeout = callback; return attempt + 1; },
    cancelTimeout: () => {},
    clock: () => `2026-08-24T04:${20 + attempt}:00.000Z`,
    randomId: () => `attempt-timeout-${++attempt}`,
  });
  await adapter.refresh(contact);
  mode = "timeout";
  const pending = adapter.refresh(contact);
  fireTimeout();
  const result = await pending;
  assert.equal(result.kind, "unavailable");
  assert.equal(result.current.profile.revision, 1);
  assert.equal(result.current.stale, true);
  assert.equal(result.status.errorCode, "ProfileFetchTimeout");
  assert.equal(result.status.attemptId, "attempt-timeout-2");
  assert.equal(fetches, 2);
  const status = structuredClone(adapter.status(contact));
  releaseFetch(current);
  await Promise.resolve();
  assert.deepEqual(adapter.status(contact), status);
  assert.equal(fetches, 2);
});

test("relationship binding permits distinct configured owner and endpoint identity", async () => {
  const distinctContact = { ...contact, owner: "profile-owner" };
  const distinctSource = source.replace("(owner rocky)", "(owner profile-owner)");
  const distinctCurrent = await currentForSource(distinctSource, "2026-08-24T04:01:00.000Z");
  const snapshot = await contactSnapshot(distinctContact, distinctCurrent);
  assert.equal(snapshot.subject.qualified, "rocky@rocky");
  assert.equal(snapshot.subject.owner, "profile-owner");
  assert.equal((await restoreContactSnapshot(distinctContact, snapshot)).profile.displayName, "Rocky");
});

test("current and status reject changed profile binding but ignore directional mailbox principal", async () => {
  const snapshot = await contactSnapshot(contact, current);
  const status = successfulContactStatus(contact, snapshot);
  const wrongIdentity = { ...contact, identity: "other", incomingPrincipal: ["rocky", "*state*", "other"] };
  const wrongPrincipal = { ...contact, incomingPrincipal: ["galactica", "rocky", "*state*", "rocky"] };
  const wrongRoute = {
    ...contact, route: ["galactica", "rocky"], incomingPrincipal: ["galactica", "rocky", "*state*", "rocky"],
  };
  await assert.rejects(() => restoreContactSnapshot(wrongIdentity, snapshot, status), /another relationship/);
  assert.equal((await restoreContactSnapshot(wrongPrincipal, snapshot, status)).profile.revision, 1);
  await assert.rejects(() => restoreContactSnapshot(wrongRoute, snapshot, status), /another relationship/);
  assert.equal(statusMatchesContact(wrongIdentity, status), false);
  assert.equal(statusMatchesContact(wrongPrincipal, status), true);
  assert.equal(statusMatchesContact(wrongRoute, status), false);
  const reboundFailure = failedContactStatus(wrongIdentity, snapshot, status, new Error("offline"));
  assert.equal(reboundFailure.lastSuccessAt, null);
  assert.equal(reboundFailure.snapshotRawSha256, null);
  assert.equal(reboundFailure.stale, false);
});

test("tampered normalized claims, raw bytes, digest, and status fail closed", async () => {
  const snapshot = await contactSnapshot(contact, current);
  await assert.rejects(() => restoreContactSnapshot(contact, { ...snapshot, profile: { ...profile, displayName: "Other" } }), /exact source bytes/);
  await assert.rejects(() => restoreContactSnapshot(contact, { ...snapshot, rawHex: `${snapshot.rawHex.slice(0, -2)}00` }));
  await assert.rejects(() => restoreContactSnapshot(contact, { ...snapshot, source: { ...snapshot.source, rawSha256: "0".repeat(64) } }), /exact source bytes/);
  await assert.rejects(() => restoreContactSnapshot(contact, { ...snapshot, source: { ...snapshot.source, authenticationScope: "terminal-journal" } }), /snapshot is invalid/);
  await assert.rejects(() => restoreContactSnapshot(contact, { ...snapshot, source: { ...snapshot.source, terminalJournalContinuity: "proven" } }), /snapshot is invalid/);
  const status = { ...successfulContactStatus(contact, snapshot), stale: true, snapshotRawSha256: "0".repeat(64) };
  assert.equal((await restoreContactSnapshot(contact, snapshot, status)).stale, false);
  assert.equal((await restoreContactSnapshot(contact, snapshot, { ...status, lastAttempt: "other" })).stale, false);
  assert.equal((await restoreContactSnapshot(contact, snapshot, { ...status, snapshotRawSha256: rawSha256, authenticationScope: "terminal-journal" })).stale, false);
  assert.equal((await restoreContactSnapshot(contact, snapshot, { ...status, snapshotRawSha256: rawSha256, terminalJournalContinuity: "proven" })).stale, false);
});

test("validated status rejects contradictory current and no-current state combinations", async () => {
  const snapshot = await contactSnapshot(contact, current);
  const accepted = successfulContactStatus(contact, snapshot, undefined, "2026-08-24T04:01:00.000Z", "accepted");
  const unavailable = failedContactStatus(contact, undefined, undefined, new Error("offline"), "2026-08-24T04:02:00.000Z", "unavailable");
  assert.equal(validatedContactStatus(contact, accepted, snapshot).lastAttempt, "accepted");
  assert.equal(validatedContactStatus(contact, unavailable, undefined).lastAttempt, "unavailable");
  const contradictory = [
    { ...accepted, snapshotRawSha256: null },
    { ...accepted, stale: true },
    { ...accepted, errorCode: "impossible" },
    { ...accepted, lastAttempt: "rejected", snapshotRawSha256: null },
    { ...accepted, lastAttempt: "unavailable", snapshotRawSha256: null },
    { ...unavailable, lastAttempt: "accepted" },
    { ...unavailable, lastAttempt: "rejected" },
    { ...unavailable, stale: true },
    { ...unavailable, snapshotRawSha256: rawSha256 },
    { ...unavailable, lastSuccessAt: unavailable.lastAttemptAt },
  ];
  for (let iteration = 0; iteration < 100; iteration += 1) {
    for (const status of contradictory) assert.equal(validatedContactStatus(contact, status, undefined), null);
  }
  assert.equal(validatedContactStatus(contact, unavailable, snapshot), null);
  assert.equal(validatedContactStatus(contact, { ...accepted, lastAttempt: "rejected", stale: false, errorCode: "fork" }, snapshot), null);
  assert.equal(validatedContactStatus(contact, { ...accepted, lastAttempt: "unavailable", stale: true, errorCode: "offline" }, snapshot).lastAttempt, "unavailable");
});

test("cache byte bounds reject before hex decoding", async () => {
  const snapshot = await contactSnapshot(contact, current);
  await assert.rejects(() => restoreContactSnapshot(contact, {
    ...snapshot, rawHex: "00".repeat(4097), source: { ...snapshot.source, rawBytes: 4097 },
  }), /snapshot is invalid/);
  await assert.rejects(() => contactSnapshot(contact, {
    ...current, value: { "*type/byte-vector*": "00".repeat(4097) },
  }), /bytes are invalid/);
});

test("consumer transition rejects revision rollback and same-revision forks", async () => {
  const snapshot = await contactSnapshot(contact, current);
  await assert.rejects(() => contactSnapshot(contact, current, {
    ...snapshot, profile: { ...snapshot.profile, revision: 2 },
  }), /rollback/);
  const forkRaw = rawHex.replace(Buffer.from("Rocky").toString("hex"), Buffer.from("Rooky").toString("hex"));
  const forkBytes = Uint8Array.from(Buffer.from(forkRaw, "hex"));
  const forkSha = Buffer.from(await crypto.subtle.digest("SHA-256", forkBytes)).toString("hex");
  await assert.rejects(() => contactSnapshot(contact, {
    ...current,
    profile: { ...profile, displayName: "Rooky" },
    value: { "*type/byte-vector*": forkRaw },
    rawSha256: forkSha,
  }, snapshot), /fork/);
  const confirmed = await contactSnapshot(contact, { ...current, observedAt: "2026-08-24T04:03:00.000Z" }, snapshot);
  assert.equal(confirmed.source.rawSha256, snapshot.source.rawSha256);
  assert.equal(confirmed.source.observedAt, "2026-08-24T04:03:00.000Z");
});

test("embedded public identity metadata is immutable within one relationship fingerprint", async () => {
  const incumbent = await contactSnapshot(contact, current);
  const changedSource = source
    .replace(/\(identity-id #u\([^)]*\)\)/, `(identity-id #u(${"0 ".repeat(31)}0))`)
    .replace("(revision 1)", "(revision 2)");
  const candidate = await contactSnapshot(contact, await currentForSource(changedSource, "2026-08-24T04:02:00.000Z"));
  const merged = await mergeContactProfileState(contact, {
    snapshot: incumbent,
    status: successfulContactStatus(contact, incumbent, undefined, "2026-08-24T04:00:00.000Z", "attempt-a"),
  }, {
    snapshot: candidate,
    status: successfulContactStatus(contact, candidate, undefined, "2026-08-24T04:02:00.000Z", "attempt-b"),
  });
  assert.equal(merged.snapshot.profile.revision, 1);
  assert.equal(merged.snapshot.profile.identityIdBase64, identityIdBase64);
  assert.equal(merged.status.lastAttempt, "rejected");
  assert.equal(merged.status.errorCode, "identity-metadata-changed");
});

test("new relationship may establish changed public identity metadata baseline", async () => {
  let fetched = current;
  let authoritative = contact;
  const adapter = createMemoryContactProfiles({ resolveContact: () => authoritative, fetchCurrent: async () => fetched });
  await adapter.refresh(contact);
  const changedRoute = {
    ...contact, route: ["galactica", "rocky"], incomingPrincipal: ["galactica", "rocky", "*state*", "rocky"],
  };
  const changedSource = source.replace(/\(identity-id #u\([^)]*\)\)/, `(identity-id #u(${"0 ".repeat(31)}0))`);
  fetched = await currentForSource(changedSource, "2026-08-24T04:02:00.000Z");
  authoritative = changedRoute;
  const accepted = await adapter.refresh(changedRoute);
  assert.equal(accepted.kind, "accepted");
  assert.equal(accepted.current.profile.identityIdBase64, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
  await assert.rejects(() => adapter.current(contact), /not authoritative/);
});

test("corrupt separate status cannot discard an exact validated incumbent snapshot", async () => {
  const snapshot = await contactSnapshot(contact, current);
  const merged = await mergeContactProfileState(contact, { snapshot, status: { broken: true } }, {
    status: failedContactStatus(contact, undefined, undefined, new Error("offline"), "2026-08-24T04:03:00.000Z", "attempt-b"),
  });
  assert.equal(merged.snapshot.source.rawSha256, snapshot.source.rawSha256);
  assert.equal(merged.status.stale, true);
});

test("serialized two-tab merge retains higher revision over a later stale response", async () => {
  const revision1 = await contactSnapshot(contact, current);
  const revision2Current = await currentForSource(source.replace("(revision 1)", "(revision 2)"), "2026-08-24T04:02:00.000Z");
  const revision2 = await contactSnapshot(contact, revision2Current);
  const incumbent = {
    snapshot: revision2,
    status: successfulContactStatus(contact, revision2, undefined, "2026-08-24T04:02:00.000Z", "attempt-a"),
  };
  const stale = {
    snapshot: revision1,
    status: successfulContactStatus(contact, revision1, undefined, "2026-08-24T04:03:00.000Z", "attempt-b"),
  };
  const merged = await mergeContactProfileState(contact, incumbent, stale);
  assert.equal(merged.snapshot.profile.revision, 2);
  assert.equal(merged.status.lastAttempt, "rejected");
  assert.equal(merged.status.errorCode, "rollback");
  assert.equal(merged.status.snapshotRawSha256, revision2.source.rawSha256);
});

test("two tab commits re-read the incumbent inside one serialized transaction", async () => {
  const revision1 = await contactSnapshot(contact, current);
  const revision2 = await contactSnapshot(contact, await currentForSource(
    source.replace("(revision 1)", "(revision 2)"), "2026-08-24T04:02:00.000Z",
  ));
  const store = serializedMemoryStore();
  await serializedContactProfileCommit(contact, {
    snapshot: revision1,
    status: successfulContactStatus(contact, revision1, undefined, "2026-08-24T04:00:00.000Z", "tab-b-start"),
  }, store);
  const tabA = serializedContactProfileCommit(contact, {
    snapshot: revision2,
    status: successfulContactStatus(contact, revision2, undefined, "2026-08-24T04:02:00.000Z", "tab-a-finish"),
  }, store);
  const tabB = serializedContactProfileCommit(contact, {
    snapshot: revision1,
    status: successfulContactStatus(contact, revision1, undefined, "2026-08-24T04:03:00.000Z", "tab-b-finish"),
  }, store);
  await Promise.all([tabA, tabB]);
  assert.equal(store.current().snapshot.profile.revision, 2);
  assert.equal(store.current().status.errorCode, "rollback");
});

test("serialized first-refresh and same-revision forks retain one incumbent", async () => {
  const revision1 = await contactSnapshot(contact, current);
  const forkCurrent = await currentForSource(source.replace('(display-name "Rocky")', '(display-name "Rooky")'), "2026-08-24T04:01:00.000Z");
  const fork = await contactSnapshot(contact, forkCurrent);
  const incumbent = {
    snapshot: revision1,
    status: successfulContactStatus(contact, revision1, undefined, "2026-08-24T04:00:00.000Z", "attempt-a"),
  };
  const candidate = {
    snapshot: fork,
    status: successfulContactStatus(contact, fork, undefined, "2026-08-24T04:01:00.000Z", "attempt-b"),
  };
  const store = serializedMemoryStore();
  await serializedContactProfileCommit(contact, incumbent, store);
  const merged = await serializedContactProfileCommit(contact, candidate, store);
  assert.equal(merged.snapshot.source.rawSha256, revision1.source.rawSha256);
  assert.equal(merged.status.errorCode, "fork");
  assert.equal(merged.status.stale, true);
});

test("equal-time merges are deterministic while higher revision still promotes", async () => {
  const revision1 = await contactSnapshot(contact, current);
  const revision2 = await contactSnapshot(contact, await currentForSource(
    source.replace("(revision 1)", "(revision 2)"), "2026-08-24T04:00:00.000Z",
  ));
  const time = "2026-08-24T04:05:00.000Z";
  const incumbent = { snapshot: revision1, status: successfulContactStatus(contact, revision1, undefined, time, "attempt-z") };
  const promoted = await mergeContactProfileState(contact, incumbent, {
    snapshot: revision2, status: successfulContactStatus(contact, revision2, undefined, time, "attempt-a"),
  });
  assert.equal(promoted.snapshot.profile.revision, 2);
  const confirmed = await mergeContactProfileState(contact, incumbent, {
    snapshot: revision1, status: successfulContactStatus(contact, revision1, undefined, time, "attempt-a"),
  });
  assert.equal(confirmed.status.attemptId, "attempt-z");
});

test("later failed refresh changes status without replacing the incumbent snapshot", async () => {
  const snapshot = await contactSnapshot(contact, current);
  const incumbent = {
    snapshot,
    status: successfulContactStatus(contact, snapshot, undefined, "2026-08-24T04:00:00.000Z", "attempt-a"),
  };
  const failure = {
    status: failedContactStatus(contact, undefined, incumbent.status, new TypeError("offline"), "2026-08-24T04:01:00.000Z", "attempt-b"),
  };
  const store = serializedMemoryStore();
  await serializedContactProfileCommit(contact, incumbent, store);
  const merged = await serializedContactProfileCommit(contact, failure, store);
  assert.equal(merged.snapshot.source.rawSha256, snapshot.source.rawSha256);
  assert.equal(merged.status.lastAttempt, "unavailable");
  assert.equal(merged.status.stale, true);
  assert.equal(merged.status.snapshotRawSha256, snapshot.source.rawSha256);
  assert.equal(merged.status.lastSuccessAt, incumbent.status.lastSuccessAt);
});

test("refresh failure preserves the last whole snapshot and marks only status stale", async () => {
  const snapshot = await contactSnapshot(contact, current);
  const accepted = successfulContactStatus(contact, snapshot, undefined, "2026-08-24T04:01:00.000Z");
  const failed = failedContactStatus(contact, snapshot, accepted, new TypeError("offline"), "2026-08-24T04:02:00.000Z");
  const restored = await restoreContactSnapshot(contact, snapshot, failed);
  assert.equal(restored.profile.displayName, "Rocky");
  assert.equal(restored.stale, true);
  assert.equal(failed.lastSuccessAt, accepted.lastSuccessAt);
  assert.equal(failed.errorCode, "TypeError");
});

test("malformed or missing profile knowledge never needs to alter messaging contact validity", async () => {
  await assert.rejects(() => restoreContactSnapshot(contact, {}, {}), /snapshot is invalid/);
  assert.equal(statusMatchesContact(contact, undefined), false);
  assert.equal(contact.identity, "rocky");
  assert.deepEqual(contact.route, ["rocky"]);
});
