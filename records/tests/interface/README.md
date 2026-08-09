# Deterministic Interface tests

The `records-test` crate executes trusted Scheme test procedures with source
files supplied as positional Scheme data. These cases are part of the single
canonical `records/tests/suite.toml` suite and use a deterministic discrete-event
scheduler for journal-to-journal workflows.

## Test file shape

The first command-line path contains one procedure. Every later path is read as
one Scheme datum and passed to that procedure in command-line order:

```sh
records-test test.scm source-a.scm source-b.scm
```

is equivalent to evaluating:

```scheme
((lambda (source-a source-b) ...)
 '(contents of source-a.scm)
 '(contents of source-b.scm))
```

This keeps test preprocessing and result validation in one customizable Scheme
invocation.

## Deterministic scenarios

The low-level action shape is:

```scheme
("canonical journal URL" expression (schedule ...) tick)
```

The optional `tick` is the non-negative time between this action's start and
the previous submitted action's start. It defaults to zero. Journals are
created lazily from the SHA-256 digest of their exact URL.

Schedule entries apply only to journal-to-journal `sync-remote` requests and
responses. A non-negative integer is relative delivery latency on one global
discrete clock; `#f` drops that message and immediately returns transport
failure to the waiting caller. Missing entries are zero and extras are ignored.
Client requests and final client results are instantaneous and consume no
schedule entries.

All events at one time execute in stable FIFO order. Newly produced
zero-latency events append behind events already queued for that time. Scheme
system-time calls observe the current scheduler second, so repeated calls in
one slot return the same value. Global time, URL-addressed journal state, and
the fixed-seed random stream persist across scheduler calls. `sync-http`
remains intentionally unsupported.

The compatibility `run-scenario` primitive submits a raw list, awaits every
action, and returns results in submission order:

```scheme
(run-scenario
  '(("http://journal-1.test/interface" expression-a (2 1) 0)
    ("http://journal-2.test/interface" expression-b () 1)))
```

## Building and running

Use the Journal target directory to reuse its compiled dependencies:

```sh
CARGO_TARGET_DIR=journal/target \
  cargo build --manifest-path records/tests/Cargo.toml

journal/target/debug/records-test \
  --suite records/tests/suite.toml --jobs 13
```

PowerShell uses the same crate and suite:

```powershell
$env:CARGO_TARGET_DIR = "journal/target"
cargo build --manifest-path records/tests/Cargo.toml
.\journal\target\debug\records-test.exe --suite records/tests/suite.toml --jobs 13
```

Run the suite directly through Cargo:

```sh
CARGO_TARGET_DIR=journal/target \
  cargo run --manifest-path records/tests/Cargo.toml -- \
  --suite records/tests/suite.toml --jobs 13
```

For a machine-readable trace, invoke one Interface case directly as shown below
and add `--trace target/interface.jsonl` before its file arguments.

The test procedure attaches exact values or predicates through `:expect` and
may process explicitly awaited results. An uncaught Scheme error causes
`records-test` to exit unsuccessfully.

`interface-harness.scm` is the common executable test procedure. Its first
argument is one flat scenario-set lambda; remaining arguments are the active
Lisp sources. Assertion support is bundled into the harness. A scenario set
receives a source-bound environment constructor:

```scheme
(lambda (make-interface-harness)
  (with-let (make-interface-harness :journals 3 :users '(alice bob))

    (define write
      ((alice journal-1 'set!) '(*state* alice document) "hello"))

    (test-submit write :expect #t)
    (test-submit
      ((alice journal-1 journal-2 'get) '(*state* bob shared))
      :schedule '(2 1) :tick 1 :expect "shared")

    (test-report)))
```

The action's first application selects principal, origin, optional federation
route, and operation. Its second application contains Interface API arguments.
`test-submit` owns test metadata: `:expect`, `:schedule`, and `:tick`.

`test-await` advances the scheduler until the oldest uncollected submitted
action finishes, checks its expectation, and returns its result. Later actions
may complete first and remain buffered. `test-report` awaits everything left
and returns `"Success (N checks)"`; it is also useful after explicit awaits when
nothing remains. Action thunks contain `#<undefined>` before collection and
their actual result afterward.

Run any one scenario set with the same flat command shape:

```sh
journal/target/debug/records-test \
  records/tests/interface/interface-harness.scm \
  records/tests/interface/test-federation.scm \
  records/lisp/root.scm \
  records/lisp/standard.scm \
  records/lisp/log-chain.scm \
  records/lisp/tree.scm \
  records/lisp/ledger.scm \
  records/lisp/authorization.scm \
  records/lisp/interface.scm
```

Current independent sets are:

- `test-basic.scm` — 175 integrated Interface, scheduler, and Ledger workflow checks;
- `test-federation.scm` — 134 Federation protocol checks over real
  deterministic `sync-remote` scheduling, including repeated high-index
  initiator deletion and same-identity re-establishment;
- `test-network.scm` — 121 checks for realistic bridge establishment,
  propagation, overlapping synchronization, idempotent re-establishment,
  crossed-establishment recovery, interface-key rotation, random journal
  identities, identity-salted signing keys, and skipped root-key rotations;
- `test-sharing.scm` — 53 checks for concurrent public/private, direct/multi-hop,
  authorization lifecycle, ancestor directories, Tree-native raw bytes,
  deletion, and recreation;
- `test-history.scm` — 98 checks for direct and multi-hop advancing committed
  views, explicit history at every hop, proof retention, resolve/pin timing,
  destructive retention shrinkage, and pinned-path isolation after widening;
- `test-retained-history.scm` — 19 checks that preserve exact historical routes
  across temporary log-chain pruning and layout boundaries.

As a loose maintenance guideline, each Interface case should represent a robust
workflow comparable in scope to `test-basic.scm` and should ordinarily finish in
about 60 seconds or less on the development host. Prefer extending a related
case over adding small scenario fragments.

Raw adapters remain only where malformed envelopes, signatures, exact proof
objects, or intentionally low-level root inspection are the behavior under test.
