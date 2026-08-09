# Federation

Design and records-layer implementation note for Sync Web 1.5 reciprocal bridges, verifiable route establishment, journal-to-journal authentication, and federated function invocation.

Status: the federation protocol is implemented on `dev-1.5` across the Scheme records, Gateway, Explorer, deployment configuration, and social-agent model. Signed application federation is explicitly limited to staged `get`/`set!`, dedicated `get-batch`/`set-batch!`, and committed `resolve`. Dedicated Scheme tests cover multi-journal semantics, while the social-agent stack exercises real HTTP transport and the Explorer working/history route UI. The former publisher/subscriber and push/pull policy machinery has been removed rather than carried into federation.

The module architecture below is an approved design for the next refactor, but is not implemented yet. It replaces the earlier installer-fragment extraction approach. The protocol semantics in the remainder of this document remain the behavioral target unless the module design below explicitly assigns responsibility differently.

## Approved module architecture

The records layer is divided into three `define-class` modules with distinct responsibilities:

```text
External API:       Interface
Permissions:        Authorization
Network protocol:   Federation
Durable data:       Ledger
```

Interface remains the sole authority over the external API. A public class method is an internal capability; its existence does not automatically expose a query-interface operation. Interface decides external operation names, argument and response shapes, authentication order, authorization, codecs, local-only versus federated eligibility, error normalization, and persistence into Root.

The public class methods listed below are the approved cross-module surface. Implementation may freely introduce and change private `~...` methods, but changes to these public methods require an explicit design decision.

### Ledger: durable data and history

Ledger performs data management only. It has no endpoint selection, `sync-remote`, transport retry, bridge scheduling, or handshake orchestration.

Its logical state is:

```text
ledger
├── config
│   ├── journal identity and root signing material
│   ├── retention window
│   └── commit configuration
├── stage
│   ├── *state*
│   ├── local signed descriptor and public keys
│   └── *bridge*
│       └── peer heads and committed relationship evidence
├── temp
└── perm
```

Federation data is not a separate history store. Federation fetches evidence and asks Ledger to write it into Stage. A later Ledger step commits the complete Stage snapshot into temporary and permanent history under the ordinary retention rules. Proof-relevant peer heads, public descriptors, interface keys, alias/root bindings, and route evidence therefore remain available through normal Ledger history and proof operations.

Approved public Ledger methods:

```scheme
(size)
(get path)
(set! path value)
(set-batch! changes)

(resolve path pinned? proof? head ancestor?)
(trace path head)

(pin! path proof)
(unpin! path)
(pinned? path)

(step! prepared-inputs)
(config path)
(update-config! changes)

(descriptor index)
(signed-head known-index)
(peer-head alias index)
(store-peer-head! alias verified-head)
(delete-peer-head! alias)

(read index supplied-object)
(merge-head! alias supplied-head)
```

The first group supports ordinary data, commit, history, proof, and retention behavior. The latter group is the Federation-facing data protocol. `peer-head` also returns an inert accepted-peer checkpoint when requested internally; this is the only relationship precondition Federation consumes, so it never reads Ledger's private bridge layout. Ledger remains responsible for structural and digest validity, chain continuity, historical anchoring, alias/root tombstones, proof-preserving merge, and persistence of peer evidence. Federation remains responsible for transport plus remote identity, key-rotation, signature, descriptor, and endpoint authentication before Ledger applies the bounded evidence envelope.

### Federation: relationships and network protocol

`federation.scm` defines class `(federation)`. Federation owns network orchestration and may call `sync-remote` inside its class methods. Interface retains only the generic authenticated self-call boundary required to separate external effects from durable mutation; it contains no federation-specific protocol state machine.

Its logical operational state is:

```text
federation
├── config
│   ├── local endpoint
│   ├── synchronization schedule
│   ├── incoming acceptance and preapproval policy
│   └── transport configuration
├── invocation identity
│   └── public interface-key metadata
└── peers
    └── alias
        ├── expected journal root
        ├── endpoint
        ├── initiation: local | remote
        ├── enabled?
        └── operational synchronization state
```

This state is operational. Any peer material needed for historical authentication, routing, or proof is written through Ledger and committed in Ledger history.

Approved public Federation methods:

```scheme
(config)
(update-config! changes)
(peers)
(peer alias)

(bridge! ledger alias endpoint transient-signing-key)
(delete-bridge! ledger alias)

(synchronize! ledger request)
(bridge-synchronize! ledger alias transient-signing-key)
(step! ledger)

(route ledger request)
(invoke ledger operation arguments route history identity transient-signing-key)
(authenticate ledger invocation)
```

`bridge!` owns the complete establishment exchange. `synchronize!` is the acceptor-side wire operation; `bridge-synchronize!` is initiator-side orchestration; Federation `step!` schedules due relationships. `route` implements the direct public route/proof protocol. `invoke` constructs, signs, delivers, and verifies an originating federated application call. `authenticate` anchors and authenticates an incoming terminal invocation and returns canonical request identity/context for Interface authorization and dispatch.

Federation preserves the Journal rule that one evaluation may not both call an external function and change durable state. Mutating network operations therefore use the existing authenticated Interface self-call boundary in two phases:

1. the public Federation method reads exact local evidence, performs remote I/O, validates the response shape, and returns a signed inert continuation without changing local state;
2. generic Interface machinery re-enters the same external/internal operation with that continuation;
3. the same Federation method privately resumes the operation, authenticates the continuation, binds it to the canonical arguments and scoped expected Ledger/Federation state, revalidates the signed peer evidence, and applies the mutation without network I/O;
4. Interface persists the resulting Ledger and Federation objects atomically.

