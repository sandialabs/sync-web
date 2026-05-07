import { FrontendApi, Configuration } from "@ory/client-fetch";

export interface KratosSessionOptions {
  cookie?: string;
  xSessionToken?: string;
}

export interface KratosClient {
  whoami(opts: KratosSessionOptions): Promise<{ identity: { id: string } }>;
}

export function createKratosClient(baseUrl: string): KratosClient {
  const api = new FrontendApi(new Configuration({ basePath: baseUrl }));
  return {
    async whoami(opts: KratosSessionOptions) {
      const session = await api.toSession(opts);
      if (!session.identity) throw new Error("Session has no identity");
      return { identity: { id: session.identity.id } };
    },
  };
}
