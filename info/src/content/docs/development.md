---
title: Development
sidebar:
  label: Development
  order: 4
head: []
---
This section is for developers extending runtime behavior, class logic, and APIs.
The core pattern is progressive layering through `interface.scm` and class modules loaded at boot.

## Architecture

This section explains the implementation layers in the same order they are assembled at runtime.
If you are debugging a behavior issue, following this order usually narrows root cause quickly.

### System

At runtime, each query is evaluated against the current `*sync-state*` root.
The left side carries executable logic and the right side carries persistent state.
The general stack boot flow is:

1. Install `control.scm`.
2. Install/instantiate `standard.scm`.
3. Install class definitions (`log-chain.scm`, `tree.scm`, `configuration.scm`, `ledger.scm`).
4. Instantiate and store `ledger` object.
5. Install interface secret and query dispatcher.

Concurrency behavior in `sync-journal` is optimistic:

- queries run against a snapshot of root state
- if state changed concurrently, evaluation is retried
- writes eventually synchronize through compare-and-set root updates
- a global lock is used after repeated collisions to serialize update attempts

Operationally, this behaves like many concurrent readers with controlled commit contention on writes.
That model is important for developers writing side-effecting logic: deterministic behavior depends on understanding retry semantics.

Memory/persistence modes:

- in-memory mode (default when no `--database` path is supplied)
- persistent RocksDB-backed storage when `--database` is configured

Periodic stepping:

- configured by `--step` and `--period`
- used in `sync-services` to invoke `*step*` continuously

### Language

The evaluator embeds s7 Scheme and removes high-risk primitives (file/port/loader/escape surfaces).
It also adds convenience and Synchronic primitives.
The result is a constrained but expressive execution environment intended for programmable policy and state logic.

References:

The s7 reference is in `sync-journal/external/s7/README.md`, and the removed primitive list is defined in `sync-journal/src/evaluator.rs` (`REMOVE`).

Extended evaluator primitives (examples):

Representative examples include `expression->byte-vector` and `byte-vector->expression`, `hex-string->byte-vector` and `byte-vector->hex-string`, and utility helpers such as `random-byte-vector`, `time-unix`, and `print`.

### Structure

Everything is represented as a hash-addressed DAG of sync nodes.
`sync-cons`, `sync-car`, `sync-cdr`, `sync-cut`, and `sync-digest` expose low-level structure operations.
The language runtime controls interpretation of this DAG through object methods and query handlers.
This separation of structure and interpretation is one reason the stack can evolve quickly without breaking core data guarantees.

### Objects

Universal object shape:

The left child holds code (constructor/behavior), and the right child holds object state.

`standard.scm` implements a compact object protocol:

`standard.scm` implements method dispatch via `(object 'method)`, state access shorthand via `(self '(path ...))`, composition without inheritance by nesting objects in state, and explicit deep operations such as `deep-get`, `deep-set!`, `deep-call!`, and `deep-merge!`.

In practice, most domain features are built by composing these object primitives rather than introducing new global runtime behavior.

## Specifications

Use this section as a reference when wiring local environments, reviewing pull requests, or mapping code changes to runtime behavior.

### Journal Configuration

`journal-sdk` command-line options:

- `--database` / `-d`: persistent DB path
- `--port` / `-p`: HTTP port (default `4096`)
- `--boot` / `-b`: boot expression evaluated at startup
- `--evaluate` / `-e`: one-shot query and exit
- `--step` / `-s`: periodic step expression
- `--period` / `-c`: seconds between step calls

### s7 Extended Primitives

The runtime extends base s7 with utility and Synchronic primitives.
These signatures are sourced from primitive registrations in `sync-journal/src/evaluator.rs` and `sync-journal/src/lib.rs`.

#### Utility Primitives

| Primitive | Signature | Description |
| --- | --- | --- |
| `expression->byte-vector` | `(expression->byte-vector expr)` | Encode expression into byte-vector form. |
| `byte-vector->expression` | `(byte-vector->expression bv)` | Decode byte-vector into expression. |
| `hex-string->byte-vector` | `(hex-string->byte-vector str)` | Convert hex string to byte-vector. |
| `byte-vector->hex-string` | `(byte-vector->hex-string bv)` | Convert byte-vector to hex string. |
| `random-byte-vector` | `(random-byte-vector length)` | Generate securely random byte-vector of given length. |
| `time-unix` | `(time-unix)` | Return current Unix time in seconds. |
| `print` | `(print obj ...)` | Print values and return last value. |

#### Sync Structure Primitives

