import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { test } from "node:test";
import Fastify from "fastify";
import fastifySwagger from "@fastify/swagger";
import {
  buildSchemeArgumentsProjection,
  gatewayRoutes,
  isJsonReadOnlyUse,
  isProjectedSchemeReadOnlyUse,
  publishesGeneralChange,
} from "../src/routes";
import { JournalSemanticError } from "../src/journal";
import { GatewayEventBroker } from "../src/events";
import { RAW_LIMITS, RAW_RESPONSE_HEADERS } from "../src/raw";
import type { JournalCall, JournalClient } from "../src/journal";
import type { ApiTokenEntry, KratosAdminIdentity, KratosClient } from "../src/kratos";

const JOURNAL_SECRET = "test-journal-secret";
const IDENTITY_ID = "test-user-id";
const SESSION_COOKIE = "ory_kratos_session=test-session-token";
const rawToken = (payload: unknown): string => Buffer.from(JSON.stringify(payload)).toString("base64url");

const rawTokenWithJsonBytes = (target: number): string => {
  for (let segments = 1; segments <= 8; segments += 1) {
    const path = [["y", "*state*"], ...Array.from({ length: segments }, () => ["y", ""])] as string[][];
    const payload = ["v1", "stage", [], path];
    let remaining = target - Buffer.byteLength(JSON.stringify(payload));
    if (remaining < 0 || remaining > segments * RAW_LIMITS.segmentStringBytes) continue;
    for (let index = 1; index < path.length && remaining > 0; index += 1) {
      const length = Math.min(remaining, RAW_LIMITS.segmentStringBytes);
      path[index][1] = "a".repeat(length);
      remaining -= length;
    }
    const encoded = rawToken(payload);
    if (Buffer.from(encoded, "base64url").byteLength === target) return encoded;
  }
  throw new Error(`cannot construct ${target}-byte Raw payload`);
};

interface MockJournal {
  client: JournalClient;
  jsonCalls: JournalCall[];
  schemeCalls: Array<{ expression: string; functionName: string }>;
  proxiedJsonBodies: unknown[];
  proxiedSchemeExpressions: string[];
}

const createMockJournal = (): MockJournal => {
  const jsonCalls: JournalCall[] = [];
  const schemeCalls: Array<{ expression: string; functionName: string }> = [];
  const proxiedJsonBodies: unknown[] = [];
  const proxiedSchemeExpressions: string[] = [];

  return {
    jsonCalls,
    schemeCalls,
    proxiedJsonBodies,
    proxiedSchemeExpressions,
    client: {
      async callJson(input: JournalCall): Promise<unknown> {
        jsonCalls.push(input);
        return { ok: true, mode: "json", function: input.functionName };
      },
      async callScheme(input: {
        expression: string;
        functionName: string;
      }): Promise<unknown> {
        schemeCalls.push(input);
        return { ok: true, mode: "scheme", function: input.functionName };
      },
      async callRootJson(input: JournalCall): Promise<unknown> {
        jsonCalls.push(input);
        return { ok: true, mode: "json", function: input.functionName };
      },
      async callRootScheme(input: {
        expression: string;
        functionName: string;
      }): Promise<unknown> {
        schemeCalls.push(input);
        return { ok: true, mode: "scheme", function: input.functionName };
      },
      async proxyJson(body: unknown): Promise<unknown> {
        proxiedJsonBodies.push(body);
        return { ok: true, mode: "proxy-json" };
      },
      async proxyScheme(expression: string): Promise<string> {
        proxiedSchemeExpressions.push(expression);
        return "((public-key #u(1 2 3)))";
      },
      async schemeToJson(expression: string): Promise<unknown> {
        proxiedSchemeExpressions.push(expression);
        return { "*type/quoted*": ["gateway-use-arguments"] };
      },
    },
  };
};

const KRATOS_UUID = "a3f8c201-b4d2-e9f0-a1b2-c3d4e5f6a7b8";

const createMockKratos = (
  identityId = IDENTITY_ID,
  apiTokens: Record<string, ApiTokenEntry> = {}
): KratosClient => {
  let storedTokens = { ...apiTokens };
  return {
    async whoami(_opts) {
      return { identity: { id: KRATOS_UUID, traits: { username: identityId } } };
    },
    async whoamiWithSessionToken(_token) {
      return { identity: { id: KRATOS_UUID, traits: { username: identityId } } };
    },
    async getIdentityById(_uuid): Promise<KratosAdminIdentity> {
      return {
        id: KRATOS_UUID,
        traits: { username: identityId },
        metadata_admin: { api_tokens: storedTokens },
      };
    },
    async patchIdentityApiTokens(_uuid, newTokens) {
      storedTokens = { ...newTokens };
    },
  };
};

const createApp = async (input: {
  allowAdminRoutes: boolean;
  journal?: JournalClient;
  kratos?: KratosClient;
  journalSecret?: string;
  loggerStream?: { write: (message: string) => void };
}) => {
  const app = Fastify({
    logger: input.loggerStream ? { stream: input.loggerStream } : false,
    ajv: { customOptions: { keywords: ["example"], allowUnionTypes: true } },
  });
  app.addContentTypeParser("text/plain", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/scheme", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/octet-stream", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/x-www-form-urlencoded", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addHook("preParsing", async (request, _reply, payload) => {
    if (!request.headers["content-type"]) {
      request.headers["content-type"] = "text/plain";
    }
    return payload;
  });
  await app.register(gatewayRoutes, {
    journal: input.journal || createMockJournal().client,
    allowAdminRoutes: input.allowAdminRoutes,
    journalSecret: input.journalSecret ?? JOURNAL_SECRET,
    kratos: input.kratos ?? createMockKratos(),
  });
  await app.ready();
  return app;
};

test("Gateway-rendered logos link to the public Gateway home", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(() => app.close());

  const response = await app.inject({ method: "GET", url: "/" });
  assert.equal(response.statusCode, 200);
  assert.match(
    response.body,
    /<a class="toolbar-logo-link" href="\/gateway"><img class="toolbar-logo"[^>]+alt="Synchronic Web" \/><\/a>/
  );

  const docsSource = readFileSync(resolve(__dirname, "../src/server.ts"), "utf8");
  assert.ok(docsSource.includes(
    `'<a class="sync-doc-logo-link" href="/gateway"><img class="sync-doc-logo" src="/gateway-logo.png" alt="Synchronic Web" /></a>'`
  ));
});

test("GET /api/v1/general/size forwards to size without auth", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({ method: "GET", url: "/api/v1/general/size" });
  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], { functionName: "size" });
});

test("restricted route returns 401 without session cookie", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { "content-type": "application/json" },
    payload: [],
  });
  assert.equal(res.statusCode, 401);
  const body = res.json();
  assert.equal(body.error, "unauthorized");
});

