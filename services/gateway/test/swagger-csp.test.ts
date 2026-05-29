import assert from "node:assert/strict";
import { test } from "node:test";
import { allowHttpSwaggerAssets } from "../src/swagger-csp";

test("Swagger CSP permits HTTP-only docs deployments", () => {
  const csp =
    "default-src 'self'; script-src 'self'; upgrade-insecure-requests;";

  const transformed = allowHttpSwaggerAssets(csp);

  assert.equal(transformed, "default-src 'self'; script-src 'self';");
  assert.ok(!transformed.includes("upgrade-insecure-requests"));
});
