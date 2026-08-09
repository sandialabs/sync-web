import assert from "node:assert/strict";
import { test } from "node:test";
import Fastify from "fastify";
import { instrumentGatewayRequests } from "../src/metrics";

test("request metrics bound unmatched URLs and retain route templates", async (t) => {
  const app = Fastify();
  instrumentGatewayRequests(app);
  app.get("/items/:id", async () => ({ ok: true }));
  await app.ready();
  t.after(async () => app.close());

  for (let index = 0; index < 1000; index += 1) {
    const response = await app.inject({ method: "GET", url: `/missing-${index}` });
    assert.equal(response.statusCode, 404);
  }
  for (const id of ["one", "two"]) {
    const response = await app.inject({ method: "GET", url: `/items/${id}` });
    assert.equal(response.statusCode, 200);
  }

  const metrics = await app.inject({ method: "GET", url: "/metrics" });
  assert.equal(metrics.statusCode, 200);
  assert.match(
    metrics.body,
    /sync_gateway_requests_total\{method="GET",route="unmatched",status_code="404"\} 1000/,
  );
  assert.match(
    metrics.body,
    /sync_gateway_requests_total\{method="GET",route="\/items\/:id",status_code="200"\} 2/,
  );

  const requestSeries = metrics.body
    .split("\n")
    .filter((line) => line.startsWith("sync_gateway_requests_total{"));
  assert.equal(requestSeries.length, 2);
  assert.ok(requestSeries.every((line) => !line.includes("missing-")));
});
