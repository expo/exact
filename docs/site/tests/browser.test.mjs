import { afterAll, beforeAll, expect, test } from 'bun:test';
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { networkInterfaces } from 'node:os';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { serveBuildTree } from '../../../host/web/serve.mjs';
import { site } from '../constants/site.ts';

const dist = resolve(import.meta.dir, '../dist');
let browser, server, origin;

beforeAll(async () => {
  if (!existsSync(resolve(dist, 'llms.txt'))) throw new Error('run bun run build in docs/site before the browser tests');
  server = createServer((request, response) => serveBuildTree(dist, request, response));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
  browser = await suiteBrowser();
});

afterAll(async () => {
  await browser?.close();
  server?.closeAllConnections();
  await new Promise(resolve => server ? server.close(resolve) : resolve());
});

async function visit(page, path, interactive = true) {
  await page.goto(origin + path);
  if (interactive) await page.waitForSelector('#exact-root[data-boot-ms]');
  await page.evaluate(() => document.fonts.ready);
}

async function withPage(options, run) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, ...options });
  context.setDefaultTimeout(5000);
  try { await run(await context.newPage()); } finally { await context.close(); }
}

async function headingInView(page) {
  await page.waitForFunction(() => {
    const box = document.querySelector('[data-testid="h-a-deep-link"]').getBoundingClientRect();
    return box.top >= 0 && box.bottom <= innerHeight;
  }, null, { timeout: 5000 });
}

test('a table-of-contents link scrolls the article to its heading', () => withPage({}, async page => {
  await visit(page, '/recipes/website');
  const before = await page.getByTestId('h-a-deep-link').boundingBox();
  expect(before.y).toBeGreaterThan(900);
  await page.locator('[data-testid="toc"] a[href="/recipes/website#a-deep-link"]').click();
  await headingInView(page);
  expect(new URL(page.url()).hash).toBe('#a-deep-link');
}));

test('a fragment survives initial page activation', () => withPage({}, async page => {
  await page.clock.install();
  await visit(page, '/recipes/website#a-deep-link');
  await page.clock.runFor(350);
  await headingInView(page);
}));

test('the sidebar can be shown after a full reload with it hidden', () => withPage({}, async page => {
  await visit(page, '/start');
  await page.getByTestId('sidebar-hide').click();
  await page.waitForFunction(() => localStorage.getItem('exact.state.sidebar') === 'false');
  await page.reload();
  await page.waitForSelector('#exact-root[data-boot-ms]');
  await page.getByTestId('sidebar-show').click();
  await page.waitForFunction(() => {
    const sidebar = document.querySelector('[data-testid="sidebar-shown"]');
    return sidebar && getComputedStyle(sidebar).visibility === 'visible' && sidebar.getBoundingClientRect().width > 200;
  }, null, { timeout: 5000 });
  await page.getByTestId('nav-recipeWebsite').click();
  await page.getByTestId('h-a-deep-link').waitFor();
  expect(new URL(page.url()).pathname).toBe('/recipes/website');
}));

for (const interactive of [false, true]) {
  for (const width of [959, 960, 961, 1279, 1280, 1281]) {
    test(`navigation and page actions are visible at ${width}px ${interactive ? 'after activation' : 'before JavaScript'}`, () =>
      withPage({ viewport: { width, height: 900 }, javaScriptEnabled: interactive }, async page => {
        await visit(page, '/recipes/website', interactive);
        const nav = page.getByTestId(width >= 960 ? 'sidebar-shown' : 'menu');
        const actions = page.getByTestId(width >= 1280 ? 'toc' : 'page-actions');
        for (const control of [nav, actions]) {
          const box = await control.boundingBox();
          expect(box?.width).toBeGreaterThan(30);
          expect(box?.height).toBeGreaterThan(20);
          if (interactive) expect(await control.evaluate(e => !!e.closest('[inert]'))).toBe(false);
        }
        if (interactive && width < 960) {
          await nav.click();
          await page.getByTestId('menu-nav-startHere').click();
          expect(new URL(page.url()).pathname).toBe('/start');
        }
      }));
  }
}

