import assert from 'node:assert/strict';
import { chromium } from 'playwright';

const required = (name) => {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value.replace(/\/$/, '');
};

const origin = required('EXPLORER_ORIGIN');
const providerOrigin = required('EXPLORER_PROVIDER_ORIGIN');
const username = required('EXPLORER_USERNAME');
const password = required('EXPLORER_PASSWORD');
const firstPeer = required('EXPLORER_FIRST_PEER');
const secondPeer = required('EXPLORER_SECOND_PEER');
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext();
const page = await context.newPage();
const bridgeRequests = [];
const expectedAdminDenials = [];
const failedRequests = [];

page.on('request', (request) => {
  if (!request.url().endsWith('/api/v1/general/use')) return;
  let body;
  try { body = request.postDataJSON(); } catch { return; }
  if (JSON.stringify(body.path) === JSON.stringify(['*bridge*'])) bridgeRequests.push(body);
});
page.on('response', (response) => {
  if (!response.url().includes('/api/v1/general/') || response.status() < 400) return;
  const failure = { url: response.url(), status: response.status() };
  if (response.url().endsWith('/api/v1/general/admins') && response.status() === 400) {
    expectedAdminDenials.push(failure);
  } else {
    failedRequests.push(failure);
  }
});

try {
  const [requesterSize, providerSize] = await Promise.all([
    fetch(`${origin}/api/v1/general/size`).then((response) => response.json()),
    fetch(`${providerOrigin}/api/v1/general/size`).then((response) => response.json()),
  ]);
  assert.notEqual(
    requesterSize - 1,
    providerSize - 1,
    'The deployed gate requires divergent requester/provider indexes',
  );

  await page.goto(`${origin}/auth/login`, { waitUntil: 'domcontentloaded' });
  await page.locator('input[name="identifier"]').fill(username);
  await page.locator('input[name="password"]').fill(password);
  await Promise.all([
    page.waitForURL((url) => !url.pathname.startsWith('/auth/login')),
    page.getByRole('button', { name: 'Sign in with password' }).click(),
  ]);
  await page.goto(`${origin}/explorer/`, { waitUntil: 'domcontentloaded' });
  await page.getByTitle('Open a bridge').waitFor();

  await page.getByTitle('Open a bridge').click();
  await page.getByRole('button', { name: firstPeer, exact: true }).click();
  await page.waitForFunction((peer) => Array.from(document.querySelectorAll('.working-route .hop-tag'))
    .map((node) => node.textContent).join(',') === `Self,${peer}`, firstPeer);
  await page.getByTitle('Open a bridge').click();
  await page.getByRole('button', { name: secondPeer, exact: true }).click();
  await page.waitForFunction(({ first, second }) => Array.from(
    document.querySelectorAll('.working-route .hop-tag'),
  ).map((node) => node.textContent).join(',') === `Self,${first},${second}`,
  { first: firstPeer, second: secondPeer });

  await page.getByRole('button', { name: 'Ledger', exact: true }).click();
  await page.waitForFunction(({ first, second }) => Array.from(
    document.querySelectorAll('.route-builder .hop-tag'),
  ).map((node) => node.textContent).join(',') === `Self,${first},${second}`,
  { first: firstPeer, second: secondPeer });
  assert.equal(await page.locator('.route-builder').count(), 1);
  const initialFirstSnapshot = await page.getByRole('textbox', {
    name: `${firstPeer} snapshot`, exact: true,
  }).inputValue();
  const initialSecondSnapshot = await page.getByRole('textbox', {
    name: `${secondPeer} snapshot`, exact: true,
  }).inputValue();
  assert.match(initialFirstSnapshot, /^\d+$/);
  assert.match(initialSecondSnapshot, /^\d+$/);
  await page.getByRole('textbox', { name: `${firstPeer} snapshot`, exact: true }).fill('-2');
  await page.getByRole('textbox', { name: `${firstPeer} snapshot`, exact: true }).press('Enter');
  await page.waitForFunction((peer) => window.location.hash.includes(`/bridge/${peer}/-2/`), firstPeer);
  assert.equal(await page.getByRole('textbox', {
    name: `${secondPeer} snapshot`, exact: true,
  }).inputValue(), initialSecondSnapshot);
  await page.locator('.route-builder .route-hop-button', { hasText: firstPeer }).click();
  await page.waitForFunction(({ first, second }) =>
    window.location.hash.includes(`/bridge/${first}/-2/state/`)
      && !window.location.hash.includes(`/bridge/${second}/`),
  { first: firstPeer, second: secondPeer });
  assert.equal(await page.getByRole('textbox', {
    name: `${firstPeer} snapshot`, exact: true,
  }).inputValue(), '-2');
  assert.equal(await page.getByRole('textbox', {
    name: `${secondPeer} snapshot`, exact: true,
  }).count(), 0);

  assert.equal(bridgeRequests.length >= 2, true);
  assert.equal(bridgeRequests.every((body) => body['read-only?'] === true), true);
  assert.equal(bridgeRequests.some((body) =>
    body.$federation?.route?.join(',') === firstPeer), true);
  assert.equal(expectedAdminDenials.length >= 1, true);
  assert.deepEqual(failedRequests, []);
  console.log(JSON.stringify({
    route: ['Self', firstPeer, secondPeer], requesterSize, providerSize,
    stableSnapshots: { first: initialFirstSnapshot, second: initialSecondSnapshot },
    bridgeRequests, expectedAdminDenials,
  }));
} finally {
  await context.close();
  await browser.close();
}