| Primitive | Signature | Description |
| --- | --- | --- |
| `sync-stub` | `(sync-stub digest)` | Create a stub node from digest. |
| `sync-hash` | `(sync-hash bv)` | Compute SHA-256 digest of byte-vector. |
| `sync-node` | `(sync-node digest)` | Load sync node identified by digest. |
| `sync-node?` | `(sync-node? obj)` | Check whether object is a sync node. |
| `sync-null` | `(sync-null)` | Return null sync node. |
| `sync-null?` | `(sync-null? sp)` | Check whether node/value is null sync node. |
| `sync-pair?` | `(sync-pair? sp)` | Check whether node is a pair node. |
| `sync-stub?` | `(sync-stub? sp)` | Check whether node is a stub node. |
| `sync-digest` | `(sync-digest node)` | Return digest of sync node. |
| `sync-cons` | `(sync-cons first rest)` | Construct sync pair node. |
| `sync-car` | `(sync-car pair)` | Return first child of sync pair. |
| `sync-cdr` | `(sync-cdr pair)` | Return second child of sync pair. |
| `sync-cut` | `(sync-cut value)` | Convert node/value into stub form. |

#### Record Lifecycle Primitives

| Primitive | Signature | Description |
| --- | --- | --- |
| `sync-create` | `(sync-create id)` | Create record for 32-byte ID. |
| `sync-delete` | `(sync-delete id)` | Delete record for 32-byte ID (except root record). |
| `sync-all` | `(sync-all)` | List all record IDs. |

#### Network, Execution, and Crypto Primitives

| Primitive | Signature | Description |
| --- | --- | --- |
| `sync-call` | `(sync-call query blocking? id)` | Evaluate query against target record (or current record if `id` omitted). |
| `sync-http` | `(sync-http method url . data)` | Perform HTTP request (`get` or `post`). |
| `sync-remote` | `(sync-remote url data)` | Perform remote post request with payload. |
| `crypto-generate` | `(crypto-generate seed)` | Derive public/private key pair from seed bytes. |
| `crypto-sign` | `(crypto-sign private-key message)` | Sign message with private key. |
| `crypto-verify` | `(crypto-verify public-key signature message)` | Verify signature against message/public key. |

When adding new primitives, keep conversion behavior and security constraints in mind so JSON/Lisp workflows remain predictable.

### Standard Objects

The object model is loosely Python-inspired:

The model uses methods instead of free global variables, captures a single state root per object, uses class-like definitions via `define-class`, and favors explicit composition over inheritance.

State path shorthand uses cdr/car traversal semantics:

State path shorthand follows cdr/car traversal semantics such as `(self '(1 0 0 1 ...))`.

In the tables below, methods prefixed with `*` (for example `*init*`) are still part of the public class API.
Only methods prefixed with `~` are treated as internal/helper methods.

#### Normative Guidelines

For standard object implementations, `define-class` and `define-method` are the normative authoring forms.

- New runtime classes SHOULD be expressed as a single `define-class` form.
- Class members MUST be declared with `define-method`; arbitrary top-level expressions inside a class body are not part of the supported model.
- Constructors SHOULD be implemented as `(*init* self ...)` methods rather than ad hoc initialization outside the class form.
- Internal helper behavior SHOULD be exposed as `~`-prefixed methods, while stable external behavior SHOULD use non-`~` method names.

This convention keeps class loading deterministic, keeps object serialization semantics predictable, and aligns with how `standard.scm` validates class definitions during `make`.

#### Standard Class

Public API (`standard.scm`):

| Method | Signature | Description |
| --- | --- | --- |
| `make` | `(make self class (init ()) debug)` | Instantiate an object from a `define-class` form; optionally run constructor args and debug tracing. |
| `dump` | `(dump self object)` | Return raw sync-node representation of an object. |
| `load` | `(load self node)` | Rehydrate an object from serialized sync-node content. |
| `deep-get` | `(deep-get self object path)` | Read value/object across nested object boundaries. |
| `deep-set!` | `(deep-set! self object path value)` | Write value across nested object boundaries. |
| `deep-slice!` | `(deep-slice! self object path)` | Slice object graph along path while preserving digest invariants. |
| `deep-prune!` | `(deep-prune! self object path)` | Prune object graph along path while preserving digest invariants. |
| `deep-merge!` | `(deep-merge! self object-source object-target)` | Merge digest-equivalent object structures. |
| `deep-copy!` | `(deep-copy! self object path-source path-target)` | Copy value from one nested path to another. |
| `deep-call!` | `(deep-call! self object path function)` | Call function at nested path and persist resulting state. |
| `serialize` | `(serialize self node query)` | Build compact proof-oriented serialization of a node/query view. |
| `deserialize` | `(deserialize self serialization)` | Rebuild sync-node structure from serialization output. |

