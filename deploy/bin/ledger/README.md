# ledger

`deploy/bin/ledger` contains the Rust source for the user-facing `ledger` executable.

Build from the journal crate:

```sh
cd journal
cargo build --bin ledger --release
```

Basic use:

```sh
./target/release/ledger
./target/release/ledger -d ./ledger-data/journal -p 8192
./target/release/ledger -d ./ledger-data/journal --secret test -e '((function info))'
```

The binary embeds the Scheme ledger records, installs them into an empty database on first run, and starts the raw journal `/interface` with periodic ledger steps. If `--secret` is omitted, it generates and stores a local secret next to the database.

Release artifacts are built by the `Ledger Binaries` GitHub Actions workflow for `ledger` and `journal-sdk` on Linux, Linux musl/Alpine, macOS, and Windows targets. Branch workflow artifacts can be downloaded for testing before a tagged release.
