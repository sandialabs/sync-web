# Rust s7 Port

## Status

Planning note. This is a potentially large effort and should not begin without an explicit implementation decision.

## Objective

Port the s7 Scheme interpreter used by sync-web from C to Rust while preserving the Scheme-level behavior sync-web depends on.

The success criteria are mostly deterministic: for a large corpus of compatible s7 programs, the Rust interpreter should produce the same observable behavior as the current vendored C s7 baseline.

## Compatibility target

Target the current sync-web vendored s7 baseline, not an abstract future version of s7.

Baseline:

- vendored source: `journal/external/s7/s7.c` and `s7.h`
- s7 version/date: `10.8`, `15-Jan-2024`
- sync-web build profile:
  - `WITH_PURE_S7=1`
  - `WITH_SYSTEM_EXTRAS=0`
  - `WITH_C_LOADER=0`
  - full print length behavior equivalent to current sync-web use

The port should prioritize the language/runtime behavior exercised by sync-web:

- reader/printer behavior needed for API and tests
- normal evaluation
- closures and lexical environments
- first-class environments/lets
- macros
- `lambda*` / keyword and optional arguments
- generalized application and `set!`
- multiple values
- errors, `catch`, and `throw`
- pairs/lists, vectors, byte-vectors, strings, symbols, keywords, numbers, booleans
- equality/equivalence behavior used by records/tests
- byte-vector expression serialization compatibility

## Sync-web-specific decisions

### No C FFI compatibility requirement

The Rust port does **not** need to preserve s7's C API or provide a C-style FFI for primitive additions/calls.

Once the interpreter itself works, sync-web should integrate it through a Rust-native API. That API can be designed around sync-web's actual needs rather than mirroring `s7_define_function`, `s7_call`, C object hooks, or raw `s7_pointer` lifetimes.

Likely Rust-native integration needs:

- register primitive functions as Rust closures/functions with typed argument/result handling;
- expose sync nodes as a Rust-backed Scheme value type or opaque host object;
- evaluate expressions and return structured Rust results/errors;
- convert between Scheme values and gateway JSON/Scheme transport formats;
- support deterministic resource limits where useful.

### Omit currently blacklisted features

The current C-backed evaluator starts with full-ish s7 and then removes/blacklists many bindings in `journal/src/evaluator.rs`.

The Rust port should not implement those unsupported features merely to remove them later. Unsupported/system features should simply be absent unless sync-web explicitly needs them.

Examples of features that should remain out of scope initially:

- filesystem loading and file ports;
- OS/system extras;
- dynamic C loading;
- continuations such as `call/cc` where sync-web intentionally disables them;
- profiling/hooks/runtime inspection features not used by records;
- C pointer/object accessors from s7's embedding API;
- other unsafe or host-environment-dependent root bindings.

This makes the compatibility target: **sync-web-compatible s7 language semantics**, not full upstream s7 embedding compatibility.

## Non-goals

- Do not translate `s7.c` line-by-line into Rust.
- Do not preserve ABI/API compatibility with s7 C headers.
- Do not initially implement all upstream s7 optional/system features.
- Do not optimize before semantic compatibility is strong.
- Do not replace sync-web's record semantics or public API as part of the interpreter port.

## Architectural direction

The Rust implementation should model s7 concepts directly and idiomatically while preserving observable behavior. It should not become one giant `s7.c`-style source file; the C implementation is useful as a behavioral reference, not as an architectural template.

Likely crate/module shape:

- `value` — Scheme value handles, immediate values, type tags, and equality hooks.
- `heap` / `gc` — managed object storage, tracing, roots, and debug invariants.
- `reader` — Scheme parser/read syntax.
- `printer` — write/display/object stringification.
- `env` — lexical environments/lets and binding operations.
- `eval` — evaluator loop, stack frames, special forms, and application.
- `procedure` — closures, primitives, macros, `lambda*` metadata.
- `error` — Scheme error/catch/throw representation and unwinding.
- `host` — future Rust-native host object and primitive integration surface.
- `cli` / `bin` — black-box executable entrypoint used by corpus validation.

