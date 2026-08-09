# Root-derived user credentials

Status: aspirational design idea. This document records a possible future trust and credential model; it is not an implementation contract, release commitment, wire specification, or migration plan.

## Motivation

The current model uses separate Root and Interface secrets, while the Interface secret authenticates many nominal users and supplies one shared signing identity for federated invocations. This obscures the actual trust hierarchy and requires users to rely on a shared journal-level credential.

A clearer model has exactly one ultimate source of cryptographic trust: the Root secret. Everything else is a scoped, revocable credential derived from it. The Root secret remains local to the journal deployment; a delegated user-issuance key may be provisioned to Kratos or another identity lifecycle service without exposing Root.

The intended benefits are:

- describe the journal's actual root of trust honestly;
- let Kratos create and recover user credentials without holding Root;
- let each user authenticate with only that user's own secret;
- carry an origin journal's user-specific authentication through federated requests;
- let the local scheduled Root operation deterministically recover a user's credential when it must call staged programs as that user;
- remove the general-purpose shared Interface secret.

## Trust hierarchy and terminology

The proposed hierarchy is:

```text
Root secret
  └── user issuer key
        └── individual user secrets
              └── request/federation signing keys
```

Terms:

- **Root secret** — the sole ultimate secret for one journal. Durable state contains only a verifier or secure one-way derivative. The raw value is available only at the local deployment boundary, including the expression configured as the periodic `--step` operation.
- **User issuer key** — a delegated derivation authority held by Kratos or a small adjacent credential broker. Configuration may call its raw value `user-issuer-secret`, but it is not a general Interface administrator credential.
- **User secret** — the credential held by one user, such as Alice, and accepted by that user's origin journal.
- **User signing key** — deterministic asymmetric signing material derived from a user secret for federated invocation authentication.

The name “Interface secret” should disappear. An Interface is a request boundary, not a cryptographic principal.

## Conceptual derivation

Exact KDFs and encodings require cryptographic review. Conceptually:

```text
K_root = SecureDerivation(encoded-root-secret)

K_issuer = KDF(
  K_root,
  "sync-web/user-issuer/v1",
  journal-id,
  issuer-generation)

K_user = KDF(
  K_issuer,
  "sync-web/user-auth/v1",
  journal-id,
  user-name,
  creation-index,
  user-epoch)
```

The construction must use a standard keyed derivation function such as HKDF with unambiguous, domain-separated encodings. If an operator supplies a human-memorable Root input rather than uniformly random key material, Root normalization should use an appropriate salted memory-hard password derivation function rather than a fast hash.

A verifier can be derived separately:

```text
V_user = H("sync-web/user-verifier/v1" || K_user)
```

Federated signing material should use another domain-separated derivation from `K_user`. A public signing key, rather than the bearer verifier, is the value that other journals need to prove and inspect.

This hierarchy intentionally means:

- Root can reproduce the issuer key and every user secret;
- the issuer can reproduce every user secret but cannot reproduce Root;
- a user secret cannot reproduce the issuer or another user's secret.

Possession of the issuer key is therefore broad authority over users at that journal. It should be treated and deployed accordingly.

## Root locality

Durable Root state should contain only a verifier or secure one-way derivative of the Root secret. The raw Root value must not become ordinary journal data, a federated credential, or an Interface request credential.

The locally configured periodic step already has a special relationship with Root: its on-box expression may contain or obtain the Root secret and pass it into the installed Root step handler. That local step may deterministically derive a user's credential and call a staged program through the ordinary user-authenticated `call!` path when needed.

“On box” is an operational boundary, not a claim that command-line arguments, process memory, container configuration, or local secret files are equivalent to hardware-backed storage. Deployments may strengthen storage independently.

## Role of the user issuer

The user issuer key exists only for user credential lifecycle operations. It should not authenticate ordinary `get`, `set!`, `resolve`, `call!`, bridge administration, or federation operations.

Its intended authority includes:

- create a user credential record;
- derive or recover that user's secret;
- rotate one user's credential epoch;
- disable or delete a user;
- reprovision users after an issuer-generation rotation.

Ordinary administrative operations should be performed by explicitly authorized admin users. Kratos may hold the issuer key in a protected integration or credential-broker component and use registration/lifecycle hooks to create corresponding journal users. It need not persist every individual user secret because it can regenerate one from the issuer key and the user's committed metadata.

## User identity and creation index

A user does not require a separate random UUID. Its immutable derivation identity can be:

```text
(journal-id, user-name, creation-index)
```

The creation index is the committed journal index at which that named user first becomes durable.

This is sufficient for the intended purpose: protecting against deletion followed by later recreation of the same name. A committed Alice that is deleted and later recreated necessarily receives a later creation index. Multiple differently named users may share one index because the name distinguishes them. Create/delete/recreate operations that occur entirely before any committed appearance do not represent multiple durable identities; only the final committed Alice matters.

The creation index must be retained in Alice's current credential record and included in every user-secret derivation. Reusing the visible name later therefore does not resurrect the old credential.

