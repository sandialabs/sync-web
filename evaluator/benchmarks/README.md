# Benchmark suite

This directory contains benchmark-oriented Scheme programs for comparing the C s7
oracle and the Rust candidate as black-box executables.

Unlike `corpus/` and `upstream-corpus/`, these cases are intended to be large
enough to expose evaluator/runtime performance. They are not part of the default
correctness gate yet.

Run an optimized comparison with, for example:

```sh
tools/build-s7-oracle.sh
cargo build --release
tools/run-benchmarks.py \
  --oracle target/c-oracle/s7-oracle \
  --candidate target/release/s7-rust \
  --repeats 5 --warmups 1 --timeout 60
```

The runner uses the oracle output as the expected output for candidate validation
and reports per-process timings. Treat results as coarse performance signals,
not final microbenchmarks.

Useful options:

```sh
# Run one category/tag.
tools/run-benchmarks.py --category hash-table --repeats 5

# Sort by worst median candidate/oracle ratio.
tools/run-benchmarks.py --sort ratio --repeats 5

# Save machine-readable and reviewable reports.
tools/run-benchmarks.py \
  --json-report target/benchmarks.json \
  --csv-report target/benchmarks.csv \
  --markdown-report target/benchmarks.md
```

Current categories cover tail calls, non-tail recursion, closures, environments,
lists, list mutation, vectors, byte-vectors, hash-tables, strings, reader-heavy
literals, macros, quasiquote, `apply`/multiple-values, sorting, and `format`.
