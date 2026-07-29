# Pair arena ownership and reclamation

The legacy `Value::Pair` representation uses stable nonmoving arena cells. A
`PairRef` remains one machine word and `Value` remains two machine words. Pair
references are intentionally cheap internal handles; lifetime is owned at the
whole-evaluation arena-generation level rather than by each pair clone.

## Evaluation generations

Each public `run_source` call creates a fresh `PairArenaGeneration` and evaluates
inside it. Parsing, evaluator state, errors, and the returned graph therefore use
one nonmoving generation. A successful result is returned as `OwnedValue`, which
contains both the internal `Value` and an owning `PairArenaLease`. A structured
`SchemeError` likewise retains the generation when it escapes evaluation.

Dropping the last owner of the result or error drops the entire generation and all
of its chunks. This reclaims acyclic, cyclic, and shared graphs without tracing or
per-pair reference-count traffic. It also prevents another evaluator or helper on
the same thread from resetting cells still referenced by a public result.

`run_source_output_repeated` uses a private generation for the complete repeated
operation. Its final reset is safe because no `Value` escapes that generation.
The default thread-local arena remains available for internal tests and legacy
operations outside a public evaluation scope; it is not used to back returned
public values.

## Public boundary

`run_source` returns `OwnedValue`, not a bare internal `Value`. `OwnedValue` exposes
safe formatting while retaining its arena. The raw graph is intentionally not
extractable through the public API because doing so would detach pair addresses
from their owner. Future Rust-native engine and host APIs must follow the same
rule: borrowed Scheme values are scoped to a live engine/callback, and any value
that escapes carries its arena generation or is converted into host-owned data.

The interpreter is single-threaded. Its `Rc`-backed values and arena leases are
not `Send`; independent interpreter threads use independent current-generation
scopes.

## Safety invariants

- Every public result or error that can contain a pair owns its generation.
- A generation cannot be reset while an owning result/error can escape.
- Nested arena scopes restore the previous current arena even during unwinding.
- Pair addresses never move within a live generation.
- Dropping one generation cannot invalidate pairs from another generation.
- Cycles and shared pair graphs need no special reclamation path: generation drop
  releases them as a unit.
- Host callbacks may borrow internal values only for the duration of the callback;
  retained values must use an owning public representation.