Keep modules cohesive, but avoid over-abstracting before behavior is understood. The differential corpus should drive boundaries when s7 semantics force design changes.

Major components:

1. **Value representation**
   - Scheme value enum or tagged arena handle.
   - Must support mutable pairs/vectors/strings/environments and cyclic graphs.
   - Must distinguish symbols, keywords, booleans, nil, unspecified/undefined/eof-like sentinels as needed.

2. **Memory/GC model**
   - Rust memory safety does not eliminate the need for Scheme GC because Scheme values can be cyclic and mutable.
   - Use an explicit managed heap/arena with tracing GC, or another design that handles cycles, mutation, roots, and finalization-like host objects if needed.
   - Rooting should be explicit in evaluator state, environments, stacks, and Rust host values.

3. **Reader/parser**
   - Parse s7 syntax needed by sync-web and tests.
   - Preserve quoted forms, byte-vector syntax, keywords, numbers, strings, vectors, lists, dotted lists, and comments as needed.

4. **Printer/writer**
   - Stable printed representation is critical for differential testing.
   - Preserve current sync-web-visible formatting for values/errors/byte-vectors.

5. **Evaluator**
   - Prefer an explicit evaluator loop and stack over recursive Rust calls where practical.
   - Model lexical environments, procedure application, macros, special forms, and multiple values carefully.
   - Keep tail behavior/performance in mind, but correctness first.

6. **Environments/lets**
   - First-class environments are central to s7 and sync-web record code.
   - Environments are applicable and mutable; this affects both evaluation and generalized setters.

7. **Procedures/macros**
   - Implement closures, primitive procedures, macros, and `lambda*` semantics.
   - Macro behavior should be differential-tested heavily because s7 macros are first-class and permissive.

8. **Host object / sync-node integration**
   - Later Rust-native API should support sync-node values and journal primitives.
   - Initial interpreter can use a minimal host-object abstraction that is sufficient to model sync-web primitive values in tests.

9. **Errors and control flow**
   - Preserve `error`, `throw`, and `catch` observable behavior.
   - Current gateway/records tests expect clear `(error tag message)`-like behavior from the evaluator wrapper.

## Subtle s7 semantics to preserve or explicitly omit

These are the areas most likely to make the port difficult. Each item should either be matched by the Rust interpreter or explicitly documented as unsupported before integration.

1. **First-class macros**
   - Macros are values: passable, assignable, applicable, and capable of having setters.
   - They are not merely compile-time syntax transformers.

2. **`lambda*` / `define*` keyword and optional arguments**
   - Every argument has keyword behavior.
   - Omitted arguments and explicit `#f` can differ at public boundaries.
   - Default expression evaluation order/context needs differential tests.

3. **First-class environments**
   - `(rootlet)`, `(curlet)`, `(funclet proc)`, `(inlet)`, and `(sublet ...)` produce values.
   - Environments are mutable, nestable, inspectable, and applicable.

4. **Environments in function position**
   - `(env 'x)` performs environment lookup.
   - This interacts with general application and setter dispatch.

5. **Generalized `set!`**
   - Assignment can target places such as `(vector 0)`, `(env 'x)`, `(obj key)`, and other applicable/settable objects.
   - sync-web record code relies on this style for object/path mutation.

6. **Applicable non-procedures**
   - Lists, vectors, strings, byte-vectors, hash tables, environments, and host objects can be called like functions when their type supports it.

7. **Procedure/object setters**
   - `(setter obj)` participates in assignment behavior.
   - Some values have custom setter behavior rather than simple variable mutation.

8. **Multiple values splicing**
   - `(values 1 2)` is not a tuple.
   - Multiple values splice into caller argument positions in many contexts.

9. **`catch` / `throw` / `error` behavior**
   - Tags match by `eq?`.
   - Handler argument shape and default error formatting matter for observable compatibility.

10. **Reader/printer round-trip quirks**
    - Symbols, keywords, byte-vectors, quotes, dotted lists, cyclic structures, strings, and special constants need stable formatting.