The continuation mechanism is generic Interface plumbing: it knows only how to relay a continuation, re-enter the same operation, and surface a returned failure. Bridge/synchronization fields, validation, conflict recovery, and mutation remain Federation semantics. Forged, mismatched, stale, and replayed continuations fail without mutation. Crossed bridge establishment performs its bounded rollback during the successful resume phase, then reports the established `bridge-race-error` to the outer operation.

Federation receives the live Ledger object as an explicit dependency. It does not maintain a parallel copy of committed peer history. Federation and Root persist only the public interface key; each originating request derives its journal-bound signing key transiently from the presented interface secret after authentication. Internal continuation signing uses a separate domain and a request-transient key. This release line rejects nonfresh state atomically rather than migrating legacy keys or peer state.

### Authorization: permission rules

Authorization remains independent of networking and data storage. Its logical state is a collection of rules:

```text
rule
├── principal
├── optional key-index
├── path
├── get
├── set!
└── resolve
```

Approved public Authorization methods:

```scheme
(authorized? principal key-index path operation)
(authorize! rule)
(deauthorize! rule-or-selector)
(authorizations)
(authorizations principal)
```

Authorization owns principal/rule validation, path matching, terminal-local key-index windows, and ancestor-directory admission. It does not fetch Ledger data or perform network calls.

### Interface: external API and composition

Interface installs and composes Ledger, Federation, and Authorization. Its normal flow is:

```text
decode request
→ authenticate the local session or ask Federation to authenticate an invocation
→ normalize paths and arguments
→ ask Authorization
→ call Ledger or Federation
→ persist changed objects
→ encode the response
```

Interface continues to enforce the external federation allowlist of `get`, `set!`, `get-batch`, `set-batch!`, and `resolve`. It alone decides which direct Federation protocol methods are network-callable and which class methods remain internal or local-admin-only. Codecs such as `expression?`, local administrator handling, Root wiring, and public error shapes remain Interface responsibilities.

## Motivation

A signature proves control of a private key, but it does not by itself give the key a meaningful identity. A receiving journal should not authenticate a remote request merely because the request includes a self-asserted public key.

Sync Web already has a stronger source of identity: the receiver's bridge graph. Public journal identity, endpoint, and cryptographic material can be resolved through that graph and verified against journal history.

This suggests a reciprocal relationship for authenticated requests:

- the requesting journal needs a route or endpoint for the receiving journal; and
- the receiving journal needs a bridge path back to the requesting journal's public identity and interface key.

The relationship is bidirectional for identity reachability. It does not require unrestricted or bidirectional synchronization of application data.

## Core invariant

> A remote principal is authenticatable only when its journal identity is resolvable through the receiving journal's bridge graph to a current public interface key.

For example, if Alice's journal sends a request to Bob's journal, Bob must be able to resolve Alice as something like:

```scheme
(alice)
```

and resolve Alice's request-verification key at:

```scheme
(alice *crypto* interface public-key)
```

A user attested by Alice is represented beneath that journal principal:

```scheme
(alice *state* carol)
```

Bob interprets these paths relative to Bob's own bridge graph.

## Reciprocal reachability, not reciprocal data access

“Bidirectional bridge” means that both journals synchronize enough head/control-plane state to identify and reach each other. It does not authorize application data and should not imply unrestricted ingestion or publication of the peer's `*state*`.

Synchronizing a peer head establishes a verifiable route to that journal's public identity, cryptographic material, endpoint metadata, and history context. Authorization of terminal data remains a separate, explicit local decision.

The bridge exchange keeps data flow narrow:

- each side synchronizes its journal head and public control-plane material;
- neither side thereby synchronizes or authorizes arbitrary `*state*` data;
- private application operations use the direct authenticated final mile;
- terminal authorization rules govern access beneath `*state*`.

A reciprocal relationship therefore consists of two locally controlled bridge views of one peer relationship. The only durable role distinction is which journal initiates scheduled exchanges.

## Interaction roles and one-round-trip synchronization

Each journal contributes its own head and verifies the peer's head during the same exchange.

Use **initiator** and **acceptor** for the roles in one synchronization interaction:

- the initiator opens the connection and sends its current head;
- the acceptor receives and verifies the initiator's head, then returns its own current head;
- the initiator receives and verifies the acceptor's head.

Both directions synchronize in one request/response exchange:

```text
initiator                               acceptor
    |                                      |
    |  request: identity + signed head A   |
    |------------------------------------->|
    |                                      | verify/store A
    |  response: identity + signed head B  |
    |<-------------------------------------|
    | verify/store B                       |
```

Avoid **leader/follower**, which implies authority, ordering, or replication leadership that does not exist. Avoid durable **sender/receiver** labels because both sides send and receive during the exchange. Initiator/acceptor describes per-exchange connection establishment without colliding with HTTP request/response terminology or implying data direction.

Either journal may initiate. A deployment may also be intentionally outbound-only: an unreachable or intermittently connected journal can initiate synchronization with a publicly reachable peer. The public peer does not need to open a connection back; it learns the outbound-only journal's new head from the request and returns its own head in the response.

Therefore distinguish:

- **logical reciprocity:** both journals track and verify each other's heads;
- **network reachability:** one or both journals may accept inbound connections;
- **initiation responsibility:** exactly one journal schedules exchanges for the relationship.

The journal that initiates creation of the bridge becomes the default ongoing initiator when both sides are capable and willing to initiate. The accepting journal records the inverse role and remains available for inbound exchanges.

```text
bridge initiator: initiation local
bridge acceptor:  initiation remote
```

One exchange synchronizes both heads, so the initial design has no `both` or alternating mode. Changing responsibility later requires an explicit administrative role change or renegotiation rather than emergent turn-taking.

No-change exchanges should be cheap: the initiator sends its current head digest and last-known peer head digest, allowing the acceptor to return a compact unchanged response where possible.

## Public journal descriptor

Each journal needs a stable, bridge-resolvable public descriptor. At minimum it should expose:

