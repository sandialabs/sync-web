# Cross-journal sharing flows

Status: exploratory follow-on note. Sync Web 1.5 implements the narrower model in `federation.md`: signed remote application calls are limited to `get`, `set!`, and `resolve`; pin/unpin and administration are origin-local. The arbitrary query, materialization, and step-job concepts below are future possibilities, not current federation behavior.

## Motivation

A useful enterprise shape for Sync Web is moving many small documents, and selected historical versions of those documents, between organizations. The goal is not large-file streaming yet. The goal is a wide, understandable pipeline for asking a peer journal for exactly the data needed, then retaining that data locally.

This note combines two related concerns:

- data-flow primitives for cross-journal synchronization; and
- the privacy boundary for final-mile access to terminal journal data.

Identity, ACL implementation details, and UI flows remain separate design areas, but the data-flow shape should leave room for them.

## Public control plane and private final mile

Bridge traversal should be treated as a public control/reachability plane. It can expose routing, terminal journal identity, head/index information, signatures, endpoint metadata, and other public verification material.

Private application data should be fetched directly from the terminal journal. In particular, bridge traversal should not casually continue through intermediaries into a terminal journal's `*state*`, `*transition*`, document paths, or document values. Those paths and bytes are potentially private.

Avoid cross-journal paths like:

```scheme
(-1 carol dave *state* bob photos trip image.jpg)
```

Prefer a split flow:

1. Traverse bridge/control state to a terminal journal descriptor.
2. Extract terminal journal identity, head/index, public key, and endpoint metadata.
3. Contact the terminal journal directly for private paths under local terminal authorization.
4. Verify returned data against the terminal journal state discovered through Sync Web.

A terminal descriptor might include public data such as:

```scheme
((journal bob)
 (head ...)
 (index ...)
 (public-key ...)
 (endpoints ((https "https://bob.example.net"))))
```

This does not attempt onion routing, encrypted offline relay, or untrusted relay of private bytes. If Bob is offline, Alice may not be able to download Bob's private file. The design chooses private final-mile access plus public multi-hop discovery/integrity.

## Query-shaped transfer

A static manifest is useful, but a more Sync Web-native primitive is to let the caller submit a safe query function to the terminal journal.

Conceptually:

```scheme
(lambda (record)
  ...)
```

`record` is a read capability over a final, resolved ledger view. It is not raw stage/config/root access. The query can invoke `record` with ledger paths and indexes such as:

```scheme
(record '(*state* org folder file))
(record '(-4 *state* org folder file))
```

The responding terminal journal evaluates the function in a safe, read-only wrapper and returns:

- the query result;
- the material needed by the calling journal to reproduce the touched resolved data;
- source context such as journal identity, ledger head/index, query digest, and authorization principal where appropriate.

This makes manifests just one query pattern rather than a special protocol. Callers can ask for latest files, historical versions, subtree listings, digest comparisons, or application-specific selections.

## Safety boundaries

The submitted query should run against a constrained capability:

- read-only access to the resolved ledger view;
- authorization enforced by `record` at every access;
- no mutation, stepping, bridge configuration, private config access, or raw root access;
- no arbitrary network, time, randomness, or host I/O;
- explicit resource budgets for runtime, number of reads, result size, and materialized data size;
- stable source context so callers know what ledger view was queried.

The terminal journal is authoritative for terminal private authorization. Intermediaries should not need user-document ACL awareness.

## Endpoint and ACL requirements

Endpoint reachability should become first-class public metadata. A client that reaches a terminal journal through bridges needs to learn where that journal can be contacted directly.

Endpoint metadata should be bridge-visible and non-sensitive, for example:

```scheme
((journal bob)
 (public-key ...)
 (endpoints
   ((https "https://bob.example.net")
    (webdav "https://bob.example.net/webdav")
    (events "https://bob.example.net/api/v1/events")))
 (updated-at ...))
```

ACLs for private final-mile access are enforced by the terminal journal for paths below its private boundary. Initial ACL work should support path-scoped grants with permissions such as read, list, download, upload, write, and delete. Useful subjects include local users, peer/journal principals, groups, and capability tokens.

