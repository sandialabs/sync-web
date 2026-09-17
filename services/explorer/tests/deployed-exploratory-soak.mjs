import assert from 'node:assert/strict';
import { chromium } from 'playwright';

const required = (name) => {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value.replace(/\/$/, '');
};

const origin = required('EXPLORER_ORIGIN');
const username = required('EXPLORER_USERNAME');
const password = required('EXPLORER_PASSWORD');
const firstPeer = required('EXPLORER_FIRST_PEER');
const soakSeconds = Number(process.env.EXPLORER_SOAK_SECONDS ?? '600');
assert.ok(Number.isFinite(soakSeconds) && soakSeconds >= 600, 'EXPLORER_SOAK_SECONDS must be at least 600');

const documentName = 'exploratory-soak.txt';
const documentPath = ['*state*', username, documentName];
const deniedPath = ['*state*', 'admin', 'exploratory-denied.txt'];
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await context.newPage();
const useRequests = [];
const failedRequests = [];
const expectedAdminDenials = [];
const expectedSelectionDenials = [];
const consoleErrors = [];
const requestFailures = [];
const expectedEventAborts = [];
const expectedReloadAborts = [];
const activeRequests = new Set();
const deliberateReloadRequests = new Set();
const logoResponses = [];
let soakTrackingActive = false;

const isExpectedReloadAbort = (request, pathname, error, reloadRequests = deliberateReloadRequests) =>
  reloadRequests.has(request)
  && pathname === '/api/v1/general/size'
  && error === 'net::ERR_ABORTED';

const boundaryRequest = {};
const postReloadDocumentRequest = {};
const boundaryRequests = new Set([boundaryRequest]);
assert.equal(isExpectedReloadAbort(
  boundaryRequest, '/api/v1/general/size', 'net::ERR_ABORTED', boundaryRequests,
), true);
assert.equal(isExpectedReloadAbort(
  postReloadDocumentRequest, '/api/v1/general/size', 'net::ERR_ABORTED', boundaryRequests,
), false, 'a request starting after the reload boundary was treated as reload-cancelled');

await page.addInitScript(() => {
  window.__syncExploratoryEvents = [];
  const addEventListener = EventSource.prototype.addEventListener;
  EventSource.prototype.addEventListener = function instrument(type, listener, options) {
    if (type !== 'sync-web-change') return addEventListener.call(this, type, listener, options);
    return addEventListener.call(this, type, function record(event) {
      try { window.__syncExploratoryEvents.push(JSON.parse(event.data)); } catch {}
      return listener.call(this, event);
    }, options);
  };
});

page.on('request', (request) => {
  activeRequests.add(request);
  if (!request.url().endsWith('/api/v1/general/use')) return;
  try { useRequests.push(request.postDataJSON()); } catch {}
});
page.on('requestfinished', (request) => {
  activeRequests.delete(request);
  deliberateReloadRequests.delete(request);
});
page.on('requestfailed', (request) => {
  activeRequests.delete(request);
  if (!soakTrackingActive) {
    deliberateReloadRequests.delete(request);
    return;
  }
  const failure = { url: request.url(), error: request.failure()?.errorText ?? 'unknown' };
  const pathname = new URL(failure.url).pathname;
  if (pathname === '/api/v1/events' && failure.error === 'net::ERR_ABORTED') {
    expectedEventAborts.push(failure);
  } else if (isExpectedReloadAbort(request, pathname, failure.error)) {
    deliberateReloadRequests.delete(request);
    expectedReloadAborts.push(failure);
  } else {
    requestFailures.push(failure);
  }
});
page.on('console', (message) => {
  if (message.type() === 'error') consoleErrors.push(message.text());
});
page.on('response', (response) => {
  if (response.url().endsWith('/explorer/logo.png')) {
    logoResponses.push({ url: response.url(), status: response.status() });
  }
  if (!response.url().includes('/api/v1/general/') || response.status() < 400) return;
  const failure = { url: response.url(), status: response.status() };
  if (response.url().endsWith('/api/v1/general/admins') && response.status() === 400) {
    expectedAdminDenials.push(failure);
    return;
  }
  let body;
  try { body = response.request().postDataJSON(); } catch {}
  if (response.url().endsWith('/api/v1/general/use') && response.status() === 400
      && JSON.stringify(body?.path) === JSON.stringify(deniedPath)) {
    expectedSelectionDenials.push(failure);
    return;
  }
  failedRequests.push(failure);
});

const post = async (path, value) => page.evaluate(async ({ path, value }) => {
  const response = await fetch('/api/v1/general/put', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, value: {
      '*type/byte-vector*': Array.from(new TextEncoder().encode(value), (byte) =>
        byte.toString(16).padStart(2, '0')).join(''),
    } }),
  });
  if (!response.ok) throw new Error(`put failed: ${response.status}`);
}, { path, value });

