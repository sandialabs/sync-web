---
title: Tools
sidebar:
  label: Tools
  order: 3
head: []
---

This section covers standalone tools that support Synchronic Web development, operation, and analysis without being part of the core journal service path.

## Agent Recorder

`agent-recorder` normalizes AI-agent session artifacts into durable provenance graph records. It is designed as a passive developer tool: it reads existing transcript/session files and writes normalized records, but it does not wrap agent execution or intercept tool calls.

The tool is useful when you want a consistent audit trail across multiple coding agents. Built-in agent adapters currently cover Pi, Codex, Claude Code, Gemini CLI, and OpenCode. Record adapters can write local JSONL, Sync Web records, or OpenTelemetry logs.

### Recording Modes

Use `import` for one-shot backfill of existing artifacts and print normalized records:

```sh
agent-recorder import \
  --agent pi \
  --agent-data tests/fixtures/pi
```

Add `--recorder file --recorder-data records.jsonl` to write a local record file instead of printing.

Use `run` for live passive recording:

```sh
agent-recorder run \
  --agent pi \
  --agent-data ~/.pi/sessions \
  --recorder file \
  --recorder-data records.jsonl
```

When `--recorder` is omitted, `import` and `run` print one normalized JSON record per line. `run` baselines the agent data at startup and only writes entries that appear while it is running. This avoids accidentally importing a user's entire historical session directory when the intended behavior is live monitoring.

### Record Model

Each normalized record is a small graph envelope:

```json
{
  "type": "agent-record",
  "node": {
    "type": "message",
    "id": "msg_...",
    "role": "assistant",
    "content": "Hello",
    "source": {"agent-adapter": "pi", "path": "session.jsonl"}
  },
  "edges": [
    {"type": "follows-message", "target": "msg_previous"}
  ]
}
```

Tool calls and tool results are message annotations. Edges such as `follows-message`, `parent-message`, and `summarizes` capture durable graph relations between normalized nodes.

### Integrity

`agent-recorder` can add optional forward-integrity metadata to each record. For the 0.1.0 tool, treat this as initial tamper-evidence and forward-integrity plumbing for review and experimentation, not as a cryptographically audited production guarantee. The public metadata contains the algorithm, key id, absolute index, payload hash, and authenticator. Local key-evolution state stays private and is not stored in backend records.

The integrity layer authenticates each indexed record independently. Backend storage, including Sync Web, remains responsible for ordering and history. The recorder provides `read --integrity-key`, `verify`, `status`, and `rekey` commands for verification and emergency key cutover workflows.

### Schema

The tool ships a JSON-LD context and Turtle vocabulary for the public record fields. These files define field meanings; they are not a separate export format requirement. Raw JSONL records can be wrapped in an `@graph` with the provided context and loaded into RDF/SPARQL or other graph analysis systems.

```sh
agent-recorder schema --format jsonld
agent-recorder schema --format turtle
```

Detailed command reference, record schema, integrity semantics, and examples live in `tools/agent-recorder/README.md` in the source repository.
