# Source Locator Contract v2

Source v2 binds publications to a web endpoint while preserving each observer's exact federation path as operation evidence. It targets Sync Web 1.6 exclusively and does not parse or migrate Source v1 references.

## Two Bindings

The portable publisher binding contains:

- `endpoint`: the terminal Journal's canonical, authenticated Interface URL;
- `owner`: the terminal owner namespace;
- `index`: the fixed terminal Ledger index;
- descriptor path, byte count, and SHA-256.

The observer binding, recorded in receipts and retention handles, contains:

- `entry-endpoint`: the Interface URL invoked by that observer;
- `route`: ordered outgoing alias names, empty for direct access;
- `history-indexes`: the observer/origin index followed by one absolute index for every route hop.

For N aliases, the history vector contains N+1 indexes. A fixed committed path has the form `[h0, alias0, h1, ..., aliasN, hN+1, *state*, owner, ...]`. The portable reference does not embed an observer's route, so different reviewers can resolve the same publication through their own relationships.

The terminal endpoint is independently probed through its credential-free public `info` descriptor and must self-identify exactly. The observer route retrieves the descriptor and chunks; their fixed hashes associate that route observation with the endpoint-named publication. Replacing the Journal behind an unchanged endpoint is publisher continuity by policy. Changing the terminal endpoint creates a different publisher binding.

Remote endpoints require HTTPS. Literal loopback addresses may use HTTP. Endpoints forbid userinfo, query, fragment, redirects, and environment proxy use. The endpoint path is exactly `/interface`; schemes and hosts are lowercase and default ports are omitted.

## References

Canonical references use the restricted Source S-expression codec.

```scheme
(source-fixed-v2
 (endpoint "https://source.example/interface")
 (owner "publisher")
 (index 31)
 (descriptor-path ("source" "project" "releases" "release-id" "release.scm"))
 (descriptor-bytes 1234)
 (descriptor-sha256 "..."))
```

A current reference carries endpoint, owner, and head path but makes no fixed-history claim:

```scheme
(source-current-v2
 (endpoint "https://source.example/interface")
 (owner "publisher")
 (head-path ("source" "project" "current-head.scm")))
```

`source-current-release-v2` is the content-only current marker. Current-content receipts keep `committed` false, `contentObservedLocator` null, and `selectedIndex` null. A later audit resolves the locator and emits a fixed v2 reference.

## Capture and Replay

Current or first-observation fixed reads use one local authenticated `retrieve-batch` with `index? #t`. Paths contain `-1` at unresolved observer/hop positions and either `-1` or the reference's exact terminal index. Every returned item must report the same nonnegative history vector. Source preserves the outgoing alias order and all absolute indexes.

Continuation batches replay the complete captured committed paths. They do not issue independent latest lookups and do not treat a `route` response's `terminal-index` as the selected data-history index.

## Operations

- `publish` creates chunks and the descriptor, then records endpoint, route, and descriptor evidence as operation-local ready input.
- `ready` probes the declared publisher endpoint, captures one observer history, verifies all resources by exact replay, and emits a portable fixed v2 reference plus observer evidence in its receipt.
- `pull`, `inspect`, and `audit-current` compare the authenticated terminal endpoint, owner, terminal index, and descriptor evidence while recording their own route history.
- `pin-view` carries the publisher and observer bindings into v2 retention handles. `unpin-retention` validates the handle digest and exact committed path without rediscovering a route.
- No mutation is retried after dispatch when completion is uncertain.

Descriptor, chunk, aggregate-tree, and manifest hashes retain their content-integrity roles; locator fields do not replace those hashes.
