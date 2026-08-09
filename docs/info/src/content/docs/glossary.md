---
title: Glossary
sidebar:
  label: Glossary
  order: 6
head: []
---
Use this glossary as a quick reference for recurring terms used across usage, operation, development, and research documentation.

## Terms

- **Synchronic Web**: A web of independently committed, cryptographically linked journal histories and reciprocal cross-journal observations.
- **Journal**: A runtime that evaluates record logic and persists one locally ordered cryptographic history.
- **Record**: A self-describing sync-node object containing behavior and durable state.
- **Service**: A process that adapts journal functionality for HTTP APIs, browsers, filesystems, identity, or other clients.
- **Index**: An integer selecting an entry in one journal or nested bridged journal history; `-1` means the latest entry relative to that history.
- **Path**: A flat ordered list of indexes, namespace markers, bridge aliases, and data segments used to address staged or committed objects.
- **Reciprocal bridge**: A root-signed relationship in which one designated initiator exchanges both journals' signed heads. A bridge establishes identity and proof reachability but grants no application access.
- **Working route**: The current `Self`-relative bridge-alias route used for live federated scalar/dedicated-batch Stage access and as the journal-name skeleton for Ledger resolution.
- **Historical cursor**: One independently selected Ledger index for `Self` and each hop in a working route; used only by federated `resolve`.
- **Terminal journal**: The final journal selected by a working route. It receives the signed application invocation directly and applies its own local authorization policy.
- **Pin**: Origin-local retention of selected proof material in `Self`'s permanent chain; it is not a request to change terminal-journal retention.
