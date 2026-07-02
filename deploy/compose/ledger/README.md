# Minimal ledger compose stack

This stack runs a single Sync Web journal with the ledger interface installed. It is intended for local/simple deployments and tools such as `agent-recorder` that only need the raw journal ledger endpoint, not gateway, explorer, identity, router, or WebDAV services.

## Start

```sh
cd deploy/compose/ledger
cp .env.example .env
$EDITOR .env   # set SECRET

docker compose up -d
# or: podman compose up -d
# or: podman-compose up -d
```

The journal listens on `${JOURNAL_PORT:-8192}` and exposes the raw interface at:

```text
http://localhost:${JOURNAL_PORT:-8192}/interface
```

Use a distinct `COMPOSE_PROJECT_NAME` per deployment so databases, containers, and networks do not collide.

## Update installed records

After changing Scheme records, restart with:

```sh
JOURNAL_UPDATE=1 docker compose up -d
```

## Data

The journal database is stored in the Compose volume named `${COMPOSE_PROJECT_NAME:-ledger}-database`.

Avoid `down -v` unless you intentionally want to delete the ledger database.
