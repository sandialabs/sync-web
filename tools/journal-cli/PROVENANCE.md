# Provenance

The initial `journal-cli` consolidation imports reviewed owner-local Python implementations and preserves their tests before adapting their Journal boundary to Sync Web 1.6.

| Component | Imported source | Imported version or SHA-256 |
|---|---|---|
| Primitive client behavior | `/usr/bin/pi-sync` | `0046a5645d4b633c295d0315b29ca5afdc2de93ece4528d161636541f2787deb` |
| Enrollment | `/usr/bin/pi-sync-inbox` | `20022aa5e07a3e88c284273d97ac4b68e02a0d6857b63ecc21f57bb0947bc326` |
| Peer convenience registry | `/usr/bin/pi-sync-peer` | `4e0482ff93da59f8e33b3f12251b29837dd59ff7e4a948002dcb23ae8e96390f` |
| Mailbox diagnostics and capability cards | `agent-railway/v7` | owner-local v7 source |
| Profile | `sync-agent-profile-v0/pi_sync_profile.py` | `d10b9a4285eec38fae1424b7b665748c66b5fc429c1c15e03dcc0acbf3faef5e` |
| Source Publication | `sync-source/v0.4.3` | qualification and manifest retained in the owner-local source package |
| Source low-level adapter | `sync-source/v0.4.3/integration-v11` | imported v1 implementation; replaced by locator-bound v2 |
| Source workflow | `sync-source-flow/v0.1.1` | accepted producer/reviewer workflow implementation |
| Bridge deletion | `pi-sync-delete-bridge` | `8c5d939979faa8d85542b794a62131e0c9e6a7506b5b928b7c1f54e6b66e05fc` |
| Recipient-route replacement | `pi-sync-recipient-route-replace` | `778ad068443e69bd69a8973455c28f4fe83f44ad4c9af18b8cc3ea80446be39c` |

The repository implementation is not a byte-for-byte package mirror. Imports are namespaced, the low-level adapter is internalized, operation and grant construction is migrated to Sync Web 1.6, and one public command tree replaces the prior launchers. Source v2 follows the owner's clean-break endpoint-plus-route decision: old Journal identity fields and external adapter compatibility are removed rather than aliased. Profile v2 likewise replaces the imported Profile identity field with a canonical publisher endpoint while keeping consumer entry routes observer-relative. Peer enrollment now exchanges the SHA-256 of the signing public key instead of the removed Journal identity ID.

The implementation base is local branch `feat/tools` at `7138a738f17a9da53de2f7267bd9bc30cedc031d`, based on released Sync Web 1.6 commit `3a1a56346607ba5c76c79bb11831cf33e37df5bc`.
