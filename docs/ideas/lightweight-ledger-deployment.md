# Lightweight ledger deployment

Status: initial `ledger` binary implemented; container/Compose wrapper follow-up pending.

## Direction

Provide a lightweight Sync Web ledger runtime for local and agent-recorder-oriented deployments without the full `deploy/compose/general` stack. The user-facing artifact should be a single executable named `ledger`.

The internal source/layout names do not need to be user-facing. In particular, `deploy/bin/ledger` can be the source home for the binary, while GitHub release artifacts and documentation should consistently refer to the executable as `ledger`.

## Motivation

The full general stack includes gateway, explorer, identity-provider, router, WebDAV, and related service wiring. That is useful for full web deployments, but it is heavier than needed for tools that only need a local ledger-backed journal endpoint.

A single-binary ledger runtime supports:

- agent-recorder writing directly to a local Sync Web ledger;
- simple local/personal deployments;
- bridge-compatible ledger synchronization without auxiliary web services;
- downloadable GitHub Actions/GitHub release artifacts for each supported OS/architecture.

## Layering

Long-term lightweight deployment should use one canonical implementation, then wrap it for container and Compose users:

```text
deploy/bin/ledger/        # Rust source for the outward-facing `ledger` executable
deploy/container/ledger/  # single-container image wrapping `ledger`
deploy/compose/ledger/    # thin one-service Compose wrapper around the image
```

The binary is the canonical logic. Container and Compose deployment should not duplicate ledger installation/startup behavior in shell scripts.

## `ledger` behavior

Running:

```sh
./ledger
```

should be enough to start a local ledger with sensible defaults.

Expected startup flow:

1. Resolve the database/data directory.
2. Create or load a local secret.
3. If the database is empty, install embedded Scheme records:
   - `root.scm`
   - `standard.scm`
   - `log-chain.scm`
   - `tree.scm`
   - `document.scm`
   - `ledger.scm`
   - `interface.scm`
4. If requested, update/reinstall embedded records into an existing database.
5. Start the journal HTTP interface.
6. Run periodic ledger `*step*` using the configured period.

The Scheme records should be embedded in the binary with `include_str!` or equivalent so users do not need a separate checkout.

## CLI sketch

`ledger` should feel like a ledger-flavored, self-contained `journal-sdk`. Preserve common journal-style passthroughs where useful:

```sh
./ledger --help
./ledger -e '((function info))'
./ledger -p 8192 -d ./ledger-data
```

Possible options:

```text
ledger [OPTIONS]

Options:
  -d, --database <PATH>       Ledger database/data directory
  -p, --port <PORT>           HTTP port for /interface
  -c, --period <SECONDS>      Step period in seconds
  -e, --eval <EXPR>           Evaluate a ledger/interface query and exit
      --secret <SECRET>       Root/interface secret; otherwise use generated local secret
      --update-records        Reinstall/update embedded records in an existing database
      --window <N>            Ledger retention window
      --interface <URL>       Public interface URL advertised in info/bridge metadata
      --name <NAME>           Journal name advertised in info/bridge metadata
      --bridge-publish <MODE> Bridge publish policy: push|pull|none
      --bridge-subscribe <MODE> Bridge subscribe policy: push|pull|none
  -h, --help
```

Current implemented flags are intentionally close to this sketch. The important UX goal is one downloadable binary that just works, while still allowing power users and scripts to pass through evaluation/startup options.

## Release artifacts and CI

The `.github/workflows/ledger.yml` workflow builds both outward-facing binaries in the same matrix job where practical:

- `journal-sdk` — the lower-level journal SDK/server binary, versioned by `journal/Cargo.toml`.
- `ledger` — the lightweight ledger distribution binary, versioned by top-level `VERSION`.

Expected artifact names:

```text
journal-sdk-linux-x86_64
ledger-linux-x86_64
journal-sdk-linux-aarch64
ledger-linux-aarch64
journal-sdk-linux-x86_64-musl
ledger-linux-x86_64-musl
journal-sdk-linux-aarch64-musl
ledger-linux-aarch64-musl
journal-sdk-macos-x86_64
ledger-macos-x86_64
journal-sdk-macos-aarch64
ledger-macos-aarch64
journal-sdk-windows-x86_64.exe
ledger-windows-x86_64.exe
```

For branch pushes, these are uploaded as workflow artifacts for testing. For `v*` tag builds, a final publish job waits for all binary builds, downloads the artifacts, generates `SHA256SUMS`, and uploads the files as release assets.

Implementation notes:

- Linux/musl artifacts are built in Alpine and checked inside Alpine with `libgcc`/`libstdc++` present.
- Windows builds use an `OUT_DIR` build-time patch of vendored s7 rather than modifying `journal/external/s7` directly.
- `journal-sdk` must not depend on top-level platform `VERSION`; only `ledger` reports the platform version.

## Secret and signing posture

The binary should not store raw private signing keys in ledger state. Current ledger direction derives step signing keys from the root/step secret at step time and stores only public key/signature material in ledger history/config.

For a just-works local UX, if no secret is provided, `ledger` may generate a local secret and store it in the data directory. It should print where the secret is stored and make the security implications clear.

## Container and Compose wrappers

`deploy/container/ledger` should define a single-container runtime that wraps `ledger` directly:

- build or copy the `ledger` executable;
- expose the journal interface port;
- mount one data directory/volume;
- translate simple environment variables to `ledger` flags;
- avoid gateway, explorer, identity-provider, router, and WebDAV dependencies.

`deploy/compose/ledger` should eventually become a thin one-service Compose file around that image. It should provide port/volume/env defaults and stack isolation, but not contain the canonical ledger install/start logic.

## Agent-recorder relationship

Agent-recorder should be able to target a local `ledger` directly at:

```text
http://127.0.0.1:<port>/interface
```

Forward-integrity HMAC generation and verification belongs in agent-recorder, not in `ledger`. The ledger runtime provides storage, stepping, proofs, bridge-compatible synchronization, and the plain ledger interface.
