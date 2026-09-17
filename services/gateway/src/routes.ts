import { randomBytes, createHash } from "node:crypto";
import type { FastifyPluginAsync, FastifyReply, FastifyRequest } from "fastify";
import { resolveIdentity, resolveSessionIdentity, UnauthorizedError } from "./auth";
import type { ApiTokenEntry, KratosClient } from "./kratos";
import type { JournalClient } from "./journal";
import { JournalSemanticError } from "./journal";
import { GatewayEventBroker, isGatewayEventPath } from "./events";
import {
  RAW_RESPONSE_HEADERS,
  classifyRawBytes,
  decodeRawSelection,
  extractRawBytes,
} from "./raw";

export interface GatewayRoutesOptions {
  journal: JournalClient;
  allowAdminRoutes: boolean;
  journalSecret: string;
  kratos: KratosClient;
}

class GatewayRequestError extends Error {
  readonly statusCode: number;
  readonly code: "invalid_request" | "unsupported_media_type";

  constructor(code: "invalid_request" | "unsupported_media_type", message: string) {
    super(message);
    this.name = "GatewayRequestError";
    this.code = code;
    this.statusCode = code === "unsupported_media_type" ? 415 : 400;
  }
}

const invalidRequest = (message: string): GatewayRequestError =>
  new GatewayRequestError("invalid_request", message);

const unsupportedMediaType = (message: string): GatewayRequestError =>
  new GatewayRequestError("unsupported_media_type", message);

const getContentType = (request: FastifyRequest): string =>
  String(request.headers["content-type"] || "")
    .split(";")[0]
    .trim()
    .toLowerCase();

const isSchemeContentType = (contentType: string): boolean =>
  contentType === "text/plain" || contentType === "application/scheme";

const isDefaultSchemeProxyContentType = (contentType: string): boolean =>
  contentType === "" ||
  contentType === "application/octet-stream" ||
  contentType === "application/x-www-form-urlencoded";

const isJsonContentType = (contentType: string): boolean =>
  contentType === "application/json";

const SCHEME_FEDERATION_ROUTE_HEADER = "x-sync-web-federation-route";
const schemeFederationOperations = new Set(["put!", "use!", "retrieve"]);

const extractSchemeFederationRoute = (
  request: FastifyRequest,
  functionName: string,
  root: boolean,
  contentType: string,
): string[] | undefined => {
  const header = request.headers[SCHEME_FEDERATION_ROUTE_HEADER];
  if (header === undefined) return undefined;
  if (!isSchemeContentType(contentType)) {
    throw invalidRequest("X-Sync-Web-Federation-Route requires application/scheme");
  }
  if (contentType !== "application/scheme") {
    throw invalidRequest("X-Sync-Web-Federation-Route is not allowed with text/plain");
  }
  if (root || !schemeFederationOperations.has(functionName)) {
    throw invalidRequest(`X-Sync-Web-Federation-Route is not allowed for ${functionName}`);
  }
  if (typeof header !== "string") {
    throw invalidRequest("X-Sync-Web-Federation-Route must occur exactly once");
  }

  let route: unknown;
  try {
    route = JSON.parse(header);
  } catch {
    throw invalidRequest("X-Sync-Web-Federation-Route must be a JSON array of bridge names");
  }
  if (!Array.isArray(route) || route.length === 0
      || !route.every((name) => typeof name === "string" && !name.includes("\0"))) {
    throw invalidRequest(
      "X-Sync-Web-Federation-Route must be a nonempty JSON array of bridge names"
    );
  }
  return route;
};

const duplicateTopLevelJsonNames = Symbol("duplicateTopLevelJsonNames");

type ClassifiedJsonRequest = FastifyRequest & {
  [duplicateTopLevelJsonNames]?: boolean;
};

const skipJsonString = (source: string, start: number): number => {
  let index = start + 1;
  while (index < source.length) {
    if (source[index] === "\\") {
      index += 2;
    } else if (source[index] === '"') {
      return index + 1;
    } else {
      index += 1;
    }
  }
  return source.length;
};

const hasDuplicateTopLevelJsonNames = (source: string): boolean => {
  const skipWhitespace = (start: number): number => {
    let index = start;
    while (/\s/.test(source[index] ?? "")) index += 1;
    return index;
  };

  let index = skipWhitespace(0);
  if (source[index] !== "{") return false;
  index = skipWhitespace(index + 1);
  const names = new Set<string>();

  while (index < source.length && source[index] !== "}") {
    if (source[index] !== '"') return false;
    const nameStart = index;
    index = skipJsonString(source, index);
    const name = JSON.parse(source.slice(nameStart, index)) as string;
    if (names.has(name)) return true;
    names.add(name);

    index = skipWhitespace(index);
    if (source[index] !== ":") return false;
    index = skipWhitespace(index + 1);
    let objectDepth = 0;
    let arrayDepth = 0;
    while (index < source.length) {
      const character = source[index];
      if (character === '"') {
        index = skipJsonString(source, index);
        continue;
      }
      if (character === "{") objectDepth += 1;
      else if (character === "[") arrayDepth += 1;
      else if (character === "}" && objectDepth > 0) objectDepth -= 1;
      else if (character === "]") arrayDepth -= 1;
      else if ((character === "," || character === "}") && objectDepth === 0 && arrayDepth === 0) break;
      index += 1;
    }
    if (source[index] === ",") index = skipWhitespace(index + 1);
  }
  return false;
};

