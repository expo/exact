// LLP 1094.000 D6, D8: the browser's drag of a draggable node, through the page's own `drag-out.js`, pan and swipe
// glue and drag rules, in Chromium and Firefox under Playwright's mouse: what the drag carries, which contacts keep
// it from the node, and a swipe beside it.
import { afterAll, beforeAll, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { browserMissing } from '../../../scripts/agent-playwright.mjs';
import { chromium, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';

// The page's own stylesheet, which decides what drags: the reset that `all: unset`s a button, the restored
// `-webkit-user-drag`, a pressable's image's, and the native fields'.
const page = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const rules = page.slice(page.indexOf('<style>') + 7, page.indexOf('</style>'));
const fixture = `<!doctype html>
<style>
${rules}
  #exact-root > * { position: absolute; left: 20px; width: 300px; height: 50px; background: #ddd; }
  img { width: 40px; height: 40px; }
</style>
<main id="exact-root">
  <button id="card" draggable="true" data-datatransferfiles="assets/a.png" style="top:10px;display:flex;align-items:center"><span id="label" style="display:inline-block;width:150px;height:40px">Card</span><img id="picture"></button>
  <div id="box" draggable="true" data-datatransfertext="box" style="top:70px"><button id="press" data-exact-on="press" style="display:inline-block;width:60px;height:40px;background:#aaa">Press<img id="icon" style="width:20px;height:20px"></button><input id="field" data-native value="some words to select" style="display:inline-block;width:110px"><input id="range" data-native type="range" min="0" max="100" value="10" style="display:inline-block;width:110px"></div>
  <div id="pan" style="top:130px;height:60px;touch-action:none"><div id="inner" draggable="true" data-datatransfertext="inner" style="width:120px;height:40px;background:#aaa">inner</div></div>
  <a id="link" href="/landmark/1016" style="top:200px"><img id="linked" draggable="true" data-datatransfertext="the picture"></a>
  <div id="card2" draggable="true" data-datatransfertext="card" style="top:260px;height:60px"><div id="row" style="width:300px;height:44px;touch-action:pan-y;background:#bbb"></div></div>
  <div id="swipe" draggable="true" data-datatransfertext="swipe row" style="position:absolute;left:20px;top:340px;width:300px;height:44px;touch-action:pan-y"></div>
</main>
<script type="module">
  import { listen } from './drag-out.js';
  import { motionController } from './motion-glue.js';
  import { createInputHandlers } from './input-glue.js';
  window.drags = []; window.deltas = []; window.begins = [];
  const c = document.createElement('canvas'); c.width = c.height = 40; c.getContext('2d').fillRect(0, 0, 40, 40);
  await Promise.all([...document.querySelectorAll('img')].map((i) => new Promise((r) => { i.onload = r; i.src = c.toDataURL(); })));
  const byId = (id) => document.getElementById(id);
  const views = new Map([[1, byId('row')], [2, byId('pan')], [3, byId('swipe')]]);
  let serial = 0;
  const motion = motionController({ views, now: () => performance.now(), generation: () => 1, inert: () => false, applyBatch() {},
    request(r) {
      if (r.op === 'gesture') return { slop: 4, knee: 64, resistance: 0.2 };
      if (r.op === 'begin') { begins.push(r.view); return { token: String(++serial), value: [0, 0] }; }
      return { accepted: true };
    } });
  for (const id of [1, 3]) {
    const el = views.get(id);
    el.exactHandlers = ['swiperight', 'swipeleft'];
    motion.attachSwipe(el, id, (type, fn) => el.addEventListener(type, fn));
  }
  const input = createInputHandlers({ root: byId('exact-root'), views, retiredViews: new Set(), ready: () => true,
    inertAncestor: () => false, dispatch: (id, delta) => deltas.push(delta) });
  const pan = byId('pan');
  pan.addEventListener('pointerdown', input.pan(pan, 2, (type, fn) => pan.addEventListener(type, fn)));
  listen(document);
  window.addEventListener('dragstart', (e) => drags.push({ target: e.target.id, prevented: e.defaultPrevented,
    kinds: [...e.dataTransfer.items].map((i) => i.kind), text: e.dataTransfer.getData('text/plain'),
    uris: e.dataTransfer.getData('text/uri-list') }));
  window.ready = true;
</script>`;

let server;
beforeAll(() => {
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/') return new Response(fixture, { headers: { 'content-type': 'text/html' } });
    if (path.startsWith('/assets/') || path === '/favicon.ico') return new Response('', { status: 404 });
    return new Response(Bun.file(new URL('..' + path, import.meta.url)));
  } });
});
afterAll(() => server?.stop(true));

