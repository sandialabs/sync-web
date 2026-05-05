import { FrontendApi, Configuration } from "@ory/client-fetch";

export interface KratosClient {
  whoami(cookie: string): Promise<{ identity: { id: string } }>;
}

export function createKratosClient(baseUrl: string): KratosClient {
  const api = new FrontendApi(new Configuration({ basePath: baseUrl }));
  return {
    async whoami(cookie: string) {
      const session = await api.toSession({ cookie });
      if (!session.identity) throw new Error("Session has no identity");
      return { identity: { id: session.identity.id } };
    },
  };
}
