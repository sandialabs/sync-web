# s7-rust Agent Instructions

This repository is an experimental Rust port of the s7 Scheme interpreter for eventual sync-web use.

## Scope

- Primary goal: implement a Rust interpreter that matches the sync-web-compatible subset of the current vendored C s7 baseline.
- The C s7 oracle in `vendor/s7/` is the semantic source of truth unless Thien-Nam explicitly changes the baseline.
- This repo is separate from sync-web. Do not modify `/home/tdinh/projects/sync-web` from this repo unless explicitly asked or only inspecting/copying reference material.

## Compatibility target

Target sync-web-compatible s7 language behavior, not full upstream s7 embedding compatibility.

Preserve Lisp/records-visible behavior: `/home/tdinh/projects/sync-web/records` should not need semantic changes for this interpreter swap, except for explicitly approved diagnostics such as clearer stack traces. Integration-level changes in sync-web's Rust journal layer are in scope and are a major motivation for the port.

The copied C baseline comes from sync-web's `journal/external/s7/` and should be built with behavior-relevant flags mirroring sync-web `journal/build.rs`:

- `-DDEFAULT_PRINT_LENGTH=9223372036854775807`
- `-DWITH_PURE_S7=1`
- `-DWITH_SYSTEM_EXTRAS=0`
- `-DWITH_C_LOADER=0`

## Non-goals and exclusions

- Do not preserve or recreate the s7 C API/ABI just for compatibility.
- Do not build a C-style FFI around `s7_pointer`, `s7_define_function`, `s7_call`, etc.
- Future sync-web integration should use a Rust-native API for primitive registration, host values, evaluation, and structured errors.
- Features currently blacklisted/removed by sync-web's C evaluator should generally be absent in the Rust port, not implemented and removed later.
- Do not implement filesystem loading, system extras, dynamic C loading, continuations, profiling/hooks, or other host/system features unless explicitly approved.

## Validation strategy

- Keep correctness empirical and differential.
- Treat the C oracle as source of truth; `corpus/*/expected.scm` files are cached snapshots for review/debugging.
- Corpus layout is flat:
  - `corpus/<case>/test.scm`
  - `corpus/<case>/meta.json`
  - `corpus/<case>/expected.scm`
- Candidate executables should accept the same CLI shape as the oracle:
  - `candidate path/to/test.scm`
  - print exactly one normalized Scheme result to stdout.
- Use `tools/run-corpus.py` for black-box validation against expected snapshots and optional candidates.
- The runner executes each test from a fresh temporary working directory and reports files left behind there as filesystem side effects.
- Performance reporting should be apples-to-apples wall-clock timing against the C oracle. Avoid Rust-only internal stats in the standard report.

## Implementation priorities

- Build validation infrastructure before interpreter implementation.
- Prefer small commits: corpus/harness changes separate from interpreter changes.
- Start with reader/printer/value representation, then minimal evaluator, then s7-specific semantics.
- Preserve or explicitly document decisions around subtle s7 behavior: first-class macros, `lambda*`, first-class environments, generalized `set!`, applicable objects, multiple values, error/catch behavior, and host object semantics.
- The interpreter may be internally single-threaded; sync-web concurrency can be handled by external orchestration or independent interpreter instances later.

## Useful commands

```sh
# Run full validation; expected to fail candidate phases until implementation exists
tools/test.py

# Run oracle-only sanity checks
tools/test.py --oracle-only

# Build C oracle
tools/build-s7-oracle.sh

# Refresh expected snapshots from C oracle
tools/update-expected.py

# Check snapshots against C oracle
tools/update-expected.py --check

# Run oracle-only corpus validation
tools/run-corpus.py

# Build the current Rust placeholder candidate
cargo build

# Exercise the runner with the intentionally wrong Rust placeholder
tools/run-corpus.py --candidate target/debug/s7-rust

# Run Rust-only metering tests; expected to fail until metered eval exists
tools/run-metering.py --candidate target/debug/s7-rust
```
