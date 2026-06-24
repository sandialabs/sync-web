# Agent Recorder

`agent-recorder` is a passive normalization tool for AI-agent session artifacts. It reads existing agent transcript/session formats, emits a small durable provenance graph record format, and can write those records to JSONL, Sync Web, or operational telemetry sinks.

The tool is passive-first: it does not wrap agent execution or intercept tool calls. Agent adapters parse agent data; record adapters decide where normalized records are written.

## Quick Start

List supported adapters:

```sh
agent-recorder adapters
```

Import existing artifacts once and print normalized records:

```sh
agent-recorder import \
  --agent pi \
  --agent-data tests/fixtures/pi
```

Write records to a local file:

```sh
agent-recorder import \
  --agent pi \
  --agent-data tests/fixtures/pi \
  --recorder file \
  --recorder-data records.jsonl
```

Run as a live passive recorder:

```sh
agent-recorder run \
  --agent pi \
  --agent-data ~/.pi/sessions \
  --recorder file \
  --recorder-data records.jsonl
```

When `--recorder` is omitted, `import` and `run` print one normalized JSON record per line. `run` first baselines the current agent data and writes only records that appear after startup. Use `import` when you want a one-shot backfill of existing artifacts.

Read indexed records:

```sh
agent-recorder read --recorder file --recorder-data records.jsonl --index 0
agent-recorder read --recorder file --recorder-data records.jsonl --range 0..10
```

## Subcommands

| Command | Purpose |
| --- | --- |
| `run` | Live passive recorder. Baselines agent data at startup, then polls and writes only new records. |
| `import` | One-shot backfill. Parses existing agent data once and writes all normalized records. |
| `read` | Reads indexed records from a backend and prints JSONL. Can verify before printing. |
| `verify` | Verifies selected integrity-bearing records and prints verification statuses. |
| `status` | Reports local integrity state and backend alignment without mutating state. |
| `rekey` | Emergency integrity key cutover at the current backend index. |
| `schema` | Prints the packaged JSON-LD context or Turtle vocabulary, or their paths. |
| `adapters` | Lists built-in agent adapters, record adapters, and record readers. |

## Adapters

Built-in agent adapters currently include:

- `pi`
- `codex`
- `claude` / `claude-code`
- `gemini` / `gemini-cli`
- `opencode`

Built-in record adapters currently include:

- `file` for local flat JSON Lines records
- `sync-web` for Sync Web storage
- `otel` for OpenTelemetry logs export

Readable backends currently include `file` and `sync-web`.

## Configuration

`run` and `import` accept command-line flags and can also read `agent-recorder.toml` from the current directory. Command-line values override config values.

Example:

```toml
[run]
agent = "pi"
agent-data = "tests/fixtures/pi"
recorder = "file"
recorder-data = "records.jsonl"
poll-interval-ms = 2000
```

## Record Shape

The transport format is a flat JSON object. JSONL backends store one record per line.

```json
{
  "type": "agent-record",
  "node": {
    "type": "message",
    "id": "msg_...",
    "timestamp": "2026-01-01T00:00:00Z",
    "role": "assistant",
    "content": "Hello",
    "cwd": "/workspace/project",
    "provider": "openai",
    "model": "gpt-test",
    "source": {
      "agent-adapter": "pi",
      "path": "session.jsonl",
      "locator": "line:3"
    }
  },
  "edges": [
    {"type": "follows-message", "target": "msg_previous"}
  ]
}
```

Top-level fields:

- `type`: currently always `agent-record`.
- `node`: the observed graph node, usually a `message`; `diagnostic` is reserved for recorder/adapter diagnostics.
- `edges`: outgoing graph edges from the node.
- `integrity`: optional public integrity metadata.

Message node fields:

- `id`: stable normalized node id.
- `timestamp`: best available source timestamp, normalized to RFC3339 when possible.
- `role`: `system`, `developer`, `user`, `assistant`, `model`, `tool`, `runtime`, or `unknown`.
- `content`: normalized textual content.
- `cwd`: best available working directory for interpreting relative paths.
- `provider` and `model`: normalized top-level model metadata when available.
- `tool-calls` and `tool-results`: message annotations, not separate graph nodes.
- `source`: source adapter, path, and locator provenance.
- `metadata`: source-specific escape hatch.

Diagnostic nodes share `id`, `timestamp`, `content`, `cwd`, `source`, and `metadata`, plus optional `severity`.

Edge fields:

- `type`: `follows-message`, `parent-message`, `summarizes`, or `inferred-from`.
- `target`: normalized target node id.
- `metadata`: optional edge metadata.

The stable public JSON uses kebab-case where field names are controlled by `agent-recorder`. Records are flat and are not wrapped in a `data` object.

## Integrity

