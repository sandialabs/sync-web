# Experimental Rust evaluator integration

This branch contains an opt-in integration checkpoint for the Rust s7 port. The
existing C evaluator remains the default and unchanged selection.

Build the dual-backend journal:

```sh
cd journal
cargo build --features rust-evaluator
```

Select the Rust backend at runtime:

```sh
SYNC_WEB_EVALUATOR=rust target/debug/journal-sdk -e '(+ 1 2)'
```

Without `SYNC_WEB_EVALUATOR=rust`, the binary uses the existing C evaluator. If
the environment variable requests Rust but the feature was not compiled, the
journal returns a configuration error.

## Current evidence

The backend uses the real journal `PERSISTOR` and per-request
`MemoryPersistor`, and Rust-native callbacks for node construction/traversal,
record create/delete/list/call, codecs, deterministic hashing, Dilithium crypto,
and system time.

The unchanged Records harness currently matches the C path through:

- Root: 23 checks;
- Standard: 26 checks;
- Tree: 45 checks;
- Document: 30 assertions;
- Linear Chain: 119 checks;
- Log Chain: 119 checks.

The C backend passes the complete unchanged nine-case Records harness. The Rust
backend is not yet acceptable for production: Ledger deterministically fails at
check 12 while applying `bridge-synchronize!`. Inside that serialized nested
method, a field load that returns the expected procedure immediately outside the
method becomes `#<unspecified>`, leading to `(#<unspecified> size)`. The state
node, digest, direct path traversal, and real persistor graph remain correct.
This persists with host application removed in favor of a normal Scheme
`%sync-loader`, with strict/current environments, and with the Word tier disabled,
so the blocker is a deeper nested method-wrapper evaluator semantic boundary.

Error-form output also still differs for a caught crypto host panic. Do not run
comparative load tests or claim restart/root parity until the complete Records
suite and exact error tests pass.
