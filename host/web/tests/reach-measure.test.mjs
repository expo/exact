// @ref LLP 1123 D13, D17 — how far a box's reach takes it on the web, measured, in Chromium, WebKit and Firefox, under a
// 47-point top band and a 34-point bottom one in an 800-point viewport: a box starting at the band's inner side reaches
// to the viewport's top (47) and one 13 points below it nothing; a box 0.2 into the band still reaches (46.8), and one
// ending on the bottom band's inner side reaches its 34; inside an ancestor that reserves 8 of safe-area padding clear of
// the band, a box at the reservation's inner side reaches the ancestor's padding edge (8); an ancestor that clips, one
// with a 2-point top border and one with a transform stop reach at their padding edges (17, 35 and 27); a box under a
// route's transparent bar, at the bar's bottom, reaches the viewport's top (91); a box out of the document reaches
// nothing; a box at the inner side of a 10-point reservation that joins the band reaches the viewport's top (57); and
// `reached` writes a box's reach on it, as its surface reads it; a box under a `display: contents` wrapper whose overflow
// is hidden, which has no padding edge to stop it, reaches the viewport's top (47), the boxes after it measured too. A
// sticky box at the band's inner side reaches it as laid out, the page scrolled or not.
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
  <div class="b" style="top:47px;left:0;height:100px;width:10px;--exact-additional-safe-area-top:10px"><div class="b" id="joined" style="top:10px;left:0;width:10px"></div></div>
  <div style="display:contents;overflow:hidden"><div class="b" id="wrapped" style="top:47px;left:150px"></div></div>
  <script type="module">
    import { reach, reached } from '/reach.js';
    const ids = ['wrapped', 'inner', 'clear', 'into', 'foot', 'reserved', 'clipped', 'bordered', 'moved', 'barred', 'joined'], gone = document.createElement('div');
    const out = reach([...ids.map((id) => document.getElementById(id)), gone], [47, 0, 34, 0]);
    const round = (v) => v.map((x) => Math.round(x * 10) / 10);
    // As written on the box, the inputs its surface reads
    reached([document.getElementById('inner'), gone], [47, 0, 34, 0]);
    const written = ['top', 'right', 'bottom', 'left'].map((e) => document.getElementById('inner').style.getPropertyValue('--exact-reach-' + e));
    window.result = { ...Object.fromEntries(ids.map((id) => [id, round(out.get(document.getElementById(id)))])), gone: out.get(gone), written };
  </script>`;

const STICKY = `<!doctype html><style>body { margin: 0; height: 3000px }</style><div style="height:47px"></div>
  <div id="sticky" style="position:sticky;top:0;height:20px;background:#888"></div>
  <script type="module">
    import { reach } from '/reach.js';
    const el = document.getElementById('sticky'), before = reach([el], [47, 0, 34, 0]).get(el);
    scrollTo(0, 100);
    window.result = { before, after: reach([el], [47, 0, 34, 0]).get(el) };
  </script>`;

let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    // The web host's navigation modules, the executor among them, as the page imports them
    if (/^\/[\w-]+\.js$/.test(path)) {
      return new Response(Bun.file(resolve(import.meta.dir, '../navigation' + path)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(path === '/sticky' ? STICKY : PAGE, { headers: { 'content-type': 'text/html' } });
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
      wrapped: [47, 0, 0, 0], inner: [47, 0, 0, 0], clear: [0, 0, 0, 0], into: [46.8, 0, 0, 0], foot: [0, 0, 34, 0], reserved: [8, 0, 0, 0],
      clipped: [17, 0, 0, 0], bordered: [35, 0, 0, 0], moved: [27, 0, 0, 0], barred: [91, 0, 0, 0], joined: [57, 0, 0, 0], gone: [0, 0, 0, 0],
      written: ['47px', '0px', '0px', '0px'],
    });
  } finally {
    await browser.close();
  }
}, 30_000);

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a sticky box reaches as laid out, the page scrolled or not, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}sticky`);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({ before: [47, 0, 0, 0], after: [47, 0, 0, 0] });
  } finally {
    await browser.close();
  }
}, 30_000);
