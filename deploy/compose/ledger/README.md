# Minimal ledger compose stack

This stack runs a single Sync Web journal with the ledger interface installed. It is intended for local/simple deployments and tools such as `agent-recorder` that only need the raw journal ledger endpoint, not gateway, explorer, identity, router, or WebDAV services.

## Start

```sh
cd deploy/compose/ledger
cp .env.example .env
$EDITOR .env   # set independent SECRET and INTERFACE_SECRET values

docker compose up -d
# or: podman compose up -d
# or: podman-compose up -d
```

The journal listens on `${JOURNAL_PORT:-8192}` and exposes the raw interface at:

```text
http://localhost:${JOURNAL_PORT:-8192}/interface
```

Use a distinct `COMPOSE_PROJECT_NAME` per deployment so databases, containers, and networks do not collide.

## Installed records

Sync Web 1.6 supports exactly one in-place transition from a version-marked
1.5.0 database. Set `JOURNAL_UPDATE=1` for that transition only; the Interface
preserves history, derived keys, relationships, and authorization meaning, and
fails atomically for incompatible required state. Installation accepts only the
exact successful Scheme result. Process errors, Scheme error values, and malformed
results exit nonzero before writing or advancing the version marker or launching
the server, and diagnostics omit result and secret content. With
`JOURNAL_UPDATE=0`, a correctly version-marked database reopens normally without
reinstallation or startup interpretation of mounted record source. Later starts
still reject missing or unsupported markers.

## Data

The journal database is stored in the Compose volume named `${COMPOSE_PROJECT_NAME:-ledger}-database`.

Avoid `down -v` unless you intentionally want to delete the ledger database.
