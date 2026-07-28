# Reproducible Execution and Journal Host Runtime

Status: active design and implementation contract for the post-`627c3dff` Records refactor. The development line implements exact one-argument `sync-eval`, copied `sync-let` boundaries, direct Standard dispatch with rollback, and primitive-traced Rust proof serialization; release migration and old installed-Root bootstrap remain deferred.

## Motivation

Sync Web supports self-coding objects: a sync node can carry executable Scheme code together with durable state. The point is not to trust code because it was authored locally or reject code because it arrived from another journal. The point is to let shared code run reproducibly in a deterministic, contained environment.

The current Standard object protocol obtains that containment indirectly. `Standard.make` builds an outer object proxy; every method call packages the method, state, and arguments into an invocation sync node; the proxy calls strict `sync-eval`; and the returned state and result are unpacked. This has three problems:

1. Strict execution is voluntary behavior implemented by the object proxy. An arbitrary object is not forced to use it.
2. Friendly host-plumbing calls pay encoding, DAG construction, strict evaluation, and decoding costs for every method.
3. The real security boundary is obscured by a class abstraction rather than enforced explicitly where shared code is entered.

The intended refactor makes containment caller-controlled and coarse-grained. Ordinary journal plumbing uses ordinary Scheme calls. Shared self-coded computation runs inside an explicit `sync-let` boundary.

## Semantic distinction

The primary distinction is not local code versus remote code, and it is not fundamentally portable classes versus non-portable classes.

### Shared reproducible operations

A shared operation is expected to produce the same result for any journal, including the author's journal, from the same evaluator semantics, code, state, and explicit inputs.

Examples include:

- Tree lookup, mutation, slicing, pruning, and merging;
- Chain traversal, append, slicing, and pruning;
- custom self-coded objects;
- proof computations that intentionally execute shared object behavior.

Shared operations:

- carry or consume explicit code and state;
- have no ambient journal, network, root, time, or random capabilities;
- execute within `sync-let` regardless of where the code originated;
- return only inert copied values and immutable sync nodes.

### Journal host plumbing

Host plumbing is necessary to operate one journal but is not represented as independently reproducible object computation.

Examples include:

- root and handler installation;
- request normalization;
- authentication and local authorization policy;
- endpoint and transport handling;
- route selection and synchronization scheduling;
- signed invocation orchestration;
- persistence coordination;
- selecting which reproducible result to commit.

Host plumbing uses ordinary Scheme procedures and classes. Its inputs remain potentially hostile: host code must parse inertly, validate shapes, authenticate, authorize, anchor proofs, and never evaluate supplied code outside a shared-code boundary.

## `sync-eval` as object instantiation

`sync-eval` is ordinary object loading:

```scheme
(sync-eval node)
```

It captures the caller's current environment once, reads and evaluates the code carried by `node` in that exact environment, and may return an ordinary Scheme value, including a callable object procedure. It has no strict-mode or environment-selection argument; `sync-let` is the containment mechanism.

The normal ergonomic object pattern becomes:

```scheme
(let ((tree (sync-eval tree-node)))
  ((tree 'set!) path value)
  (tree))
```

Object methods call one another directly. They do not automatically package each method invocation into a sync node.

Because `sync-eval` is an ordinary loader, host code must never use it on code obtained from requests, journal data, proofs, transferred objects, or custom object state unless execution is already inside `sync-let`.

## `sync-let` as the containment boundary

`sync-let` explicitly enters shared reproducible computation:

```scheme
(sync-let ((node object-node)
           (path requested-path))
  (let ((object (sync-eval node)))
    ((object 'get) path)))
```

Its responsibilities are:

1. Evaluate binding expressions in the caller's host environment.
2. Validate and copy each bound value into an isolated shared-code environment.
3. Evaluate the body in that environment, without capturing host lexical bindings.
4. Permit ordinary `sync-eval` object instantiation inside the boundary.
5. Validate and copy the result back to the host.
6. Sanitize errors so procedures, environments, ports, and other executable capabilities cannot escape.

A boundary should encompass a coherent computation, not one method at a time. For example, one `sync-let` may instantiate a Chain and several Trees, traverse history, slice a proof, and return the final proof node and inert result.

### Mutation

Portable object state remains immutable sync-node structure. An object closure may update its private reference while executing inside `sync-let`, then explicitly return its final state:

```scheme
(sync-let ((node tree-node)
           (path path)
           (value value))
  (let ((tree (sync-eval node)))
    ((tree 'set!) path value)
    (list (tree) #t)))
```

On success, the host receives the returned state node. On error, no state is returned, so the host retains the original node. Unreachable nodes allocated during a failed computation are ordinary garbage-collection concerns, not committed state.