const escapeLispString = (value: string): string =>
  value.replace(/\\/g, "\\\\").replace(/"/g, '\\"');

const functionsWithArgumentsField = new Set([
  "use!", "use-batch!", "run!", "retrieve", "retrieve-batch",
]);

const extractJsonArguments = (
  body: unknown,
  allowArgumentsField = false,
): unknown => {
  if (body === undefined) {
    return undefined;
  }
  if (Array.isArray(body)) {
    return body;
  }
  if (!body || typeof body !== "object") {
    throw invalidRequest("JSON body must provide an argument object/array.");
  }
  const record = body as Record<string, unknown>;

  if ("function" in record || "authentication" in record) {
    throw invalidRequest("Gateway JSON bodies should provide only operation arguments.");
  }

  if ("arguments" in record) {
    if (!allowArgumentsField) {
      throw invalidRequest(
        "Gateway JSON bodies must provide operation arguments directly, not under an arguments wrapper."
      );
    }
    if (!Array.isArray(record.arguments)) {
      throw invalidRequest("The operation arguments field must be an array.");
    }
  }

  // Treat plain object bodies as direct keyword argument objects.
  return record;
};

const extractSchemeArguments = (body: unknown): string => {
  const expression = typeof body === "string"
    ? body
    : Buffer.isBuffer(body) ? body.toString("utf8") : null;
  if (expression === null) {
    throw invalidRequest("Scheme requests must provide plain text argument expression body");
  }
  if (expression.includes("\0")) {
    throw invalidRequest("Scheme requests cannot contain a null byte");
  }
  return expression;
};

export const buildSchemeArgumentsProjection = (argsExpression: string): string =>
  `'(gateway-use-arguments . ${argsExpression})`;

const isCompleteArgumentPairCollection = (
  entries: unknown[]
): entries is [string, unknown][] => {
  const names = new Set<string>();
  for (const entry of entries) {
    if (!Array.isArray(entry) || entry.length !== 2 || typeof entry[0] !== "string" ||
        names.has(entry[0])) {
      return false;
    }
    names.add(entry[0]);
  }
  return true;
};

const projectedSchemeArgumentPairs = (projection: unknown): [string, unknown][] | null => {
  if (!projection || typeof projection !== "object" || Array.isArray(projection)) return null;
  const object = projection as Record<string, unknown>;
  if (Object.keys(object).length !== 1 || !Array.isArray(object["*type/quoted*"])) return null;
  const [tag, ...entries] = object["*type/quoted*"];
  return tag === "gateway-use-arguments" && isCompleteArgumentPairCollection(entries)
    ? entries : null;
};

export const isProjectedSchemeReadOnlyUse = (projection: unknown): boolean =>
  projectedSchemeArgumentPairs(projection)
    ?.some(([name, value]) => name === "read-only?" && value === true) ?? false;

const exactSchemeOperationFields: Record<string, Set<string>> = {
  "put!": new Set(["path", "value", "expression?", "object?", "expected"]),
  "use!": new Set(["path", "method", "arguments", "read-only?", "expression?"]),
  retrieve: new Set([
    "path", "method", "arguments", "pinned?", "proof?", "index?", "expression?",
  ]),
};

const validateProjectedSchemeArguments = (
  functionName: string,
  projection: unknown,
): [string, unknown][] => {
  const entries = projectedSchemeArgumentPairs(projection);
  const allowed = exactSchemeOperationFields[functionName];
  if (!entries || !allowed || entries.some(([name]) => !allowed.has(name))) {
    throw invalidRequest(`${functionName} requires one exact collection of unique supported argument pairs`);
  }
  const fields = new Map(entries);
  if (!Array.isArray(fields.get("path"))) {
    throw invalidRequest(`${functionName} path must be one complete Scheme list`);
  }
  if (functionName === "put!" && !fields.has("value")) {
    throw invalidRequest("put! requires exactly one value field");
  }
  if (fields.has("arguments")
      && fields.get("arguments") !== null
      && !Array.isArray(fields.get("arguments"))) {
    throw invalidRequest(`${functionName} arguments must be one complete Scheme list`);
  }
  if (fields.has("method")
      && fields.get("method") !== null
      && typeof fields.get("method") !== "string") {
    throw invalidRequest(`${functionName} method must be one exact Scheme symbol or empty`);
  }
  for (const name of ["expression?", "object?", "read-only?", "pinned?", "proof?", "index?"]) {
    if (fields.has(name) && typeof fields.get(name) !== "boolean") {
      throw invalidRequest(`${functionName} ${name} must be boolean`);
    }
  }
  return entries;
};

const buildSchemeExpression = (
  functionName: string,
  argsExpression: string,
  authSecret?: string,
  identityId?: string,
): string => {
  const parts = [`(function ${functionName})`, `(arguments ${argsExpression})`];
  if (authSecret) {
    const identityPart = identityId ? `(identity (*state* ${identityId})) ` : "";
    parts.push(`(authentication (${identityPart}(credentials "${escapeLispString(authSecret)}")))`);
  }
  return `(${parts.join(" ")})`;
};

const extractFederationContext = (body: unknown): {
  argsBody: unknown;
  present: boolean;
  routeTarget?: string[];
  historyIndexes?: number[];
} => {
  if (!body || typeof body !== "object" || Array.isArray(body) || Buffer.isBuffer(body)) {
    return { argsBody: body, present: false };
  }
  const record = body as Record<string, unknown>;
  const context = record.$federation;
  if (!context || typeof context !== "object" || Array.isArray(context)) {
    return { argsBody: body, present: false };
  }
  const federation = context as Record<string, unknown>;
  const route = federation.route;
  const history = federation.history;
  if (!Array.isArray(route) || !route.every((name) => typeof name === "string")) {
    throw invalidRequest("$federation.route must be an array of bridge names");
  }
  if (history !== undefined &&
      (!Array.isArray(history) || !history.every((index) => Number.isInteger(index)))) {
    throw invalidRequest("$federation.history must be an array of integer indexes");
  }
  const { $federation: _ignored, ...argsBody } = record;
  return {
    argsBody,
    present: true,
    routeTarget: route,
    historyIndexes: history as number[] | undefined,
  };
};

const validateFederationContext = (
  functionName: string,
  context: ReturnType<typeof extractFederationContext>,
  root: boolean,
): void => {
  if (!context.present) return;
  if (root) {
    throw invalidRequest("Federation context is not allowed on root operations");
  }
  const route = context.routeTarget ?? [];
  if (route.length === 0) {
    throw invalidRequest("Federation context requires a nonempty route");
  }
  if (!new Set(["put!", "copy!", "use!", "put-batch!", "copy-batch!", "use-batch!", "run!", "retrieve", "retrieve-batch"]).has(functionName)) {
    throw invalidRequest(`Federation context is not allowed for ${functionName}`);
  }
  if (context.historyIndexes) {
    throw invalidRequest("Federation history is not part of the public Gateway envelope");
  }
};

const buildRootSchemeExpression = (
  functionName: string,
  argsExpression: string,
  authSecret: string
): string => {
  const trimmed = argsExpression.trim();
  if (trimmed === "" || trimmed === "()") {
    return `(${functionName} "${escapeLispString(authSecret)}")`;
  }
  return `(${functionName} "${escapeLispString(authSecret)}" ${trimmed})`;
};

export const isJsonReadOnlyUse = (args: unknown): boolean => {
  if (Array.isArray(args)) {
    if (!isCompleteArgumentPairCollection(args)) return false;
    return args.some(([name, value]) => name === "read-only?" && value === true);
  }
  return !!args
    && typeof args === "object"
    && !Buffer.isBuffer(args)
    && (args as Record<string, unknown>)["read-only?"] === true;
};

const callWithNegotiation = async (input: {
  request: FastifyRequest;
  journal: JournalClient;
  functionName: string;
  requiresAuth: boolean;
  root?: boolean;
  journalSecret: string;
  kratos: KratosClient;
}): Promise<{ result: unknown; readOnlyUse: boolean }> => {
  const { request, journal, functionName, requiresAuth, root = false, journalSecret, kratos } = input;
  const resolved = requiresAuth
    ? await resolveIdentity(request, journalSecret, kratos)
    : undefined;
  const authSecret = resolved?.journalSecret;
  const identityId = resolved?.identityId;
  const contentType = getContentType(request);
  const schemeFederationRoute = extractSchemeFederationRoute(
    request, functionName, root, contentType,
  );

  if (isSchemeContentType(contentType)) {
    const argsExpression = extractSchemeArguments(request.body);
    const expression =
      root && authSecret
        ? buildRootSchemeExpression(functionName, argsExpression, authSecret)
        : buildSchemeExpression(functionName, argsExpression, authSecret, identityId);
    let readOnlyUse = false;
    let projectedArguments: [string, unknown][] | undefined;
    if (!root && (functionName === "use!" || functionName === "use-batch!"
        || (contentType === "application/scheme" && schemeFederationOperations.has(functionName)))) {
      const projection = await journal.schemeToJson(buildSchemeArgumentsProjection(argsExpression));
      if (contentType === "application/scheme" && schemeFederationOperations.has(functionName)) {
        projectedArguments = validateProjectedSchemeArguments(functionName, projection);
      }
      readOnlyUse = isProjectedSchemeReadOnlyUse(projection);
    }
    if (schemeFederationRoute) {
      const result = await journal.callJson({
        functionName,
        args: projectedArguments,
        authentication: authSecret,
        identityId,
        routeTarget: schemeFederationRoute,
      });
      return { result, readOnlyUse };
    }
    const result = root
      ? await journal.callRootScheme({ expression, functionName })
      : await journal.callScheme({ expression, functionName });
    return { result, readOnlyUse };
  }

  if (!isJsonContentType(contentType)) {
    throw unsupportedMediaType(
      "Unsupported content-type. Use application/json or text/plain (or application/scheme)."
    );
  }

  const federation = extractFederationContext(request.body);
  validateFederationContext(functionName, federation, root);
  const rawArgs = extractJsonArguments(
    federation.argsBody,
    !root && functionsWithArgumentsField.has(functionName)
  );
  if (!root && functionName === "run!") {
    if (!rawArgs || typeof rawArgs !== "object" || Array.isArray(rawArgs) ||
        !Array.isArray((rawArgs as Record<string, unknown>).arguments)) {
      throw invalidRequest("Gateway JSON bodies must provide call arguments as an array");
    }
  }
  const args = rawArgs;
  const result = root
    ? await journal.callRootJson({
        functionName,
        args,
        authentication: authSecret,
      })
    : await journal.callJson({
        functionName,
        args,
        authentication: authSecret,
        identityId,
        ...(federation.routeTarget ? { routeTarget: federation.routeTarget } : {}),
        ...(federation.historyIndexes ? { historyIndexes: federation.historyIndexes } : {}),
      });
  const readOnlyUse = !root
    && (functionName === "use!" || functionName === "use-batch!")
    && !(request as ClassifiedJsonRequest)[duplicateTopLevelJsonNames]
    && isJsonReadOnlyUse(args);
  return { result, readOnlyUse };
};

const extractEventPath = (body: unknown): Array<string | number> | undefined => {
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    return undefined;
  }
  const path = (body as Record<string, unknown>).path;
  return isGatewayEventPath(path) ? path : undefined;
};

