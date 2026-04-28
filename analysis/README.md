# sync-analysis
Synchronic Web Analysis

This project provides analysis tools and simulation components for the Synchronic Web, a distributed ledger system. It includes FIREWHEEL model components for network simulation and Locust-based load testing.

## Components

### FIREWHEEL Model Components

- **general-journal**: Creates journal nodes that maintain distributed ledger state with configurable periodicity and secrets
- **network-monitor**: Provides monitoring and observability using Prometheus and Grafana for real-time network metrics and visualization
- **social-agent**: Simulates social agents that interact with the ledger system, with configurable connectivity, size, and activity parameters

### Load Testing

- **locust**: HTTP load testing using Locust to simulate concurrent users interacting with the ledger system
- **compose/social-agent-network**: local Docker Compose harness generator for multi-node social-agent networks based on the `sync-services` general stack

### Container Runtime Assets

- **docker/social-agent**: canonical social-agent container runtime shared by FIREWHEEL publishing and local compose generation

## Usage

The FIREWHEEL components can be used to create network topologies with journal nodes and social agents for testing distributed ledger behavior at scale. The compose harness under `compose/social-agent-network/` provides a local Docker-only alternative that reuses the `sync-services` general stack with one full service cluster and one social agent per node. The Locust tests provide performance analysis of the system under load.

### Local Compose Social-Agent Network

Use `compose/social-agent-network/generate.py` to derive a local multi-node stack from `sync-services/compose/general/docker-compose.yml`.

Required input:

- `SYNC_SERVICES_GENERAL_COMPOSE`

Defaulted inputs:

- `NODE_COUNT=4`
- `SECRET=pass`
- `CONNECTIVITY=2`
- `PERIOD=2`
- `WINDOW=1024`
- `SIZE=32`
- `ACTIVITY=0`
- `WORDS=8`

Basic flow:

```bash
cd /code/sync-analysis/compose/social-agent-network
SYNC_SERVICES_GENERAL_COMPOSE=/code/sync-services/compose/general/docker-compose.yml \
python3 generate.py
docker compose up
```

Generated outputs include:

- `docker-compose.yml`
- `peers.json`
- `metrics/social-agent-*/` host directories for social-agent Prometheus textfile output
- `results/social-agent-*/benchmark.json` host files for simple per-agent throughput snapshots
- `results/network-benchmark.json` when `compose/social-agent-network/aggregate_results.py` is running

## Requirements

- FIREWHEEL simulation framework
- Docker (for containerized components)
- Python 3.x with Locust for load testing
