import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { encodePng, decodePng } from '../../scripts/png.mjs';
import { withLook } from '../web/look.mjs';

const root = resolve(import.meta.dir, '../..');
const sprite = encodePng({ width: 3, height: 2, data: Uint8Array.from([
  255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255,
  0, 0, 0, 255, 255, 255, 255, 128, 255, 255, 255, 255,
]) });

test('image rendering inherits and changes live, matching browser sprites at integer and fractional scales', async () => {
  const dir = mkdtempSync(resolve(root, 'target/image-rendering-web-'));
  let browser, server;
  try {
    mkdirSync(resolve(dir, 'assets'));
    writeFileSync(resolve(dir, 'assets/sprite.png'), sprite);
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state mode = "auto"
  action change
    mode = (mode == "auto" ? "crisp-edges" : mode == "crisp-edges" ? "pixelated" : "auto")
  view
    column image-rendering=mode
      button "Change" press=change testId="change"
      image "assets/sprite.png" width=6 height=4 testId="integer"
      image "assets/sprite.png" width=7 height=5 testId="fractional"
      image "assets/sprite.png" width=8 height=3 testId="stretched"
      image "assets/sprite.png" width=7 height=5 image-rendering="auto" testId="own"
`);
    const built = spawnSync(resolve(root, 'target/debug/exact-web-js'), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    const shell = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8'));
    const style = shell.match(/<style>([\s\S]*?)<\/style>/)[1];
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<style>${style} body{background:white}</style><link rel="stylesheet" href="/app.css">
        <div id="exact-root"></div><img id="oracle" src="assets/sprite.png" style="display:block;object-fit:fill">
        <script type="module">import app from '/app.js';app();window.ready=true;</script>`, { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Missing', { status: 404 });
    } });
    browser = await suiteBrowser();
    for (const dpr of [1, 2]) {
      const page = await browser.newPage({ deviceScaleFactor: dpr });
      try {
        await page.goto(server.url.href);
        await page.waitForFunction(() => window.ready && [...document.images].every(i => i.complete && i.naturalWidth));
        const modes = ['auto', 'crisp-edges', 'pixelated', 'auto'];
        for (const mode of modes) {
          await page.waitForFunction(mode => getComputedStyle(document.querySelector('[data-testid="fractional"]')).imageRendering === mode, mode);
          for (const [id, width, height] of [['integer', 6, 4], ['fractional', 7, 5], ['stretched', 8, 3], ['own', 7, 5]]) {
            const expectedMode = id === 'own' ? 'auto' : mode;
            await page.evaluate(({ width, height, mode }) => Object.assign(document.getElementById('oracle').style, { width: `${width}px`, height: `${height}px`, imageRendering: mode }), { width, height, mode: expectedMode });
            const actual = decodePng(await page.locator(`[data-testid="${id}"]`).screenshot());
            const expected = decodePng(await page.locator('#oracle').screenshot());
            expect([...actual.data], `${id} ${mode} @${dpr}`).toEqual([...expected.data]);
          }
          await page.locator('[data-testid="change"]').click();
        }
      } finally { await page.close(); }
    }
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 60000);
