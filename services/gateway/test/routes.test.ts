import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";
import Fastify from "fastify";
import fastifySwagger from "@fastify/swagger";
import { gatewayRoutes } from "../src/routes";
import { JournalSemanticError } from "../src/journal";
import type { JournalCall, JournalClient } from "../src/journal";
import type { ApiTokenEntry, KratosAdminIdentity, KratosClient } from "../src/kratos";

const JOURNAL_SECRET = "test-journal-secret";
const IDENTITY_ID = "test-user-id";
const SESSION_COOKIE = "ory_kratos_session=test-session-token";

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
}) => {
  const app = Fastify({ ajv: { customOptions: { keywords: ["example"], allowUnionTypes: true } } });
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
    url: "/api/v1/general/get",
    headers: { "content-type": "application/json" },
    payload: [],
  });
  assert.equal(res.statusCode, 401);
  const body = res.json();
  assert.equal(body.error, "unauthorized");
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
    url: "/api/v1/general/get",
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

test("POST /api/v1/general/get accepts JSON keyword-object payload with Kratos session", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: ["*state*", "docs"] };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "get",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/admins forwards to interface admin operation", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/admins",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {},
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "*admins-get*",
    args: {},
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
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

test("POST /api/v1/general/get accepts legacy JSON array payload", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = [[["path", ["*state*", "docs"]]]];
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "get",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/get accepts Lisp payload and injects identity into expression", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: "(((path (*state* docs))))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].functionName, "get");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function get\) /);
  assert.match(
    mock.schemeCalls[0].expression,
    /\(arguments \(\(\(path \(\*state\* docs\)\)\)\)\)/
  );
  assert.match(
    mock.schemeCalls[0].expression,
    /\(authentication \(\(identity test-user-id\) \(credentials "test-journal-secret"\)\)\)/
  );
});

test("POST /api/v1/general/batch accepts JSON payload", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = {
    queries: [
      { function: "get", arguments: { path: ["*state*", "docs"] } },
      { function: "config" },
    ],
  };

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].functionName, "batch!");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function batch!\) /);
  assert.ok(mock.schemeCalls[0].expression.includes("(queries "));
  assert.ok(mock.schemeCalls[0].expression.includes("((function get) (arguments ((path (*state* docs))))"));
  assert.ok(mock.schemeCalls[0].expression.includes("((function config))"));
  assert.match(
    mock.schemeCalls[0].expression,
    /\(authentication \(\(identity test-user-id\) \(credentials "test-journal-secret"\)\)\)/
  );
});

test("POST /api/v1/general/batch decodes Scheme batch results for JSON callers", async (t) => {
  const mock = createMockJournal();
  mock.client.callScheme = async (input: { expression: string; functionName: string }) => {
    mock.schemeCalls.push(input);
    return '(#t round3 (directory ((data directory)) #t) ((public ((window 1024)))) #u(1 2 255))';
  };
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { queries: [{ function: "set!", arguments: { path: ["*state*", "admin", "x"], value: "round3", "expression?": true } }] },
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(res.json(), [
    true,
    "round3",
    ["directory", { data: "directory" }, true],
    { public: { window: 1024 } },
    { "*type/byte-vector*": "0102ff" },
  ]);
});

test("POST /api/v1/general/batch accepts Lisp payload and injects identity into expression", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload:
      "(((queries (((function get) (arguments ((path (*state* docs)))) ((function config))))))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].functionName, "batch!");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function batch!\) /);
  assert.ok(mock.schemeCalls[0].expression.includes("(queries "));
  assert.ok(
    mock.schemeCalls[0].expression.includes(
      "((function get) (arguments ((path (*state* docs))))"
    )
  );
  assert.ok(mock.schemeCalls[0].expression.includes("((function config))"));
  assert.match(
    mock.schemeCalls[0].expression,
    /\(authentication \(\(identity test-user-id\) \(credentials "test-journal-secret"\)\)\)/
  );
});

test("returns 415 for unsupported content type", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/xml" },
    payload: "<x/>",
  });

  assert.equal(res.statusCode, 415);
  assert.equal(res.json().error, "unsupported_media_type");
});

test("returns 400 for JSON arguments wrapper", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { arguments: "bad" },
  });

  assert.equal(res.statusCode, 400);
  assert.equal(res.json().error, "invalid_request");
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
  };

  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
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
    payload: "((function synchronize) (arguments ((index 0))))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(res.headers["content-type"], "text/plain; charset=utf-8");
  assert.equal(res.body, "((public-key #u(1 2 3)))");
  assert.equal(mock.proxiedSchemeExpressions.length, 1);
  assert.equal(mock.proxiedSchemeExpressions[0], "((function synchronize) (arguments ((index 0))))");
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
  };
  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
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
  };
  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/set",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: "((path (*state* alice foo)) (value bar))",
  });

  assert.equal(res.statusCode, 400);
  assert.equal(res.json().error, "permissions-error");
  assert.equal(res.json().source, "journal");
});

test("POST /api/v1/general/set forwards with auth in JSON mode", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: ["*state*", "mykey"], value: "myvalue" };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/set",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "set!",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("OpenAPI spec includes per-operation body examples", async (t) => {
  const app = Fastify({ ajv: { customOptions: { keywords: ["example"], allowUnionTypes: true } } });
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

  type SchemaWithExample = { example?: unknown };
  type MediaType = { schema?: SchemaWithExample };
  type Operation = { requestBody?: { content?: Record<string, MediaType> } };
  type PathItem = Record<string, Operation>;
  const paths = (app.swagger() as { paths: Record<string, PathItem> }).paths;

  const schemaExample = (path: string) =>
    paths[path]?.post?.requestBody?.content?.["application/json"]?.schema?.example;

  assert.deepEqual(schemaExample("/api/v1/general/get"), { path: ["*state*", "mykey"], "expression?": true });
  assert.deepEqual(schemaExample("/api/v1/general/bridge"), {
    name: "peer-a",
    "info-local": {
      interface: "http://peer-a/interface",
      policy: { publish: "push", subscribe: "pull" },
      role: false,
      "remote-name": "my-journal",
    },
  });
  assert.deepEqual(schemaExample("/api/v1/general/admins"), {});
  assert.deepEqual(schemaExample("/api/v1/general/set-admins"), { admins: ["admin", "alice"] });
  assert.deepEqual(schemaExample("/api/v1/general/set-window"), { value: 128 });
  assert.deepEqual(schemaExample("/api/v1/general/batch"), {
    queries: [{ function: "get", arguments: { path: ["*state*", "mykey"] } }, { function: "config" }],
  });
  assert.deepEqual(schemaExample("/api/v1/root/step"), []);
  assert.deepEqual(schemaExample("/api/v1/root/eval"), [["+", 1, 2]]);

  const schemeExample = (path: string) =>
    paths[path]?.post?.requestBody?.content?.["text/plain"]?.schema?.example;

  assert.equal(schemeExample("/api/v1/general/get"), "((path (*state* mykey)))");
  assert.equal(schemeExample("/api/v1/general/bridge"), '((name peer-a) (info-local ((interface "http://peer-a/interface") (policy ((publish push) (subscribe pull))) (role #f) (remote-name my-journal))))');
  assert.equal(schemeExample("/api/v1/general/admins"), "()");
  assert.equal(schemeExample("/api/v1/general/set-admins"), "((admins (admin alice)))");
  assert.equal(schemeExample("/api/v1/general/set-window"), "((value 128))");
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
