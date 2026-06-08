# Journal peer identities

Idea note for future journal-to-journal request authentication. Not yet implemented.

## Motivation

Some journal endpoints should remain public permanently:

- `info` — public journal metadata and advertised policy.
- `size` — coarse public chain size.

Other journal-to-journal operations need authentication for abuse control, privacy, and future non-public sync policy. `trace` is the important case: it can be called repeatedly while constructing proofs, including by journals that are not otherwise bridged.

Signed journal blocks remain the authoritative data-authentication layer. Peer request authentication is a separate transport/request layer for deciding who may ask for costly or sensitive operations.

## Current preferred direction

Start with signed authenticated requests, not bearer secrets.

Reasons:

- A whitelist or identity policy needs a lookup either way: name/key/policy for signatures, or name/key/secret/policy for bearer secrets.
- Current-stack signature verification is cheap enough that the extra secret lifecycle is not justified up front.
- Signed requests are easier to reason about: no bearer secret to leak, rotate, hash, scope, or accidentally log.
- The identity registry shape can leave room for hot-path bearer credentials later if real `trace` workloads show signing/verification overhead matters.

Bearer secrets should be treated as a future optimization, not the foundational protocol.

## Peer identity registry

Add a journal-local identity registry, independent of bridge configuration. A remote journal can reserve a local peer name and associate it with a public key and operation policy.

This identity does not imply a bridge exists. Non-bridged journals may still identify themselves to request operations such as `trace` under whatever policy the target journal allows.

Operation auth policy should support at least:

- `public` — no authentication required.
- `registered` — registered peer identity authentication required.
- `deny` — operation disabled for peer identities.

A journal may choose fully public/no-auth `trace` if it accepts the resource and privacy tradeoff. Public operations still need normal IP/rate-limit abuse controls.

Store identities in root state, not ledger state, so request authentication is available to raw journal interface handling before ledger/bridge policy enters the picture.

Proposed root path:

```scheme
(interface identities <peer-name>)
```

Entry shape:

```scheme
((public-key ...)
 (policy ((trace allow)
          (synchronize allow)
          (synchronize! allow)))
 (created ...)
 (updated ...))
```

Default registered policy is allow-all for peer-authenticated operations. Separately, each operation can be globally configured as `public`, `registered`, or `deny`. Deployments can set an identity's policy to deny selected operations; if policy denies an operation, the request returns an authorization error.

Names are local to the receiving journal. They may match bridge names when convenient, but they are conceptually peer identity names. The registry is independent of ledger bridge/subscriber config; admin deletion of an identity does not modify bridge state.

The durable identity authority is the public key. The local name is an authorization/policy handle.

## Lifecycle operation

Use one lifecycle function, `identify!`, with an operation argument for registration, key rotation, and deletion. Identity registration is always reachable; authorization policy controls later use of the registered identity.

## Registration

Initial registration is public / TOFU-style:

```scheme
(identify!
  (operation register)
  (name name)
  (public-key public-key)
  (signature signature))
```

The signature is made by `public-key` over the canonical expression bytes of a registration message containing at least:

```scheme
(register name public-key)
```

Use `expression->byte-vector` for the signed message bytes unless a later implementation discovers a concrete ambiguity that requires a stricter codec.

The receiver verifies the signature, rejects malformed requests, and accepts only when `name` is unused. If `name` already exists with a different public key, reject. Same-key idempotence can be decided later.

Cross-journal replay of the initial registration payload is not considered a serious semantic attack: replaying the tuple to unrelated journals merely registers the same key there, comparable to ordinary public identity spam. Rate limiting and garbage collection handle spam/name squatting separately.

## Signed request authentication

After registration, authenticated journal-to-journal endpoint calls are signed by the registered private key.

Conceptual peer request shape:

```scheme
(authentication
  ((journal peer-name)
   (signature signature)
   (message ((operation trace)
             (arguments ...)))))
```

`identity` keeps its existing meaning as the caller identity within the target journal's normal user/interface authorization model. If `identity` is omitted, it means the root journal caller. This avoids exposing or reserving `*journal*` as a normal public username, while preserving the current internal journal-admin concept.

`journal` is the peer journal identity name. If `journal` is omitted, it means `*self*`, matching the local/default case. For peer calls, set `journal` to the registered peer name to look up under `(interface identities <peer-name>)`.

Local Kratos/user-authenticated calls continue to use `identity` plus credentials and can omit `journal`:

```scheme
(authentication
  ((identity alice)
   (credentials "...")))
```

Receiver behavior:

1. Read `identity` from authentication, defaulting to the root journal caller when omitted.
2. Read `journal` from authentication, defaulting to `*self*` when omitted.
3. For peer-authenticated operations, look up `journal` under `(interface identities <peer-name>)`.
4. Canonicalize `(message ...)` with `expression->byte-vector`.
5. Verify `signature` with the stored public key.
6. Authorize operation using the identity policy.

