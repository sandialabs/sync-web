# Synchronic Web Services

Service components of the Synchronic Web monorepo:

- `gateway` for versioned web-native API routes and Swagger docs over journal interfaces
- `router` for edge routing and optional TLS termination
- `explorer` for browsing/editing journal content
- `workbench` for developer-oriented journal queries
- `file-system` for WebDAV projection of `/stage`, `/ledger`, and `/control`

## Quick Start

Run the compose stack from the repo root:

```bash
COMPOSE_PROJECT_NAME=sync-local SECRET=password \
HTTP_PORT=8192 HTTPS_PORT=8193 \
tests/api/local-compose.sh up
```

Run with direct HTTP compose deployment (no TLS):

```bash
COMPOSE_PROJECT_NAME=sync-dev SECRET=password \
HTTP_PORT=8192 HTTPS_PORT=8193 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

Run with optional TLS (single compose file; router auto-enables TLS if cert/key files exist):

```bash
COMPOSE_PROJECT_NAME=sync-prod \
TLS_CERT_HOST_PATH=/absolute/path/to/fullchain.pem \
TLS_KEY_HOST_PATH=/absolute/path/to/privkey.pem \
SECRET=password ORIGIN=https://example.com HTTP_PORT=80 HTTPS_PORT=443 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

Run automated smoke validation (up, verify, down):

```bash
tests/api/local-compose.sh smoke
```

`tests/api/local-compose.sh` forces HTTP mode by default for local runs. To allow TLS behavior, set `LOCAL_COMPOSE_FORCE_HTTP=0`.

The compose journal service runs the generic `journal-sdk` image directly and mounts
the general Lisp deployment inputs from `records/lisp` plus
`deploy/compose/general/run.sh`. There is no separate `general` image layer.

Bring down a stack without deleting data:

```bash
COMPOSE_PROJECT_NAME=sync-dev docker compose -f deploy/compose/general/compose.yaml down
```

Only add `-v` when you intentionally want to delete that stack's database and identity-provider volumes.

## Documentation Map

- Compose deployment/testing docs: [../deploy/compose/general/README.md](../deploy/compose/general/README.md)
- Router service docs: [router/README.md](router/README.md)
- Explorer service docs: [explorer/README.md](explorer/README.md)
- Workbench service docs: [workbench/README.md](workbench/README.md)
- Gateway service docs: [gateway/README.md](gateway/README.md)
- File-system service docs: [file-system/README.md](file-system/README.md)
