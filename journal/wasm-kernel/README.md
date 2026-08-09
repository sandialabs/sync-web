# Wasmer evaluator kernel

This directory builds the exact Journal evaluator, s7, request-local sync graph, and serialization code as a bounded WebAssembly kernel. The native Journal process retains durable RocksDB persistence, external capabilities, commit/collision handling, deadlines, and authority.

Wasmer is the sole Journal request evaluator. The `wasmer-evaluator` feature is enabled by default and production builds fail to compile without it. Native C/s7 executes inside the Wasm kernel and narrowly explicit development-test internals only; there is no server-request runtime selector or native host fallback.

## Supported target and fail-closed configuration

The qualified AOT path is Linux x86_64. Production `journal-sdk` and `ledger` builds fail explicitly for another target rather than substituting an evaluator.

Configure the required verified artifact with:

```sh
export SYNC_WEB_WASMER_KERNEL=/absolute/path/kernel.wasmer
export SYNC_WEB_WASMER_KERNEL_SHA256=<lowercase-sha256>
```

Server binaries run an isolated in-memory genesis-state AOT health evaluation before durable persistor initialization, boot/install evaluation, or request service and exit with configuration status 78 if it is unavailable, invalid, or cannot execute. The health evaluation does not open, dispatch through, or mutate the persisted application Root, so a failed first start leaves the database fresh and installed Interface state can reopen unchanged. A separate invalid leaf-limit setting cannot bypass mandatory AOT health. The runtime rejects artifacts over 64 MiB from file metadata, performs a second bounded read, verifies the configured lowercase SHA-256, then deserializes the AOT.

## Reproducible AOT build

On Linux x86_64, install exact Rust/Cargo 1.96.0, LLVM 22.1.8 development libraries, and libffi/libxml2 runtime or development libraries, then run from a clean worktree:

```sh
scripts/build-wasmer-kernel /absolute/output/directory
```

The script rejects caller build flags/wrappers, pins and re-verifies a freshly extracted WASI SDK 33, uses locked Rust dependencies, and links a reactor with an exact 512 MiB memory maximum. Two independent target directories each rebuild source → Wasm → baseline-x86_64 Wasmer LLVM 7.2.1 AOT; unequal Wasm or AOT output fails. Dirty builds require the explicit development-only `SYNC_WEB_ALLOW_DIRTY_BUILD=1` and are marked unqualified in provenance. It emits:

- `kernel.wasmer` — architecture-specific trusted AOT;
- `provenance.json` — clean commit/content-manifest identity, source/Wasm/AOT hashes, compiler/runtime identity, empty CPU-feature baseline, target, memory bound, and both reproducibility results;
- `source.sha256`, `rustc.txt`, and `wasi-clang.txt`;
- `env.sh` — the exact runtime selector/path/hash settings.

AOT artifacts are target-specific and must not be reused across an unqualified architecture/runtime combination. The builder emits an `x86_64-unknown-linux-gnu` artifact with an empty optional CPU-feature set. The host attests only the verified artifact hash and actual host architecture/OS; it does not infer provenance from the artifact. Operators must retain and validate the separately emitted provenance manifest. The host verifies the configured SHA-256 before unsafe Wasmer deserialization and verifies the module-declared memory maximum.

## Isolation contract

Each top-level evaluation receives a fresh guest instance and request-local graph. The host enforces:

- 512 MiB module-declared memory maximum per guest;
- one cumulative, host-enforced 25-second wall-containment deadline for guest execution plus host HTTP send/stream work; this is a runtime safety boundary, not a deterministic language execution meter;
- at most 10,000,000 cumulative host transfers and 1 GiB cumulative request/response bytes per top-level operation; there is no cross-request or process-wide guest admission policy;
- 512 MiB of committed guest linear memory across each top-level causal operation, including blocking and detached descendants; every `memory.grow` reserves the causal operation-local remainder before VM growth, while independent operations do not share an admission counter;
- memory reset remains conservatively charged until instance teardown, and runtime memory clone/copy requests fail closed rather than creating unaccounted backing storage;
- at most 4 MiB minus 4 KiB per ordinary host capability request, HTTP/remote body, nested blocking result, or incremental `sync-all` response, and 16 MiB after Scheme transport materialization; top-level guest requests are preflighted at 16 MiB before host allocation, while final guest responses remain bounded at 256 MiB;
- durable root scans stop after the same bounded root count, including temporary roots;
- every native/Wasmer leaf write, graph import, and read boundary enforces `SYNC_WEB_MAX_LEAF_BYTES`; the setting is a fail-closed decimal byte count from 62 (the exact genesis leaf size) through 67,108,864 and defaults independently to 67,108,864, the same numeric value as the Journal Rocket query-size limit;
- trusted local RocksDB content is checked after retrieval without changing the five-column-family `sync-node-v2` format; leaf bytes, digests, proofs, root ordering, cross-open behavior, and the public `Persistor` API remain unchanged;
- blocking `sync-call` shares the caller's deadline, host quotas, and 512 MiB causal memory remainder; detached nonblocking work receives fresh deadline/call/byte counters but every generation shares the initiating operation's causal memory remainder, while independent top-level operations share no admission counter; each detached message is capped at 4 MiB minus 4 KiB with at most 64 queued/running jobs or 64 MiB of queued message bytes process-wide;
- bounded capability failures return an error (or a guest trap error if the shared deadline expires before an error can be copied back), and the next request receives fresh containment state;
- no ambient WASI (only bounded random and clock imports; all other WASI imports deny);
- parent-owned durable commit and external capabilities, with the native collision policy: one optimistic attempt followed by serialized re-evaluation;
- no commit when external capability use also changes state;
- no replay of failed remote capabilities; when a capability or persistence response outgrows the guest's initial buffer, the host retains that invocation's bounded response only for the immediate resize retry, copies it without re-executing the operation, and clears it before any later invocation;
- rollback on traps, malformed responses, quota exhaustion, and commit collision;
- immediate next-request health after guest failure.

There is no Wasmer engine fuel or generated s7 dispatch meter. Fine-grained deterministic execution accounting is intentionally deferred. The Wasmer host's wall deadline, guest memory, host transfers/bytes, response sizes, and detached-work admission are runtime containment boundaries; they do not define portable Scheme semantics.

After the first successful guest evaluation in a process lifetime, stderr emits a structured `wasmer-runtime-attestation` line containing evaluator name, verified artifact SHA-256, host target, and a positive evaluation count.

## Performance status

Prior native-default or generated-meter results do not qualify this no-meter runtime. Performance and batch-size claims require positively attested Wasmer runs with the exact AOT hash. Capability-request sizing is reported by opt-in `SYNC_WEB_WASM_PROFILE` instrumentation; the 4,190,208-byte per-capability limit remains unchanged.
