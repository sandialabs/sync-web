# Sync Web 1.5 development plan

Status: historical planning note, superseded for federation details by `federation.md` and the current `dev-1.5` implementation. In particular, signed application federation is now limited to `get`, `set!`, `get-batch`, `set-batch!`, and `resolve`; interface administrators are local principals; pin/unpin and all administration remain local.

Working plan for the 1.5 feature integration branch. This note consolidates current discussion plus still-relevant parts of existing idea notes. Older notes remain in place for reference and may be deleted or folded in later.

## Scope

Primary 1.5 topics:

- journal-to-journal authentication grounded in bridge-derived identity;
- path-shaped authorization principals;
- public resolvable journal descriptors and endpoint metadata;
- custom local objects for application workflows;
- cron-like scheduled actions tied to journal stepping.

A near-term motivating use case is an agentic prediction market: specialist forecaster and arbiter agents post questions, forecasts, stakes, evidence, resolution decisions, and payouts as auditable journal actions. Participants may live on different journals, so the market needs bridge-verifiable identities, clear write permissions, scheduled settlement actions, and typed local objects.

## Design posture

Journals should double as:

1. verifiable state/history substrates; and
2. authenticated coordination endpoints around that state.

Ad hoc journal-to-journal communication is acceptable, but it is not itself durable truth. Durable claims should either be written into journal state or returned with enough proof/material to verify against a journal head.

Bridges remain the public control and trust plane. Private or user-specific final-mile operations should be terminal journal requests, not arbitrary traversal through intermediate bridged paths into another journal's private state.

## Bridge-derived journal authentication

Do not introduce a separate random-web TOFU peer identity registry for 1.5. Meaningful journal identity should be bridge-derived.

A receiving journal should treat a remote journal as meaningful only when the alleged caller identity can be resolved from the receiver's perspective through its bridge graph. Out-of-band coordination is acceptable for discovery: Alice and Bob may coordinate endpoint/name/path manually, but Bob's journal should verify Alice's current identity material through a bridge relationship before granting private or privileged access.

Authentication flow:

1. Caller claims a principal path, interpreted from the receiver's journal.
2. Receiver resolves that path through local state and bridge state to a public journal/user descriptor.
3. Receiver obtains the current public request-verification key from that descriptor.
4. Receiver verifies the request signature over the operation and arguments.
5. Receiver authorizes the operation using the resolved principal path and local policy.

This replaces the earlier standalone peer registry idea from `journal-peer-identities.md`. Still-relevant pieces from that note:

- signed requests are preferred over bearer peer secrets;
- public operations such as `info`/`size` can remain unauthenticated;
- signed messages must bind operation and arguments tightly;
- operation-level policy vocabulary such as `public`, `registered`/`verified`, and `deny` may still be useful;
- errors should distinguish malformed identity, authentication failure, and authorization denial.

Non-goals for the first slice:

- global discovery of all journals;
- user-managed private/public keys;
- arbitrary unbridged TOFU trust;
- path-prefix ACLs for unknown random web keys.

## Path-shaped principals

Current authentication uses a bare symbol identity such as `alice` and defaults missing identity to `*journal*`. For 1.5, identities should become journal-relative principal paths.

Canonical principal examples:

```scheme
()                              ; root/current journal principal
(*state* alice)                 ; local user alice
(bob)                           ; bridged journal bob
(bob *state* alice)             ; user alice on bridged journal bob
(org market)                    ; multi-hop bridged journal principal
(org market *state* arbiter)    ; user on multi-hop journal
```

Rules:

- The identity path is always interpreted from the receiving journal's perspective.
- `()` is the root/current journal principal.
- Local users live at `(*state* <user>)`.
- Remote journal principals use the bridge path itself, e.g. `(foo)`.
- Remote users extend the remote journal principal with that journal's local state path, e.g. `(foo *state* alice)`.
- Multi-hop remote principals use a concise prefix of directional bridge names.
- The first implementation slice may verify only direct bridge principals, e.g. `(bob)` and `(bob *state* alice)`, and should reject unsupported multi-hop principals rather than verifying them against the wrong bridge key.

This shape makes authorization align with journal path semantics. A permission can refer to the principal's path rather than a separate namespace of peer names.

Implementation transition:

- Gateway session/API-token auth maps username `alice` to identity path `(*state* alice)`.
- Missing identity maps to `()`.
- Admin list entries are principal paths such as `(*state* tdinh)`, not bare symbols.
- Existing authorization checks that compared `identity` to an owner symbol are rewritten to compare principal paths against target state paths.
- No backwards compatibility is planned for bare-symbol identities on the 1.5 development branch.

## Authentication envelope direction

Old envelope shape:

```scheme
(authentication
  ((identity alice)
   (credentials "shared-interface-secret")))
```

Current local credential shape:

```scheme
(authentication
  ((identity (*state* alice))
   (credentials "shared-interface-secret")))
```

Target remote signature shape:

```scheme
(authentication
  ((identity (bob *state* alice))
   (signature signature)
   (message ((function set!)
             (arguments ...)))))
```

For local gateway-mediated users, credentials may remain an internal gateway-to-journal mechanism during transition:

```scheme
(authentication
  ((identity (*state* alice))
   (credentials "journal-interface-secret")))
```

For journal-to-journal requests, the receiver should verify `signature` using the public key resolved from `identity`'s journal descriptor. The signed `message` must include or canonically cover the function and arguments being executed so a signature for one call cannot be replayed as another call.

Key names should reflect authority domains:

```scheme
(*crypto* journal public-key)        ; verifies signed journal history
(*crypto* journal signature)         ; signature over the committed head
(*crypto* interface public-key)      ; verifies signed interface requests
(*crypto* public-key)                    ; temporary alias for journal public-key
(*crypto* signature)                     ; temporary alias for journal signature
```

Private keys should not be stored durably. They are derived inside trusted interface/root code from domain-separated seeds when needed. For example, journal history signing and interface request signing should derive separate keypairs from the root secret.

Open questions:

- exact signature primitive exposed to Scheme/Rust;
- canonical signed bytes: likely `expression->byte-vector` over a constrained message expression;
- whether the signature signs the full top-level query minus `authentication`, or a nested `(message ...)` copied from it;
- whether to include receiver journal identity/head to prevent cross-target replay for non-idempotent operations;
- how to represent user delegation when a remote journal acts for one of its local users.

## Public resolvable journal descriptors

Avoid relying on an ad hoc `info` endpoint as the authority for peer identity. `/info` may mirror metadata for convenience, but bridge-verifiable state should be authoritative.

A public journal descriptor should be resolvable through bridge paths and contain non-sensitive identity and endpoint metadata, for example:

```scheme
((name bob)
 (public-key ...) ; temporary alias for journal public-key
 (journal ((public-key ...)))
 (interface ((public-key ...)))
 (endpoints
   ((interface "https://bob.example.net/api/v1/journal/interface")
    (gateway "https://bob.example.net")
    (webdav "https://bob.example.net/webdav")
    (events "https://bob.example.net/api/v1/events")))
 (head ...)
 (index ...)
 (updated-at ...))
```

The descriptor is the source for:

- interface request signature verification key;
- public endpoint discovery;
- displayed journal name;
- bridge terminal-trace results;
- future reputation or market participant metadata.

Implementation questions:

- where descriptor state lives: likely public ledger config plus bridge-visible terminal trace output;
- how long to preserve the old `public-key` alias for `journal public-key`;
- how descriptor updates are signed and versioned;
- whether descriptors should be objects or plain alists at first.

## Terminal requests rather than bridged private traversal

Keep the useful part of `cross-journal-data-flow.md`: bridge traversal should discover and verify a terminal journal, not act as a private-data relay through every intermediary.

Preferred flow:

1. Resolve a bridge path to a terminal descriptor: identity, public key, endpoints, head/index.
2. Contact the terminal journal directly for private reads/writes/queries.
3. Authenticate with a bridge-derived principal path and signed request.
4. Terminal journal enforces local authorization.
5. Response includes values plus proof/material against the terminal head where appropriate.

This discourages paths like:

```scheme
(-1 carol dave *state* bob private-file)
```

for private data. The bridge path can identify Dave/Bob and prove heads; private content should be requested from the terminal journal under terminal authorization.

## Authorization direction

Initial authorization should be simple and path-shaped. Interface authentication remains in `interface.scm`; request authorization is factored through `authorization.scm` with the provisional protocol:

```scheme
(authorize! user rule)          ; local policy mutation placeholder
(authorize? principal request)  ; request-time authorization check
```

`request` is the canonical interface request without authentication:

```scheme
((function get)
 (arguments ((path (*state* tdinh docs file))
             (expression? #t))))
```

Local behavior today:

- admins bypass path restrictions;
- ordinary user `alice` writes only under `(*state* alice ...)`;
- private namespace reads under another user's `*private*` are rejected.

Target behavior:

- Principal is a path, not a symbol.
- Local user `(*state* alice)` retains current semantics for `(*state* alice ...)`.
- Remote journal principal `(market)` may be authorized by explicit local policy, not by default.
- Remote user principal `(market *state* forecaster)` can be granted rights as a specific external participant.
- Root principal `()` remains privileged locally.

For 1.5, avoid a large ACL system unless needed for the first use case. Minimal policy can start with:

- admin principal list using principal paths;
- operation-level allow/deny for bridge-derived journal principals;
- path grants for specific external principals if required by prediction-market workflows.

Future ACLs can support read, list, download, upload, write, delete, groups, and capabilities, as sketched in `cross-journal-data-flow.md`.

## Custom local objects

1.5 likely needs local typed objects for application-level workflows without baking prediction-market semantics into core ledger/tree/document layers.

Potential object families:

- public journal descriptor object;
- authorization/grant object;
- scheduled job object;
- prediction market question object;
- forecast/stake object;
- arbiter/resolution object;
- payout/settlement object.

Guidelines:

- Keep core records generic and semantic.
- Install custom classes through root/interface configuration rather than hard-coding application concerns into `tree.scm`, `ledger.scm`, or `standard.scm`.
- Durable state belongs in object state; live mutations must be persisted explicitly.
- Network fetches should be orchestrated by interface-level code and passed into object methods as prepared data.

Open design item: define a safe mechanism for installing and invoking local custom objects without expanding public API surface unnecessarily.

## Scheduled actions / step jobs

Keep the `cross-journal-data-flow.md` step-job idea.

A scheduled action is an admin-configured local object invoked at a defined point in the journal step lifecycle. It can support:

- recurring imports/materialization from known peers;
- prediction market phase transitions;
- opening/closing forecast windows;
- arbiter resolution checks;
- payout settlement;
- local cleanup and summaries;
- metadata normalization.

Initial constraints:

- admin-only configuration;
- bounded runtime and operation count;
- durable last-run status and last error;
- failure isolation so one failed job does not brick ordinary stepping by default;
- clear lifecycle placement, likely after the normal commit reaches a stable head;
- mutation semantics must be explicit: either jobs write into the next step, or their writes are staged for a later commit.

Open questions:

- cron syntax vs period/step-count predicates vs Scheme predicate object;
- whether jobs run before or after bridge synchronization;
- how jobs authenticate/authorize themselves: likely root/local principal plus configured target principal;
- whether failed jobs create durable transition records;
- how to keep resource budgets deterministic in s7.

## Prediction-market implications

Valentine's use case needs:

- participant journal identities resolvable through bridge-derived descriptors;
- question, forecast, stake, outcome, and payout objects;
- signed participant actions that can be attributed to principal paths;
- scheduled phase changes and settlement actions;
- transparent audit trail in local-first journal state;
- likely capability or grant objects for forecasters/arbiters to write into market-owned paths.

This does not require users to manage keys manually if their home journal signs requests on their behalf. Users interact with local credentials/session; journals handle cross-journal signatures and bridge verification.

## Candidate implementation slices

1. Normalize principal paths internally while preserving current local gateway behavior.
2. Add/update tests for identity path authorization and admin principal paths.
3. Define public journal descriptor state and expose it through bridge-verifiable resolution.
4. Add terminal trace/descriptor resolution helper for bridge paths.
5. Add signed journal-to-journal request verification using `(*crypto* interface public-key)` resolved through bridge paths.
6. Add minimal policy/grant checks for bridge-derived principals.
7. Add terminal request operations for remote get/query/write as needed by the first workflow.
8. Add custom local object installation/invocation support.
9. Add scheduled step-job object support.
10. Build the first prediction-market objects on top of the generic primitives.
