# Architecture

`journal-cli` is one invoked process with one public executable and one configuration boundary. Internal modules separate transport and workflow concerns so consolidation does not collapse unrelated state machines into one file.

## Boundaries

- `journal_cli.client` owns bounded `/interface` transport, current operation admission, authentication framing, mutation-outcome classification, redirect/proxy refusal, and redaction.
- `journal_cli.message` owns one-shot Message envelope construction, configured recipient resolution, mailbox paths, group fan-out outcomes, and exact reads. It does not poll or acknowledge an inbox.
- `journal_cli.peer` owns capability cards, relationship diagnostics, plan/apply enrollment, bridge and grant operations, exact bridge deletion, and atomic recipient-route replacement.
- `journal_cli.profile` preserves profile parsing, validation, publication, and committed observation.
- `journal_cli.source` preserves Source Publication's low-level contract and high-level receipts/materialization/recovery. The low-level adapter is an in-process module rather than a second executable.
- `journal_cli.flow` preserves Source producer/reviewer orchestration and Git/process isolation.

The resident Pi inbox service remains separately installed and managed. Synchronic Messenger remains a separate browser application. The Ledger remains a separately running process and owns its database.

## Outcomes

Top-level Journal and Message operations use `journal-cli-outcome-v1`. Exit codes are:

| Status | Meaning |
|---|---|
| `0` | Read completed or mutation was explicitly accepted |
| `2` | Request was rejected before dispatch or unsupported locally |
| `3` | Remote operation explicitly rejected or a conditional mutation returned false |
| `4` | Dispatch may have occurred but completion was not established |

Source and flow commands preserve their existing versioned receipt and outcome schemas. No code automatically retries status `4`.

## Configuration and credentials

The CLI accepts an installed fixed-agent configuration or the standalone schema in the README. Credentials come only from an owner-only regular file. HTTP is restricted to literal loopback; remote endpoints require HTTPS. The transport does not use environment proxies or redirects.

## Version boundary

Live request construction is Sync Web 1.6-only. Historical fixtures may mention pre-1.6 operations to prove rejection or preserve imported regression evidence, but production modules do not emit those operations or grant fields.
