import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "node:test";
import type { FastifyRequest } from "fastify";
import { resolveIdentity, resolveSessionIdentity, UnauthorizedError } from "../src/auth";
import type { ApiTokenEntry, KratosAdminIdentity, KratosClient } from "../src/kratos";

const JOURNAL_SECRET = "test-journal-secret";
const IDENTITY_ID = "test-identity-id";
const KRATOS_UUID = "a3f8c201-b4d2-e9f0-a1b2-c3d4e5f6a7b8";
const KRATOS_UUID_HEX = "a3f8c201b4d2e9f0a1b2c3d4e5f6a7b8";
const KEY_ID = "3f8a2c1d";
const SECRET = "9b4e2a1c0d3f8e7b6a5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3";
const SECRET_HASH = createHash("sha256").update(SECRET).digest("hex");

const req = (headers: Record<string, string | undefined>): FastifyRequest =>
  ({ headers } as unknown as FastifyRequest);

const makeApiTokenEntry = (overrides: Partial<ApiTokenEntry> = {}): ApiTokenEntry => ({
  hash: SECRET_HASH,
  description: "test token",
  created_at: "2026-05-14T00:00:00Z",
  ...overrides,
});

const makeAdminIdentity = (apiTokens: Record<string, ApiTokenEntry> = {}): KratosAdminIdentity => ({
  id: KRATOS_UUID,
  traits: { username: IDENTITY_ID },
  metadata_admin: { api_tokens: apiTokens },
});

const mockKratos = (identityId: string): KratosClient => ({
  async whoami(_cookie) {
    return { identity: { id: KRATOS_UUID, traits: { username: identityId } } };
  },
  async whoamiWithSessionToken(_token) {
    return { identity: { id: KRATOS_UUID, traits: { username: identityId } } };
  },
  async getIdentityById(_uuid) {
    return makeAdminIdentity({ [KEY_ID]: makeApiTokenEntry() });
  },
  async patchIdentityApiTokens(_uuid, _apiTokens) {},
});

