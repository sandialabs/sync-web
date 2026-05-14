import assert from "node:assert/strict";
import { test } from "node:test";
import type { FastifyRequest } from "fastify";
import { resolveIdentity, UnauthorizedError } from "../src/auth";
import type { KratosClient } from "../src/kratos";

const JOURNAL_SECRET = "test-journal-secret";
const IDENTITY_ID = "test-identity-id";

const req = (headers: Record<string, string | undefined>): FastifyRequest =>
  ({ headers } as unknown as FastifyRequest);

const mockKratos = (identityId: string): KratosClient => ({
  async whoami(_cookie) {
    return { identity: { id: identityId, traits: { username: identityId } } };
  },
});

const failingKratos: KratosClient = {
  async whoami(_cookie) {
    throw new Error("session invalid");
  },
};

test("resolves identity from valid Kratos session cookie", async () => {
  const result = await resolveIdentity(
    req({ cookie: "ory_kratos_session=abc123" }),
    JOURNAL_SECRET,
    mockKratos(IDENTITY_ID)
  );
  assert.equal(result.journalSecret, JOURNAL_SECRET);
  assert.equal(result.identityId, IDENTITY_ID);
});

test("throws UnauthorizedError when Kratos whoami fails", async () => {
  await assert.rejects(
    () =>
      resolveIdentity(
        req({ cookie: "ory_kratos_session=expired" }),
        JOURNAL_SECRET,
        failingKratos
      ),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when no session cookie is present", async () => {
  await assert.rejects(
    () => resolveIdentity(req({}), JOURNAL_SECRET, failingKratos),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when cookie exists but lacks ory_kratos_session", async () => {
  await assert.rejects(
    () =>
      resolveIdentity(
        req({ cookie: "some_other_cookie=value" }),
        JOURNAL_SECRET,
        failingKratos
      ),
    UnauthorizedError
  );
});


test("resolves identity from valid Authorization Bearer journal secret", async () => {
  const result = await resolveIdentity(
    req({ authorization: `Bearer ${JOURNAL_SECRET}` }),
    JOURNAL_SECRET,
    failingKratos
  );
  assert.equal(result.journalSecret, JOURNAL_SECRET);
  assert.equal(result.identityId, undefined);
});

test("throws UnauthorizedError when Authorization Bearer is wrong secret", async () => {
  await assert.rejects(
    () =>
      resolveIdentity(
        req({ authorization: "Bearer wrong-secret" }),
        JOURNAL_SECRET,
        failingKratos
      ),
    UnauthorizedError
  );
});
