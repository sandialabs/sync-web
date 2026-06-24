# Synchronic Web Ledger Compose Network

This compose stack runs one journal with gateway, explorer, workbench, router, identity-provider, and the WebDAV `file-system` service.

The journal service uses the generic `journal-sdk` image directly. The general interface is assembled at startup from mounted deployment inputs:

- `records/lisp/*.scm`
- `deploy/compose/general/run.sh`
- the persistent `database` volume

For a fresh database, `run.sh` installs the general interface from the mounted Lisp files. For an existing database, mounted Lisp files are only applied when `JOURNAL_UPDATE=1`; otherwise the durable journal state continues from the database.

## Requirements

- A Compose-compatible container runtime: Docker Compose, Podman Compose, or `podman-compose`
- `curl` for the local smoke helper

The bind mounts use SELinux relabel flags (`:Z`) so rootless Podman can read the mounted startup script and Lisp files.

## Stack identity and ports

Use `COMPOSE_PROJECT_NAME` to isolate local, public, and experimental stacks. Container and network names are prefixed from this value, for example `sync-dev-journal` and `sync-prod-gateway`.

Default local ports are adjacent and non-privileged:

- `HTTP_PORT` default `8192`
- `HTTPS_PORT` default `8193`

Example local stack:

```bash
COMPOSE_PROJECT_NAME=sync-dev SECRET=password \
HTTP_PORT=8192 HTTPS_PORT=8193 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

Example public stack using standard ports:

```bash
COMPOSE_PROJECT_NAME=sync-prod SECRET=password \
ORIGIN=https://example.com HTTP_PORT=80 HTTPS_PORT=443 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

With Podman Compose:

```bash
COMPOSE_PROJECT_NAME=sync-dev SECRET=password \
HTTP_PORT=8192 HTTPS_PORT=8193 \
podman-compose -f deploy/compose/general/compose.yaml up -d
```

## Configuration

- `COMPOSE_PROJECT_NAME` (recommended): stack/project identity used for generated volumes and parameterized container/network names
- `SECRET` (required): authentication secret for restricted journal/gateway operations
- `HTTP_PORT` (default `8192`): host HTTP port exposed by router
- `HTTPS_PORT` (default `8193`): host TLS port exposed by router
- `ORIGIN` (default `http://localhost:8192`): public origin used by the identity provider; set this explicitly when using non-default/public ports or hostnames
- `PERIOD` (default `2`): journal periodicity exponent
- `WINDOW` (default `1024`): retained historical state window
- `JOURNAL_UPDATE` (default empty): set to `1` to update an existing journal database from the mounted Lisp files before serving
- `TLS_CERT_HOST_PATH` (default `./tls/tls.crt`): host certificate file mounted into router
- `TLS_KEY_HOST_PATH` (default `./tls/tls.key`): host key file mounted into router
- `ACME_WEBROOT_HOST_PATH` (default `./acme-challenge`): host directory mounted at `/var/www/acme-challenge` for HTTP-01 challenge files
- `TLS_CERT_FILE` (default `/etc/nginx/certs/tls.crt`): in-container certificate path used by router
- `TLS_KEY_FILE` (default `/etc/nginx/certs/tls.key`): in-container key path used by router
- `FILE_SYSTEM_IMAGE` (default `ghcr.io/sandialabs/sync-web/file-system:1.4.0`): image used by the `file-system` service
- `SYNC_FS_MAX_OBJECT_BYTES` (default `1048576`): maximum WebDAV object size

Gateway note:

- `ALLOW_ADMIN_ROUTES` is enabled by default in `deploy/compose/general/compose.yaml`.
- Gateway landing page is exposed through the router at `/gateway`.
- Public/client-facing API traffic should go to `gateway` under `/api/v1/general/*` and `/api/v1/root/*`.
- WebDAV traffic is exposed through the router under `/webdav/`.
- The raw `/interface` endpoint is still present for direct journal transport use and bridge-oriented internals.
- The journal's periodic scheduler uses the raw root-step call `(*step* "<secret>")`, which depends on the merged `records/` root-step pipeline.

## TLS behavior

This stack uses one compose file. Router auto-selects mode at startup:

- HTTP mode: if TLS cert/key files are not present
- TLS mode: if both `TLS_CERT_FILE` and `TLS_KEY_FILE` exist

`tests/api/local-compose.sh` forces HTTP mode by default (`LOCAL_COMPOSE_FORCE_HTTP=1`) for predictable local smoke runs. Set `LOCAL_COMPOSE_FORCE_HTTP=0` if you explicitly want TLS behavior during local-compose execution.

In TLS mode, router serves:

- container port `80` for ACME HTTP-01 challenge path and HTTPS redirects
- container port `443` for proxied application routes

The host ports are controlled by `HTTP_PORT` and `HTTPS_PORT`.

The default compose stack mounts ACME webroot to `/var/www/acme-challenge`. Use `ACME_WEBROOT_HOST_PATH` to override where challenge files come from on the host.

Example TLS-backed public stack:

```bash
COMPOSE_PROJECT_NAME=sync-prod \
TLS_CERT_HOST_PATH=/absolute/path/to/fullchain.pem \
TLS_KEY_HOST_PATH=/absolute/path/to/privkey.pem \
SECRET=password ORIGIN=https://example.com HTTP_PORT=80 HTTPS_PORT=443 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

## Local runner

Use the local helper from repository root:

```bash
# Interactive run
COMPOSE_PROJECT_NAME=sync-local tests/api/local-compose.sh up

# Smoke test
COMPOSE_PROJECT_NAME=sync-local tests/api/local-compose.sh smoke
```

The local compose helper defaults to `COMPOSE_PROJECT_NAME=sync-local`, `HTTP_PORT=8192`, and `HTTPS_PORT=8193`. It builds local service images, enables the WebDAV file-system service by default, and uses the same mounted Lisp/runtime script layout as the reference compose stack.

To override the file-system image during local development:

```bash
FILE_SYSTEM_IMAGE=sync-web/local-file-system:1.0.0 tests/api/local-compose.sh up
FILE_SYSTEM_IMAGE=sync-web/local-file-system:1.0.0 tests/api/local-compose.sh smoke
```

## Teardown

Stop a stack without deleting data:

```bash
COMPOSE_PROJECT_NAME=sync-dev docker compose -f deploy/compose/general/compose.yaml down
```

Delete stack volumes only when you intentionally want to remove the journal database and identity-provider state:

```bash
COMPOSE_PROJECT_NAME=sync-dev docker compose -f deploy/compose/general/compose.yaml down -v
```

Do not run `down -v` against a public/prod `COMPOSE_PROJECT_NAME` unless you intend to destroy that stack's persisted data.
