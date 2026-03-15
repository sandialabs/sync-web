# Social Agent Network

Generate a local multi-node social-agent network from the `sync-services` general compose stack.

The generator expects `python3` with `PyYAML` available in the environment where you run it.

## Required Environment

- `SYNC_SERVICES_GENERAL_COMPOSE`

All other inputs default if omitted:

- `NODE_COUNT=4`
- `SECRET=pass`
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
SYNC_SERVICES_GENERAL_COMPOSE=/code/sync-services/compose/general/docker-compose.yml \
python3 generate.py
docker compose up
```

The generator writes:

- `docker-compose.yml`
- `peers.json`
- `metrics/social-agent-*/`
- `results/social-agent-*/benchmark.json`
- `results/network-benchmark.json` when you run `aggregate_results.py`

The generated `peers.json` uses:

- `nodes`: journal-name to router-host mapping
- `edges`: deterministic FIREWHEEL-style outgoing peer adjacency

Routers expose HTTP ports starting at `8192`.
File-system services expose SMB on `445` for node `0`, then `1445`, `1446`, and so on.
Each social agent writes Prometheus textfile metrics into its own host-mounted directory under `metrics/`.
Each social agent also writes a rolling benchmark snapshot to `results/social-agent-*/benchmark.json` with per-agent request totals, failure totals, latency averages, and current/lifetime requests-per-second estimates.

To maintain a network-wide aggregate snapshot while the compose stack is running:

```bash
cd /code/sync-analysis/compose/social-agent-network
python3 aggregate_results.py
```

That writes `results/network-benchmark.json`, summing throughput counters and rates across all reporting agents and recomputing latency averages from raw latency sums/counts.
