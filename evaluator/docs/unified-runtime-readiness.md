# Unified runtime readiness invariants

Status: private `S7_UNIFIED=1` runtime hardening. This document freezes ownership and control-flow boundaries for later Sync Web integration; it does not expose an integration API.

## Immutable interpreter state

`ImmutableModule` is the only compiled-code container. Its code, function descriptors, and `FrozenConstant` values contain no `GcValue`, environment, host/session value, mutable Scheme object, or request pointer. Installed modules may outlive requests because escaped closures name them by `ModuleId`.

The approved builtin metadata registry is versioned and generated from the pinned vendored `s7.c` `H_*` and `Q_*` macros. Each available entry records source macro/line provenance; the registry records the source SHA-256. `evaluator/tools/generate-builtin-metadata.py --check` is the deterministic freshness gate. Authority-filtered unavailable entries remain explicit.

## Fresh request state

`UnifiedVm::reset_request_state` is the single request boundary. It clears:

- operand/control stacks, handlers, temporary roots, conditions, gas, mutable print length, gensym sequence, and dynamic port state;
- all pending macro/invoke/map/search/sort/hash/port/eval continuations;
- request-local builtin/constant/syntax/dynamic-literal values and dynamic-eval cache;
- request metadata side tables (setters, typers, ports, mutability, environments, procedure sources, defaults, and features).

It does not clear immutable installed modules or explicit `RootLease`s. A successful result is rooted before `finish_request_generation`; unreachable request objects are reclaimed, while values reachable from a lease are promoted. Errors are rendered before the request generation finishes.

A surviving `RootLease` guarantees memory validity for host observation, serialization, and drop only. Its value is rejected as an argument to a later `run`: request reset deliberately clears semantic side metadata such as gensym identity, custom setters, immutability, weak-table status, subvector parents, typers, environment flags, procedure sources/defaults, and dynamic literals. No cross-request invocation or semantic-continuity promise exists. A future continuity API requires owner approval, generation validation, and explicit promotion of every relevant side table; it must not silently reuse a lease as a VM argument.

Dynamic quoted literals are request-owned traced `GcValue`s. They are never placed in the content cache and are cleared at the next request boundary.

## Root inventory

`active_roots_with` is the allocation/collection root inventory. It includes:

- operand values, VM frames, handlers, temporary condition roots, root/default/current ports;
- request-local builtin/constants/syntax/dynamic literals and callable metadata;
- setter, vector-typer, subvector parent, procedure-source, and lambda-star default graphs;
- every `GcValue` owned by pending output/function-port/scope/sort/map/hash/search/invoke/macro state, including sort merge buffers and search visited nodes;
- operation-local extra roots supplied by the allocating caller.

Side tables whose entries are observations only (line numbers, open/closed flags, gensym membership) do not independently keep dead Scheme objects alive and are cleared at the request boundary.

## Exactly-once resumable ownership

Each pending operation records the owning frame depth and stack depth. `truncate_resumable_state` is the single exceptional/metered-unwind cleanup path. Both coordinates matter because tail replacement can start an operation at the same frame depth as an enclosing catcher. Cleanup discards all operations at or beyond the caught operand boundary and restores dynamic input/output scopes. Normal completion consumes the top pending operation once before injecting its result. No path may replay a callback, macro expansion, setter, comparator, host call, or already-entered expression.

Pending state added in the future must be included in both `active_roots_with` and `truncate_resumable_state`, with forced-collection and exceptional-unwind tests.

## Graph identity and cycles

`graph_has_cycle_by` and `graph_reaches_identity_by` are the shared traversal algorithms for frozen `Value` graphs and request-local `GcValue` graphs. Runtime edge enumeration is centralized in `gc_graph_children`; reconstructive display edges are centralized in `display_graph_children` because print-length truncation is a presentation boundary.

Traversal must preserve identity/sharing, terminate on cycles, and never reconstruct mutable graphs from rendered text. Quoted runtime graphs remain request-local dynamic literals. Cyclic syntax errors occur only when a cyclic node is consumed as syntax. Outlet parent cycles reject before mutation.

## Forbidden integration shortcuts

Do not add thaw/proxy/frozen bridges, a persistent mutable evaluator, dual authoritative representations, replay after partial effects, source/name recognizers, unsafe host handles, or direct filesystem/process/network/native-loader authority. Any public API, system-policy decision, or broad heap/backend redesign requires owner approval.

## Acceptance gates

Before retaining readiness changes:

```sh
evaluator/tools/generate-builtin-metadata.py --check
# adversarial 1586/1586; upstream 800/800
# focused/evaluator/metering/tail/system-authority
# frozen setter/procedure/cyclic/dynamic-literal/metadata matrices
# git diff --check; no evaluator/Cargo.lock
```
