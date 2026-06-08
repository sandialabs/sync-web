# Social Agent Network

Generate a local multi-node social-agent network from the reference general compose stack.

The generator expects `python3` with `PyYAML` available in the environment where you run it.

## Required Environment

- `SYNC_SERVICES_GENERAL_COMPOSE`

All other inputs default if omitted:

- `NODE_COUNT=4`
- `SECRET=password`
- `CONNECTIVITY=2`
- `PERIOD=2`
- `WINDOW=1024`
- `SIZE=32`
- `ACTIVITY=0`
- `WORDS=8`

Optional image overrides:

- `IMAGE_OVERRIDE_JOURNAL`
- `IMAGE_OVERRIDE_GATEWAY`
- `IMAGE_OVERRIDE_ROUTER`
- `IMAGE_OVERRIDE_EXPLORER`
- `IMAGE_OVERRIDE_WORKBENCH`
- `IMAGE_OVERRIDE_FILE_SYSTEM`
- `IMAGE_OVERRIDE_SOCIAL_AGENT`

## Usage

```bash
cd /code/sync-analysis/compose/social-agent-network
SYNC_SERVICES_GENERAL_COMPOSE=/code/sync-web/deploy/compose/general/compose.yaml \
python3 generate.py
docker compose up
```

## All-Local Stack

If you want the full network to use local sources from:

- `sync-journal`
- `sync-records`
- `sync-services`
- `sync-analysis`

use the helper script:

```bash
cd /code/sync-analysis/compose/social-agent-network
SYNC_JOURNAL=/path/to/sync-journal \
SYNC_RECORDS=/path/to/sync-records \
SYNC_SERVICES=/path/to/sync-services \
./local-stack.sh up
```

This does the following:

- builds a local `sync-journal` image and tags it as the journal SDK image expected by `sync-services`
- builds local service images and uses `journal-sdk` plus mounted `records/lisp` inputs for each journal
- builds a local `sync-analysis` social-agent image
- regenerates `docker-compose.yml` and `peers.json`
- starts the generated multi-node network

Useful modes:

- `./local-stack.sh build`
  - build/tag local images only
- `./local-stack.sh generate`
  - build images and regenerate compose files without starting the stack
- `./local-stack.sh up`
  - build, regenerate, and run the stack
- `./local-stack.sh down`
  - stop the generated stack and remove its volumes

Required environment:

- `SYNC_JOURNAL`
- `SYNC_RECORDS`
- `SYNC_SERVICES`

The script derives:

- `sync-records/lisp`
- `deploy/compose/general/compose.yaml`

from those repo roots.

Optional environment overrides:

- `CUSTOM_SETUP_FILE`
- `DOCKER_PLATFORM`
- `COMPOSE_PROJECT_NAME`

For custom local certificate/bootstrap setup, point `CUSTOM_SETUP_FILE` at your existing helper script before running:

```bash
cd /code/sync-analysis/compose/social-agent-network
SYNC_JOURNAL=/path/to/sync-journal \
SYNC_RECORDS=/path/to/sync-records \
SYNC_SERVICES=/path/to/sync-services \
CUSTOM_SETUP_FILE=/path/to/custom-setup.sh \
./local-stack.sh up
```

The generator writes:

- `docker-compose.yml`
- `peers.json`
- `metrics/social-agent-*/`
- `results/social-agent-*/benchmark.json`

The generated `peers.json` uses:

- `nodes`: journal-name to router-host mapping
- `edges`: deterministic FIREWHEEL-style outgoing peer adjacency

Routers expose HTTP ports starting at `8192`.
File-system WebDAV routes are exposed through each node router under `/webdav/`.
The aggregate benchmark dashboard is exposed on `8290`.
Each social agent writes Prometheus textfile metrics into its own host-mounted directory under `metrics/`.
Each social agent also writes a rolling benchmark snapshot to `results/social-agent-*/benchmark.json` with per-agent request totals, failure totals, latency averages, and current/lifetime requests-per-second estimates.
The generated compose stack also runs an `aggregate-results` sidecar that serves a simple dashboard with side-by-side successful-request-throughput and request-success-rate graphs at `http://127.0.0.1:8290/`.
