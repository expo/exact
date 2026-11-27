// @ref LLP 1123 D13, D17 — how far a box's reach takes it on the web, measured, in Chromium, WebKit and Firefox, under a
// 47-point top band and a 34-point bottom one in an 800-point viewport: a box starting at the band's inner side reaches
// to the viewport's top (47) and one 13 points below it nothing; a box 0.2 into the band still reaches (46.8), and one
// ending on the bottom band's inner side reaches its 34; inside an ancestor that reserves 8 of safe-area padding clear of
// the band, a box at the reservation's inner side reaches the ancestor's padding edge (8); an ancestor that clips, one
// with a 2-point top border and one with a transform stop reach at their padding edges (17, 35 and 27); a box under a
// route's transparent bar, at the bar's bottom, reaches the viewport's top (91); and a box out of the document reaches
// nothing.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const PAGE = `<!doctype html><style>body { margin: 0 } div, header { box-sizing: border-box } .b { position: absolute; width: 40px; height: 40px }
  [data-navigationbar] { position: absolute; top: 0; height: 300px; width: 40px }
  header[data-exact-clear] { display: block; height: 91px; margin-bottom: -91px; position: relative }</style>
  <div class="b" id="inner" style="top:47px;left:0"></div>
  <div class="b" id="clear" style="top:60px;left:50px"></div>
  <div class="b" id="into" style="top:46.8px;left:100px"></div>
  <div class="b" id="foot" style="top:726px;left:0"></div>
  <div class="b" style="top:100px;left:150px;height:100px;--exact-safe-area-padding-top:8px"><div class="b" id="reserved" style="top:8px;left:0"></div></div>
  <div class="b" style="top:30px;left:200px;height:100px;overflow:hidden"><div class="b" id="clipped" style="top:17px;left:0"></div></div>
  <div class="b" style="top:10px;left:250px;height:100px;border-top:2px solid"><div class="b" id="bordered" style="top:35px;left:0"></div></div>
  <div class="b" style="top:20px;left:300px;height:100px;transform:translateX(0)"><div class="b" id="moved" style="top:27px;left:0"></div></div>
  <div data-navigationbar="transparent" style="left:350px"><header data-exact-clear></header><div class="b" id="barred" style="top:91px;left:0"></div></div>
  <script type="module">
    import { reach } from '/reach.js';
    const ids = ['inner', 'clear', 'into', 'foot', 'reserved', 'clipped', 'bordered', 'moved', 'barred'], gone = document.createElement('div');
    const out = reach([...ids.map((id) => document.getElementById(id)), gone], [47, 0, 34, 0]);
    const round = (v) => v.map((x) => Math.round(x * 10) / 10);
    window.result = { ...Object.fromEntries(ids.map((id) => [id, round(out.get(document.getElementById(id)))])), gone: out.get(gone) };
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

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a box's reach is measured from layout, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({
      inner: [47, 0, 0, 0], clear: [0, 0, 0, 0], into: [46.8, 0, 0, 0], foot: [0, 0, 34, 0], reserved: [8, 0, 0, 0],
      clipped: [17, 0, 0, 0], bordered: [35, 0, 0, 0], moved: [27, 0, 0, 0], barred: [91, 0, 0, 0], gone: [0, 0, 0, 0],
    });
  } finally {
    await browser.close();
  }
}, 30_000);
