# Synchronic Web

The Synchronic Web is infrastructure for data assurance. Each journal maintains immutable, cryptographically linked records. Reciprocal bridges exchange root-signed heads so journals can preserve and inspect verifiable cross-journal history without requiring global consensus. Applications can retain selective proof material and verify data against an exact committed journal state.

Full documentation: [sandialabs.github.io/sync-web](https://sandialabs.github.io/sync-web)

## Federation model

Sync Web 1.5 uses reciprocal bridges for identity, proof reachability, and signed-head synchronization. A bridge grants no application-data access by itself. Remote application calls are delivered directly to the terminal journal, whose local policy may authorize staged `get`/`set!`, dedicated `get-batch`/`set-batch!`, and committed `resolve`; retention, bridge/configuration changes, access policy, and administration remain local to the origin journal. Ready routes retain their exact public reverse-key material across process boundaries and unchanged Journal restarts. See [`docs/ideas/federation.md`](docs/ideas/federation.md) for the protocol and trust model.

## Isolated stored programs

Local `call!` executes a staged Scheme procedure only for the root caller or a configured local Interface administrator. Interface evaluates it outside `sync-let` in a masked environment and supplies the only authenticated Journal capability. Namespace ownership and Authorization rules cannot grant it, and remote/federated principals are always denied. Sync Web 1.5 defines no execution meter, limit, budget, timeout, or meter configuration because those controls cannot yet be enforced reliably across s7 and nested host work. Every nested operation re-enters Interface authentication and administrator checks, execution remains local and non-replayable, and capabilities cannot escape. Shared self-coded object work separately runs through fresh `sync-let` children cloned from a sealed request-local capability template; no mutable child, result, proof, or application state is cached across boundaries.

## Repository Layout

| Directory | Description |
|---|---|
| `journal/` | Rust journal-sdk: HTTP server, S7 Scheme evaluator, RocksDB persistence |
| `records/` | Scheme record logic: `root`, `standard`, `tree`, `chain`, `ledger`, `federation`, `authorization`, `interface` |
| `services/` | Web services: `gateway`, `router`, `explorer`, `workbench`, `file-system` |
| `deploy/` | Single-node Compose-compatible container deployment |
| `tests/` | API smoke tests, load tests, multi-node network tests |
| `docs/` | Documentation site (Astro/Starlight) |
| `scripts/` | Compact check orchestration and read-only network diagnostics |

## Quickstart

The fastest way to run a local stack:

```sh
COMPOSE_PROJECT_NAME=sync-local SYNC_WEB_VERSION=1.5.0 \
SECRET=your-root-secret INTERFACE_SECRET=your-interface-secret \
ADMIN_PASSWORD=your-login-password HTTP_PORT=8192 HTTPS_PORT=8193 \
docker compose -f deploy/compose/general/compose.yaml up
```

Use `podman-compose` or `podman compose` instead of `docker compose` if that is your container runtime. Sync Web 1.5 requires a fresh database: preserve a 1.4.x volume and its exact runtime for read-only historical access rather than opening it with 1.5. See `deploy/compose/general/README.md` for full configuration options, `tests/release-qa/README.md` for exact-image acceptance checks, and `docs/development-checks.md` for validation commands and tool dependencies.

## License

MIT — see [LICENSE](LICENSE).