test("Raw Stage GET reauthenticates, forwards the typed route/path, and sends exact inert bytes", async (t) => {
  const mock = createMockJournal();
  mock.client.callJson = async (input) => {
    mock.jsonCalls.push(input);
    return { "*type/byte-vector*": "003c7363726970743e0d0aff" };
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const selection = rawToken([
    "v1", "stage", ["peer-a"],
    [["y", "*state*"], ["i", 1], ["y", "1"], ["s", "1"]],
  ]);
  const res = await app.inject({
    method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(res.rawPayload, Buffer.from([0, 60, 115, 99, 114, 105, 112, 116, 62, 13, 10, 255]));
  assert.equal(res.headers["content-type"], "application/octet-stream");
  assert.match(String(res.headers["content-disposition"]), /^attachment/);
  assert.equal(res.headers["x-content-type-options"], "nosniff");
  assert.equal(res.headers["referrer-policy"], "no-referrer");
  assert.match(String(res.headers["content-security-policy"]), /sandbox/);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "use!",
    args: {
      path: ["*state*", 1, "1", { "*type/string*": "1" }],
      "read-only?": true,
      "expression?": false,
    },
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
    routeTarget: ["peer-a"],
  });
});

test("Raw Ledger GET accepts only concrete selectors and uses ordinary retrieve authorization", async (t) => {
  const mock = createMockJournal();
  mock.client.callJson = async (input) => {
    mock.jsonCalls.push(input);
    return { content: { "*type/byte-vector*": Buffer.from("<svg onload='x'/>").toString("hex") } };
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const selection = rawToken([
    "v1", "ledger",
    [["i", 12], ["y", "peer-a"], ["i", 8], ["y", "*state*"], ["y", "doc"]],
  ]);
  const res = await app.inject({
    method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
  });

  assert.equal(res.statusCode, 200, res.body);
  assert.equal(res.headers["content-type"], "text/plain; charset=utf-8");
  assert.equal(res.body, "<svg onload='x'/>");
  assert.deepEqual(mock.jsonCalls[0]?.args, {
    path: [12, "peer-a", 8, "*state*", "doc"],
    "expression?": false,
    "pinned?": false,
    "proof?": false,
    "index?": false,
  });
});

test("Raw Stage reload tracks current bytes while one concrete Ledger URL stays fixed", async (t) => {
  const mock = createMockJournal();
  let stageHex = "41";
  mock.client.callJson = async (input) => {
    mock.jsonCalls.push(input);
    if (input.functionName === "use!") return { "*type/byte-vector*": stageHex };
    assert.deepEqual((input.args as { path: unknown }).path, [4, "*state*", "doc"]);
    return { content: { "*type/byte-vector*": "41" } };
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());
  const stage = rawToken(["v1", "stage", [], [["y", "*state*"], ["y", "doc"]]]);
  const ledger = rawToken(["v1", "ledger", [["i", 4], ["y", "*state*"], ["y", "doc"]]]);
  const request = (selection: string) => app.inject({
    method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
  });

  assert.equal((await request(stage)).body, "A");
  stageHex = "42";
  assert.equal((await request(stage)).body, "B");
  assert.equal((await request(ledger)).body, "A");
  stageHex = "43";
  assert.equal((await request(ledger)).body, "A");
});

test("Raw GET preserves sanitized authentication, missing, unavailable, and Journal status classes", async (t) => {
  const selection = rawToken(["v1", "stage", [], [["y", "*state*"], ["y", "doc"]]]);
  const unauthenticated = await createApp({ allowAdminRoutes: false });
  t.after(async () => unauthenticated.close());
  const denied = await unauthenticated.inject({ method: "GET", url: `/api/v1/raw?selection=${selection}` });
  assert.equal(denied.statusCode, 401);
  assert.equal(denied.json().error, "unauthorized");

  for (const [result, status, code] of [
    [["nothing"], 404, "not_found"],
    [["unknown"], 503, "unavailable"],
    [{ content: "not bytes" }, 415, "raw_value_required"],
  ] as const) {
    const mock = createMockJournal();
    mock.client.callJson = async () => result;
    const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
    const res = await app.inject({
      method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
    });
    assert.equal(res.statusCode, status);
    assert.deepEqual(res.json(), { error: code });
    await app.close();
  }

  const mock = createMockJournal();
  mock.client.callJson = async () => {
    throw new JournalSemanticError({
      code: "authentication-error", message: "secret path details", statusCode: 403,
    });
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());
  const res = await app.inject({
    method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
  });
  assert.equal(res.statusCode, 403);
  assert.deepEqual(res.json(), { error: "authorization_error" });
  assert.doesNotMatch(res.body, /secret|path/i);

  for (const [journalCode, publicCode] of [
    ["bridge-index-error", "unavailable"],
    ["availability-error", "unavailable"],
    ["authentication-error-detail", "journal_error"],
    ["future-proof-error", "journal_error"],
    ["bridgework-error", "journal_error"],
  ] as const) {
    const classified = createMockJournal();
    classified.client.callJson = async () => {
      throw new JournalSemanticError({ code: journalCode, message: "omitted" });
    };
    const classifiedApp = await createApp({ allowAdminRoutes: false, journal: classified.client });
    const classifiedResponse = await classifiedApp.inject({
      method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
    });
    assert.deepEqual(classifiedResponse.json(), { error: publicCode }, journalCode);
    await classifiedApp.close();
  }
});

test("Raw GET rejects direct and resolved byte-vector wrappers with extra members", async () => {
  const selection = rawToken(["v1", "stage", [], [["y", "*state*"], ["y", "doc"]]]);
  for (const result of [
    { "*type/byte-vector*": "ff", control: true },
    { content: { "*type/byte-vector*": "ff", control: true }, indexes: [4] },
  ]) {
    const mock = createMockJournal();
    mock.client.callJson = async () => result;
    const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
    const response = await app.inject({
      method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
    });
    assert.equal(response.statusCode, 415);
    assert.deepEqual(response.json(), { error: "raw_value_required" });
    await app.close();
  }
});

test("Raw query applies max and max+1 inside the fixed non-echoing boundary", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());
  const max = rawTokenWithJsonBytes(RAW_LIMITS.jsonBytes);
  assert.equal(max.length, RAW_LIMITS.tokenBytes);

  const accepted = await app.inject({
    method: "GET", url: `/api/v1/raw?selection=${max}`, headers: { cookie: SESSION_COOKIE },
  });
  assert.equal(accepted.statusCode, 415);
  assert.deepEqual(accepted.json(), { error: "raw_value_required" });

  const over = "SENSITIVE_RAW_TOKEN_" + "A".repeat(RAW_LIMITS.tokenBytes + 1);
  const rejected = await app.inject({
    method: "GET", url: `/api/v1/raw?selection=${over}`, headers: { cookie: SESSION_COOKIE },
  });
  assert.equal(rejected.statusCode, 400);
  assert.deepEqual(rejected.json(), { error: "invalid_raw_selection" });
  assert.doesNotMatch(rejected.body, /SENSITIVE_RAW_TOKEN/);
  for (const [name, value] of Object.entries(RAW_RESPONSE_HEADERS)) {
    assert.equal(rejected.headers[name], value);
  }

  const oldPath = await app.inject({
    method: "GET",
    url: `/api/v1/raw/${"SENSITIVE_OLD_RAW_PATH".repeat(32)}`,
    headers: { cookie: SESSION_COOKIE },
  });
  assert.equal(oldPath.statusCode, 400);
  assert.deepEqual(oldPath.json(), { error: "invalid_raw_selection" });
  assert.doesNotMatch(oldPath.body, /SENSITIVE_OLD_RAW_PATH/);
  assert.equal(oldPath.headers["cache-control"], RAW_RESPONSE_HEADERS["cache-control"]);

  const ordinary = await app.inject({ method: "GET", url: "/ordinary-missing" });
  assert.equal(ordinary.statusCode, 404);
  assert.match(ordinary.body, /ordinary-missing/);
});

test("Raw Stage and Ledger transport failures are fixed and absent from route logs", async (t) => {
  const sensitive = "upstream failure for secret selected path";
  let logs = "";
  const mock = createMockJournal();
  mock.client.callJson = async () => { throw new Error(sensitive); };
  const app = await createApp({
    allowAdminRoutes: false,
    journal: mock.client,
    loggerStream: { write: (message) => { logs += message; } },
  });
  t.after(async () => app.close());

  const selections = [
    rawToken(["v1", "stage", [], [["y", "*state*"], ["y", "doc"]]]),
    rawToken(["v1", "ledger", [["i", 4], ["y", "*state*"], ["y", "doc"]]]),
  ];
  for (const selection of selections) {
    const response = await app.inject({
      method: "GET", url: `/api/v1/raw?selection=${selection}`, headers: { cookie: SESSION_COOKIE },
    });
    assert.equal(response.statusCode, 502);
    assert.deepEqual(response.json(), { error: "gateway_error" });
    assert.equal(response.headers["cache-control"], RAW_RESPONSE_HEADERS["cache-control"]);
    assert.doesNotMatch(response.body, /secret|selected|path/i);
  }
  assert.doesNotMatch(logs, /upstream failure|secret selected path/i);
});

test("only potentially mutating successful operations publish general change hints", () => {
  assert.equal(isJsonReadOnlyUse({ "read-only?": true }), true);
  assert.equal(isJsonReadOnlyUse({ "read-only?": false }), false);
  assert.equal(isJsonReadOnlyUse([["path", ["*state*", "docs"]], ["read-only?", true]]), true);
  assert.equal(isJsonReadOnlyUse([["read-only?", true], ["read-only?", true]]), false);
  assert.equal(isJsonReadOnlyUse([["read-only?", true], ["read-only?", false]]), false);
  assert.equal(isJsonReadOnlyUse([["read-only?", true], "junk"]), false);
  assert.equal(isJsonReadOnlyUse([["read-only?", true], ["path"]]), false);
  assert.equal(isJsonReadOnlyUse([["read-only?", true], ["path", [], "junk"]]), false);
  assert.equal(isJsonReadOnlyUse([["read-only?", true], [42, []]]), false);
  assert.equal(isJsonReadOnlyUse([["read-only?", true], ["path", []], ["path", []]]), false);
  assert.equal(isJsonReadOnlyUse([["arguments", [["read-only?", true]]]]), false);
  assert.equal(isProjectedSchemeReadOnlyUse({
    "*type/quoted*": [
      "gateway-use-arguments",
      ["path", ["*state*", "docs"]],
      ["read-only?", true],
    ],
  }), true);
  assert.equal(isProjectedSchemeReadOnlyUse({
    "*type/quoted*": [
      "gateway-use-arguments",
      ["read-only?", false],
      ["arguments", [["read-only?", true]]],
    ],
  }), false);
  for (const entries of [
    [["read-only?", true], ["read-only?", true]],
    [["read-only?", true], ["read-only?", false]],
    [["read-only?", true], "junk"],
    [["read-only?", true], ["path"]],
    [["read-only?", true], ["path", [], "junk"]],
    [["read-only?", true], [42, []]],
    [["read-only?", true], ["path", []], ["path", []]],
  ]) {
    assert.equal(isProjectedSchemeReadOnlyUse({
      "*type/quoted*": ["gateway-use-arguments", ...entries],
    }), false);
  }
  assert.equal(publishesGeneralChange("use", true), false);
  assert.equal(publishesGeneralChange("use-batch", true), false);
  assert.equal(publishesGeneralChange("use", false), true);
  assert.equal(publishesGeneralChange("use-batch"), true);
  assert.equal(publishesGeneralChange("put"), true);
  assert.equal(publishesGeneralChange("put-batch"), true);
});

