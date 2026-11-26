// @ref LLP 1123 D6 on the wasm target: `exactGeometry` answers from the box the browser draws, as on the JS target
// (host/web-js/facts.test.mjs), through the runner's page import, and a commit that moves the box is followed by one
// that answers it again at the page's next layout. A plan runs here as conform runs a synthetic one: on a wasm
// Caltrain dist built with every capability, by `exact.reload`.
import { expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertWebDistApp } from '../../../scripts/agent.mjs';
import { chromium, refuseStale, suiteBrowser, webChanges } from '../../../scripts/agent-launch.mjs';
import { resolveApp } from '../../../scripts/app.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { jsTargetBuild } from '../serve.mjs';

const ROOT = resolve(fileURLToPath(new URL('../../..', import.meta.url)));
const wasmDist = resolve(process.env.EXACT_WEB_DIST ?? resolve(ROOT, 'host/web/dist'));
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm' };
const BUILD = 'EXACT_WEB_LINK=all bun host/web/build.mjs caltrain-web --wasm';

async function prerequisite() {
  const { unavailable } = chromium();
  if (unavailable) return ['chrome', unavailable];
  try {
    const app = resolveApp('caltrain');
    await assertWebDistApp(wasmDist, app);
    if (jsTargetBuild(wasmDist)) return ['fixture', `${wasmDist} is a JS-target build; this test reads the wasm target (${BUILD})`];
    // Without every capability linked a swapped-in plan's module imports no geometry, and every fact is unavailable.
    let receipt;
    try { receipt = JSON.parse(readFileSync(resolve(wasmDist, 'bake.json'), 'utf8')); } catch { return ['fixture', `${wasmDist} has no readable bake receipt`]; }
    if (receipt.binary?.configuration?.flags?.EXACT_WEB_LINK !== 'all') return ['fixture', `${wasmDist} was not built with EXACT_WEB_LINK=all`];
    refuseStale('web', resolve(wasmDist, '.exact-build.json'), webChanges(wasmDist, app).all, BUILD);
  } catch (error) { return ['fixture', error.message]; }
  return null;
}
const check = need(...((await prerequisite()) ?? ['chrome', null]));

const APP = `shape Size
  width: number
  height: number
shape Rect
  x: number
  y: number
  width: number
  height: number
shape Hero
  available: bool
  generation: number
  size: option<Size>
  frame: option<Rect>
component App
  state w = 120
  action widen
    w = 300
  resource g = exactGeometry("hero", "owner", "") as shape Hero
  resource n = exactGeometry("hero", "node", "frame") as shape Hero
  derive width = match g.size { case some(s) => s.width, case none => 0 }
  derive top = match g.frame { case some(f) => f.y, case none => -1 }
  derive inner = match n.frame { case some(f) => f.x, case none => -1 }
  view
    column id="frame" padding=16
      button "widen" testId="widen" press=widen
      box id="hero" width=w height=40
      text \`\${g.available} \${g.generation} \${width} \${top} \${inner}\` testId="out"
`;

function serve(plan) {
  return Bun.serve({ port: 0, hostname: '127.0.0.1', async fetch(req) {
    let path = new URL(req.url).pathname;
    if (path === '/__plan') return new Response(Bun.file(plan));
    if (path.endsWith('/')) path += 'index.html';
    const file = Bun.file(resolve(wasmDist, '.' + path));
    if (!(await file.exists())) return new Response('', { status: 404 });
    return new Response(file, { headers: { 'content-type': TYPES[extname(path)] ?? 'application/octet-stream', 'cache-control': 'no-store' } });
  } });
}

for (const engine of ['chromium', 'webkit', 'firefox']) check(`the wasm host answers a geometry fact from the drawn box, and again after a commit moves it, in ${engine}`, async () => {
  const tmp = mkdtempSync(resolve(tmpdir(), 'exact-wasm-geometry-facts-')), plan = resolve(tmp, 'app.plan');
  let browser, server;
  try {
    writeFileSync(resolve(tmp, 'app.contract'), APP);
    const c = spawnSync('cargo', ['run', '-q', '-p', 'contract', '--', 'build', resolve(tmp, 'app.contract'), '-o', plan], { cwd: ROOT, encoding: 'utf8' });
    expect(c.status, c.stderr).toBe(0);
    browser = await suiteBrowser(engine);
    server = serve(plan);
    const page = await browser.newPage({ viewport: { width: 600, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.getElementById('exact-root')?.dataset.bootMs != null, null, { timeout: 30000 });
    await page.evaluate(async () => {
      const bytes = await fetch('/__plan').then((r) => r.arrayBuffer());
      await globalThis.exact.reload(new Uint8Array(bytes), true);
    });
    const out = () => page.evaluate(() => document.querySelector('[data-testid="out"]')?.textContent ?? '');
    const drawn = () => page.evaluate(() => {
      const hero = document.getElementById('hero').getBoundingClientRect(), frame = document.getElementById('frame').getBoundingClientRect();
      return `${hero.width} ${hero.top + scrollY} ${hero.left - frame.left}`;
    });
    // Answered once the geometry glue loads, with the box as drawn: its width, its top in the document and its left
    // in the column's border box
    await page.waitForFunction(() => /^true \d+ 120 /.test(document.querySelector('[data-testid="out"]')?.textContent ?? ''), null, { timeout: 10000 });
    const [, first] = (await out()).split(' ');
    expect((await out()).split(' ').slice(2).join(' ')).toBe(await drawn());
    await page.click('[data-testid="widen"]');
    await page.waitForFunction(() => /^true \d+ 300 /.test(document.querySelector('[data-testid="out"]')?.textContent ?? ''), null, { timeout: 10000 });
    const [, later] = (await out()).split(' ');
    expect(Number(later)).toBeGreaterThan(Number(first));
    expect((await out()).split(' ').slice(2).join(' ')).toBe(await drawn());
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(tmp, { recursive: true, force: true });
  }
}, 120000);