```scheme
((name alice)
 (identity ((id <sha256>) (nonce <32-random-bytes>)))
 (public-key <active-journal-signing-key>)
 (interface ((public-key ...) (endpoint "https://alice.example/interface"))))
```

The identity nonce is generated once with 32 random bytes. The stable journal ID
is the SHA-256 digest of the domain-separated nonce expression. A receiver checks
both lengths and recomputes the digest. This commits the public identity to a
fixed-size creation value and makes accidental identity collisions negligible;
it does not prove that a malicious creator sampled its nonce without vanity
grinding.

The existing canonical crypto path remains:

```scheme
(*crypto* interface public-key)
```

The descriptor should point to that material rather than create a second independent identity registry. Private bridge configuration may cache endpoints or operational state, but it is not the authority for the remote public key.

A peer's synchronized, verified head is sufficient for reverse identity resolution even when that peer does not accept inbound network connections. Reverse **cryptographic reachability** does not require reverse **transport reachability**.

## Invocation identity and canonical principals

Invocation identity is always an explicit scalar:

```scheme
(identity *journal*) ; the originating journal itself
(identity alice)     ; local user Alice at the origin
(identity *public*)  ; unauthenticated caller where permitted
```

Omitting identity is invalid; privileged journal identity must never arise from an absent field.

Routing provenance and scalar identity combine into the canonical journal-relative principal used by authorization. For example, at Carol:

```scheme
(identity alice)
(route-source (origin bob))
(route-target ())
```

corresponds to a canonical principal like:

```scheme
(bob origin *state* alice)
```

while `(identity *journal*)` corresponds to the originating journal principal without a `*state*` suffix. Internal authorization storage and comparisons may continue using canonical principal paths even though invocation identity is scalar.

A signature by a journal interface key authenticates the origin journal. A non-special scalar identity means that journal is also attesting which local user initiated the request. The origin journal is responsible for authenticating that user through its own session/token system before signing. Other journals do not independently authenticate the origin user's password, token, or session; they trust the origin journal's narrower attestation and then apply local authorization rules.

## Invocation route and authenticated request flow

Routing and authentication metadata live under `invocation`; function and arguments remain an ordinary scalar function call. Federated application invocations are delivered directly from the origin journal to the terminal journal. Intermediate bridges provide verified identity, route, head, and endpoint metadata but do not receive the private invocation.

A local invocation at Alice is:

```scheme
((function set!)
 (arguments
   ((path (*state* bob shared document))
    (value #u(...))))
 (invocation
   ((identity carol)
    (route-source ())
    (route-target (bob market))
    (credentials ...))))
```

Both route fields exclude the current journal. Both empty means an ordinary local call. A non-empty `route-target` tells Alice's trusted interface to follow public, exactly committed bridge objects before calling the terminal directly.

A non-empty route asks the journals to read the exact nested states already committed along that route. It never advances an embedded peer from an older head to a newer continuation. If Alice's current head contains Carol C4, C4 contains Bob B1, and B1 contains Alice A1 on the reciprocal path, those exact indexes define the interaction. Missing Merkle material may be fetched to complete stumps because that preserves each committed digest.

The public route read returns Bob's partial chain object at B1, sliced along Bob's receiver-relative bridge path back to Alice's interface key. Alice independently reads the Bob object embedded beneath her own current head and requires its B1 digest to match. That Alice-rooted object supplies Bob's canonical endpoint and root identity; Bob's returned B1 object supplies the Alice key Bob had committed at B1.

Alice authenticates Carol locally and sends Bob a direct request:

```scheme
((function set!)
 (arguments
   ((path (*state* bob shared document))
    (value #u(...))))
 (invocation
   ((identity carol)
    (route-source (alice bob))
    (route-target ())
    (object <bob-B1-object-containing-alice-key>)
    (terminal-index B1)
    (audience <bob-stable-journal-id>)
    (signature ...))))
```

At the terminal, `route-source` is origin-first and `route-target` must be empty. Bob deserializes the supplied object, checks its raw digest against Bob's own historical chain at B1 before evaluating it, and merges it with Bob's canonical local structure. Bob then reads Alice's interface key through the receiver-relative reverse bridge path, verifies the signature and audience, constructs the canonical principal, and applies local authorization.

The authorization `key-index` window applies to B1: the Bob-local history index whose committed bridge state authenticated Alice. Relative ranges are resolved against Bob's current local head. There is no separate origin watermark or invocation-learned freshness state.

Local credentials are removed before transmission. The bridge graph establishes origin identity and terminal endpoint authenticity; terminal-local authorization establishes permission.

## Bridge creation and bootstrap

Bridge creation uses the bridge protocol directly; there is no separate pairing concept or pending-request queue.

By default, a journal automatically accepts an incoming bridge-creation request when the requesting journal agrees to remain the ongoing initiator. The acceptor does not assume outbound scheduling responsibility. The request carries the initiator's public descriptor and signed head; the response carries the acceptor's public descriptor and signed head. Both sides can therefore establish reciprocal verified head state in the bridge-creation exchange.

Conceptually:

1. Alice's admin configures Bob's endpoint and creates a bridge.
2. Alice sends a bridge-creation request containing Alice's descriptor/signed head and declaring Alice as the ongoing initiator.
3. Bob verifies Alice's identity commitment and current head signature, then stores that current key/index as the bridge checkpoint. With no prior accepted key, Bob does not replay Alice's historical rotations. Bridge creation is content-authenticated by the signed head rather than by a federated invocation signature.
4. If Bob's incoming-bridge policy permits the request, Bob immediately creates the reciprocal bridge locally; he does not create pending state.
5. Bob returns his public descriptor and signed head.
6. Alice verifies Bob's response and activates her local bridge.
7. Future exchanges are initiated by Alice and synchronize both heads.

