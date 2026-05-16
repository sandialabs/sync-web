# Changelog

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
