# Developing Custom Synchronic Web Functionality

While the [Quickstart](quickstart.md) guide covers deploying prepackaged Synchronic Web assets (like the ledger and ontology Docker Compose networks), this page is for developers who want to build their own custom journals, records, and application logic on top of the Synchronic Web infrastructure.

> **Note:** At each step, we provide command-line examples that mirror the functionality of the prepackaged Docker Compose deployments, so you can see how to achieve the same results manually or in development workflows.
This workflow illustrates the power of Lisp/Scheme afforded to bootstrap into successively complex levels of abstraction using the same minimal and homoiconic syntax.

## The Journal SDK: Managing Data Structures with Lisp

You can start a journal instance directly using the SDK:

```bash
cargo run -- --port 4096 --database dev.db
```

At the heart of the Synchronic Web is the **journal-sdk**, which combines:
- Persistent **immutable cryptographic data structures** for tamper-evident, version-controlled storage ("records").
- An embedded **s7 Lisp/Scheme evaluation environment** for flexible, programmable logic.

You can interact with the journal's evaluator by inputing text at `http://localhost:4096/interface`.
For instance, try the following query:

`(+ 2 2)`

Returns:

`4`

Alternatively, you can also pass in 

```bash
curl -X POST http://localhost:4096/interface --data-binary '(+ 2 2)'
```

## sync-cons/car/cdr: Primitives for working with Cryptographic Data Structures

While an unmodified interface will allow for arbitrary code execution, it executes every request in a clean runtime environment without any persistent state... except for one variable: `*sync-state*`.
Using this construct, you can:
- Modifying the `*sync-state*` variable within the function body to change the behavior of the main record in-place.
- Using `sync-cons`, `sync-car`, and `sync-cdr` to build and traverse arbitrarily complex binary trees within `*sync-state*` variable.

The way it works is as follows: for every request, the journal will read the left side of the state root (`(sync-car *sync-state*)`) and evaluate it by passing in as parameters the existing `*sync-state*` and the body of the `query`.
The right side the state root (`(sync-cdr *sync-state*)`) can contain any arbitrary structure.
By default, all records begin with the following top-level function:

`(lambda (*sync-state* query) (cons (eval query) *sync-state*))`

The expected return for all records is a `cons` pair where the first (`car`) value is the return result and the second (`cdr`) value is the new *sync-state*.
Notice that this function evaluates any arbitrary code--including code that might modify `*sync-state*`

This mechanism enables modification of the `*sync-state*` in a controlled manner.
For instance, running the following query will cause all future invocations of the interface to duplicate the returned result:

```
(begin 
  (set! *sync-state* 
    (sync-cons 
      (expression->byte-vector 
        '(lambda (*sync-state* query) 
          (cons (cons (eval query) (eval query)) *sync-state*)))
      (sync-cdr *sync-state*))) 
  "queries will now run twice")
```

In this way, we can program, in-place, arbitrary complex functionality to handle arbitrarily complex data according to arbitrarily complex queries.
 
## Making State Management Easy: `record.scm`

In practice, it can be cumbersome to manage state and paths manually.
To load the record interface into your journal, you can use:

```bash
curl -X POST http://localhost:4096/interface --data-binary @path/to/record.scm
```

The [`record.scm`](https://github.com/sandialabs/sync-records/blob/main/lisp/record.scm) interface abstracts away much of the complexity of working with Merkle trees and state. It provides:
- Tree-like path navigation and mutation primitives.
- The ability to store arbitrary Lisp expressions—including function bodies—at any path in the record tree.
- Automatic handling of state passing and updates, so you can focus on your application logic.

With `record.scm`, you can store code, data, and even entire APIs as values in the record tree, and retrieve/evaluate them as needed. This enables highly dynamic, evolvable applications.

## Version Control and Peer-to-Peer: `ledger.scm`

To load the ledger interface and enable version control:

```bash
curl -X POST http://localhost:4096/interface --data-binary @path/to/ledger.scm
```

The [`ledger.scm`](https://github.com/sandialabs/sync-records/blob/main/lisp/ledger.scm) module builds on `record.scm` to provide version control, history, and peer-to-peer synchronization. It allows you to:
- Track changes to records over time.
- Synchronize state with other journals in the network.
- Implement access control and authentication for record updates.

## Extending Functionality: `ontology.scm` and Semantic Web

To add semantic web support, load the ontology module:

```bash
curl -X POST http://localhost:4096/interface --data-binary @path/to/ontology.scm
```

The [`ontology.scm`](https://github.com/sandialabs/sync-records/blob/main/lisp/ontology.scm) module is an example of extending the journal with new logic and constraints. It adds support for [RDF triples](https://www.w3.org/TR/rdf11-concepts/) and semantic web operations, enabling journals to:
- Store and query semantic relationships between data.
- Enforce custom constraints and logic on top of the base record/ledger functionality.

## Integrating External Services and Microservices

To run the Explorer service and connect it to your journal:

```bash
python service.py --journal http://localhost:4096/interface --secret <root password>
```

Beyond the core journal and record logic, you can extend your Synchronic Web deployment by integrating external microservices. These services can provide additional APIs, user interfaces, or automation around your journal's data and logic.

For example, the [Explorer service](https://github.com/sandialabs/sync-services/tree/main/services/explorer) is a point-and-click web front-end for browsing and interacting with records and ledgers. You can run it alongside your journal (using Docker Compose or manually) and connect it to your journal's interface endpoint.

Other microservices can be developed to automate workflows, provide analytics, or expose custom APIs. These services typically communicate with the journal via HTTP, using the same interface endpoint and record/ledger APIs described above.

- See the [sync-services](https://github.com/sandialabs/sync-services) repository for more examples and ready-to-use service deployments.
