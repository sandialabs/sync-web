# Synchronic Gateway

Web-facing API gateway for Synchronic `general` and (optionally) `root` operations.
It presents a versioned HTTP API that maps function-oriented journal calls into web-native routes, request schemas, and header-based authentication.

## What This Service Is For

Use `gateway` when clients should not call the raw journal transport endpoints directly.
It adds:

- stable versioned route paths (`/api/v1/...`)
- explicit auth headers instead of body-only credentials
- JSON and Scheme request-body support behind one route shape
- Swagger/OpenAPI docs for discoverability and onboarding
- Prometheus-compatible metrics emission at `/metrics`
- readiness/liveness probes for container orchestration

## Development

### Prerequisites

- Node.js 20+
- npm

### Install

```bash
npm install
npm --prefix ui install
```

### Run (dev)

```bash
npm run dev
```

### Build

```bash
npm run build
```

Builds both the React auth UI and the Fastify server from the gateway parent package.

### Unit tests

```bash
npm test
```

Runs the server tests and the auth UI test script. The UI test currently typechecks the Vite app.

### Run (prod)

```bash
npm run start
```

## Environment Variables

- `HOST` (default: `0.0.0.0`)
- `PORT` (default: `8180`)
- `JOURNAL_ENDPOINT` (default: `http://127.0.0.1:8192/interface`)
- `ROOT_ENDPOINT` (default: `http://127.0.0.1:8192/interface`)
- `REQUEST_TIMEOUT_MS` (default: `30000`)
- Request body limit: `64 MiB`
- `ALLOW_ADMIN_ROUTES` (default: `false`)
- `DEBUG_FORWARDING` (default: `false`)
- `KRATOS_PUBLIC_URL` (default: `http://identity-provider:4433`)
- `KRATOS_ADMIN_URL` (default: `http://identity-provider:4434`)

## API Style

- Versioned base: `/api/v1`
- Function-final aliases:
  - `POST /api/v1/general/set` -> journal `set!`
  - `POST /api/v1/general/pin` -> journal `pin!`
  - etc.
- Public `GET` endpoints:
  - `/api/v1/general/size`
  - `/api/v1/general/info`

Swagger UI:

- `GET /api/v1/docs` (canonical)

Landing page:

- `GET /` (overview, route groups, quick examples)

## Content Negotiation

Gateway supports both JSON and Scheme request bodies for `POST` operation endpoints.

### JSON Mode

- `Content-Type: application/json`
- Body shape:

```json
{ ... }
```

- Use keyword-style argument object fields directly (for example `{ "path": ... }` for staged reads or `{ "path": ..., "pinned?": true, "proof?": true }` for committed/indexed `resolve` calls). `set` and `set-batch` accept optional `expected` values: every expected staged value must match or the operation returns `false` without writing; conditional writes require both read and write authorization.

- General routes are forwarded to the raw journal interface transport endpoint: `/interface` with `Content-Type: application/json`

### Scheme Mode

- `Content-Type: text/plain` or `application/scheme`
- Body: Scheme arguments expression text only (not a full query envelope)

Example body:

```scheme
((path (*state* docs article hash)) (pinned? #t) (proof? #t))
```

Gateway composes the full Scheme call expression and forwards to the raw journal transport endpoint:

- `/interface` with `Content-Type: application/scheme`

### Root Route Forwarding

Root routes do not use the interface query envelope upstream.
They are forwarded as raw root calls instead:

- JSON mode:
  - `POST /api/v1/root/step` with `[]` becomes `["*step*", {"*type/string*": "<secret>"}]`
- Scheme mode:
  - `POST /api/v1/root/step` with body `()` becomes `(*step* "<secret>")`
  - `POST /api/v1/root/step` with body `(ledger-step #t)` becomes `(*step* "<secret>" (ledger-step #t))`

This matters because `*step*`, `*set-step*`, and related admin operations are raw root expressions, not general-interface queries.

## Authentication

Restricted routes accept two forms of authentication:

- **Session cookie**: `ory_kratos_session=<token>` (obtain by logging in at `/auth/login`)
- **API token**: `Authorization: Bearer sync-<uuid>-<key-id>-0-<secret>` (obtain via `POST /api/v1/tokens`)

API tokens are machine/delegated credentials stored as SHA-256 hashes in Kratos `metadata_admin`. The plaintext token is returned once at creation and cannot be retrieved again.

## Route Summary

### Health

- `GET /healthz`
- `GET /readyz`
- `GET /metrics` (public Prometheus metrics)

