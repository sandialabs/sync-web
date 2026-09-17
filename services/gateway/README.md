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
  - `POST /api/v1/general/put` -> journal `put!`
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

- Use keyword-style argument object fields directly (for example `{ "path": ... }` for staged reads or `{ "path": ..., "pinned?": true, "proof?": true }` for committed/indexed `retrieve` calls). `set`, `set-batch`, `copy`, and `copy-batch` accept optional target `expected` values: every expected staged value must match or the operation returns `false` without writing. Copy operations use `source`/`sources` plus target `path`/`paths`, preserve raw file or directory content, and require source read plus target write authorization; conditional operations additionally require target read authorization.

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
- `application/scheme` requests to only `put`, `use`, and `retrieve` may carry `X-Sync-Web-Federation-Route`. Its value is exactly one nonempty JSON array of alias strings and maps to the existing `$federation.route` invocation semantics. Alias strings are exact Scheme symbols; cooperating applications percent-encode unsafe user-facing names before placing them here. The same s7 reader projection requires one exact collection of unique operation fields before these values are wrapped. Empty arrays, malformed or repeated headers, wrong content types, other operations, and ambiguous argument pairs are rejected. The header carries no credentials or history indexes.

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

### Standalone Raw

- `GET /api/v1/raw?selection=TOKEN` — session-authenticated exact-byte projection for an Explorer-generated canonical Stage or concrete Ledger selection.

Raw URLs contain only a bounded typed content identifier. The 4,096-character token cap leaves the complete request line below the Router's HTTP and TLS single-buffer boundary; Explorer suppresses links that exceed the same token, payload, route, path, or segment limits. They carry no credential or capability, recheck existing Journal authorization on every request, and never create a Stage snapshot or implicit pin. Valid UTF-8 is served as inert plain text, allowlisted raster images may display inline, and unproven media or unknown binary downloads with strict `nosniff`, sandbox CSP, no-referrer, and no-store headers.

### General

- `GET /api/v1/general/size` (public)
- `GET /api/v1/general/info` (public)
- `POST /api/v1/general/use`
- `POST /api/v1/general/use-batch!`
- `POST /api/v1/general/put`
- `POST /api/v1/general/copy`
- `POST /api/v1/general/copy-batch`
- `POST /api/v1/general/truncate`
- `POST /api/v1/general/pin`
- `POST /api/v1/general/pin-batch`
- `POST /api/v1/general/unpin`
- `POST /api/v1/general/unpin-batch`
- `POST /api/v1/general/prune`
- `POST /api/v1/general/prune-batch`
- `POST /api/v1/general/run`
- `POST /api/v1/general/put-batch`
- `POST /api/v1/general/synchronize!` (public reciprocal exchange)
- `POST /api/v1/general/retrieve`
- `POST /api/v1/general/retrieve-batch`
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

