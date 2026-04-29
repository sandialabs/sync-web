# Synchronic Web Services

Service components of the Synchronic Web monorepo:

- `gateway` for versioned web-native API routes and Swagger docs over journal interfaces
- `router` for edge routing and optional TLS termination
- `explorer` for browsing/editing journal content
- `workbench` for developer-oriented journal queries
- `file-system` for SMB projection of `/stage`, `/ledger`, and `/control`

## Quick Start

Run the compose stack from the repo root:

```bash
SECRET=password PORT=8192 tests/api/local-compose.sh up
```

Run with direct HTTP compose deployment (no TLS):

```bash
SECRET=password PORT=8192 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

Run with optional TLS (single compose file; router auto-enables TLS if cert/key files exist):

```bash
TLS_CERT_HOST_PATH=/absolute/path/to/fullchain.pem \
TLS_KEY_HOST_PATH=/absolute/path/to/privkey.pem \
SECRET=password PORT=8192 \
HTTPS_PORT=443 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

Run automated smoke validation (up, verify, down):

```bash
tests/api/local-compose.sh smoke
```

`tests/api/local-compose.sh` forces HTTP mode by default for local runs. To allow TLS behavior, set `LOCAL_COMPOSE_FORCE_HTTP=0`.

Bring down the base compose stack manually:

```bash
docker compose -f deploy/compose/general/compose.yaml down -v
```

## Documentation Map

- Compose deployment/testing docs: [../deploy/compose/general/README.md](../deploy/compose/general/README.md)
- Router service docs: [router/README.md](router/README.md)
- Explorer service docs: [explorer/README.md](explorer/README.md)
- Workbench service docs: [workbench/README.md](workbench/README.md)
- Gateway service docs: [gateway/README.md](gateway/README.md)
- File-system service docs: [file-system/README.md](file-system/README.md)
