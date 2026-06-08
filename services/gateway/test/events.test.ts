import assert from "node:assert/strict";
import { test } from "node:test";
import { GatewayEventBroker, isGatewayEventPath } from "../src/events";

test("GatewayEventBroker publishes change events to subscribers", () => {
  const writes: string[] = [];
  const response = { write: (chunk: string) => { writes.push(chunk); return true; } };
  const broker = new GatewayEventBroker();

  const unsubscribe = broker.subscribe(response as any);
  assert.equal(broker.getSubscriberCount(), 1);
  assert.equal(writes[0], ": connected\n\n");

  const event = broker.publish({ operation: "set!", path: ["*state*", "alice", "note.txt"] });
  assert.equal(event.id, 1);
  assert.equal(event.operation, "set!");
  assert.deepEqual(event.path, ["*state*", "alice", "note.txt"]);
  assert.match(event.time, /^\d{4}-\d{2}-\d{2}T/);
  assert.match(writes[1], /^id: 1\nevent: sync-web-change\ndata: /);
  assert.match(writes[1], /"operation":"set!"/);
  assert.match(writes[1], /"path":\["\*state\*","alice","note\.txt"\]/);

  unsubscribe();
  assert.equal(broker.getSubscriberCount(), 0);
});

test("isGatewayEventPath accepts only string and number path segments", () => {
  assert.equal(isGatewayEventPath(["*state*", "alice", 1]), true);
  assert.equal(isGatewayEventPath(["*state*", null]), false);
  assert.equal(isGatewayEventPath({ path: ["*state*"] }), false);
});