test("duplicate top-level JSON names conservatively publish use hints", async (t) => {
  const published: Array<{ operation: string; path?: Array<string | number> }> = [];
  const originalPublish = GatewayEventBroker.prototype.publish;
  t.mock.method(
    GatewayEventBroker.prototype,
    "publish",
    function (this: GatewayEventBroker, input: { operation: string; path?: Array<string | number> }) {
      published.push(input);
      return originalPublish.call(this, input);
    },
  );

  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  for (const payload of [
    '{"path":["*state*","docs"],"read-only?":false,"read-only?":true}',
    '{"path":["*state*","docs"],"read-only?":false,"read-\\u006fnly?":true}',
  ]) {
    const response = await app.inject({
      method: "POST",
      url: "/api/v1/general/use",
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload,
    });
    assert.equal(response.statusCode, 200);
  }

  assert.equal(mock.jsonCalls.length, 2);
  for (const call of mock.jsonCalls) {
    assert.deepEqual(call.args, { path: ["*state*", "docs"], "read-only?": true });
  }
  assert.deepEqual(published, [
    { operation: "use!", path: ["*state*", "docs"] },
    { operation: "use!", path: ["*state*", "docs"] },
  ]);

  const validResponse = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: '{"path":["*state*","docs"],"read-only?":true}',
  });
  assert.equal(validResponse.statusCode, 200);
  assert.equal(published.length, 2);
});

test("event stream requires authentication", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({ method: "GET", url: "/api/v1/events" });
  assert.equal(res.statusCode, 401);
  assert.equal(res.json().error, "unauthorized");
});

test("restricted route returns 401 when Kratos session is invalid", async (t) => {
  const failingKratos: KratosClient = {
    async whoami() { throw new Error("session invalid"); },
    async whoamiWithSessionToken() { throw new Error("session invalid"); },
    async getIdentityById() { throw new Error("not found"); },
    async patchIdentityApiTokens() { throw new Error("not found"); },
  };
  const app = await createApp({ allowAdminRoutes: false, kratos: failingKratos });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: [],
  });
  assert.equal(res.statusCode, 401);
  assert.equal(res.json().error, "unauthorized");
});

test("POST /api/v1/general/synchronize! forwards pushed payload without auth", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { name: "peer-a", index: -1, response: [] };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/synchronize!",
    headers: { "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "synchronize!",
    args,
    authentication: undefined,
    identityId: undefined,
  });
});

test("POST /api/v1/general/use accepts JSON keyword-object payload with Kratos session", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: ["*state*", "docs"] };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "use!",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/use forwards authenticated bridge discovery", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: ["*bridge*"] };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "use!",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/retrieve forwards one canonical committed path", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/retrieve",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: [-1, "carol", 3, "bob", 7, "*state*", "docs"] },
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "retrieve",
    args: { path: [-1, "carol", 3, "bob", 7, "*state*", "docs"] },
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("retrieve index selection is forwarded without a new route", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const scalar = { path: [-1, "peer-a", -1, "*state*", "a"], "index?": true };
  const batch = { paths: [[-1, "*state*", "local"]], "index?": true };
  for (const [operation, payload] of [["retrieve", scalar], ["retrieve-batch", batch]] as const) {
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload,
    });
    assert.equal(response.statusCode, 200);
  }
  assert.deepEqual(mock.jsonCalls.map((call) => ({ functionName: call.functionName, args: call.args })), [
    { functionName: "retrieve", args: scalar },
    { functionName: "retrieve-batch", args: batch },
  ]);
});

test("retrieve absent and false index selection preserve exact JSON response bytes", async (t) => {
  const mock = createMockJournal();
  mock.client.callJson = async (input: JournalCall): Promise<unknown> => {
    mock.jsonCalls.push(input);
    return "baseline";
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const bodies: string[] = [];
  for (const payload of [
    { path: [-1, "*state*", "a"] },
    { path: [-1, "*state*", "a"], "index?": false },
  ]) {
    const response = await app.inject({
      method: "POST",
      url: "/api/v1/general/retrieve",
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload,
    });
    assert.equal(response.statusCode, 200);
    bodies.push(response.body);
  }
  assert.equal(bodies[0], "baseline");
  assert.equal(Buffer.from(bodies[0]).compare(Buffer.from(bodies[1])), 0);
});

test("batch data operations forward canonical paths and method names", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const paths = [
    [-1, "peer-a", -1, "*state*", "a"],
    [-1, "*state*", "local"],
  ];
  const cases = [
    { operation: "use-batch", functionName: "use-batch!" },
    { operation: "retrieve-batch", functionName: "retrieve-batch" },
    { operation: "pin-batch", functionName: "pin-batch!" },
    { operation: "unpin-batch", functionName: "unpin-batch!" },
    { operation: "prune-batch", functionName: "prune-batch!" },
  ];
  for (const entry of cases) {
    const result = await app.inject({
      method: "POST",
      url: `/api/v1/general/${entry.operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: { paths },
    });
    assert.equal(result.statusCode, 200, entry.operation);
  }
  assert.deepEqual(mock.jsonCalls, cases.map((entry) => ({
    functionName: entry.functionName,
    args: { paths },
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  })));

  const tracePaths = [["*state*", "a"], ["*state*", "b"]];
  const traced = await app.inject({
    method: "POST",
    url: "/api/v1/general/trace-batch",
    headers: { "content-type": "application/json" },
    payload: { index: 3, paths: tracePaths },
  });
  assert.equal(traced.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[cases.length], {
    functionName: "trace-batch",
    args: { index: 3, paths: tracePaths },
    authentication: undefined,
    identityId: undefined,
  });

  const stagedPaths = [["*state*", "a"], ["*state*", "b"]];
  const setBatchArgs = {
    paths: stagedPaths,
    values: ["new-a", "new-b"],
    expected: ["old-a", "old-b"],
  };
  const setBatch = await app.inject({
    method: "POST",
    url: "/api/v1/general/put-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: setBatchArgs,
  });
  assert.equal(setBatch.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[cases.length + 1], {
    functionName: "put-batch!",
    args: setBatchArgs,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
  assert.deepEqual(setBatch.json(), { ok: true, mode: "json", function: "put-batch!" });
});

test("POST /api/v1/general/copy forwards atomic copy arguments and federation context", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = {
    source: ["*state*", "alice", "source"],
    path: ["*state*", "alice", "target"],
    expected: false,
    "expression?": true,
    $federation: { route: ["peer"] },
  };
  const response = await app.inject({
    method: "POST",
    url: "/api/v1/general/copy",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(response.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "copy!",
    args: {
      source: args.source,
      path: args.path,
      expected: false,
      "expression?": true,
    },
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
    routeTarget: ["peer"],
  });
  assert.deepEqual(response.json(), { ok: true, mode: "json", function: "copy!" });
});

test("POST /api/v1/general/truncate forwards exact administrative cutoff", async (t) => {
  const mock = createMockJournal();
  mock.client.callJson = async (input: JournalCall): Promise<unknown> => {
    mock.jsonCalls.push(input);
    if ((input.args as { index?: number } | undefined)?.index === 9) {
      throw new JournalSemanticError({ code: "index-error", message: "Index is out of bounds" });
    }
    return true;
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const json = await app.inject({
    method: "POST",
    url: "/api/v1/general/truncate",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { index: -1 },
  });
  assert.equal(json.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "truncate!",
    args: { index: -1 },
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
  assert.equal(json.body, "true");

  const invalid = await app.inject({
    method: "POST",
    url: "/api/v1/general/truncate",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { index: 9 },
  });
  assert.equal(invalid.statusCode, 400);
  assert.deepEqual(invalid.json(), {
    error: "index-error",
    message: "Index is out of bounds",
    source: "journal",
  });

  const scheme = await app.inject({
    method: "POST",
    url: "/api/v1/general/truncate",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: "((index 2))",
  });
  assert.equal(scheme.statusCode, 200);
  assert.equal(mock.schemeCalls[0].functionName, "truncate!");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function truncate!\) \(arguments \(\(index 2\)\)\)/);

  const denied = await app.inject({
    method: "POST",
    url: "/api/v1/general/truncate",
    headers: { "content-type": "application/json" },
    payload: { index: 0 },
  });
  assert.equal(denied.statusCode, 401);
});

test("POST /api/v1/general/prune forwards exact local retention requests", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const scalar = await app.inject({
    method: "POST",
    url: "/api/v1/general/prune",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: [-1, "*state*", "alice", "old"] },
  });
  assert.equal(scalar.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "prune!",
    args: { path: [-1, "*state*", "alice", "old"] },
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });

  const batch = await app.inject({
    method: "POST",
    url: "/api/v1/general/prune-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { paths: [[-1, "*state*", "alice", "old"], [-1, "*state*", "alice", "archive"]] },
  });
  assert.equal(batch.statusCode, 200);
  assert.equal(mock.jsonCalls[1]?.functionName, "prune-batch!");
  assert.deepEqual(mock.jsonCalls[1]?.args, {
    paths: [[-1, "*state*", "alice", "old"], [-1, "*state*", "alice", "archive"]],
  });

  const scheme = await app.inject({
    method: "POST",
    url: "/api/v1/general/prune",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: "((path (-1 *state* alice old)))",
  });
  assert.equal(scheme.statusCode, 200);
  assert.equal(mock.schemeCalls[0].functionName, "prune!");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function prune!\)/);
});

test("application/scheme routes opaque put, use, and retrieve through the narrow federation header", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  for (const [operation, functionName, payload, fields] of [
    ["put", "put!", "((path (*state* alice object)) (value item))",
      [["path", ["*state*", "alice", "object"]], ["value", "item"]]],
    ["use", "use!", "((path (*state* alice object)) (read-only? #t))",
      [["path", ["*state*", "alice", "object"]], ["read-only?", true]]],
    ["retrieve", "retrieve", "((path (-1 *state* alice object)) (pinned? #t))",
      [["path", [-1, "*state*", "alice", "object"]], ["pinned?", true]]],
  ] as const) {
    mock.client.schemeToJson = async () => ({
      "*type/quoted*": ["gateway-use-arguments", ...fields],
    });
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${operation}`,
      headers: {
        cookie: SESSION_COOKIE,
        "content-type": "application/scheme",
        "x-sync-web-federation-route": JSON.stringify(["alpha", "beta"]),
      },
      payload,
    });
    assert.equal(response.statusCode, 200, operation);
    const call = mock.jsonCalls.at(-1)!;
    assert.equal(call.functionName, functionName);
    assert.deepEqual(call.args, fields);
    assert.deepEqual(call.routeTarget, ["alpha", "beta"]);
    assert.equal(call.historyIndexes, undefined);
  }
});

