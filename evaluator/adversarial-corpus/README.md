# Adversarial correctness corpus

This corpus contains quasi-adversarial, in-subset Scheme programs that currently
break or mismatch the Rust implementation while passing under the C s7 oracle.

Each case is a normal corpus leaf:

- `test.scm` — expression/program under test
- `expected.scm` — C oracle output
- `meta.json` — counterexample notes

Run oracle-only sanity:

```sh
tools/run-adversarial-corpus.py --oracle-only
```

Run against the Rust candidate; this is expected to fail until the bugs are
fixed:

```sh
cargo build --release
tools/run-adversarial-corpus.py
```

The suite intentionally includes semantic mismatches and panic-level bugs across
multiple values, quasiquote, `lambda*`, environments, mutable procedure source,
ports, hash tables, strings/chars, vectors, sorting, and diagnostics.