## Boundary values

`sync-let` accepts recursively inert data. This is an allowed-value check, not a new wire encoding.

Expected allowed values include:

- booleans, numbers, characters, strings, symbols, and keywords;
- proper lists and alists containing allowed values;
- vectors and byte vectors containing allowed values where applicable;
- immutable sync nodes;
- existing sentinel expressions such as `(nothing)` and `(unknown)`.

Rejected values include:

- procedures and macros;
- environments and lexical lets;
- ports and continuations;
- arbitrary C objects other than sync nodes;
- cyclic or otherwise non-copyable mutable structures;
- values whose setters or applicable behavior would carry executable capability across the boundary.

The boundary preserves current strict-call copy semantics rather than introducing new semantics:

- ordinary lists, strings, vectors, and byte vectors are copied;
- sync nodes cross as immutable graph handles;
- host-owned mutable Scheme objects are not aliased into the sandbox;
- returned ordinary data is likewise copied before the host receives it.

The exact recursively allowed s7 scalar/container set should be implemented and tested explicitly.

### Deferred size and metering policy

This boundary currently addresses capability isolation and copy correctness, not size/space denial of service. In particular, current C s7 can exhaust its C stack while printing an extremely deep ordinary top-level result even though `sync-let` copies and consumes the same structure iteratively. Do not add an arbitrary boundary depth cap or broaden this refactor into a C-s7 printer rewrite. Metering and deeply nested result serialization remain deferred to the s7-rust runtime; shared production operations should return bounded ordinary control data or sync-node handles in the meantime.

## Isolated environment

`sync-let` must evaluate shared code in a capability environment, preferably defined by an allowlist rather than a growing denylist.

The environment should include:

- deterministic core Scheme syntax and data operations;
- sync-node structural operations;
- digest and hashing operations;
- deterministic expression/byte-vector conversion where required;
- `sync-eval` for instantiating bound self-coded objects.

It must not expose ambient:

- journal root or session state;
- `sync-call`, `sync-http`, or `sync-remote`;
- journal creation or deletion;
- filesystem or port operations;
- environment introspection or mutation;
- host evaluation/loading facilities;
- randomness or system time;
- Interface, Ledger, authorization, endpoint, credential, or secret bindings.

Time, randomness, remote responses, and other external facts enter shared computation only as explicit copied bindings.

A host lexical variable is an ordinary binding captured by installed journal closures, such as `root`, `ledger`, `authenticated-user`, `endpoint`, or a secret. Free variables in shared code must resolve only against explicit `sync-let` bindings and the shared-code environment, never against those surrounding host bindings.

## Classes and methods

`define-class` remains useful authoring syntax for both shared objects and host components. The semantic property belongs to operations, but files should normally avoid mixing shared computation and host plumbing because mixed execution expectations are difficult to audit.

Expected shared object classes:

```text
Standard
Tree
Linear Chain
Log Chain
custom portable object classes
```

Expected host classes or modules:

```text
Ledger
Authorization
Federation
possibly an extracted Interface Core
```

This classification is architectural shorthand, not a trust decision based on code provenance. Shared code always enters through `sync-let`, including on its author's journal. Host code always treats external data as hostile, but it is not packaged as shared object computation.

## Ledger and Interface boundary

The current Interface/Ledger boundary is expected to change substantially.

Ledger is a local stage-and-history concept rather than a firm protocol boundary. It should own:

- staged state;
- permanent and temporary history;
- commit and retention operations;
- local historical lookup;
- bridge-head storage;
- proof construction over already prepared inputs.

Interface and Federation should own:

- public call normalization;
- authentication and authorization;
- endpoints and network calls;
- route selection;
- signed invocation creation and verification;
- synchronization protocol sequencing;
- response anchoring;
- origin-local retention orchestration.

Ledger should not need to know that a request arrived through a particular Gateway or federation transport envelope. Host Interface/Federation code prepares and validates inputs, then calls local Ledger operations normally. Ledger enters `sync-let` when it asks shared Tree, Chain, or custom object code to perform reproducible computation.

## Standard simplification

`Standard.make` should stop wrapping every method in a strict invocation protocol. It should compile an ordinary object closure with the existing state and method-dispatch conventions.

This removes per-method:

- ordinary argument encoding;
- invocation sync-node construction;
- strict evaluator entry;
- explicit state/result pair extraction;
- tagged result decoding.

Host methods may then naturally accept and return dynamic Scheme values, including local procedures where appropriate. Such dynamic capabilities cannot cross `sync-let`.

Containment must be enforced by the caller's `sync-let`, not voluntarily by an object-generated proxy.

## Serialization

