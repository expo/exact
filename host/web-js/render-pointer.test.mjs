import { afterAll, beforeAll, expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { serveBuildTree } from '../web/serve.mjs';
import { webCompiler } from './module.mjs';
import { renderer } from './render.mjs';

const root = resolve(import.meta.dir, '../..');
const kinds = ['pointerdown', 'pointerup', 'pointermove'];
let dir, server, url;
beforeAll(async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  dir = mkdtempSync(resolve(root, 'target/render-pointer-test-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Render Pointer', app: { id: 'test.render-pointer', name: 'Render Pointer' }, host: { web: {} } }));
  writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request
${kinds.map(kind => `  ${kind} "/${kind}" render=request`).join('\n')}

component App
  state heard = 0
  state pressed = 0
  action hear
    heard = heard + 1
  action press
    pressed = pressed + 1
  view
    column
${kinds.map(kind => `      if top(nav).name == "${kind}"
        button ${kind}=hear press=press testId="target" width=160 height=80
          text "Pointer"`).join('\n')}
      text \`heard=\${heard}, pressed=\${pressed}\` testId="result"
`);
  const dist = resolve(dir, 'dist');
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-pointer-test', '--out', dist, '--render', 'js'], {
    cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
  expect(status, stdout + stderr).toBe(0);
  const render = renderer(dist); render.ahead = false;
  server = createServer((req, res) => serveBuildTree(dist, req, res, {
    pages: async (_id, location) => ({ ...await render(location), cacheControl: 'no-store' }),
  }));
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  url = `http://127.0.0.1:${server.address().port}`;
}, 120000);
afterAll(async () => {
  if (server) await new Promise(ok => server.close(ok));
  if (dir) rmSync(dir, { recursive: true, force: true });
});

for (const kind of kinds) test(`${kind}: a request renders and the adopted button delivers pointer and press events`, async () => {
  const response = await fetch(`${url}/${kind}`), html = await response.text();
  expect(response.status, html).toBe(200);
  expect(html).toContain('heard=0, pressed=0');
  expect(html).toContain('application/vnd.exact.checkpoint');
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`${url}/${kind}`);
    await page.waitForFunction(() => !!document.querySelector('[data-testid="target"]')?.$pointer);
    const target = page.locator('[data-testid="target"]');
    const box = await target.boundingBox();
    await page.mouse.move(box.x + 30, box.y + 30);
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
    await page.mouse.down();
    await page.mouse.up();
    await page.waitForFunction(() => document.querySelector('[data-testid="result"]').textContent === 'heard=1, pressed=1');
    expect(await page.locator('[data-testid="result"]').textContent()).toBe('heard=1, pressed=1');
    if (kind === 'pointermove') {
      // The first move in a frame goes out in its own event; later ones in that frame wait for the next, which
      // sends the latest (#543).
      const sent = await page.evaluate(() => new Promise(done => requestAnimationFrame(() => {
        const el = document.querySelector('[data-testid="target"]'), r = el.getBoundingClientRect(), s = el.$pointer, send = s.pointermove;
        const xs = [];
        s.pointermove = (record, ...rest) => { xs.push(Math.round(record[0])); return send(record, ...rest); };
        const move = x => el.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, isPrimary: true, pointerId: 1, clientX: r.x + x, clientY: r.y + 5 }));
        move(1); const first = xs.length; move(2); move(3); const held = xs.length;
        requestAnimationFrame(() => { s.pointermove = send; done([first, held, xs]); });
      })));
      expect(sent).toEqual([1, 1, [1, 3]]);
    }
    expect(errors).toEqual([]);
  } finally { await browser.close(); }
}, 30000);
