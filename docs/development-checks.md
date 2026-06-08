# Development Checks and Dependencies

This repo has several independent validation layers. Run the checks relevant to the area you changed.

## Core journal and records

- Rust journal tests
  - Path: `journal/`
  - Requires: Rust toolchain with Cargo, C build dependencies
  - Command: `cargo test`
- Scheme record tests
  - Path: repo root
  - Requires: built `journal-sdk` binary
  - Command: `./records/tests/test.sh ./journal/target/debug/journal-sdk`

## Services

- Gateway tests
  - Path: `services/gateway/`
  - Requires: Node.js/npm
  - Command: `npm test`
- Explorer tests
  - Path: `services/explorer/`
  - Requires: Node.js/npm
  - Command: `npm test -- --watchAll=false`
- Workbench tests
  - Path: `services/workbench/`
  - Requires: Node.js/npm
  - Command: `npm test -- --watchAll=false`
- File-system/WebDAV tests
  - Path: `services/file-system/`
  - Requires: Go
  - Command: `go test ./...`

## Integrated local stack

- Single-node compose smoke
  - Path: repo root
  - Requires: Docker Compose, Podman Compose, or `podman-compose`; `curl`; Python 3 for smoke helpers
  - Command: `COMPOSE_PROJECT_NAME=sync-local SECRET=password tests/api/local-compose.sh smoke`
  - Notes: builds local images, starts the general compose stack, runs API/WebDAV checks, and tears the stack down.

- Container image test targets
  - Path: repo root
  - Requires: Docker or Podman compatible build command
  - Command: `CONTAINER_RUNTIME=podman tests/test.sh` or `CONTAINER_RUNTIME=docker tests/test.sh`

## Load and network tests

- Locust unit tests
  - Path: `tests/load/locust/`
  - Requires: Python 3
  - Command: `python -m unittest discover -s tests`
- Locust load run
  - Path: `tests/load/locust/`
  - Requires: Python 3, Locust dependencies, running gateway, API token
  - Example: `API_TOKEN=sync-... locust --host=http://localhost:8192`
- Social-agent unit tests
  - Path: `tests/network/common/social-agent/`
  - Requires: Python 3
  - Command: `python -m unittest discover -s tests`
- Multi-node compose harness
  - Path: `tests/network/compose/`
  - Requires: Compose-compatible runtime, Python 3, PyYAML
  - Commands: `tests/network/compose/local-compose.sh generate`, then run compose from `tests/network/compose/`
- FIREWHEEL harness
  - Path: `tests/network/firewheel/`
  - Requires: FIREWHEEL and Docker-specific model-component support
  - Notes: this harness is intentionally Docker-specific and is not part of the baseline Compose-compatible workflow.

## Browser/manual checks

- Playwright is useful for local manual UI sanity checks, but is not part of the Explorer unit-test or CI setup unless explicitly added later.
- For ad hoc scripts that `require('playwright')`, set `NODE_PATH=$(npm root -g)` if using the globally installed package.
