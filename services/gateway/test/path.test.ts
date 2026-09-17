import assert from "node:assert/strict";
import { test } from "node:test";
import { resolve } from "node:path";
import { isPathWithinDirectory } from "../src/path";

test("directory containment is segment-structural rather than prefix-based", () => {
  const directory = resolve("/srv/auth-ui");
  assert.equal(isPathWithinDirectory(directory, resolve(directory, "index.html")), true);
  assert.equal(isPathWithinDirectory(directory, resolve(directory, "..legitimate-asset")), true);
  assert.equal(isPathWithinDirectory(directory, resolve(directory, "..legitimate", "asset.js")), true);
  assert.equal(isPathWithinDirectory(directory, directory), true);
  assert.equal(isPathWithinDirectory(directory, resolve("/srv/auth-ui-spoof/index.html")), false);
  assert.equal(isPathWithinDirectory(directory, resolve(directory, "../secret")), false);
});