test('copying delivers the onboarding prompt and the current article after navigation', () => withPage({}, async page => {
  await page.addInitScript(() => {
    window.clipboardWrites = [];
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: text => { window.clipboardWrites.push(text); return Promise.resolve(); } },
    });
  });
  await visit(page, '/start');
  await page.getByTestId('agent-prompt-copy').click();
  await page.waitForFunction(() => window.clipboardWrites.length === 1);
  const prompt = await page.evaluate(() => window.clipboardWrites[0]);
  expect(prompt).toContain('Exact');
  expect(prompt).toContain('setup --check');
  expect(prompt).toContain('feedback note');
  expect(prompt).toContain('https://github.com/expo/exact/blob/main/docs/start-here.md');
  expect(prompt).not.toContain('docs.exact.example');

  await page.getByTestId('copy-markdown').click();
  await page.waitForFunction(() => window.clipboardWrites.length === 2);
  expect(await page.evaluate(() => window.clipboardWrites[1])).toStartWith('# Start here: building an app with Exact');
  await page.getByTestId('nav-recipeWebsite').click();
  await page.getByTestId('h-a-deep-link').waitFor();
  await page.getByTestId('copy-markdown').click();
  await page.waitForFunction(() => window.clipboardWrites.length === 3);
  const markdown = await page.evaluate(() => window.clipboardWrites[2]);
  expect(markdown).toStartWith('# Recipe: a website');
  expect(markdown).toContain('## A deep link');
  expect(markdown).not.toContain('# Start here: building an app with Exact');
}));

for (const kind of ['table of contents', 'permalink']) {
  test(`${kind} URLs retain their article without JavaScript`, () => withPage({ javaScriptEnabled: false }, async page => {
    await visit(page, '/recipes/website', false);
    const link = kind === 'table of contents'
      ? page.getByTestId('toc').locator('a').filter({ hasText: 'A deep link' })
      : page.getByTestId('h-a-deep-link').locator('..').getByRole('link', { name: 'Link to this section' });
    const href = await link.evaluate(a => a.href);
    expect(new URL(href).pathname).toBe('/recipes/website');
    const opened = await page.context().newPage();
    await opened.goto(href);
    await headingInView(opened);
    expect(new URL(opened.url()).hash).toBe('#a-deep-link');
  }));
}

for (const width of [800, 1440]) {
  test(`the search shortcut works with the sidebar hidden at ${width}px`, () => withPage({ viewport: { width, height: 900 } }, async page => {
    await visit(page, '/start');
    if (width === 1440) await page.getByTestId('sidebar-hide').click();
    await page.keyboard.press('ControlOrMeta+k');
    await page.getByTestId('search-input').waitFor({ state: 'visible' });
    expect(await page.getByTestId('search-input').evaluate(e => e === document.activeElement)).toBe(true);
  }));
}

test('published Markdown feedback follows the app policy', async () => {
  const response = await fetch(origin + '/start', { headers: { Accept: 'text/markdown' } });
  expect(response.headers.get('content-type')).toContain('text/markdown');
  const instructions = (await response.text()).match(/<AgentInstructions>([\s\S]*?)<\/AgentInstructions>/)?.[1] ?? '';
  expect(instructions).toContain('bun exact.mjs feedback status');
  expect(instructions).toContain('bun exact.mjs feedback note');
  expect(instructions).toContain('never');
  expect(instructions).toContain('EXACT_FEEDBACK=0');
  expect(instructions).not.toContain('npx');
  expect(instructions).not.toContain('curl');
});

test('the home page copies the required repository guide', () => withPage({}, async page => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: text => { window.copiedGuide = text; return Promise.resolve(); } },
    });
  });
  await visit(page, '/');
  await page.getByTestId('agent-pill').getByRole('button').click();
  await page.waitForFunction(() => typeof window.copiedGuide === 'string');
  expect(await page.evaluate(() => window.copiedGuide)).toContain('https://github.com/expo/exact/blob/main/docs/start-here.md');
}));

