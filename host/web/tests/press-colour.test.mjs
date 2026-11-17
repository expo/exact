import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// The press paint's background colour in a real browser (LLP 1061 D7): the shell's CSS and the input glue, in
// every engine.
import { expect, beforeAll, afterAll } from 'bun:test';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { decodePng } from '../../../scripts/png.mjs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';
import { fileURLToPath } from 'node:url';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests();

const WEB = resolve(fileURLToPath(new URL('..', import.meta.url)));
const sheet = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>[\s\S]*?<\/style>/)[0];
const page = sheet + `<div id="exact-root"></div>
<script type="module">
  import { createInputHandlers } from './input-glue.js';
  const root = document.getElementById('exact-root');
  createInputHandlers({ root, views: new Map(), retiredViews: new Set(), ready: () => true, inertAncestor: () => false, dispatch() {} });
  window.ready = true;
</script>`;

let server;
beforeAll(async () => {
  server = createServer((req, res) => {
    if (req.url === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(page); return; }
    if (!req.url.endsWith('.js')) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': 'text/javascript' }); res.end(readFileSync(resolve(WEB, req.url.slice(1))));
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
});
afterAll(() => server?.close());

// Every engine: a translucent pressed colour stands in for the node's own, as CSS's `:active` background-color does
// (expo/exact#530), with a declared transition of it and without: held, a box whose pressed colour equals its own
// paints the same pixels, and one with another shows that colour over the page, not over its own; the declared
// transition never starts.
for (const engine of ['chromium', 'firefox', 'webkit']) engineTest[engine](`${engine}: a translucent pressed colour stands in for the node's own`, async () => {
  const browser = await suiteBrowser(engine);
  const page = await browser.newPage({ viewport: { width: 300, height: 200 } });
  try {
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(() => window.ready);
    await page.evaluate(() => {
      document.body.style.background = 'rgb(255, 255, 255)';
      const box = (id, x, y, base, pressed, extra = '') => `<div id="${id}" data-exact-on="press" style="--exact-press-background-color:${pressed};position:fixed;left:${x}px;top:${y}px;width:60px;height:40px;background-color:${base};${extra}"></div>`;
      document.getElementById('exact-root').innerHTML = box('same', 20, 20, 'rgba(0,0,0,0.4)', 'rgba(0,0,0,0.4)') + box('other', 100, 20, 'rgba(0,0,255,0.4)', 'rgba(255,0,0,0.4)')
        + box('same-t', 20, 100, 'rgba(0,0,0,0.4)', 'rgba(0,0,0,0.4)', 'transition:background-color 1s linear') + box('other-t', 100, 100, 'rgba(0,0,255,0.4)', 'rgba(255,0,0,0.4)', 'transition:background-color 1s linear');
    });
    const pixel = async (x, y) => { const { width, data } = decodePng(await page.screenshot()); return [...data.slice((y * width + x) * 4, (y * width + x) * 4 + 3)]; };
    const near = (got, want) => got.every((v, i) => Math.abs(v - want[i]) <= 3);
    for (const [id, x, y, rest, held] of [['same', 50, 40, [153, 153, 153], [153, 153, 153]], ['other', 130, 40, [153, 153, 255], [255, 153, 153]], ['same-t', 50, 120, [153, 153, 153], [153, 153, 153]], ['other-t', 130, 120, [153, 153, 255], [255, 153, 153]]]) {
      const before = await pixel(x, y);
      const transitions = await page.evaluate(async (id) => {
        const el = document.getElementById(id), r = el.getBoundingClientRect();
        el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerType: 'mouse', pointerId: 1, isPrimary: true, button: 0, buttons: 1, clientX: r.x + 5, clientY: r.y + 5 }));
        const started = el.getAnimations().filter(a => a.transitionProperty).length;
        el.getAnimations().forEach(a => a.finish());
        await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
        return started + el.getAnimations().filter(a => a.transitionProperty).length;
      }, id);
      const pressed = await pixel(x, y);
      const after = await page.evaluate(async (id) => {
        const el = document.getElementById(id), r = el.getBoundingClientRect();
        el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerType: 'mouse', pointerId: 1, isPrimary: true, button: 0, buttons: 0, clientX: r.x + 5, clientY: r.y + 5 }));
        const started = el.getAnimations().filter(a => a.transitionProperty).length;
        el.getAnimations().forEach(a => a.finish());
        await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
        return started;
      }, id);
      const released = await pixel(x, y);
      expect([engine, id, near(before, rest) || before, near(pressed, held) || pressed, near(released, rest) || released, transitions, after]).toEqual([engine, id, true, true, true, 0, 0]);
    }
  } finally {
    await page.close();
  }
}, 60_000);