### Metrics

The gateway emits Prometheus-format metrics directly from the process at `GET /metrics`.

Included metrics:

- default Node.js/process metrics from `prom-client`
- `sync_gateway_requests_total`
- `sync_gateway_request_duration_seconds`
- `sync_gateway_in_flight_requests`
- `sync_gateway_journal_requests_total`
- `sync_gateway_journal_request_duration_seconds`

### API Tokens

- `POST /api/v1/tokens` — create an API token (session required; token returned once)
- `GET /api/v1/tokens` — list tokens for the current user (id + description + created_at)
- `DELETE /api/v1/tokens/:id` — revoke a token by id

### General

- `GET /api/v1/general/size` (public)
- `GET /api/v1/general/info` (public)
- `POST /api/v1/general/get`
- `POST /api/v1/general/get-batch`
- `POST /api/v1/general/set`
- `POST /api/v1/general/pin`
- `POST /api/v1/general/pin-batch`
- `POST /api/v1/general/unpin`
- `POST /api/v1/general/unpin-batch`
- `POST /api/v1/general/call`
- `POST /api/v1/general/set-batch`
- `POST /api/v1/general/synchronize!` (public reciprocal exchange)
- `POST /api/v1/general/resolve`
- `POST /api/v1/general/resolve-batch`
- `POST /api/v1/general/trace` (public)
- `POST /api/v1/general/trace-batch` (public)
- `POST /api/v1/general/route` (public)
- `POST /api/v1/general/bridge`
- `POST /api/v1/general/config`
- `POST /api/v1/general/update-config`
- `POST /api/v1/general/admins`
- `POST /api/v1/general/set-admins`
- `POST /api/v1/general/set-window`
- `POST /api/v1/general/set-secret`
- `POST /api/v1/general/authorizations`
- `POST /api/v1/general/authorize`
- `POST /api/v1/general/deauthorize`

Admin-oriented general endpoints:

- `admins` calls `*admins-get*` and returns local administrator principal paths.
- `set-admins` calls `*admins-set*` and replaces that list wholesale; entries must be `[*state*, <name>]` principals.
- `set-window` calls `*window-set*` and updates the public ledger retention window.
- `bridge` creates a reciprocal relationship from `name`, `interface`, and `remote-name`.
- `update-config` manages explicit ledger configuration such as `(public bridge-accept)` and `(private bridge-preapproval <name>)`.
- Authorization routes are Self-local. `user` is the owner's local `[*state*, USER]` namespace and `rule.path` is owner-relative. A remote exact bridge principal requires a terminal authentication `key-index` such as `[-32, -1]`; exact local/public principals omit it. Resolve is independent and uses `true`, `false`, or a document-history range such as `[0, -1]`. The complete stored rule must be sent unchanged to `deauthorize`.
- `call` asks Interface to load the current staged Scheme procedure from `path`, evaluate it outside `sync-let` in an Interface-owned masked environment, and apply it to an inherited authenticated journal capability followed by the explicit `arguments` list. Invocation is limited to configured Interface administrators/root; namespace ownership and Authorization rules cannot grant it, and federated invocation is rejected.
- Staged JSON `get`, `set`, `get-batch`, and `set-batch` calls accept `$federation: { route: [<aliases>] }`; one dedicated batch uses one exact working route and signs its complete ordered arguments. Committed `resolve`, `pin`, and `unpin`, including their batch forms, use canonical full paths containing the origin index and each alias/index hop; one resolution batch may span multiple route/history groups. Interface preserves result order, verifies one compact terminal proof per compatible group, and never returns those internal proofs from `resolve-batch`. Optional `pinned?` status is checked independently for each origin-relative path. Pin/unpin and all administration remain local mutations at the origin journal; `pin-batch!` fetches every remote proof before its one atomic retention mutation. Batch calls accept at most 1,024 paths; existing transport body, response, and timeout bounds still apply.

Dedicated batch routes preserve duplicates and request order. Missing content is returned as the Journal `(nothing)` sentinel. `set` and `set-batch` distinguish an omitted `expected` field from explicit `false`/`#f`; batch expectations compare against one snapshot, cardinalities must match, and a conflict returns `false` without a transition. Conditional writes require both read and write authority. Empty data/retention batches are identity operations, while `trace-batch` needs at least one same-anchor path. There is no arbitrary `/general/batch` or `/general/copy` route.

```json
POST /api/v1/general/set-batch
{
  "paths": [["*state*", "alice", "one"], ["*state*", "alice", "two"]],
  "values": ["new-one", "new-two"],
  "expected": ["old-one", "old-two"],
  "expression?": true
}
```