If both journals initiate the same new relationship concurrently, neither call
wins an ongoing role implicitly. Each detects that an acceptor-side half was
created while its outbound request was in flight, rolls that half back while
preserving any preapproval and permanent root/name binding, and returns a
retryable bridge-race error. One administrator then retries from the journal
that should remain the sole ongoing initiator.

Default incoming policy:

```text
auto-accept requests whose remote journal accepts initiation responsibility
```

A locked-down journal instead uses explicit journal-ID preapproval. Its admin records the acceptance mode and expected stable remote identity through ordinary ledger configuration, for example:

```scheme
(update-config! '(public bridge-accept) 'preapproved)
(update-config! '(private bridge-preapproval alice) <journal-id>)
```

An incoming request is accepted only when:

- its descriptor contains the preapproved, well-formed journal identity;
- its current head is signed by the descriptor's current key, which becomes the new bridge checkpoint;
- its bridge-creation request is otherwise valid; and
- the requester accepts ongoing initiation responsibility.

Unmatched or malformed requests are rejected without creating pending bridge state. A preapproval is local authorization to create a bridge, not an independent remote identity authority: after creation, the synchronized signed head and canonical `*crypto*` path remain authoritative. Deleting a bridge cascades through local policy: all authorization rules beginning with that bridge alias are removed, along with its preapproval and active configuration. The alias-to-journal-ID binding remains as a tombstone: the same alias/identity can be re-established, while a different identity cannot take over the alias and the same identity cannot move to another alias.

Open auto-accept mode can consume durable state. The records layer deliberately does not impose bridge-count, route-hop, or serialized-node ceilings; serialized inputs are still parsed strictly without evaluation. Sync Web 1.5 does not claim unified evaluator metering or comprehensive resource control; transport-level byte limits, request-rate limits, preapproval, and administrative cleanup remain deployment tools.

## Simplified bridge policy

The reciprocal head exchange replaces the current publish/subscribe push/pull/none negotiation matrix.

A bridge needs only:

- a synchronization role: initiator or acceptor;
- an incoming acceptance policy: auto-accept or journal-ID preapproval;
- committed identity, endpoint, active public-key, and reciprocal `remote-name` information;
- the latest verified peer head and synchronization status.

`remote-name` is stored in the committed bridge `info` alongside the endpoint and peer key. It is therefore interpreted relative to the selected immutable journal head rather than reconstructed from mutable private configuration. A concurrent rename creates a later head; it does not alter the descriptor used by an in-progress read. Private bridge configuration remains scheduling/orchestration state.

Every successful exchange synchronizes both heads. Neither role grants application-data authorization, and there is no bridge mode for one-way application-state replication. Private application access is handled by signed direct requests and terminal-local authorization rules.

Key freshness still matters: the peer's synchronized head must advance so its current interface key can be resolved and verified.

## Routed reads and direct private final mile

For a route:

```text
Alice → Carol → David → Bob
```

Alice reads only through exact committed bridge objects:

```text
Alice A2
  └── Carol C4
      └── David D3
          └── Bob B1
```

If Alice A2 contains Bob B0 rather than B1, the usable object is B0. A continuity argument that B1 extends B0 cannot substitute B1 beneath A2 because Alice did not commit that view. If the selected Bob index is stale, unreachable, or outside policy, synchronization must first produce and commit a newer Alice head.

The endpoint is an ordinary read beneath the forward path:

```scheme
(carol david bob
 *crypto* interface endpoint)
```

On the return path Bob reads Alice's key from Bob B1:

```scheme
(david-back carol-back alice-back
 *crypto* interface public-key)
```

A route read returns a serialized partial Bob chain object containing that key path. Alice also structurally slices her own history down to the embedded Bob object and checks that the two Bob objects have the same B1 digest. Alice can then read the endpoint and key as normal object values and send the Bob-rooted object with the private invocation.

Bob checks the supplied object's B1 digest against his own history. Because synchronization verified peer heads before Bob committed them, ordinary Merkle inclusion in Bob B1 is the authority for the nested Alice key; invocation authentication does not reconstruct or extend a second live chain of heads.

Function, arguments, private paths, and values never traverse intermediate journals. Missing stumps may be filled lazily at the exact committed indexes, but embedded journals are never advanced during routing. A forward route is not invocable until its exact terminal committed object contains the complete reverse path to the origin interface public key; setup waits while ordinary reciprocal synchronization and committed steps propagate that evidence. Missing reverse evidence fails closed without historical-key fallback or silent synchronization inside invocation. The terminal must be directly reachable for the private call.

The records implementation uses ordinary object operations: `trace` retains a nested chain as a structural object when its path ends at that object boundary, while remaining leaf-oriented for paths that continue into values. `get`/`resolve` read endpoint and key values, `read` accepts a supplied partial chain only when its raw digest is an exact local history head, and digest-equal partial objects can be merged. No separate public `slice` operation, watermark, or compiled segment-list protocol is maintained.

## Working-journal route and Ledger history path

The Explorer and service APIs distinguish two complementary paths with the same ordered journal names.

The **working-journal route** selects where the user's current session operates:

```text
Local › Carol › Bob
```

It is canonical and live relative to the local journal's latest committed view: every forward and reverse bridge traversal uses `-1`. It resolves the terminal endpoint, the terminal authentication head, and the terminal's committed reverse path to the origin interface key. Stage uses this route for the allowlisted live operations `get` and `set!`. Access policy, administration, bridge management, configuration, and retention mutation remain local to the origin journal and do not follow the working route.

The Ledger view adds a **historical data path** whose journal names mirror the working route while every index remains independently selectable:

```text
Local [A_i] › Carol [C_k] › Bob [B_j]
```

The public representation is one flat path. An optional leading integer selects the local head, and an optional integer following each directional bridge name selects that destination head. Omitted indexes mean `-1`; a namespace marker ends traversal:

```scheme
(12 carol 7 bob -3 *state* owner document)
```

