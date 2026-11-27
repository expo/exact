// @ref LLP 1123 D9, D17 — a route's transparent bar is part of the band its boxes read on the web, measured from layout,
// in Chromium, WebKit and Firefox: under a 47-point status bar, a route that pads its own top by the 47 it reads, whose
// 91-point bar then lies over its content from 47 to 138 (as `routes.js` lays a transparent bar, in its flow with a
// negative margin of its height); its first box, at 47, reads the 91 to the bar's bottom, one 60 points lower the 31
// still under the bar, one below it nothing; a box inside the bar takes none of it and reads the status bar's overlap,
// none at 47; a route nested in another with a bar of its own to 131 gives its box that lower bar; and a box outside
// every such route reads the status bar's 47 at the top as before. The boxes read through `outline-offset`, which moves
// nothing, so each stays where the fixture puts it.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const PAGE = `<!doctype html><style>body { margin: 0 } div, header { box-sizing: border-box }
  [data-navigationbar] { position: absolute; top: 0; height: 600px; width: 100px }
  header[data-exact-clear] { display: block; height: var(--bar); margin-bottom: calc(-1 * var(--bar)); position: relative; z-index: 1 }
  .r { height: 20px; outline-offset: var(--exact-safe-area-inset-top, 0px) }</style>
  <div id="route" data-navigationbar="transparent" style="left:0;padding-top:var(--exact-safe-area-inset-top, 0px)">
    <header data-exact-clear style="--bar:91px"><div class="r" id="inbar"></div></header>
    <div class="r" id="top"></div><div style="height:40px"></div><div class="r" id="mid"></div><div style="height:200px"></div><div class="r" id="below"></div>
  </div>
  <div data-navigationbar="transparent" style="left:120px">
    <header data-exact-clear style="--bar:91px"></header>
    <div data-navigationbar="transparent" style="position:relative;height:300px;width:100px">
      <header data-exact-clear style="--bar:131px"></header>
      <div class="r" id="nested"></div>
    </div>
  </div>
  <div class="r" id="plain" style="position:absolute;left:240px;top:0;width:100px"></div>
  <script type="module">
    import { observe } from '/observe.js';
    const ids = ['route', 'inbar', 'top', 'mid', 'below', 'nested', 'plain'];
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

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a route's transparent bar is part of the band its boxes read, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({
      route: '47px', inbar: '0px', top: '91px', mid: '31px', below: '0px', nested: '131px', plain: '47px',
    });
  } finally {
    await browser.close();
  }
}, 30_000);
