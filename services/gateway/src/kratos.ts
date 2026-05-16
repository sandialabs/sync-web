import { FrontendApi, IdentityApi, Configuration } from "@ory/client-fetch";

export interface ApiTokenEntry {
  hash: string;
  description: string;
  created_at: string;
}

export interface KratosAdminIdentity {
  id: string;
  traits: { username: string };
  metadata_admin: { api_tokens?: Record<string, ApiTokenEntry> } | null;
}

export interface KratosClient {
  whoami(cookie: string): Promise<{ identity: { id: string; traits: { username: string } } }>;
  whoamiWithSessionToken(sessionToken: string): Promise<{ identity: { id: string; traits: { username: string } } }>;
  getIdentityById(uuid: string): Promise<KratosAdminIdentity>;
  patchIdentityApiTokens(uuid: string, apiTokens: Record<string, ApiTokenEntry>): Promise<void>;
}

function resolveSession(session: { identity?: { id: string; traits?: unknown } | null }) {
  if (!session.identity) throw new Error("Session has no identity");
  const username = (session.identity.traits as Record<string, unknown>)?.username;
  if (typeof username !== "string" || !username) throw new Error("Session identity missing username trait");
  return { identity: { id: session.identity.id, traits: { username } } };
}

export function createKratosClient(publicUrl: string, adminUrl: string): KratosClient {
  const frontendApi = new FrontendApi(new Configuration({ basePath: publicUrl }));
  const identityApi = new IdentityApi(new Configuration({ basePath: adminUrl }));

  return {
    async whoami(cookie: string) {
      return resolveSession(await frontendApi.toSession({ cookie }));
    },

    async whoamiWithSessionToken(sessionToken: string) {
      return resolveSession(await frontendApi.toSession({ xSessionToken: sessionToken }));
    },

    async getIdentityById(uuid: string): Promise<KratosAdminIdentity> {
      const identity = await identityApi.getIdentity({ id: uuid });
      const username = (identity.traits as Record<string, unknown>)?.username;
      if (typeof username !== "string") throw new Error("Identity missing username trait");
      const metadataAdmin = (identity.metadata_admin ?? null) as { api_tokens?: Record<string, ApiTokenEntry> } | null;
      return {
        id: identity.id,
        traits: { username },
        metadata_admin: metadataAdmin,
      };
    },

    async patchIdentityApiTokens(uuid: string, apiTokens: Record<string, ApiTokenEntry>): Promise<void> {
      const identity = await identityApi.getIdentity({ id: uuid });
      const existing = (identity.metadata_admin ?? {}) as Record<string, unknown>;
      const newMetadata = { ...existing, api_tokens: apiTokens };
      await identityApi.patchIdentity({
        id: uuid,
        jsonPatch: [{ op: "replace", path: "/metadata_admin", value: newMetadata }],
      });
    },
  };
}
