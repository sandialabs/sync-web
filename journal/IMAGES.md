# Provenance-bound Wasmer Journal images

Two Linux/amd64 image variants package the same locked Journal release build and a current-source Wasmer AOT kernel:

- `Dockerfile.musl`: Rust 1.96 on Alpine 3.23, native musl/static-PIE linkage.
- `Dockerfile.glibc`: Rust 1.96 on Debian 12, with a pinned Debian bookworm-slim runtime.

Both variants use `--locked --features wasmer-evaluator`, build both `journal-sdk` and `ledger`, run the release test suite, verify the exact supplied AOT SHA-256, and embed provenance and linkage receipts. Wasmer nofuel is the sole request evaluator; there is no runtime selector or native host fallback. The AOT target restricts these image and binary-release paths to Linux/amd64, and builds fail for another `TARGETARCH`. Other architectures require independently built and qualified AOT artifacts. Binary releases include the exact tested kernel and its provenance beside each glibc or musl executable set; startup still requires its explicit path and hash environment.

The AOT binary is an external attested build input and is not committed to Git. `scripts/build-wasmer-kernel` builds it twice from a clean current checkout in one declared environment, compares both WASM and AOT outputs, and records the exact source, toolchain, WASM, and AOT hashes. A release workflow semantically tests and reuses its produced bytes; byte equality with artifacts independently built on another host is not required.

Build an image from a clean checkout:

```sh
journal/scripts/build-journal-image \
  --variant musl \
  --kernel /absolute/path/kernel.wasmer \
  --tag localhost/sync-web/journal-sdk:musl-candidate \
  --evidence-dir /absolute/path/evidence

journal/scripts/build-journal-image \
  --variant glibc \
  --kernel /absolute/path/kernel.wasmer \
  --tag localhost/sync-web/journal-sdk:glibc-candidate \
  --evidence-dir /absolute/path/evidence
```

The image script requires a clean Git checkout, derives the AOT hash from the supplied file, exports the complete `HEAD` tree, verifies the tracked lock, records every context-file hash, and passes exact commit/tree/input/AOT identities into OCI labels. Builder and runtime stages independently compare the packaged file to that derived hash. The script never pushes an image.

The historical ec655 image qualification used AOT SHA-256 `7fecb3e55febe8680a909dd41ea1bc027e94283cf86403d51662a504f29de4b6`; that receipt does not constrain current-source builds.

Both runtime images retain `/bin/sh`, `wget`, `/srv`, and the existing `journal-sdk` entrypoint contract so the primary general compose deployment and local API smoke path can use either image tag without changing startup scripts, health checks, database layout, TLS/plaintext behavior, or record mounts.
