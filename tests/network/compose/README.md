# Social Agent Network

Generate a local multi-node Sync Web network from the primary general compose stack. The default topology has four journals and reciprocal bridges; it is intended for federation and service-boundary testing, not as a lightweight unit-test fixture.

## Prerequisites

- a standard container runtime with Compose support (Docker or Podman);
- Python 3 with PyYAML;
- local source under this repository when using `local-compose.sh`.

## Defaults

The generator accepts environment overrides and otherwise uses:

- `NODE_COUNT=4`
- `SECRET=password`
- `CONNECTIVITY=2`
- `PERIOD=2`
- `WINDOW=1024`
- `SIZE=32`
- `ACTIVITY=4` — one controlled activity cycle per fixture user every four seconds
- `USERS=1` — non-admin users created on every journal
- `SEGMENTS=2` — maximum simple federated route length for same-user private access
- `WORDS=8`
- `CLIENTS=1`
- `HTTP_PORT_BASE=8192`
- `AGGREGATE_RESULTS_PORT=8290`

A positive `ACTIVITY` value is the number of seconds between continuous activity cycles. Use the positive default for ordinary workflow and integration tests. Reserve `ACTIVITY=0` for setup-only or benchmark baselines where continuous activity must be disabled. Each node receives a distinct journal secret derived from `SECRET` so reciprocal journal identities remain distinct.

Optional image overrides:

- `IMAGE_OVERRIDE_JOURNAL`
- `IMAGE_OVERRIDE_GATEWAY`
- `IMAGE_OVERRIDE_ROUTER`
- `IMAGE_OVERRIDE_EXPLORER`
- `IMAGE_OVERRIDE_WORKBENCH`
- `IMAGE_OVERRIDE_FILE_SYSTEM`
- `IMAGE_OVERRIDE_SOCIAL_AGENT`

## Local source workflow

From this directory:

```bash
CONTAINER_RUNTIME=podman \
CONTAINER_COMPOSE='podman compose' \
./local-compose.sh up -d
```

Available modes:

- `./local-compose.sh build` — build local journal, service, and social-agent images;
- `./local-compose.sh generate` — build and regenerate `compose.yml` plus project-scoped topology/result paths without starting;
- `./local-compose.sh up [-d|--detach] [--no-build]` — regenerate and start the network;
- `./local-compose.sh down` — stop the generated project and remove its test volumes.

Useful overrides:

```bash
COMPOSE_PROJECT_NAME=sync-federation-4 \
NODE_COUNT=4 \
CONNECTIVITY=2 \
ACTIVITY=4 \
USERS=2 \
SEGMENTS=2 \
CONTAINER_RUNTIME=podman \
CONTAINER_COMPOSE='podman compose' \
./local-compose.sh up -d
```

`down` removes this harness's generated test volumes. Use a unique `COMPOSE_PROJECT_NAME` and never point it at a production project.

For a targeted UI/service rebuild during QA, pass `--no-deps` to the generated Compose command. In particular, some `podman-compose` versions cascade `up --force-recreate journal-N` through dependent gateway, router, and social-agent services. That is a whole-stack restart/update test, not a journal-only restart, and can invalidate before/after activity measurements. Use `podman restart <journal-container>` to test an unchanged journal process restart, or deliberately stage a supported data/code upgrade separately from concurrent activity.

## Generate only

To use already-built or published images without the helper:

```bash
SYNC_SERVICES_GENERAL_COMPOSE=../../../deploy/compose/general/compose.yaml \
python3 generate.py
podman compose -f compose.yml up
```

The generator writes `compose.yml` plus isolated runtime artifacts under:

```text
runs/<COMPOSE_PROJECT_NAME>/peers.json
runs/<COMPOSE_PROJECT_NAME>/metrics/social-agent-*/
runs/<COMPOSE_PROJECT_NAME>/results/social-agent-*/benchmark.json
```

A convenience `peers.json` mirror is also written for manual inspection, but running containers bind the project-scoped copy so a later generation for another stack cannot change their topology or diagnostics.

The project topology contains:

- `nodes`: journal name to router host mapping;
- `edges`: deterministic reciprocal-bridge initiator adjacency. Each edge is established once and creates both sides of the relationship.

Routers expose HTTP ports starting at `8192`. WebDAV is available under each router's `/webdav/` path. The aggregate results sidecar serves its dashboard on `http://127.0.0.1:8290/` by default.

## Federation behavior

Social agents create deterministic non-admin users (`alice` through `zara`, then suffixed cycles such as `alice-2`) on every journal; disposable fixture passwords use `<username>-pass` so they satisfy Kratos's supported minimum length. `SIZE` is per user per journal, split between `<user>/data/public` and `<user>/data/private`, with an odd extra key assigned to public. Public keys are readable/resolvable by everyone and remotely read-only. Private keys admit only the same username over every simple, non-revisiting route of at most `SEGMENTS` hops. Different users, loops, and longer routes receive no implicit authority. Agents configure only path-scoped `get`, `set!`, and `resolve`; they do not install remote administrators or federate pin/unpin.

Ancestor traversal remains implicit: descendant grants expose ordinary immediate names in parent listings while unauthorized values remain unreadable. Activity runs independently per user and updates existing local or same-user private keys in place; per-user attempts/successes are exported in metrics and benchmark snapshots. Re-running setup preserves identities, bridge state, fixture values, and grants. Remote retention captures one explicit origin history index, resolves against it, then pins the verified proof at the same local index. Before positive activity begins, each agent waits for signed reads across exercised routes so identity setup, bridge commits, authorization, and reciprocal synchronization are readiness rather than benchmark failures.

With `ACTIVITY=0`, each agent performs bootstrap/setup and exits rather than entering a request loop. Ordinary workflow and integration tests should use a controlled positive interval such as `ACTIVITY=4`; zero is reserved for setup-only or benchmark runs.