Changing the working route rebuilds the Ledger path with the same journal names. Changing Ledger indexes never changes the working journal. The two breadcrumbs are intentionally both visible: one controls network/authentication context, while the other controls the immutable objects being resolved. Pinning is a separate origin-local retention action over the full origin-relative bridge/history path.

For `resolve`, one routed request carries two cursors through the same intermediaries:

1. an authentication cursor using canonical `-1` at every hop;
2. a historical cursor using the Ledger breadcrumb's explicit indexes.

The cursors share one network traversal but produce different terminal contexts. Bob may authenticate Alice from a recent Bob head while authorizing resolution of a much older Bob index. `key-index` applies to the authentication head; the requested path/index and `resolve` permission apply to the historical object.

The historical cursor does not provide network endpoints or identity keys. Forwarding and reverse-key lookup come from the canonical authentication cursor. If alias reuse, missing history, or another divergence makes the historical object unavailable at some hop, ordinary exact-object/history checks fail there. Implementations may report the failing alias, but stable identity is compared by journal ID rather than by the currently active signing key.

## Sender-side federated invocation

Function remains scalar for both local and federated calls. Federation is determined entirely by invocation route state:

```scheme
((function set!)
 (arguments ...)
 (invocation
   ((identity carol)
    (route-source ())
    (route-target (bob market))
    (credentials ...))))
```

The local trusted interface:

1. authenticates the explicit scalar identity;
2. requires `route-source` to be empty for a newly originated call;
3. follows `route-target` through exact committed bridge indexes and records the receiver-relative aliases;
4. reads the terminal chain object embedded in the origin's current head;
5. receives the terminal-rooted structural trace containing the origin interface key and requires both terminal objects to have the same index/digest;
6. reads the terminal endpoint, audience root, and origin key from those objects;
7. signs the immutable invocation inside the trusted runtime;
8. removes local credentials;
9. sends the invocation and terminal object directly to the terminal endpoint;
10. for `resolve` or `resolve-batch`, carries separately indexed Ledger paths through the same hops and checks returned content/proofs against authenticated historical terminal objects before returning them.

Pinning is not sent as a terminal mutation. Clients submit full origin-relative bridge/history paths to local `pin!` or `pin-batch!`; Interface performs proof-bearing signed resolution, verifies every result, then re-enters the origin-local retention mutation. Clients neither transfer proofs nor construct invocation fields.


Interface and continuation signing keys exist only as request-transient values in trusted host orchestration; their private bytes never enter persistent state. The independently rotatable Interface bearer is the narrow exception: it is private Root state so trusted orchestration can reenter the ordinary Interface, but it never enters Federation/Ledger objects, user state, history, proofs, continuations, traces, logs, or public configuration. Root remains host-only. No generic arbitrary-byte signing endpoint is exposed.

## Interface and method implications

Federation has an explicit application-function allowlist:

```text
get
set!
resolve
resolve-batch
```

A non-empty originating `route-target`, or a signed terminal invocation with a non-empty `route-source`, is valid only for these application functions. `resolve-batch` is internal federation orchestration behind complete Self-relative public batch paths; Gateway does not expose invocation fields. Records enforce the allowlist at both boundaries: the origin rejects a disallowed operation before route/proof/network work, and the terminal rejects it before application dispatch. Gateway checks provide earlier client errors but are not the authority.

The scalar functions have distinct consistency roles:

- `set!` writes the terminal journal's current Stage, enabling direct real-time delivery;
- `get` reads the terminal journal's current Stage, enabling immediate observation before a commit;
- `resolve` reads committed terminal Ledger history and verifies returned content against the selected exact object.

`get`, `set!`, `get-batch`, and `set-batch!` use only one canonical working route per invocation. They do not accept a historical cursor. A signed dedicated-batch invocation binds complete ordered paths, values, the expression codec flag, and exact optional-expectation presence/content. Terminal Interface authenticates the audience and signature, authorizes every path (both `get` and `set!` when expectations are present), then reads one staged snapshot or applies one atomic prewrite-snapshot mutation. Committed `resolve` and `resolve-batch` use independently indexed Ledger history paths; Interface groups resolve-batch paths and verifies one terminal multiproof per compatible route/history group.

### Direct bridge and proof protocol

The following operations remain network-callable protocol machinery but are not signed federated application functions:

- `info` is unauthenticated convenience metadata, not identity authority;
- `size` is an unauthenticated local coarse hint;
- `synchronize!` is the reciprocal acceptor-side head-exchange operation;
- `bridge-synchronize!` is internal initiator-side step/orchestration machinery;
- `route` follows exact committed bridge indexes and returns terminal endpoint/key context;
- constrained `trace` returns only public crypto/bridge proof structure needed by routing, synchronization, and resolve.

Bridge synchronization is content-authenticated by root-signed heads and continuity, not by an application invocation. Direct protocol handlers reject application invocation envelopes rather than silently inheriting application authentication semantics. Public `trace` remains restricted to crypto/control paths; “protocol-public” does not mean arbitrary public state access.

The ledger exposes the ordinary object operations needed by this protocol: `trace` retains a nested object boundary, `read` anchors an exact historical object before its code can run, `signed-head` produces reciprocal synchronization payloads, and digest-equal partial objects can be merged. Internal `slice!`/`deep-slice!` remain implementation tools. Stored custom objects remain opaque to Ledger value handling.

### Local-only operations

Every other normal query-interface operation is local-only, including:

```text
pin!                   unpin!
set-batch!
config                 update-config!
bridge!                delete-bridge!
authorizations         authorize!
deauthorize!           *admins-get*
*admins-set*           *window-set*
*secret*
```

Bridge creation is initiated by a local administrator and uses the direct bridge protocol to contact the peer; it cannot itself be routed as a federated application action. Root-plane functions such as `*eval*`, `*call*`, `*step*`, `*set-query*`, and `*set-step*` remain local-only as before.