const writeEventStreamHeaders = (reply: FastifyReply): void => {
  reply.raw.writeHead(200, {
    "Content-Type": "text/event-stream; charset=utf-8",
    "Cache-Control": "no-cache, no-transform",
    "Connection": "keep-alive",
    "X-Accel-Buffering": "no",
  });
};

const generalAliases = {
  put: "put!",
  copy: "copy!",
  use: "use!",
  pin: "pin!",
  "pin-batch": "pin-batch!",
  unpin: "unpin!",
  "unpin-batch": "unpin-batch!",
  prune: "prune!",
  "prune-batch": "prune-batch!",
  run: "run!",
  "put-batch": "put-batch!",
  "use-batch": "use-batch!",
  "copy-batch": "copy-batch!",
  retrieve: "retrieve",
  "retrieve-batch": "retrieve-batch",
  truncate: "truncate!",
  info: "info",
  size: "size",
  "synchronize!": "synchronize!",
  trace: "trace",
  "trace-batch": "trace-batch",
  route: "route",
  bridge: "bridge!",
  "delete-bridge": "delete-bridge!",
  config: "config",
  "update-config": "update-config!",
  admins: "*admins-get*",
  "set-admins": "*admins-set*",
  "set-window": "*window-set*",
  "set-secret": "*secret*",
  authorizations: "authorizations",
  authorize: "authorize!",
  deauthorize: "deauthorize!",
} as const;

const rootAliases = {
  eval: "*eval*",
  call: "*call*",
  step: "*step*",
  "set-secret": "*set-secret*",
  "set-step": "*set-step*",
  "set-query": "*set-query*",
} as const;

const publicGeneralFunctions = new Set<string>(["synchronize!", "trace", "trace-batch", "route"]);
const eventedGeneralOperations = new Set<string>([
  "put",
  "copy",
  "use",
  "use-batch",
  "pin",
  "pin-batch",
  "unpin",
  "unpin-batch",
  "prune",
  "prune-batch",
  "run",
  "put-batch",
  "copy-batch",
  "truncate",
  "bridge",
  "delete-bridge",
  "synchronize!",
  "update-config",
  "set-admins",
  "set-window",
  "set-secret",
  "authorize",
  "deauthorize",
]);
export const publishesGeneralChange = (
  operation: string,
  readOnlyUse = false,
): boolean => eventedGeneralOperations.has(operation) && !readOnlyUse;

const eventedRootOperations = new Set<string>([
  "step",
  "set-secret",
  "set-step",
  "set-query",
]);
const requestModeDescription =
  "JSON mode: Content-Type application/json with a keyword argument object. Legacy array arguments are also accepted for compatibility. Scheme mode: Content-Type text/plain or application/scheme with a raw Scheme arguments expression (the gateway wraps it into the full journal transport call). Only application/scheme put, use, and retrieve may carry X-Sync-Web-Federation-Route as one nonempty JSON array of aliases; each string becomes the exact same s7 symbol through Journal's JSON codec without narrowing the existing $federation.route vocabulary. The s7 reader first validates one unique supported argument-pair collection.";
const authorizationDescription =
  "Authorization is Self-local. `user` is the local owner namespace and `rule.path` is owner-relative. Rules grant `put!`, qualified `use!`, `run!`, and `retrieve`; `(use! ((read-only? #t)))` grants read-only use only while `#f` grants both modes. Remote principals require a `key-index` authentication window. `retrieve` may be true, false, or a two-index history range.";
const authorizationBodyDescription =
  "Authorization body. The schema stays permissive so existing object and legacy-array transports remain accepted; the example shows canonical fields and tuple shapes.";