const failingKratos: KratosClient = {
  async whoami(_cookie) {
    throw new Error("session invalid");
  },
  async whoamiWithSessionToken(_token) {
    throw new Error("session token invalid");
  },
  async getIdentityById(_uuid) {
    throw new Error("identity not found");
  },
  async patchIdentityApiTokens(_uuid, _apiTokens) {
    throw new Error("patch failed");
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
  assert.equal(result.kratosId, KRATOS_UUID);
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

test("resolves with undefined identityId when whoami returns no username", async () => {
  const noUsernameKratos: KratosClient = {
    async whoami(_cookie) {
      return { identity: { id: KRATOS_UUID, traits: { username: "" as unknown as string } } };
    },
    async whoamiWithSessionToken(_token) {
      return { identity: { id: KRATOS_UUID, traits: { username: "" as unknown as string } } };
    },
    async getIdentityById(_uuid) {
      return { id: KRATOS_UUID, traits: { username: "" as unknown as string }, metadata_admin: null };
    },
    async patchIdentityApiTokens(_uuid, _apiTokens) {},
  };
  const result = await resolveIdentity(
    req({ cookie: "ory_kratos_session=abc123" }),
    JOURNAL_SECRET,
    noUsernameKratos
  );
  assert.equal(result.journalSecret, JOURNAL_SECRET);
  assert.equal(result.identityId, "");
});

test("resolves identity from valid API token Bearer token", async () => {
  const token = `sync-${KRATOS_UUID_HEX}-${KEY_ID}-0-${SECRET}`;
  const result = await resolveIdentity(
    req({ authorization: `Bearer ${token}` }),
    JOURNAL_SECRET,
    mockKratos(IDENTITY_ID)
  );
  assert.equal(result.journalSecret, JOURNAL_SECRET);
  assert.equal(result.identityId, IDENTITY_ID);
  assert.equal(result.kratosId, KRATOS_UUID);
});

test("throws UnauthorizedError when API token Bearer token has wrong prefix", async () => {
  const token = `bad-${KRATOS_UUID_HEX}-${KEY_ID}-0-${SECRET}`;
  await assert.rejects(
    () => resolveIdentity(req({ authorization: `Bearer ${token}` }), JOURNAL_SECRET, mockKratos(IDENTITY_ID)),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when API token Bearer token has wrong part count", async () => {
  await assert.rejects(
    () => resolveIdentity(req({ authorization: "Bearer sync-abc123" }), JOURNAL_SECRET, mockKratos(IDENTITY_ID)),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when API token UUID hex is invalid", async () => {
  const token = `sync-notuuid-${KEY_ID}-0-${SECRET}`;
  await assert.rejects(
    () => resolveIdentity(req({ authorization: `Bearer ${token}` }), JOURNAL_SECRET, mockKratos(IDENTITY_ID)),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when API token version is unsupported", async () => {
  const token = `sync-${KRATOS_UUID_HEX}-${KEY_ID}-1-${SECRET}`;
  await assert.rejects(
    () => resolveIdentity(req({ authorization: `Bearer ${token}` }), JOURNAL_SECRET, mockKratos(IDENTITY_ID)),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when API token id not found in metadata_admin", async () => {
  const kratosNoToken: KratosClient = {
    async whoami(_cookie) {
      return { identity: { id: KRATOS_UUID, traits: { username: IDENTITY_ID } } };
    },
    async whoamiWithSessionToken(_token) {
      return { identity: { id: KRATOS_UUID, traits: { username: IDENTITY_ID } } };
    },
    async getIdentityById(_uuid) {
      return makeAdminIdentity({});
    },
    async patchIdentityApiTokens(_uuid, _apiTokens) {},
  };
  const token = `sync-${KRATOS_UUID_HEX}-${KEY_ID}-0-${SECRET}`;
  await assert.rejects(
    () => resolveIdentity(req({ authorization: `Bearer ${token}` }), JOURNAL_SECRET, kratosNoToken),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when API token secret hash does not match", async () => {
  const kratosWrongHash: KratosClient = {
    async whoami(_cookie) {
      return { identity: { id: KRATOS_UUID, traits: { username: IDENTITY_ID } } };
    },
    async whoamiWithSessionToken(_token) {
      return { identity: { id: KRATOS_UUID, traits: { username: IDENTITY_ID } } };
    },
    async getIdentityById(_uuid) {
      return makeAdminIdentity({ [KEY_ID]: makeApiTokenEntry({ hash: "deadbeef" }) });
    },
    async patchIdentityApiTokens(_uuid, _apiTokens) {},
  };
  const token = `sync-${KRATOS_UUID_HEX}-${KEY_ID}-0-${SECRET}`;
  await assert.rejects(
    () => resolveIdentity(req({ authorization: `Bearer ${token}` }), JOURNAL_SECRET, kratosWrongHash),
    UnauthorizedError
  );
});

test("throws UnauthorizedError when API token identity lookup throws", async () => {
  const token = `sync-${KRATOS_UUID_HEX}-${KEY_ID}-0-${SECRET}`;
  await assert.rejects(
    () => resolveIdentity(req({ authorization: `Bearer ${token}` }), JOURNAL_SECRET, failingKratos),
    UnauthorizedError
  );
});

test("resolveSessionIdentity resolves from valid Kratos session cookie", async () => {
  const result = await resolveSessionIdentity(
    req({ cookie: "ory_kratos_session=abc123" }),
    JOURNAL_SECRET,
    mockKratos(IDENTITY_ID)
  );
  assert.equal(result.journalSecret, JOURNAL_SECRET);
  assert.equal(result.identityId, IDENTITY_ID);
  assert.equal(result.kratosId, KRATOS_UUID);
});

test("resolveSessionIdentity resolves from valid X-Session-Token header", async () => {
  const result = await resolveSessionIdentity(
    req({ "x-session-token": "kratos-session-token-abc" }),
    JOURNAL_SECRET,
    mockKratos(IDENTITY_ID)
  );
  assert.equal(result.journalSecret, JOURNAL_SECRET);
  assert.equal(result.identityId, IDENTITY_ID);
  assert.equal(result.kratosId, KRATOS_UUID);
});

test("resolveSessionIdentity throws UnauthorizedError for Bearer API token", async () => {
  const token = `sync-${KRATOS_UUID_HEX}-${KEY_ID}-0-${SECRET}`;
  await assert.rejects(
    () => resolveSessionIdentity(
      req({ authorization: `Bearer ${token}` }),
      JOURNAL_SECRET,
      mockKratos(IDENTITY_ID)
    ),
    UnauthorizedError
  );
});

test("resolveSessionIdentity throws UnauthorizedError when X-Session-Token is invalid", async () => {
  await assert.rejects(
    () => resolveSessionIdentity(
      req({ "x-session-token": "invalid-token" }),
      JOURNAL_SECRET,
      failingKratos
    ),
    UnauthorizedError
  );
});

test("resolveSessionIdentity throws UnauthorizedError when neither cookie nor token present", async () => {
  await assert.rejects(
    () => resolveSessionIdentity(req({}), JOURNAL_SECRET, failingKratos),
    UnauthorizedError
  );
});
