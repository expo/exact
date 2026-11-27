// @ref LLP 1123 D13, D14, D17 — how deep the unsafe area occupies each scroller on the web, measured, in Chromium, WebKit
// and Firefox, under a 47-point top band and a 34-point bottom one in an 800-point viewport: a scroller over the whole
// viewport is occupied 47 at the top and 34 at the bottom; one starting at the band's inner side reaches to the
// viewport's top and is occupied 47 too; one clear of both bands not at all; one under a route's transparent bar 91, to
// the bar's bottom; one inside a box reserving 8 of safe-area padding clear of the band 8; and with a 300-point docked
// keyboard the scroller over the whole viewport is occupied 300 at the bottom, the keyboard being deeper than the band;
// a scroller below the viewport with no bottom band is occupied nothing, one at the band's inner side whose reach is
// none nothing, and one inside another scroller nothing, as the outer one takes the area (its overflow inline, as the
// emitter writes it); and under a 300-point keyboard a sideways scroller and one inside a vertical one take none of it, only
// a vertical scroller no other vertical one holds taking the keyboard, one inside a sideways scroller among them: 100 tall
// at the viewport's bottom, it is occupied by the keyboard's whole 300 past that edge, as the kernel's `taken` measures
// it, and under `overlays-content`, where no scroller takes the keyboard, by none.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const PAGE = `<!doctype html><style>body { margin: 0 } div, header { box-sizing: border-box } .s { position: absolute; width: 40px; overflow: auto }
  [data-navigationbar] { position: absolute; top: 0; height: 300px; width: 40px }
  header[data-exact-clear] { display: block; height: 91px; margin-bottom: -91px; position: relative }</style>
  <div class="s" id="whole" style="top:0;left:0;height:800px"></div>
  <div class="s" id="inner" style="top:47px;left:50px;height:300px"></div>
  <div class="s" id="clear" style="top:100px;left:100px;height:300px"></div>
  <div data-navigationbar="transparent" style="left:150px"><header data-exact-clear></header><div class="s" id="barred" style="top:91px;left:0;height:100px"></div></div>
  <div style="position:absolute;top:100px;left:200px;width:40px;height:200px;--exact-safe-area-padding-top:8px"><div class="s" id="reserved" style="top:8px;left:0;height:100px"></div></div>
  <div class="s" id="below" style="top:900px;left:250px;height:100px"></div>
  <div class="s" id="held" style="top:47px;left:300px;height:100px"></div>
  <div class="s" style="top:0;left:350px;height:800px;overflow:auto"><div class="s" id="nested" style="top:0;left:0;height:100px"></div></div>
  <div class="s" id="sideways" style="top:700px;left:250px;height:100px;overflow-x:auto;overflow-y:hidden"></div>
  <div class="s" style="top:500px;left:300px;height:300px;overflow:auto"><div class="s" id="nestedbottom" style="top:200px;left:0;height:100px"></div></div>
  <div class="s" style="top:500px;left:200px;height:300px;overflow-x:auto;overflow-y:hidden"><div class="s" id="insideways" style="top:200px;left:0;height:100px"></div></div>
  <script type="module">
    import { insets } from '/reach.js';
    const ids = ['whole', 'inner', 'clear', 'barred', 'reserved'], els = ids.map((id) => document.getElementById(id));
    const read = (el) => ['top', 'right', 'bottom', 'left'].map((e) => el.style.getPropertyValue('--exact-scroll-occupied-' + e));
    insets(els, [47, 0, 34, 0], 0);
    const result = Object.fromEntries(ids.map((id, i) => [id, read(els[i])]));
    insets([els[0]], [47, 0, 34, 0], 300);
    const below = document.getElementById('below'), held = document.getElementById('held'), nested = document.getElementById('nested');
    insets([below], [47, 0, 0, 0], 0);
    insets([held], [47, 0, 34, 0], 0, { reaches: () => [false, false, false, false] });
    insets([nested], [47, 0, 34, 0], 0);
    const [sideways, nestedbottom, insideways] = ['sideways', 'nestedbottom', 'insideways'].map((id) => document.getElementById(id));
    insets([sideways, nestedbottom, insideways], [47, 0, 0, 0], 300);
    const taking = read(insideways);
    insets([insideways], [47, 0, 0, 0], 300, { policy: 'overlays-content' });
    window.result = { ...result, keyboard: read(els[0]), below: read(below), held: read(held), nested: read(nested), sideways: read(sideways), nestedbottom: read(nestedbottom),
      insideways: taking, overlaid: read(insideways) };
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

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a scroller's occupied depth is measured from layout, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({
      whole: ['47px', '0px', '34px', '0px'], inner: ['47px', '0px', '0px', '0px'], clear: ['0px', '0px', '0px', '0px'],
      barred: ['91px', '0px', '0px', '0px'], reserved: ['8px', '0px', '0px', '0px'], keyboard: ['47px', '0px', '300px', '0px'],
      below: ['0px', '0px', '0px', '0px'], held: ['0px', '0px', '0px', '0px'], nested: ['0px', '0px', '0px', '0px'],
      sideways: ['0px', '0px', '0px', '0px'], nestedbottom: ['0px', '0px', '0px', '0px'], insideways: ['0px', '0px', '300px', '0px'], overlaid: ['0px', '0px', '0px', '0px'],
    });
  } finally {
    await browser.close();
  }
}, 30_000);
