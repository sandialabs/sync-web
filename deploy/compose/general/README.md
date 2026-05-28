# Synchronic Web Ledger Compose Network

This compose stack runs one journal with gateway, explorer, workbench, router, and the `file-system` SMB service.

The journal service uses the generic `journal-sdk` image directly. The general
interface is assembled at startup from mounted deployment inputs:

- `records/lisp/*.scm`
- `deploy/compose/general/run.sh`
- the persistent `database` volume

For a fresh database, `run.sh` installs the general interface from the mounted Lisp
files. For an existing database, mounted Lisp files are only applied when
`JOURNAL_UPDATE=1`; otherwise the durable journal state continues from the database.

## Requirements

- Docker
- Docker Compose
- Podman Compose on Fedora is also supported. The bind mounts use SELinux
  relabel flags so rootless Podman can read the mounted startup script and Lisp
  files.

## Configuration

- `SECRET` (required): authentication secret for restricted journal/gateway operations
- `PORT` (default `8192`): host HTTP port exposed by router
- `HTTPS_PORT` (default `443`): host TLS port exposed by router
- `PERIOD` (default `2`): journal periodicity exponent
- `WINDOW` (default `1024`): retained historical state window
- `JOURNAL_UPDATE` (default empty): set to `1` to update an existing journal database from the mounted Lisp files before serving
- `TLS_CERT_HOST_PATH` (default `./tls/tls.crt`): host certificate file mounted into router
- `TLS_KEY_HOST_PATH` (default `./tls/tls.key`): host key file mounted into router
- `ACME_WEBROOT_HOST_PATH` (default `./acme-challenge`): host directory mounted at `/var/www/acme-challenge` for HTTP-01 challenge files
- `TLS_CERT_FILE` (default `/etc/nginx/certs/tls.crt`): in-container certificate path used by router
- `TLS_KEY_FILE` (default `/etc/nginx/certs/tls.key`): in-container key path used by router
- `SMB_PORT` (default `445`): host port exposed by the `file-system` service
- `FILE_SYSTEM_IMAGE` (default `ghcr.io/sandialabs/sync-web/file-system:1.0.0`): image used by the optional `file-system` service
- `SYNC_FS_Backend` (default `http-journal-stage`): file-system backend override
- `SYNC_FS_JournalJsonUrl` (default `http://journal/interface`): direct journal JSON endpoint used by the default file-system backend
- `SYNC_FS_GatewayBaseUrl` (default `http://gateway/api/v1`): gateway endpoint used only when gateway-backed file-system modes are selected

Gateway note:
- `ALLOW_ADMIN_ROUTES` is enabled by default in `deploy/compose/general/compose.yaml`.
- Gateway landing page is exposed through the router at `/gateway`.
- Public/client-facing API traffic should go to `gateway` under `/api/v1/general/*` and `/api/v1/root/*`.
- The raw `/interface` endpoint is still present for direct journal transport use and bridge-oriented internals.
- The journal's periodic scheduler uses the raw root-step call `(*step* "<secret>")`, which depends on the merged `records/` root-step pipeline.

## TLS Behavior

This stack uses one compose file. Router auto-selects mode at startup:

- HTTP mode: if TLS cert/key files are not present
- TLS mode: if both `TLS_CERT_FILE` and `TLS_KEY_FILE` exist

`tests/api/local-compose.sh` forces HTTP mode by default (`LOCAL_COMPOSE_FORCE_HTTP=1`) for predictable local smoke runs.
Set `LOCAL_COMPOSE_FORCE_HTTP=0` if you explicitly want TLS behavior during local-compose execution.

In TLS mode, router serves:
- `80` for ACME HTTP-01 challenge path and HTTPS redirects
- `443` for proxied application routes

This is certificate-provider agnostic. Only file paths are required.

HTTP-only deployment (no TLS files configured):

```bash
SECRET=password PORT=8192 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

With Podman Compose:

```bash
SECRET=password PORT=8192 \
podman-compose -f deploy/compose/general/compose.yaml up -d
```

The default compose stack now mounts ACME webroot to `/var/www/acme-challenge`.
Use `ACME_WEBROOT_HOST_PATH` to override where challenge files come from on the host.

Example:

```bash
TLS_CERT_HOST_PATH=/absolute/path/to/fullchain.pem \
TLS_KEY_HOST_PATH=/absolute/path/to/privkey.pem \
SECRET=password PORT=8192 \
HTTPS_PORT=443 \
docker compose -f deploy/compose/general/compose.yaml up -d
```

## Local Runner

Use the local helper from repository root:

```bash
# Interactive run
tests/api/local-compose.sh up

# Smoke test
tests/api/local-compose.sh smoke
```

The local compose helper enables the SMB file-system service by default.

The helper builds a local `journal-sdk` image and uses the same mounted Lisp/runtime
script layout as the reference compose stack. It no longer builds a separate
`general` image.

To override the file-system image during local development:

```bash
FILE_SYSTEM_IMAGE=sync-web/local-file-system:1.0.0 tests/api/local-compose.sh up
FILE_SYSTEM_IMAGE=sync-web/local-file-system:1.0.0 tests/api/local-compose.sh smoke
```

## Manual Teardown

```bash
docker compose -f deploy/compose/general/compose.yaml down -v
```
