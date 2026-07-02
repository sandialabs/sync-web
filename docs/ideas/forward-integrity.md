# Forward integrity for Sync Web

## Purpose

Forward integrity is a journal-adjacent hardening direction for Sync Web-backed audit logs. The goal is to make historical operation records harder to forge after a later compromise of the current local secret state.

This should not be built into Sync Web's core record/persistor semantics for the first implementation. The better boundary is a small sibling tool/library that owns local secret state and HMAC chaining, then commits public tags/events/checkpoints into Sync Web.

This is broader than `agent-recorder`. Agent provenance is one useful consumer, but the same forward-integrity logger could help other audit streams: user writes, admin operations, bridge/synchronization events, WebDAV mutations, gateway events, and future session ingest.

## Location and packaging

Implementation should live as a sibling tool crate, close to `agent-recorder` but independent from it:

```text
tools/
  agent-recorder/
  forward-integrity-log/
```

`tools/forward-integrity-log/` should contain the reusable Rust library plus CLI/service binary:

```text
tools/forward-integrity-log/
  Cargo.toml
  src/
    lib.rs
    main.rs          # CLI/service entrypoint
    chain.rs         # HMAC/key evolution
    key_store.rs     # local current-key state
    sync_web.rs      # Sync Web checkpoint/write integration
    service.rs       # optional service mode
  Containerfile      # optional minimal image
```

Deployment wiring, if needed, belongs separately under `deploy/`:

```text
deploy/compose/forward-integrity-log/  # optional orchestration only
```

The tool should be usable as:

- a Rust library imported by `agent-recorder`;
- a bare executable;
- a minimal container/sidecar;
- a lightweight service process.

Keep implementation logic out of `deploy/`; `deploy/` should describe how to run services, not contain the service's core behavior.

## Desired property

A later attacker who compromises the current journal state should not be able to forge convincing historical operation tags for earlier epochs, assuming earlier forward-integrity keys were not captured when they existed.

A simple shape:

```text
K_i       = current secret key for epoch/index i
tag_i_j   = HMAC(K_i_j, operation_j || previous_operation_tag)
K_{i+1}   = H(K_i || block_context || operation_root)
```

After advancing to `K_{i+1}`, the journal deletes/forgets `K_i` and any per-operation keys.

This is not a replacement for Sync Web's hash-linked records, signatures, proofs, or bridge/witness commitments. HMAC-based verification is private/shared-secret verification. Public/non-repudiable verification still needs normal Sync Web commitments and signatures.

## Two-level chain model

A promising design is a two-level chain:

```text
block/index lineage:
  K_0 -> K_1 -> K_2 -> ...

within each open block/index interval:
  operation tags for set!, batch!, bridge!, synchronize!, etc.
```

For each operation before the next committed ledger step:

```text
op_key_i_j = HKDF(K_i, "operation", j)
op_tag_i_j = HMAC(op_key_i_j,
                  operation_event || previous_op_tag_i_(j-1))
```

At `step!` / block commit:

```text
op_root_i   = final operation tag or compact Merkle/root commitment
block_tag_i = HMAC(K_i,
                   index || previous_block_tag || op_root_i || new_head_digest)
K_{i+1}     = HKDF(K_i, "next", block_tag_i)
```

Then erase `K_i` and transient operation keys.

This bounds work by ledger epoch and lets verification replay from the initial secret/seed through committed indexes. Operation tags between indexes provide additional integrity over staged operations before they are folded into the next block commitment.

## Secret deletion reality

Forward integrity depends on old keys becoming unrecoverable. That is the hard part.

Do not store forward-integrity keys as normal Sync Web nodes. The normal node store and ledger history are intentionally durable; old states, proofs, pins, bridges, caches, backups, RocksDB files, and remote peers may preserve data.

RocksDB deletion is logical deletion, not secure erasure:

```text
Delete writes tombstones
compaction later removes old live entries
WAL/SST/filesystem/SSD remnants may retain bytes
```

Changing the main KV backend would not eliminate the issue. LSM stores, B-tree/page stores, copy-on-write databases, filesystem journals, snapshots, backups, and SSD wear-leveling all complicate forensic erasure. SQLite has `PRAGMA secure_delete=ON`, which can overwrite deleted payload bytes in database pages, but WAL/filesystem/SSD realities still prevent a general secure-erasure guarantee.

The practical rule:

> Secrets that require deletion should not enter the normal Sync Web record graph.

## Forward-integrity logger boundary

