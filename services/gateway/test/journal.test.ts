import assert from "node:assert/strict";
import { test } from "node:test";
import type { FastifyBaseLogger } from "fastify";
import { createJournalClient, JournalSemanticError, redactAuth } from "../src/journal";

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

test("callJson sends username symbol when identityId provided", async (t) => {
  let captured: string | undefined;
  t.mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
    captured = init.body as string;
    return respondWith("ok");
  });
  await makeClient().callJson({ functionName: "get", authentication: "secret", identityId: "alice" });
  const body = JSON.parse(captured!);
  assert.equal(body.authentication.identity, "alice");
  assert.deepEqual(body.authentication.credentials, { "*type/string*": "secret" });
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

// --- redactAuth ---

test("redactAuth masks credentials in new envelope shape", () => {
  const body = {
    function: "get",
    authentication: { identity: "alice", credentials: { "*type/string*": "secret" } },
  };
  const result = redactAuth(body);
  assert.deepEqual((result.authentication as Record<string, unknown>).credentials, "***REDACTED***");
  assert.equal((result.authentication as Record<string, unknown>).identity, "alice");
  assert.equal((body.authentication as Record<string, unknown>).credentials["*type/string*"], "secret");
});

test("redactAuth masks whole authentication when no credentials key", () => {
  const body = { function: "size", authentication: "plain-secret" };
  const result = redactAuth(body);
  assert.equal(result.authentication, "***REDACTED***");
});

test("redactAuth is a no-op when no authentication field", () => {
  const body = { function: "size" };
  const result = redactAuth(body);
  assert.deepEqual(result, body);
});
