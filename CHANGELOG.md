# Changelog

## 1.4.1

### Added

- **Windows ARM64 binaries** — Add Windows ARM64 builds for `ledger`, `journal-sdk`, and `agent-recorder` release artifacts.

### Changed

- **Journal SDK patch version** — Bump `journal-sdk` to `1.3.1` because the journal crate dependency configuration changed.
- **Journal dependency build scope** — Disable unused default `pqcrypto` algorithm features so Windows ARM64 builds avoid compiling unused PQClean C implementations.

## 1.4.0

### Added

- **Lightweight ledger binary** — Added a user-facing `ledger` executable for local and agent-recorder-oriented deployments without the full gateway/explorer/identity/router stack. The binary embeds core ledger records, initializes empty databases, runs periodic steps, supports record updates, bridge configuration, generated local secrets, and direct query evaluation.
- **Ledger release artifacts** — Added a `Ledger Binaries` workflow that builds downloadable `ledger-*` and `journal-sdk-*` artifacts for Linux, Linux musl/Alpine, macOS, and Windows targets. Versioned tag builds publish release assets from a final publish job with `SHA256SUMS`.
- **Ledger deployment docs** — Added lightweight ledger deployment notes and initial binary/Compose documentation.

### Changed

- **Explorer stage tree** — The staged-state tree now shows all readable stage users while keeping the current user’s staged home as the default view.
- **Platform package metadata** — Added tooling to keep platform-versioned TypeScript package metadata aligned with top-level `VERSION`.

### Fixed

- **Ledger bridge tracing** — Preserve document metadata through remote bridge trace results.
- **Ledger public config** — Restrict public configuration exposure to intended public fields.
- **Log-chain truncation** — Ensure `truncate!` mutates log-chain state correctly.
- **Ledger signing keys** — Derive ledger signing keys at step time instead of storing private signing keys in ledger config.
- **Docs 404 page** — Added a custom docs 404 page and disabled Starlight’s injected 404 route to avoid build warnings.
- **Windows ledger builds** — Patch vendored s7 at build time for Windows/MSVC builds without modifying vendored s7 sources.

## 1.3.2

### Fixed

- **Explorer path handling** — Parse ledger directory entries as `[name, type]` pairs, preserve encoded path segments for API calls, and display decoded names for files and directories with spaces or other escaped characters.
- **Release promotion** — Avoid expensive main-branch validation rebuilds for versioned releases by promoting branch-validated SHA images.
- **WebDAV docs** — Clarified API-token authentication, service-side path escaping, and Windows client limitations.

## 1.3.1

### Fixed

- **WebDAV compatibility** — Added minimal lock responses for clients that require class-2 WebDAV behavior, including macOS Finder.
- **WebDAV path handling** — Percent-encode WebDAV names that are not unescaped R7RS identifiers before ledger calls and decode them for directory listings.
- **Ledger browsing** — Accept synthetic ledger collection probes without trailing slashes for file managers that normalize paths.
- **WebDAV authentication docs** — Clarified that WebDAV Basic Auth uses a Sync Web API token as the password, not the account password.

## 1.3.0

### Added

- **Document records** — Added byte-vector-only document payloads with metadata support and explicit `expression?` public I/O codec for expression-oriented callers.
- **WebDAV file-system service** — Replaced the previous file-system adapter with a gateway-backed WebDAV service and navigable ledger index paths.
- **Gateway events** — Added authenticated Server-Sent Events change hints and Explorer live refresh integration.
- **Router splash page** — Added a static journal node home page and WebDAV guide served directly by the router.

### Changed

- **Record paths** — Public ledger/interface paths now use flat path syntax for staged, indexed, and bridge traversal paths.
- **Document adapters** — Explorer, WebDAV, gateway examples, social-agent, and load workloads now write normal document content as byte-vectors unless explicitly using `expression?`.
- **Pinned state** — `pinned?` now reports local permanent proof value availability; confirmed values including `(nothing)` are pinned, while `(unknown)` is not.
- **Compose defaults** — All service image tags default to `1.3.0`.

### Fixed

- **Bridge resolution** — Historical multi-hop bridge paths now return semantic values/unknowns instead of low-level sync-node access errors.
- **Explorer stability** — Preserved active staged edits during background refreshes and fixed bridged file pin/unpin display.
- **Batch JSON** — Normalized JSON batch subqueries and decoded batch results consistently.
- **Directory discovery** — Explorer directory discovery uses content-only reads to avoid unnecessary proof/pin coupling.

## 1.2.0

### Added

- **Identity provider** — Ory Kratos service for username/password login and registration. Username-only identity schema; no email required. Admin identity seeded on startup via `ADMIN_USERNAME` / `ADMIN_PASSWORD` environment variables.
- **Gateway authentication** — Kratos session cookies validated at the gateway boundary; unauthenticated requests rejected with 401. API tokens (`Authorization: Bearer sync-...`) accepted for machine/automation callers. Resolved identity forwarded to all journal calls.
- **Auth UI** — Login, registration, and account settings pages served at `/auth/...` by the gateway, consuming Kratos self-service flows.
- **User namespaces** — Each user may only write to `(*state* <username> ...)` paths. Cross-namespace writes are rejected with a permissions error.
- **Private paths** — Paths containing `*private*` (e.g. `(*state* alice *private* ...)`) are readable only by the owning user.
- **Admin list** — `*admins-get*` and `*admins-set*` operations manage the admin list. Admins bypass all path restrictions. `INTERFACE_ADMINS` environment variable seeds the list on startup.
- **Restricted operations** — `bridge!`, `*secret*`, `*admins-get*`, and `*admins-set*` are restricted to admins; regular users receive a `permissions-error`.
- **Explorer session awareness** — Explorer detects and surfaces the logged-in user identity in the toolbar.

### Changed

- **Authentication envelope** — Canonicalized to `(identity *journal*)` for root and a bare symbol (e.g. `alice`) for named users. The previous `(self)` and list-wrapped forms are no longer used.
- **File system / SMB** — Gateway-backed path removed; the file-system service now authenticates directly to the journal using a bearer token. `HttpGatewayClient` removed.
- **Router** — nginx routes extended to cover `/auth/`, explorer, workbench, gateway docs, and API under both HTTP and TLS configurations.
- **Compose defaults** — All service image tags default to `1.2.0`.

## 1.0.0

Initial release. Monorepo consolidation of journal, records, services, and analysis repositories.
