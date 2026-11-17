import { afterAll, beforeAll, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { engineTests, need } from '../../../scripts/coverage.mjs';
import { webCompiler } from '../../web-js/module.mjs';
import { serveBuildTree, serveStatic } from '../serve.mjs';

const engineTest = await engineTests();

const root = resolve(import.meta.dir, '../../..');
const fields = [
  ['none', 'appearance="none"', false], ['auto', 'appearance="auto"', true], ['default', '', true],
  ['class-none', 'class=Bare', false], ['class-auto', 'class=Native', true],
  ['override-auto', 'class=Bare appearance="auto"', true], ['override-none', 'class=Native appearance="none"', false],
  ['devolved', 'background-color="#ffffff"', false],
];
const source = `routes nav
  home "/" render=build activate=idle

style Bare
  appearance="none"
style Native
  appearance="auto"

component App
  state query = "query"
  state native = false
  action edit(value: string)
    query = value
  action clear
    query = ""
  action toggle
    native = not native
  view
    column padding=20 gap=8
${fields.map(([id, style]) => `      input type="search" ${style} value=query input=edit aria-label="${id}" testId="${id}" width=220 height=32`).join('\n')}
      if native
        input type="search" appearance="auto" value=query input=edit aria-label="changing" testId="changing" width=220 height=32
      else
        input type="search" appearance="none" value=query input=edit aria-label="changing" testId="changing" width=220 height=32
      button press=toggle testId="toggle"
        text "Change appearance"
      button press=clear testId="clear"
        text "Clear"
      text query testId="query"
`;
let dir, jsServer, wasmServer, jsURL, wasmURL;
const wasmDist = process.env.EXACT_WEB_DIST;
beforeAll(async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  dir = mkdtempSync(resolve(root, 'target/search-appearance-test-'));
  // A wasm session accepts only plans for its baked app identity.
  const id = wasmDist ? JSON.parse(readFileSync(resolve(wasmDist, 'exact.json'), 'utf8')).app.id : 'test.search-appearance';
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Search Appearance', app: { id, name: 'Search Appearance' }, host: { web: {} } }));
  writeFileSync(resolve(dir, 'app.contract'), source);
  const dist = resolve(dir, 'dist');
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'search-appearance-test', '--out', dist, '--render', 'js'], {
    cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
  expect(status, stdout + stderr).toBe(0);
  jsServer = createServer((req, res) => serveBuildTree(dist, req, res));
  await new Promise(ok => jsServer.listen(0, '127.0.0.1', ok));
  jsURL = `http://127.0.0.1:${jsServer.address().port}/`;
  if (wasmDist) {
    wasmServer = createServer((req, res) => {
      if (req.url === '/__plan') { res.writeHead(200); res.end(readFileSync(resolve(dist, 'app.plan'))); }
      else serveStatic(resolve(wasmDist), req, res);
    });
    await new Promise(ok => wasmServer.listen(0, '127.0.0.1', ok));
    wasmURL = `http://127.0.0.1:${wasmServer.address().port}/?agent=search-appearance`;
  }
}, 120000);
afterAll(async () => {
  for (const server of [jsServer, wasmServer]) if (server) await new Promise(ok => server.close(ok));
  if (dir) rmSync(dir, { recursive: true, force: true });
});

// Click through the trailing control's hit area: native padding and the cancel icon differ by engine.
async function clears(page, id) {
  const field = page.locator(`[data-testid="${id}"]`);
  await field.scrollIntoViewIfNeeded();
  const box = await field.boundingBox();
  for (let inset = 4; inset <= 36; inset += 4) {
    await field.fill('query');
    await page.mouse.click(box.x + box.width - inset, box.y + box.height / 2);
    if (await field.inputValue() === '') return true;
  }
  return false;
}

for (const engine of ['chromium', 'webkit', 'firefox']) for (const target of ['js', 'wasm', 'document']) {
  const check = target === 'wasm' && !wasmDist ? need('fixture', 'build a wasm web dist and name it with EXACT_WEB_DIST') : engineTest[engine];
  check(`${engine} ${target}: only native search fields keep the browser's clear control`, async () => {
    const browser = await suiteBrowser(engine);
    try {
      const page = await browser.newPage({ javaScriptEnabled: target !== 'document' });
      await page.goto(target === 'wasm' ? wasmURL : jsURL);
      if (target !== 'document') {
        await page.waitForFunction(() => document.getElementById('exact-root')?.dataset.bootMs != null);
        await page.evaluate(() => globalThis.exact.ready);
        if (target === 'wasm') await page.evaluate(async () => exact.reload(new Uint8Array(await (await fetch('/__plan')).arrayBuffer()), true));
      }
      const results = [];
      for (const [id, , native] of fields) results.push([id, await clears(page, id), native && engine !== 'firefox']);
      results.push(['changing: none', await clears(page, 'changing'), false]);
      if (target !== 'document') {
        for (const native of [true, false]) {
          await page.locator('[data-testid="toggle"]').click();
          expect(await page.locator('[data-testid="changing"]').getAttribute('data-native') !== null).toBe(native);
          results.push([`changing: ${native ? 'auto' : 'none'}`, await clears(page, 'changing'), native && engine !== 'firefox']);
        }
        await page.locator('[data-testid="none"]').fill('query');
        await page.locator('[data-testid="clear"]').click();
        expect(await page.locator('[data-testid="none"]').inputValue()).toBe('');
        expect(await page.locator('[data-testid="query"]').textContent()).toBe('');
      }
      expect(results.map(([id, actual]) => [id, actual])).toEqual(results.map(([id, , expected]) => [id, expected]));
    } finally { await browser.close(); }
  }, 60000);
}