- `admins` calls `*admins-get*` and returns `null` when empty or a username-keyed object such as `{"alice":["*state*","alice"]}`.
- `set-admins` calls `*admins-set*` and atomically replaces the list from that username-keyed object; every exact key must match its local `[*state*, <name>]` principal value.
- `set-window` calls `*window-set*` and updates the public ledger retention window.
- `truncate` irreversibly releases locally available committed history through an inclusive index. It is limited to Root/configured local Interface administrators, preserves logical chain identity and future appends, and cannot recall peer/client/backup copies.
- `prune` and `prune-batch` remove selected canonical committed leaves/directories from both temporary and permanent retention as Root/configured local Interface administrators. A batch installs both complete retained-field candidates or neither; Stage and history identity remain unchanged. These operations make no secure-erasure claim.
- `bridge` creates a reciprocal relationship from `name`, `interface`, and `remote-name`.
- `update-config` manages explicit ledger configuration such as `(public bridge-accept)` and `(private bridge-preapproval <name>)`.
- Authorization routes are Self-local. `user` is the owner's local `[*state*, USER]` namespace and `rule.path` is owner-relative. A remote exact bridge principal requires a terminal authentication `key-index` such as `[-32, -1]`; exact local/public principals omit it. Retrieve is independent and uses `true`, `false`, or a document-history range such as `[0, -1]`. The complete stored rule must be sent unchanged to `deauthorize`.
- `call` asks Interface to load the current staged Scheme procedure from `path`, evaluate it outside `sync-let` in an Interface-owned masked environment, and apply it to an inherited authenticated journal capability followed by the explicit `arguments` list. Root/configured local administrators retain default access; authenticated local and federated principals require an independent path-scoped `run!` rule. Namespace ownership and blank read-only `use!` do not imply execution.
- Staged JSON blank read-only `use!`, `set`, `use-batch!`, `set-batch`, and `call` calls accept `$federation: { route: [<aliases>] }`; one dedicated batch uses one exact working route and signs its complete ordered arguments. `retrieve` and `retrieve-batch` also accept that same route to select one terminal responder. Their committed paths are interpreted locally at the responder and succeed only from its permanent retention; a trailing bridge alias returns its structural Chain inventory, an exact following integer selects a payload, and continued traversal implies `-1`. No provider search or outward history field exists. Canonical committed paths without `$federation` retain ordinary grouped resolution. Interface preserves result order and verified proofs. Optional `pinned?` remains an origin-local status report and is forced false on the signed wire. Pin/unpin and all administration remain local mutations at the origin journal; `pin-batch!` fetches every remote proof before its one atomic retention mutation. Batch calls accept at most 1,024 paths; existing transport body, response, and timeout bounds still apply.

Dedicated batch routes preserve duplicates and request order. Missing content is returned as the Journal `(nothing)` sentinel. `set` and `set-batch` distinguish an omitted `expected` field from explicit `false`/`#f`; batch expectations compare against one snapshot, cardinalities must match, and a conflict returns `false` without a transition. Conditional writes require both read and write authority. Empty data/retention batches are identity operations, including `prune-batch`; duplicate and overlapping prune paths equal their union, while `trace-batch` needs at least one same-anchor path. There is no arbitrary mixed `/general/batch` route.

`retrieve` and `retrieve-batch` accept optional boolean `index?`. With `true`, every successful result includes an `indexes` array containing the absolute origin and per-hop indexes selected for the request's integer selectors. Scalar raw content is wrapped under `content`; each batch result carries its own indexes while retaining the original path, order, and duplicates. Absent or false preserves the previous response shape exactly. This reports committed read selection only: it does not add a committed index to staged writes, wait for commits, or change proof bytes.

```json
POST /api/v1/general/put-batch
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
    "put!": false,
    "use!": {"read-only?": true},
    "run!": false,
    "retrieve": [0, -1]
  }
}
```

Restricted JSON call:

```bash
curl -X POST http://127.0.0.1:8180/api/v1/general/use \
  -H "Authorization: Bearer sync-<uuid>-<key-id>-0-<secret>" \
  -H "Content-Type: application/json" \
  -d '{"path":["*state*","docs","article","hash"],"read-only?":true}'
```

Retained committed read from one explicitly selected responder:

```bash
curl -X POST http://127.0.0.1:8180/api/v1/general/retrieve \
  -H "Authorization: Bearer sync-<uuid>-<key-id>-0-<secret>" \
  -H "Content-Type: application/json" \
  -d '{
    "path": [12,"archive",7,"*state*","bob","shared","message"],
    "pinned?": true,
    "proof?": true,
    "$federation": {
      "route": ["retention-provider"]
    }
  }'
```

The origin verifies the returned proof against `retention-provider`; that responder must already hold the exact `archive` path in `perm` and never contacts the attributed source. `pinned?` reports only whether the requester also retained the returned path locally. To retain a new remote proof at the requester, send it to the local `pin` endpoint with no `$federation` context.

Restricted Scheme call:

```bash
curl -X POST http://127.0.0.1:8180/api/v1/general/use \
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
