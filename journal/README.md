Welcome to the Synchronic Web Journal Software Development Kit!
This repository contains technical documentation and code that operates the core of the synchronic web infrastructure.
For an older higher-level overview, please see public [whitepaper](https://arxiv.org/ftp/arxiv/papers/2301/2301.10733.pdf).

# Install

Please see the following two pages to install the build dependencies:

* [Instructions](https://doc.rust-lang.org/cargo/getting-started/installation.html) to install Rust and Cargo
* [Instructions](https://rust-lang.github.io/rust-bindgen/requirements.html) to install CLang for generating C/Rust bindings

# Run

The recommended usage strategy for most users is the Docker-based compose stack in `deploy/compose/general/`.
Reproducible Linux musl and glibc Wasmer image builds are documented in [IMAGES.md](IMAGES.md).
The journal binary can also be run directly for development and testing:

* As a low-optimization single step for development:
  * `$ cargo run`
* As a high-optimization two-step procedure for release:
  * `$ cargo build --release`
  * `$ ./target/release/journal-sdk`
  
By default, the journal will run with an in-memory database only.
To persist to disk, pass in the `--database <database name>` argument.

If successful, there will be a Journal server running at the configured URL (default: http://localhost:4096)
To interact with the Journal through the web browser:

1. Navigate to `http://localhost:4096`
2. Click on `Interface`
3. Type in `(+ 2 2)` into the input box and press `evaluate`

If the interface correctly outputs the value `4`, then the Journal is functional.

## Commandline

In addition to using the rudimentary browser interface, the HTTP endpoint can also be invoked using any standard commandline client, for instance:

`$ curl -X POST http://localhost:4096/interface -d "(+ 2 2)"`

## Other Actions

Here are other auxiliary actions that may be useful in the course of development:

* Enumerate and display configuration options
  * `$ cargo run -- --help`
* Run all unit and doc comment tests:
  * `$ cargo test`
* Generate Rust documentation
  * `$ cargo doc --no-deps --open`
  
# Use

The `interface` endpoint exposes an evaluator for executing arbitrary code in a Scheme dialect.
Once the service deploys, all interaction with the Journal SDK should take place through this interface.
The evaluator itself is stateless; all variables and computations are cleared between each invocation of the endpoint.
However, the SDK provides a controlled ability to read and write persistent data to the backend database.
By leveraging the functionality and workflow specified below, it is possible to create arbitrarily complex stateful interfaces that benefit from the same core cryptographic verifiability afforded by the Journal.

## Scheme Evaluation

The Scheme dialect used for Synchronic Web code is a lightly modified version of s7 Scheme.
All source code and documentation is available in the [./external/s7](./external/s7) folder.
Other basic modifications include:

- Build flags found in [./build.rs](./build.rs)
- Blacklisted functions found in [./evaluator.rs](./evaluator.rs)
- Additional convenience functions found in [./evaluator.rs](./evaluator.rs)

The Scheme record modules (`root`, `standard`, `tree`, `log-chain`, `ledger`, `federation`, `authorization`, and `interface`) live in `records/lisp/` at the repo root.

Journal SDK 1.4 loads stored objects with exact one-argument `sync-eval`. Shared self-coded computation executes in fresh `sync-let` children cloned from a sealed request-local capability template; trusted host orchestration remains outside that boundary. Active-boundary and dynamic-evaluation helpers are internal rather than Scheme primitives. Staged `call!` orchestration belongs entirely to the Records Interface and is not a Journal primitive or `sync-let` capability. Journal SDK 1.4 does not expose or claim an execution meter or limit.

### Optional Wasmer isolation

Journal requests execute only through the bounded Wasmer evaluator and a provenance-bound, SHA-256-verified AOT kernel. Production builds require the default `wasmer-evaluator` feature and are qualified only for Linux x86_64; an unsupported target, missing feature, missing artifact, or invalid artifact fails closed rather than selecting the native host evaluator. Native s7 remains solely inside the Wasm kernel and narrowly explicit development-test internals. Build, configuration, isolation, test, and qualification details are in [`wasm-kernel/README.md`](wasm-kernel/README.md).

The following commands will be helpful in getting started:

- List all functions in the root environment: `(map car (rootlet))`
- Display the docstring for a given function: `(help my-function-name)`

## Synchronic Web State

Synchronic Web Journals store all stateful information, which we call records, in the form of cryptographic binary trees.
The SDK implements a set of functions in the [./lib.rs](./lib.rs) file for working with records.
These custom functions (and descriptions) are available in the root environment of the evaluator and are identifiable by their `sync-` prefix.
Functionality includes:

- Management of records (binary hash trees)
- Usage of Lisp-inspired constructs (cons, car, cdr, etc.) to build and traverse binary hash trees
- Invocation of other records and generic endpoints

There is only one structural constraint on the form of the record: for a record to be correctly handled by the SDK, the top-most left child must be a unicode bytes string encoding an s7 expression of the following form:

`(lambda (*sync-state* query) (cons ... *sync-state*))`

At request time, the journal supplies the current record root as the `*sync-state*` argument and also exposes `(sync-state)` as a primitive that returns the current session root as a `sync-node`.
The `query` parameter is the expression provided through the `/interface` endpoint.
The function returns a Scheme pair where the first item is the response to the `/interface` call and the second item is the new root node.
For example, the default function is the following:

`(lambda (*sync-state* query) (cons (eval query) *sync-state*))`

This function simply evaluates any user query against the current state binding.
From this highly generic and permissive functionality, it is possible to construct arbitrarily specific and controlled interfaces.
The `records/lisp/` folder provides the production implementation.
