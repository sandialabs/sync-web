# Specifications

Formal methods applied to sync-web through two complementary lenses, each suited to
a different part of the problem.

## TLA+ — behavior over time

Journal coordination is a state machine problem. TLA+ is the right tool for:

- what interface/root/bridge transitions are allowed, and in what order
- what cross-journal invariants must hold across time
- whether synchronized history can be retroactively invalidated by a source journal

The central invariant being pursued: once a target journal accepts a synchronized history
snapshot from a source journal, later root activity on the source cannot retroactively
falsify that accepted view. This is `SynchronizedHistoryStable` in the current sketch.

Journals are modeled as composed root/interface control surfaces; tree/chain/ledger
internals stay abstract. This keeps the state space manageable and focuses the model
on the questions that are actually temporal.

Likely eventual module split: `root.tla`, `interface.tla`, `journal.tla`, `network.tla`.
Deferred until the abstraction vocabulary stabilizes.

## Alloy — structure and reachability

The object graph has properties that are not temporal — they are relational facts about
what exists, what is reachable, and what can be written. Alloy handles these naturally:

- which nodes are reachable from a given tree or chain root
- whether a `sync-eval` computation can escape its authorized region
- authorization as a graph property: who can read or write which subgraph
- structural consistency across Merkle nodes, objects, and journals

An important observation: because sync-web reifies history as graph structure, many
questions that sound temporal are structurally answerable at a snapshot. A question like
"can a user on journal A reach journal B's state from 5 indices ago?" is an Alloy question
if historical structure is already in the graph.

The clearest near-term target is `sync-eval` safety: modeling that a `sync-eval`
computation's read/write footprint stays inside the authorized subgraph and cannot
structurally escape. This is one of the sharpest fits between the problem and what Alloy
is good at.

## Invariant catalog

The formal models are only useful if their invariants connect to real code. The bridge
is a named invariant catalog — specific claims, each with a code location, a related spec
file, and a candidate runtime assertion or property test.

Examples of invariants that should appear in such a catalog:

- read-only operations do not mutate committed state
- synchronized history remains valid after root activity
- `sync-eval` cannot escape its authorized region
- bridge synchronization preserves trust assumptions

These are more system-specific than the formal tools themselves. The catalog is the
actual semantic backbone; TLA+ and Alloy are how the invariants get stated precisely
enough to check.

## Property testing for Scheme

No mature s7 property testing framework exists. A small custom one is feasible and
probably more valuable than trying to adapt a general framework. The strongest early
targets are structural: serialization roundtrips, digest/path/slice/prune/merge
invariants, and `standard` encoding properties. A good first version needs only a
deterministic random source, small domain-specific generators, a run-many-times harness,
and seed/case recording on failure.

## Scheme tooling

Effect annotations are more important than types for this codebase. The interesting
categories are not data types but computation regions: `pure`, `remote`, `write:stage`,
`write:perm`, `root`, `sync-eval-safe`. Optional comment-based annotations
(e.g. `@effects`, `@sig`) checked by an external tool keep the annotation burden light
and out of the core language.

A repo-specific s7 LSP — not a general-purpose one, but one that understands sync-web's
object protocol, top-level `define` structure, and known special forms — would materially
improve both human and AI-assisted coding quality. The checker and the LSP are separate
tools that compose: the LSP surfaces diagnostics from the checker.
