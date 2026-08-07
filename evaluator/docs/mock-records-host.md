# Mock Records host

`src/bin/rust_journal_sdk.rs` is a deterministic in-memory acceptance harness for
the Rust-native host contract. It is **not** the production journal backend.

Build and inspect its executable primitive inventory:

```sh
cargo build --release --bin rust_journal_sdk --features mock-records-host
# Primitives implemented by the mock:
target/release/rust_journal_sdk --primitive-inventory

# Full current Sync Web host contract and explicit mock gaps:
target/release/rust_journal_sdk --contract-primitive-inventory
target/release/rust_journal_sdk --missing-primitive-inventory
```

Run the focused active Records checks without changing their Scheme sources:

```sh
tools/run-records-focused.py \
  --candidate target/release/rust_journal_sdk \
  --records-root /home/tdinh/projects/sync-web/records
```

The focused gate loads the active, unmodified `support.scm`, `standard.scm`,
`tree.scm`, `document.scm`, and their corresponding test files. Current expected
results are Standard 30 checks, Tree 45 checks, and Document 30 assertions.

## Implemented mock primitives

The executable inventory currently contains:

- expression/byte-vector and hexadecimal codecs;
- `sync-node?`, `sync-null`, `sync-null?`, `sync-pair?`, `sync-stub?`;
- `sync-cons`, `sync-car`, `sync-cdr`, `sync-cut`, `sync-stub`;
- `sync-hash`, `sync-digest`, `sync-state`, `sync-eval`;
- a test-only host-provided `stacktrace` placeholder used by the existing Records
  assertion helper.

The mock deliberately does not implement sync-web's variadic `print` callback;
it appears in `--contract-primitive-inventory` and is therefore reported as a
real-integration requirement rather than silently omitted from the contract.

The mock node store implements deterministic SHA-256 leaf, pair, and stub behavior
sufficient for object serialization/deserialization and controlled `sync-eval`.
`HostOutput::Apply` releases the callback reentrancy guard before applying the
host-requested Scheme loader, so nested sync primitives execute exactly once.

## Deliberate limitations

The harness does not implement record roots, journal calls, HTTP/remotes, crypto,
randomness, system time, disk persistence, transactions, or concurrent sessions.
It therefore cannot replace the real Journal SDK and is not evidence for ledger,
interface, federation, cross-process durability, or crash recovery. Those require
the real Sync Web host implementation and acceptance matrix after independent
review of this prerequisite.

The interpreter itself receives no new system authority. SHA-256 and in-memory
node storage live in the explicit host executable.