## Durable credential material

A public, signed credential projection might be shaped conceptually as:

```scheme
(*crypto* users alice creation-index)
(*crypto* users alice epoch)
(*crypto* users alice public-key)
```

The exact shape is intentionally unresolved. Authentication material belongs under the reserved `*crypto*` namespace, not ordinary user-writable `*state*` data.

Only material needed for federated verification must be projected into public signed `*crypto*` proofs. The local bearer verifier may remain in private/root credential state if no remote verifier needs it. Ordinary `set!` must never be able to alter reserved credential records.

## Local authentication

A local request supplies:

```text
identity = alice
credentials = K_alice
```

The origin journal:

1. resolves the latest committed Alice credential record;
2. verifies the supplied secret against Alice's current verifier;
3. maps the request to the local principal `(*state* alice)`;
4. performs ordinary authorization for that principal.

Authentication must always use current committed credential state. Resolving historical application data must not resurrect a retired user secret.

Alice stores only Alice's secret. She never receives the Root or issuer key and never needs another user's credential.

## Federated authentication

The purpose of user-specific federation authentication is modest and origin-scoped. It does not establish that a globally canonical human named Alice exists or that the terminal journal approves the origin journal's user-provisioning practices.

It establishes:

> The trusted origin journal currently recognizes this key as controlling its local principal Alice.

The origin journal remains authoritative for its own principal namespace. If its setup or provisioning is wrong, that is an origin-journal failure, just as incorrect origin authorization state is today.

A conceptual federated flow is:

1. Alice authenticates to her origin journal with only `K_alice`.
2. The origin derives Alice's federation signing key and signs the canonical invocation.
3. The invocation carries an authenticated origin Journal object/proof sufficient to resolve Alice's committed public key.
4. Across bridge hops, the terminal key path ends at a user-specific path such as `(*crypto* users alice public-key)`, rather than `(*crypto* interface public-key)`.
5. The terminal validates the origin Journal head and extracts that public key.
6. The terminal verifies the invocation signature and maps the origin-scoped identity to the routed Alice principal.
7. Intermediaries and the terminal never receive Alice's bearer secret.

This requirement concerns terminal verification of the origin-scoped user. It does **not** require the response to prove which principal the terminal authorized, create a new authorization-audit protocol, or provide globally portable claims about Alice.

## Rotation

There are two security levels.

### User rotation

Incrementing one user's epoch produces a new secret without changing the visible name or creation index:

```text
Alice at creation index C, epoch E → epoch E + 1
```

A user may rotate their own credential; the issuer may rotate, disable, or reprovision users under its lifecycle authority. Updates should be compare-and-set or otherwise retry-safe.

### Issuer-generation rotation

Compromise recovery for the user issuer requires Root to increment `issuer-generation` and derive a new sibling issuer key. An attacker holding the old issuer key must not be able to derive the new one.

Root then reprovisions Kratos with the new issuer key. Kratos regenerates active user credentials under the new generation and updates their verifiers/public keys. This is expected to be rare and requires an atomic or explicitly bounded transition plan so that the credential registry is not left partially rotated.

One API operation can represent both levels without multiplying endpoints, for example:

```scheme
(credential-rotate!
  :scope 'user
  :user 'alice
  :expected-epoch 3)

(credential-rotate!
  :scope 'user-issuer
  :expected-generation 1)
```

The shared endpoint does not imply shared authority: a user may rotate itself, the issuer may manage users, and only Root may rotate the issuer generation.

## Kratos posture

Kratos is a human/application usability and user-lifecycle layer, not the journal's ultimate authenticator.

A standard deployment may give a protected Kratos integration the user issuer key. It can:

- create the journal credential when a Kratos identity is registered;
- map the Kratos identity to the committed journal user name and creation index;
- regenerate the user's journal secret when needed;
- rotate or revoke the user's journal credential;
- bulk reprovision users when Root deliberately rotates the issuer generation.

Direct clients, agents, and services may authenticate to the raw Journal Interface with their own user secrets without depending on a live Kratos session. Kratos-specific storage and hook mechanics remain an implementation question.

## Consequence for staged programs

Because Root can deterministically derive the issuer and any current user's secret, the local Root step can authenticate an ordinary Interface call as that user. This allows scheduled local orchestration to call staged programs with `call!` while preserving the same per-operation user authorization checks as external requests.

This is intentional Root authority, not ambient authority granted to the staged program. The program still receives only its bounded Journal capability, and nested operations reauthenticate/re-authorize according to the `call!` contract.

## Non-goals

This idea does not yet specify:

- exact KDF parameters, binary encodings, or key algorithms;
- an implementation or migration from the shared Interface secret;
- final Root/private/public record layouts;
- exact Kratos hook or credential-broker mechanics;
- global human identity across journals;
- trust in an origin journal's user-provisioning quality;
- terminal proof of which principal was authorized in a response;
- user activity auditing or non-repudiation;
- deployment or rotation of any live credential.

Those details require separate correctness, security, and operational review before implementation.
