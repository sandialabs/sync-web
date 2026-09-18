# Messenger Architecture

Synchronic Messenger is a browser consumer of existing Sync Web Gateway operations. It does not extend the Journal, Records, Interface, federation, or authorization protocols.

## Message Transport

A direct v1 envelope contains a UUID, exact sender and recipient endpoints, a canonical creation timestamp, and ordinary text. A minimal v2 group envelope adds one stable conversation UUID and a sorted, fixed participant list. Group delivery is independent point-to-point fanout: the same message UUID and conversation UUID are written separately to each configured recipient.

The browser serializes envelopes with compact `JSON.stringify` UTF-8 bytes. Receivers derive the expected sender, recipient, and UUID from the authenticated mailbox path and reject mismatches. Unknown v1 fields are dropped after validation. Envelope metadata does not create contacts, routes, grants, or authority.

Replies carry only the referenced message UUID. Quoted-parent rendering is resolved from validated local projection state. A missing parent remains visibly unavailable; Messenger does not fetch it or claim message ancestry.

## Mailboxes And Authority

Each contact records two deliberately separate directions:

- an outbound route used to reach the remote owner's mailbox;
- an incoming principal used to authorize that correspondent against the local mailbox.

Contact presence is the local Messenger communication grant. After authentication, Messenger reads the authoritative authorization table, adds the exact `[-32, -1]` incoming mailbox rule for every contact, removes stale or differently shaped Messenger mailbox rules, and requires exact convergence before send or poll begins.

This reconciliation does not create a bridge, route, remote grant, credential, or recipient relationship. Remote policy remains independently controlled by the remote Journal.

## Browser Projection

Messages, drafts, read state, blocks, and groups are projected into `localStorage`. State is partitioned by browser origin, authenticated username, and local Journal. Writes merge with the latest stored value so one tab cannot blindly erase a newer send; polling adopts newer cross-tab state. Retention is ordered by envelope creation time, and prepared wire copies are not persisted.

The projection is a convenience cache, not canonical history. Messenger can recover a missing outgoing direct or group message by reading the sender-keyed copy from the configured recipient through an existing exact federated read-only `use` grant. Recovery validates local-sender and remote-recipient bindings and performs no write or retry.

## Contact Registry

The contact registry is durable raw-byte state at `(*state* <username> messenger contacts.json)`. Updates use exact expected bytes, so concurrent edits fail rather than overwrite. Contact, endpoint, route, owner, Journal, and incoming-principal components are normalized and validated before migration, profile access, authorization reconciliation, or messaging.

## Profiles

Profiles are public owner-authored descriptive metadata, not identity or authority. Remote display performs one explicit current federated read using the configured outbound route, owner, Journal, and exact owner-relative `profile.scm` path.

The embedded 32-byte identity ID is validated as publisher metadata only. Every observation reports route-owner-Journal-path-current authentication scope and explicitly leaves terminal-Journal continuity unproven. Profile code does not run in messaging, polling, authorization, or recovery paths.

A local profile save uses one exact expected-byte write and exact current readback. Conflict or ambiguous failure requires a fresh current read before another write. Profile prose is rendered only as text.

## Gateway Boundary

The nginx sidecar forwards the existing Kratos session cookie while stripping browser-supplied `Authorization`. It exposes only:

- Kratos `whoami`;
- Gateway Journal information and change events;
- `use`, `put`, `authorizations`, `authorize`, and `deauthorize`.

The sidecar never receives the Journal secret, Root verifier, bridge credentials, or Interface bearer credential.
