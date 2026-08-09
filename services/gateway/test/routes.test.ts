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

test("POST /api/v1/general/get forwards authenticated bridge discovery", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: ["*bridge*"] };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "get",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
});

test("POST /api/v1/general/resolve forwards one canonical committed path", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/resolve",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: [-1, "carol", 3, "bob", 7, "*state*", "docs"] },
  });

  assert.equal(res.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "resolve",
    args: { path: [-1, "carol", 3, "bob", 7, "*state*", "docs"] },
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
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
    { operation: "get-batch", functionName: "get-batch" },
    { operation: "resolve-batch", functionName: "resolve-batch" },
    { operation: "pin-batch", functionName: "pin-batch!" },
    { operation: "unpin-batch", functionName: "unpin-batch!" },
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
  assert.deepEqual(mock.jsonCalls[4], {
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
    url: "/api/v1/general/set-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: setBatchArgs,
  });
  assert.equal(setBatch.statusCode, 200);
  assert.deepEqual(mock.jsonCalls[5], {
    functionName: "set-batch!",
    args: setBatchArgs,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
  assert.deepEqual(setBatch.json(), { ok: true, mode: "json", function: "set-batch!" });
});

test("every batch operation accepts canonical Scheme payloads", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const cases = [
    ["get-batch", "get-batch", "((paths ((*state* alice a) (*state* alice missing))) (expression? #t))", true],
    ["set-batch", "set-batch!", "((paths ((*state* alice a))) (values (new)) (expected (old)) (expression? #t))", true],
    ["resolve-batch", "resolve-batch", "((paths ((-1 *state* alice a))) (pinned? #t) (expression? #t))", true],
    ["pin-batch", "pin-batch!", "((paths ((-1 *state* alice a))))", true],
    ["unpin-batch", "unpin-batch!", "((paths ((-1 *state* alice a))))", true],
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
    ["get-batch", { paths }],
    ["set-batch", { paths, values: paths, expected: paths, "expression?": true }],
    ["resolve-batch", { paths: committed }],
    ["pin-batch", { paths: committed }],
    ["unpin-batch", { paths: committed }],
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
    url: "/api/v1/general/get-batch",
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

test("removed general batch and copy routes remain absent", async (t) => {
  const app = await createApp({ allowAdminRoutes: false });
  t.after(async () => app.close());
  for (const operation of ["batch", "copy"]) {
    const response = await app.inject({
      method: "POST",
      url: `/api/v1/general/${operation}`,
      headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
      payload: {},
    });
    assert.equal(response.statusCode, 404, operation);
  }
});

test("Gateway restricts federation context to staged scalar and dedicated batch access", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const setResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/set",
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
    url: "/api/v1/general/get-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      paths: [["*state*", "docs"], ["*state*", "docs"]],
      "$federation": { route: ["bob"] },
    },
  });
  assert.equal(getBatchResult.statusCode, 200);
  assert.equal(mock.jsonCalls[1]?.functionName, "get-batch");
  assert.deepEqual(mock.jsonCalls[1]?.routeTarget, ["bob"]);
  assert.deepEqual(mock.jsonCalls[1]?.args, {
    paths: [["*state*", "docs"], ["*state*", "docs"]],
  });

  const setBatchResult = await app.inject({
    method: "POST",
    url: "/api/v1/general/set-batch",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      paths: [["*state*", "docs"], ["*state*", "docs"]],
      values: ["hello", "world"], expected: [false, ["nothing"]],
      "$federation": { route: ["bob"] },
    },
  });
  assert.equal(setBatchResult.statusCode, 200);
  assert.equal(mock.jsonCalls[2]?.functionName, "set-batch!");
  assert.deepEqual(mock.jsonCalls[2]?.routeTarget, ["bob"]);
  assert.deepEqual(mock.jsonCalls[2]?.args, {
    paths: [["*state*", "docs"], ["*state*", "docs"]],
    values: ["hello", "world"], expected: [false, ["nothing"]],
  });

  for (const operation of [
    "resolve", "resolve-batch", "trace-batch",
    "pin", "pin-batch", "unpin-batch", "call", "bridge", "config", "admins", "route",
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
    { operation: "get", context: { route: ["bob"], history: [-1, -1] } },
    { operation: "resolve", context: { route: [], history: [-1] } },
    { operation: "resolve", context: { route: ["bob"], history: [-1] } },
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

test("authorization JSON routes preserve key-index and Resolve as distinct exact fields", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());
  const rule = {
    principal: ["peer-a", "*state*", "bob"],
    "key-index": [-32, -1],
    path: ["docs"], get: true, "set!": false, resolve: [0, -1],
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

test("authorization Scheme routes preserve exact key-index and Resolve expressions", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());
  const rule = "((principal (peer-a *state* bob)) (key-index (-32 -1)) (path (docs)) (get #t) (set! #f) (resolve (0 -1)))";
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
    /\(authentication \(\(identity \(\*state\* test-user-id\)\) \(credentials "test-journal-secret"\)\)\)/
  );
});

test("POST /api/v1/general/call forwards a staged program path and arguments", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/call",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: {
      path: ["*state*", "test-user-id", "programs", "echo"],
      arguments: [1, "two"],
    },
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.equal(mock.jsonCalls[0].functionName, "call!");
  assert.deepEqual(mock.jsonCalls[0].args, {
    path: ["*state*", "test-user-id", "programs", "echo"],
    arguments: [1, "two"],
  });
  assert.equal(mock.jsonCalls[0].authentication, "test-journal-secret");
  assert.equal(mock.jsonCalls[0].identityId, "test-user-id");
});