Pinning means retention at the origin journal where the user authenticated and began the route. For remote content, a signed `resolve` returns verified proof material; the local `pin!` keeps that material at the full origin-relative bridge/history path and mutates only the origin's permanent proof state. `unpin!` likewise changes only origin retention. Neither operation is delivered to the terminal.

Interface administrators are local principals only. A bridge-shaped remote principal cannot gain broad administrator bypass; remote callers always require explicit path-scoped `get`, `set!`, and/or `resolve` rules. `call!` is root/configured-local-admin only and is not an Authorization permission or federated application operation. Rules otherwise contain principal, path, and terminal authentication `key-index`; `key-index` is required for remote principals and omitted for local/public principals. Public routing/trace policy remains structural and separate from application-owner authorization.

### Gateway and Explorer implications

The Gateway accepts non-empty `$federation.route` only for staged `get`/`set` and dedicated `get-batch`/`set-batch`. Public `resolve`, `resolve-batch`, `pin`, `pin-batch`, `unpin`, and `unpin-batch` instead use committed full paths or origin-local retention orchestration; `trace-batch` remains proof protocol machinery. Interface alone normalizes committed paths into the existing internal route, history, and terminal path; clients do not submit `$federation.history` or proof responses. Federation context on other aliases, direct protocol operations, and root operations is rejected with a clear client error rather than ignored.

Explorer's working route applies to Stage `get`/`set!`; its Ledger breadcrumb is already the canonical committed path used by `resolve`, `pin`, and `unpin`. Access and Admin are unavailable while a remote working route is selected, because silently administering Self beneath a remote breadcrumb would be ambiguous. Returning to Self exposes those panels. Explorer does not need a direct-path escape hatch: when the selected owner root is an ancestor of an applicable grant, the interface returns each ancestor directory's ordinary immediate listing. Folder names remain navigable, while opening a listed child still requires its own direct or descendant authorization.

A remote Ledger's latest index is learned from the existing public `route` response's `terminal-index`; Explorer does not federate `size`. Bridge selection may continue using allowlisted `get` of the public bridge directory.

The social-agent model exercises ordinary scoped authorization rather than remote-admin bypass: positive and negative remote `get`, `set!`, and `resolve`, ancestor traversal with child-level denial, and local retention of resolved remote proofs. It creates configurable deterministic users on every journal; each user's fixed keys are split between a public remotely read-only subtree and a private subtree available only to the same username over every exact walk within the configured segment bound, including finite journal revisits. Independent per-user activity updates assigned keys in place. Retention activity sends the canonical full path directly to local `pin!`, which acquires and verifies its remote proof internally, then unpins the same path; it does not federate pin/unpin or install bridge principals as interface administrators. Optional batch activity keeps the existing random anchor route and latest `-1` history indexes, selects only unique paths in that exact user's route/access group, and replaces the scalar activity pair with `get-batch`/`set-batch` or local `pin-batch`/`unpin-batch`; it adds no standalone resolve workload. Metrics keep HTTP requests separate from logical per-path operations so batch throughput is reported honestly.

The route handshake remains general: live scalar and dedicated-batch Stage access need only one canonical working cursor, while `resolve` adds the mirrored historical cursor. Adding another federated function later is a protocol-surface decision requiring explicit semantics, authorization mapping, Gateway/Explorer behavior, negative tests, and documentation; implementing a normal local query method does not automatically federate it.

## HTTPS and replay posture

The signed invocation travels only over the direct HTTPS connection to the canonical terminal endpoint proven by the route handshake. Intermediate journals never receive the plaintext invocation. TLS record protection prevents a passive or active network observer from capturing and replaying application requests, just as it does for an ordinary local-user REST call made from a remote client.


Federation therefore adds no mandatory revisions, nonces, timestamps, request-ID table, replay window, or exactly-once mechanism. An endpoint or other party that obtains a valid invocation can repeat it, so `set!` keeps the same repetition/concurrency semantics it already has through the normal query interface. Applications that use `set!` for message delivery may define identifiers, idempotency, or state preconditions when their domain requires them; those are not implicit federation semantics.

Application retries after ambiguous transport failures can still repeat a request, exactly as with existing HTTPS APIs. Functions with irreversible external effects may define their own idempotency keys or state preconditions when their domain requires it, but that is an operation/API concern rather than a federation requirement.

Current terminal authorization and the remote signing-key index window are checked every time an invocation executes. The signature audience prevents a captured invocation from being redirected to another terminal.

## Journal identity, root-secret rotation, and interface keys

A journal has a stable identity distinct from its active signing key:

```scheme
(identity ((id <sha256>) (nonce <32-random-bytes>)))
```

The signing key remains coupled to the root secret, but is salted by the stable
journal ID and domain-separated before deterministic key generation:

```text
K = derive("sync-web/journal-signing-key/v1", journal-id, SHA256(root-secret))
```

Consequently, two journals using the same root secret still derive different
keypairs. The public key permits per-journal offline guessing of weak secrets,
so operators should still use high-entropy root secrets; identity salting is not
a substitute for a password-hardening KDF.

Changing the root secret creates one transition certificate in the rotation
head. It names its activation index, previous rotation index, old key, and new
key, and is signed by the previously active private key over that
journal-ID-bound, domain-separated expression. The same atomic root operation
commits the head using the new key.

A bridge caches the last accepted key and history index. Later synchronization
verifies only certificates after that checkpoint: an ordinary exchange with no
rotation verifies the latest head directly with the cached key, while a peer
that missed `K2 -> K3 -> K4` verifies just those missing transitions starting at
its cached `K2`. The sender selectively materializes rotation metadata after the
receiver-known index rather than transmitting cumulative key history. History
continuity anchors those certificates to the retained peer head.