/** A mouse drag from `selector`'s middle (moved by `from`) by `by` pixels in `steps`, and what the page saw. */
async function drag(page, selector, by, { from = [0, 0], steps = 15 } = {}) {
  await page.evaluate(() => { drags.length = 0; deltas.length = 0; begins.length = 0; });
  const box = await page.locator(selector).boundingBox();
  const [x, y] = [box.x + box.width / 2 + from[0], box.y + box.height / 2 + from[1]];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + by[0], y + by[1], { steps });
  await page.mouse.up();
  return page.evaluate(() => {
    const field = document.getElementById('field');
    return { drags: [...drags], deltas: deltas.length, begins: [...begins], range: Number(document.getElementById('range').value),
      selection: field.selectionEnd - field.selectionStart };
  });
}
/** The drags the browser went on with. */
const went = (seen) => seen.drags.filter((d) => !d.prevented);

for (const [engine, missing] of [['chromium', chromium().unavailable], ['firefox', await browserMissing('firefox')]]) {
  need(engine, missing)(`${engine}: a draggable node's drag carries its declaration, and what keeps a contact keeps it`, async () => {
    const browser = await suiteBrowser(engine);
    try {
      const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
      await page.goto(`http://127.0.0.1:${server.port}/`);
      await page.waitForFunction('window.ready');
      // The card drags by its label and by its picture, carrying its file as a URL and not the picture's own bytes.
      for (const at of ['#label', '#picture']) {
        const card = went(await drag(page, at, [0, 20]));
        expect(card.map((d) => [d.kinds.every((kind) => kind === 'string'), d.uris])).toEqual([[true, `http://127.0.0.1:${server.port}/assets/a.png`]]);
      }
      // A press, a picture in it, a text field and a slider inside the node keep a mouse's contact, and the slider
      // moves. The field selects its text in Chromium; Firefox selects none in a field inside a node it may drag.
      expect(went(await drag(page, '#press', [0, 20], { from: [-15, 0] }))).toEqual([]);
      expect(went(await drag(page, '#icon', [0, 20]))).toEqual([]);
      const field = await drag(page, '#field', [50, 0], { from: [-40, 0] });
      expect([went(field), field.selection > 0]).toEqual([[], engine === 'chromium']);
      const slider = await drag(page, '#range', [60, 0], { from: [-44, 0] });
      expect(went(slider)).toEqual([]);
      expect(slider.range).toBeGreaterThan(40);
      // By a mouse, the innermost draggable node inside a pan drags, and the pan does not.
      const inner = await drag(page, '#inner', [0, 20]);
      expect(went(inner).map((d) => d.text)).toEqual(['inner']);
      expect(inner.deltas).toBe(0);
      // A draggable picture inside a link carries its own declaration.
      expect(went(await drag(page, '#linked', [0, 20])).map((d) => [d.text, d.uris])).toEqual([['the picture', '']]);
      // A swipe inside a draggable card takes a sideways drag; an upright one drags the card.
      const sideways = await drag(page, '#row', [70, 0]);
      expect([went(sideways), sideways.begins]).toEqual([[], [1]]);
      expect(went(await drag(page, '#row', [0, 30])).map((d) => d.text)).toEqual(['card']);
      // A fast move whose first step passes the browser's threshold: Firefox starts the drag before its pointermove,
      // so the swipe takes the unknown movement, which swipes sideways and drags nothing upright; Chromium's move comes
      // first, even when it lands off the row, so an upright one drags.
      const fast = await drag(page, '#swipe', [70, 0], { steps: 3 });
      expect([went(fast), fast.begins]).toEqual([[], [3]]);
      const upright = await drag(page, '#swipe', [0, 70], { steps: 3 });
      expect([went(upright).map((d) => d.text), upright.begins]).toEqual([engine === 'chromium' ? ['swipe row'] : [], []]);
    } finally {
      await browser.close();
    }
  }, 60_000);
}
