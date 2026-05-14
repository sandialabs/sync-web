import assert from "node:assert/strict";
import { test } from "node:test";
import Fastify from "fastify";
import fastifySwagger from "@fastify/swagger";
import { gatewayRoutes } from "../src/routes";
import { JournalSemanticError } from "../src/journal";
import type { JournalCall, JournalClient } from "../src/journal";
import type { KratosClient } from "../src/kratos";

const JOURNAL_SECRET = "test-journal-secret";
const IDENTITY_ID = "test-user-id";
const SESSION_COOKIE = "ory_kratos_session=test-session-token";

interface MockJournal {
  client: JournalClient;
  jsonCalls: JournalCall[];
  schemeCalls: Array<{ expression: string; functionName: string }>;
  proxiedJsonBodies: unknown[];
}

const createMockJournal = (): MockJournal => {
  const jsonCalls: JournalCall[] = [];
  const schemeCalls: Array<{ expression: string; functionName: string }> = [];
  const proxiedJsonBodies: unknown[] = [];

  return {
    jsonCalls,
    schemeCalls,
    proxiedJsonBodies,
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
    },
  };
};

const createMockKratos = (identityId = IDENTITY_ID): KratosClient => ({
  async whoami(_opts) {
    return { identity: { id: identityId, traits: { username: identityId } } };
  },
});

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

test("restricted route returns 401 when Kratos session is invalid", async (t) => {
  const failingKratos: KratosClient = {
    async whoami() { throw new Error("session invalid"); },
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

test("POST /api/v1/general/get accepts JSON keyword-object payload with Kratos session", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: [["*state*", "docs"]] };
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

test("POST /api/v1/general/get accepts legacy JSON array payload", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = [[["path", [["*state*", "docs"]]]]];
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
    payload: "(((path ((*state* docs)))))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].functionName, "get");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function get\) /);
  assert.match(
    mock.schemeCalls[0].expression,
    /\(arguments \(\(\(path \(\(\*state\* docs\)\)\)\)\)\)/
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
      { function: "get", arguments: { path: [["*state*", "docs"]] } },
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
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "batch!",
    args,
    authentication: JOURNAL_SECRET,
    identityId: IDENTITY_ID,
  });
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
      "(((queries (((function get) (arguments ((path ((*state* docs))))) ((function config))))))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].functionName, "batch!");
  assert.match(mock.schemeCalls[0].expression, /^\(\(function batch!\) /);
  assert.ok(mock.schemeCalls[0].expression.includes("(queries "));
  assert.ok(
    mock.schemeCalls[0].expression.includes(
      "((function get) (arguments ((path ((*state* docs)))))"
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
  };

  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: [["*state*", "docs"]], "pinned?": true, "proof?": true },
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
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].expression, "((function synchronize) (arguments ((index 0))))");
  assert.equal(mock.schemeCalls[0].functionName, "interface");
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
  assert.equal(mock.schemeCalls.length, 1);
  assert.equal(mock.schemeCalls[0].expression, "((function info))");
  assert.equal(mock.schemeCalls[0].functionName, "interface");
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

test("bearer token auth uses *journal* identity in JSON mode", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: [["*state*", "docs"]] };
  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { authorization: `Bearer ${JOURNAL_SECRET}`, "content-type": "application/json" },
    payload: args,
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.jsonCalls.length, 1);
  assert.deepEqual(mock.jsonCalls[0], {
    functionName: "get",
    args,
    authentication: JOURNAL_SECRET,
    identityId: undefined,
  });
});

test("bearer token auth uses *journal* identity in Scheme mode", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { authorization: `Bearer ${JOURNAL_SECRET}`, "content-type": "text/plain" },
    payload: "(((path ((*state* docs)))))",
  });

  assert.equal(res.statusCode, 200);
  assert.equal(mock.schemeCalls.length, 1);
  assert.match(
    mock.schemeCalls[0].expression,
    /\(authentication \(\(identity \*journal\*\) \(credentials "test-journal-secret"\)\)\)/
  );
});

test("non-semantic journal error becomes 502", async (t) => {
  const journal: JournalClient = {
    async callJson(): Promise<unknown> { throw new Error("connection refused"); },
    async callScheme(): Promise<unknown> { return {}; },
    async callRootJson(): Promise<unknown> { return {}; },
    async callRootScheme(): Promise<unknown> { return {}; },
    async proxyJson(): Promise<unknown> { return {}; },
  };
  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/get",
    headers: { cookie: SESSION_COOKIE, "content-type": "application/json" },
    payload: { path: [["*state*", "docs"]] },
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
  };
  const app = await createApp({ allowAdminRoutes: false, journal });
  t.after(async () => app.close());

  const res = await app.inject({
    method: "POST",
    url: "/api/v1/general/set",
    headers: { cookie: SESSION_COOKIE, "content-type": "text/plain" },
    payload: "((path ((*state* alice foo))) (value bar))",
  });

  assert.equal(res.statusCode, 400);
  assert.equal(res.json().error, "permissions-error");
  assert.equal(res.json().source, "journal");
});

test("POST /api/v1/general/set forwards with auth in JSON mode", async (t) => {
  const mock = createMockJournal();
  const app = await createApp({ allowAdminRoutes: false, journal: mock.client });
  t.after(async () => app.close());

  const args = { path: [["*state*", "mykey"]], value: "myvalue" };
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

  assert.deepEqual(schemaExample("/api/v1/general/get"), { path: [["*state*", "mykey"]] });
  assert.deepEqual(schemaExample("/api/v1/general/bridge"), { name: "peer-a", interface: "http://peer-a/interface" });
  assert.deepEqual(schemaExample("/api/v1/general/batch"), {
    queries: [{ function: "get", arguments: { path: [["*state*", "mykey"]] } }, { function: "config" }],
  });
  assert.deepEqual(schemaExample("/api/v1/root/step"), []);
  assert.deepEqual(schemaExample("/api/v1/root/eval"), [["+", 1, 2]]);

  const schemeExample = (path: string) =>
    paths[path]?.post?.requestBody?.content?.["text/plain"]?.schema?.example;

  assert.equal(schemeExample("/api/v1/general/get"), "((path ((*state* mykey))))");
  assert.equal(schemeExample("/api/v1/general/bridge"), '((name peer-a) (interface "http://peer-a/interface"))');
  assert.equal(schemeExample("/api/v1/root/eval"), "(+ 1 2)");
  assert.equal(schemeExample("/api/v1/root/set-secret"), '"new-admin-secret"');
  assert.equal(schemeExample("/api/v1/root/eval"), "(+ 1 2)");
});