A new peer has no prior trusted key, so historical lineage would add no trust.
It validates the journal identity commitment, verifies the current head with the
descriptor's current key, and stores that key/index as its initial checkpoint.

The journal ID, rather than any active key, is the bridge identity and invocation
audience. Bridge aliases and deletion tombstones bind to that ID. Historical
heads retain their original signatures, while new heads use the latest key.
Root-secret rotation requires the runtime/operator configuration to begin using
the new secret for subsequent root calls and steps. Because the rotation head is
committed atomically, there is no externally visible pending-key interval.

The identity nonce is public durable recovery material. Reconstructing the same
signing key requires both the root secret and journal identity; the nonce can be
backed up or recovered from signed heads held by peers.

The interface key used for federated invocation signatures is separate and may change. Its public key is published inside journal state:

```scheme
(*crypto* interface public-key)
```

A new interface key is trusted only after it appears in a head verified from the bridge's accepted journal-key checkpoint and synchronized through the relationship. The ordering is:

1. the origin updates and publishes its interface public key;
2. its active journal key signs the head containing that key;
3. peers synchronize and verify the head, any rotations since their checkpoint, and continuity against the pinned journal ID;
4. federated invocations signed by the new interface key become verifiable.

The old interface key does not need to sign the new one because the verified journal head authorizes the replacement. Journal state retains only public keys and non-secret convergence evidence. The current request derives exactly one interface signing key from `("sync-web/interface-signing-key/v1", journal-id, SHA256(presented-interface-secret))`; no old private key remains available after credential rotation. Stale direct or multi-hop routes may therefore fail closed until ordinary bridge synchronization commits the replacement public key and route recompilation selects it. Private bridge configuration cannot override the key selected by the compiled verified proof.

Internal two-phase continuations use the separate `sync-web/federation-continuation-signing-key/v1` domain. Their bodies contain signatures and inert operation data, never private bytes or source credentials.

Every bridge synchronization path must therefore verify identity, the necessary checkpoint-relative key transitions, head signature, and continuity.

## Direct and multi-hop identities

Direct and multi-hop routes use the same proof-handshake and terminal invocation model. A direct route has one reciprocal bridge edge; a multi-hop route composes adjacent reciprocal head proofs.

Multi-hop routing does not make intermediaries authorization principals or private-data relays. They contribute public proof links that let the endpoints verify each other. Remaining concerns include:

- translating origin-side bridge aliases into terminal-relative source aliases;
- choosing among multiple valid routes to the same root identity;
- preserving finite routes that revisit a journal, including routes back to the origin, without treating them as malformed plumbing;
- applying terminal policy when a self-targeted or repeated-journal route should not be authorized;
- handling authorization rules when the same root identity is reachable through a changed route.

Authorization initially remains path-shaped, so route changes may require rule updates unless a later stable root-identity subject is introduced.

## Authorization

Authentication answers “which bridge-resolvable journal/user made this request?” Authorization remains entirely local to the receiving interface.

Example terminal rule:

```scheme
((principal (alice *state* carol))
 (key-index (-100 -1))
 (path (shared project))
 (get #t)
 (set! #t)
 (resolve #t))
```

Remote rules require a signing-key index window. Authentication supplies the terminal-local history index whose committed object contained the origin key. Relative ranges such as `(-100 -1)` are normalized against the terminal's current local index, and authorization succeeds only when that historical authentication index falls in the range. No separate origin freshness or watermark state is maintained.

Different rules may select different tolerances. `(-1 -1)` requires authentication from the terminal journal's latest local head and can reject ordinary requests when synchronization advances after a route checkpoint. Wider windows tolerate that normal lag and bounded pending-key overlap during rotation; the social-agent integration harness uses `(0 -1)` to cover its full test history. Windows count terminal journal indices rather than wall-clock time.

Reciprocal bridging does not grant access by itself. Without an explicit rule, the remote principal remains denied. A `(*public*)` grant applies to all callers for its enabled operations. A request above an applicable grant prefix may traverse implicitly and receives the ancestor directory's ordinary immediate listing. This behavior is admitted at the authorization/interface boundary rather than changing tree semantics; every listed child remains independently access-controlled, and directory proofs retain the original Merkle digest without disclosing child content.

## Failure behavior

Suggested deterministic failures:

- no reciprocal bridge path: `authentication-error`;
- public interface key unavailable or stale/unknown: `authentication-error`;
- claimed principal does not agree with reciprocal naming: `identity-error`;
- signature/invocation mismatch: `authentication-error`;
- authenticated principal lacks a local rule: `authorization-error`;
- terminal endpoint unavailable: transport error surfaced by the origin journal;
- failed mutation revision/state precondition: operation-specific conflict error.

Avoid falling back to an inline supplied public key when bridge resolution fails.

## Security properties

This design aims to provide:

- no arbitrary self-asserted peer keys;
- explicit local incoming-bridge policy, with optional journal-ID preapproval for locked-down journals;
- public keys anchored in bridge-verifiable journal state;
- interface and continuation private keys excluded from all Journal-persisted state and derived only as request-transient, journal-bound capabilities;
- strict non-evaluating parsing of untrusted serialized proofs and heads;
- terminal enforcement of private authorization;
- independent local control over synchronization direction and access policy;
- no implication that a reciprocal bridge grants data access.

It does not by itself provide:

- safe unrestricted egress for open auto-accept deployments; a self-signed peer can publish an arbitrary endpoint, so production services must use preapproval or enforce endpoint/egress policy before enabling open auto-accept;
- confidentiality without TLS;
- anonymity or onion routing;
- offline relay of private requests;
- independent proof of a remote local user's real-world identity;
- generic exactly-once execution or replay prevention; mutations rely on operation-specific replay safety or signed state/version preconditions.

## Relationship to earlier idea notes

