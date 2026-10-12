import assert from "node:assert/strict";
import test from "node:test";
import { canonicalKeys, incomingBatch, MESSAGE_TYPE, pollDelay, renderBatch } from "./inbox.mjs";

const id = "223e4567-e89b-42d3-a456-426614174000";
const config = { owner: "agent-1", localJournal: "agent-1" };
const item = { peer: { identity: "admin", journal: "human" }, envelope: {
  version: 1, id, from: "admin@human", to: "agent-1@agent-1",
  createdAt: "2026-01-02T03:04:05.123Z", body: "hello",
} };

test("canonical message entries deduplicate the resumed inbox", () => {
  const keys = canonicalKeys([{ type: "custom_message", customType: MESSAGE_TYPE, details: { keys: [`admin@human/${id}`] } }]);
  assert.equal(incomingBatch([item], config, keys).length, 0);
  assert.equal(incomingBatch([item], config, new Set()).length, 1);
});

test("invalid timestamps, senders, and recipients are rejected", () => {
  for (const changes of [{ createdAt: "2026-01-02T03:04:05.123456Z" }, { from: "other@human" }, { to: "agent-2@agent-2" }]) {
    assert.throws(() => incomingBatch([{ ...item, envelope: { ...item.envelope, ...changes } }], config, new Set()));
  }
});

test("group presentation preserves participants and exact reply reference", () => {
  const envelope = { ...item.envelope, version: 2, conversationId: id,
    participants: ["admin@human", "agent-1@agent-1", "agent-2@agent-2"],
    inReplyTo: { from: "agent-2@agent-2", id } };
  const text = renderBatch(incomingBatch([{ ...item, envelope }], config, new Set()));
  assert.match(text, /Participants: admin@human, agent-1@agent-1, agent-2@agent-2/);
  assert.match(text, /"from":"agent-2@agent-2"/);
});

test("fleet timing defaults and explicit faster overrides", () => {
  const base = { lastActivity: 1000, now: 2000, random: () => 0.5 };
  assert.equal(pollDelay({}, { ...base, idle: false }), 5000);
  assert.equal(pollDelay({}, { ...base, idle: true }), 15000);
  assert.equal(pollDelay({}, { ...base, idle: true, now: 400000 }), 30000);
  assert.equal(pollDelay({ POLL_IDLE_MS: "1000", POLL_JITTER: "0" }, { ...base, idle: true, now: 400000 }), 1000);
  assert.throws(() => pollDelay({ POLL_ACTIVE_MS: "0" }, { ...base, idle: false }));
});