#### Tree Class

Public API (`tree.scm`):

| Method | Signature | Description |
| --- | --- | --- |
| `obj->node` | `(obj->node self obj)` | Encode Lisp/runtime value into sync-node storage representation. |
| `node->obj` | `(node->obj self node)` | Decode sync-node storage representation into runtime value. |
| `get` | `(get self path)` | Read value at key-path; returns value, `(nothing)`, `(unknown)`, or directory metadata. |
| `equal?` | `(equal? self source path)` | Exact structural equality check between two paths. |
| `equivalent?` | `(equivalent? self source path)` | Digest-equivalence check between two paths. |
| `set!` | `(set! self path value)` | Write value at path (or delete when value is `(nothing)`). |
| `copy!` | `(copy! self source path)` | Copy value from source path to target path. |
| `prune!` | `(prune! self path keep-key?)` | Prune proof/state detail at path. |
| `slice!` | `(slice! self path)` | Slice state to keep proof for path and cut unrelated branches. |
| `merge!` | `(merge! self other)` | Merge compatible tree structures. |
| `valid?` | `(valid? self)` | Validate structural/key-prefix consistency of the tree. |

#### Linear Chain Class

Public API (`linear-chain.scm`):

| Method | Signature | Description |
| --- | --- | --- |
| `*init*` | `(*init* self)` | Initialize empty chain state. |
| `get` | `(get self index)` | Return entry at normalized index. |
| `previous` | `(previous self index)` | Build proof chain ending at index. |
| `digest` | `(digest self (index ...))` | Return digest for proof chain at index. |
| `size` | `(size self)` | Return chain length. |
| `index` | `(index self index~)` | Normalize/index-check external index input. |
| `push!` | `(push! self data)` | Append entry to chain. |
| `set!` | `(set! self index data)` | Replace entry at index. |
| `slice!` | `(slice! self index)` | Slice proof view around index. |
| `prune!` | `(prune! self index)` | Prune proof detail at index. |
| `truncate!` | `(truncate! self index)` | Truncate chain after index and return cut tail. |

#### Log Chain Class

Public API (`log-chain.scm`):

| Method | Signature | Description |
| --- | --- | --- |
| `*init*` | `(*init* self)` | Initialize empty log-structured chain state. |
| `size` | `(size self)` | Return chain length. |
| `index` | `(index self index~)` | Normalize/index-check external index input. |
| `get` | `(get self index)` | Return entry at normalized index. |
| `previous` | `(previous self index)` | Build proof chain ending at index. |
| `digest` | `(digest self (index ...))` | Return digest for proof chain at index. |
| `push!` | `(push! self data)` | Append entry to chain. |
| `set!` | `(set! self index data)` | Replace entry at index. |
| `slice!` | `(slice! self index)` | Slice proof view around index. |
| `prune!` | `(prune! self index)` | Prune proof detail at index. |
| `truncate!` | `(truncate! self depth)` | Truncate proof tree depth by cutting deeper nodes. |

#### Configuration Class

Public API (`configuration.scm`):

| Method | Signature | Description |
| --- | --- | --- |
| `*init*` | `(*init* self (config '()))` | Initialize configuration expression state. |
| `get` | `(get self path)` | Read nested configuration value by symbol-path. |
| `set!` | `(set! self path value)` | Set nested value; delete branch when value is `'()`. |

#### Ledger Class

Public API (`ledger.scm`):

| Method | Signature | Description |
| --- | --- | --- |
| `*init*` | `(*init* self standard config)` | Initialize ledger with standard helper and configuration object. |
| `configuration` | `(configuration self)` | Return full configuration (public + private). |
| `information` | `(information self)` | Return public configuration subset. |
| `size` | `(size self)` | Return permanent chain length. |
| `peer!` | `(peer! self name info)` | Register/update peer metadata and cached public key. |
| `peers` | `(peers self)` | List configured peer names. |
| `set!` | `(set! self path value)` | Stage local state mutation. |
| `get` | `(get self path details?)` | Read staged/historical value; optional content/pinned/proof bundle. |
| `pin!` | `(pin! self path)` | Pin path into permanent chain retention. |
| `unpin!` | `(unpin! self path)` | Remove previously pinned path. |
| `synchronize` | `(synchronize self index)` | Serialize peer-sync proof view at index. |
| `resolve` | `(resolve self index path)` | Serialize resolved path view for remote verification. |
| `step-peer!` | `(step-peer! self name)` | Fetch and verify peer chain head into staged state. |
| `step-chain!` | `(step-chain! self)` | Commit staged state to chain, sign, prune by window, return new size. |
| `step-generate` | `(step-generate self)` | Generate ordered step operations (chain + peers). |
| `*update*` | `(*update* self class function)` | Return updated ledger object after applying class-scoped transformation. |

