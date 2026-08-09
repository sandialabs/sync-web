# Record Tests

`records/tests` is the single test surface for Scheme Records, from low-level
object behavior through deterministic multi-journal Interface workflows.

## Run the suite

Build the cross-platform runner from the repository root:

```sh
CARGO_TARGET_DIR=journal/target \
  cargo build --manifest-path records/tests/Cargo.toml
```

Run every Records case:

```sh
journal/target/debug/records-test --suite records/tests/suite.toml
```

Run independent cases concurrently when faster feedback is useful:

```sh
journal/target/debug/records-test --suite records/tests/suite.toml --jobs 4
```

`suite.toml` is the canonical top-level suite. Paths resolve relative to the
manifest, and every case runs in a fresh `records-test` subprocess so evaluator,
persistence, scheduler time, and deterministic randomness do not leak between
cases. Parallel execution retains manifest-ordered output and does not change
scheduling inside a case.

## Layout

```text
records/tests/
  suite.toml             complete declarative suite
  unit/                  source-driven lower-level object tests
    unit-harness.scm     shared assertions
  interface/             deterministic Interface and federation workflows
    interface-harness.scm
  src/                   Rust runner and deterministic scheduler
```

Unit procedures receive the active module sources they need as quoted Scheme
data. Interface cases run unchanged production `sync-remote` calls through the
deterministic scheduler. See [`interface/README.md`](interface/README.md) for
the Interface DSL and scheduling model.

Run one unit case directly while iterating:

```sh
journal/target/debug/records-test --unit \
  records/tests/unit/test-tree.scm \
  records/tests/unit/unit-harness.scm \
  records/lisp/standard.scm \
  records/lisp/tree.scm
```

Run one Interface case directly:

```sh
journal/target/debug/records-test \
  records/tests/interface/interface-harness.scm \
  records/tests/interface/test-network.scm \
  records/lisp/root.scm \
  records/lisp/standard.scm \
  records/lisp/log-chain.scm \
  records/lisp/tree.scm \
  records/lisp/ledger.scm \
  records/lisp/authorization.scm \
  records/lisp/interface.scm
```
