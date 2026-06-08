# s7 differential corpus

Each corpus case is a directory with uniform leaf files:

```text
corpus/<case-name>/
  test.scm
  meta.json
  expected.scm   # generated later from the C oracle
```

`test.scm` should be self-contained and evaluate to one final value. The oracle harness runs the same file against the current sync-web C s7 baseline and the Rust candidate interpreter.

`meta.json` fields:

- `status`: `required`, `nice-to-have`, or `unsupported`
- `category`: broad grouping such as `lambda-star` or `environments`
- `features`: feature tags used for filtering/reporting
- `description`: short human explanation

The C oracle remains the source of truth. `expected.scm` snapshots, when present, are cached review/debug artifacts rather than the authority.
