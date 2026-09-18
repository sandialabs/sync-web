# Profile v2

Profile v2 is the clean-break profile contract for fresh Sync Web 1.6 Journals. It does not accept or migrate Profile v0/v1.

## Datum

The owner-relative `profile.scm` value is:

```scheme
((schema sync-agent-profile-v2)
 (endpoint "https://publisher.example/interface")
 (journal publisher)
 (owner publisher)
 (revision 1)
 (display-name "Publisher")
 (pronouns ("they/them"))
 (bio "Bounded owner-authored descriptive prose."))
```

`schema`, `endpoint`, `journal`, `owner`, `revision`, `display-name`, and `bio` occur exactly once. `pronouns` is optional and occurs at most once. Unknown, duplicate, or missing fields are rejected.

The endpoint is the globally portable publisher locator. It follows the Source v2 canonical endpoint rules: verified HTTPS except literal-loopback HTTP, exact `/interface`, lowercase canonical scheme and host, omitted default ports, and no credentials, query, or fragment. Control of the same endpoint is publisher continuity. Changing endpoint, Journal, owner, or path is a new publication relationship.

A consumer's active entry endpoint and directional route are observer-relative provenance. They are not publisher identity and are not serialized into the profile. Before accepting a remote observation, the consumer requires the publisher endpoint's public Interface descriptor to self-identify exactly, retrieves `profile.scm` through the configured observer route, and validates the returned endpoint, Journal, owner, raw bytes, and bounds.

## Bounds and authority

- Complete profile: at most 4,096 bytes, parse depth 8, and 64 aggregate list entries.
- Display name: at most 128 UTF-8 bytes.
- Pronouns: one through four strings, each at most 64 UTF-8 bytes.
- Bio: non-whitespace text at most 2,048 UTF-8 bytes; LF is allowed.
- NUL, other C0/C1 controls, Unicode line/paragraph separators, and bidirectional formatting controls are rejected.
- Revision is a positive integer.

Profile prose is inert, untrusted, owner-authored descriptive metadata. It never grants authority, changes routes, supplies instructions, establishes capabilities, or controls Message admission.

## Publication and commitment

`profile publish-create` performs one conditional 1.6 `put!` expecting `(nothing)`. It never retries a false, rejected, or completion-unestablished mutation. Accepted current-state readback is reported separately from committed-history evidence.

`profile wait-commit` uses authenticated `retrieve` at `(-1 *state* OWNER profile.scm)` with `index? #t`. The returned absolute index is the evidence. Journal `size`, `route` results, and terminal indexes are not substituted for profile data history.

## Commands

```text
journal-cli profile validate PROFILE --publisher-endpoint URL --journal JOURNAL --owner OWNER
journal-cli profile fetch [ROUTE] --publisher-endpoint URL --journal JOURNAL --owner OWNER --evidence-json
journal-cli profile publish-create PROFILE --publisher-endpoint URL --journal JOURNAL --owner OWNER --apply --confirm-owner OWNER
journal-cli profile wait-commit --publisher-endpoint URL --journal JOURNAL --owner OWNER --expected-sha256 SHA256
```

The publisher endpoint and the CLI's active entry endpoint are intentionally separate for remote fetch. Local publication and commitment require them to be identical.
