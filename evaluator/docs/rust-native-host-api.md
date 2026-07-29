# Rust-native host value and primitive API

`RustHost` is the initial Rust-native embedding contract. It intentionally does
not recreate the s7 C API and grants no authority by itself.

## Values

A host-defined value implements `HostObject`:

- `type_name` supplies the Scheme-visible type name;
- `identity` supplies the default stable deterministic equality within that type;
- `equals` can override equality when the host type needs more than identity;
- `display` supplies deterministic object text;
- `as_any` permits typed downcasting inside host primitive implementations.

The interpreter stores the object behind a thin `Rc<HostValueData>`, preserving
the two-word `Value` representation. Scheme `equal?`/`eq?` use the host equality
contract. Formatting and equality use unwind guards when the embedding build uses
`panic = "unwind"`. The repository's optimized release profile uses
`panic = "abort"`, so production host implementations must not panic; an aborting
panic cannot be converted into a Scheme error.

Host objects may own journal nodes or other Rust state through their `Rc` payload.
The surrounding `OwnedValue` keeps Scheme pair storage alive independently.

## Primitives

A caller registers `PrimitiveSpec` values with a `RustHost`. Callbacks receive:

- a slice of scoped `BorrowedValue` arguments;
- a `HostCallContext` for explicit cancellation checks and metering charges;
- no filesystem, process, network, clock, randomness, or native-loader handle.

Callbacks return typed `HostOutput` values or `HostError`. Supported output forms
include booleans, integers, strings, byte vectors, parsed Scheme expressions,
lists, pairs, host objects, nil, and unspecified. `HostOutput::Apply` requests a
controlled Scheme call after the callback has returned. Argument wrappers provide
checked access to integers, strings, byte vectors, and typed host objects.

Direct callback reentry is rejected. A returned `HostOutput::Apply` releases the
callback guard before applying Scheme, so Scheme-driven nested host calls run
normally without replaying the original callback. Callback panics become
`host-error` only in unwind-enabled builds; production abort-profile callbacks
must be panic-free. The current policy is single-threaded and non-reentrant;
independent hosts can execute on independent threads because no interpreter state
is shared.

`RustHost::register_codecs` installs deterministic expression/byte-vector and hex
codecs. Crypto, secure random, time, HTTP, and remote operations must be supplied
as ordinary explicit callbacks by the journal. They are never interpreter
builtins.

`SYNC_WEB_HOST_PRIMITIVES` is the executable inventory of the integration surface,
including sync-web's current variadic `print` primitive.
`missing_sync_web_primitives` reports contract gaps before running Records.

## Evaluation and lifetime

`RustHost::evaluate` creates an isolated evaluator and pair-arena generation,
registers callbacks, evaluates configured initialization source, then evaluates
the request. External host state captured by callback closures persists across
calls and can be reconstructed by a new `RustHost`, while Scheme request state is
isolated. This conservative initial policy avoids persistent-root arena leakage;
a pooled persistent evaluator will require an explicit tracing/rooting design.

Results and errors retain their arena generation as documented in
[`pair-arena-ownership.md`](pair-arena-ownership.md).

## Sync Web implementation mapping

The journal can implement sync nodes as `HostObject` values whose identity is the
node digest or another stable node identifier, and register:

- node/state: `sync-state`, `sync-node?`, `sync-null`, `sync-null?`,
  `sync-pair?`, `sync-stub`, `sync-stub?`, `sync-digest`;
- construction/storage: `sync-cons`, `sync-car`, `sync-cdr`, `sync-cut`,
  `sync-create`, `sync-delete`, `sync-all`;
- journal/service calls: `sync-call`, `sync-eval`, `sync-http`, `sync-remote`;
- console output: variadic `print`, returning its final argument or unspecified;
- deterministic hashing/codecs: `sync-hash`, expression and hex codecs;
- explicit capabilities: secure random, crypto operations, and system time.

Host callbacks remain responsible for transaction/session semantics and typed
journal errors. The interpreter remains a deterministic in-memory evaluator.
