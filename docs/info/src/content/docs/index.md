---
title: Overview
sidebar:
  label: Overview
  order: 1
head: []
---
The Synchronic Web is a shared state layer for open-world, partially-trusted networks.
It combines cryptographic data structures, programmable semantics, and distributed synchronization so participants can reason about state across time and space.

## Contents

Each section builds on the previous one:

1. [Usage](usage): how users interact with journals and services.
2. [Operation](operation): how operators deploy, run, and maintain the system.
3. [Development](development): how developers extend runtime behavior and class logic.
4. [Research](research): research directions motivated by the current architecture.

If you are new to the stack, read this page, then move through the sections in order.
If you already operate or build on the stack, you can jump directly to the section aligned with your role.

## Purpose

The core value proposition is verifiable shared state in adversarial or partially-trusted environments.
Traditional APIs can serve data quickly, but they often lack strong guarantees about temporal ordering, provenance, and cross-node consistency.
The Synchronic Web addresses that by treating state as a cryptographically linked structure with programmable rules for mutation and synchronization.

### Dynamic Object Structures

The stack uses a minimal language and object protocol to encapsulate both data and behavior over time.
Rather than hard-coding every interface in Rust, the runtime allows controlled Lisp-level composition of classes (`standard`, `tree`, `chain`, `ledger`) and query/step handlers.
This makes higher-level behavior evolvable without replacing the runtime substrate.
In practice, this means teams can iterate on application semantics at the Lisp layer while retaining a stable underlying execution and persistence runtime.

### Dynamic Peer Topologies

The distributed model is usage-driven: peer relationships can be introduced and updated at runtime.
Nodes can synchronize proofs and selectively resolve remote state while preserving cryptographic verifiability.
This gives a practical notion of state across space, not just within one process or one database.
This topology flexibility is useful in real systems where trust boundaries and communication patterns change over time.

## Resources

This section links to the primary repositories and live endpoints that developers and operators use most often.

### Source Repository

All components live in [sandialabs/sync-web](https://github.com/sandialabs/sync-web):

| Directory | Description |
|---|---|
| `journal/` | Rust runtime, HTTP interface, evaluator extensions, persistence |
| `records/` | Lisp class logic (`root`, `standard`, `tree`, `chain`, `ledger`, `interface`) |
| `services/` | Web services (`gateway`, `explorer`, `workbench`, `file-system`, `router`) |
| `deploy/` | Docker Compose deployment for a single-node stack |
| `tests/` | Smoke, load, and multi-node network tests |
| `docs/` | This documentation site |

The project is open source under the MIT license with Sandia National Laboratories authorship and stewardship.

### Live Nodes

- [The Beagle](http://beagle.sync-web.org/explorer): flagship general-purpose synchronic web node accessible from the open web

Use the operation and usage documentation before depending on live infrastructure for production workflows.
