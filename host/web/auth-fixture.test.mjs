// The authored Cancel releases the real mutation, so closing a popup never strands sign-in.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { createServer } from 'node:http';
import { rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { serveBuildTree } from './serve.mjs';

const dist = resolve(import.meta.dir, '../../target/auth-fixture-web-test');
const issuer = 'http://127.0.0.1:4331';
let server, fixture, origin;
beforeAll(async () => {
  const build = Bun.spawn(['bun', 'host/web/build.mjs', 'auth-fixture', '--out', dist, '--render', 'none'], {
    stdout: 'inherit', stderr: 'inherit',
  });
  expect(await build.exited).toBe(0);
  if (!await fetch(`${issuer}/fixture/log`).then(r => r.ok, () => false)) {
    fixture = Bun.spawn(['bun', 'apps/auth-fixture/fixture.mjs'], { stdout: 'ignore', stderr: 'inherit' });
    for (const end = Date.now() + 10000; Date.now() < end; await Bun.sleep(50)) {
      if (await fetch(`${issuer}/fixture/log`).then(r => r.ok, () => false)) break;
    }
  }
  server = createServer((req, res) => serveBuildTree(dist, req, res));
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  origin = `http://127.0.0.1:${server.address().port}`;
}, 120000);
afterAll(async () => {
  server?.close();
  if (fixture) { fixture.kill(); await fixture.exited; }
  rmSync(dist, { recursive: true, force: true });
});

test('chromium: close the auth popup, Cancel, then sign in successfully', async () => {
  await fetch(`${issuer}/fixture/reset`);
  const browser = await suiteBrowser();
  const page = await browser.newPage();
  page.setDefaultTimeout(10000);
  const node = id => page.locator(`[data-testid="${id}"]`);
  try {
    await page.goto(origin);
    await node('session').waitFor();
    await page.waitForFunction(() => globalThis.exact?.ready);
    await node('prepare').click();
    await page.waitForFunction(() => document.querySelector('[data-testid=prepared]')?.textContent === 'Ready to sign in');
    const start = async () => {
      const opened = page.waitForEvent('popup');
      await node('sign-in').click();
      const popup = await opened;
      await popup.locator('#approve').waitFor();
      return popup;
    };
    const closed = await start();
    await closed.close();
    expect(await node('sign-in').isDisabled()).toBe(true);
    await node('cancel-sign-in').waitFor({ timeout: 2000 });
    expect(await node('cancel-sign-in').isEnabled()).toBe(true);
    await node('cancel-sign-in').click();
    await page.waitForFunction(() => !document.querySelector('[data-testid=sign-in]').disabled);
    expect(await node('prepare').isEnabled()).toBe(true);
    // Retry at once, with no new preparation or ten-minute wait.
    const retry = await start();
    await retry.locator('#approve').click();
    await page.waitForFunction(() => document.querySelector('[data-testid=session]')?.textContent === 'Signed in as fixture.test');
    expect(await node('session').textContent()).toBe('Signed in as fixture.test');
    expect(await node('sign-in').isEnabled()).toBe(true);
    expect(await node('cancel-sign-in').count()).toBe(0);
    const log = await fetch(`${issuer}/fixture/log`).then(r => r.json());
    expect(log.par).toBe(1);
    expect(log.token).toBe(1);
    expect(log.pkce).toBe(1);
  } finally { await browser.close(); }
}, 30000);
