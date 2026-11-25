// Build exact-web-js before running this test (docs/testing.md).
import { test as bunTest, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { chromium, suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { decodePng } from '../../scripts/png.mjs';
import { EXE } from '../../scripts/paths.mjs';

const test = need('chrome', chromium().unavailable, { test: bunTest });

test('per-axis scale draws, hits and overflows like Chrome, including motion and press feedback', async () => {
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(root, 'target/per-axis-scale-browser-'));
  const sheet = readFileSync(resolve(root, 'host/web/index.html'), 'utf8').match(/<style>[\s\S]*?<\/style>/)[0];
  let server, browser;
  try {
    const built = spawnSync(resolve(root, `target/debug/exact-web-js${EXE}`), [
      'js', resolve(root, 'runner/tests/fixtures/per-axis-scale.contract'), '-o', dir,
    ], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`${sheet}<link rel="stylesheet" href="/app.css"><div id="exact-root"></div>
        <script type="module">import app from '/app.js';app();</script>`, { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage({ viewport: { width: 800, height: 700 }, deviceScaleFactor: 1 });
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.goto(server.url.href);
    await page.getByTestId('pressed').waitFor({ state: 'attached' });
    // Compare the compiled rows to a second tree using the author's CSS directly.
    await page.evaluate(() => {
      const port = document.querySelector('[data-testid="port"]');
      [port, ...port.querySelectorAll('*')].forEach((e, i) => e.dataset.probe = String(i));
      const oracle = port.cloneNode(true);
      oracle.dataset.testid = 'oracle';
      oracle.querySelector('[data-testid="squash"]').style.scale = '1 0.5';
      port.style.cssText += ';position:absolute;left:20px;top:60px;z-index:10';
      oracle.style.cssText += ';position:absolute;left:220px;top:60px;z-index:10';
      port.after(oracle);
    });
    const compare = async () => {
      const result = await page.evaluate(() => {
        const ports = ['port', 'oracle'].map(id => document.querySelector(`[data-testid="${id}"]`));
        return ports.map(p => {
          const r = p.getBoundingClientRect(), box = p.querySelector('[data-testid="squash"]').getBoundingClientRect();
          const hits = [];
          for (let y = 5; y < 120; y += 10) for (let x = 5; x < 120; x += 10)
            hits.push(document.elementFromPoint(r.x + x, r.y + y)?.dataset.probe ?? null);
          return { extent: [p.scrollWidth, p.scrollHeight], box: [box.x - r.x, box.y - r.y, box.width, box.height], hits };
        });
      });
      expect(result[0]).toEqual(result[1]);
      expect(result[0].box.slice(2)).toEqual([100, 50]);
      const pictures = [];
      for (const id of ['port', 'oracle']) {
        const r = await page.getByTestId(id).boundingBox();
        pictures.push(decodePng(await page.screenshot({ clip: r })));
      }
      expect(pictures[0].data).toEqual(pictures[1].data);
    };
    await compare();
    await page.evaluate(() => document.querySelectorAll('[data-testid="port"],[data-testid="oracle"]').forEach(e => { e.scrollTop = 80; e.scrollLeft = 20; }));
    await compare();
    const scale = id => page.getByTestId(id).evaluate(e => getComputedStyle(e).scale.split(/\s+/).map(Number));
    await page.getByTestId('animated').evaluate(e => { const a = e.getAnimations()[0]; a.pause(); a.currentTime = 500; });
    expect(await scale('animated')).toEqual([1, 0.5]);
    await page.getByTestId('animated').evaluate(e => e.style.setProperty('--exact-press-factor', '0.5'));
    expect(await scale('animated')).toEqual([0.5, 0.25]);
    // The base pair and the uniform press factor multiply independently.
    await page.getByTestId('pressed').evaluate(e => e.style.setProperty('--exact-press-factor', '0.5'));
    expect(await scale('pressed')).toEqual([0.5, 0.25]);
    await page.getByTestId('toggle').click();
    await page.getByTestId('transitioned').evaluate(e => { getComputedStyle(e).scale; e.getAnimations().forEach(a => { a.pause(); a.currentTime = 500; }); });
    expect(await scale('transitioned')).toEqual([2, 0.5]);
    await page.getByTestId('pressed').evaluate(e => { getComputedStyle(e).scale; e.getAnimations().forEach(a => { a.pause(); a.currentTime = 1000; }); });
    expect(await scale('pressed')).toEqual([1, 0.125]);
    // Both web targets use this controller for sampled spring frames.
    await page.evaluate(async () => {
      const { motionController } = await import('/motion-glue.js');
      const views = new Map([[1, document.querySelector('[data-testid="transitioned"]')],
        [2, document.querySelector('[data-testid="pressed"]')]]);
      window.probeMotion = motionController({ views, now: () => 0, generation: () => 1,
        request: () => ({}), applyBatch: () => {}, inert: () => false });
      for (const [id, e] of views) {
        e.getAnimations().forEach(a => a.cancel());
        probeMotion.animate({ id, property: 'scale', delay: 0, duration: 1000, values: [[1, 1], [3, 0]] });
        e.getAnimations().forEach(a => { a.pause(); a.currentTime = 500; });
      }
    });
    expect(await scale('transitioned')).toEqual([2, 0.5]);
    expect(await scale('pressed')).toEqual([1, 0.25]);
    expect(errors).toEqual([]);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 60000);
