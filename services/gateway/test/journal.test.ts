import assert from "node:assert/strict";
import { test } from "node:test";
import type { FastifyBaseLogger } from "fastify";
import { createJournalClient, JournalSemanticError } from "../src/journal";

const noop = () => {};
const logger = {
  info: noop, warn: noop, error: noop, debug: noop, trace: noop, fatal: noop,
  child: () => logger,
} as unknown as FastifyBaseLogger;

const JOURNAL_EP = "http://journal.test/interface";
const ROOT_EP = "http://journal.test/root/interface";

const makeClient = (timeoutMs = 5000) =>
  createJournalClient(JOURNAL_EP, ROOT_EP, timeoutMs, logger);

const respondWith = (body: unknown, status = 200) =>
  Promise.resolve({
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  } as Response);

// --- callJson auth envelope ---

test("callJson omits identity when no identityId provided", async (t) => {
  let captured: string | undefined;
  t.mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
    captured = init.body as string;
    return respondWith("ok");
  });
  await makeClient().callJson({ functionName: "get", authentication: "secret" });
  const body = JSON.parse(captured!);
  assert.equal("identity" in body.authentication, false);
  assert.deepEqual(body.authentication.credentials, { "*type/string*": "secret" });
});

test("callJson sends local principal path when identityId provided", async (t) => {
  let captured: string | undefined;
  t.mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
    captured = init.body as string;
    return respondWith("ok");
  });
  await makeClient().callJson({ functionName: "get", authentication: "secret", identityId: "alice" });
  const body = JSON.parse(captured!);
  assert.deepEqual(body.authentication.identity, ["*state*", "alice"]);
  assert.deepEqual(body.authentication.credentials, { "*type/string*": "secret" });
});

test("callJson builds a federated invocation with optional Ledger indexes", async (t) => {
  let captured: string | undefined;
  t.mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
    captured = init.body as string;
    return respondWith("ok");
  });
  await makeClient().callJson({
    functionName: "resolve",
    authentication: "secret",
    identityId: "alice",
    routeTarget: ["carol", "bob"],
    historyIndexes: [-1, 3, 7],
  });
  const body = JSON.parse(captured!);
  assert.equal("authentication" in body, false);
  assert.deepEqual(body.invocation, {
    identity: "alice",
    "route-source": [],
    "route-target": ["carol", "bob"],
    "history-indexes": [-1, 3, 7],
    credentials: { "*type/string*": "secret" },
  });
});

test("callJson omits authentication block when no authentication provided", async (t) => {
  let captured: string | undefined;
  t.mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
    captured = init.body as string;
    return respondWith("ok");
  });
  await makeClient().callJson({ functionName: "size" });
  const body = JSON.parse(captured!);
  assert.equal("authentication" in body, false);
});

// --- callJson error handling ---

test("callJson throws JournalSemanticError on semantic error response", async (t) => {
  t.mock.method(globalThis, "fetch", async () =>
    respondWith([
      "error",
      { "*type/quoted*": "authentication-error" },
      { "*type/string*": "Could not authenticate" },
    ])
  );
  await assert.rejects(
    () => makeClient().callJson({ functionName: "get" }),
    (err: unknown) => err instanceof JournalSemanticError && err.code === "authentication-error"
  );
});

test("callJson throws on request timeout", async (t) => {
  t.mock.method(globalThis, "fetch", (_url: string, init: RequestInit) =>
    new Promise<Response>((_resolve, reject) => {
      (init.signal as AbortSignal).addEventListener("abort", () => {
        reject(new DOMException("The operation was aborted", "AbortError"));
      });
    })
  );
  await assert.rejects(
    () => makeClient(50).callJson({ functionName: "get" }),
    /Failed to call journal/
  );
});

test("forwarding diagnostics omit request, response, and credential bodies", async (t) => {
  const entries: unknown[][] = [];
  const recordingLogger = {
    info: (...args: unknown[]) => entries.push(args),
    warn: (...args: unknown[]) => entries.push(args),
    error: (...args: unknown[]) => entries.push(args),
    debug: noop, trace: noop, fatal: noop,
    child: () => recordingLogger,
  } as unknown as FastifyBaseLogger;
  const client = createJournalClient(JOURNAL_EP, ROOT_EP, 5000, recordingLogger, {
    debugForwarding: true,
  });
  t.mock.method(globalThis, "fetch", async () => respondWith("RESPONSE-LEAK-SENTINEL"));

  await client.callJson({
    functionName: "set!",
    args: { value: "BODY-LEAK-SENTINEL" },
    authentication: "INTERFACE-LEAK-SENTINEL",
  });
  await client.callScheme({
    functionName: "set!",
    expression: '(set! "SCHEME-LEAK-SENTINEL")',
  });
  await client.callRootJson({
    functionName: "*set-secret*",
    args: ["NEW-ROOT-LEAK-SENTINEL"],
    authentication: "ROOT-LEAK-SENTINEL",
  });

  const logged = JSON.stringify(entries);
  for (const sentinel of [
    "RESPONSE-LEAK-SENTINEL",
    "BODY-LEAK-SENTINEL",
    "INTERFACE-LEAK-SENTINEL",
    "SCHEME-LEAK-SENTINEL",
    "NEW-ROOT-LEAK-SENTINEL",
    "ROOT-LEAK-SENTINEL",
  ]) {
    assert.equal(logged.includes(sentinel), false, `logged ${sentinel}`);
  }
});
