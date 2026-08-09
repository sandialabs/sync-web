# Pre-release usability QA

This directory contains opt-in, process-isolated acceptance checks. These checks complement the in-process Records suite; they do not replace it.

## Current release holds

The 1.5 candidate remains fresh-install-only. Existing or older state must fail before interpretation or rewrite; no migration, conversion, or production update is implied.

Sync Web 1.5 intentionally defines no stored-execution meter, limit, budget, configuration, timeout, or meter-specific error/SSE contract because one cannot yet be enforced reliably across s7 and nested host work. The compensating public boundary is root/configured-local-admin-only `call!`; namespace ownership, Authorization rules, and remote principals cannot grant it.

Release acceptance also requires the process-isolated federation readiness oracle below. A route that has become ready must remain usable across ordinary committed steps and an unchanged process restart. Transport timeouts remain operational failures; deterministic loss of committed route material is a correctness failure.

## Sticky federation readiness

`sticky_readiness.py` runs signed staged `get` and proof-bearing `resolve` requests across every direct and two-hop route in a generated four-node network. Qualification requires, simultaneously:

- 30 continuous seconds with no route or background-worker failure;
- ten new activity cycles on every node;
- three successful sweeps of every route;
- unchanged Gateway and social-agent failure counters;
- typed byte-vector `get` and resolve content, `pinned? == false`, root `n-1`, and a closed proof-reference graph (with implicit null `n-0`);
- at least 8 GiB available memory, no new OOM kill, and memory full PSI `avg10` no greater than 1.

Pre-ready failures may retry. The first complete successful all-route sweep establishes readiness; any later route, proof-structure, worker-counter, or Gateway-counter failure is terminal and cannot be reset into a later pass. The proof check is structural transport validation, not a standalone cryptographic verifier.

`run_sticky_readiness.sh` starts an isolated positive-activity network, qualifies it, restarts all four Journal processes without replacing their RocksDB volumes, qualifies it again, and removes the entire test project.

Build the local stack once, or let the wrapper build it:

```bash
CONTAINER_RUNTIME=podman \
CONTAINER_COMPOSE='podman compose' \
tests/release-qa/run_sticky_readiness.sh --build
```

For already-built exact local images, retain the source-bound image manifest from the build output and require it explicitly for a separate repeat:

```bash
IMAGE_MANIFEST=target/release-qa/sticky-build/image-manifest.json \
OUTPUT=target/release-qa/sticky-repeat \
CONTAINER_RUNTIME=podman \
CONTAINER_COMPOSE='podman compose' \
tests/release-qa/run_sticky_readiness.sh --no-build
```

The repeat fails before startup if the clean source commit/tree or any mutable tag's image ID/digest differs from the build manifest.

Use a unique `COMPOSE_PROJECT_NAME` and port range when another QA stack exists. Never run this beside another container-heavy or timed benchmark. Evidence defaults to `target/release-qa/sticky-readiness/` and excludes bearer tokens.

`sticky-oracle-provenance.json` records the frozen base failure, candidate pass, source/tree, raw/probe hashes, exact reproduction command, and same-digest sparse-versus-hydrated serializer oracle. The unit test hashes and parses `sticky-oracle-raw.txt` against `sticky-oracle-probe.scm` rather than trusting conclusion fields alone.

## Fresh single-node journey

The existing primary compose smoke can additionally exercise real registration, session-to-API-token exchange, administrator policy visibility, two nonadmin users, cross-user read/write/call/grant denials, staged program storage, owner denial, removed-call-grant rejection, configured-admin execution/readback, malformed/federated denial, Workbench help, OpenAPI, and the existing WebDAV workflow:

```bash
LOCAL_COMPOSE_RELEASE_QA=1 \
CONTAINER_RUNTIME=podman \
CONTAINER_COMPOSE='podman compose' \
tests/api/local-compose.sh smoke
```

The compose smoke creates fresh volumes and removes them on exit. A build writes `IMAGE_MANIFEST` (default `target/release-qa/local-images.json`). After that exact build, set `LOCAL_COMPOSE_SKIP_BUILD=1` and reuse the same manifest to repeat only the disposable smoke journey; mutable tags are rejected if their IDs/digests changed.

## Lightweight checks

```bash
python3 -m unittest discover -s tests/release-qa -p 'test_*.py' -v
python3 -m py_compile \
  tests/release-qa/sticky_readiness.py \
  tests/release-qa/single_node_journey.py \
  tests/release-qa/image_manifest.py
sh -n tests/release-qa/run_sticky_readiness.sh tests/api/local-compose.sh
```

## Remaining acceptance matrix

| Surface | First tranche | Remaining work |
| --- | --- | --- |
| Fresh install and seeded admin | Single-node compose journey | Explicit nonfresh-state rejection in the final image |
| Identity and user authorization | Two registrations/tokens, cross-user read/write/call/grant denial, removed-call-grant rejection, configured-admin execution | Public/ancestor listing, expanded admin matrix, localized denial UI |
| Stored `call!` | Owner/policy/remote denial, configured-admin execution/readback, malformed/local-only errors | Interrupted-request recovery and failed-partial-effect SSE observation |
| Federation | All direct/two-hop staged reads and proof resolves | Negative principal/path matrix and bridge delete/re-establish workflow |
| Restart | All four Journals with persisted RocksDB state | Gateway/identity restart and interrupted-request recovery |
| Proof and history | Latest indexed proof resolve on every route | Explicit older indexes, pin/unpin persistence, retained-window edges |
| WebDAV | OPTIONS, auth denial, PUT/GET/list/MOVE/DELETE | Committed index files, COPY, directory errors, multi-user authorization |
| Explorer | Route health and existing component/build suites | Browser visual matrix, remote breadcrumb denial localization, restart refresh |
| Workbench | Route health and live `call!` help entry | Authenticated browser execution/error presentation |
| TLS and soak | Not in this tranche | Only if the release owner reinstates the previously waived checks |

Do not interpret a passing first tranche as production approval. Preserve exact commit, image, topology, and output hashes with the final release run.