const generalOperationDocs: Record<string, { summary: string; description: string }> = {
  put: {
    summary: "Stage inert content or an active resource",
    description:
      "Calls canonical `put!`. With `object?` false it preserves inert set semantics; with true it stores one uninitialized Standard class shell without running user code.",
  },
  use: {
    summary: "Exercise staged content or an active resource",
    description:
      "Calls `use!` with separate optional `method` and `arguments`. Object successors persist only when their digest changes; inert use requires both fields blank.",
  },
  copy: {
    summary: "Atomically copy staged content",
    description:
      "Calls `copy!`. Copies raw file or directory content from `source` to target `path` in one staged snapshot. Optional `expected` compares the target before mutation, and one signed working route may carry the complete operation.",
  },
  "put-batch": {
    summary: "Atomically stage ordered inert or object writes",
    description:
      "Calls `put-batch!`. Parallel object flags select inert content or uninitialized Standard shells; expectations compare one snapshot and the ordered batch persists atomically.",
  },
  "use-batch": {
    summary: "Atomically exercise ordered resources",
    description:
      "Calls `use-batch!`. Duplicate paths observe prior staged successors, results retain request order, and any failure rolls back the complete batch.",
  },
  "copy-batch": {
    summary: "Atomically copy ordered staged content",
    description:
      "Calls `copy-batch!`. Captures every raw source and optional target expectation before mutation, then applies ordered target replacements with last-write-wins duplicate targets. The complete operation may use one signed working route.",
  },
  truncate: {
    summary: "Truncate local committed history",
    description:
      "Calls administrative `truncate!`. Irreversibly releases locally available committed history through the inclusive `index` while preserving logical chain identity, numbering, retained suffix, staged state, and future appends. The operation is Self-local and cannot recall remote copies or backups.",
  },
  pin: {
    summary: "Pin state/proof into permanent history",
    description:
      "Calls general function `pin!` with one canonical committed path. Interface resolves and verifies remote proof material before retaining it at the origin.",
  },
  "pin-batch": {
    summary: "Atomically pin ordered committed paths",
    description:
      "Calls `pin-batch!`. Fetches and verifies compact proof groups before atomically retaining every path at the origin.",
  },
  unpin: {
    summary: "Remove a previously pinned path/proof",
    description:
      "Calls general function `unpin!` with the same canonical committed path and returns origin-retained content to normal retention behavior.",
  },
  "unpin-batch": {
    summary: "Atomically unpin committed paths",
    description:
      "Calls `unpin-batch!`. Applies digest-preserving proof cuts for all authorized paths in one local mutation.",
  },
  prune: {
    summary: "Prune retained committed evidence",
    description:
      "Calls local administrative `prune!`. Removes one canonical committed leaf or directory from temporary and permanent retention while preserving staged state and committed history identity.",
  },
  "prune-batch": {
    summary: "Atomically prune retained committed evidence",
    description:
      "Calls local administrative `prune-batch!`. Removes the union of up to 1,024 canonical committed paths from temporary and permanent retention, installing both complete candidates or neither.",
  },
  run: {
    summary: "Run a staged Scheme orchestration program",
    description:
      "Calls canonical `run!`. Interface evaluates the staged procedure outside `sync-let`, supplies the authenticated journal capability, and preserves original-caller nested authorization.",
  },
  info: {
    summary: "Get public info",
    description:
      "Calls public general function `info`. Returns public node metadata.",
  },
  "synchronize!": {
    summary: "Exchange reciprocal signed heads",
    description:
      "Calls public peer function `synchronize!`. Applies the initiator head and returns the acceptor head in one reciprocal exchange.",
  },
  retrieve: {
    summary: "Resolve committed chain content",
    description:
      "Calls general function `retrieve`. Without `$federation`, Interface resolves the canonical origin/alias path. With one explicit route, the selected responder interprets the committed path locally and requires permanent retention; a trailing alias returns its structural Chain inventory. Optional `index?` appends exact selected indexes.",
  },
  "retrieve-batch": {
    summary: "Resolve ordered committed paths",
    description:
      "Calls `retrieve-batch`. Canonical paths may use ordinary grouped resolution, or one `$federation.route` may send the complete batch to one responder for permanent-only local interpretation. Verified multiproofs remain internal, order is preserved, and optional `index?` appends exact selected indexes.",
  },
  trace: {
    summary: "Trace remote content against a chain index",
    description:
      "Calls public general function `trace`. Used by bridges/services to fetch a serialized remote path view from a committed chain index.",
  },
  "trace-batch": {
    summary: "Trace multiple paths into one multiproof",
    description:
      "Calls public `trace-batch`. Returns one compact serialized proof for authorized paths sharing an authenticated history anchor.",
  },
  route: {
    summary: "Resolve a federated journal route",
    description:
      "Calls public function `route`. Resolves canonical committed endpoint/key material through reciprocal bridges.",
  },
  bridge: {
    summary: "Create a reciprocal bridge",
    description:
      "Calls `bridge!` with the local peer alias, peer interface, and the name the peer should use for this journal.",
  },
  "update-config": {
    summary: "Update ledger configuration",
    description:
      "Calls admin function `update-config!`. Used for bridge acceptance/preapproval and other explicit configuration updates.",
  },
  config: {
    summary: "Read full node config",
    description:
      "Calls general function `config`. Includes private/runtime fields and should be treated as sensitive output.",
  },
  admins: {
    summary: "Read interface admins",
    description:
      "Calls general function `*admins-get*`. Returns null when empty or an object keyed by each exact local username.",
  },
  "set-admins": {
    summary: "Replace interface admins",
    description:
      "Calls general function `*admins-set*`. Atomically replaces admins from an object whose exact username keys match its local-principal values.",
  },
  authorizations: {
    summary: "List authorization rules",
    description: `Calls general function \`authorizations\`. Returns exact local rules for an owner principal. ${authorizationDescription}`,
  },
  authorize: {
    summary: "Add authorization rule",
    description: `Calls general function \`authorize!\`. Adds an exact local rule. ${authorizationDescription}`,
  },
  deauthorize: {
    summary: "Remove authorization rule",
    description: `Calls general function \`deauthorize!\`. The rule must be shape-equivalent to the stored rule, including \`key-index\` and Resolve range. ${authorizationDescription}`,
  },
  "set-window": {
    summary: "Set ledger window size",
    description:
      "Calls general function `*window-set*`. Updates the public ledger retention window to a positive integer.",
  },
  "set-secret": {
    summary: "Rotate the general interface secret",
    description:
      "Calls general function `*secret*`. Updates the shared interface secret used for restricted general operations.",
  },
};

const rootOperationDocs: Record<string, { summary: string; description: string }> = {
  eval: {
    summary: "Evaluate Scheme in admin context",
    description:
      "Calls root function `*eval*`. Highly privileged and intended for tightly controlled operations only.",
  },
  call: {
    summary: "Invoke function against root object",
    description:
      "Calls root function `*call*`. Supports runtime-level updates and administrative transformations.",
  },
  step: {
    summary: "Execute full root step cycle",
    description:
      "Calls root function `*step*`. Triggers configured step handler pipeline.",
  },
  "set-secret": {
    summary: "Rotate admin/root secret",
    description:
      "Calls root function `*set-secret*`. Atomically changes the root credential and commits an identity-bound journal signing-key rotation; runtime root-secret configuration must then use the new value.",
  },
  "set-step": {
    summary: "Replace step handler",
    description:
      "Calls root function `*set-step*`. Updates the root-plane step function at runtime.",
  },
  "set-query": {
    summary: "Replace query handler",
    description:
      "Calls root function `*set-query*`. Updates the root-plane query function at runtime.",
  },
};

const makeBodyContent = (
  jsonExample?: unknown,
  schemeExample?: string,
  operationDescription?: string,
) => ({
  content: {
    "application/json": {
      schema: {
        type: ["array", "object"],
        description: operationDescription || "Keyword argument object (preferred) or legacy array.",
        ...(jsonExample !== undefined ? { example: jsonExample } : {}),
      },
    },
    "text/plain": {
      schema: {
        type: "string",
        description: operationDescription
          ? `${operationDescription} Raw Scheme arguments expression.`
          : "Raw Scheme arguments expression.",
        ...(schemeExample !== undefined ? { example: schemeExample } : {}),
      },
    },
    "application/scheme": {
      schema: {
        type: "string",
        description: operationDescription
          ? `${operationDescription} Raw Scheme arguments expression.`
          : "Raw Scheme arguments expression.",
        ...(schemeExample !== undefined ? { example: schemeExample } : {}),
      },
    },
  },
});

const authorizationRuleExample = {
  principal: ["peer-a", "*state*", "bob"],
  "key-index": [-32, -1],
  path: ["docs"],
  "put!": false,
  "use!": { "read-only?": true },
  "run!": false,
  retrieve: [0, -1],
};
const authorizationSchemeRuleExample =
  "((principal (peer-a *state* bob)) (key-index (-32 -1)) (path (docs)) (put! #f) (use! ((read-only? #t))) (run! #f) (retrieve (0 -1)))";