## Resolve plus retain

A common workflow is: resolve selected remote data, then retain it locally so future local queries can use it without depending on the peer being online.

This should probably not be a side-effecting option on plain `resolve`; mutation would blur a read-oriented operation. A separate mutating operation should combine resolution and retention.

Candidate names:

- `materialize!` — emphasizes making remote/resolved data locally available.
- `import!` — intuitive for cross-journal movement, but may imply copying into user state rather than retaining source-shaped data.
- `retain!` — matches the storage effect, but may be confused with retention policy setup.
- `fetch!` — familiar, but too transport-oriented.

Avoid `pull!` for now because the term is overloaded and can conflict with other domain language.

Open design questions:

- What exactly is retained: a single address, everything touched by a query, or an explicitly named subset of the query result?
- Should the operation return the same value as `resolve`/query, a retention summary, or both?
- How should quotas and maximum retained size be enforced?
- Should retention of remote bridge paths preserve source context explicitly in the retained shape?

## Step jobs

For moving-target retention, a general step-job mechanism may be better than a bespoke pin policy language.

An admin-configured job would run at a defined point in the step lifecycle. For example, “retain everything currently under `(*state* foo bar)`” can be expressed as a job that retains the current resolved path every step. As the ledger advances, the job naturally retains each new version it observes.

This generalizes beyond retention:

- periodic retention of moving paths;
- scheduled local cleanup;
- metadata normalization;
- recurring imports from known peers;
- operational checks or summaries.

Initial constraints should be conservative:

- admin-only configuration;
- bounded runtime and operation count;
- durable last-run status and last error;
- failure isolation so one failed job does not brick ordinary stepping by default;
- clear lifecycle placement, likely after the normal commit reaches a stable head;
- state mutations from jobs should either be restricted or intentionally apply to a later step.

## Mobile Explorer flow

A mobile-friendly Explorer can use this flow:

1. Alice opens a shared bookmark or discovers Bob's public share descriptor through bridges.
2. Explorer traces through Sync Web to a verified Bob terminal descriptor.
3. Explorer extracts Bob's endpoint and requested head/index.
4. Explorer calls Bob directly with Alice's identity or share capability.
5. Bob returns directory listing, upload target, file bytes, or query results.
6. Explorer verifies returned content against the bridged terminal head.
7. Explorer saves/downloads files through normal browser/mobile APIs.

## Pin and retention enumeration

Pin management remains an open question.

The current concern is operational leak detection: users/operators need to understand what data is being retained so forgotten pins do not become slow storage leaks. Provenance such as who pinned what and why can wait.

A duplicate pin index is straightforward but unattractive because it records information already implied by retained data. Enumerating retained data from the permanent chain avoids duplicate metadata, but the right query semantics are not obvious.

Rejected or unresolved approaches:

- simple prefix listing is too weak; users may need “this document at any index” or other non-prefix views;
- regex/globs are string-centric and do not feel native to ledger paths and indexes;
- a custom structured criteria language is possible but did not yet feel obviously right;
- Bloom filters may help membership or reconciliation later, but they cannot enumerate retained data and have false positives.

Leave this open until the retained-data shape and step-job/materialization workflows make the natural enumeration axes clearer.

## Implementation slices

Likely implementation slices:

1. Define endpoint metadata in public journal/bridge info.
2. Add terminal tracing that stops at the target journal boundary and returns terminal identity, endpoints, and head/index context.
3. Add terminal-journal ACL objects and a central authorization helper for private paths.
4. Add safe query-shaped transfer against a resolved terminal ledger view.
5. Add a mutating materialize/import/retain operation for selected resolved data.
6. Add admin-configured step jobs for recurring retention and related operations.
7. Update Explorer with mobile-friendly shared bookmarks, direct final-mile download/upload, and verification UI.
8. Revisit bridge path grammar to discourage or reject cross-journal traversal into `*state*` and related private namespaces by default.