test("routed Scheme mutation returns its completed JSON result and publishes once", async (t) => {
  const published: Array<{ operation: string; path?: Array<string | number> }> = [];
  const originalPublish = GatewayEventBroker.prototype.publish;
  t.mock.method(
    GatewayEventBroker.prototype,
    "publish",
    function (this: GatewayEventBroker, input: { operation: string; path?: Array<string | number> }) {
      published.push(input);
      return originalPublish.call(this, input);
    },
  );

  const mock = createMockJournal();
  mock.client.schemeToJson = async () => ({
    "*type/quoted*": ["gateway-use-arguments",
      ["path", ["*state*", "alice", "object"]], ["value", "item"]],
  });
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const response = await app.inject({
    method: "POST",
    url: "/api/v1/general/put",
    headers: {
      cookie: SESSION_COOKIE,
      "content-type": "application/scheme",
      "x-sync-web-federation-route": JSON.stringify(["two%20words"]),
    },
    payload: "((path (*state* alice object)) (value item))",
  });

  assert.equal(response.statusCode, 200);
  assert.deepEqual(response.json(), { ok: true, mode: "json", function: "put!" });
  assert.equal(mock.jsonCalls.length, 1);
  assert.equal(mock.schemeCalls.length, 0);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "put!",
    args: [["path", ["*state*", "alice", "object"]], ["value", "item"]],
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
    routeTarget: ["two%20words"],
  });
  assert.deepEqual(published, [{ operation: "put!", path: undefined }]);
});

test("Scheme federation header rejects malformed, empty, conflicting, and unsupported presence", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const cases = [
    { operation: "use", contentType: "application/scheme", route: "not-json", payload: "()" },
    { operation: "use", contentType: "application/scheme", route: "[]", payload: "()" },
    { operation: "use", contentType: "application/scheme", route: '["alpha",3]', payload: "()" },
    { operation: "use", contentType: "application/scheme", route: '["peer\\u0000reader"]', payload: "()" },
    { operation: "use", contentType: "text/plain", route: '["alpha"]', payload: "()" },
    { operation: "prune", contentType: "application/scheme", route: '["alpha"]', payload: "()" },
    {
      operation: "use",
      contentType: "application/json",
      route: '["alpha"]',
      payload: JSON.stringify({ path: ["*state*", "alice"] }),
    },
  ];
  for (const input of cases) {
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${input.operation}`,
      headers: {
        cookie: SESSION_COOKIE,
        "content-type": input.contentType,
        "x-sync-web-federation-route": input.route,
      },
      payload: input.payload,
    });
    assert.equal(response.statusCode, 400, JSON.stringify(input));
    assert.equal(response.json().error, "invalid_request");
  }
  const duplicate = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: {
      cookie: SESSION_COOKIE,
      "content-type": "application/scheme",
      "x-sync-web-federation-route": ['["alpha"]', '["beta"]'],
    },
    payload: "()",
  });
  assert.equal(duplicate.statusCode, 400);
  assert.equal(duplicate.json().error, "invalid_request");

  assert.equal(mock.schemeCalls.length, 0);
  assert.equal(mock.jsonCalls.length, 0);
});

test("Scheme federation header leaves route strings unchanged for Records admission", async (t) => {
  const mock = createMockJournal();
  mock.client.schemeToJson = async () => ({
    "*type/quoted*": ["gateway-use-arguments",
      ["path", ["*state*", "alice", "object"]], ["read-only?", true]],
  });
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const aliases = ["ordinary", "two words", ".", "#.(reader)", "peer|reader", "peer\\reader", "雪", ""];
  const response = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: {
      cookie: SESSION_COOKIE,
      "content-type": "application/scheme",
      "x-sync-web-federation-route": JSON.stringify(aliases),
    },
    payload: "((path (*state* alice object)) (read-only? #t))",
  });
  assert.equal(response.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0].routeTarget, aliases);
  assert.deepEqual(mock.jsonCalls[0].args, [
    ["path", ["*state*", "alice", "object"]], ["read-only?", true],
  ]);
  assert.equal(mock.schemeCalls.length, 0);
});

test("application/scheme resource arguments are admitted only from exact s7 projections", async (t) => {
  const cases = [
    {
      operation: "put", source: "((path (*state* alice object)) (value ()) (expected (nothing)))",
      projection: { "*type/quoted*": ["gateway-use-arguments",
        ["path", ["*state*", "alice", "object"]], ["value", []], ["expected", ["nothing"]]] },
      accepted: true,
    },
    {
      operation: "put", source: "((path (*state* alice object)) (value ()) (expected old) (expected (nothing)))",
      projection: { "*type/quoted*": ["gateway-use-arguments",
        ["path", ["*state*", "alice", "object"]], ["value", []], ["expected", "old"], ["expected", ["nothing"]]] },
    },
    {
      operation: "put", source: "((path (*state* alice object)) (value ()) (read-only? #f))",
      projection: { "*type/quoted*": ["gateway-use-arguments",
        ["path", ["*state*", "alice", "object"]], ["value", []], ["read-only?", false]] },
    },
    {
      operation: "put", source: "((path (*state* alice object)) (value a b))",
      projection: { "*type/quoted*": ["gateway-use-arguments",
        ["path", ["*state*", "alice", "object"]], ["value", "a", "b"]] },
    },
    {
      operation: "use", source: "((path (*state* alice object)) (method value) (arguments 2))",
      projection: { "*type/quoted*": ["gateway-use-arguments",
        ["path", ["*state*", "alice", "object"]], ["method", "value"], ["arguments", 2]] },
    },
    {
      operation: "use", source: "((path (*state* alice object)) (method value) (arguments ()) (read-only? #f) (arguments ()))",
      projection: { "*type/quoted*": ["gateway-use-arguments",
        ["path", ["*state*", "alice", "object"]], ["method", "value"], ["arguments", []],
        ["read-only?", false], ["arguments", []]] },
    },
  ] as const;

  for (const input of cases) {
    const mock = createMockJournal();
    mock.client.schemeToJson = async () => input.projection;
    const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${input.operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/scheme" },
      payload: input.source,
    });
    await app.close();
    assert.equal(response.statusCode, input.accepted ? 200 : 400, input.source);
    assert.equal(mock.schemeCalls.length, input.accepted ? 1 : 0, input.source);
  }
});

test("every batch operation accepts canonical Scheme payloads", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const cases = [
    ["use-batch", "use-batch!", "((paths ((*state* alice a) (*state* alice missing))) (expression? #t))", true],
    ["put-batch", "put-batch!", "((paths ((*state* alice a))) (values (new)) (expected (old)) (expression? #t))", true],
    ["copy-batch", "copy-batch!", "((sources ((*state* alice a))) (paths ((*state* alice b))) (expected (old)) (expression? #t))", true],
    ["retrieve-batch", "retrieve-batch", "((paths ((-1 *state* alice a))) (pinned? #t) (expression? #t))", true],
    ["pin-batch", "pin-batch!", "((paths ((-1 *state* alice a))))", true],
    ["unpin-batch", "unpin-batch!", "((paths ((-1 *state* alice a))))", true],
    ["prune-batch", "prune-batch!", "((paths ((-1 *state* alice a))))", true],
    ["trace-batch", "trace-batch", "((index -1) (paths ((*state* alice a))))", false],
  ] as const;
  for (const [operation, functionName, payload, authenticated] of cases) {
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${operation}`,
      headers: {
        ...(authenticated ? { cookie: SESSION_COOKIE } : {}),
        "content-type": "text/plain",
      },
      payload,
    });
    assert.equal(response.statusCode, 200, operation);
    assert.deepEqual(response.json(), { ok: true, mode: "scheme", function: functionName });
  }
  assert.deepEqual(mock.schemeCalls.map((call) => call.functionName),
    cases.map((entry) => entry[1]));
  for (const [index, entry] of cases.entries()) {
    assert.match(mock.schemeCalls[index].expression,
      new RegExp(`^\\(\\(function ${entry[1].replace("!", "\\!")}\\)`));
  }
});

