# Synchronic Web

The Synchronic Web is a global infrastructure for data assurance. Journals maintain immutable, cryptographically linked records and continuously synchronize metadata with other journals to achieve global consensus. Applications built on top can prove the existence and integrity of any data at a specific point in time.

Full documentation: [sandialabs.github.io/sync-web](https://sandialabs.github.io/sync-web)

## Repository Layout

| Directory | Description |
|---|---|
| `journal/` | Rust journal-sdk: HTTP server, S7 Scheme evaluator, RocksDB persistence |
| `records/` | Scheme record logic: `root`, `standard`, `tree`, `chain`, `ledger`, `interface` |
| `services/` | Web services: `gateway`, `router`, `explorer`, `workbench`, `file-system` |
| `deploy/` | Single-node Docker Compose deployment |
| `tests/` | API smoke tests, load tests, multi-node network tests |
| `docs/` | Documentation site (Astro/Starlight) |

## Quickstart

The fastest way to run a local stack:

```sh
cd deploy/compose/general
SECRET=yourpassword docker compose up
```

See `deploy/compose/general/README.md` for full configuration options.

## License

MIT — see [LICENSE](LICENSE).
