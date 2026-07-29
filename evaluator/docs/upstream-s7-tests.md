# Imported upstream s7 tests

Last updated: 2026-06-06.

This repository has two upstream-derived test paths:

1. `tools/run-upstream-s7test.py` — a more faithful suite runner that stages upstream s7 and loads `s7test.scm` with its original harness under the sync-web C oracle profile.
2. `upstream-corpus/` — isolated assertions imported from upstream s7's `s7test.scm` for black-box candidate comparison.

## Purpose

The main `corpus/` remains the curated sync-web-oriented differential corpus.

The faithful suite runner is intended to preserve upstream test semantics as much as possible: it keeps upstream `s7test.scm`'s own setup, macros, state, and ordering.

`upstream-corpus/` is broader but less faithful: it samples upstream s7 unit assertions that make sense for this port's subset and pass the vendored C oracle in isolation.

These paths are intentionally separate because upstream `s7test.scm` is huge and includes:

- filesystem and loading behavior;
- system/host features;
- C loader and optional library tests;
- long timing/performance scripts;
- setup-dependent assertions that only work when the full upstream test file has already run.

## Faithful upstream suite runner

Run:

```sh
tools/run-upstream-s7test.py --timeout 120
```

The runner stages the upstream checkout in a temporary working directory, patches out upstream sections that are outside the sync-web profile/subset, then loads `s7test.scm` under the C oracle. It installs small profile shims for absent pure-profile/system helpers and support procedures normally provided by skipped optional files.

Skipped categories include filesystem-backed ports, C loader/C objects/C functions, optional `libc`/`libm`/`libgsl`/similar library sections, `mockery.scm`, `case.scm`, `lint.scm` blockers, continuations through file ports, and known profile-sensitive assertions such as permissive `(append ... #f)` behavior.

Current C-oracle/sync-profile result:

```text
completed: yes
returncode: 0
skipped-profile-tests: 541
reported-failures-before-stop: 0
```

The faithful runner is therefore a sync-profile upstream smoke test, not a full upstream s7 conformance gate.

## Imported isolated category

The importer keeps only individual `(test EXPR EXPECTED)` forms that:

1. can be parsed as a two-argument upstream `test` form;
2. do not mention unsupported sync-web-subset features such as continuations/`dynamic-wind`, filesystem/system/loading APIs, C embedding APIs, hooks/profiling/debug machinery, or `random-state` pressure points;
3. pass when run alone under the sync-web-profile C oracle;
4. produce no filesystem side effects in a temporary working directory.

## Current category

Current generated set:

```text
upstream-corpus/: 800 cases
source: ~/projects/miscellaneous/s7/s7test.scm
spread: max 100 passing isolated cases per 5000 source-line window
oracle status: expected-current 800/800
```

Each case has:

- `test.scm` — self-contained wrapper around one upstream assertion;
- `expected.scm` — C oracle snapshot;
- `meta.json` — source line, original upstream form, and category metadata.

## Commands

Build the oracle first:

```sh
tools/build-s7-oracle.sh
```

Regenerate the imported category:

```sh
tools/import-upstream-s7-tests.py \
  --replace \
  --max-cases 800 \
  --max-per-window 100 \
  --window-lines 5000 \
  --timeout 2
```

Run oracle-only validation for the category directly:

```sh
tools/run-upstream-corpus.py --timeout 5
```

This oracle check and candidate comparison are included in the default validation flow:

```sh
tools/test.py
```

Expected current result:

```text
expected-current: 800/800
candidate: not provided
```

Candidate comparison can also be run directly:

```sh
tools/run-upstream-corpus.py --candidate target/debug/s7-rust --timeout 5
```

## Notes

- This category is generated from upstream material, but it is not a wholesale copy of upstream `s7test.scm` execution semantics.
- Passing this category does not imply full upstream s7 compatibility.
- Failing this category after candidate comparison should be triaged against sync-web relevance before implementation work; do not implement explicitly unsupported features merely to improve this score.
- `tools/test.py` runs this category against both oracle and candidate by default. Use `tools/test.py --skip-upstream-candidate` for a faster sync-web-focused candidate pass when needed.
