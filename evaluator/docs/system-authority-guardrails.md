# System authority guardrails

`s7-rust` targets sync-web-compatible s7 behavior without granting the interpreter direct host/system authority.

The hard boundary is:

> `s7-rust` must not gain host/system authority beyond deterministic in-memory evaluation and explicit sync-web-provided host primitives.

This is more important than avoiding “full s7” compatibility in the abstract. Compatibility work on macros, environments, generalized setters, in-memory ports, formatting, numbers, sequences, reader/printer behavior, and diagnostics is acceptable unless it crosses this authority boundary.

## Denied authority surface

Do not add or re-enable direct interpreter access to:

- **Filesystem authority**: `open-input-file`, `open-output-file`, `load`, path probing, directory/file operations.
- **Process/system authority**: `system`, `getenv`, `exit`, `abort`, shell/process execution.
- **Dynamic/native loading and embedder APIs**: `autoload`, loading `require`, C loader, C pointers, C objects, C/native function access.
- **Direct network authority**: sockets, HTTP, or other network access from the interpreter. Network behavior must be host-mediated through approved sync-web primitives.
- **Nondeterminism**: `random`, `random-state`, wall-clock/time, environment-dependent state unless explicitly injected by the host.
- **Broad debug/profiling/hook machinery** if it creates runtime authority or mutable global plumbing.

## Enforcement direction

Keep the guard concrete and testable:

1. Maintain an explicit denied-system-surface list.
2. Add tests asserting denied names are absent/removed with sync-web-compatible behavior.
3. Add a builtin registry/source scan that fails if denied names are registered.
4. Treat any new OS/network/time/native-access feature as out of scope unless Thien-Nam explicitly approves a host-mediated primitive.

The standalone guard check is:

```sh
tools/check-system-authority.py --candidate target/release/s7-rust
```

It is intentionally separate from the default `tools/test.py` flow until the current denied surface is cleaned up.

When a compatibility case touches a denied area, prefer matching the sync-web-visible absence/error behavior rather than implementing the underlying authority and removing it later.
