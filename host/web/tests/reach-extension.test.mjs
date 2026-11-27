// @ref LLP 1123 D13, D17 — how far a box's colour extends past its border box on the web, measured, in Chromium, WebKit
// and Firefox, under a 47-point top band, each box's bounds in the registered properties the emitter writes them in: an
// `auto` bound at the band's inner side extends to the viewport's top (47) and on an edge the box does not extend on
// nothing; `auto` clear of the band nothing; a length is taken as given (30), `calc(10px + 1rem)` resolved (26), and an
// ancestor's clip cuts it at its padding edge (20); a box that clips its own overflow, on either axis, one whose
// background colour is transitioning, and one under a filtered ancestor extend nothing.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const PAGE = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box } .b { position: absolute; width: 40px; height: 40px }</style>
  <div class="b" id="auto" style="top:47px;left:0;--exact-safe-area-extend-bounds-top:auto"></div>
  <div class="b" id="clear" style="top:60px;left:50px;--exact-safe-area-extend-bounds-top:auto"></div>
  <div class="b" id="length" style="top:60px;left:100px;--exact-safe-area-extend-bounds-top:30px"></div>
  <div class="b" id="calc" style="top:60px;left:150px;--exact-safe-area-extend-bounds-top:calc(10px + 1rem)"></div>
  <div class="b" style="top:40px;left:200px;height:100px;overflow:hidden"><div class="b" id="cut" style="top:20px;left:0;--exact-safe-area-extend-bounds-top:30px"></div></div>
  <div class="b" id="clips" style="top:47px;left:250px;overflow:hidden;--exact-safe-area-extend-bounds-top:auto"></div>
  <div class="b" id="fading" style="top:47px;left:300px;background-color:red;transition:background-color 10s linear;--exact-safe-area-extend-bounds-top:auto"></div>
  <div class="b" style="top:0;left:350px;height:100px;filter:brightness(.8)"><div class="b" id="filtered" style="top:47px;left:0;--exact-safe-area-extend-bounds-top:auto"></div></div>
  <div class="b" id="xclip" style="top:47px;left:200px;overflow-x:clip;--exact-safe-area-extend-bounds-top:auto"></div>
  <script type="module">
    import { extension } from '/reach.js';
    const ids = ['auto', 'clear', 'length', 'calc', 'cut', 'clips', 'fading', 'filtered', 'xclip'], els = ids.map((id) => document.getElementById(id));
    getComputedStyle(document.body).color;
    document.getElementById('fading').style.backgroundColor = 'blue';
    getComputedStyle(document.getElementById('fading')).backgroundColor;
    extension(els, [47, 0, 34, 0]);
    const extent = (el) => ['top', 'right', 'bottom', 'left'].map((e) => el.style.getPropertyValue('--exact-extend-' + e));
    window.result = Object.fromEntries(ids.map((id, i) => [id, extent(els[i])]));
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

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a box's colour extension is measured from layout, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result);
    const none = ['0px', '0px', '0px'];
    expect(await page.evaluate(() => window.result)).toEqual({
      auto: ['47px', ...none], clear: ['0px', ...none], length: ['30px', ...none], calc: ['26px', ...none], cut: ['20px', ...none],
      clips: ['0px', ...none], fading: ['0px', ...none], filtered: ['0px', ...none], xclip: ['0px', ...none],
    });
  } finally {
    await browser.close();
  }
}, 30_000);