Use a host-local mutable secret store outside normal Sync Web history. The forward-integrity logger owns:

- current key state;
- HMAC/key evolution;
- append ordering;
- crash/recovery policy;
- local key deletion/replacement;
- public checkpoint/tag emission to Sync Web.

Sync Web records only public outputs:

```text
epoch/index
operation tag
block tag
algorithm/version
public commitment metadata
optional event payload/hash
```

The local FI store holds only current mutable secret state.

This avoids adding new secret primitives to Scheme, special sync-node leaf formats, or persistor transaction semantics. It also avoids awkward cases where record evaluation creates/deletes secrets and then fails, retries, or loses a compare-and-swap. The FI logger can have its own simple append protocol and recovery story.

Conceptual API:

```text
append(stream, event) -> tag/checkpoint metadata
checkpoint(stream) -> Sync Web commitment metadata
verify(stream, seed, events/checkpoints) -> result
serve/listen(...) -> optional local service mode
```

`agent-recorder` should call the FI logger as a library or local service when it wants forward-integrity protection, then write/checkpoint public results into Sync Web.

## Initial store backend

Start small: a plaintext current-key side file, with an honest guarantee statement.

Example layout:

```text
$DATA_DIR/forward-integrity-log/<stream-id>.key
```

or binary equivalent.

Recommended practices:

- directory mode `0700`;
- file mode `0600`;
- store only current epoch/key, never old keys;
- atomic replace on advance:
  - write temp file;
  - fsync temp file;
  - rename over old file;
  - fsync parent directory where practical;
- use `zeroize`/`secrecy` for in-memory key buffers where easy;
- never log key bytes;
- store only the minimal current state needed to resume the stream.

This is small footprint and adequate for the first software-only implementation.

Guarantee level:

> Protects against later logical compromise of current Sync Web state/current key file. Does not protect against forensic recovery from disk remnants, backups, snapshots, swap, prior compromise, or a live attacker who captures the key before it advances.

This is still useful. If hardware/offline forensic recovery is out of scope, logical deletion/replacement of a plaintext side file gives a reasonable practical forward-integrity baseline.

## Future store backends

Keep the secret store pluggable so deployments can improve the guarantee without changing ledger records.

Potential backends:

| Backend | Use | Notes |
| --- | --- | --- |
| plaintext file | simple development/default MVP | small footprint; honest logical-compromise guarantee |
| OS secret store | stronger local default later | macOS Keychain, Windows DPAPI/Credential Manager, Linux Secret Service/kernel keyring where available |
| encrypted file | unlock-once/session workflow | useful when a user can unlock after login/boot |
| TPM/KMS/HSM | higher-assurance services | more operational complexity; not MVP |
| SQLite side DB | many secret records/metadata | possible with `secure_delete=ON`, but still not SSD-forensic secure |

For a software-only local app, machine/user-bound OS secret storage is the most plausible later improvement: it supports non-interactive operation while avoiding a plaintext key file. It still does not protect against a live same-user compromise.

## Storage overhead

Forward-integrity key rotation is not a meaningful SSD wear issue.

A 256-bit key is 32 bytes. Even at one million rotations per day:

```text
32 bytes * 1,000,000 = 32 MB/day logical payload
```

Filesystem block/write granularity may make this behave more like several GB/day, but that is still modest for modern SSDs and likely much smaller than normal journal/event/RocksDB write volume. The hard problem is secure-erasure semantics, not write endurance.

## Why not explicit sync-node secure delete?

A primitive that promises definite deletion of a sync-node would be misleading and cross-cutting.

Normal Sync Web data may be retained by:

- immutable history;
- proofs/pins;
- bridges/peers;
- RocksDB WAL/SST files;
- caches/overlays;
- filesystem journals;
- backups/snapshots;
- SSD wear-leveling;
- process memory or swap.

So secure deletion should be scoped to the local secret/capability store, not normal content-addressed Sync Web state.

## Open design questions

- What exact crate/binary name should be used: `forward-integrity-log`, `fi-log`, or something else?
- Should `agent-recorder` use the FI logger as a Rust library first, a local service first, or support both from the start?
- What is the first event/checkpoint format emitted into Sync Web?
- Should batches be represented as one batch tag, per-item tags, or both?
- How should verification work for an operator with the initial seed/key?
- How should key initialization work for unattended service/container use?
- What is the migration path from plaintext side-file to OS-secret backend?
- What recovery story is acceptable if the local current key file is lost?
