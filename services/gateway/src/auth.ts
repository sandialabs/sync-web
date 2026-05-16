import { createHash } from "node:crypto";
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
  identityId?: string;
  kratosId: string;
}

const toUuidFormat = (hex: string): string =>
  `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;

export const resolveSessionIdentity = async (
  request: FastifyRequest,
  journalSecret: string,
  kratos: KratosClient
): Promise<ResolvedIdentity> => {
  const sessionToken = request.headers["x-session-token"];
  if (typeof sessionToken === "string" && sessionToken) {
    try {
      const session = await kratos.whoamiWithSessionToken(sessionToken);
      return { journalSecret, identityId: session.identity.traits.username, kratosId: session.identity.id };
    } catch {
      throw new UnauthorizedError();
    }
  }
  const cookie = request.headers.cookie ?? "";
  if (!cookie.includes("ory_kratos_session")) throw new UnauthorizedError();
  try {
    const session = await kratos.whoami(cookie);
    return { journalSecret, identityId: session.identity.traits.username, kratosId: session.identity.id };
  } catch {
    throw new UnauthorizedError();
  }
};

export const resolveIdentity = async (
  request: FastifyRequest,
  journalSecret: string,
  kratos: KratosClient
): Promise<ResolvedIdentity> => {
  const authHeader = request.headers.authorization;
  if (authHeader?.startsWith("Bearer ")) {
    const token = authHeader.slice(7);
    const parts = token.split("-");
    if (parts.length !== 5 || parts[0] !== "sync") throw new UnauthorizedError();
    const [, uuidHex, tokenId, version, secret] = parts;
    if (!/^[0-9a-f]{32}$/i.test(uuidHex)) throw new UnauthorizedError();
    if (version !== "0") throw new UnauthorizedError();
    try {
      const uuid = toUuidFormat(uuidHex);
      const identity = await kratos.getIdentityById(uuid);
      const tokenEntry = identity.metadata_admin?.api_tokens?.[tokenId];
      if (!tokenEntry) throw new UnauthorizedError();
      const hash = createHash("sha256").update(secret).digest("hex");
      if (hash !== tokenEntry.hash) throw new UnauthorizedError();
      return { journalSecret, identityId: identity.traits.username, kratosId: identity.id };
    } catch (e) {
      if (e instanceof UnauthorizedError) throw e;
      throw new UnauthorizedError();
    }
  }

  const cookie = request.headers.cookie ?? "";
  if (!cookie.includes("ory_kratos_session")) throw new UnauthorizedError();
  try {
    const session = await kratos.whoami(cookie);
    return {
      journalSecret,
      identityId: session.identity.traits.username,
      kratosId: session.identity.id,
    };
  } catch {
    throw new UnauthorizedError();
  }
};
