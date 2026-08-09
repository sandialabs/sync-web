# Social Agent Network

Generate a local multi-node Sync Web network from the primary general compose stack. The default topology has four journals and reciprocal bridges; it is intended for federation and service-boundary testing, not as a lightweight unit-test fixture.

## Prerequisites

- a standard container runtime with Compose support (Docker or Podman);
- Python 3 with PyYAML;
- local source under this repository when using `local-compose.sh`.

## Defaults

The generator accepts environment overrides and otherwise uses:

- `NODE_COUNT=4`
- `SECRET=root-password`
- `INTERFACE_SECRET=interface-password`
- `ADMIN_PASSWORD=admin-pass`
- `CONNECTIVITY=2`
- `PERIOD=2`
- `WINDOW=1024`
- `SIZE=32`
- `ACTIVITY=4` — one controlled activity cycle per fixture user every four seconds
- `USERS=1` — non-admin users created on every journal
- `SEGMENTS=2` — maximum federated walk length for same-user private access; bounded walks may revisit journals
- `WORDS=8`
- `CLIENTS=1`
- `BATCH` unset — optional positive continuous-activity batch size, maximum `1024`
- `HTTP_PORT_BASE=8192`
- `AGGREGATE_RESULTS_PORT=8290`

`ACTIVITY` is the number of seconds between continuous activity cycles. Use the positive default for controlled workflow and integration tests; `ACTIVITY=0` removes the delay and runs maximum-throughput saturation traffic. Set `ACTIVITY_DISABLED=1` for setup-only runs with no continuous activity. Each node receives a distinct journal secret derived from `SECRET` so reciprocal journal identities remain distinct.

When `BATCH` is absent, activity retains the scalar `get`/`set` and `pin`/`unpin` workflows. A positive `BATCH=N` keeps setup and readiness scalar but uses `get-batch`/`set-batch` or `pin-batch`/`unpin-batch` for each activity cycle. The existing random anchor still chooses the exact route and access group; the remaining paths are sampled uniquely from that same user, route, and group. Historical paths keep the current latest `-1` index at the origin and each hop. Configuration fails rather than reducing `N` when it exceeds `1024`, `SIZE`, or any selectable public/private route-group capacity; because `SIZE` is split between public and private, the practical bound is often `floor(SIZE / 2)`. `BATCH=1` deliberately exercises batch endpoint overhead.

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

Social agents create deterministic non-admin users (`alice` through `zara`, then suffixed cycles such as `alice-2`) on every journal; disposable fixture passwords use `<username>-pass` so they satisfy Kratos's supported minimum length. `SIZE` is per user per journal, split between `<user>/data/public` and `<user>/data/private`, with an odd extra key assigned to public. Public keys are readable/resolvable by everyone and remotely read-only. Private keys admit only the same username over every bounded walk of at most `SEGMENTS` hops, including finite walks that revisit a journal. Each exact walk receives its own rule; different users, non-enumerated routes, and longer routes receive no implicit authority. Agents configure only path-scoped `get`, `set!`, and `resolve`; they do not install remote administrators or federate pin/unpin.

Ancestor traversal remains implicit: descendant grants expose ordinary immediate names in parent listings while unauthorized values remain unreadable. Activity runs independently per user and updates existing local or same-user private keys in place; per-user attempts/successes are exported in metrics and benchmark snapshots. HTTP request throughput remains separate from logical path-operation throughput: a successful batch request counts once as a request and `N` times as logical path operations. Dashboards lead with logical path operations per second and retain requests per second alongside it; these are logical-operation, not byte-throughput, claims. Re-running setup preserves identities, bridge state, fixture values, and grants. Remote retention sends one canonical full path directly to `pin`; Interface resolves and verifies the remote proof before pinning it at the origin, including across concurrent origin advancement, and activity then unpins the same path without a redundant client-side proof resolve. Batch activity sends the corresponding unique latest-index paths through `pin-batch` and `unpin-batch`; it does not add a standalone resolve workload. Before positive activity begins, each agent waits for signed reads across exercised routes so identity setup, bridge commits, authorization, and reciprocal synchronization are readiness rather than benchmark failures.

With `ACTIVITY=0`, each agent continuously runs activity cycles without sleeping, saturating the federated workload for throughput and concurrency testing. Ordinary workflow and integration tests should use a controlled positive interval such as `ACTIVITY=4`. Use `ACTIVITY_DISABLED=1` when agents should perform bootstrap/setup and exit without entering the request loop.
