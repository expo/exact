// @ref LLP 1123 D9, D11, D12, D17 — the area a reader observes on the web follows the kernel's (`kernel/src/kernel/
// area.rs`), in Chromium, WebKit and Firefox, under a 47-point top band: a box clear of the band with a 10-point top
// border and 20 of safe-area padding gives a child 15 below its padding edge the 5 left of the 20, measured from the
// padding edge; a 10-point reservation on a box at the band's inner side joins the band, so a child at its inner side observes
// none of it and the area runs from the viewport's top to 57; a reservation on the right alone leaves the top band where
// it was for a child placed back into it; a reservation that reads its own box's observation is read again each round,
// so its child's reported input agrees with a fresh measurement; a transparent bar whose header the runtime wraps in a
// list item is found; a new reader starts at 0 and so settles on the fixed point there, not on its fallback's; and a
// positioned `display: contents` ancestor is no containing block, so a scrolled box beyond it, which does not hold its
// absolutely positioned child, moves nothing of it; a reader with no box keeps the input it had; and an animation of a
// flex row's second item counts as moving the first, which a centred row moves. The boxes read through `outline-offset`, which moves nothing, but where a box's input moves it.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const PAGE = `<!doctype html><style>body { margin: 0 } div, header { box-sizing: border-box } .b { position: absolute; width: 40px; height: 100px }
  .r { height: 20px; outline-offset: var(--exact-safe-area-inset-top, 0px) } .at { position: absolute; left: 0; width: 20px }
  [data-navigationbar] { position: absolute; top: 0; height: 300px; width: 40px }
  header[data-exact-clear] { display: block; height: 91px; margin-bottom: -91px; position: relative }</style>
  <div class="b" style="top:100px;left:0;border-top:10px solid;--exact-safe-area-padding-top:20px"><div class="r at" id="bordered" style="top:15px"></div></div>
  <div class="b" style="top:47px;left:50px;--exact-additional-safe-area-top:10px"><div class="r at" id="joined" style="top:10px"></div><div class="r at" id="joinedtop" style="top:0"></div></div>
  <div class="b" style="top:60px;left:100px;--exact-additional-safe-area-right:10px"><div class="r at" id="untouched" style="top:-40px"></div></div>
  <div id="coupled" style="position:absolute;left:150px;width:40px;bottom:660px;outline-offset:var(--exact-safe-area-inset-top, 0px);--exact-additional-safe-area-top:calc(10px + var(--exact-safe-area-inset-top, 0px))">
    <div id="coupledchild" style="height:calc(100px - var(--exact-safe-area-inset-top, 0px))"></div></div>
  <div data-navigationbar="transparent" style="left:200px"><div data-exact-list-item><header data-exact-clear></header></div><div class="r at" id="wrapped" style="top:47px"></div></div>
  <div class="b" id="zero" style="left:250px;height:20px;top:calc(50px - var(--exact-safe-area-inset-top, 50px))"></div>
  <div style="position:absolute;top:0;left:300px;width:40px;height:100px"><div id="port" style="height:100px;overflow:hidden"><div style="height:1000px"></div>
    <div style="display:contents;position:relative"><div class="r" id="contents" style="position:absolute;top:0;left:0;width:20px"></div></div></div></div>
  <div class="r" id="hidden" style="display:none;--exact-safe-area-inset-top:30px"></div>
  <div style="position:absolute;top:600px;left:0;width:390px;display:flex;justify-content:center"><div class="r" id="first" style="width:20px"></div><div id="second" style="width:10px;height:20px"></div></div>
  <script type="module">
    import { moving, observe } from '/observe.js';
    document.getElementById('port').scrollTop = 35;
    const ids = ['bordered', 'joined', 'joinedtop', 'untouched', 'coupled', 'coupledchild', 'wrapped', 'zero', 'contents', 'hidden'];
    const els = ids.map((id) => document.getElementById(id)), { unsettled } = observe(els, [47, 0, 34, 0]);
    const read = () => Object.fromEntries(ids.map((id, i) => [id, parseFloat(els[i].style.getPropertyValue('--exact-safe-area-inset-top')) || 0]));
    const reported = read();
    // A fresh transaction from where the first left: a settled input is its measurement, so nothing moves
    observe(els, [47, 0, 34, 0]);
    // Widening the row's second item moves its first, so the first is among what the animation moves
    const widening = document.getElementById('second').animate([{ width: '10px' }, { width: '100px' }], { duration: 10000 });
    const moved = moving([document.getElementById('first')]).map((el) => el.id);
    widening.cancel();
    window.result = { reported, fresh: read(), unsettled: unsettled.map((el) => el.id), moved };
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

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a reader's area follows the kernel's, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result);
    const { reported, fresh, unsettled, moved } = await page.evaluate(() => window.result);
    expect(unsettled).toEqual([]);
    expect(moved).toEqual(['first']);
    expect(reported.bordered).toBeCloseTo(5, 1);
    expect(reported.joined).toBeCloseTo(0, 1);
    expect(reported.joinedtop).toBeCloseTo(10, 1);
    expect(reported.untouched).toBeCloseTo(27, 1);
    // The coupled box and its child: what each reports is what a fresh measurement gives
    expect(Math.abs(fresh.coupled - reported.coupled)).toBeLessThan(0.5);
    expect(Math.abs(fresh.coupledchild - reported.coupledchild)).toBeLessThan(0.5);
    expect(reported.wrapped).toBeCloseTo(44, 1);
    expect(reported.zero).toBe(0);
    expect(reported.contents).toBe(47);
    expect(reported.hidden).toBe(30);
  } finally {
    await browser.close();
  }
}, 30_000);