This direction supersedes the independent TOFU peer registry and inline registration proposed in `journal-peer-identities.md`. That note remains useful for canonical signed-message, key-rotation, error, and abuse-control considerations, but remote identity authority should come from bridge-resolved public state rather than a separate registry of arbitrary keys.

It aligns with `cross-journal-data-flow.md`: bridges provide public identity, head, proof, route, and endpoint discovery, while private application operations use a direct authenticated final mile to the terminal journal.

## Decisions settled

1. **Meaning of bidirectional:** reciprocal bridges synchronize journal heads/control-plane state in both directions. This establishes identity reachability only. It grants no authorization to application data; terminal data access always requires an explicit local authorization rule.
2. **Public descriptor ownership:** ledger-published journal ID, rotation certificates, `*crypto*`, and public endpoint/name metadata are authoritative. Private bridge configuration caches the accepted identity/key checkpoint but cannot override signed history.
3. **Interaction terminology:** the journal opening a synchronization exchange is the initiator; the journal accepting it is the acceptor. These are per-exchange roles between symmetric peers.
4. **Initiation policy:** exactly one journal initiates scheduled synchronization. The journal that initiates bridge creation is the default ongoing initiator when either could do so; the acceptor records the inverse role. There is no `both` or alternating mode initially.
5. **Remote user attestation:** the requesting journal authenticates its local user and signs the remote request. The accepting journal trusts that journal's attestation of the user, then applies its own authorization rules; it does not independently verify the remote user's credential.
6. **Bridge creation acceptance:** there is no pending queue. The default is immediate auto-accept when the remote journal remains the initiator. Locked-down journals instead require an admin-preapproved stable journal ID; unmatched requests are rejected without durable pending state. Acceptance mode and preapproval values are managed through `update-config!`, not dedicated interface methods.
7. **Bridge policy shape:** reciprocal head exchange replaces the publish/subscribe push/pull/none matrix. Bridges retain only initiator/acceptor role, incoming acceptance policy, endpoint/name configuration, and verified synchronization state. Application-data access is separate.
8. **Invocation envelope:** function is always scalar. Routing, identity, credentials, request metadata, and signature live under `invocation`. Identity is always explicit and scalar; `*journal*` denotes the originating journal and omission is invalid.
9. **Federated route shape:** invocation uses flat `route-source` and `route-target` fields. Both exclude the current journal; both empty means a local call. At origin, `route-source` is empty and `route-target` identifies the terminal through public bridge state. The terminal receives a direct invocation with `route-target` empty and a terminal-relative `route-source` identifying the origin.
10. **HTTPS/replay parity:** private invocations travel directly over HTTPS and retain the same retry, repetition, and concurrency semantics as ordinary query-interface calls. Federation adds no mandatory preconditions, nonce, timestamp, deduplication table, or exactly-once layer. Operations may define idempotency/precondition semantics independently when their domain requires them.
11. **Identity and rotating keys:** a SHA-256 commitment to a random 32-byte nonce is the stable journal identity. The active journal signing key remains derived from the root secret plus that ID. Each rotation head carries one old-key-signed transition; existing peers verify only transitions after their cached key/index checkpoint, while new peers accept the current signed head without replaying old lineage. Interface request keys remain separate and are authenticated by verified heads.
12. **Direct private final mile:** intermediate bridges carry only public control/proof/discovery state. The origin sends function, arguments, private paths, and values directly to the verified terminal endpoint over HTTPS. Offline relay and application-level end-to-end encryption are out of scope initially.
13. **Exact routed objects:** federation follows only journal states already committed beneath each parent. The origin reads the terminal object embedded in its own head; the terminal returns a structural `trace` of that same historical object containing the reverse key path. Their terminal indexes/digests must match. No live continuation may replace an embedded checkpoint.
14. **Directional local route names:** there are no global journal names. Forward and reverse routes are sequences of edge-local names. Public paths use those names directly as a concise traversal prefix, with an optional index after each name; repeated `*bridge*` markers remain only an internal storage detail. Each reciprocal bridge stores the local/remote mapping needed to read both directions, and the terminal key must exist at that exact receiver-relative path.
15. **Response assurance:** direct terminal responses are protected by HTTPS at the canonical endpoint proven during routing; federation does not add a second generic response-signature envelope. `resolve` verifies returned content/proof against the separately selected Ledger history path. `get` results and `set!` acknowledgements remain operational rather than durable cryptographic claims.
16. **Handshake and data indexes:** the canonical authentication cursor and historical Ledger cursor may reach different indexes at the same terminal journal. Current terminal policy authorizes historical `resolve` using the caller key committed at the authentication index. `get` and `set!` operate on current Stage under current authorization.
17. **Authorization key windows:** every remote authorization rule constrains the terminal-local history indexes whose committed bridge state may authenticate the caller. The window is relative to the terminal's current index; no separate origin watermark exists.
18. **Federated application surface:** exactly `get`, `set!`, `get-batch`, `set-batch!`, and `resolve` may use a signed federated application invocation. Records enforce this allowlist at origin and terminal. Direct `route`/`trace`/`synchronize!` protocol calls and internally routed `resolve-batch` groups are separate content-authenticated/proof-control categories; every other query and root operation is local-only.
19. **Committed reciprocal names:** bridge `info` commits `remote-name` with identity, endpoint, and active-key metadata. Routes interpret this descriptor from the selected immutable head; private bridge configuration remains operational scheduling state.
20. **Complementary route cursors:** the working-journal route uses canonical `-1` traversal for authentication and live scalar/dedicated-batch Stage access. Ledger exposes a mirrored, independently indexed history path used by committed resolution; journal identity comes from the stable committed journal ID rather than active-key equality when the paths diverge.
21. **Local retention and administration:** pin/unpin mutate only the origin journal's retention state over a full origin-relative path. Access policy, interface administration, configuration, and bridge management never follow a working route. Interface administrators are local principals; remote callers use path-scoped application rules.
