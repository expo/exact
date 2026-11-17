import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { brotliCompressSync, constants } from 'node:zlib';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';
import { builtFile } from './content-names.mjs';

test('a production hello page ships no input chunk and still mounts', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/hello-bytes-'));
  let browser, server;
  try {
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'hello', '--production', '--render', 'none', '--out', dir], { cwd: root, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const scripts = readdirSync(dir).filter(f => f.endsWith('.js'));
    expect(scripts.some(f => f.startsWith('input-glue-'))).toBe(false);
    const size = file => {
      const code = readFileSync(resolve(dir, file));
      return brotliCompressSync(code, { params: { [constants.BROTLI_PARAM_QUALITY]: 11, [constants.BROTLI_PARAM_SIZE_HINT]: code.length } }).length;
    };
    expect(scripts.reduce((total, f) => total + size(f), 0)).toBeLessThan(14 * 1024);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: request => new Response(Bun.file(resolve(dir, new URL(request.url).pathname.slice(1) || 'index.html'))) });
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href);
    await page.waitForFunction(() => globalThis.exact?.ready);
    expect(await page.locator('#exact-root').textContent()).toContain('Hello');
    expect(errors).toEqual([]);
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

test('the production starter stays within its 45 KiB budget and its header button works', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/starter-bytes-'));
  const app = resolve(dir, 'starter'), dev = resolve(dir, 'development'), dist = resolve(dir, 'production');
  let browser, server;
  try {
    const run = (args, env = {}) => {
      const result = spawnSync(process.execPath, args, { cwd: root, env: { ...process.env, ...env }, encoding: 'utf8' });
      expect(result.status, result.stdout + result.stderr).toBe(0);
    };
    run(['scripts/exact.mjs', 'new', app, '--bundle-id', 'com.example.starter']);
    run(['host/web/build.mjs', 'starter-web'], { EXACT_APP_DIR: app, EXACT_WEB_DIST: dev });
    run(['host/web-js/build.mjs', 'starter', '--plan', resolve(dev, 'app.plan'), '--out', dist, '--production'], { EXACT_APP_DIR: app });
    const entry = readFileSync(resolve(dist, builtFile(dist, 'app.js')));
    const bytes = brotliCompressSync(entry, { params: {
      [constants.BROTLI_PARAM_QUALITY]: 11, [constants.BROTLI_PARAM_SIZE_HINT]: entry.length,
    } }).length;
    // scripts/metrics.mjs JS_TARGET_BYTES: the production artifact, compressed as it ships.
    expect(bytes).toBeLessThanOrEqual(45 * 1024);
    expect(entry.toString()).not.toContain('focus window');
    expect(entry.toString()).not.toContain('clipboard');
    expect(entry.toString()).not.toContain('vibrate');
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href + '?agent&epoch=1&seed=3');
    await page.waitForFunction(() => globalThis.exact?.ready);
    expect(await page.evaluate(() => exact.clock.agent)).toBe(false);
    await page.waitForFunction(() => document.querySelector('[data-testid=greeting]')?.textContent === 'Hello from Starter.');
    await page.getByTestId('like').click();
    expect(await page.getByTestId('like').getAttribute('aria-label')).toBe('Unlike');
    await page.getByTestId('like').click();
    expect(await page.getByTestId('like').getAttribute('aria-label')).toBe('Like');
    expect(errors).toEqual([]);
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);