test("batch JSON preserves the 1,024 boundary and relays Journal limit errors", async (t) => {
  const paths = Array.from({ length: 1024 }, (_, index) => ["*state*", "alice", index]);
  const committed = paths.map((path) => [-1, ...path]);
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const cases = [
    ["use-batch", { paths }],
    ["put-batch", { paths, values: paths, expected: paths, "expression?": true }],
    ["copy-batch", { sources: paths, paths, expected: paths, "expression?": true }],
    ["retrieve-batch", { paths: committed }],
    ["pin-batch", { paths: committed }],
    ["unpin-batch", { paths: committed }],
    ["prune-batch", { paths: committed }],
    ["trace-batch", { index: -1, paths }],
  ] as const;
  for (const [operation, payload] of cases) {
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${operation}`,
      headers: {
        ...(operation === "trace-batch" ? {} : { cookie: SESSION_COOKIE }),
        "content-type": "application/json",
      },
      payload,
    });
    assert.equal(response.statusCode, 200, operation);
  }
  assert.equal(mock.jsonCalls.length, cases.length);
  for (const call of mock.jsonCalls) {
    assert.equal((call.args as { paths: unknown[] }).paths.length, 1024);
  }

  const semantic: JournalClient = {
    ...mock.client,
    async callJson(): Promise<unknown> {
      throw new JournalSemanticError({
        code: "argument-error",
        message: "Batch path count exceeds 1024",
        details: ["error", "argument-error", "Batch path count exceeds 1024"],
      });
    },
  };
  const rejecting = await createApp({ allowAdminRoutes: false, journal: semantic });
  t.after(async () => rejecting.close());
  const response = await rejecting.inject({
    method: "POST",
    url: "/api/v1/general/use-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { paths: [...paths, ["*state*", "alice", 1024]] },
  });
  assert.equal(response.statusCode, 400);
  assert.deepEqual(response.json(), {
    error: "argument-error",
    message: "Batch path count exceeds 1024",
    details: ["error", "argument-error", "Batch path count exceeds 1024"],
    source: "journal",
  });
});

test("removed legacy general batch route remains absent", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());
  const response = await app.inject({
    method: "POST",
    url: "/api/v1/general/batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {},
  });
  assert.equal(response.statusCode, 404);
});

test("Gateway admits staged operations and retained retrieval through federation context", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const setResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/put",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      path: ["*state*", "docs"], value: "hello",
      "$federation": { route: ["bob"] },
    },
  });
  assert.equal(setResult.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0]?.routeTarget, ["bob"]);

  const getBatchResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/use-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      paths: [["*state*", "docs"], ["*state*", "docs"]],
      "$federation": { route: ["bob"] },
    },
  });
  assert.equal(getBatchResult.statusCode, 200);
  assert.equal(mock.jsonCalls[1]?.functionName, "use-batch!");
  assert.deepEqual(mock.jsonCalls[1]?.routeTarget, ["bob"]);
  assert.deepEqual(mock.jsonCalls[1]?.args, {
    paths: [["*state*", "docs"], ["*state*", "docs"]],
  });

  const setBatchResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/put-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      paths: [["*state*", "docs"], ["*state*", "docs"]],
      values: ["hello", "world"], expected: [false, ["nothing"]],
      "$federation": { route: ["bob"] },
    },
  });
  assert.equal(setBatchResult.statusCode, 200);
  assert.equal(mock.jsonCalls[2]?.functionName, "put-batch!");
  assert.deepEqual(mock.jsonCalls[2]?.routeTarget, ["bob"]);
  assert.deepEqual(mock.jsonCalls[2]?.args, {
    paths: [["*state*", "docs"], ["*state*", "docs"]],
    values: ["hello", "world"], expected: [false, ["nothing"]],
  });

  const callResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/run",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      path: ["*state*", "docs", "program"], arguments: ["value"],
      "$federation": { route: ["bob"] },
    },
  });
  assert.equal(callResult.statusCode, 200);
  assert.equal(mock.jsonCalls[3]?.functionName, "run!");
  assert.deepEqual(mock.jsonCalls[3]?.routeTarget, ["bob"]);

  const retrieveResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/retrieve",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      path: [3, "archive", 1, "*state*", "docs"], "pinned?": false,
      "$federation": { route: ["provider"] },
    },
  });
  assert.equal(retrieveResult.statusCode, 200);
  assert.equal(mock.jsonCalls[4]?.functionName, "retrieve");
  assert.deepEqual(mock.jsonCalls[4]?.routeTarget, ["provider"]);
  assert.deepEqual(mock.jsonCalls[4]?.args, {
    path: [3, "archive", 1, "*state*", "docs"], "pinned?": false,
  });

  const retrieveBatchResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/retrieve-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      paths: [[3, "archive", 1, "*state*", "docs"]],
      "$federation": { route: ["provider"] },
    },
  });
  assert.equal(retrieveBatchResult.statusCode, 200);
  assert.equal(mock.jsonCalls[5]?.functionName, "retrieve-batch");
  assert.deepEqual(mock.jsonCalls[5]?.routeTarget, ["provider"]);

  for (const operation of [
    "trace-batch",
    "pin", "pin-batch", "unpin-batch", "prune", "prune-batch",
    "bridge", "config", "admins", "route",
    "authorizations", "authorize", "deauthorize",
  ]) {
    const result = await app.inject({
      method: "POST",
      url: `/api/v1/general/${operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: { "$federation": { route: ["bob"] } },
    });
    assert.equal(result.statusCode, 400, operation);
    assert.match(result.json().message, /Federation context is not allowed/);
  }
});

test("Gateway rejects outward federation history", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const cases = [
    { operation: "use", context: { route: ["bob"], history: [-1, -1] } },
    { operation: "retrieve", context: { route: [], history: [-1] } },
    { operation: "retrieve", context: { route: ["bob"], history: [-1] } },
  ];
  for (const entry of cases) {
    const result = await app.inject({
      method: "POST",
      url: `/api/v1/general/${entry.operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: { path: [-1, "*state*", "docs"], "$federation": entry.context },
    });
    assert.equal(result.statusCode, 400);
  }
});

test("authorization JSON routes preserve key-index and retrieve as distinct exact fields", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());
  const rule = {
    principal: ["peer-a", "*state*", "bob"],
    "key-index": [-32, -1],
    path: ["docs"], "put!": false, "use!": { "read-only?": true }, "run!": false, retrieve: [0, -1],
  };
  const cases = [
    { operation: "authorizations", functionName: "authorizations", args: { user: ["*state*", "alice"] } },
    { operation: "authorize", functionName: "authorize!", args: { user: ["*state*", "alice"], rule } },
    { operation: "deauthorize", functionName: "deauthorize!", args: { user: ["*state*", "alice"], rule } },
  ];

  for (const entry of cases) {
    const result = await app.inject({
      method: "POST",
      url: `/api/v1/general/${entry.operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: entry.args,
    });
    assert.equal(result.statusCode, 200, entry.operation);
  }
  assert.deepEqual(mock.jsonCalls, cases.map((entry) => ({
    functionName: entry.functionName,
    args: entry.args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  })));
});

