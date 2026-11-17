import { beforeAll, afterAll, test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { drivable } from '../../scripts/coverage.mjs';

const rendered = new Map();
beforeAll(async () => {
  const root = resolve(import.meta.dir, '../..');
  const fixed = `
component Fixed
  state draft = ""
  state presses = 0
  action edit(value: string)
    draft = value
  action press
    presses = presses + 1
  view
    column
      list virtualized=true appearance="none" height=120 estimated-item-height=40 testId="fixed"
        button "Start" press=press testId="fixed-start"
        input value=draft change=edit testId="fixed-input"
        text \`\${draft}:\${presses}\` testId="fixed-state"
        for n in range(100) key=n
          text "Row" height=40
        column testId="fixed-trail"
          button "End" testId="fixed-end"
      button "After" testId="after-fixed"
`;
  const source = 'routes nav\n  home "/" render=build\n\n' + readFileSync(resolve(import.meta.dir, 'conformance/list-items.contract'), 'utf8')
    .replace('      list testId="rows"', '      Fixed()\n      list testId="rows"') + fixed;
  for (const renderer of ['js', 'rust']) {
    const dir = mkdtempSync(resolve(root, 'target/list-document-'));
    rendered.set(renderer, { dir, dist: resolve(dir, 'dist') });
    writeFileSync(resolve(dir, 'app.contract'), source);
    if (renderer === 'js') writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'List Document Test', app: { id: 'test.list-document', name: 'List Document Test' }, host: { web: {} } }));
    const app = renderer === 'rust' ? ['caltrain', '--contract', resolve(dir, 'app.contract')] : ['list-document-test'];
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', ...app, '--out', resolve(dir, 'dist'), '--render', renderer], { cwd: root, env: { ...process.env, ...(renderer === 'js' ? { EXACT_APP_DIR: dir } : {}) }, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
  }
}, 1_800_000);
afterAll(() => { for (const { dir } of rendered.values()) rmSync(dir, { recursive: true, force: true }); });

test.each(['js', 'rust'])('%s-rendered list items keep their links through adoption', async renderer => {
  const { dist } = rendered.get(renderer);
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
  } });
  let browser;
  try {
    for (const engine of await drivable(['chromium', 'webkit'])) {
      browser = await suiteBrowser(engine);
      const before = await browser.newPage({ javaScriptEnabled: false });
      await before.goto(server.url.href);
      expect((await before.locator('[data-testid="rows"]').ariaSnapshot()).match(/listitem/g)).toHaveLength(3);
      expect((await before.locator('[data-testid="conditional"]').ariaSnapshot()).match(/listitem/g)).toHaveLength(3);
      expect((await before.locator('[data-testid="role-list"]').ariaSnapshot()).match(/listitem/g)).toHaveLength(1);
      expect(await before.locator('[data-testid="role-fallbacks"]').ariaSnapshot()).not.toContain('listitem');
      expect((await before.locator('[data-testid="earlier-role"]').ariaSnapshot()).match(/listitem/g)).toHaveLength(1);
      expect((await before.locator('[data-testid="directory-role"]').ariaSnapshot()).match(/listitem/g)).toHaveLength(1);
      for (const id of ['dpub-owner', 'graphics-owner', 'comment-owner']) expect(await before.locator(`[data-testid="${id}"] > [data-exact-list-item][role="listitem"]`).count()).toBe(0);
      for (const id of ['dpub-row', 'graphics-row', 'mark-row', 'unknown-owner']) expect(await before.locator(`[data-testid="${id}"] > [data-exact-list-item][role="listitem"]`).count()).toBe(1);
      expect(await before.locator('[data-testid="rows"] a').allTextContents()).toEqual(['Orchard:0', 'Harbor:0', 'Meadow:0']);
      await before.close();
      const page = await browser.newPage();
      await page.addInitScript(() => {
        const observer = new MutationObserver(() => {
          const link = document.querySelector('[data-testid="row-Orchard"]');
          if (link) { globalThis.renderedListLink = link; observer.disconnect(); }
        });
        observer.observe(document, { childList: true, subtree: true });
      });
      await page.goto(server.url.href);
      await page.waitForFunction(() => globalThis.exact?.journal?.some(line => line.endsWith('adopted the document')));
      expect(await page.evaluate(() => renderedListLink === document.querySelector('[data-testid="row-Orchard"]'))).toBe(true);
      expect((await page.locator('[data-testid="rows"]').ariaSnapshot()).match(/listitem/g)).toHaveLength(3);
      await page.locator('[data-testid="row-Orchard"]').click();
      await page.locator('[data-testid="reverse"]').click();
      expect(await page.evaluate(() => ({ same: renderedListLink === document.querySelector('[data-testid="row-Orchard"]'), label: renderedListLink.textContent }))).toEqual({ same: true, label: 'Orchard:1' });
      await browser.close(); browser = null;
    }
  } finally { await browser?.close(); server.stop(true); }
}, 60000);

test('a Rust-rendered gated trail is inert before activation and skipped by Tab', async () => {
  const { dist } = rendered.get('rust');
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
  } });
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage({ javaScriptEnabled: false });
    await page.goto(server.url.href);
    const trail = page.locator('[data-testid="fixed-trail"]');
    expect(await trail.getAttribute('aria-hidden')).toBe('true');
    expect(await trail.getAttribute('inert')).not.toBeNull();
    await page.locator('[data-testid="fixed-input"]').focus();
    await page.keyboard.press('Tab');
    expect(await page.evaluate(() => document.activeElement.dataset.testid)).toBe('after-fixed');
  } finally { await browser.close(); server.stop(true); }
}, 60000);

test.each(['js', 'rust'])('%s-rendered fixed children keep early edits, focus and presses through activation', async renderer => {
  const { dist } = rendered.get(renderer);
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
  } });
  const browser = await suiteBrowser('chromium');
  let activate;
  const gate = new Promise(resolve => { activate = resolve; });
  try {
    const page = await browser.newPage();
    await page.route('**/*.js', async route => { await gate; await route.continue(); });
    await page.goto(server.url.href, { waitUntil: 'commit' });
    const input = page.locator('[data-testid="fixed-input"]'), button = page.locator('[data-testid="fixed-start"]');
    await input.waitFor();
    await page.evaluate(() => {
      globalThis.earlyField = document.querySelector('[data-testid="fixed-input"]');
      globalThis.earlyButton = document.querySelector('[data-testid="fixed-start"]');
    });
    await input.fill('Early edit');
    await button.click();
    await input.focus();
    activate();
    await page.waitForFunction(() => globalThis.exact?.journal?.some(line => line.endsWith('adopted the document')));
    expect(await page.evaluate(() => ({
      field: earlyField === document.querySelector('[data-testid="fixed-input"]'),
      button: earlyButton === document.querySelector('[data-testid="fixed-start"]'),
      value: earlyField.value, focus: document.activeElement === earlyField,
      state: document.querySelector('[data-testid="fixed-state"]').textContent,
    }))).toEqual({ field: true, button: true, value: 'Early edit', focus: true, state: 'Early edit:1' });
  } finally { activate(); await browser.close(); server.stop(true); }
}, 60000);
