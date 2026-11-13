// LLP 1039 D9: the JS target answers exactViewport().keyboardHeight as the
// browser reports the keys: the VirtualKeyboard rect, else the visual viewport.
import { test, expect } from 'bun:test';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('../../..', import.meta.url)));

async function served(contract, drive, options = {}, init = null) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-keyboard-height-'));
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Keyboard height', app: { id: 'com.example.keyboardheight', name: 'Keyboard height' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), contract);
    const dist = resolve(dir, 'dist');
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'keyboard-height', '--out', dist, '--render', 'js'], {
      cwd: ROOT, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir },
    });
    expect(build.status, build.stderr + build.stdout).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: req => {
      const path = new URL(req.url).pathname;
      if (['/navigation.js', '/grant-admission.js'].includes(path)) return new Response(Bun.file(resolve(dist, '.gen', path.slice(1))));
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage(options);
    if (init) await page.addInitScript(init);
    await page.goto(`http://127.0.0.1:${server.port}/?agent`);
    await page.waitForFunction(() => typeof globalThis.exact?.agentSettled === 'function');
    const resource = () => page.evaluate(async () => (await exact.agentSettled({ op: 'state' })).resources.viewport);
    // Polled from here: a page function's promise is truthy before it settles.
    const until = async (holds, what) => {
      let v;
      for (let i = 0; i < 200; i++) {
        v = await resource();
        if (holds(v)) return v;
        await Bun.sleep(50);
      }
      throw new Error(`${what}: the resource is ${JSON.stringify(v)}`);
    };
    await drive({ page, resource, until, cdp: await page.context().newCDPSession(page) });
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

// Desktop Chrome draws no software keyboard: the visual viewport stands in
// for one, shrinking from the bottom as a phone's keys make it, and then the
// VirtualKeyboard API's rect, which wins where the page overlays the keys.
test('keyboardHeight is what the keyboard covers', () => served([
  'shape Viewport',
  '  keyboardHeight: number',
  'component App',
  '  resource viewport = exactViewport() as shape Viewport',
  '  view',
  '    text `${viewport.keyboardHeight}` testId="keyboard"',
  '',
].join('\n'), async ({ page, resource, until }) => {
  expect((await resource()).keyboardHeight).toBe(0);
  const keys = height => page.evaluate(height => {
    window.keys = height;
    visualViewport.dispatchEvent(new Event('resize'));
  }, height);
  await keys(291);
  await until(v => v.keyboardHeight === 291, 'the visual viewport\'s keyboard');
  await page.waitForFunction(() => document.querySelector('[data-testid="keyboard"]')?.textContent === '291');
  expect(await page.evaluate(async () => (await exact.agentSettled({ op: 'layout' })).env['keyboard-inset-height'])).toBe(291);
  // Scrolled down 40 inside the layout viewport: its bottom is 40 nearer the keys.
  await page.evaluate(() => { window.scrolled = 40; visualViewport.dispatchEvent(new Event('resize')); });
  await until(v => v.keyboardHeight === 251, 'the visual viewport scrolled');
  await page.evaluate(() => { window.scrolled = 0; });
  await keys(0);
  await until(v => v.keyboardHeight === 0, 'the keys gone');
  await page.evaluate(() => { window.keysApi = 336; navigator.virtualKeyboard.dispatchEvent(new Event('geometrychange')); });
  await until(v => v.keyboardHeight === 336, 'the VirtualKeyboard API\'s rect');
}, {}, () => {
  const height = Object.getOwnPropertyDescriptor(VisualViewport.prototype, 'height').get;
  Object.defineProperty(VisualViewport.prototype, 'height', { configurable: true, get() { return height.call(this) - (window.keys ?? 0); } });
  Object.defineProperty(VisualViewport.prototype, 'offsetTop', { configurable: true, get() { return window.scrolled ?? 0; } });
  if (navigator.virtualKeyboard) {
    Object.defineProperty(navigator.virtualKeyboard, 'boundingRect', { configurable: true, get: () => new DOMRect(0, innerHeight - (window.keysApi ?? 0), innerWidth, window.keysApi ?? 0) });
  }
}), 180000);

// Under `interactive-widget=resizes-content`, without the VirtualKeyboard
// API, the keys shrink the page itself: both viewports lose them, nothing is
// over the page, and the honest overlap is 0, its height the shorter one.
test('keyboardHeight under resizes-content without the VirtualKeyboard API is 0', () => served([
  'shape Viewport',
  '  keyboardHeight: number',
  '  height: number',
  'component App',
  '  resource viewport = exactViewport() as shape Viewport',
  '  state name = ""',
  '  action edit(v: string)',
  '    name = v',
  '  view',
  '    column interactive-widget="resizes-content"',
  '      input value=name input=edit aria-label="Name" testId="name"',
  '',
].join('\n'), async ({ page, resource, until }) => {
  const full = await page.evaluate(() => innerHeight);
  await page.focus('[data-testid="name"]');
  await page.evaluate(() => {
    window.keys = 300;
    dispatchEvent(new Event('resize'));
    visualViewport.dispatchEvent(new Event('resize'));
  });
  await until(v => v.height === full - 300, 'the page the keys shrank');
  expect((await resource()).keyboardHeight).toBe(0);
}, {}, () => {
  if (navigator.virtualKeyboard) Object.defineProperty(navigator, 'virtualKeyboard', { configurable: true, value: undefined });
  for (const [proto, name] of [[VisualViewport.prototype, 'height'], [window, 'innerHeight']]) {
    const read = Object.getOwnPropertyDescriptor(proto, name).get;
    Object.defineProperty(proto, name, { configurable: true, get() { return read.call(this) - (window.keys ?? 0); } });
  }
}), 180000);
