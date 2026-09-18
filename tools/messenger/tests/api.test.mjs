import assert from "node:assert/strict";
import test from "node:test";
import { MessengerApi } from "../public/api.mjs";
import { textToByteVector } from "../public/logic.mjs";

const config = { gatewayBase: "/api/v1", authBase: "/auth", maxMessageBytes: 65536 };
const createApi = (fetchImpl) => {
  const api = new MessengerApi(config, fetchImpl);
  api.setLocalIdentity("thiennam", "galactica");
  return api;
};
const contact = { contactId: "rocky", id: "rocky", handle: "Rocky", identity: "rocky", journal: "rocky", owner: "rocky", route: ["rocky"], incomingPrincipal: ["rocky", "*state*", "rocky"] };
const id = "123e4567-e89b-42d3-a456-426614174000";

const response = (value, status = 200) => new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

test("the default browser fetch remains callable through the API instance", async () => {
  const api = createApi();
  const result = await api.fetchImpl("data:application/json,true");
  assert.equal(await result.json(), true);
});

test("whoami uses only the proxied Kratos session", async () => {
  const calls = [];
  const api = createApi(async (url, options) => {
    calls.push({ url, options });
    return response({ identity: { traits: { username: "thiennam" } } });
  });
  assert.equal((await api.whoami()).identity.traits.username, "thiennam");
  assert.equal(calls[0].url, "/auth/.ory/sessions/whoami");
  assert.equal(calls[0].options.method, "GET");
  assert.equal("Authorization" in calls[0].options.headers, false);
});

