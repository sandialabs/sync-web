# Agent Recorder Instructions

Purpose: normalize heterogeneous AI-agent artifacts into a small common provenance graph.

Data-shape priorities:
- Prefer normalized common fields over source/agent-specific noise.
- Do not preserve source-local IDs, timestamps, or raw field names in graph targets or top-level metadata unless they have clear cross-agent meaning.
- Use `timestamp` as the only normalized message timestamp. It should be RFC3339/ISO-8601 and represent when the agent/source completed or recorded the message into the transcript: received/recorded time for user input, completed/recorded time for assistant or tool output.
- Treat `timestamp` as ordering/provenance metadata, not durable identity.
- Use `cwd` as the normalized current-working-directory field when the source exposes one. Do not preserve duplicate source fields like `directory`, `project`, or `workspace` in metadata unless they have separate clear meaning.
- Use normalized `provider` and `model` fields when available. Do not keep duplicate source field names such as `providerID` or `modelID` in metadata.
- Keep source-specific leftovers in `metadata` only when they are useful and not already represented by a normalized field.
- Edge targets must be normalized node IDs, e.g. `msg_...`, not source-local IDs.
