# s7-rust

Experimental Rust port of the s7 Scheme interpreter for Synchronic Web.

This repository is currently a planning/prototype workspace. The immediate goal is to explore a Rust implementation of the s7 language/runtime behavior that sync-web depends on, using the current vendored C s7 as a deterministic compatibility oracle.

## Sync-web context

Synchronic Web currently embeds s7 through `journal/src/evaluator.rs` in the main sync-web repository. The journal uses s7 to evaluate Scheme record code under `records/lisp/`, including the object system, tree, ledger, document, and authenticated interface layers.

The current baseline is the vendored s7 in sync-web:

- source: `journal/external/s7/s7.c` and `s7.h`
- version/date: s7 `10.8`, `15-Jan-2024`
- build profile used by sync-web:
  - `WITH_PURE_S7=1`
  - `WITH_SYSTEM_EXTRAS=0`
  - `WITH_C_LOADER=0`

The Rust port should target sync-web-compatible s7 behavior, not necessarily full upstream s7 embedding compatibility.

## Important non-goals

- Do not preserve the s7 C API or ABI.
- Do not provide a C-style FFI just to mimic `s7_define_function`, `s7_call`, or raw `s7_pointer` lifetimes.
- Do not implement sync-web's currently blacklisted/system features only to remove them later; unsupported features should simply be absent.
- Do not rewrite sync-web record semantics as part of this project.
- Do not optimize before differential correctness is strong.

## Integration direction

If the interpreter becomes viable, sync-web should integrate it through a Rust-native API:

- register primitive functions as Rust functions/closures;
- expose sync nodes as Rust-backed Scheme host values;
- return structured Rust results/errors from evaluation;
- convert values to/from sync-web's Scheme/JSON gateway formats;
- keep the current C s7 backend available until compatibility is demonstrated.

## One-command validation

Run the full validation flow:

```sh
tools/test.py
```

This builds/checks the C oracle, runs the curated oracle corpus, runs the imported upstream corpus against the oracle, builds the Rust candidate, runs the candidate corpus, runs the candidate against the imported upstream corpus, and runs the Rust-only metering and tail-call tests.

For oracle-only sanity checks:

```sh
tools/test.py --oracle-only
```

## C s7 oracle

The exact sync-web vendored C s7 baseline is copied under `vendor/s7/` for differential testing.

Build the oracle runner:

```sh
tools/build-s7-oracle.sh
```

The build script mirrors sync-web's behavior-relevant `journal/build.rs` flags:

- `DEFAULT_PRINT_LENGTH=9223372036854775807`
- `WITH_PURE_S7=1`
- `WITH_SYSTEM_EXTRAS=0`
- `WITH_C_LOADER=0`

Run a corpus case:

```sh
target/c-oracle/s7-oracle corpus/000-smoke/literals.scm
```

## Corpus

Initial differential programs live under `corpus/`. Each `corpus/<case>/test.scm` file is self-contained and returns one final structured value. Each case has `meta.json` metadata and a generated `expected.scm` snapshot from the C oracle.

Run oracle-only validation:

```sh
tools/run-corpus.py
```

Run against a candidate executable with the same one-file CLI shape:

```sh
tools/run-corpus.py --candidate target/debug/s7-rust
```

The current Rust binary is only a harness placeholder: it accepts a Scheme file and intentionally returns `#t` for every input so the validation runner reports real failures, diffs, and timing numbers before interpreter work begins.

## Metering tests

Metered `eval` is a Rust/sync-web extension, not a C s7 oracle feature. These tests are candidate-only and should not be run in oracle-only validation:

```sh
tools/run-metering.py --candidate target/debug/s7-rust
```

The runner wraps selected corpus cases and checks relational gas invariants: enough gas succeeds, half the measured gas interrupts with `#<unspecified>`, repeated work costs at least twice as much, and `(*s7* 'gas)` exposes active remaining gas.

## Tail-call tests

Tail-call optimization is a hard requirement for the Rust interpreter. The tail-call harness generates deep tail-recursive programs and is part of default candidate validation:

```sh
tools/run-tail-calls.py --candidate target/debug/s7-rust
```

See [`docs/tail-calls.md`](docs/tail-calls.md) for the requirement, current harness, and acceptance checks.

## Imported upstream s7 tests

Upstream s7 tests are available in two forms:

```sh
# More faithful: stage upstream checkout and load s7test.scm with its own harness.
tools/run-upstream-s7test.py --timeout 120

# Isolated imported assertions, used by tools/test.py for oracle and candidate comparison.
tools/run-upstream-corpus.py --timeout 5
```

The faithful suite is patched to skip unsupported/profile-specific upstream sections and currently completes as a sync-profile smoke test. See [`docs/upstream-s7-tests.md`](docs/upstream-s7-tests.md).

## Planning document

See [`docs/rust-s7-port.md`](docs/rust-s7-port.md) for the current plan, risk map, semantic compatibility concerns, differential testing strategy, GC notes, and phased rollout sketch.