11. **Equality family**
    - `eq?`, `eqv?`, `equal?`, and any s7-specific equivalence behavior need focused tests for numbers, containers, closures, and host objects.

12. **Mutation and identity**
    - Pairs, vectors, strings, and environments are mutable and identity-sensitive.
    - Copying vs sharing changes semantics.

13. **Lexical environment capture**
    - Closures capture environments that can later be inspected or mutated through mechanisms such as `funclet`.

14. **Macro expansion environment**
    - Expansion can depend on lexical/runtime environment details, especially under s7's permissive macro model.

15. **Bacros**
    - s7 has bacros that expand/evaluate in the caller environment.
    - sync-web likely should omit them unless a corpus check proves they are needed.

16. **Quasiquote/unquote edge cases**
    - Nested quasiquote, unquote-splicing, vectors/lists, and macro-generated structures need dedicated tests.

17. **Tail/evaluator stack behavior**
    - Tail-call optimization is required, not optional.
    - Scheme calls in tail position must not consume unbounded Rust stack.
    - This includes self recursion, mutual recursion, named `let`, and tail calls through `if`, `begin`, `cond`, `case`, `let`, `let*`, and `letrec` bodies.
    - `tools/run-tail-calls.py` is the Rust-only harness for this requirement; see `docs/tail-calls.md`.

18. **Numbers**
    - Exact/inexact behavior, integer/real boundaries, division, comparisons, and parse/print behavior need differential tests.

19. **Special sentinels**
    - `#f`, `()`, unspecified, undefined, eof-like values, and sync-web-layer values such as `(nothing)` and `(unknown)` need distinct handling where applicable.

20. **Host object behavior**
    - sync nodes currently appear as opaque s7 C objects with custom equality/string behavior.
    - Rust host objects need equivalent Scheme-visible semantics for sync-web primitives.

## Differential test strategy

Build a test harness that runs the same Scheme program against:

1. current C s7 as vendored/configured by sync-web;
2. the Rust s7 port.

Compare observable output:

- printed result;
- error tag and message shape;
- environment side effects when relevant;
- read/write round-trips;
- byte-vector and expression serialization;
- deterministic stdout-like output if `print` is part of the test;
- final heap/GC invariants for stress tests where applicable.

Avoid nondeterminism or control it explicitly.

### Metering and interruption tests

Metering is a Rust interpreter contract rather than a C s7 compatibility feature, so it needs a dedicated test suite in addition to the C-oracle differential corpus.

Core expectations:

- `(eval expr env gas)` returns the same Scheme result as `(eval expr env)` when sufficient gas is available.
- `(eval expr env gas)` returns `#<unspecified>` when gas is exhausted.
- `(*s7* 'gas)` reports gas spent by the last `eval` attempt whether the eval returned normally, raised/caught an error, or exhausted gas.
- `(*s7* 'gas)` also reports remaining gas for the currently active outer evaluation, if one exists.
- Nested eval cannot bypass gas. Work in nested evals is charged to the active outer budget as well as any explicit inner budget.
- External interruption/cancellation is tested separately from gas exhaustion and should be distinguishable at the Rust execution-result layer.
- Captured output before normal return, error, gas exhaustion, or interruption remains deterministic.
- Host primitive and sync-node-like operations can be assigned configurable costs and are charged according to the active Rust-side gas schedule.

Test structure:

- Reuse expressions from the main corpus where possible, wrapping them in metered `eval` calls with sufficient and insufficient gas.
- Add targeted metering cases for infinite recursion/loops, nested eval, expected errors under gas, output-before-exhaustion, allocation-heavy expressions, and configurable host primitive costs.
- Prefer invariant/relationship tests early: enough gas succeeds, too little gas returns unspecified, repeated runs consume the same gas under the same TOML cost schedule, larger bounded computations consume more gas than smaller ones.
- Once the cost model stabilizes, add snapshot tests for exact gas numbers under the default schedule.
- Load gas costs from Rust-side TOML in tests so default configuration and custom schedules are both exercised.

## Test corpus layers