test('the prompt follows every introductory paragraph', () => withPage({}, async page => {
  await visit(page, '/recipes/porting-and-release');
  const last = page.getByText('What ships.', { exact: true });
  await last.waitFor();
  expect(await last.evaluate(e => !!(e.compareDocumentPosition(document.querySelector('[data-testid="agent-prompt-copy"]')) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
}));

for (const path of ['/start?from=https%3A%2F%2Fevil.example%2Flogin', '/recipes/website?from=%2Fstart-here']) {
  test(`the redirect banner rejects an unrelated source on ${path}`, () => withPage({}, async page => {
    await visit(page, path);
    await page.getByRole('heading', { level: 1, name: path.startsWith('/start') ? 'Start here: building an app with Exact' : 'Recipe: a website', exact: true }).waitFor();
    expect(await page.getByTestId('redirected').count()).toBe(0);
  }));
}

test('a page whose module fails to download offers to try again, not a 404', () => withPage({}, async page => {
  await visit(page, '/no-such-page');
  await page.getByTestId('not-found').waitFor();
  // Every page preloads every page's module, so the request fails from the start.
  const chunk = /\/recipeWebsite-[\w-]+\.js$/;
  await page.route(chunk, route => route.abort('internetdisconnected'));
  await visit(page, '/start');
  await page.getByTestId('nav-recipeWebsite').click();
  await page.getByTestId('load-failed').waitFor();
  expect(await page.getByTestId('not-found').count()).toBe(0);
  expect(new URL(page.url()).pathname).toBe('/recipes/website');
  await page.unroute(chunk);
  await page.getByTestId('load-retry').click();
  await page.getByTestId('h-a-deep-link').waitFor();
  expect(await page.getByTestId('load-failed').count()).toBe(0);
}));

test('a known moved page keeps its redirect banner', () => withPage({}, async page => {
  await visit(page, '/start-here');
  await page.getByTestId('redirected').waitFor();
  expect(new URL(page.url()).pathname).toBe('/start');
  expect(await page.getByTestId('redirected').innerText()).toContain('/start-here');
}));

test('Open in AI uses the public page or its repository source', () => withPage({}, async page => {
  await visit(page, '/recipes/website');
  await page.getByTestId('ai-openai').click();
  const href = await page.getByTestId('ai-openai-choices').getByRole('link').getAttribute('href');
  const prompt = new URL(href).searchParams.get('q');
  if (site.origin) expect(prompt).toContain(site.origin + '/recipes/website.md');
  else {
    expect(prompt).toContain('https://github.com/expo/exact/blob/main/docs/recipes/website.md');
  }
  expect(prompt).not.toContain('docs.exact.example');
}));

for (const mode of ['default', '--loopback', '--lan']) {
  test(`start serves the published site on the requested port (${mode})`, async () => {
    const fixture = mkdtempSync(resolve(import.meta.dir, '../../../target/docs-start-'));
    let child;
    try {
      cpSync(resolve(import.meta.dir, '..'), fixture, { recursive: true, preserveTimestamps: true, filter: path => !path.split('/').some(part => ['node_modules', 'target', '.exact'].includes(part)) });
      const reservation = createServer();
      await new Promise(resolve => reservation.listen(0, '0.0.0.0', resolve));
      const port = reservation.address().port;
      await new Promise(resolve => reservation.close(resolve));
      const command = JSON.parse(readFileSync(resolve(fixture, 'package.json'), 'utf8')).scripts.start.split(' ');
      expect(command.shift()).toBe('bun');
      command.push('--port', String(port), ...(mode === 'default' ? [] : [mode]));
      child = spawn(process.execPath, command, { cwd: fixture, env: { ...process.env, PORT: '0', EXACT_LOOPBACK: mode === '--lan' ? '1' : '0', CARGO_BUILD_JOBS: '2' }, stdio: ['ignore', 'pipe', 'pipe'] });
      let output = '';
      const url = await new Promise((resolve, reject) => {
        const limit = setTimeout(() => reject(new Error(`start did not listen: ${output}`)), 30000);
        child.on('exit', code => { clearTimeout(limit); reject(new Error(`start exited ${code}: ${output}`)); });
        child.stderr.on('data', data => { output += data; });
        child.stdout.on('data', data => {
          output += data;
          const match = output.match(/http:\/\/127\.0\.0\.1:\d+/);
          if (match) { clearTimeout(limit); resolve(match[0]); }
        });
      });
      expect((await fetch(url + '/', { headers: { Accept: 'text/html' } })).status).toBe(200);
      const index = await fetch(url + '/llms.txt');
      expect(index.status).toBe(200);
      expect(await index.text()).toContain('# Exact Docs');
      expect((await fetch(url + '/start.md')).headers.get('content-type')).toContain('text/markdown');
      const lan = Object.values(networkInterfaces()).flat().find(a => a && !a.internal && a.family === 'IPv4');
      if (!lan) throw new Error('the LAN binding check needs a non-loopback IPv4 interface');
      const response = await fetch(`http://${lan.address}:${new URL(url).port}/llms.txt`, { signal: AbortSignal.timeout(2000) }).catch(() => null);
      if (mode === '--lan') expect(response?.status).toBe(200);
      else expect(response).toBeNull();
      expect(new URL(url).port).toBe(String(port));
    } finally {
      if (child && child.exitCode === null) {
        const exited = new Promise(resolve => child.once('exit', resolve));
        child.kill('SIGTERM');
        await exited;
      }
      rmSync(fixture, { recursive: true, force: true });
    }
  }, 120000);
}

for (const [from, label, path, title] of [
  ['/recipes/permissions', 'error alert', '/recipes/error-alert', 'Recipe: tell the person something failed'],
  ['/recipes/app-json', 'porting and release', '/recipes/porting-and-release', 'Recipe: porting an app, a release build, splitting the work'],
]) {
  test(`the short recipe label ${label} opens its titled page`, () => withPage({}, async page => {
    await visit(page, from);
    await page.getByRole('link', { name: label, exact: true }).click();
    await page.getByRole('heading', { name: title, level: 1, exact: true }).waitFor();
    expect(new URL(page.url()).pathname).toBe(path);
  }));
}
