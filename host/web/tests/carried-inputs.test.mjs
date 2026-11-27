// @ref LLP 1123 D17 — what the web's executor writes on an element survives the runtime replacing the element's whole
// inline style, in Chromium, WebKit and Firefox: a reader's observation, which pads it through a one-second
// transition, is still there after the `style` op writes the runtime's text, through the motion controller as when
// motion is not loaded, so its padding holds and no transition starts; a snapshot's stale copy gives way to the current
// value; a dropped observation is not brought back, even from a snapshot taken while it was written; and a quoted
// semicolon in the replacing text stays its value's. A replacement that does not carry it loses the padding and
// starts a transition, which is what the carry prevents.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const PAGE = `<!doctype html><style>body { margin: 0 } div { position: absolute; width: 50px; height: 100px }</style>
  <div id="carried"></div><div id="motion"></div><div id="stale"></div><div id="dropped"></div><div id="lost"></div>
  <script type="module">
    import { observe } from '/observe.js';
    import { carry, drop } from '/inputs.js';
    import { motionController } from '/glue/motion-glue.js';
    const RUNTIME = 'padding-top:var(--exact-safe-area-inset-top, 5px);transition:padding-top 1s linear';
    const ids = ['carried', 'motion', 'stale', 'dropped', 'lost'], els = ids.map((id) => document.getElementById(id));
    // Observed before the runtime gives them their transition, so none runs as the replacements come
    els.forEach((el, i) => { el.style.cssText = 'padding-top:var(--exact-safe-area-inset-top, 5px);left:' + i * 60 + 'px'; });
    observe(els, [47, 0, 0, 0]);
    getComputedStyle(document.body).color;
    const [carried, motion, stale, dropped, lost] = els;
    const views = new Map(els.map((el, i) => [i, el]));
    const controller = motionController({ views, now: () => performance.now(), generation: () => 0, request: () => null,
      applyBatch: () => {}, inert: () => null, carry });
    carried.style.cssText = carry(carried, RUNTIME + ';left:0px;color:red');
    controller.style(1, RUNTIME + ';left:60px;color:red');
    stale.style.cssText = carry(stale, RUNTIME + ';left:120px;--exact-safe-area-inset-top:10px');
    // A snapshot taken while the observation was written, put back after it is dropped, as a hold that lets go does
    const snapshot = dropped.style.cssText;
    drop(dropped, '--exact-safe-area-inset-top');
    dropped.style.cssText = carry(dropped, snapshot.replace('padding-top', 'transition: padding-top 1s linear; padding-top'));
    lost.style.cssText = RUNTIME + ';left:240px';
    // A quoted semicolon stays its value's
    const quoted = document.createElement('div');
    document.body.append(quoted);
    quoted.style.cssText = 'padding-top:var(--exact-safe-area-inset-top, 5px)';
    observe([quoted], [47, 0, 0, 0]);
    quoted.style.cssText = carry(quoted, 'content:"a;b";color:red;padding-top:var(--exact-safe-area-inset-top, 5px)');
    window.quoted = { color: getComputedStyle(quoted).color, padding: getComputedStyle(quoted).paddingTop };
    // The transitions the replacements started, then each padding once they end
    const running = els.map((el) => el.getAnimations().length);
    document.getAnimations().forEach((a) => a.finish());
    window.result = { ...Object.fromEntries(ids.map((id, i) => [id, { padding: getComputedStyle(els[i]).paddingTop, running: running[i],
      color: getComputedStyle(els[i]).color }])), quoted: window.quoted };
  </script>`;

let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/glue/motion-glue.js') {
      return new Response(Bun.file(resolve(import.meta.dir, '../motion-glue.js')), { headers: { 'content-type': 'text/javascript' } });
    }
    // The web host's navigation modules, the executor among them, as the page imports them
    if (/^\/[\w-]+\.js$/.test(path)) {
      return new Response(Bun.file(resolve(import.meta.dir, '../navigation' + path)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(PAGE, { headers: { 'content-type': 'text/html' } });
  } });
});
afterAll(() => server?.stop(true));

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a replaced inline style keeps the executor's declarations, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result);
    const red = 'rgb(255, 0, 0)';
    expect(await page.evaluate(() => window.result)).toEqual({
      carried: { padding: '47px', running: 0, color: red },
      motion: { padding: '47px', running: 0, color: red },
      stale: { padding: '47px', running: 0, color: 'rgb(0, 0, 0)' },
      dropped: { padding: '5px', running: 1, color: 'rgb(0, 0, 0)' },
      lost: { padding: '5px', running: 1, color: 'rgb(0, 0, 0)' },
      quoted: { color: red, padding: '47px' },
    });
  } finally {
    await browser.close();
  }
}, 30_000);