```scheme
((paths ((*state* alice one) (*state* alice two)))
 (values (new-one new-two))
 (expected (old-one old-two))
 (expression? #t))
```

The OpenAPI document at `/api/v1/docs` contains JSON and Scheme examples for all six dedicated batch routes. Journal semantic failures retain their symbolic error code and are returned as HTTP 400; transport/runtime failures remain HTTP 502/504 according to the existing Gateway contract.

### Root (disabled in the standard deployment)

The codebase retains these explicitly opt-in routes, but the standard deployment gives Gateway only the independently rotatable Interface credential, never Root. Root administration therefore remains Journal-local:

- `POST /api/v1/root/eval`
- `POST /api/v1/root/call`
- `POST /api/v1/root/step`
- `POST /api/v1/root/set-secret`
- `POST /api/v1/root/set-step`
- `POST /api/v1/root/set-query`

Root `set-secret` atomically commits a journal signing-key transition bound to
the journal's stable random identity. Update the Journal-only runtime `SECRET`
configuration before subsequent root calls or steps; peers verify the transition
during normal bridge synchronization.

## Examples

Public read:

```bash
curl http://127.0.0.1:8180/api/v1/general/size
```

Authorization add/delete JSON rule (use the identical rule for both operations):

```json
{
  "user": ["*state*", "alice"],
  "rule": {
    "principal": ["peer-a", "*state*", "bob"],
    "key-index": [-32, -1],
    "path": ["docs"],
    "get": true,
    "set!": false,
    "resolve": [0, -1]
  }
}
```

Restricted JSON call:

```bash
curl -X POST http://127.0.0.1:8180/api/v1/general/get \
  -H "Authorization: Bearer sync-<uuid>-<key-id>-0-<secret>" \
  -H "Content-Type: application/json" \
  -d '{"path":["*state*","docs","article","hash"]}'
```

Federated committed read (Alice → Carol → Bob):

```bash
curl -X POST http://127.0.0.1:8180/api/v1/general/resolve \
  -H "Authorization: Bearer sync-<uuid>-<key-id>-0-<secret>" \
  -H "Content-Type: application/json" \
  -d '{
    "path": [7,"*state*","bob","shared","message"],
    "pinned?": true,
    "proof?": true,
    "$federation": {
      "route": ["carol","bob"],
      "history": [-1,4,7]
    }
  }'
```

The returned proof is verified by the origin. To retain remote content, send that proof to the local `pin` endpoint with the full origin-relative historical path and no `$federation` context.

Restricted Scheme call:

```bash
curl -X POST http://127.0.0.1:8180/api/v1/general/get \
  -H "Authorization: Bearer sync-<uuid>-<key-id>-0-<secret>" \
  -H "Content-Type: text/plain" \
  -d '((path (*state* docs article hash)))'
```

Restricted root step call:

```bash
curl -X POST http://127.0.0.1:8180/api/v1/root/step \
  -H "Authorization: Bearer sync-<uuid>-<key-id>-0-<secret>" \
  -H "Content-Type: application/json" \
  -d '[]'
```

Application-specific multi-step workflows belong in staged Scheme programs invoked through `call`. Nested calls retain ordinary authorization and are explicitly non-atomic; use conditional `set`/`set-batch` expectations when optimistic concurrency is required.

Forwarding debug mode:

```bash
DEBUG_FORWARDING=1 npm run dev
```

## Developer Notes

Recommended integration pattern:

1. Use `GET` endpoints for simple public reads (`size`, `info`).
2. Use `POST /api/v1/general/<operation>` for everything that takes arguments.
3. Default to JSON in services; use Scheme mode for advanced evaluator-native flows.
4. Use dedicated batch operations for common bulk work. Configured local administrators/root may use staged `call` programs for application-specific, explicitly non-atomic composition.
5. Validate payloads in Swagger first, then copy canonical samples into tests.

## Metrics

The gateway emits Prometheus-format metrics at:

- `GET /metrics`

Current metrics include:

- default Node.js/process metrics from `prom-client`
- `sync_gateway_requests_total`
- `sync_gateway_request_duration_seconds`
- `sync_gateway_in_flight_requests`
- `sync_gateway_journal_requests_total`
- `sync_gateway_journal_request_duration_seconds`

Operational cautions:

- `root` routes are admin-level and disabled by default.
- Forwarding diagnostics omit request and response bodies entirely. There is no mode that logs raw credentials or payloads.