test("authorization Scheme routes preserve exact key-index and retrieve expressions", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());
  const rule = "((principal (peer-a *state* bob)) (key-index (-32 -1)) (path (docs)) (put! #f) (use! ((read-only? #t))) (run! #f) (retrieve (0 -1)))";
  const cases = [
    { operation: "authorizations", functionName: "authorizations", args: "((user (*state* alice)))" },
    { operation: "authorize", functionName: "authorize!", args: `((user (*state* alice)) (rule ${rule}))` },
    { operation: "deauthorize", functionName: "deauthorize!", args: `((user (*state* alice)) (rule ${rule}))` },
  ];

  for (const entry of cases) {
    const result = await app.inject({
      method: "POST",
      url: `/api/v1/general/${entry.operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
      payload: entry.args,
    });
    assert.equal(result.statusCode, 200, entry.operation);
  }
  assert.deepEqual(mock.schemeCalls.map((call) => call.functionName), cases.map((entry) => entry.functionName));
  cases.forEach((entry, index) => {
    assert.ok(mock.schemeCalls[index].expression.includes(`(arguments ${entry.args})`));
    assert.match(mock.schemeCalls[index].expression, /\(authentication \(\(identity \(\*state\* test-user-id\)\)/);
  });
});

test("POST /api/v1/general/admins forwards the username-keyed admin projection", async (t) => {
  const mock = createMockJournal();
  mock.client.callJson = async (input: JournalCall): Promise<unknown> => {
    mock.jsonCalls.push(input);
    return { alice: ["*state*", "alice"], bob: ["*state*", "bob"] };
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/admins",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {},
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(JSON.parse(res.body), {
    alice: ["*state*", "alice"],
    bob: ["*state*", "bob"],
  });
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "*admins-get*",
    args: {},
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/set-admins preserves zero, one, and multiple username entries", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());
  const replacements = [
    { admins: {} },
    { admins: { alice: ["*state*", "alice"] } },
    { admins: { alice: ["*state*", "alice"], bob: ["*state*", "bob"] } },
  ];

  for (const args of replacements) {
    const response = await app.inject({
      method: "POST",
      url: "/api/v1/general/set-admins",
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: args,
    });
    assert.equal(response.statusCode, 200);
  }
  assert.deepEqual(mock.jsonCalls.map((call) => ({
    functionName: call.functionName,
    args: call.args,
  })), replacements.map((args) => ({ functionName: "*admins-set*", args })));
});

test("POST /api/v1/general/set-window forwards positive window value", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { value: 32 };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/set-window",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "*window-set*",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/use accepts legacy JSON array payload", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = [[["path", ["*state*", "docs"]]]];
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "use!",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/use accepts Lisp payload and injects identity into expression", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: "(((path (*state* docs))))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].functionName, "use!");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function use!\) /);
  assert.match(
    mock.schemeCalls[0].expression,
    /\(arguments \(\(\(path \(\*state\* docs\)\)\)\)\)/
  );
  assert.match(
    mock.schemeCalls[0].expression,
    /\(authentication \(\(identity \(\*state\* test-user-id\)\) \(credentials "test-journal-secret"\)\)\)/
  );
});

test("Scheme use classification is delegated to the Journal reader codec", async (t) => {
  const mock = createMockJournal();
  mock.client.schemeToJson = async (expression) => {
    mock.proxiedSchemeExpressions.push(expression);
    return {
      "*type/quoted*": [
        "gateway-use-arguments",
        ["path", ["*state*", "docs"]],
        ["read-only?", true],
      ],
    };
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const source = "((path (*state* docs)) (read-only? #t) (arguments (\\\"nested (read-only? #f)\\\")))";
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: source,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.proxiedSchemeExpressions.length, 1);
  assert.equal(mock.proxiedSchemeExpressions[0], buildSchemeArgumentsProjection(source));
  assert.equal(mock.schemeCalls.length, 1);
});

test("malformed Scheme projections remain potentially mutating without replacing Scheme execution", async (t) => {
  const mock = createMockJournal();
  mock.client.schemeToJson = async () => ["error", { "*type/quoted*": "parse-error" }];
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/scheme" },
    payload: "((paths ((*state* docs))) (read-only? #t))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
});

test("direct resource operation arguments remain operation fields", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const cases = [
    {
      operation: "use", functionName: "use!",
      payload: { path: ["*state*", "test-user-id", "counter"], method: "*init*", arguments: [5] },
    },
    {
      operation: "use-batch", functionName: "use-batch!",
      payload: {
        paths: [["*state*", "test-user-id", "counter"]],
        methods: ["increment!"], arguments: [[2]],
      },
    },
    {
      operation: "retrieve", functionName: "retrieve",
      payload: { path: [-1, "*state*", "test-user-id", "counter"], method: "increment!", arguments: [3] },
    },
    {
      operation: "retrieve-batch", functionName: "retrieve-batch",
      payload: {
        paths: [[-1, "*state*", "test-user-id", "counter"]],
        methods: ["increment!"], arguments: [[4]],
      },
    },
  ] as const;

  for (const entry of cases) {
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${entry.operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: entry.payload,
    });
    assert.equal(response.statusCode, 200, entry.operation);
  }

  assert.deepEqual(mock.jsonCalls.map((call) => ({
    functionName: call.functionName, args: call.args,
  })), cases.map((entry) => ({
    functionName: entry.functionName, args: entry.payload,
  })));
});

test("resource operation arguments reject nested wrappers and transport envelopes", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  for (const operation of ["use", "use-batch", "retrieve", "retrieve-batch"]) {
    for (const payload of [
      { path: ["*state*", "test-user-id", "counter"], arguments: { arguments: [] } },
      { function: "use!", arguments: { path: ["*state*", "test-user-id", "counter"] } },
      { authentication: "forged", arguments: [] },
    ]) {
      const response = await app.inject({
        method: "POST",
        url: `/api/v1/general/${operation}`,
        headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
        payload,
      });
      assert.equal(response.statusCode, 400, `${operation}: ${JSON.stringify(payload)}`);
      assert.equal(response.json().error, "invalid_request");
    }
  }
  assert.equal(mock.jsonCalls.length, 0);
});

test("POST /api/v1/general/run forwards a staged program path and arguments", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/run",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      path: ["*state*", "test-user-id", "programs", "echo"],
      arguments: [1, "two"],
    },
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.equal(mock.jsonCalls[0].functionName, "run!");
  assert.deepEqual(mock.jsonCalls[0].args, {
    path: ["*state*", "test-user-id", "programs", "echo"],
    arguments: [1, "two"],
  });
  assert.equal(mock.jsonCalls[0].authentication, "test-journal-secret");
  assert.equal(mock.jsonCalls[0].identityId, "test-user-id");
});

test("POST /api/v1/general/run rejects a nested arguments wrapper", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/run",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      path: ["*state*", "test-user-id", "programs", "echo"],
      arguments: { arguments: [] },
    },
  });

  assert.equal(res.statusCode, 400);
  assert.equal(res.json().error, "invalid_request");
  assert.equal(mock.jsonCalls.length, 0);
});

test("POST /api/v1/general/run rejects transport authority fields", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  for (const reserved of ["function", "authentication"] as const) {
    const res = await app.inject({
      method: "POST",
      url: "/api/v1/general/run",
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: {
        path: ["*state*", "test-user-id", "programs", "echo"],
        arguments: [],
        [reserved]: "forged",
      },
    });
    assert.equal(res.statusCode, 400, reserved);
    assert.equal(res.json().error, "invalid_request", reserved);
  }
  assert.equal(mock.jsonCalls.length, 0);
});

test("returns 415 for unsupported content type", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/xml" },
    payload: "<x/>",
  });

  assert.equal(res.statusCode, 415);
  assert.equal(res.json().error, "unsupported_media_type");
});

test("returns 400 for an arguments field on operations that do not define it", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/put",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { arguments: [] },
  });

  assert.equal(res.statusCode, 400);
  assert.equal(res.json().error, "invalid_request");
});

test("returns structured 400 for malformed JSON", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: "{malformed",
  });

  assert.equal(res.statusCode, 400);
  assert.equal(res.json().error, "invalid_request");
});

test("unexpected errors are not reclassified by familiar message fragments", async (t) => {
  const mock = createMockJournal();
  mock.client.callJson = async () => {
    throw new Error("Unsupported content-type in a Federation context");
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: ["*state*", "test-user-id", "doc"], "read-only?": true },
  });

  assert.equal(res.statusCode, 502);
  assert.equal(res.json().error, "gateway_error");
});

test("admin root routes are disabled unless explicitly enabled", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/root/step",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: [],
  });

  assert.equal(res.statusCode, 404);
});

test("admin root routes forward when enabled", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: true, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/root/step",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: [],
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "*step*",
    args: [],
    authentication: JOURNAL_SECRET,
  });
});

test("relays journal semantic error payloads as HTTP errors (JSON mode)", async (t) => {
  const journal: JournalClient = {
    async callJson(): Promise<unknown> {
      throw new JournalSemanticError({
        code: "authentication-error",
        message: "Could not authenticate restricted interface call",
        details: [
          "error",
          { "*type/quoted*": "authentication-error" },
          { "*type/string*": "Could not authenticate restricted interface call" },
        ],
      });
    },
    async callScheme(): Promise<unknown> {
      return { ok: true };
    },
    async callRootJson(): Promise<unknown> {
      return { ok: true };
    },
    async callRootScheme(): Promise<unknown> {
      return { ok: true };
    },
    async proxyJson(): Promise<unknown> {
      return { ok: true };
    },
    async proxyScheme(): Promise<string> {
      return "";
    },
    async schemeToJson(): Promise<unknown> {
      return { "*type/quoted*": ["gateway-use-arguments"] };
    },
  };

  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: ["*state*", "docs"], "pinned?": true, "proof?": true },
  });

  assert.equal(res.statusCode, 400);
  assert.deepEqual(res.json(), {
    error: "authentication-error",
    message: "Could not authenticate restricted interface call",
    details: [
      "error",
      { "*type/quoted*": "authentication-error" },
      { "*type/string*": "Could not authenticate restricted interface call" },
    ],
    source: "journal",
  });
});

test("POST /api/v1/journal/interface forwards Scheme body to journal", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/journal/interface",
    headers: { "content-type": "text/plain" },
    payload: "((function info))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(res.headers["content-type"], "text/plain; charset=utf-8");
  assert.equal(res.body, "((public-key #u(1 2 3)))");
  assert.equal(mock.proxiedSchemeExpressions.length, 1);
  assert.equal(mock.proxiedSchemeExpressions[0], "((function info))");
});

test("POST /api/v1/journal/interface forwards JSON body to journal", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const body = { function: "synchronize", arguments: { index: 0 } };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/journal/interface",
    headers: { "content-type": "application/json" },
    payload: body,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.proxiedJsonBodies.length, 1);
  assert.deepEqual(mock.proxiedJsonBodies[0], body);
});

test("POST /api/v1/journal/interface treats missing content-type as Scheme (sync-remote compat)", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/journal/interface",
    payload: "((function info))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(res.body, "((public-key #u(1 2 3)))");
  assert.equal(mock.proxiedSchemeExpressions.length, 1);
  assert.equal(mock.proxiedSchemeExpressions[0], "((function info))");
});

test("POST /api/v1/journal/interface treats octet-stream as Scheme (sync-remote compat)", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/journal/interface",
    headers: { "content-type": "application/octet-stream" },
    payload: "((function info))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(res.body, "((public-key #u(1 2 3)))");
  assert.equal(mock.proxiedSchemeExpressions.length, 1);
  assert.equal(mock.proxiedSchemeExpressions[0], "((function info))");
});

test("POST /api/v1/journal/interface treats form content as Scheme (sync-remote compat)", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/journal/interface",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    payload: "((function info))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(res.body, "((public-key #u(1 2 3)))");
  assert.equal(mock.proxiedSchemeExpressions.length, 1);
  assert.equal(mock.proxiedSchemeExpressions[0], "((function info))");
});

test("GET /api/v1/general/info forwards to info without auth", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({ method: "GET", url: "/api/v1/general/info" });
  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], { functionName: "info" });
});


test("non-semantic journal error becomes 502", async (t) => {
  const journal: JournalClient = {
    async callJson(): Promise<unknown> { throw new Error("connection refused"); },
    async callScheme(): Promise<unknown> { return {}; },
    async callRootJson(): Promise<unknown> { return {}; },
    async callRootScheme(): Promise<unknown> { return {}; },
    async proxyJson(): Promise<unknown> { return {}; },
    async proxyScheme(): Promise<string> { return ""; },
    async schemeToJson(): Promise<unknown> {
      return { "*type/quoted*": ["gateway-use-arguments"] };
    },
  };
  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/use",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: ["*state*", "docs"] },
  });

  assert.equal(res.statusCode, 502);
  assert.equal(res.json().error, "gateway_error");
});

test("relays journal semantic error payloads as HTTP errors (Scheme mode)", async (t) => {
  const journal: JournalClient = {
    async callJson(): Promise<unknown> { return {}; },
    async callScheme(): Promise<unknown> {
      throw new JournalSemanticError({
        code: "permissions-error",
        message: "User may only write to their own space",
        details: ["error", { "*type/quoted*": "permissions-error" }, { "*type/string*": "User may only write to their own space" }],
      });
    },
    async callRootJson(): Promise<unknown> { return {}; },
    async callRootScheme(): Promise<unknown> { return {}; },
    async proxyJson(): Promise<unknown> { return {}; },
    async proxyScheme(): Promise<string> { return ""; },
    async schemeToJson(): Promise<unknown> {
      return { "*type/quoted*": ["gateway-use-arguments"] };
    },
  };
  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/put",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: "((path (*state* alice foo)) (value bar))",
  });

  assert.equal(res.statusCode, 400);
  assert.equal(res.json().error, "permissions-error");
  assert.equal(res.json().source, "journal");
});

test("POST /api/v1/general/put forwards with auth in JSON mode", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: ["*state*", "mykey"], value: "myvalue", expected: false };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/put",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "put!",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("OpenAPI spec includes per-operation body examples", async (t) => {
  const app = Fastify({
    ajv: { customOptions: { keywords: ["example"], allowUnionTypes: true } },
  });
  app.addContentTypeParser("text/plain", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/scheme", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/octet-stream", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  app.addContentTypeParser("application/x-www-form-urlencoded", { parseAs: "string" }, (_req, body, done) =>
    done(null, body)
  );
  await app.register(fastifySwagger, {
    openapi: { info: { title: "test", version: "0" } },
  });
  await app.register(gatewayRoutes, {
    journal: createMockJournal().client,
    allowAdminRoutes: true,
    journalSecret: JOURNAL_SECRET,
    kratos: createMockKratos(),
  });
  await app.ready();
  t.after(async () => app.close());

  type SchemaWithExample = { example?: unknown; description?: string; type?: unknown };
  type MediaType = { schema?: SchemaWithExample };
  type Operation = { description?: string; requestBody?: { content?: Record<string, MediaType> } };
  type PathItem = Record<string, Operation>;
  const paths = (app.swagger() as { paths: Record<string, PathItem> }).paths;

  const schemaExample = (path: string) =>
    paths[path]?.post?.requestBody?.content?.["application/json"]?.schema?.example;

  assert.deepEqual(schemaExample("/api/v1/general/use"), {
    path: ["*state*", "mykey"], "read-only?": true, "expression?": true,
  });
  assert.deepEqual(schemaExample("/api/v1/general/use-batch"), {
    paths: [["*state*", "a"]], "read-only?": true, "expression?": true,
  });
  assert.deepEqual(schemaExample("/api/v1/general/put-batch"), {
    paths: [["*state*", "a"]], values: ["value"], "expression?": true,
  });
  assert.deepEqual(schemaExample("/api/v1/general/trace"), {
    index: 0, path: ["*state*", "mykey"],
  });
  assert.deepEqual(schemaExample("/api/v1/general/retrieve-batch"), {
    paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]],
    "pinned?": true, "index?": true, "expression?": true,
  });
  assert.deepEqual(schemaExample("/api/v1/general/trace-batch"), {
    index: 0, paths: [["*state*", "a"], ["*state*", "b"]],
  });
  assert.deepEqual(schemaExample("/api/v1/general/pin-batch"), {
    paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]],
  });
  assert.deepEqual(schemaExample("/api/v1/general/unpin-batch"), {
    paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]],
  });
  assert.deepEqual(schemaExample("/api/v1/general/prune"), {
    path: [-1, "peer-a", -1, "*state*", "mykey"],
  });
  assert.deepEqual(schemaExample("/api/v1/general/prune-batch"), {
    paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]],
  });
  assert.deepEqual(schemaExample("/api/v1/general/bridge"), {
    name: "peer-a",
    interface: "http://peer-a/interface",
    "remote-name": "my-journal",
  });
  assert.deepEqual(schemaExample("/api/v1/general/admins"), {});
  assert.deepEqual(schemaExample("/api/v1/general/set-admins"), {
    admins: { admin: ["*state*", "admin"], alice: ["*state*", "alice"] },
  });
  assert.deepEqual(schemaExample("/api/v1/general/set-window"), { value: 128 });
  assert.deepEqual(schemaExample("/api/v1/general/truncate"), { index: 0 });
  assert.deepEqual(schemaExample("/api/v1/general/run"), {
    path: ["*state*", "alice", "programs", "example"], arguments: [],
  });
  const exactAuthorization = {
    user: ["*state*", "alice"],
    rule: {
      principal: ["peer-a", "*state*", "bob"],
      "key-index": [-32, -1], path: ["docs"], "put!": false, "use!": { "read-only?": true }, "run!": false, retrieve: [0, -1],
    },
  };
  assert.deepEqual(schemaExample("/api/v1/general/authorize"), exactAuthorization);
  assert.deepEqual(schemaExample("/api/v1/general/deauthorize"), exactAuthorization);
  const setBatchOperation = paths["/api/v1/general/put-batch"]?.post;
  assert.match(setBatchOperation?.description ?? "", /one snapshot/);
  assert.match(setBatchOperation?.description ?? "", /persists atomically/);
  const authorizationOperation = paths["/api/v1/general/authorize"]?.post;
  assert.match(authorizationOperation?.description ?? "", /Self-local/);
  assert.match(authorizationOperation?.description ?? "", /key-index.*authentication window/);
  assert.match(authorizationOperation?.description ?? "", /two-index history range/);
  const authorizationSchema = authorizationOperation?.requestBody?.content?.["application/json"]?.schema;
  assert.deepEqual(authorizationSchema?.type, ["array", "object"]);
  assert.match(authorizationSchema?.description ?? "", /schema stays permissive/);
  assert.deepEqual(schemaExample("/api/v1/root/step"), []);
  assert.deepEqual(schemaExample("/api/v1/root/eval"), [["+", 1, 2]]);

  const schemeExample = (path: string) =>
    paths[path]?.post?.requestBody?.content?.["text/plain"]?.schema?.example;

  assert.equal(schemeExample("/api/v1/general/use"),
    "((path (*state* mykey)) (read-only? #t))");
  assert.equal(schemeExample("/api/v1/general/use-batch"),
    "((paths ((*state* a))) (read-only? #t))");
  assert.equal(schemeExample("/api/v1/general/put-batch"),
    "((paths ((*state* a))) (values (value)))");
  assert.equal(schemeExample("/api/v1/general/trace"),
    "((index 0) (path (*state* mykey)))");
  assert.equal(schemeExample("/api/v1/general/retrieve-batch"),
    "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))) (pinned? #t) (index? #t))");
  assert.equal(schemeExample("/api/v1/general/trace-batch"),
    "((index 0) (paths ((*state* a) (*state* b))))");
  assert.equal(schemeExample("/api/v1/general/pin-batch"),
    "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))");
  assert.equal(schemeExample("/api/v1/general/unpin-batch"),
    "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))");
  assert.equal(schemeExample("/api/v1/general/prune"),
    "((path (-1 peer-a -1 *state* mykey)))");
  assert.equal(schemeExample("/api/v1/general/prune-batch"),
    "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))");
  assert.equal(schemeExample("/api/v1/general/bridge"), '((name peer-a) (interface "http://peer-a/interface") (remote-name my-journal))');
  assert.equal(schemeExample("/api/v1/general/admins"), "()");
  assert.equal(schemeExample("/api/v1/general/set-admins"),
    "((admins ((admin (*state* admin)) (alice (*state* alice)))))");
  assert.equal(schemeExample("/api/v1/general/set-window"), "((value 128))");
  assert.equal(schemeExample("/api/v1/general/truncate"), "((index 0))");
  assert.equal(schemeExample("/api/v1/general/run"), "((path (*state* alice programs example)) (arguments ()))");
  const exactSchemeAuthorization = "((user (*state* alice)) (rule ((principal (peer-a *state* bob)) (key-index (-32 -1)) (path (docs)) (put! #f) (use! ((read-only? #t))) (run! #f) (retrieve (0 -1)))))";
  assert.equal(schemeExample("/api/v1/general/authorize"), exactSchemeAuthorization);
  assert.equal(schemeExample("/api/v1/general/deauthorize"), exactSchemeAuthorization);
  assert.equal(schemeExample("/api/v1/root/eval"), "(+ 1 2)");
  assert.equal(schemeExample("/api/v1/root/set-secret"), '"new-admin-secret"');
  assert.equal(schemeExample("/api/v1/root/eval"), "(+ 1 2)");
});

test("POST /api/v1/tokens creates a token and returns token once", async (t) => {
  const kratos = createMockKratos();
  const app = await createApp({ allowAdminRoutes: false, kratos });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/tokens",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { description: "ci bot" },
  });
  assert.equal(res.statusCode, 201);
  const body = res.json();
  assert.ok(typeof body.token === "string", "token is a string");
  assert.ok(body.token.startsWith("sync-"), "token starts with sync-");
  assert.ok(typeof body.id === "string", "id is a string");
  assert.ok(typeof body.created_at === "string", "created_at is a string");
  assert.equal(body.description, "ci bot");

  const parts = body.token.split("-");
  assert.equal(parts.length, 5, "token has 5 dash-separated parts");
  assert.equal(parts[0], "sync");
});

test("POST /api/v1/tokens returns 401 without session cookie", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/tokens",
    headers: { "content-type": "application/json" },
    payload: { description: "bot" },
  });
  assert.equal(res.statusCode, 401);
});

test("GET /api/v1/tokens lists tokens without secrets", async (t) => {
  const existingToken: ApiTokenEntry = {
    hash: createHash("sha256").update("somesecret").digest("hex"),
    description: "my agent",
    created_at: "2026-05-14T00:00:00Z",
  };
  const kratos = createMockKratos(IDENTITY_ID, { abc12345: existingToken });
  const app = await createApp({ allowAdminRoutes: false, kratos });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "GET",
    url: "/api/v1/tokens",
    headers: { cookie: SESSION_COOKIE },
  });
  assert.equal(res.statusCode, 200);
  const body = res.json() as Array<{ id: string; description: string; created_at: string }>;
  assert.equal(body.length, 1);
  assert.equal(body[0].id, "abc12345");
  assert.equal(body[0].description, "my agent");
  assert.equal(body[0].created_at, "2026-05-14T00:00:00Z");
  assert.ok(!("hash" in body[0]), "hash must not be returned");
  assert.ok(!("token" in body[0]), "token must not be returned");
});

test("DELETE /api/v1/tokens/:id revokes a token", async (t) => {
  const existingToken: ApiTokenEntry = {
    hash: createHash("sha256").update("somesecret").digest("hex"),
    description: "old bot",
    created_at: "2026-05-14T00:00:00Z",
  };
  const kratos = createMockKratos(IDENTITY_ID, { abc12345: existingToken });
  const app = await createApp({ allowAdminRoutes: false, kratos });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "DELETE",
    url: "/api/v1/tokens/abc12345",
    headers: { cookie: SESSION_COOKIE },
  });
  assert.equal(res.statusCode, 204);

  const listRes = await app.inject({
    method: "GET",
    url: "/api/v1/tokens",
    headers: { cookie: SESSION_COOKIE },
  });
  assert.deepEqual(listRes.json(), []);
});

test("DELETE /api/v1/tokens/:id returns 404 for unknown token", async (t) => {
  const kratos = createMockKratos();
  const app = await createApp({ allowAdminRoutes: false, kratos });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "DELETE",
    url: "/api/v1/tokens/nonexistent",
    headers: { cookie: SESSION_COOKIE },
  });
  assert.equal(res.statusCode, 404);
  assert.equal(res.json().error, "not_found");
});

test("POST /api/v1/tokens accepts X-Session-Token for headless bootstrap", async (t) => {
  const kratos = createMockKratos();
  const app = await createApp({ allowAdminRoutes: false, kratos });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/tokens",
    headers: { "x-session-token": "kratos-session-token-abc", "content-type": "application/json" },
    payload: { description: "headless agent" },
  });
  assert.equal(res.statusCode, 201);
  assert.ok(res.json().token.startsWith("sync-"));
});

test("POST /api/v1/tokens rejects Bearer API token auth", async (t) => {
  const kratos = createMockKratos();
  const app = await createApp({ allowAdminRoutes: false, kratos });
  t.after(async () => app.close());

  const fakeToken = `sync-${"a".repeat(32)}-deadbeef-0-${"b".repeat(64)}`;
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/tokens",
    headers: { authorization: `Bearer ${fakeToken}`, "content-type": "application/json" },
    payload: { description: "should be rejected" },
  });
  assert.equal(res.statusCode, 401);
});
