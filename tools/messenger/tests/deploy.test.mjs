import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const nginx = await readFile(new URL("../deploy/nginx.conf.template", import.meta.url), "utf8");
const serviceWorker = await readFile(new URL("../public/sw.js", import.meta.url), "utf8");
const styles = await readFile(new URL("../public/styles.css", import.meta.url), "utf8");
const entrypoint = await readFile(new URL("../deploy/entrypoint.sh", import.meta.url), "utf8");
const compose = await readFile(new URL("../compose.yaml", import.meta.url), "utf8");
const app = await readFile(new URL("../public/app.js", import.meta.url), "utf8");
const index = await readFile(new URL("../public/index.html", import.meta.url), "utf8");
const contactProfile = await readFile(new URL("../public/contact-profile.mjs", import.meta.url), "utf8");
const defaultContacts = JSON.parse(await readFile(new URL("../public/contacts.json", import.meta.url), "utf8"));
const exampleContacts = JSON.parse(await readFile(new URL("../examples/contacts.example.json", import.meta.url), "utf8"));

test("the public registry is inert and the example remains explicit", () => {
  assert.deepEqual(defaultContacts, { version: 1, contacts: [] });
  assert.equal(exampleContacts.version, 1);
  assert.equal(exampleContacts.contacts.length, 1);
  assert.equal(exampleContacts.contacts[0].contactId, "example-agent");
  assert.doesNotMatch(index, /Galactica|placeholder="rocky/);
});

test("ES module files are served with a JavaScript MIME type", () => {
  assert.match(nginx, /location ~ \\.mjs\$ \{\s*default_type application\/javascript;/);
});

test("the shell cache generation advances with explicit reply context", () => {
  assert.match(serviceWorker, /sync-messenger-shell-v31/);
  assert.match(serviceWorker, /profile\.mjs/);
  assert.match(serviceWorker, /contact-profile\.mjs/);
});

test("author styles cannot override the hidden attribute", () => {
  assert.match(styles, /\[hidden\] \{ display: none !important; \}/);
});

test("long conversations scroll inside a bounded pane with a pinned composer", () => {
  assert.match(styles, /\.workspace \{[^}]*min-height: 0;[^}]*overflow: hidden;/);
  assert.match(styles, /\.conversation \{[^}]*min-height: 0;[^}]*overflow: hidden;[^}]*grid-template-rows: 76px minmax\(0, 1fr\) auto;/);
  assert.match(styles, /\.message-list \{ min-height: 0; overflow-y: auto;/);
  assert.match(styles, /\.composer \{ position: sticky; z-index: 2; bottom: 0;/);
});

test("Messenger binds loopback and uses only forwarded Kratos cookies", () => {
  assert.match(nginx, /listen 127\.0\.0\.1:\$\{MESSENGER_PORT\};/);
  assert.doesNotMatch(nginx, /auth_basic|MESSENGER_AUTH_HEADER/);
  assert.match(nginx, /location = \/auth\/\.ory\/sessions\/whoami/);
  assert.match(nginx, /proxy_set_header Authorization "";/);
  assert.match(nginx, /location \/api\/ \{ return 404; \}/);
  assert.doesNotMatch(entrypoint, /API_TOKEN|ACCESS_PASSWORD|htpasswd|MESSENGER_USERNAME|MESSENGER_JOURNAL/);
  assert.doesNotMatch(compose, /API_TOKEN|ACCESS_PASSWORD|ACCESS_USERNAME|MESSENGER_USERNAME|MESSENGER_JOURNAL/);
});

test("contact profiles use an injected current-only adapter independent of transport", () => {
  assert.match(app, /profileProvenanceLabel\(contactEndpoint\(contact\), observed\.current\)/);
  assert.match(app, /Stale after failed refresh/);
  assert.match(app, /createMemoryContactProfiles/);
  assert.match(app, /createContactProfileViewCache/);
  assert.match(app, /state\.contactProfileViews\.refresh\(contact\)/);
  assert.match(app, /state\.contactProfileViews\?\.read\(contact\)/);
  assert.match(contactProfile, /const stillCurrent = \(contact, fingerprint, generation\)/);
  assert.match(contactProfile, /if \(!stillCurrent\(contact, fingerprint, generation\)\) return \{ discarded: true \}/);
  assert.doesNotMatch(app, /contactProfileSnapshots|PROFILE_STORAGE_SUFFIX|navigator\.locks/);
  assert.doesNotMatch(`${app}\n${index}`, /contact\.identityIdBase64|Bind profile|bindContactProfile|contact-identity-id|contact-profile-epoch/i);
  assert.match(app, /contact-principal/);
  assert.doesNotMatch(contactProfile, /incomingPrincipal:\s*\[\.\.\.contact\.incomingPrincipal\]|responsePrincipal|sourcePrincipal/);
});

test("route-canonical contacts strip obsolete identity pins before enabling profiles", () => {
  assert.match(app, /migrateRouteCanonicalRegistry/);
  assert.match(app, /if \(state\.profileMigration\.ready\)/);
  assert.match(app, /refresh\.disabled = !state\.profileMigration\.ready/);
  assert.match(app, /profiles remain unavailable and messaging continues/);
  assert.doesNotMatch(app, /window\.prompt/);
  assert.doesNotMatch(app, /identityIdBase64.*public-key|public-key.*identityIdBase64/);
});

test("reply context is explicit, cancelable, and locally resolved", () => {
  assert.match(index, /id="reply-preview"/);
  assert.match(index, /id="cancel-reply"/);
  assert.match(app, /function chooseReply\(message\)/);
  assert.match(app, /findReplyParent\(projected, message\)/);
  assert.match(app, /Referenced message unavailable/);
  assert.match(app, /replyReferenceFor\(parent, Boolean\(group\)\)/);
  assert.doesNotMatch(app, /messagesFor\(selected\.id\)\.at\(-1\)/);
  assert.match(styles, /\.composer-reply/);
  assert.match(styles, /\.reply-context\.unavailable/);
});

test("workspace tabs drive readable main-pane details", () => {
  assert.match(index, /id="detail-view"/);
  assert.match(app, /function renderMainPane\(\)/);
  assert.match(app, /renderContactDetail\(\)/);
  assert.match(app, /renderGroupDetail\(\)/);
  assert.match(app, /renderLocalProfileDetail\(\)/);
  assert.match(app, /No profile document exists yet/);
  assert.match(app, /Create profile revision 1/);
});

test("profile save toast distinguishes predispatch rejection from ambiguous refresh", () => {
  assert.match(app, /Profile save was not dispatched/);
  assert.match(app, /current state was refreshed before another edit/);
  assert.match(app, /api\.isProfileEditBaseConsumed\(attemptedBase\)/);
});

test("the Gateway proxy exposes only Messenger operations", () => {
  assert.match(nginx, /location = \/api\/v1\/general\/info/);
  assert.match(nginx, /general\/\(use\|put\|authorizations\|authorize\|deauthorize\)/);
  assert.doesNotMatch(nginx, /general\/call|general\/bridge|general\/update-config/);
});

test("browser origin is constrained to localhost for shared Kratos cookies", () => {
  assert.match(entrypoint, /\^https\?:\/\/localhost/);
  assert.match(nginx, /return 308 http:\/\/localhost:\$\{MESSENGER_PORT\}/);
});
