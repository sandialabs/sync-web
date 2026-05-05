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
  const cookie = request.headers.cookie ?? "";
  if (!cookie.includes("ory_kratos_session")) throw new UnauthorizedError();
  try {
    const session = await kratos.whoami(cookie);
    return { journalSecret, identityId: session.identity.id };
  } catch {
    throw new UnauthorizedError();
  }
};