1. **Existing sync-web tests**
   - `records/tests/*`
   - journal JSON/Scheme conversion tests
   - gateway examples that depend on Scheme conversion

2. **s7 reference snippets**
   - Extract compatible examples from local s7 docs/source.
   - Skip features intentionally out of scope.

3. **Focused semantic suites**
   - `lambda*`, defaults, keywords, omitted-vs-`#f`
   - macros and macro expansion
   - first-class environments and `with-let`
   - generalized application and `set!`
   - multiple values splicing
   - equality/equivalence
   - reader/printer edge cases
   - byte-vectors
   - errors/catch/throw
   - mutation and cyclic structures

4. **Synthetic/generated programs**
   - Generate small expressions over supported features.
   - Differential-test thousands of deterministic snippets.
   - Include shrinking/minimization for failures if possible.

5. **Long-running stress tests**
   - Allocation-heavy loops.
   - Deep recursive/list/vector operations.
   - Repeated macro expansion/evaluation.
   - Cyclic data and environment retention.
   - Repeated sync-web record install/query cycles.

## GC and memory correctness

Correctness work should include explicit GC validation, not just Rust memory safety.

Important cases:

- cyclic pairs/lists;
- closures retaining environments;
- environments referencing closures;
- mutation of old objects to point at new objects;
- vectors and byte-vectors under heavy allocation;
- temporary values during reader/printer/evaluator operations;
- host objects held by Scheme and Rust;
- error unwinding and root cleanup.

Potential tools/approaches:

- heap invariant checker in debug builds;
- forced-GC mode after every allocation or evaluation step for tests;
- allocation counters and object graph validators;
- differential stress tests against C s7;
- Rust sanitizers/Miri where applicable for unsafe/internal code, though Scheme GC correctness needs its own checks.

## Performance and benchmarking

Performance should be measured after semantic compatibility is credible.

Benchmarks:

- reader/parser throughput;
- printer throughput;
- evaluator arithmetic/list/vector microbenchmarks;
- macro-heavy workloads;
- environment lookup/set workloads;
- byte-vector expression serialization;
- full records test runtime;
- representative gateway/journal API operations;
- long-running social-agent/load-test style workloads.

Compare:

- current C s7 baseline;
- Rust s7 debug and release builds;
- memory usage and allocation counts;
- GC pause/frequency behavior.

## Proposed Rust architecture

The architecture should start from the constraints that are now settled:

- Preserve sync-web records/Lisp-visible behavior. `sync-web/records` should not need semantic changes for the interpreter swap, except for explicitly approved diagnostics.
- Improve the Rust/journal integration layer: memory safety, no C FFI boundary, metered evaluation, captured output, structured results, and safer host objects.
- Prefer isolated evaluator instances over a shared concurrency-enabled evaluator.
- Make fresh evaluator provisioning relatively cheap.
- Make GC deterministic in normal operation.
- Make primitive availability caller-configurable at evaluator construction time, including custom blacklists.
- Do not design cross-evaluator value/prepared-loader caches unless profiling later proves they matter.
- Do not preserve the s7 C API/ABI or upstream embedder/system features that sync-web does not need.

This is a Rust runtime for sync-web-compatible s7, not a direct translation of `s7.c`.

See [`primitive-inventory.md`](primitive-inventory.md) for the generated baseline rootlet inventory, current sync-web blacklist, and sync-web-added host primitive checklist.

### Top-level crate shape

```text
src/
  lib.rs
  main.rs                 # corpus-compatible CLI

  config.rs               # evaluator config, primitive blacklist, gas TOML
  runtime.rs              # Evaluator/Vm construction and public Rust API
  execution.rs            # structured execution result/receipt

  value.rs                # Value, Object, handles, predicates
  heap.rs                 # deterministic arena + tracing GC
  symbol.rs               # symbol interner, keywords, gensym
  number.rs               # integer/ratio/real/complex arithmetic
  pair.rs                 # pair/list helpers, improper/cyclic-safe utilities

  reader.rs               # parser/read syntax
  printer.rs              # write/display/object printing
  port.rs                 # string ports and captured output sinks

  env.rs                  # first-class environments/lets/slots
  procedure.rs            # closures, primitives, macros, lambda*
  eval.rs                 # evaluator loop, special forms, tail behavior
  apply.rs                # procedure/applicable-object dispatch
  set_place.rs            # generalized set! place evaluation
  error.rs                # Scheme errors, catch/throw, stack/context frames

  metering.rs             # gas accounting, interruption, cost schedule
  primitive.rs            # built-in primitive registration
  host.rs                 # Rust-native sync-web host values/primitives
```

