import type { FastifyRequest } from "fastify";
import type { KratosClient } from "./kratos";

export class UnauthorizedError extends Error {
  constructor() {
    super("Unauthorized");
    this.name = "UnauthorizedError";
  }
}

export interface ResolvedIdentity {
  journalSecret: string;
  identityId: string;
}

export const resolveIdentity = async (
  request: FastifyRequest,
  journalSecret: string,
  kratos: KratosClient
): Promise<ResolvedIdentity> => {
  const authHeader = request.headers["authorization"] as string | undefined;
  if (authHeader?.startsWith("Bearer ") && authHeader.slice(7) === journalSecret) {
    return { journalSecret, identityId: "system" };
  }
  const sessionToken = request.headers["x-session-token"] as string | undefined;
  const cookie = request.headers.cookie ?? "";
  if (!sessionToken && !cookie.includes("ory_kratos_session")) throw new UnauthorizedError();
  try {
    const kratosOpts = sessionToken ? { xSessionToken: sessionToken } : { cookie };
    const session = await kratos.whoami(kratosOpts);
    return { journalSecret, identityId: session.identity.traits.username };
  } catch {
    throw new UnauthorizedError();
  }
};
