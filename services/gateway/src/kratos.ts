import { FrontendApi, Configuration } from "@ory/client-fetch";

export interface KratosSessionOptions {
  cookie?: string;
  xSessionToken?: string;
}

export interface KratosClient {
  whoami(opts: KratosSessionOptions): Promise<{ identity: { id: string; traits: { username: string } } }>;
}

export function createKratosClient(baseUrl: string): KratosClient {
  const api = new FrontendApi(new Configuration({ basePath: baseUrl }));
  return {
    async whoami(opts: KratosSessionOptions) {
      const session = await api.toSession(opts);
      if (!session.identity) throw new Error("Session has no identity");
      const username = (session.identity.traits as Record<string, unknown>)?.username;
      if (typeof username !== "string" || !username) throw new Error("Session identity missing username trait");
      return { identity: { id: session.identity.id, traits: { username } } };
    },
  };
}
