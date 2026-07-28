# Synchronic Web

The Synchronic Web is infrastructure for data assurance. Each journal maintains immutable, cryptographically linked records. Reciprocal bridges exchange root-signed heads so journals can preserve and inspect verifiable cross-journal history without requiring global consensus. Applications can retain selective proof material and verify data against an exact committed journal state.

Full documentation: [sandialabs.github.io/sync-web](https://sandialabs.github.io/sync-web)

## Federation model

Sync Web 1.5 uses reciprocal bridges for identity, proof reachability, and signed-head synchronization. A bridge grants no application-data access by itself. Remote application calls are delivered directly to the terminal journal, whose local policy may authorize only `get`, `set!`, and `resolve`; pinning, batching, bridge/configuration changes, access policy, and administration remain local to the origin journal. See [`docs/ideas/federation.md`](docs/ideas/federation.md) for the protocol and trust model.

## Repository Layout

| Directory | Description |
|---|---|
| `journal/` | Rust journal-sdk: HTTP server, S7 Scheme evaluator, RocksDB persistence |
| `records/` | Scheme record logic: `root`, `standard`, `tree`, `chain`, `ledger`, `interface` |
| `services/` | Web services: `gateway`, `router`, `explorer`, `workbench`, `file-system` |
| `deploy/` | Single-node Compose-compatible container deployment |
| `tests/` | API smoke tests, load tests, multi-node network tests |
| `docs/` | Documentation site (Astro/Starlight) |
| `scripts/` | Compact check orchestration and read-only network diagnostics |

## Quickstart

The fastest way to run a local stack:

```sh
COMPOSE_PROJECT_NAME=sync-local SECRET=yourpassword \
HTTP_PORT=8192 HTTPS_PORT=8193 \
docker compose -f deploy/compose/general/compose.yaml up
```

Use `podman-compose` or `podman compose` instead of `docker compose` if that is your container runtime. See `deploy/compose/general/README.md` for full configuration options and `docs/development-checks.md` for validation commands and tool dependencies.

## License

MIT — see [LICENSE](LICENSE).