const generalOperationExamples: Record<string, unknown> = {
  put:          { path: ["*state*", "mykey"], value: "myvalue", expected: "oldvalue", "expression?": true },
  use:          { path: ["*state*", "mykey"], "read-only?": true, "expression?": true },
  "put-batch": { paths: [["*state*", "a"]], values: ["value"], "expression?": true },
  "use-batch": { paths: [["*state*", "a"]], "read-only?": true, "expression?": true },
  run:          { path: ["*state*", "alice", "programs", "example"], arguments: [] },
  copy:         { source: ["*state*", "source"], path: ["*state*", "target"], expected: "oldvalue", "expression?": true },
  pin:          { path: [-1, "peer-a", -1, "*state*", "mykey"] },
  "pin-batch": { paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]] },
  unpin:        { path: [-1, "peer-a", -1, "*state*", "mykey"] },
  "unpin-batch": { paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]] },
  prune:        { path: [-1, "peer-a", -1, "*state*", "mykey"] },
  "prune-batch": { paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]] },
  retrieve:     { path: [-1, "peer-a", -1, "*state*", "mykey"], "pinned?": true, "proof?": false, "index?": true, "expression?": true },
  "retrieve-batch": { paths: [[-1, "peer-a", -1, "*state*", "a"], [-1, "*state*", "local"]], "pinned?": true, "index?": true, "expression?": true },
  "copy-batch": { sources: [["*state*", "source"]], paths: [["*state*", "target"]], expected: ["oldvalue"], "expression?": true },
  truncate:     { index: 0 },
  info:         {},
  bridge:       { name: "peer-a", interface: "http://peer-a/interface", "remote-name": "my-journal" },
  "update-config": { path: ["public", "bridge-accept"], value: "preapproved" },
  config:       {},
  admins:       {},
  "set-admins": { admins: { admin: ["*state*", "admin"], alice: ["*state*", "alice"] } },
  "set-window": { value: 128 },
  "set-secret": { secret: "new-secret" },
  authorizations: { user: ["*state*", "alice"] },
  authorize: { user: ["*state*", "alice"], rule: authorizationRuleExample },
  deauthorize: { user: ["*state*", "alice"], rule: authorizationRuleExample },
  "synchronize!": { name: "peer-a", response: [], info: {}, interface: "https://peer-a/interface", "remote-name": "local" },
  trace:        { index: 0, path: ["*state*", "mykey"] },
  "trace-batch": { index: 0, paths: [["*state*", "a"], ["*state*", "b"]] },
  route:        { "route-target": ["peer-a"] },
};

const generalSchemeExamples: Record<string, string> = {
  put:          "((path (*state* mykey)) (value myvalue) (expected oldvalue))",
  use:          "((path (*state* mykey)) (read-only? #t))",
  "put-batch": "((paths ((*state* a))) (values (value)))",
  "use-batch": "((paths ((*state* a))) (read-only? #t))",
  run:          "((path (*state* alice programs example)) (arguments ()))",
  copy:         "((source (*state* source)) (path (*state* target)) (expected oldvalue) (expression? #t))",
  pin:          "((path (-1 peer-a -1 *state* mykey)))",
  "pin-batch": "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))",
  unpin:        "((path (-1 peer-a -1 *state* mykey)))",
  "unpin-batch": "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))",
  prune:        "((path (-1 peer-a -1 *state* mykey)))",
  "prune-batch": "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))))",
  retrieve:     "((path (-1 peer-a -1 *state* mykey)) (pinned? #t) (proof? #f) (index? #t))",
  "retrieve-batch": "((paths ((-1 peer-a -1 *state* a) (-1 *state* local))) (pinned? #t) (index? #t))",
  "copy-batch": "((sources ((*state* source))) (paths ((*state* target))) (expected (oldvalue)) (expression? #t))",
  truncate:     "((index 0))",
  info:         "()",
  bridge:       "((name peer-a) (interface \"http://peer-a/interface\") (remote-name my-journal))",
  "update-config": "((path (public bridge-accept)) (value preapproved))",
  config:       "()",
  admins:       "()",
  "set-admins": "((admins ((admin (*state* admin)) (alice (*state* alice)))))",
  "set-window": "((value 128))",
  "set-secret": "((secret new-secret))",
  authorizations: "((user (*state* alice)))",
  authorize: `((user (*state* alice)) (rule ${authorizationSchemeRuleExample}))`,
  deauthorize: `((user (*state* alice)) (rule ${authorizationSchemeRuleExample}))`,
  "synchronize!": "((name peer-a) (response ()) (info ()) (interface \"https://peer-a/interface\") (remote-name local))",
  trace:        "((index 0) (path (*state* mykey)))",
  "trace-batch": "((index 0) (paths ((*state* a) (*state* b))))",
  route:        "((route-target (peer-a)))",
};

const rootOperationExamples: Record<string, unknown> = {
  eval:          [["+", 1, 2]],
  call:          [["lambda", ["root"], [["root", { "*type/quoted*": "get" }], { "*type/quoted*": ["root", "object", "ledger"] }]]],
  step:          [],
  "set-secret":  [{ "*type/string*": "new-admin-secret" }],
  "set-step":    [["lambda", ["root", "secret", "query"], "root"]],
  "set-query":   [["lambda", ["root", "query"], "root"]],
};

const rootSchemeExamples: Record<string, string> = {
  eval:          "(+ 1 2)",
  call:          "(lambda (root) ((root 'get) '(root object ledger)))",
  step:          "",
  "set-secret":  '"new-admin-secret"',
  "set-step":    "(lambda (root secret query) root)",
  "set-query":   "(lambda (root query) root)",
};


