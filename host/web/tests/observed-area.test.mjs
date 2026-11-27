// @ref LLP 1123 D9, D11, D12, D17 — the area a reader observes on the web follows the kernel's (`kernel/src/kernel/
// area.rs`), in Chromium, WebKit and Firefox, under a 47-point top band; each case's geometry is beside its assertion.
// The boxes read through `outline-offset`, which moves nothing, but where a box's input moves it.
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
  <div id="coupled" style="position:absolute;left:150px;width:40px;bottom:690px;outline-offset:var(--exact-safe-area-inset-top, 0px);--exact-additional-safe-area-top:calc(10px + var(--exact-safe-area-inset-top, 0px))">
    <div id="coupledchild" style="height:calc(100px - 0.5 * var(--exact-safe-area-inset-top, 0px))"></div></div>
  <div data-navigationbar="transparent" style="left:200px"><div data-exact-list-item><header data-exact-clear></header></div><div class="r at" id="wrapped" style="top:47px"></div></div>
  <div class="b" id="zero" style="left:250px;height:20px;top:calc(50px - var(--exact-safe-area-inset-top, 50px))"></div>
  <div style="position:absolute;top:0;left:300px;width:40px;height:100px"><div id="port" style="height:100px;overflow:hidden"><div style="height:1000px"></div>
    <div style="display:contents;position:relative"><div class="r" id="contents" style="position:absolute;top:0;left:0;width:20px"></div></div></div></div>
  <div class="r" id="hidden" style="display:none;--exact-safe-area-inset-top:30px"></div>
  <div style="position:absolute;top:600px;left:0;width:390px;display:flex;justify-content:center"><div class="r" id="first" style="width:20px"></div><div id="second" style="height:20px"><div id="inside" style="width:10px;height:20px"></div></div></div>
  <script type="module">
    import { moving, observe } from '/observe.js';
    document.getElementById('port').scrollTop = 35;
    const ids = ['bordered', 'joined', 'joinedtop', 'untouched', 'coupled', 'coupledchild', 'wrapped', 'zero', 'contents', 'hidden'];
    const els = ids.map((id) => document.getElementById(id)), { unsettled } = observe(els, [47, 0, 34, 0]);
    const read = () => Object.fromEntries(ids.map((id, i) => [id, parseFloat(els[i].style.getPropertyValue('--exact-safe-area-inset-top')) || 0]));
    const reported = read();
    // A fresh transaction from where the first left: a settled input is its measurement, so nothing moves
    observe(els, [47, 0, 34, 0]);
    // Widening a box inside the row's second item widens the item and moves the first, so the first is among what the
    // animation moves
    const widening = document.getElementById('inside').animate([{ width: '10px' }, { width: '100px' }], { duration: 10000 });
    const moved = moving([document.getElementById('first')]).map((el) => el.id);
    widening.cancel();
    window.result = { reported, fresh: read(), unsettled: unsettled.map((el) => el.id), moved };
  </script>`;

// A scroller's own 20 of safe-area padding, which its content observes; a `display: contents` box whose inline style
// says `overflow: auto`, which scrolls nothing; a scroller and a reader the classes style, found by their marks; under a
// 300-point keyboard, a reader in a vertical scroller at the bottom, which takes the keyboard, one in a sideways one,
// which does not, and one in a sideways scroller inside a vertical one, which does; a reader that comes to read its bottom
// edge too; and a class-held reader whose inline style reads another edge, which reads every input
const MORE = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box }
  .r { height: 20px; outline-offset: var(--exact-safe-area-inset-top, 0px) } .at { position: absolute; top: 0; left: 0; width: 20px }
  .classed { overflow-y: auto } .classreader { outline-offset: var(--exact-safe-area-inset-top, 0px) }
  .k { position: absolute; bottom: 0; width: 40px; height: 100px } .kr { height: 20px; outline-offset: var(--exact-keyboard-inset-bottom, 0px); margin-top: 80px }</style>
  <div style="position:absolute;top:300px;left:0;width:40px;height:100px;overflow:auto;--exact-safe-area-padding-top:20px"><div class="r at" id="ownreserve"></div></div>
  <div style="position:absolute;top:0;left:50px;width:40px;height:100px"><div style="display:contents;overflow:auto"><div class="r at" id="contents"></div></div></div>
  <div class="classed" data-exact-safe-area="scroller" style="position:absolute;top:0;left:100px;width:40px;height:100px"><div class="r at" id="classcut"></div></div>
  <div class="classreader" data-exact-safe-area="reader" id="classreader" style="position:absolute;top:0;left:150px;width:20px;height:20px"></div>
  <div class="k" style="left:200px;overflow-y:auto"><div class="kr" id="vertical"></div></div>
  <div class="k" style="left:250px;overflow-x:auto;overflow-y:hidden"><div class="kr" id="sideways"></div></div>
  <div id="grows" style="position:absolute;top:0;left:300px;width:20px;height:20px;padding-top:var(--exact-safe-area-inset-top, 30px)"></div>
  <div class="k" style="left:350px;overflow-y:auto"><div style="height:100px;overflow-x:auto;overflow-y:hidden"><div class="kr" id="deep"></div></div></div>
  <div class="classreader" data-exact-safe-area="reader" id="mixed" style="position:absolute;top:0;left:350px;width:20px;height:20px;padding-bottom:var(--exact-safe-area-inset-bottom, 0px)"></div>
  <script type="module">
    import { observe, readers } from '/observe.js';
    const $ = (id) => document.getElementById(id), top = (el) => el.style.getPropertyValue('--exact-safe-area-inset-top');
    const found = readers(document).map((el) => el.id).filter(Boolean);
    observe(['ownreserve', 'contents', 'classcut', 'classreader', 'grows', 'mixed'].map($), [47, 0, 0, 0]);
    observe(['vertical', 'sideways', 'deep'].map($), [47, 0, 0, 0], 300);
    // The reader comes to read its bottom edge, which nothing obstructs
    $('grows').style.paddingBottom = 'var(--exact-safe-area-inset-bottom, 30px)';
    observe([$('grows')], [47, 0, 0, 0]);
    window.result = { found, ownreserve: top($('ownreserve')), contents: top($('contents')), classcut: top($('classcut')), classreader: top($('classreader')),
      vertical: $('vertical').style.getPropertyValue('--exact-keyboard-inset-bottom'), sideways: $('sideways').style.getPropertyValue('--exact-keyboard-inset-bottom'),
      deep: $('deep').style.getPropertyValue('--exact-keyboard-inset-bottom'), mixed: top($('mixed')),
      bottom: $('grows').style.getPropertyValue('--exact-safe-area-inset-bottom'), padding: getComputedStyle($('grows')).paddingBottom };
  </script>`;