## Testing

Testing coverage is intentionally split across correctness, stress, and topology concerns so regressions can be isolated by failure type.

### Synchronous Testing

Use `sync-records/tests` for deterministic, script-driven correctness checks over simulated journals.

Prerequisites:

You need either a built `journal-sdk` binary from `sync-journal` or Docker access to run `ghcr.io/sandialabs/sync-journal/journal-sdk`, and you need a POSIX shell environment.

Run with local binary:

```bash
cd /code/sync-records/tests
./test.sh /absolute/path/to/journal-sdk
```

Run with Docker:

```bash
cd /code/sync-records/tests
docker pull ghcr.io/sandialabs/sync-journal/journal-sdk
./test.sh "docker run ghcr.io/sandialabs/sync-journal/journal-sdk"
```

These tests validate class behavior and cross-journal message scripts in a repeatable sequence.

### Service Stack Smoke Tests

Use `sync-services/tests` when you need to validate the integrated runtime plus web routes (`/explorer`, `/workbench`, `/interface/json`).

Prerequisites:

You need Docker Engine with the `docker compose` plugin and `curl` for route and API checks.

Run interactive stack:

```bash
cd /code/sync-services
SECRET=password PORT=8192 ./tests/local-compose.sh up
```

Run automated health/smoke checks:

```bash
cd /code/sync-services
./tests/local-compose.sh smoke
```

Optional local Lisp override (to test in-progress local bootstrap code):

```bash
cd /code/sync-services
LOCAL_LISP_DIRECTORY=/absolute/path/to/lisp ./tests/local-compose.sh smoke
```

The smoke script verifies route readiness and key API behavior (for example `size` and authenticated `configuration` responses).

### UI Unit Tests

`sync-services/services/explorer` and `sync-services/services/workbench` include component/unit tests for client behavior.

Prerequisites:

You need Node.js 20+ and npm to run the Explorer and Workbench unit test suites.

Run tests:

```bash
cd /code/sync-services/services/explorer
npm install
npm test
```

```bash
cd /code/sync-services/services/workbench
npm install
npm test
```

### Stress Testing

Use `sync-analysis/locust` for HTTP load generation against journal interface endpoints.

Prerequisites:

You need a running interface endpoint (typically from `sync-services`), Python 3 with `pip` and Locust dependencies from `requirements.txt`, and a `SECRET` environment variable that matches server configuration.

Install and run:

```bash
cd /code/sync-analysis/locust
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt
SECRET=pass locust --host=http://localhost:8192/interface
```

Headless example:

```bash
cd /code/sync-analysis/locust
. .venv/bin/activate
SECRET=pass locust --host=http://localhost:8192/interface --users=10 --spawn-rate=2 --run-time=60s --headless
```

### Network Emulation

Use `sync-analysis/firewheel` for topology-level distributed simulation across journals, agents, and monitoring components.

Prerequisites:

You need the FIREWHEEL framework installed and configured, Docker available on the host, and enough host resources for multi-node simulation workloads.

Example experiment:

```bash
cd /code/sync-analysis/firewheel
firewheel experiment -r synchronic_web.ledger_journal:4:2 synchronic_web.social_agent:4:32:2:8 synchronic_web.network_monitor control_network minimega.launch
```

This mode is best for validating emergent behavior, peer dynamics, and monitoring instrumentation under realistic network layouts.

## Guidance

This section captures conventions and failure patterns that are useful during day-to-day implementation and review work.

### Style Guide

Recommended conventions used across current class modules:

- prefer `(object 'method)` style to reduce namespace pollution
- use `*variable*` names for special/global runtime variables
- use `~name` for internal/helper methods and hidden fields
- use association lists for web-facing calls to keep JSON conversion predictable
- keep code in left node and mutable state in right node

Following these conventions substantially improves maintainability when multiple contributors are updating class logic concurrently.

### Gotchas

Common pitfalls in this stack:

- read remote moving state -> wait on network -> mutate local state based on stale assumptions
- mutating a child object does not automatically reattach it into its parent container; write it back explicitly
- broad `*eval*`/`*call*` usage can bypass intended API boundaries; prefer narrow method-level updates where possible

When possible, add targeted assertions around state digests or path expectations to detect these issues early during development.
