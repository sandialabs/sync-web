# Runtime architecture

This document describes the sync-web-compatible runtime implemented by `s7-rust`.
The vendored C s7 oracle remains the semantic authority.

## Value representation

`Value` is a 16-byte tagged Rust enum. Immediate booleans, integers, floats, and
characters are inline. Payloads that would enlarge the enum are held by a
single-word shared handle. Their definitions remain adjacent to `Value` in `src/core.rs`; moving them across module boundaries has measurable release-code layout costs.
A compile-time assertion in `src/core.rs` prevents accidental representation
growth.

The representation is small but is not yet a bitwise `Copy` word: shared
payload clones still perform `Rc` operations. Optimizations must not assume
that cloning a `Value` is free.

## Pairs and identity

Pairs use stable `PairRef` handles into a thread-owned, nonmoving arena. Arena
chunks contain `PairCell` values, so pair identity and mutation remain stable
across allocation. Cycles are valid. The arena may only be reset at a boundary
where no live Scheme value can retain a pair handle.

Pairs deliberately remain `RefCell`-protected. Replacing the borrow checks with
unchecked aliases would require a separate proof covering callbacks,
comparators, generalized setters, and cyclic values.

## Environments and callable guards

Environments use small linear binding storage and promote to an FNV hash map
after eight bindings. Related environments share callable-shadow and guard
generation state. Compiled and native code validates the bindings it assumed;
shadowing or mutating a callable invalidates or bypasses optimized code.

The compiler may use slot frames only when environment-observing operations are
absent or otherwise guarded. Known lexical bindings are propagated through
`let` bodies, including locally bound applicable objects.

## Execution tiers

Execution proceeds through conservative tiers:

1. the tree evaluator and tail-call trampoline;
2. compiled `CExpr` bodies;
3. private bytecode functions using slot frames where safe;
4. private Cranelift lowering for guarded integer loops and lambdas.

Every optimized tier has an ordinary evaluator fallback. `unsupported-compiled-form`
is a pre-entry rejection for a framed body, not permission to replay a body
that has already produced effects. Instruction-local slow paths must resume at
the operation or return a structured error; they must never restart prior
Scheme code.

Native code is deterministic and in-memory. It does not expose native loading,
filesystem, process, network, random, or debug authority to Scheme.

## Unsafe-code invariants

- `PairArenaOwner` converts its raw allocation back into a `Box` exactly once at thread exit. `PairRef` dereferences are valid only while the arena has not been reset; reset requires that no Scheme or host root retains a pair.
- Callback-free list cursors only use pointers obtained from rooted `PairRef` values. Arena cells do not move during traversal.
- The `qsort_r` comparator receives elements from a live vector stable-slot array and a live comparator context; both outlive the complete native sort call, including recursive Scheme callbacks.
- Cranelift pointers are transmuted only after the corresponding function was declared, defined, and finalized with the exact stored C ABI signature. The thread-local JIT module outlives all calls through those pointers.

## Multiple values and generalized mutation

`Value::ValuesData` is an internal splice carrier. Calls, boolean forms,
quasiquote, binding, setters, and diagnostics each have s7-specific handling;
it must not be treated as an ordinary list or silently truncated.

Applicable vectors, lists, strings, hash tables, environments, and cooperating
procedures preserve normal mutation and error semantics. Vector sorting uses
stable movable slots while the comparator runs so comparator-visible mutation
matches the C oracle.

## Performance validation

Use process-level comparisons against the C oracle. Because oracle timings can
vary between runs, benchmark reports may also compare candidate medians to an
adjacent baseline report via `--baseline-report`. A change is retained only
when correctness gates pass and candidate-time balance is favorable.