export const gatewayRoutes: FastifyPluginAsync<GatewayRoutesOptions> = async (
  app,
  { journal, allowAdminRoutes, journalSecret, kratos }
) => {
  const rootRoutePath = "/api/v1/root";
  const eventBroker = new GatewayEventBroker();
  const defaultJsonParser = app.getDefaultJsonParser("error", "error");
  app.removeContentTypeParser("application/json");
  app.addContentTypeParser(
    "application/json",
    { parseAs: "string" },
    (request, body, done) => {
      const source = body as string;
      defaultJsonParser(request, source, (error, parsed) => {
        if (!error) {
          (request as ClassifiedJsonRequest)[duplicateTopLevelJsonNames] =
            hasDuplicateTopLevelJsonNames(source);
        }
        done(error, parsed);
      });
    },
  );
  const eventKeepalive = setInterval(() => eventBroker.keepalive(), 25_000);
  eventKeepalive.unref?.();
  app.addHook("onClose", async () => {
    clearInterval(eventKeepalive);
  });

  app.get("/", async (_request, reply) =>
    reply.type("text/html").send(`<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Synchronic Gateway</title>
    <style>
      :root {
        color-scheme: light;
        --blue: #00add0;
        --medium-blue: #0076a9;
        --dark-blue: #002b4c;
        --teal: #008e74;
        --blue-gray: #7d8ea0;
        --toolbar-bg: #171a1f;
        --toolbar-text: #ffffff;
        --bg-primary: #ffffff;
        --bg-secondary: #f8f8f8;
        --text-primary: #002b4c;
        --text-secondary: #7d8ea0;
        --border-color: #e0e0e0;
      }
      [data-theme="dark"] {
        color-scheme: dark;
        --toolbar-bg: #101318;
        --toolbar-text: #f3f6fb;
        --bg-primary: #181a1f;
        --bg-secondary: #20242b;
        --text-primary: #f0f3f8;
        --text-secondary: #a1abb8;
        --border-color: #343b46;
      }
      body {
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
        margin: 0;
        line-height: 1.45;
        background: var(--bg-primary);
        color: var(--text-primary);
      }
      code { font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }
      main {
        max-width: 880px;
        padding: 2rem;
      }
      .toolbar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        padding: 10px 18px;
        min-height: 58px;
        box-sizing: border-box;
        background-color: var(--toolbar-bg);
        color: var(--toolbar-text);
      }
      .toolbar-left,
      .toolbar-right {
        display: flex;
        align-items: center;
        gap: 12px;
        flex-wrap: wrap;
      }
      .toolbar-logo-link {
        display: inline-flex;
      }
      .toolbar-logo {
        width: 36px;
        height: 36px;
        object-fit: contain;
      }
      .toolbar-nav {
        display: flex;
        align-items: center;
        gap: 8px;
      }
      .toolbar-pill {
        padding: 7px 15px;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 999px;
        background: transparent;
        color: var(--toolbar-text);
        font-size: 13px;
        font-weight: 600;
        line-height: 1;
      }
      .toolbar-pill.active {
        background: rgba(255, 255, 255, 0.14);
        border-color: rgba(255, 255, 255, 0.3);
      }
      .toolbar-pill:hover {
        background: rgba(255, 255, 255, 0.1);
        text-decoration: none;
      }
      .card {
        border: 1px solid var(--border-color);
        border-radius: 8px;
        padding: 1rem 1.2rem;
        margin: 1rem 0;
        background: var(--bg-secondary);
      }
      h1 { margin-top: 0; }
      ul { padding-left: 1.2rem; }
      a { color: var(--medium-blue); text-decoration: none; }
      a:hover { text-decoration: underline; }
      #auth-status {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 4px 8px 4px 12px;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 999px;
        font-size: 0.85rem;
        background: rgba(255, 255, 255, 0.06);
        color: var(--toolbar-text);
      }
      .auth-btn {
        padding: 3px 10px;
        border-radius: 999px;
        font-size: 12px;
        font-weight: 600;
        cursor: pointer;
        text-decoration: none;
      }
      .auth-name-link {
        color: var(--toolbar-text);
        opacity: 0.85;
        max-width: 200px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        text-decoration: none;
      }
      .auth-name-link:hover {
        opacity: 1;
        text-decoration: underline;
      }
      .auth-btn-login {
        background: transparent;
        color: var(--toolbar-text);
        border: 1px solid rgba(255, 255, 255, 0.18);
        padding: 7px 15px;
        font-size: 13px;
      }
      #auth-status.logged-out {
        padding: 0;
        border: none;
        background: transparent;
      }
      .toolbar-icon {
        width: 36px;
        height: 36px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 999px;
        background: rgba(255, 255, 255, 0.08);
        color: var(--toolbar-text);
        cursor: pointer;
        font-size: 16px;
        line-height: 1;
      }
      .toolbar-icon:hover {
        background: rgba(255, 255, 255, 0.14);
      }
      .auth-btn-logout {
        background: transparent;
        color: var(--toolbar-text);
        border: 1px solid rgba(255, 255, 255, 0.25);
        opacity: 0.8;
      }
      .auth-btn-logout:hover {
        opacity: 1;
        background: rgba(255, 255, 255, 0.1);
      }
      @media (max-width: 640px) {
        .toolbar {
          align-items: flex-start;
          flex-direction: column;
        }
        .toolbar-right {
          width: 100%;
        }
        .toolbar-nav {
          flex-wrap: wrap;
        }
        #auth-status {
          max-width: 100%;
        }
        main {
          padding: 1.25rem;
        }
      }
    </style>
  </head>
  <body>
    <div class="toolbar">
      <div class="toolbar-left">
        <a class="toolbar-logo-link" href="/gateway"><img class="toolbar-logo" src="/gateway-logo.png" alt="Synchronic Web" /></a>
        <nav class="toolbar-nav" aria-label="Gateway sections">
          <span class="toolbar-pill active">Gateway</span>
          <a class="toolbar-pill" href="/api/v1/docs">API Reference</a>
        </nav>
      </div>
      <div class="toolbar-right">
        <div id="auth-status">
          <span id="auth-label">Checking session…</span>
        </div>
        <button id="theme-toggle" class="toolbar-icon" type="button" title="Switch to dark mode" aria-label="Switch to dark mode">◐</button>
      </div>
    </div>

    <main>
      <h1>Synchronic Gateway</h1>

      <p>
        Web-facing gateway for Synchronic <code>general</code> and optional <code>root</code> operations.
        This service forwards operation calls to journal endpoints with session-based authentication.
      </p>
      <p>
        Use this service when you want stable, versioned HTTP endpoints that map directly to function-level journal calls
        while preserving authentication and request-shape consistency across clients.
      </p>

      <div class="card">
        <h2>API Docs</h2>
        <ul>
          <li><a href="/api/v1/docs">Swagger UI</a> (<code>/api/v1/docs</code>)</li>
        </ul>
        <p>
          Start there for route-by-route schemas, authentication requirements, and JSON/Scheme request-body guidance.
        </p>
      </div>

    <div class="card">
      <h2>Route Groups</h2>
      <ul>
        <li><code>/api/v1/general/*</code>: primary app-facing operations.</li>
        <li><code>/api/v1/root/*</code>: admin operations (only when enabled).</li>
        <li><code>/healthz</code> and <code>/readyz</code>: container and dependency probes.</li>
      </ul>
    </div>

    <div class="card">
      <h2>Common Patterns</h2>
      <ul>
        <li>Public reads: <code>GET /api/v1/general/size</code>, <code>GET /api/v1/general/info</code>.</li>
        <li>Restricted operations require a valid Kratos session cookie — <a href="/auth/login">log in</a> first.</li>
        <li>Mutating calls are <code>POST</code> and accept either JSON or Scheme argument bodies.</li>
      </ul>
    </div>

    <div class="card">
      <h2>Health</h2>
      <ul>
        <li><a href="/healthz"><code>/healthz</code></a></li>
        <li><a href="/readyz"><code>/readyz</code></a></li>
      </ul>
    </div>

    <div class="card">
      <h2>Quick Start</h2>
      <p>Public size call:</p>
      <pre><code>curl http://127.0.0.1:8180/api/v1/general/size</code></pre>
      <p>Authenticated call (pass session cookie from browser):</p>
      <pre><code>curl -X POST http://127.0.0.1:8180/api/v1/general/use \\
  -H "Cookie: ory_kratos_session=&lt;session&gt;" \\
  -H "Content-Type: application/json" \\
  -d '{"path":["*state*","docs"]}'</code></pre>
      <p>Scheme body call:</p>
      <pre><code>curl -X POST http://127.0.0.1:8180/api/v1/general/use \\
  -H "Cookie: ory_kratos_session=&lt;session&gt;" \\
  -H "Content-Type: text/plain" \\
  -d '((path (*state* docs)))'</code></pre>
    </div>
    </main>
  </body>
  <script>
    (function () {
      const THEME_KEY = 'sync-gateway-theme';

      function getPreferredTheme() {
        const stored = localStorage.getItem(THEME_KEY);
        if (stored === 'light' || stored === 'dark') return stored;
        return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches
          ? 'dark'
          : 'light';
      }

      function applyTheme(theme) {
        document.documentElement.setAttribute('data-theme', theme);
        const btn = document.getElementById('theme-toggle');
        if (!btn) return;
        const nextTheme = theme === 'light' ? 'dark' : 'light';
        btn.textContent = theme === 'light' ? '◐' : '◑';
        btn.title = 'Switch to ' + nextTheme + ' mode';
        btn.setAttribute('aria-label', 'Switch to ' + nextTheme + ' mode');
      }

      applyTheme(getPreferredTheme());
      const themeToggle = document.getElementById('theme-toggle');
      if (themeToggle) {
        themeToggle.addEventListener('click', function () {
          const current = document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
          const next = current === 'light' ? 'dark' : 'light';
          localStorage.setItem(THEME_KEY, next);
          applyTheme(next);
        });
      }

      async function getSession() {
        try {
          const res = await fetch('/auth/.ory/sessions/whoami', {
            credentials: 'include',
            headers: { accept: 'application/json' },
          });
          if (res.ok) {
            const data = await res.json();
            return { loggedIn: true, name: data?.identity?.traits?.username ?? '' };
          }
        } catch (_) {}
        return { loggedIn: false };
      }

      async function logout() {
        try {
          const res = await fetch('/auth/.ory/self-service/logout/browser?return_to=' + encodeURIComponent(window.location.origin + '/api/v1/docs'), {
            credentials: 'include',
            headers: { accept: 'application/json' },
          });
          if (res.ok) {
            const data = await res.json();
            if (data.logout_url) { window.location.href = data.logout_url; return; }
          }
        } catch (_) {}
        window.location.href = '/auth/login';
      }

      getSession().then(function (session) {
        const el = document.getElementById('auth-status');
        const label = document.getElementById('auth-label');
        if (!el || !label) return;
        if (session.loggedIn) {
          el.classList.add('logged-in');
          if (session.name) {
            const accountLink = document.createElement('a');
            accountLink.className = 'auth-name-link';
            accountLink.href = '/auth/settings';
            accountLink.title = 'Account settings';
            accountLink.textContent = session.name;
            label.replaceWith(accountLink);
          } else {
            label.textContent = 'Signed in';
          }
          const btn = document.createElement('button');
          btn.className = 'auth-btn auth-btn-logout';
          btn.textContent = 'Sign out';
          btn.addEventListener('click', logout);
          el.appendChild(btn);
        } else {
          el.classList.add('logged-out');
          label.remove();
          const a = document.createElement('a');
          a.className = 'auth-btn auth-btn-login';
          a.href = '/auth/.ory/self-service/login/browser?return_to=' + encodeURIComponent(window.location.origin + '/api/v1/docs');
          a.textContent = 'Log in';
          el.appendChild(a);
        }
      });
    })();
  </script>
</html>`)
  );

  app.get("/docs", async (_request, reply) => reply.redirect("/api/v1/docs"));
  app.get("/api/docs", async (_request, reply) => reply.redirect("/api/v1/docs"));

  app.get(
    "/healthz",
    {
      schema: {
        tags: ["Health"],
        summary: "Liveness probe",
        description:
          "Returns process liveness only. Does not verify journal connectivity.",
        response: {
          200: {
            type: "object",
            properties: { ok: { type: "boolean" } },
            required: ["ok"],
          },
        },
      },
    },
    async () => ({ ok: true })
  );

  app.get(
    "/readyz",
    {
      schema: {
        tags: ["Health"],
        summary: "Readiness probe",
        description:
          "Verifies the gateway can successfully execute a lightweight upstream call (`size`) against the journal JSON endpoint.",
        response: {
          200: {
            type: "object",
            properties: { ok: { type: "boolean" } },
            required: ["ok"],
          },
          503: {
            type: "object",
            properties: { ok: { type: "boolean" }, error: { type: "string" } },
            required: ["ok", "error"],
          },
        },
      },
    },
    async (_request, reply) => {
      try {
        await journal.callJson({ functionName: "size" });
        return { ok: true };
      } catch {
        return reply.code(503).send({ ok: false, error: "journal_unavailable" });
      }
    }
  );

  app.get(
    "/api/v1/general/size",
    {
      schema: {
        tags: ["General API (Public)"],
        summary: "Get ledger size (public)",
        description:
          "Public convenience endpoint for general function `size`. Useful for quick health/chain progression checks.",
      },
    },
    async () => journal.callJson({ functionName: "size" })
  );

  app.get(
    "/api/v1/general/info",
    {
      schema: {
        tags: ["General API (Public)"],
        summary: "Get public info (public)",
        description:
          "Public convenience endpoint for general function `info`. Returns public node metadata.",
      },
    },
    async () => journal.callJson({ functionName: "info" })
  );

  app.get(
    "/api/v1/raw",
    { schema: { hide: true } },
    async (request, reply) => {
      Object.entries(RAW_RESPONSE_HEADERS).forEach(([name, value]) => reply.header(name, value));
      const resolved = await resolveSessionIdentity(request, journalSecret, kratos);
      let selection;
      try {
        const query = request.query as Record<string, unknown>;
        if (Object.keys(query).length !== 1 || typeof query.selection !== "string") {
          throw new Error("invalid raw query");
        }
        selection = decodeRawSelection(query.selection);
      } catch {
        return reply.code(400).send({ error: "invalid_raw_selection" });
      }

      let result: unknown;
      try {
        result = selection.mode === "stage"
          ? await journal.callJson({
              functionName: "use!",
              args: { path: selection.path, "read-only?": true, "expression?": false },
              authentication: resolved.journalSecret,
              identityId: resolved.identityId,
              ...(selection.route.length > 0 ? { routeTarget: selection.route } : {}),
            })
          : await journal.callJson({
              functionName: "retrieve",
              args: {
                path: selection.path,
                "expression?": false,
                "pinned?": false,
                "proof?": false,
                "index?": false,
              },
              authentication: resolved.journalSecret,
              identityId: resolved.identityId,
            });
      } catch (error) {
        if (error instanceof JournalSemanticError) {
          const authorization = new Set(["authentication-error", "authorization-error"])
            .has(error.code);
          const unavailable = new Set([
            "availability-error",
            "bridge-error",
            "bridge-index-error",
            "index-error",
          ]).has(error.code);
          return reply.code(error.statusCode).send({
            error: authorization ? "authorization_error" : unavailable ? "unavailable" : "journal_error",
          });
        }
        return reply.code(502).send({ error: "gateway_error" });
      }

      const extracted = extractRawBytes(result);
      if (extracted.kind === "missing") return reply.code(404).send({ error: "not_found" });
      if (extracted.kind === "unavailable") return reply.code(503).send({ error: "unavailable" });
      if (extracted.kind === "unsupported") return reply.code(415).send({ error: "raw_value_required" });

      const classification = classifyRawBytes(extracted.bytes);
      reply.header("content-type", classification.contentType);
      reply.header(
        "content-disposition",
        classification.disposition === "inline" ? "inline" : 'attachment; filename="raw.bin"',
      );
      reply.header("content-length", String(extracted.bytes.byteLength));
      return reply.send(extracted.bytes);
    },
  );

  app.get(
    "/api/v1/events",
    {
      schema: {
        tags: ["Events"],
        summary: "Subscribe to gateway-local change events",
        description:
          "Authenticated Server-Sent Events stream for lightweight gateway-local change hints. Events are not authoritative history; clients should re-fetch content under normal authorization.",
      },
    },
    async (request, reply) => {
      await resolveIdentity(request, journalSecret, kratos);
      writeEventStreamHeaders(reply);
      const unsubscribe = eventBroker.subscribe(reply.raw);
      request.raw.on("close", unsubscribe);
      reply.hijack();
    }
  );

  app.post(
    "/api/v1/journal/interface",
    {
      schema: {
        tags: ["Journal (Proxy)"],
        summary: "Transparent journal interface proxy",
        description:
          "Thin pass-through to the journal interface. Scheme bodies (text/plain or application/scheme) are forwarded as-is to the journal Scheme endpoint. JSON bodies are forwarded as-is to the journal JSON endpoint. No authentication injection or body transformation. Intended for journal-to-journal bridge calls.",
        body: makeBodyContent(
          { function: "size" },
          "((function size))"
        ),
      },
    },
    async (request, reply) => {
      const contentType = getContentType(request);
      if (isDefaultSchemeProxyContentType(contentType) || isSchemeContentType(contentType)) {
        const expression = extractSchemeArguments(request.body);
        const response = await journal.proxyScheme(expression);
        return reply.type("text/plain; charset=utf-8").send(response);
      }
      if (isJsonContentType(contentType)) {
        return journal.proxyJson(request.body);
      }
      return reply.code(415).send({
        error: "unsupported_media_type",
        message: "Use application/json, text/plain, or application/scheme.",
      });
    }
  );

  for (const [operation, functionName] of Object.entries(generalAliases)) {
    const requiresAuth = !publicGeneralFunctions.has(functionName);
    app.post(
      `/api/v1/general/${operation}`,
      {
        schema: {
          tags: [
            requiresAuth ? "General API (Restricted)" : "General API (Public)",
          ],
          summary:
            generalOperationDocs[operation]?.summary ||
            `General operation '${operation}'`,
          description: `${generalOperationDocs[operation]?.description || "General operation."} ${requestModeDescription}`,
          body: makeBodyContent(
            generalOperationExamples[operation],
            generalSchemeExamples[operation],
            new Set(["authorizations", "authorize", "deauthorize"]).has(operation)
              ? authorizationBodyDescription
              : undefined,
          ),
        },
      },
      async (request) => {
        const { result, readOnlyUse } = await callWithNegotiation({
          request,
          journal,
          functionName,
          requiresAuth,
          journalSecret,
          kratos,
        });
        if (publishesGeneralChange(operation, readOnlyUse)) {
          eventBroker.publish({
            operation: functionName,
            path: extractEventPath(request.body),
          });
        }
        return result;
      }
    );
  }

  if (allowAdminRoutes) {
    for (const [operation, functionName] of Object.entries(rootAliases)) {
      app.post(
        `${rootRoutePath}/${operation}`,
        {
          schema: {
            tags: ["Root API (Admin)"],
            summary:
              rootOperationDocs[operation]?.summary ||
              `Root operation '${operation}'`,
            description: `${rootOperationDocs[operation]?.description || "Root operation."} ${requestModeDescription}`,
            body: makeBodyContent(rootOperationExamples[operation], rootSchemeExamples[operation]),
          },
        },
        async (request) => {
          const { result } = await callWithNegotiation({
            request,
            journal,
            functionName,
            requiresAuth: true,
            root: true,
            journalSecret,
            kratos,
          });
          if (eventedRootOperations.has(operation)) {
            eventBroker.publish({ operation: functionName });
          }
          return result;
        }
      );
    }
  }

  const generateKeyId = (): string => randomBytes(4).toString("hex");
  const generateSecret = (): string => randomBytes(32).toString("hex");
  const hashSecret = (s: string): string => createHash("sha256").update(s).digest("hex");
  const stripUuidHyphens = (uuid: string): string => uuid.replace(/-/g, "");

  app.post(
    "/api/v1/tokens",
    {
      schema: {
        tags: ["API Tokens"],
        summary: "Create an API token",
        description:
          "Creates a new API token for the authenticated user. Returns the plaintext token exactly once — store it immediately, it cannot be retrieved again.",
        body: {
          type: "object",
          properties: { description: { type: "string" } },
        },
        response: {
          201: {
            type: "object",
            properties: {
              token: { type: "string" },
              id: { type: "string" },
              description: { type: "string" },
              created_at: { type: "string" },
            },
            required: ["token", "id", "created_at"],
          },
        },
      },
    },
    async (request, reply) => {
      const resolved = await resolveSessionIdentity(request, journalSecret, kratos);
      const identity = await kratos.getIdentityById(resolved.kratosId);
      const existingTokens = identity.metadata_admin?.api_tokens ?? {};

      let tokenId: string;
      do {
        tokenId = generateKeyId();
      } while (tokenId in existingTokens);

      const secret = generateSecret();
      const description =
        typeof (request.body as Record<string, unknown>)?.description === "string"
          ? ((request.body as Record<string, string>).description)
          : "";
      const created_at = new Date().toISOString();

      const newEntry: ApiTokenEntry = { hash: hashSecret(secret), description, created_at };
      await kratos.patchIdentityApiTokens(resolved.kratosId, { ...existingTokens, [tokenId]: newEntry });

      const token = `sync-${stripUuidHyphens(resolved.kratosId)}-${tokenId}-0-${secret}`;
      return reply.code(201).send({ token, id: tokenId, description, created_at });
    }
  );

  app.get(
    "/api/v1/tokens",
    {
      schema: {
        tags: ["API Tokens"],
        summary: "List API tokens",
        description: "Lists all API tokens for the authenticated user. Never returns secrets.",
        response: {
          200: {
            type: "array",
            items: {
              type: "object",
              properties: {
                id: { type: "string" },
                description: { type: "string" },
                created_at: { type: "string" },
              },
              required: ["id", "created_at"],
            },
          },
        },
      },
    },
    async (request) => {
      const resolved = await resolveSessionIdentity(request, journalSecret, kratos);
      const identity = await kratos.getIdentityById(resolved.kratosId);
      const tokens = identity.metadata_admin?.api_tokens ?? {};
      return Object.entries(tokens).map(([id, entry]) => ({
        id,
        description: entry.description,
        created_at: entry.created_at,
      }));
    }
  );

  app.delete(
    "/api/v1/tokens/:id",
    {
      schema: {
        tags: ["API Tokens"],
        summary: "Revoke an API token",
        description: "Permanently revokes an API token by id. The token is immediately invalid.",
        params: {
          type: "object",
          properties: { id: { type: "string" } },
          required: ["id"],
        },
        response: {
          204: { type: "null" },
          404: {
            type: "object",
            properties: { error: { type: "string" } },
            required: ["error"],
          },
        },
      },
    },
    async (request, reply) => {
      const { id } = request.params as { id: string };
      const resolved = await resolveSessionIdentity(request, journalSecret, kratos);
      const identity = await kratos.getIdentityById(resolved.kratosId);
      const existing = identity.metadata_admin?.api_tokens ?? {};
      if (!(id in existing)) {
        return reply.code(404).send({ error: "not_found" });
      }
      const updated = Object.fromEntries(Object.entries(existing).filter(([k]) => k !== id));
      await kratos.patchIdentityApiTokens(resolved.kratosId, updated);
      return reply.code(204).send();
    }
  );

  app.setNotFoundHandler(async (request, reply) => {
    const pathname = new URL(request.raw.url ?? "/", "http://gateway.invalid").pathname;
    if (pathname.startsWith("/api/v1/raw/")) {
      Object.entries(RAW_RESPONSE_HEADERS).forEach(([name, value]) => reply.header(name, value));
      await resolveSessionIdentity(request, journalSecret, kratos);
      return reply.code(400).send({ error: "invalid_raw_selection" });
    }
    return reply.code(404).send({
      message: `Route ${request.method}:${request.raw.url} not found`,
      error: "Not Found",
      statusCode: 404,
    });
  });

  app.setErrorHandler((error, request, reply) => {
    const asRecord =
      typeof error === "object" && error !== null
        ? (error as Record<string, unknown>)
        : {};
    const errorMessage =
      error instanceof Error ? error.message : String(asRecord.message || error);

    // Fastify validation errors should be surfaced as 400, not generic gateway failures.
    if ("validation" in asRecord && asRecord.validation) {
      return reply.code(400).send({
        error: "invalid_request",
        message: errorMessage,
      });
    }
    // Unsupported media types can be raised by Fastify before handler logic runs.
    if (asRecord.code === "FST_ERR_CTP_INVALID_MEDIA_TYPE") {
      return reply.code(415).send({
        error: "unsupported_media_type",
        message: errorMessage,
      });
    }
    if (asRecord.code === "FST_ERR_CTP_INVALID_JSON_BODY") {
      return reply.code(400).send({
        error: "invalid_request",
        message: errorMessage,
      });
    }
    if (error instanceof UnauthorizedError) {
      return reply.code(401).send({
        error: "unauthorized",
        message: "Valid Kratos session cookie required",
      });
    }
    if (error instanceof GatewayRequestError) {
      return reply.code(error.statusCode).send({
        error: error.code,
        message: error.message,
      });
    }
    if (error instanceof JournalSemanticError) {
      return reply.code(error.statusCode).send({
        error: error.code || "journal_error",
        message: error.message,
        details: error.details,
        source: "journal",
      });
    }
    request.log.error({ err: error }, "Unhandled gateway error");
    return reply.code(502).send({
      error: "gateway_error",
      message: errorMessage,
    });
  });
};
