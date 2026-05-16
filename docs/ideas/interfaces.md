# Interfaces

Sync-web exposes its journal through a constellation of protocol adapters. Each targets a
different client type. The journal itself is never exposed directly.

## MCP (Model Context Protocol)

Primary interface for AI agents and coding tools (Claude Code, Cursor, etc.).

- Transport: HTTP/SSE — no client install required; agents connect to a URL
- **Tools**: journal operations (`get`, `set!`, `batch!`, `resolve`, etc.)
- **Resources**: S7 style guide, interface.scm API reference, architecture docs, library
  index — injected into model context automatically at session start
- **Prompts**: workflow templates for common sync-web patterns
- Policy enforcement is structural: agents can only call tools the server exposes
- Session state: in-memory only (current path, env vars — ephemeral by design)
- Auth: Kratos session token or PAT via `X-Session-Token` header
- Implemented in interpreter service (Rust) alongside MCP; shares the live Scheme eval context

## WebDAV

Filesystem mounting for humans and desktop applications; replaces SMB.

- HTTP-native; simpler than SMB (no dialect negotiation, no NTLM, no handle lifecycle)
- Auth: Basic Auth over HTTPS → Kratos login → session token; OIDC/passkey users use PATs
- Implemented in storage service (Go); nginx router proxies `/webdav/`
- macOS native WebDAV client has aggressive ~30s stat cache; use rclone
  (`--vfs-cache-mode off`) to bypass it
- `Content-Length` in PROPFIND: omit rather than guess for values whose size requires a fetch

## S3

Programmatic blob access for pipelines and tools that hardcode S3.

- Maps naturally: bucket → journal root, key → path, ETag → content hash (free)
- File size limit (~1MB) is coherent; eliminates need for multipart upload
- Not for large binary assets — those belong in real S3; sync-web stores metadata/references
- Implemented in storage service (Go) alongside WebDAV

## SSH / WebSocket REPL

Terminal interface for advanced users and autonomous agents.

- SSH: zero client install; password auth → Kratos login directly (no NTLM problem);
  public key auth → key stored in Kratos `metadata_private` for OIDC/passkey users
- SSH over WebSocket via wstunnel: routes through port 443, no extra port to expose
- Custom shell: Scheme REPL + filesystem navigation; session state in-memory
- WebSocket REPL alternative: xterm.js in workbench for browser access; websocat for CLI;
  auth via standard HTTP headers on the upgrade request
- Implemented in interpreter service (Rust) alongside MCP; shares the live Scheme eval context

## SSE (Server-Sent Events)

Push notifications for connected clients (browser UIs, long-running agents).

- Embedded in gateway; ~30 lines with Fastify raw response API, no library needed
- Client subscribes with path prefix filter: `GET /api/v1/events?prefix=...`
- Value included in payload; clients filter further locally without a round-trip
- `Last-Event-ID` enables catch-up after reconnect; gateway maintains a short ring buffer
- Webhooks deferred: external systems poll ledger `size` as a cheap change signal for now

## Web Server

Experimental separate service (not part of the gateway). Tests whether HTML-on-sync-web
is a useful direction before committing it to core infrastructure.

Auth: Kratos session cookie; unauthenticated requests redirect to login (not a JSON 401).
GET-only; writes go through the workbench, API, or MCP.

JavaScript is allowed; the URL path is the source of trust that content is journal-verified.
CSP (`connect-src 'self'`) is available as an opt-in per path/subtree for users who want
to restrict JS to journal-only fetches.

### Navigation

- Directory listing when a path has children and no `index.html` — fetches children,
  renders as HTML with links; same spirit as nginx `autoindex`
- `index.html` convention: if present, served instead of the directory listing
- Explorer can embed a raw-content iframe for native browser rendering (PDF, images, video)
  without leaving the explorer context

### Versioned URLs

- `/v/47/path` serves journal content at ledger index 47
- All downstream journal fetches clamp to `ledger_index ≤ 47` for the duration of the request
- CSP `connect-src` set dynamically per response to the requested version prefix; path-level
  CSP enforcement is browser-inconsistent but `connect-src` is reliable
- Ledger URL path structure to be flattened (current nested form is unwieldy)

### Link annotation

CSP nonce pattern: server generates a nonce per response, sets
`Content-Security-Policy: script-src 'nonce-{value}'`, injects
`<script nonce="{value}">` that walks the DOM annotating sync-web links. Page-level JS
(no nonce) is blocked; injected JS runs freely.


## LSP (Language Server Protocol)

Deferred — high cost, revisit when the Scheme library ecosystem matures.

- Goal: S7-aware completions and diagnostics in editors and coding agents
- Near-term substitute: rich MCP Resources (style guide, API reference, architecture docs)
  achieve the same goal at a fraction of the cost
