# Rust interpreter implementation status

Last updated: 2026-06-06.

The Rust candidate is now a real broad-coverage interpreter prototype. It is still not architecturally polished, but the black-box C-oracle corpus, Rust-only metering checks, and Rust-only tail-call checks pass.

## Current validation

```sh
cd /home/tdinh/projects/s7-rust
tools/test.py
```

Latest result:

```text
expected-current: 98/98
correct: 98/98
metering-current: 8/8
tail-call-current: 5/5
all requested checks passed
```

Unified validation status:

- C oracle build/check/corpus: passing.
- Rust candidate build: passing.
- Rust candidate corpus: passing, 98/98.
- Rust-only metering suite: passing, 8/8.
- Rust-only tail-call suite: passing, 5/5 at the default validation depth.

## Implemented coverage

- Reader/evaluator/printer path wired into `src/main.rs`.
- Core values: booleans, nil, unspecified/undefined, integers, rationals, floats, complex numbers, chars/named chars, strings, symbols, keywords, pairs/lists, vectors, multidimensional vectors, byte-vectors, float-vectors, int-vectors, hash tables, environments, procedures, macros, ports, multiple-values wrapper, datum-comment wrapper, root metadata values, and simple gas state.
- Core special forms: `quote`, `quasiquote`/`unquote`/`unquote-splicing`, `if`, `begin`, `define`, `define*`, `set!`, `lambda`, `lambda*`, `let`, `let*`, named `let`, `letrec`, `cond`, `case`, `do`, `and`, `or`, `catch`, `throw`, `define-macro`, `define-macro*`, `define-bacro`, `macro`, `macro*`, `bacro`, `with-let`.
- Broad primitive set: arithmetic/comparison, numeric predicates/transcendentals used by corpus, list accessors, list helpers, map/apply/for-each, equality, predicates, strings/chars, vectors/typed vectors/multidimensional vector helpers, hash tables, copy/fill, environments/rootlet/curlet/funclet, eval/read, string input/output ports, display/write/newline, format subset, errors, gensym, values, setters, help/documentation/signature/object->let/type-of.
- Metered `eval` extension: `(eval expr env gas)` returns `#<unspecified>` on gas exhaustion and records gas status.
- Gas introspection: `(*s7* 'gas)` returns `((last USED STATUS) (current REMAINING-OR-#f))` for the current prototype contract.
- Tail-position evaluator/trampoline support for self recursion, mutual recursion, named `let`, tail calls through `if`, `begin`, `cond`, `case`, `let`, `let*`, `letrec`, and metered eval cases covered by `tools/run-tail-calls.py`.
- Rootlet/top-level separation: rootlet membership/order now matches the generated C oracle inventory exactly for `(map car (rootlet))`.
- Introspection compatibility: `help`, `documentation`, `signature`, and `object->let` match the current corpus summaries, including root metadata for unsupported rootlet bindings.
- Managed string/captured ports are implemented enough for the corpus output-string cases.
- Multiple values splice into argument lists for the covered cases.
- CLI runs evaluation on a larger-stack thread and returns stringified output to avoid aborting on current recursive evaluator paths before proper tail-call/trampoline work.

## Known limitations / next implementation slices

1. Tail calls are prototype-level.
   - Covered tail positions pass `tools/run-tail-calls.py` at deep recursion depths.
   - The CLI stack-size increase remains a temporary safety crutch for non-tail recursive paths.
   - Further evaluator refactoring should simplify and centralize the trampoline logic.

2. Metering is prototype-level.
   - Current gas costs are simple fixed charges on evaluation/application paths.
   - The TOML gas schedule file is still not loaded or applied.
   - External interruption/cancellation is not implemented yet.

3. Architecture is prototype-quality.
   - `src/lib.rs` is intentionally broad and monolithic from the first implementation push.
   - Refactor toward the documented modules once behavior remains stable.

4. Numeric tower is corpus-compatible, not complete.
   - Rationals/complex values pass current corpus, but arbitrary precision, exactness semantics, and edge cases need more systematic work.

5. Error/introspection fidelity is partially table-driven.
   - Current behavior matches corpus expectations, but should eventually be generated/validated from the rootlet inventory and oracle probes rather than hand-maintained.

6. Unsupported rootlet bindings use metadata stubs.
   - This preserves rootlet/introspection shape without implementing every upstream feature yet.
   - Actual implementation of safe-but-currently-stubbed functions should continue until the full documented primitive inventory is genuinely supported.

## Useful commands

```sh
# Full validation, including corpus, metering, and tail-call checks
tools/test.py

# Oracle baseline only
tools/test.py --oracle-only

# Candidate corpus only
tools/run-corpus.py --candidate target/debug/s7-rust --timeout 5 --failures 20

# Candidate metering only
tools/run-metering.py --candidate target/debug/s7-rust --timeout 5 --failures 20

# Tail-call requirement harness
tools/run-tail-calls.py --candidate target/debug/s7-rust --iterations 200000
```
