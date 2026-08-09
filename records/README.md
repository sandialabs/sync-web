# Synchronic Web Records

This directory contains the Scheme object and interface composition used by the
Synchronic Web Journal SDK. Durable state is built from sync nodes; the Rust
journal supplies persistence, evaluation, cryptography, serialization tracing,
and HTTP transport primitives.

Read [`LANGUAGE.md`](LANGUAGE.md) before changing active record code.

## Active modules

- `lisp/root.scm` — journal Root and privileged query/step hooks.
- `lisp/standard.scm` — shared object compiler, deep object operations, and
  structural proof serialization facade.
- `lisp/tree.scm` — Tree-native byte-vector values and Merkle directory paths.
- `lisp/linear-chain.scm` and `lisp/log-chain.scm` — history/proof chains.
- `lisp/ledger.scm` — Stage, permanent/temporary history, proofs, retention,
  signatures, and anchored peer evidence.
- `lisp/federation.scm` — reciprocal bridge operations, routing, networking,
  signed invocation transport/authentication, and peer operational state.
- `lisp/authorization.scm` — path-scoped `get`, `set!`, and `resolve` policy.
- `lisp/interface.scm` — fresh installation and the authenticated external API.

`lisp/archive/` is historical reference material and is not installed by the
active stack.

## Installation

Use `deploy/compose/general/run.sh`, `deploy/compose/ledger/run.sh`, or the
bundled `deploy/bin/ledger` executable. These entry points pass all active class
forms to `interface.scm` in the required order.

Sync Web 1.5 is fresh-install-only. The Interface rejects every nonempty root
before mutation; the Compose runners additionally record and check the platform
version in each fresh database volume. Preserve older databases with their exact
runtime for read-only historical access rather than loading them with 1.5.

## Object and execution boundaries

- `standard.make` returns an uninitialized object node; `standard.init` invokes
  `*init*` and returns the initialized node.
- `(sync-eval node)` takes exactly one argument and loads object code in the
  current environment.
- Installed Root, Interface, Standard, Ledger, Federation, and Authorization are
  trusted host composition.
- Tree, Chain, custom, peer, and historical behavior executes through explicit
  `sync-let` child boundaries.
- User payloads are byte vectors stored directly by Tree. `expression?` is an
  Interface codec, not a durable metadata or Document wrapper.

## Testing

The canonical suite is [`tests/suite.toml`](tests/suite.toml). It runs each case
in an isolated process:

```sh
CARGO_TARGET_DIR=journal/target cargo build --manifest-path records/tests/Cargo.toml
journal/target/debug/records-test --suite records/tests/suite.toml --jobs 13
```

Unit cases live under `tests/unit/`; deterministic multi-journal cases live
under `tests/interface/`. See [`tests/README.md`](tests/README.md) and
[`tests/interface/README.md`](tests/interface/README.md) for focused commands
and scheduler semantics.