test("local journal name and identity id come from distinct Gateway info fields", async () => {
  const api = createApi(async (url, options) => {
    assert.equal(url, "/api/v1/general/info");
    assert.equal(options.method, "GET");
    return response({
      name: { "*type/string*": "galactica" },
      identity: { id: { "*type/byte-vector*": "00".repeat(32) } },
      "public-key": { "*type/byte-vector*": "01020304" },
    });
  });
  assert.deepEqual(await api.getLocalJournalInfo(), { name: "galactica", identityIdBase64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=" });
  assert.equal(await api.getLocalJournalName(), "galactica");
});

test("missing or malformed local profile identity leaves messaging discovery operational", async () => {
  for (const identity of [undefined, { id: { "*type/byte-vector*": "abcd" } }]) {
    const api = createApi(async () => response({ name: { "*type/string*": "galactica" }, identity }));
    assert.deepEqual(await api.getLocalJournalInfo(), { name: "galactica" });
  }
});

test("send emits the current raw-byte federated mailbox request", async () => {
  const calls = [];
  const api = createApi(async (url, options) => { calls.push({ url, options }); return response(true); });
  const result = await api.send(contact, "hello", { id });
  const body = JSON.parse(calls[0].options.body);
  assert.equal(calls[0].url, "/api/v1/general/put");
  assert.equal(calls[0].options.headers["X-Sync-Messenger-Request"], "v1");
  assert.deepEqual(body.path, ["*state*", "rocky", "mailbox", "inbox", "galactica", "thiennam", id]);
  assert.deepEqual(body.$federation, { route: ["rocky"] });
  assert.equal(body["expression?"], false);
  const raw = new TextDecoder().decode(Uint8Array.from(body.value["*type/byte-vector*"].match(/../g).map((part) => Number.parseInt(part, 16))));
  assert.equal(raw, `{"version":1,"id":"${id}","from":"thiennam@galactica","to":"rocky@rocky","createdAt":"${result.envelope.createdAt}","body":"hello"}`);
  assert.equal(result.envelope.to, "rocky@rocky");
});

test("group fanout preparation shares one message and conversation id", () => {
  const api = createApi(async () => response(true));
  const conversationId = "223e4567-e89b-42d3-a456-426614174000";
  const prepared = api.prepare(contact, "group hello", {
    id,
    conversationId,
    participants: ["thiennam@galactica", "rocky@rocky", "grace@grace"],
    createdAt: "2026-08-16T12:00:00.000Z",
  });
  assert.equal(prepared.envelope.version, 2);
  assert.equal(prepared.envelope.id, id);
  assert.equal(prepared.envelope.conversationId, conversationId);
  assert.deepEqual(prepared.envelope.participants, ["grace@grace", "rocky@rocky", "thiennam@galactica"]);
});

test("poll list and read validate exact mailbox source", async () => {
  const envelope = { version: 1, id, from: "rocky@rocky", to: "thiennam@galactica", createdAt: "2026-08-16T12:00:00.000Z", body: "reply" };
  const fetchImpl = async (_url, options) => {
    const body = JSON.parse(options.body);
    return body.path.at(-1) === id
      ? response(textToByteVector(JSON.stringify(envelope)))
      : response(["directory", { [id]: "value" }, true]);
  };
  const api = createApi(fetchImpl);
  assert.deepEqual(await api.listIncoming(contact), [{ name: id, kind: "value" }]);
  assert.equal((await api.readIncoming(contact, id)).envelope.body, "reply");
});

test("sent-copy recovery reads the sender-keyed remote mailbox without writing", async () => {
  const calls = [];
  const envelope = { version: 1, id, from: "thiennam@galactica", to: "rocky@rocky", createdAt: "2026-08-16T12:00:00.000Z", body: "sent root" };
  const api = createApi(async (url, options) => {
    const body = JSON.parse(options.body);
    calls.push({ url, body });
    return body.path.at(-1) === id
      ? response(textToByteVector(JSON.stringify(envelope)))
      : response(["directory", { [id]: "value" }, true]);
  });
  assert.deepEqual(await api.listOutgoingCopies(contact), [{ name: id, kind: "value" }]);
  assert.equal((await api.readOutgoingCopy(contact, id)).envelope.body, "sent root");
  assert.deepEqual(calls[0].body.path, ["*state*", "rocky", "mailbox", "inbox", "galactica", "thiennam"]);
  assert.deepEqual(calls[1].body.path, ["*state*", "rocky", "mailbox", "inbox", "galactica", "thiennam", id]);
  assert.deepEqual(calls.map((call) => call.body.$federation), [{ route: ["rocky"] }, { route: ["rocky"] }]);
  assert.equal(calls.every((call) => call.body["read-only?"] === true && call.body["expression?"] === false), true);
  assert.equal(calls.every((call) => call.url.endsWith("/general/use")), true);
});

test("profile exact CAS distinguishes accepted current readback from conflict", async () => {
  const identityIdBase64 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  const currentText = `((schema sync-agent-profile-v0.experimental)\n (identity-id #u(${"0 ".repeat(31)}0))\n (journal galactica)\n (owner thiennam)\n (display-name "Thien-Nam"))\n`;
  const currentValue = textToByteVector(currentText);
  const calls = [];
  let requestCount = 0;
  const api = createApi(async (url, options) => {
    const body = JSON.parse(options.body); calls.push({ url, body }); requestCount += 1;
    if (requestCount === 1) return response(currentValue);
    if (requestCount === 2) return response(true);
    return response(body.path.at(-1) === "profile.scm" ? calls[1].body.value : currentValue);
  });
  const current = await api.readLocalProfile({ owner: "thiennam", journal: "galactica", identityIdBase64 });
  const result = await api.saveLocalProfile(current, { displayName: "Thien-Nam", pronouns: ["he/him"], bio: "Sync Web owner." });
  assert.equal(result.status, "saved-current");
  assert.deepEqual(calls[1].body.expected, currentValue);
  assert.equal(calls[1].body["expression?"], false);
  assert.equal(calls.length, 3);

  let conflictCalls = 0;
  const conflictApi = createApi(async () => response(++conflictCalls === 1 ? currentValue : false));
  const conflictBase = await conflictApi.readLocalProfile({ owner: "thiennam", journal: "galactica", identityIdBase64 });
  const conflict = await conflictApi.saveLocalProfile(conflictBase, { displayName: "Thien-Nam", pronouns: [], bio: "Conflict fixture." });
  assert.deepEqual(conflict, { status: "conflict" });
  assert.equal(conflictCalls, 2);
  await assert.rejects(() => conflictApi.saveLocalProfile(conflictBase, { displayName: "Thien-Nam", pronouns: [], bio: "Again." }), /exact fresh local current-get object/);
  assert.equal(conflictCalls, 2);

  let ambiguousCalls = 0;
  let ambiguousPhase = "refresh";
  const ambiguousApi = createApi(async () => {
    ambiguousCalls += 1;
    if (ambiguousPhase === "refresh") return response(currentValue);
    if (ambiguousPhase === "set") return response(["unexpected-semantic-result"]);
    return response(false);
  });
  const fields = { displayName: "Thien-Nam", pronouns: [], bio: "Ambiguous fixture." };
  const ambiguousBase = await ambiguousApi.readLocalProfile({ owner: "thiennam", journal: "galactica", identityIdBase64 });
  let foreignCalls = 0;
  const foreignApi = createApi(async () => { foreignCalls += 1; return response(true); });
  await assert.rejects(() => foreignApi.saveLocalProfile(ambiguousBase, fields), /exact fresh local current-get object/);
  assert.equal(foreignCalls, 0);
  await assert.rejects(() => ambiguousApi.saveLocalProfile(ambiguousBase, { ...fields, bio: " " }), /non-whitespace/);
  assert.equal(ambiguousApi.isProfileEditBaseConsumed(ambiguousBase), false);
  assert.equal(ambiguousCalls, 1);
  for (const forged of [structuredClone(ambiguousBase), { ...ambiguousBase }, JSON.parse(JSON.stringify(ambiguousBase))]) {
    await assert.rejects(() => ambiguousApi.saveLocalProfile(forged, fields), /exact fresh local current-get object/);
  }
  assert.equal(ambiguousCalls, 1);

  ambiguousPhase = "set";
  await assert.rejects(() => ambiguousApi.saveLocalProfile(ambiguousBase, fields), /fresh current readback is required/);
  assert.equal(ambiguousApi.isProfileEditBaseConsumed(ambiguousBase), true);
  assert.equal(ambiguousCalls, 2);
  await assert.rejects(() => ambiguousApi.saveLocalProfile(ambiguousBase, fields), /exact fresh local current-get object/);
  assert.equal(ambiguousCalls, 2);

  ambiguousPhase = "refresh";
  const refreshed = await ambiguousApi.readLocalProfile({ owner: "thiennam", journal: "galactica", identityIdBase64 });
  ambiguousPhase = "conflict";
  assert.deepEqual(await ambiguousApi.saveLocalProfile(refreshed, fields), { status: "conflict" });
  assert.equal(ambiguousCalls, 4);

  let thrownCalls = 0;
  const thrownApi = createApi(async () => {
    thrownCalls += 1;
    if (thrownCalls === 1) return response(currentValue);
    throw new Error("connection lost after dispatch");
  });
  const thrownBase = await thrownApi.readLocalProfile({ owner: "thiennam", journal: "galactica", identityIdBase64 });
  await assert.rejects(() => thrownApi.saveLocalProfile(thrownBase, fields), /fresh current readback is required/);
  await assert.rejects(() => thrownApi.saveLocalProfile(thrownBase, fields), /exact fresh local current-get object/);
  assert.equal(thrownCalls, 2);

  let mismatchCalls = 0;
  const mismatchApi = createApi(async () => {
    mismatchCalls += 1;
    if (mismatchCalls === 2) return response(true);
    return response(currentValue);
  });
  const mismatchBase = await mismatchApi.readLocalProfile({ owner: "thiennam", journal: "galactica", identityIdBase64 });
  await assert.rejects(() => mismatchApi.saveLocalProfile(mismatchBase, fields), /fresh current readback is required/);
  await assert.rejects(() => mismatchApi.saveLocalProfile(mismatchBase, fields), /exact fresh local current-get object/);
  assert.equal(mismatchCalls, 3);
});

test("missing local profile is an exact create base for revision one", async () => {
  const identityIdBase64 = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  const nothing = ["nothing"];
  const calls = [];
  const api = createApi(async (_url, options) => {
    const body = JSON.parse(options.body); calls.push(body);
    if (calls.length === 1) return response(nothing);
    if (calls.length === 2) return response(true);
    return response(calls[1].value);
  });
  const empty = await api.readLocalProfile({ owner: "thiennam", journal: "galactica", identityIdBase64 });
  assert.equal(empty.profile, null);
  const saved = await api.saveLocalProfile(empty, {
    displayName: "Thien-Nam", pronouns: ["he/him"], bio: "Sync Web owner.",
  });
  assert.equal(saved.status, "saved-current");
  assert.equal(saved.current.profile.revision, 1);
  assert.deepEqual(calls[1].expected, nothing);
  assert.equal(calls[1]["expression?"], false);
  assert.equal(calls.length, 3);
});

test("contact profile reads use route-canonical owner and path without pasted identity", async () => {
  const profile = textToByteVector(`((schema sync-agent-profile-v1.experimental)\n (identity-id #u(${"0 ".repeat(31)}0))\n (journal rocky)\n (owner rocky)\n (revision 1)\n (display-name "Rocky")\n (bio "Lead implementation agent."))\n`);
  const requests = [];
  const api = createApi(async (_url, options) => { requests.push(JSON.parse(options.body)); return response(profile); });
  const result = await api.readProfile({ owner: "rocky", journal: "rocky", route: ["rocky"] });
  assert.equal(result.profile.displayName, "Rocky");
  assert.equal("identityIdBase64" in result, false);
  assert.deepEqual(requests[0], { path: ["*state*", "rocky", "profile.scm"], $federation: { route: ["rocky"] }, "read-only?": true, "expression?": false });
  await assert.rejects(
    () => api.saveLocalProfile(result, { displayName: "Rocky", pronouns: [], bio: "Forged local edit." }),
    /exact fresh local current-get object/,
  );
  assert.equal(requests.length, 1);
});

test("contact registry bootstraps and updates with exact expected bytes", async () => {
  const calls = [];
  const seed = { version: 1, contacts: [contact] };
  const api = createApi(async (url, options) => {
    const body = JSON.parse(options.body);
    calls.push({ url, body });
    if (calls.length === 1) return response(["nothing"]);
    return response(true);
  });
  assert.deepEqual(await api.loadContactRegistry(seed), seed);
  await api.saveContactRegistry({ version: 1, contacts: [] });
  assert.deepEqual(calls[0].body.path, ["*state*", "thiennam", "messenger", "contacts.json"]);
  assert.deepEqual(calls[1].body.expected, ["nothing"]);
  assert.deepEqual(calls[2].body.expected, calls[1].body.value);
});

test("Journal null encoding of an empty Scheme authorization list is accepted narrowly", async () => {
  const emptyApi = createApi(async () => response(null));
  assert.deepEqual(await emptyApi.getAuthorizations(), []);
  const malformedApi = createApi(async () => response({}));
  await assert.rejects(() => malformedApi.getAuthorizations(), /invalid authorization table/);
});

test("contacts reconcile to the exact mailbox grant set", async () => {
  const exact = {
    principal: ["rocky", "*state*", "rocky"], "key-index": [-32, -1],
    path: ["mailbox", "inbox", "rocky", "rocky"], "put!": true, "use!": { "read-only?": true },
  };
  const broad = { ...exact, "key-index": [0, -1] };
  const stale = {
    principal: ["grace", "*state*", "grace"], "key-index": [-32, -1],
    path: ["mailbox", "inbox", "grace", "grace"], "put!": true, "use!": { "read-only?": true },
  };
  let reads = 0;
  const calls = [];
  const api = createApi(async (url, options) => {
    const body = JSON.parse(options.body);
    calls.push({ url, body });
    if (url.endsWith("/authorizations")) return response(reads++ === 0 ? [broad, stale] : [exact]);
    return response(true);
  });
  assert.deepEqual(await api.reconcileContactAuthorizations([contact]), { rocky: "grant-ready" });
  assert.deepEqual(calls.map((call) => call.url), [
    "/api/v1/general/authorizations", "/api/v1/general/authorize",
    "/api/v1/general/deauthorize", "/api/v1/general/deauthorize",
    "/api/v1/general/authorizations",
  ]);
  assert.deepEqual(calls[1].body.rule, exact);
  assert.deepEqual(calls[2].body.rule, broad);
  assert.deepEqual(calls[3].body.rule, stale);
});

test("gateway denial surfaces without automatic retry", async () => {
  let calls = 0;
  const api = createApi(async () => { calls += 1; return response({ error: "authorization-error", message: "denied" }, 400); });
  await assert.rejects(() => api.send(contact, "hello", { id }), /denied/);
  assert.equal(calls, 1);
});
