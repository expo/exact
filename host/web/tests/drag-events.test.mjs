// LLP 1094.000 D16 through a compiled app on the JS target: a card's `dragstart` and `dragend` actions, bound by the
// generated module and told by drag-out.js, around the app's own `drop` node, under Playwright's mouse in Chromium,
// Firefox and WebKit, as the browser's real drag. Each action writes the log the test reads, with its record's fields.
import { afterAll, beforeAll, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { browserMissing } from '../../../scripts/agent-playwright.mjs';
import { chromium, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { webCompiler } from '../../web-js/module.mjs';

const root = resolve(import.meta.dir, '../../..');
const APP = `component App
  state log = ""
  state dragging = false
  action started(e: DragEvent)
    dragging = true
    log = \`\${log}start \${e.offsetX > 0 && e.offsetY > 0} \${e.dropEffect} \${e.files.length}\${e.text}\${e.url};\`
  action ended(e: DragEvent)
    dragging = false
    log = \`\${log}end \${e.dropEffect};\`
  action entered(e: DragEvent)
    log = \`\${log}enter \${e.dropEffect};\`
  action dropped(e: DragEvent)
    log = \`\${log}drop \${e.text} \${e.dropEffect};\`
  view
    column padding=20 row-gap=20 width=600
      text log testId="log"
      row testId="card" draggable=true dataTransfer=DataTransfer(text="Mount Fuji") dragstart=started dragend=ended opacity=(dragging ? 0.4 : 1) padding=16
        text "Drag me"
      column testId="gap" height=120
        text "Nothing takes a drop here"
      column testId="zone" drop=dropped dragenter=entered height=120
        text "Drop here"
`;

let dir, server;
beforeAll(async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  dir = mkdtempSync(resolve(root, 'target/drag-events-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Drag events', app: { id: 'test.drag-events', name: 'Drag events' }, host: { web: {} } }));
  writeFileSync(resolve(dir, 'app.contract'), APP);
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'drag-events', '--out', resolve(dir, 'dist'), '--render', 'none'], {
    cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
  expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    const file = resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1));
    return existsSync(file) ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
  } });
}, 120_000);
afterAll(() => { server?.stop(true); if (dir) rmSync(dir, { recursive: true, force: true }); });

for (const [engine, missing] of [['chromium', chromium().unavailable], ['firefox', await browserMissing('firefox')], ['webkit', await browserMissing('webkit')]]) {
  need(engine, missing)(`${engine}: a card's drag start and end reach its actions around the app's own drop`, async () => {
    const browser = await suiteBrowser(engine);
    try {
      const page = await browser.newPage({ viewport: { width: 800, height: 700 } });
      const log = () => page.locator('[data-testid="log"]').textContent();
      const run = async (to, look) => {
        // WebKit under Playwright dispatches no `pointerdown` for a press after a drag on the same page
        await page.goto(server.url.href);
        await page.locator('[data-testid="card"]').waitFor();
        const a = await page.locator('[data-testid="card"]').boundingBox(), b = await page.locator(`[data-testid="${to}"]`).boundingBox();
        await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
        await page.mouse.down();
        await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 20 });
        // The card's own look while the drag is held, which its `dragstart` action set
        look.push(await page.locator('[data-testid="card"]').evaluate((el) => getComputedStyle(el).opacity));
        await page.mouse.up();
        await page.waitForTimeout(300);
        return log();
      };
      const looks = [];
      expect(await run('zone', looks)).toBe('start true none 0;enter none;drop Mount Fuji copy;end copy;');
      // Where nothing takes the drop
      expect(await run('gap', looks)).toBe('start true none 0;end none;');
      expect(looks).toEqual(['0.4', '0.4']);
    } finally {
      await browser.close();
    }
  }, 90_000);
}