const schemeUse = async (argumentsExpression) => page.evaluate(async (body) => {
  const response = await fetch('/api/v1/general/use', {
    method: 'POST',
    headers: { 'Content-Type': 'application/scheme' },
    body,
  });
  if (!response.ok) throw new Error(`Scheme use failed: ${response.status} ${await response.text()}`);
}, argumentsExpression);

const eventCount = () => page.evaluate(() => window.__syncExploratoryEvents?.length ?? 0);
const exactPathEventCount = (path) => page.evaluate((expectedPath) =>
  (window.__syncExploratoryEvents ?? []).filter((event) =>
    JSON.stringify(event.path) === JSON.stringify(expectedPath)).length, path);
const pathlessUseEventCount = () => page.evaluate(() =>
  (window.__syncExploratoryEvents ?? []).filter((event) =>
    event.operation === 'use!' && event.path === undefined).length);
const exactUseCount = (path) => useRequests.filter((body) =>
  JSON.stringify(body.path) === JSON.stringify(path)).length;
const assertPickerClosed = async () => {
  await page.locator('.peer-picker-shell').waitFor({ state: 'detached' });
};

let soakStartedAt = 0;
try {
  await page.goto(`${origin}/auth/login`, { waitUntil: 'domcontentloaded' });
  await page.locator('input[name="identifier"]').fill(username);
  await page.locator('input[name="password"]').fill(password);
  await Promise.all([
    page.waitForURL((url) => !url.pathname.startsWith('/auth/login')),
    page.getByRole('button', { name: 'Sign in with password' }).click(),
  ]);
  consoleErrors.length = 0;
  await page.goto(`${origin}/explorer/`, { waitUntil: 'domcontentloaded' });
  await page.getByTitle('Open a bridge').waitFor();

  const logo = page.getByRole('img', { name: 'Synchronic Web' });
  await logo.waitFor();
  assert.equal(await logo.getAttribute('src'), '/explorer/logo.png');
  await page.waitForFunction(() => {
    const image = document.querySelector('.toolbar-logo');
    return image instanceof HTMLImageElement && image.complete && image.naturalWidth > 0;
  });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await logo.evaluate((image) => image.complete && image.naturalWidth > 0), true);
  await page.setViewportSize({ width: 1440, height: 900 });

  for (const mode of ['Stage', 'Ledger', 'Access']) {
    await page.getByTitle('Open a bridge').click();
    await page.locator('.peer-picker-shell').waitFor();
    await page.getByRole('button', { name: mode, exact: true }).click();
    await assertPickerClosed();
  }
  await page.getByRole('button', { name: 'Stage', exact: true }).click();
  await page.getByTitle('Open a bridge').click();
  await page.locator('.peer-picker-shell').getByRole('button', { name: firstPeer, exact: true }).click();
  await page.waitForFunction((peer) => Array.from(document.querySelectorAll('.working-route .hop-tag'))
    .map((node) => node.textContent).join(',') === `Self,${peer}`, firstPeer);
  await assertPickerClosed();
  await page.getByTitle('Move back one journal').click();
  await page.waitForFunction(() => Array.from(document.querySelectorAll('.working-route .hop-tag'))
    .map((node) => node.textContent).join(',') === 'Self');
  await assertPickerClosed();
  await page.getByTitle('Open a bridge').click();
  await page.locator('.peer-picker-shell').waitFor();
  await page.evaluate((name) => {
    window.location.hash = `#stage/${encodeURIComponent(name)}/`;
  }, username);
  await assertPickerClosed();

  await post(documentPath, 'seeded content');
  await page.evaluate(({ username, documentName }) => {
    window.location.hash = `#stage/${encodeURIComponent(username)}/${encodeURIComponent(documentName)}`;
  }, { username, documentName });
  await page.getByRole('button', { name: 'Edit', exact: true }).waitFor();
  await page.waitForTimeout(1000);
  soakStartedAt = Date.now();
  soakTrackingActive = true;

  const settledUses = exactUseCount(documentPath);
  const settledDocumentEvents = await exactPathEventCount(documentPath);
  await page.waitForTimeout(10_000);
  assert.equal(exactUseCount(documentPath), settledUses, 'static document recursively issued use requests');
  assert.equal(
    await exactPathEventCount(documentPath),
    settledDocumentEvents,
    'read-only use published a change event',
  );

  const schemePath = `(*state* ${username} ${documentName})`;
  const settledPathlessUses = await pathlessUseEventCount();
  await schemeUse(`((path ${schemePath}) (read-only? #t) (expression? #f))`);
  await page.waitForTimeout(300);
  assert.equal(
    await pathlessUseEventCount(),
    settledPathlessUses,
    'Scheme read-only use published a change event',
  );
  await schemeUse(`((path ${schemePath}) (note "nested (read-only? #t)" ) `
    + `; (read-only? #t)\n (read-only? #f) (expression? #f))`);
  await page.waitForFunction((count) => (window.__syncExploratoryEvents ?? []).filter((event) =>
    event.operation === 'use!' && event.path === undefined).length > count, settledPathlessUses);
  const eventsAfterMutatingScheme = await pathlessUseEventCount();
  await schemeUse(`((path ${schemePath}) (read-only? #t) (read-only? #t) (expression? #f))`);
  await page.waitForFunction((count) => (window.__syncExploratoryEvents ?? []).filter((event) =>
    event.operation === 'use!' && event.path === undefined).length > count,
  eventsAfterMutatingScheme);

  await page.getByRole('button', { name: 'Edit', exact: true }).click();
  const editor = page.getByRole('textbox');
  await editor.fill('unsaved local edit');
  await page.waitForTimeout(15_000);
  assert.equal(await editor.inputValue(), 'unsaved local edit');

  const eventsBeforeMutation = await exactPathEventCount(documentPath);
  await post(documentPath, 'external change hint');
  await page.waitForFunction(({ count, path }) => (window.__syncExploratoryEvents ?? []).filter((event) =>
    JSON.stringify(event.path) === JSON.stringify(path)).length > count,
  { count: eventsBeforeMutation, path: documentPath });
  await page.waitForTimeout(1000);
  assert.equal(await editor.inputValue(), 'unsaved local edit', 'genuine change hint erased unsaved text');
  await editor.fill('saved exact content');
  await page.getByRole('button', { name: 'Save', exact: true }).click();
  await page.getByText('Saved', { exact: true }).waitFor();
  // Freeze request identity at the causal boundary; new-document requests cannot enter this set.
  for (const request of activeRequests) {
    if (new URL(request.url()).pathname === '/api/v1/general/size') {
      deliberateReloadRequests.add(request);
    }
  }
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.getByText('saved exact content', { exact: true }).waitFor();

  assert.equal(
    consoleErrors.every((message) => message === 'Failed to load resource: the server responded with a status of 400 (Bad Request)'),
    true,
    `unexpected console errors before denial check: ${consoleErrors.join(' | ')}`,
  );
  assert.equal(consoleErrors.length <= expectedAdminDenials.length, true);
  await page.evaluate(() => {
    window.location.hash = '#stage/admin/exploratory-denied.txt';
  });
  await page.getByRole('alert').filter({ hasText: 'Unable to load this location' }).waitFor();
  const failedUses = exactUseCount(deniedPath);
  await page.waitForTimeout(10_000);
  assert.equal(exactUseCount(deniedPath), failedUses, 'failed selection repeatedly issued the same use');

  const remaining = soakSeconds * 1000 - (Date.now() - soakStartedAt);
  if (remaining > 0) await page.waitForTimeout(remaining);
  assert.equal(exactUseCount(deniedPath), failedUses, 'failed selection grew during the full soak');

  await page.route('**/auth/.ory/self-service/logout/browser?**', (route) =>
    route.fulfill({ status: 503, contentType: 'application/json', body: '{}' }));
  await page.getByTitle('Open a bridge').click();
  await page.locator('.peer-picker-shell').waitFor();
  await page.getByRole('button', { name: 'Sign out', exact: true }).click();
  await assertPickerClosed();

  assert.equal(logoResponses.some(({ status }) => status === 200), true);
  assert.equal(expectedAdminDenials.length >= 1, true);
  assert.equal(expectedSelectionDenials.length, 1);
  assert.deepEqual(failedRequests, []);
  assert.deepEqual(requestFailures, []);
  const genericDeniedConsole = 'Failed to load resource: the server responded with a status of 400 (Bad Request)';
  const explicitDeniedConsole = 'Gateway request failed:';
  assert.equal(consoleErrors.every((message) =>
    message === genericDeniedConsole || message.startsWith(explicitDeniedConsole)), true,
  `unexpected console errors: ${consoleErrors.join(' | ')}`);
  assert.equal(
    consoleErrors.length <= expectedAdminDenials.length + (2 * expectedSelectionDenials.length),
    true,
    `too many expected-denial console records: ${consoleErrors.join(' | ')}`,
  );
  console.log(JSON.stringify({
    soakSeconds,
    documentPath,
    useRequests: useRequests.length,
    documentUses: exactUseCount(documentPath),
    deniedUses: exactUseCount(deniedPath),
    events: await eventCount(),
    logoResponses,
    expectedAdminDenials,
    expectedSelectionDenials,
    expectedEventAborts,
    expectedReloadAborts,
    consoleErrors,
  }));
} finally {
  await context.close();
  await browser.close();
}
