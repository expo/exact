// @ref LLP 1123 D9, D11, D17 — on the web the boxes around a reader count what they reserve, in Chromium, WebKit and
// Firefox, under a 47-point status bar, each box's lengths in the registered properties the emitter writes them in: a
// box at the top adding 20 lays its first child out 20 lower, which reads the 47 still above it, while a child placed at
// its top reads 67, and the box itself only the 47 that reaches it; one adding -30 leaves 17 of the 47; safe-area padding
// of 8 on a box below the bar gives a child at its top 8 and its first child in flow, 8 lower, none; `1rem` of padding
// is 16 and a box inside it adding 10 makes 26, as reservations nest; and a box's value does not reach its children,
// which reserve nothing of their own. The boxes read through `outline-offset`, which moves nothing.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const PAGE = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box } .b { position: absolute; width: 100px; height: 200px }
  .r { height: 20px; outline-offset: var(--exact-safe-area-inset-top, 0px) } .at { position: absolute; top: 0; left: 0; width: 50px }</style>
  <div class="b r" id="adds" style="top:0;left:0;height:200px;--exact-additional-safe-area-top:20px;padding-top:20px">
    <div class="r" id="flow"></div><div class="r at" id="placed"></div></div>
  <div class="b" style="top:0;left:120px;--exact-additional-safe-area-top:-30px"><div class="r at" id="less"></div></div>
  <div class="b" style="top:100px;left:240px;--exact-safe-area-padding-top:8px;padding-top:8px">
    <div class="r" id="padded"></div><div class="r at" id="padtop"></div></div>
  <div class="b" style="top:400px;left:0;--exact-safe-area-padding-top:1rem">
    <div class="b" style="top:0;left:0;--exact-additional-safe-area-top:10px"><div class="r at" id="nested"></div></div></div>
  <div class="b" style="top:400px;left:120px;--exact-additional-safe-area-top:20px">
    <div class="b" style="top:0;left:0"><div class="r at" id="child"></div></div></div>
  <script type="module">
    import { observe } from '/observe.js';
    const ids = ['adds', 'flow', 'placed', 'less', 'padded', 'padtop', 'nested', 'child'];
    observe(ids.map((id) => document.getElementById(id)), [47, 0, 34, 0]);
    window.result = Object.fromEntries(ids.map((id) => [id, document.getElementById(id).style.getPropertyValue('--exact-safe-area-inset-top')]));
  </script>`;

let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    // The web host's navigation modules, the executor among them, as the page imports them
    if (/^\/[\w-]+\.js$/.test(path)) {
      return new Response(Bun.file(resolve(import.meta.dir, '../navigation' + path)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(PAGE, { headers: { 'content-type': 'text/html' } });
  } });
});
afterAll(() => server?.stop(true));

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a reader counts what the boxes around it reserve, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({
      adds: '47px', flow: '47px', placed: '67px', less: '17px', padded: '0px', padtop: '8px', nested: '26px', child: '20px',
    });
  } finally {
    await browser.close();
  }
}, 30_000);
