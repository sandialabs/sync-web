# Unified runtime architecture

Status: Milestone A prototype, isolated from the accepted evaluator.

The production goal is one runtime representation rather than another tier connected to the legacy `Value`, Word, or frozen-proxy runtimes.

```text
content-addressed immutable compiled modules
                    ↓
          8-byte Scheme handles
                    ↓
             explicit-frame VM
                    ↓
 interpreter-owned paged tracing heap
                    ↓
       explicit Rust host root leases
```

## Ownership model

Rust owns the heap. The heap owns every mutable Scheme object. Scheme graph edges are copyable `GcValue` handles, not `Rc` ownership edges.

A heap handle contains a slot index and reuse epoch. Reclaimed slots increment their epoch, so stale handles cannot alias later objects. Immediate fixnums, characters, booleans, nil, unspecified, undefined, and EOF do not allocate.

Host-visible values use explicit root-table leases. Cloning a lease shares one root entry; dropping its final clone removes the root. A collector may reclaim the value only after it is no longer reachable from a lease or active VM root.

The explicit-frame VM will contribute its value stacks, environments, pending calls, and temporary registers to the root set at collection safepoints. Collection cannot run while unregistered raw handles exist in native local state.

## Collection and request generations

Milestone A implements iterative tracing and sweeping over fixed-size pages. Cycles are ordinary graph edges and need no special ownership treatment.

Each request can allocate into a fresh generation. At request completion:

1. mark from explicit roots;
2. reclaim unreachable request cells in bulk;
3. promote escaped survivors into the long-lived generation;
4. retain pages for cheap reuse.

Full collection traces all generations. Allocation quotas fail closed with a structured resource condition after a collection cannot recover enough space.

This initial collector performs a full root trace when closing a request generation. Later nursery collection may add remembered sets, but must not weaken exact identity or mutation semantics.

## Immutable modules

`ImmutableModule` owns only immutable instructions and frozen constants through `Arc` storage. It cannot contain heap handles, environments, host/session values, mutable cells, or native pointers.

Future cache keys must include source digest, semantic/compiler ABI, builtin inventory, metering mode, and behavior-relevant flags. Modules may be shared across requests and interpreter instances; every mutable closure/environment remains request-owned.

## Milestone A evidence

Focused tests cover:

- 8-byte handles and immediate boundaries;
- rooted cyclic pairs;
- environment/closure cycles;
- request generation reclamation and escaped-value promotion;
- stale-handle rejection after slot reuse;
- hard allocation quotas;
- bounded page reuse over 100 request cycles.

The release allocator diagnostic creates 500,050 representative environment/closure objects:

```text
legacy Rc graph: approximately 101 ns/object
unified GC heap: approximately 5 ns/object
observed diagnostic speedup: 18.61x
```

This is a representation microbenchmark, not an integrated performance claim. It establishes that bulk heap ownership plausibly removes the allocator/drop bottleneck seen in matched Journal profiles.

Current sizes on x86-64:

```text
GcValue: 8 bytes
GcObject: 64 bytes
Slot: 80 bytes
```

The 80-byte prototype slot is 1.67x the vendored C s7 48-byte cell and remains below the eventual 2x memory ceiling before page/free-list overhead. Object-class splitting or packed slot metadata can reduce it later if integrated RSS requires it.

## Explicit-frame VM checkpoint

Milestone B adds the first execution slice over the same handles and heap:

- one non-recursive value stack and frame stack;
- immutable module/function metadata;
- lexical frames in the tracing heap;
- closures containing only module/function/environment handles;
- ordinary and tail calls;
- guarded frame reuse only when compilation proves the call environment cannot be observed or captured;
- branches, locals, constants, fixnum arithmetic, pairs, and returns;
- active VM roots supplied to collection safepoints;
- exact step quotas.

A 100,000-iteration tail loop stays at frame depth two while repeated nursery collections preserve active values. The initial generic dispatch takes approximately 4.3 ms versus 0.17 ms for the accepted evaluator's already-specialized named-loop compiler. This is not yet a performance pass: the unified backend must carry forward conservative whole-loop/trace compilation into immutable modules rather than interpreting twelve generic instructions per recurrence. The result nevertheless bounds generic dispatch at roughly 3.6 ns/instruction and keeps the architecture compatible with shared verified traces.

The go/no-go remains credible for the matched Journal workload because its measured cost is dominated by graph allocation/drop rather than arithmetic loops, and the new allocation lifecycle diagnostic is approximately 18.6x faster. The tight-loop result is an explicit risk and must improve through general compiler traces before integrated acceptance.

Milestone C starts the immutable compiler without routing accepted evaluation through it. Parsed source is lowered into module-owned constants, function metadata, lexical-slot operands, and instructions; no legacy `Value`, environment, or pair is retained in a module. The current semantic slice covers definitions, lexical closures, mutation, ordinary/tail calls, named `let`, `let*`, `lambda*` defaults/keywords, branches, `and`/`or`/`cond`/`case`, quote/quasiquote/splicing, multiple values, apply, first-class environments, applicable lists/vectors/hashes, generalized setters, catch/throw, dynamic eval, and first-class macros including dynamically installed macros. Compiler tests cover recursive definitions, captured mutation, exact-once macro expansion, dynamic invocation, explicit catch unwinding, tail-frame safety, and pre-effect rejection of unbound authority such as `load`. An opt-in `S7_UNIFIED=1` CLI path exists only for differential development.

Cold environment/hash payloads are boxed so the paged heap cell remains exactly 48 bytes on x86-64, matching the vendored C s7 cell size. This is an interim layout: pooled side slabs should replace per-environment boxes before integrated performance acceptance.

## Required continuation gates

The representation and first VM microgates are credible, but no accepted evaluator route changes yet. Subsequent checkpoints must preserve:

- no replay across host calls;
- exact multiple values, macros, environments, mutation, setters, and diagnostics;
- metering and authority boundaries;
- evaluator differential suites and exact Sync Web durable roots;
- matched c1/c4 CPU at no more than 4x C for the authorized endpoint;
- no performance claim before semantic gates pass.