No timestamp, nonce, request id, or replay window is required initially. Replays are explicitly allowed if the key matches, the signature validates, and the operation is authorized. Signed messages must still bind the operation and arguments tightly enough that a signature for one operation cannot be reused as a different operation.

## Inline identification bootstrap

Avoid requiring an extra round trip when an unregistered journal wants to make one authenticated request, such as `trace`.

Allow a request authentication envelope to carry enough registration material to identify and authenticate in the same call:

```scheme
(authentication
  ((journal peer-name)
   (public-key public-key)
   (identify-signature identify-signature)
   (signature request-signature)
   (message ((operation trace)
             (arguments ...)))))
```

Semantics:

1. If `(interface identities peer-name)` exists, use the stored key. If `public-key` is present, it must match the stored key.
2. If the identity does not exist, require `public-key` and `identify-signature`.
3. Verify `identify-signature` with `public-key` over the canonical registration message, e.g. `(register peer-name public-key)`.
4. Create the identity with default registered policy.
5. Verify `request-signature` with the identity key over `(message ...)`.
6. Authorize and execute the requested operation.

The response is the requested operation response, not a separate registration response. Registration is an internal side effect of authenticating the request. A standalone `identify!` lifecycle operation should still exist for explicit registration, key rotation, and deletion.

If the operation is globally `public`, the caller may omit authentication entirely. If authentication is supplied anyway, the receiver may still process it, but public access must not require registration.

## Authorization policy

Start with identity-level allow/deny per operation only. Do not implement path-prefix or per-resource policies yet. Registered identities default to allow-all.

Example policy shape:

```scheme
((trace allow)
 (synchronize deny)
 (synchronize! deny))
```

Exact symbols can be chosen during implementation. The semantic goal is simple operation-level authorization.

## Key rotation

Key rotation should require proof from both old and new keys and explicitly name the old public key:

```scheme
(identify!
  (operation rotate-key)
  (name name)
  (old-public-key old-public-key)
  (new-public-key new-public-key)
  (old-signature old-signature)
  (new-signature new-signature))
```

Rules:

1. Look up `name` and current public key.
2. Require `old-public-key` to equal the stored public key.
3. Verify `old-signature` with `old-public-key` authorizing `new-public-key`.
4. Verify `new-signature` with `new-public-key` to prove possession.
5. Replace `public-key` and update `updated`.

No timestamp/nonce/epoch replay guard is required initially. Since the message includes the old public key and the receiver checks it against current state, old rotations cannot be replayed after the key changes.

## Deletion

Two deletion paths should exist:

- Peer-owned deletion: signed special delete message from the current public key.
- Admin deletion: admin-only operation by identity name for cleanup, abuse response, or stale entries.

Peer-owned deletion can be key-only. Replay of a delete message is harmless once the entry is gone. Admin deletion only removes the identity entry; it does not modify ledger bridge/subscriber configuration.

## Error tags

Suggested error tags:

- `identity-error` — malformed registration/lifecycle request, name conflict, unknown identity for lifecycle operations.
- `authentication-error` — missing auth, bad signature, key mismatch, malformed signed message.
- `authorization-error` — authenticated identity is not allowed to perform the requested operation.

Keep errors concise and deterministic. Avoid leaking more policy detail than necessary for unauthenticated failures.

## Future bearer-secret optimization

If profiling real workloads shows that per-request signing or verification is a meaningful cost, add an optional hot-path credential to registered identities:

```scheme
(secret-digest ...)
(secret-updated ...)
```

In that model:

- Lifecycle/control operations remain signed.
- Hot data/query operations such as repeated `trace` calls may use `(identity, secret)` authentication.
- Secret rotation must include the current bearer secret to prevent replay/rollback of old rotation messages.
- Secrets are stored only as digests/hashes and transported only over TLS.

This should be added only after measuring enough benefit to justify the extra lifecycle and compromise semantics.

## Abuse control and cleanup

Public registration requires normal operational controls outside the cryptographic protocol:

- IP / network rate limiting.
- Per-name and per-source failure throttling.
- Admin deletion of unwanted identities.
- Potential garbage collection of unused, expired, or never-authenticated entries.

These are policy and operations concerns, separate from peer identity proof.

## Endpoint implications

- `info` and `size`: stay public. `info` should advertise whether peer identity registration is enabled and per-operation auth mode (`public`, `registered`, or `deny`). Registration is expected to be enabled.
- `trace`: support `public` or `registered` mode. In `registered` mode, support inline identification bootstrap so first-time callers can identify and trace in one round trip.
- `synchronize` / `synchronize!`: signed block verification remains the source of data truth; peer identity auth may later gate non-public sync policies or resource budgets. These should also use the same operation auth mode vocabulary where applicable.
- Bridge config remains separate. A bridge may refer to a peer identity name, but identity registration alone does not create or authorize a bridge.
- Gateway should support this as a raw journal interface authentication envelope. It is also fine to expose convenient gateway `/api/v1/general/*` routes where useful.
