# Synchronic Router

Nginx edge router for the Synchronic compose stack.

## Runtime Modes

- HTTP mode (default): serves routes on port `80`
- TLS mode (automatic): enabled when both cert and key files exist

Router checks these files at startup:

- `TLS_CERT_FILE` (default `/etc/nginx/certs/tls.crt`)
- `TLS_KEY_FILE` (default `/etc/nginx/certs/tls.key`)

If both files exist, router uses TLS config with:

- `80` for ACME HTTP-01 path + HTTPS redirect
- `443` for proxied application routes

If either file is missing, router uses HTTP-only config.

## Routed Paths

Canonical public paths:
- `/api/` -> gateway API
- `/auth/` -> gateway auth UI and Ory proxy
- `/gateway` -> gateway landing page
- `/docs` -> gateway docs
- `/healthz` -> gateway health probe
- `/readyz` -> gateway readiness probe
- `/metrics` -> `404` (Prometheus metrics are internal-only)
- `/explorer` -> explorer UI
- `/workbench` -> workbench UI
- `/webdav/` -> file-system WebDAV service

Compatibility/internal path retained for bridge communication:
- `/interface` -> raw journal Scheme interface

`/interface` is intentionally kept for existing journal-to-journal flows such as social-agent bridge wiring, but it is not the preferred public integration surface.

Prometheus should scrape `http://gateway/metrics` from the private Compose network. The gateway has no host-published port, and the router returns `404` for exact public `/metrics` requests in HTTP and TLS modes. Public Gateway routes use the container network's DNS resolver with a five-second cache so an unchanged Router automatically follows a restarted Gateway at its new address.

Request body limit:
- router currently accepts up to `64 MiB` bodies