test("POST /api/v1/general/call rejects a nested arguments wrapper", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/call",
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

test("POST /api/v1/general/call rejects transport authority fields", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  for (const reserved of ["function", "authentication"] as const) {
    const res = await app.inject({
      method: "POST",
      url: "/api/v1/general/call",
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

  const args = { path: ["*state*", "mykey"], value: "myvalue", expected: false };
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

  type SchemaWithExample = { example?: unknown; description?: string; type?: unknown };
  type MediaType = { schema?: SchemaWithExample };
  type Operation = { description?: string; requestBody?: { content?: Record<string, MediaType> } };
  type PathItem = Record<string, Operation>;
  const paths = (app.swagger() as { paths: Record<string, PathItem> }).paths;

  const schemaExample = (path: string) =>
    paths[path]?.post?.requestBody?.content?.["application/json"]?.schema?.example;

  assert.deepEqual(schemaExample("/api/v1/general/get"), { path: ["*state*", "mykey"], "expression?": true });
  assert.deepEqual(schemaExample("/api/v1/general/get-batch"), {
    paths: [["*state*", "a"], ["*state*", "b"]], "expression?": true,
  });
  assert.deepEqual(schemaExample("/api/v1/general/set-batch"), {
    paths: [["*state*", "mykey"]], values: ["myvalue"], expected: ["oldvalue"], "expression?": true,
  });
  assert.deepEqual(schemaExample("/api/v1/general/trace"), {
    index: 0, path: ["*state*", "mykey"],
  });
  assert.deepEqual(schemaExample("/api/v1/general/resolve-batch"), {
    paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]],
    "pinned?": true, "expression?": true,
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
  assert.deepEqual(schemaExample("/api/v1/general/bridge"), {
    name: "peer-a",
    interface: "http://peer-a/interface",
    "remote-name": "my-journal",
  });
  assert.deepEqual(schemaExample("/api/v1/general/admins"), {});
  assert.deepEqual(schemaExample("/api/v1/general/set-admins"), { admins: [["*state*", "admin"], ["*state*", "alice"]] });
  assert.deepEqual(schemaExample("/api/v1/general/set-window"), { value: 128 });
  assert.deepEqual(schemaExample("/api/v1/general/call"), {
    path: ["*state*", "alice", "programs", "example"], arguments: [],
  });
  const exactAuthorization = {
    user: ["*state*", "alice"],
    rule: {
      principal: ["peer-a", "*state*", "bob"],
      "key-index": [-32, -1], path: ["docs"], get: true, "set!": false, resolve: [0, -1],
    },
  };
  assert.deepEqual(schemaExample("/api/v1/general/authorize"), exactAuthorization);
  assert.deepEqual(schemaExample("/api/v1/general/deauthorize"), exactAuthorization);
  const setBatchOperation = paths["/api/v1/general/set-batch"]?.post;
  assert.match(setBatchOperation?.description ?? "", /one staged snapshot/);
  assert.match(setBatchOperation?.description ?? "", /atomically in request order/);
  const authorizationOperation = paths["/api/v1/general/authorize"]?.post;
  assert.match(authorizationOperation?.description ?? "", /Self-local/);
  assert.match(authorizationOperation?.description ?? "", /terminal-local committed bridge-state authentication window/);
  assert.match(authorizationOperation?.description ?? "", /document-history indexes/);
  const authorizationSchema = authorizationOperation?.requestBody?.content?.["application/json"]?.schema;
  assert.deepEqual(authorizationSchema?.type, ["array", "object"]);
  assert.match(authorizationSchema?.description ?? "", /schema stays permissive/);
  assert.deepEqual(schemaExample("/api/v1/root/step"), []);
  assert.deepEqual(schemaExample("/api/v1/root/eval"), [["+", 1, 2]]);

  const schemeExample = (path: string) =>
    paths[path]?.post?.requestBody?.content?.["text/plain"]?.schema?.example;

  assert.equal(schemeExample("/api/v1/general/get"), "((path (*state* mykey)))");
  assert.equal(schemeExample("/api/v1/general/get-batch"),
    "((paths ((*state* a) (*state* b))))");
  assert.equal(schemeExample("/api/v1/general/set-batch"),
    "((paths ((*state* mykey))) (values (myvalue)) (expected (oldvalue)) (expression? #t))");
  assert.equal(schemeExample("/api/v1/general/trace"),
    "((index 0) (path (*state* mykey)))");
  assert.equal(schemeExample("/api/v1/general/resolve-batch"),
    "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))) (pinned? #t))");
  assert.equal(schemeExample("/api/v1/general/trace-batch"),
    "((index 0) (paths ((*state* a) (*state* b))))");
  assert.equal(schemeExample("/api/v1/general/pin-batch"),
    "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))");
  assert.equal(schemeExample("/api/v1/general/unpin-batch"),
    "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))");
  assert.equal(schemeExample("/api/v1/general/bridge"), '((name peer-a) (interface "http://peer-a/interface") (remote-name my-journal))');
  assert.equal(schemeExample("/api/v1/general/admins"), "()");
  assert.equal(schemeExample("/api/v1/general/set-admins"), "((admins ((*state* admin) (*state* alice))))");
  assert.equal(schemeExample("/api/v1/general/set-window"), "((value 128))");
  assert.equal(schemeExample("/api/v1/general/call"), "((path (*state* alice programs example)) (arguments ()))");
  const exactSchemeAuthorization = "((user (*state* alice)) (rule ((principal (peer-a *state* bob)) (key-index (-32 -1)) (path (docs)) (get #t) (set! #f) (resolve (0 -1)))))";
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