Serialization is distinct from method-call containment. It converts a sync-node graph into an inert representation for a transport or storage boundary.

Serialization query selection executes in a fresh child of the masked shared-code environment. The Journal traces primitive `sync-car`, `sync-cdr`, and `sync-cons` calls request-locally, then Rust builds the exact compact `c`/`s`/`p` proof structure from the authoritative Session view. Query source is canonically re-read before evaluation so caller-origin bindings cannot bypass the masked environment.

This should remove the need for confusing shadowed strict/non-strict method behavior. Arbitrary self-coded traversal remains allowed and contained; unknown code is not replaced by a fixed structural code allowlist.

## Caching

The refactor should initially use minimal caching.

Removing per-method strict invocation should greatly reduce loader and codec pressure. Simple reuse of immutable code loaders within the canonical shared environment may remain where naturally correct. More elaborate caching should wait for measurements and the longer-term s7-rust direction.

Evaluated result graphs must not be cached merely by content word. Digest-equivalent objects can have different materialized proof availability, and prior result reuse caused persistent incomplete graphs and severe memory amplification.

## Security invariant

The central invariant is:

> Code carried by a sync node may execute only inside a `sync-let` whose inputs and outputs are recursively inert copied values or immutable sync nodes. Host journal code may instantiate only installed host components outside `sync-let`; it must never evaluate code obtained from journal data, proofs, requests, transferred objects, or custom object state in the host environment.

This invariant permits arbitrary self-coding behavior without class-code allowlists. Containment follows the shared-computation boundary, not whether code is described as local or remote.

Cryptographic signatures and history anchoring answer whose committed object is being inspected. They do not make its code safe to execute outside `sync-let`.

## Validation strategy

The refactor should add focused positive and negative contracts.

Positive contracts:

- locally authored and transferred self-coded objects execute identically inside `sync-let`;
- multiple ordinary object calls occur within one boundary;
- Tree/Chain mutation returns the expected state and digest;
- historical proof and serialization outputs preserve current semantics;
- ordinary host Ledger calls accept useful dynamic Scheme values without portable marshalling.

Containment contracts should provide malicious shared code that attempts to:

- read host lexical variables;
- access root or journal session state;
- perform HTTP, remote, or journal calls;
- access time or randomness without an explicit binding;
- inspect or mutate environments;
- return a procedure, macro, environment, port, or continuation;
- smuggle such a capability inside a list or vector;
- mutate a host-owned list, string, vector, or byte vector through aliasing;
- escape a capability through an error payload.

All attempts must fail without changing host-visible state. The same tests should run for objects authored by the current journal and objects supplied through a serialized proof; provenance must not change execution semantics.

A call-site audit should identify every `sync-eval` reachable from Root, Interface, Federation, Ledger, Authorization, and serializer traversal. Every node containing shared executable code must be instantiated inside `sync-let`.

## Implementation sequence

A bounded sequence is preferable to a single rewrite:

1. Specify and test the copied boundary-value predicate.
2. Implement `sync-let` with an isolated environment and safe error/result transfer.
3. Change `sync-eval` to default to ordinary object loading.
4. Add containment probes for free-variable, capability, mutation-alias, result, and error escape.
5. Simplify `Standard.make` to ordinary method dispatch.
6. Move Ledger to host execution while wrapping its shared Tree/Chain/custom computations in `sync-let`.
7. Rebalance Interface, Federation, and Ledger protocol responsibilities.
8. Move Authorization and remaining host orchestration to ordinary execution.
9. Run a complete call-site audit and remove the obsolete per-method strict invocation machinery.
10. Measure local and zero-to-four-hop workloads before considering additional caching.

Each step should preserve deterministic Records contracts and compare behavior across locally authored and transferred self-coded objects.

## Compatibility baseline

This release line supports fresh installation only. The installer rejects every nonempty journal state atomically, including earlier Tree-native development installations and the released 1.4.3 Document layout. Reset or conversion policy is a separate release decision; no Root bootstrap, recoding, or migration adapter is included here.

Installed Root, Interface, Standard, Ledger, Authorization, and Federation code is host infrastructure. Historical Ledger-shaped structures may travel as inert authenticated data during normal federation, but current host code anchors them and enters `sync-let` only for embedded reproducible Tree, Chain, or custom behavior rather than executing historical Ledger code.

## Non-goals

This design does not:

- distinguish trusted local code from untrusted remote code by provenance;
- add fixed Tree, Chain, or custom-object code allowlists;
- weaken signature, history, authorization, or proof validation;
- introduce result caching;
- define production migration for released 1.4.x data;
- depend on the future s7-rust implementation.

It establishes a clearer execution invariant now while leaving evaluator and caching internals free to evolve later.
