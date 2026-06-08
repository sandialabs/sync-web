# Synchronic Web

The Synchronic Web is a global infrastructure for data assurance. Journals maintain immutable, cryptographically linked records and continuously synchronize metadata with other journals to achieve global consensus. Applications built on top can prove the existence and integrity of any data at a specific point in time.

Full documentation: [sandialabs.github.io/sync-web](https://sandialabs.github.io/sync-web)

## Repository Layout

| Directory | Description |
|---|---|
| `journal/` | Rust journal-sdk: HTTP server, S7 Scheme evaluator, RocksDB persistence |
| `records/` | Scheme record logic: `root`, `standard`, `tree`, `chain`, `ledger`, `interface` |
| `services/` | Web services: `gateway`, `router`, `explorer`, `workbench`, `file-system` |
| `deploy/` | Single-node Compose-compatible container deployment |
| `tests/` | API smoke tests, load tests, multi-node network tests |
| `docs/` | Documentation site (Astro/Starlight) |

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