The exact module split can change during implementation, but the major boundaries should stay clear: language semantics, runtime/integration envelope, and host/sync-web integration should not be tangled together.

### Public Rust API shape

The integration-facing API should make evaluator construction and execution explicit:

```rust
pub struct EvaluatorConfig {
    pub primitive_policy: PrimitivePolicy,
    pub default_gas_schedule: GasSchedule,
    pub default_output_policy: OutputPolicy,
}

pub struct ExecutionConfig {
    pub gas: Option<u64>,
    pub gas_schedule: Option<GasSchedule>,
    pub limits: RuntimeLimits,
    pub interrupt: Option<InterruptHandle>,
    pub output_policy: Option<OutputPolicy>,
}

pub struct Evaluator {
    vm: Vm,
}

pub struct ExecutionResult {
    pub value: Option<Value>,
    pub error: Option<SchemeError>,
    pub gas: GasReport,
    pub output: CapturedOutput,
    pub interrupted: bool,
}
```

Evaluator configuration describes the evaluator's available primitives and defaults. Budgets, runtime limits, cancellation handles, and output overrides belong to `ExecutionConfig` because they are properties of a single eval/call request.

The CLI used by `tools/run-corpus.py` can remain simple and print one normalized Scheme result. The Rust/journal API should not be forced to parse that printed form; it should receive structured results directly.

### Evaluator lifecycle and cheap provisioning

sync-web often creates a fresh evaluator per request/context. The design should optimize for cheap isolated construction without sharing mutable evaluator state across concurrent requests.

Guidelines:

- Build an evaluator from `EvaluatorConfig` plus a host context.
- Keep mutable Scheme state per evaluator.
- Share only immutable initialization data if it is clearly safe: interned built-in names, parsed primitive metadata, gas schedules, or other static tables.
- Avoid a single global evaluator with locks; it would complicate metering, cancellation, captured output, and request isolation.
- Do not commit to cross-evaluator Scheme value or prepared-loader caching now.

### Value and heap model

Use stable heap handles and an explicit deterministic heap rather than pervasive `Rc<RefCell<_>>`.

Sketch:

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(u32);

pub enum Value {
    Nil,
    Bool(bool),
    Char(char),
    Int(i64),
    Real(f64),
    Ratio(i64, i64),
    Complex(f64, f64),
    Symbol(SymbolId),
    Keyword(SymbolId),
    Undefined,
    Unspecified,
    Eof,
    Obj(Handle),
}