Integrity is optional and backend-independent. In the 0.1.0 tool, treat this as initial tamper-evidence and forward-integrity plumbing for review and experimentation, not as a cryptographically audited production guarantee. Enable it on writes with `--integrity agent-recorder-integrity-v1`, an integrity state path, and an initial key.

```sh
AGENT_RECORDER_INTEGRITY_KEY='example secret' \
agent-recorder import \
  --agent pi \
  --agent-data tests/fixtures/pi \
  --recorder file \
  --recorder-data records.jsonl \
  --integrity agent-recorder-integrity-v1 \
  --integrity-state agent-recorder.integrity.json \
  --integrity-key-env AGENT_RECORDER_INTEGRITY_KEY
```

Public integrity metadata:

```json
{
  "integrity": {
    "algorithm": "agent-recorder-integrity-v1",
    "key-id": "...",
    "index": 0,
    "payload-hash": "...",
    "authenticator": "..."
  }
}
```

The payload hash is SHA-256 over canonical JSON for the whole normalized record with the `integrity` block removed. The authenticator is:

```text
HMAC(K_i,
     "agent-recorder/integrity/authenticator/v1" ||
     u64be(index) ||
     payload_hash)
```

The HMAC layer is not a blockchain and does not include a previous authenticator. Ordering and history are backend responsibilities.

Local integrity state stores private future keys and is not written into records. The key schedule is a no-horizon, 1-based `v2(i)` skip schedule. Backend indexes are zero-based, but key scheduling uses one-based event numbers.

At one-based event `i`, compute `h = v2(i)` and generate future edge keys for levels `0..h`, each targeting `i + 2^d`. At event `j`, consume pending incoming keys whose target is `j`; after the backend write succeeds, consumed keys are deleted and the local state advances. A verifier with the root key derives `K_i` in logarithmic time and verifies the selected indexed record independently. Cryptographic review is still recommended before relying on this for high-assurance audit workflows.

Read with verification:

```sh
agent-recorder read \
  --recorder file \
  --recorder-data records.jsonl \
  --range 0..10 \
  --integrity-key-env AGENT_RECORDER_INTEGRITY_KEY
```

Verify without printing records:

```sh
agent-recorder verify \
  --recorder file \
  --recorder-data records.jsonl \
  --range 0..10 \
  --integrity-key-env AGENT_RECORDER_INTEGRITY_KEY
```

Check backend/state alignment:

```sh
agent-recorder status \
  --recorder file \
  --recorder-data records.jsonl \
  --integrity-state agent-recorder.integrity.json
```

`status` reports `aligned`, `one-step-repairable`, `state-ahead-of-backend`, or `backend-too-far-ahead`. Append and `rekey` can repair the safe one-step case where a backend write succeeded but local state did not advance.

Emergency rekey keeps the absolute backend index, replaces local key state at the current next index, and does not mutate prior backend records:

```sh
agent-recorder rekey \
  --recorder file \
  --recorder-data records.jsonl \
  --integrity-state agent-recorder.integrity.json \
  --integrity-key-env NEW_AGENT_RECORDER_INTEGRITY_KEY
```

## RDF and JSON-LD

The schema files define the public vocabulary for record fields:

- `schema/agent-recorder.context.jsonld`
- `schema/agent-recorder.ttl`

Print them with:

```sh
agent-recorder schema --format jsonld
agent-recorder schema --format turtle
```

Raw JSONL records can be wrapped in a JSON-LD document:

```json
{
  "@context": "schema/agent-recorder.context.jsonld",
  "@graph": [
    {"type": "agent-record", "node": {"type": "message", "id": "msg_..."}}
  ]
}
```

The context models the raw transport shape directly, including `edges[].type` and `edges[].target`. Downstream systems may load the records into RDF, SPARQL, Neo4j, or other analysis systems without changing the recorder format.

A real signed JSON-LD example is checked in at `examples/integrity-agent-record.jsonld`.

## Release Artifacts

The `Agent Recorder Binaries` GitHub Actions workflow builds downloadable `agent-recorder-*` binaries for Linux, Linux musl/Alpine, macOS, and Windows targets. Branch workflow artifacts can be downloaded for testing before a tagged release.

Tagged releases use `agent-recorder-v*` tags, for example `agent-recorder-v0.1.0`. Ledger binary releases use separate `ledger-v*` tags so agent-recorder artifacts do not mix with ledger/journal release assets.

## Sync Web Backend

Sync Web writes store readable JSON bytes at deterministic entry names such as `entry-000000000000`.

Example gateway write:

```sh
agent-recorder import \
  --agent pi \
  --agent-data tests/fixtures/pi \
  --recorder sync-web \
  --recorder-data https://djali.net \
  --sync-web-mode gateway \
  --sync-web-api-key-env AGENT_RECORDER_SYNC_WEB_API_KEY \
  --sync-web-path '*state*/alice/agent-recorder/pi'
```

Use a disposable path/account for experiments and avoid committing private session artifacts or private backend exports.