let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    // The web host's navigation modules, the executor among them, as the page imports them
    if (/^\/[\w-]+\.js$/.test(path)) {
      return new Response(Bun.file(resolve(import.meta.dir, '../navigation' + path)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(path === '/more' ? MORE : PAGE, { headers: { 'content-type': 'text/html' } });
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
    // Widening a box inside a centred row's second item moves the first
    expect(moved).toEqual(['first']);
    // A box at 100, clear of the band, with a 10-point top border and 20 of safe-area padding: its area runs from its
    // padding edge at 110 to 130, so a child at 125 observes 5
    expect(reported.bordered).toBeCloseTo(5, 1);
    // A 10-point reservation on a box at the band's inner side joins the band, its area 0 to 57: a child at 57 observes
    // none of it, one at 47 its 10
    expect(reported.joined).toBeCloseTo(0, 1);
    expect(reported.joinedtop).toBeCloseTo(10, 1);
    // A reservation on the right alone leaves the top band where it was, for a child placed back into it at 20
    expect(reported.untouched).toBeCloseTo(27, 1);
    // The coupled box and its child, the child resizing the box whose reservation reads the box's own observation: each
    // settles where a fresh measurement agrees, the child near its fixed point of 42
    expect(Math.abs(fresh.coupled - reported.coupled)).toBeLessThan(0.5);
    expect(Math.abs(fresh.coupledchild - reported.coupledchild)).toBeLessThan(0.5);
    expect(Math.abs(reported.coupledchild - 42)).toBeLessThan(1);
    // A transparent bar to 91, its header in a list-item wrapper, over a reader at 47
    expect(reported.wrapped).toBeCloseTo(44, 1);
    // `top: calc(50px - input)`: from 0 it lies at 50, clear of the band, its fixed point there
    expect(reported.zero).toBe(0);
    // A positioned `display: contents` box contains nothing, so the clipping box scrolled by 35 does not move its child
    expect(reported.contents).toBe(47);
    // A reader with no box keeps the input it had
    expect(reported.hidden).toBe(30);
  } finally {
    await browser.close();
  }
}, 30_000);

for (const engine of ['chromium', 'webkit', 'firefox']) test(`scrollers keep their own reservations and take the keyboard on their axis, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}more`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    expect(r.found).toContain('classreader');
    expect(r).toMatchObject({ ownreserve: '20px', contents: '47px', classcut: '0px', classreader: '47px', vertical: '0px', sideways: '300px', deep: '0px', bottom: '0px', padding: '0px', mixed: '47px' });
  } finally {
    await browser.close();
  }
}, 30_000);
