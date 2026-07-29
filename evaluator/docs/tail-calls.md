# Tail-call requirement and test plan

Last updated: 2026-06-06.

Tail-call optimization is a hard requirement for the Rust s7 port. Scheme code in sync-web can naturally use recursive loops, named `let`, mutually recursive helpers, and tail calls hidden behind control forms. The Rust interpreter must not consume unbounded Rust stack for Scheme tail calls.

## Requirement

A Scheme call in tail position must execute in constant Rust stack space.

This includes tail positions through at least:

- procedure bodies and the final expression of `begin`;
- `if` branches;
- `cond` and `case` selected clauses;
- `let`, `let*`, named `let`, and `letrec` bodies;
- tail calls to the same procedure;
- tail calls to another procedure, including mutual recursion;
- metered evaluation via `(eval expr env gas)`.

The current CLI runs the evaluator on a larger-stack thread as a temporary safety crutch. That is not a substitute for TCO. The tail-call harness uses sufficiently deep recursion to catch stack-consuming evaluators despite that crutch.

## Current harness

`tools/run-tail-calls.py` is a Rust-only candidate harness. It generates deep tail-recursive Scheme programs and checks that the candidate CLI returns the expected result.

Run it with:

```sh
tools/run-tail-calls.py --candidate target/debug/s7-rust
```

Useful options:

```sh
# Lower depth for quick debugging
tools/run-tail-calls.py --candidate target/debug/s7-rust --iterations 10000

# Higher depth for stronger stack-safety checks
tools/run-tail-calls.py --candidate target/debug/s7-rust --iterations 1000000 --timeout 30
```

The harness currently covers:

1. Deep named-`let` loop.
2. Deep self-recursive procedure.
3. Deep mutual recursion (`even-tail?` / `odd-tail?`).
4. Tail calls through `begin`, `cond`, and `let`.
5. Deep tail recursion inside metered `eval`.

It reports:

```text
tail-call-current: PASSED/TOTAL
iterations: N
```

## Current status

TCO is implemented at the prototype level and this harness is part of the default full validation command, `tools/test.py`.

The current implementation uses a tail-position evaluator/trampoline for procedure calls and key control forms. The CLI still runs on a larger-stack thread as a temporary safety crutch for non-tail recursive paths, but tail-position calls covered by this harness complete at deep recursion depths without stack overflow.

## Implementation direction

The preferred implementation is a real evaluator trampoline or explicit evaluator loop, not a special-case self-recursion optimization. The current prototype follows this direction for covered tail positions.

The implementation should preserve these properties:

- Tail calls return a tail-call continuation/outcome instead of recursively invoking Rust `eval`/`apply`.
- The trampoline repeatedly evaluates tail outcomes until a value/error is produced.
- Non-tail calls may still use ordinary nested evaluation temporarily, but tail positions must not grow the Rust stack.
- Metering must charge tail-call work normally; a tail-recursive loop with insufficient gas must still return `#<unspecified>` from metered `eval`.
- Existing corpus and metering validations must remain green.

## Acceptance checks

After TCO implementation, run:

```sh
tools/test.py
tools/run-tail-calls.py --candidate target/debug/s7-rust --iterations 200000
tools/run-tail-calls.py --candidate target/debug/s7-rust --iterations 1000000 --timeout 30
```

Acceptance target:

- `tools/test.py` passes.
- Tail-call harness reports all generated cases passing.
- No stack overflow, abort, or timeout in deep tail cases.
- Metered tail recursion obeys gas accounting and still completes with abundant gas.