pub enum Object {
    Pair { car: Value, cdr: Value },
    String(String),
    Vector(Vec<Value>),
    ByteVector(Vec<u8>),
    FloatVector(Vec<f64>),
    HashTable(HashTableObj),
    Env(EnvObj),
    Closure(ClosureObj),
    Macro(ClosureObj),
    Bacro(ClosureObj),
    Primitive(PrimitiveId),
    InputStringPort(InputStringPort),
    OutputStringPort(OutputStringPort),
    Host(HostObjectId),
}
```

Reasons for explicit heap handles:

- Scheme values are mutable and can be cyclic.
- Generalized `set!`, `funclet`, environments, cyclic pairs, and host objects all need stable identity.
- GC rooting should be visible in the VM, stacks, environments, host values, and tests.
- Deterministic GC is easier to reason about with explicit allocation order and deterministic tracing.

Initial omissions remain acceptable for full upstream multidimensional vector behavior, GMP/bignums, weak refs, random states, and C-pointer/C-object APIs. Basic int-vector support should be included with the first broad implementation because it is part of the baseline rootlet surface and is not inherently unsafe.

### Deterministic GC

GC should be deterministic in normal operation, not merely in test mode.

Design expectations:

- deterministic allocation order;
- deterministic root traversal order;
- deterministic object marking/sweeping order;
- no dependence on address ordering or hash-map iteration order for observable behavior;
- explicit roots from VM registers, eval stack, catch stack, current environments, protected host handles, ports, and captured outputs.

Also provide a forced-GC/debug mode that collects at hostile allocation boundaries to catch rooting bugs early. That mode is for tests; normal GC should still be deterministic.

### Reader, printer, and output capture

Reader/printer compatibility matters because the oracle corpus compares normalized printed output. Implement reader and printer as normal language components, not as test-only helpers.

Start with:

- lists, dotted lists, quote/quasiquote/unquote forms;
- symbols, keywords, booleans, characters, strings, numbers;
- vectors, byte-vectors, int-vectors, float-vectors;
- comments and datum comments;
- string input ports;
- string output ports;
- current input/output/error ports backed by interpreter-managed captured streams.

Port policy:

- Implement and expose safe in-memory/captured ports: `open-input-string`, `open-output-string`, `get-output-string`, `read`, `display`, `write`, `newline`, char/string byte operations where they apply to managed ports, and port predicates/close/flush operations.
- Current ports should be safe because the Rust interpreter controls them; they should not write directly to the process terminal by default.
- Keep filesystem-backed ports absent unless explicitly exposed through a future host capability: `open-input-file`, `open-output-file`, file `call-with-*`/`with-*` helpers, and file metadata helpers.
- Defer function ports unless needed; they are not OS-dangerous, but they complicate callback/reentrancy/metering behavior.

Output should flow through interpreter-managed sinks:

- `display`, `write`, `newline`, and sync-web's `print` should not write directly to the process terminal by default.
- Evaluation should return captured stdout-like and stderr/error-port output in `ExecutionResult`.
- Lisp-visible port behavior should remain compatible; captured output is an integration improvement, not a record-language semantic change.

### Evaluation loop and control model

Use an explicit evaluator loop and stack as the long-term control model. Recursive helper functions are fine where safe, but the VM should have obvious places to meter, interrupt, track frames, and perform deterministic GC.

Core VM state:

- heap and symbols;
- root environment and current environment;
- evaluation stack/control frames;
- catch frames;
- current gas state;
- captured output sinks;
- host context;
- current result/error state.

Required special forms and runtime behavior:

- `quote`, `if`, `begin`, `define`, `set!`;
- `lambda`, `lambda*`, `define*`;
- `define-macro`, `define-macro*`, `define-bacro`;
- `let`, `let*`, `letrec`;
- `catch`, `throw`, `error`;
- `eval` with optional gas;
- `with-let`.

Application must dispatch on procedures and s7 applicable values: closures, primitives, macros, lists, vectors, strings, byte-vectors, hash tables, environments, and later host objects.

### Metering and interruption as VM primitives

Metering should be built into the evaluator and primitive boundary from the beginning.

Scheme-facing extension:

```scheme
(eval expr env gas)
(*s7* 'gas)
```

Expected behavior:

- `(eval expr env gas)` runs with a gas budget.
- If gas is exhausted, it returns `#<unspecified>` rather than throwing a Scheme error.
- `(*s7* 'gas)` reports gas spent by the last `eval` attempt and gas remaining in the active outer evaluation, if any.
- Nested eval cannot bypass metering: inner work is charged to the active outer budget as well as any explicit inner budget.
- External cancellation/interruption is distinct from gas exhaustion and should be represented in the Rust `ExecutionResult`.

Rust-side configuration:

- Load a gas cost schedule from TOML with sane defaults.
- Charge evaluator steps, special forms, application, allocation, primitive calls, and host operations.
- Include per-primitive and host-operation cost hooks so sync-node traversal, hashing/crypto, serialization, and future network boundaries can be priced separately.

### Environments and closures

`env.rs` is central because s7 environments are first-class values.

Required behavior:

- `rootlet`, `curlet`, `funclet`, `inlet`, `sublet`;
- `varlet`, `let-ref`, `let-set!`;
- `with-let`;
- environment application: `(env 'name)`;
- generalized env assignment: `(set! (env 'name) value)`;
- closure environments that can be inspected and mutated through `funclet`.

The environment model should support cheap normal lexical lookup while preserving first-class mutation and identity semantics.

### Procedures, macros, bacros, and `lambda*`

Represent procedure kind explicitly:

```rust
enum ProcKind {
    Lambda,
    LambdaStar,
    Macro,
    MacroStar,
    Bacro,
    BacroStar,
}
```

`lambda*` needs a dedicated binder:

- required/default args;
- keywords;
- `:rest`;
- `:allow-other-keys`;
- left-to-right default evaluation;
- observable omitted-vs-explicit value behavior.

Macros and bacros should be runtime values, not a separate compile-time-only system. This is required by s7 semantics and by the corpus.

### Generalized application and `set!`

Generalized `set!` should have an explicit implementation path in `set_place.rs`.

Places include:

- variables;
- list/vector/string/byte-vector indexes;
- hash table keys;
- environment bindings;
- procedure setters via `(setter obj)`;
- future host-object setters.

This is central to sync-web record code and should not be treated as incidental syntax sugar.

### Primitive availability and blacklisting

Do not start from a full upstream environment and remove dangerous bindings after initialization. Construct the evaluator with the requested primitive availability.

Design:

```rust
pub struct PrimitivePolicy {
    pub deny: BTreeSet<SymbolId>,
    pub allow_extra: BTreeMap<SymbolId, PrimitiveId>,
}
```

The exact shape can change, but the important point is caller control:

- sync-web can provide its existing blacklist at evaluator construction time;
- stricter `sync-eval`-style restrictions can be provided by the caller;
- the core interpreter should not hard-code opinionated named profiles;
- absent unsafe/system primitives should be absent structurally.

### Host integration

Here "host" is VM terminology: it means the Rust runtime outside the Scheme VM that provides native capabilities to Scheme. In sync-web, host state includes journal/session context, persistors, sync-node storage, caches, crypto/time helpers, output sinks, and gas accounting for native operations.

Do not mirror s7's C object API. Use Rust-native host primitives and host objects.

Sketch:

```rust
pub trait HostPrimitive {
    fn call(&self, vm: &mut Vm, args: &[Value]) -> Result<Value, SchemeError>;
    fn gas_cost(&self, args: &[Value], schedule: &GasSchedule) -> GasCost;
}

pub trait HostObject {
    fn type_name(&self) -> &'static str;
    fn trace(&self, tracer: &mut dyn Tracer);
    fn equal(&self, other: &dyn HostObject) -> bool;
    fn display(&self, vm: &Vm) -> String;
    fn apply(&self, vm: &mut Vm, args: &[Value]) -> Result<Value, SchemeError>;
    fn set(&self, vm: &mut Vm, args: &[Value], value: Value) -> Result<Value, SchemeError>;
}
```

This should eventually support sync nodes and journal primitives such as `sync-cons`, `sync-car`, `sync-cdr`, `sync-eval`, `sync-digest`, and expression/byte-vector serialization without C pointer lifetimes.

### Errors, stack traces, and structured results

Preserve Scheme-level `catch`, `throw`, and `error` semantics. Improve Rust/journal diagnostics through structured results.

Guidelines:

- `catch` sees Scheme errors/throws according to s7 semantics.
- Gas exhaustion returns `#<unspecified>` from metered `eval`, not a catchable Scheme error.
- External interruption/cancellation is integration-level and represented in `ExecutionResult`.
- Stack/context frames should be recorded where the VM naturally has them: Scheme procedure calls, macro expansion, primitive calls, and host operations.
- Do not commit to pervasive source-span tracking yet.

### Functionality to omit initially

The Rust port should omit upstream/system/embedder features that sync-web currently blacklists or does not need:

- C API/ABI compatibility;
- C loader and dynamic FFI;
- file ports, filesystem `load`, and OS/system extras;
- `call/cc`, continuations, and `dynamic-wind`;
- profiling, history, hooks, debugger machinery;
- upstream C pointer/C object accessor APIs;
- weak refs and random states;
- GMP/bignums initially, unless corpus/sync-web tests force them;
- full Snd/Sndlib embedding behavior;
- file/function ports beyond string ports;
- upstream optimization machinery.

The target is sync-web-compatible s7 language behavior plus approved integration-level improvements, not full upstream s7 embedding compatibility.

## Integration constraints summary

The architecture above turns the Rust port's integration goals into concrete runtime constraints:

- preserve records/Lisp-visible behavior while improving the Rust/journal API;
- keep evaluator instances isolated and cheap to provision;
- make metered `eval`, `(*s7* 'gas)`, interruption, captured output, and structured execution results first-class runtime features;
- make normal GC deterministic and provide forced-GC debug testing;
- configure primitive availability, including custom blacklists, at evaluator construction time;
- use Rust-native host primitives and host objects instead of the s7 C API/ABI;
- avoid speculative cross-evaluator value/prepared-loader caching unless profiling later proves it worthwhile;
- do not design versioned runtime negotiation or a separate event/log channel now.

These improvements should not be used as an excuse to drift from sync-web-compatible Lisp semantics. The rule of thumb is: improve the journal/runtime interface, diagnostics, limits, and safety; do not surprise record-language code.

## Phased plan

### Phase 0: baseline capture

- Pin exact C s7 baseline and build flags.
- Build differential harness around current journal evaluator or a small standalone C-s7 runner.
- Collect initial corpus from existing records/tests and s7 snippets.

### Phase 1: reader/printer/value skeleton

- Implement Scheme value model, reader, and printer for core values.
- Differential-test read/write round-trips and printed forms.

### Phase 2: minimal evaluator

- Implement literals, variables, quote, if, begin, define, set!, lambda, application.
- Add primitive arithmetic/list/vector/string basics as needed.
- Start running small generated expression tests.

### Phase 3: s7-specific semantics

- Implement `lambda*`, keywords, environments/lets, generalized application/setters, multiple values, and macros.
- Add focused semantic differential suites.

### Phase 4: errors and sync-web primitive surface

- Implement `catch`/`throw`/`error` behavior.
- Add Rust-native primitive registration sufficient for sync-web journal primitives.
- Start running existing sync-web record tests against the Rust interpreter.

### Phase 5: GC hardening

- Add forced-GC testing mode and heap invariant checks.
- Run stress corpus and long-running record tests.
- Fix retention/rooting/mutation issues.

### Phase 6: integration experiment

- Add an alternate Rust interpreter backend behind a local feature flag.
- Keep C s7 backend available for comparison.
- Run records, journal, gateway, compose smoke, and selected network tests.

### Phase 7: performance and cleanup

- Benchmark against C s7.
- Optimize hot paths only after compatibility failures are under control.
- Decide whether Rust s7 becomes default, optional, or remains experimental.

## Major risks

- s7 has many small semantic edge cases not obvious from sync-web tests alone.
- Macros and environments are unusually first-class compared with simpler Scheme interpreters.
- Generalized `set!` and applicable objects can hide subtle behavior.
- Multiple values splice more pervasively than many Scheme implementations.
- Reader/printer compatibility can become a large surface area.
- GC correctness is a language-runtime problem even in Rust.
- A too-large initial scope could stall before sync-web can benefit.

## Open questions

- Should the first Rust implementation aim for exact s7 behavior or a sync-web-compatible subset with explicit documented deviations?
- How much upstream s7 documentation/test material should be imported into the corpus?
- What Rust GC design best fits mutable Scheme graphs and future sync-node host objects?
- Should the differential harness live under `journal/`, `tests/`, or a new interpreter crate/workspace member?
- Should C s7 remain as a fallback backend indefinitely?
- What performance regression threshold is acceptable if the Rust port improves maintainability and safety?
