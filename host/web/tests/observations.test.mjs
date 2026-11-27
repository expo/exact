// @ref LLP 1123 D9, D17 — each reader's own safe-area observation on the web, in Chrome with a phone's safe area, and in
// WebKit and Firefox with that band given: a
// reader at the top takes the status bar's depth and one below it none; a reader its own input moves settles over a few
// rounds, D9's secant steps finding a fixed point where substitution would swing; one with no fixed point is unsettled at
// D9's bound; a reader is measured with every scroll offset at zero; a reader out of the document takes `initial`; and
// an ancestor's value never reaches a reader, which has its own. The properties are unregistered, as activation leaves
// them, so `initial` is the guaranteed-invalid value and a reader's fallback applies.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    // The web host's navigation modules, the executor among them, as the page imports them
    if (/^\/[\w-]+\.js$/.test(path)) {
      return new Response(Bun.file(resolve(import.meta.dir, '../navigation' + path)), { headers: { 'content-type': 'text/javascript' } });
    }
    if (path === '/keyboard') return new Response(KEYBOARD, { headers: { 'content-type': 'text/html' } });
    if (path === '/transitions') return new Response(TRANSITIONS, { headers: { 'content-type': 'text/html' } });
    if (path === '/scrolled') return new Response(SCROLLED, { headers: { 'content-type': 'text/html' } });
    if (path === '/observed') return new Response(OBSERVED, { headers: { 'content-type': 'text/html' } });
    if (path === '/readers') return new Response(READERS, { headers: { 'content-type': 'text/html' } });
    if (path === '/motion') return new Response(MOTION, { headers: { 'content-type': 'text/html' } });
    if (path === '/handoff') return new Response(HANDOFF, { headers: { 'content-type': 'text/html' } });
    if (path === '/solve') return new Response(SOLVE, { headers: { 'content-type': 'text/html' } });
    if (path === '/narrowing') return new Response(NARROWING, { headers: { 'content-type': 'text/html' } });
    if (path === '/placed') return new Response(PLACED, { headers: { 'content-type': 'text/html' } });
    if (path === '/clamped') return new Response(CLAMPED, { headers: { 'content-type': 'text/html' } });
    if (path === '/delayed') return new Response(DELAYED, { headers: { 'content-type': 'text/html' } });
    if (path === '/listed') return new Response(LISTED, { headers: { 'content-type': 'text/html' } });
    return new Response(`<!doctype html><meta name="viewport" content="width=device-width, viewport-fit=cover">
      <style>body { margin: 0 } div { box-sizing: border-box }</style>
      <div id="top" style="position:absolute;top:0;left:0;width:390px;height:200px;padding-top:var(--exact-safe-area-inset-top, 5px)"></div>
      <div id="low" style="position:absolute;top:300px;left:0;width:100px;height:100px;padding-top:var(--exact-safe-area-inset-top, 5px)"></div>
      <div id="settles" style="position:absolute;left:100px;width:50px;height:50px;top:calc(60px - var(--exact-safe-area-inset-top, 60px))"></div>
      <div id="swings" style="position:absolute;left:200px;width:50px;height:50px;top:calc(2 * var(--exact-safe-area-inset-top, 0px))"></div>
      <div id="jumps" style="position:absolute;left:250px;width:40px;height:50px;top:clamp(0px, (var(--exact-safe-area-inset-top, 0px) - 10px) * 1000, 100px)"></div>
      <div style="position:absolute;top:0;left:300px;--exact-safe-area-inset-top:30px"><div id="inside" style="width:20px;height:400px;padding-top:var(--exact-safe-area-inset-top, 5px)"></div></div>
      <script type="module">
        import { observe, documentBand } from '/observe.js';
        const ids = ['top', 'low', 'settles', 'swings', 'jumps', 'inside'], gone = document.createElement('div');
        const readers = [...ids.map((id) => document.getElementById(id)), gone];
        // Chrome's override is the browser's env(); another engine takes the same band from the query
        const given = new URL(location.href).searchParams.get('band');
        const band = given ? given.split(',').map(Number) : documentBand(), { rounds, unsettled } = observe(readers, band);
        const inset = (el) => ['top', 'right', 'bottom', 'left'].map((e) => el.style.getPropertyValue('--exact-safe-area-inset-' + e));
        window.result = { band, rounds, unsettled: unsettled.map((el) => el.id || 'gone'), gone: inset(gone),
          inputs: Object.fromEntries(ids.map((id) => [id, inset(document.getElementById(id))])),
          padding: Object.fromEntries(['top', 'low', 'inside'].map((id) => [id, parseFloat(getComputedStyle(document.getElementById(id)).paddingTop)])),
          tops: Object.fromEntries(['settles', 'swings'].map((id) => [id, document.getElementById(id).getBoundingClientRect().top])) };
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
});
afterAll(() => server?.stop(true));

// Readers whose inputs transition: one at the top padding by its input over a second, one its input moves over a
// second whose transition the page's sheet makes important, beside a box whose opacity is already transitioning and one
// whose opacity the page sets just before the rounds, which starts its own, the rounds 100ms in
const TRANSITIONS = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box; position: absolute; width: 50px; height: 50px }
  #moves { transition: top 1s linear !important }</style>
  <div id="pads" style="top:0;left:0;height:200px;padding-top:var(--exact-safe-area-inset-top, 5px);transition:padding-top 1s linear"></div>
  <div id="moves" style="left:100px;top:calc(60px - var(--exact-safe-area-inset-top, 60px));transition:top 1s linear"></div>
  <div id="other" style="left:200px;top:300px;opacity:0;transition:opacity 1s linear"></div>
  <div id="fresh" style="left:300px;top:300px;opacity:0;transition:opacity 1s linear"></div>
  <div id="hands" style="left:0;top:0;width:390px;--hero:var(--exact-safe-area-inset-top, 5px)"><div id="takes" style="position:static;height:var(--hero);transition:height 1s linear"></div></div>
  <script type="module">
    import { observe } from '/observe.js';
    const other = document.getElementById('other'), takes = document.getElementById('takes'), events = [];
    for (const type of ['transitionrun', 'transitioncancel']) takes.addEventListener(type, (e) => events.push(e.type + ' ' + e.propertyName));
    other.getBoundingClientRect();
    other.style.opacity = '1';
    getComputedStyle(other).opacity;
    setTimeout(() => {
      const running = Math.round(other.getAnimations()[0].currentTime), fresh = document.getElementById('fresh');
      fresh.style.opacity = '1';
      const readers = ['pads', 'moves', 'hands'].map((id) => document.getElementById(id)), { rounds } = observe(readers, [47, 0, 34, 0]);
      const transitions = (el) => el.getAnimations().map((a) => [a.transitionProperty, a.effect.getKeyframes()[0][a.transitionProperty === 'top' ? 'top' : 'paddingTop'], a.effect.getKeyframes().at(-1)[a.transitionProperty === 'top' ? 'top' : 'paddingTop']]);
      const result = { rounds, pads: transitions(readers[0]), moves: transitions(readers[1]), other: other.getAnimations().map((a) => Math.round(a.currentTime) - running),
        fresh: fresh.getAnimations().map((a) => a.transitionProperty), marked: document.querySelectorAll('[data-exact-round]').length,
        takes: takes.getAnimations().map((a) => [a.transitionProperty, a.effect.getKeyframes()[0].height, a.effect.getKeyframes().at(-1).height]) };
      // The transition events a commit's change dispatches come at the next rendering step
      requestAnimationFrame(() => requestAnimationFrame(() => { window.result = { ...result, events }; }));
    }, 100);
  </script>`;

// A reader 100 below the document's top, in a page scrolled 100 down, beside one a scroller of its own holds scrolled
const SCROLLED = `<!doctype html><style>body { margin: 0; height: 3000px } div { box-sizing: border-box; position: absolute; left: 0; width: 100px }</style>
  <div id="page" style="top:100px;height:50px"></div>
  <div id="port" style="top:400px;height:100px;overflow:auto"><div style="position:static;height:1000px"><div id="held" style="position:static;margin-top:200px;height:50px"></div></div></div>
  <div id="panel" style="position:fixed;top:300px;height:100px;overflow:auto"><div style="position:static;height:1000px"><div id="pinned" style="position:static;margin-top:200px;height:50px"></div></div></div>
  <script type="module">
    import { observe } from '/observe.js';
    scrollTo(0, 100);
    const port = document.getElementById('port');
    port.scrollTop = 600;
    document.getElementById('panel').scrollTop = 500;
    const readers = ['page', 'held', 'pinned'].map((id) => document.getElementById(id));
    observe(readers, [47, 0, 34, 0]);
    window.result = readers.map((el) => el.style.getPropertyValue('--exact-safe-area-inset-top'));
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a reader is measured with every scroll offset at zero, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}scrolled`);
    await page.waitForFunction(() => window.result);
    // At rest 100, 600 and, in a fixed panel the page's scroll does not move, 500 below the top, clear of the 47-point
    // band, though scrolling has moved each under it
    expect(await page.evaluate(() => window.result)).toEqual(['0px', '0px', '0px']);
  } finally {
    await browser.close();
  }
}, 30_000);

// Readers placed by their containing blocks, each 10 below a top the page or a 300-point scroller lays it at, every
// scroller then scrolled by 100: an absolute box in a scroller that is not its containing block, which that scroller
// does not move; a fixed box whose containing block is a transformed scroller, which moves it; and a sticky box stuck
// to the page's top
const PLACED = `<!doctype html><style>body { margin: 0; height: 3000px } .r { height: 20px; padding-top: var(--exact-safe-area-inset-top, 0px) }
  .s { overflow: auto; height: 300px; width: 100px }</style>
  <div style="position:relative;height:300px"><div class="s" id="plain"><div style="height:1000px"></div>
    <div class="r" id="absolute" style="position:absolute;top:10px;width:50px"></div></div></div>
  <div class="s" id="transformed" style="position:absolute;top:0;left:200px;transform:translateX(0)"><div style="height:1000px"></div>
    <div class="r" id="fixed" style="position:fixed;top:10px;width:50px"></div></div>
  <div class="r" id="sticky" style="position:sticky;top:0;width:50px;margin-top:-290px;margin-left:120px"></div>
  <script type="module">
    import { observe } from '/observe.js';
    for (const id of ['plain', 'transformed']) document.getElementById(id).scrollTop = 100;
    scrollTo(0, 100);
    const readers = ['absolute', 'fixed', 'sticky'].map((id) => document.getElementById(id));
    observe(readers, [47, 0, 34, 0]);
    window.result = { inputs: readers.map((el) => el.style.getPropertyValue('--exact-safe-area-inset-top')),
      stuck: document.getElementById('sticky').getBoundingClientRect().top, marked: document.querySelectorAll('[data-exact-at-rest]').length };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a reader is measured where its containing blocks lay it, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}placed`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    // Each at rest 10 below its top, under 37 of the 47-point band, the absolute box at 10 on the page though its
    // scroller has moved, the fixed one 10 down its scroller, which moves it, and the sticky one laid out at 10, though stuck
    expect(r.inputs).toEqual(['37px', '37px', '37px']);
    // The sticky box is stuck again once the rounds end
    expect(r.stuck).toBe(0);
    expect(r.marked).toBe(0);
  } finally {
    await browser.close();
  }
}, 30_000);

// A 100-point scroller scrolled by 500 that reads its own input, its margin 37 less half of it, so its input settles at
// 20 under the 47-point band, and hands it to its content, 50 times its input tall, 1000 until the rounds write one: a
// new reader's first input, 0, and the rounds' before 20 leave too little to scroll to 500, but the settled one enough.
// Beside it the same reader around a scroller of its own content, scrolled by 500, which only the reader's input sizes;
// one around a box a class clips, which the JS compiler marks and a script scrolled by 500; and one whose text area is
// 900 tall at a first input of 0 and 100 once settled, over 1,200 of text scrolled by 500; a list box the same; and a
// route header's heading, which the host's sheet clips, scrolled 500 across a line the reader's input makes 1,000 wide
const CLAMPED = `<!doctype html><style>body { margin: 0 } .hid { overflow: hidden; height: 100px }
  textarea, select { display: block; width: 100px; padding: 0; border: 0; line-height: 20px; resize: none; height: var(--tall) }
  header h1 { overflow: hidden; width: 100px; margin: 0; white-space: nowrap }</style>
  <div id="port" style="overflow:auto;height:100px;width:100px;margin-top:calc(37px - 0.5 * var(--exact-safe-area-inset-top, 0px));--content:calc(50 * var(--exact-safe-area-inset-top, 20px))">
    <div style="height:var(--content)"></div></div>
  <div id="holder" style="position:absolute;left:150px;top:0;width:100px;margin-top:calc(37px - 0.5 * var(--exact-safe-area-inset-top, 0px));--content:calc(50 * var(--exact-safe-area-inset-top, 20px))">
    <div id="inner" style="overflow:auto;height:100px"><div style="height:var(--content)"></div></div></div>
  <div id="clipper" style="position:absolute;left:300px;top:0;width:100px;margin-top:calc(37px - 0.5 * var(--exact-safe-area-inset-top, 0px));--content:calc(50 * var(--exact-safe-area-inset-top, 20px))">
    <div id="hidden" class="hid" data-exact-safe-area="scroller"><div style="height:var(--content)"></div></div></div>
  <div id="writer" style="position:absolute;left:450px;top:0;width:100px;margin-top:calc(37px - 0.5 * var(--exact-safe-area-inset-top, 0px));--tall:calc(100px + 40 * (20px - var(--exact-safe-area-inset-top, 20px)))">
    <textarea id="text"></textarea></div>
  <div id="chooser" style="position:absolute;left:600px;top:0;width:100px;margin-top:calc(37px - 0.5 * var(--exact-safe-area-inset-top, 0px));--tall:calc(100px + 40 * (20px - var(--exact-safe-area-inset-top, 20px)))">
    <select multiple id="list"></select></div>
  <div id="headed" style="position:absolute;left:750px;top:0;width:100px;margin-top:calc(37px - 0.5 * var(--exact-safe-area-inset-top, 0px));--content:calc(50 * var(--exact-safe-area-inset-top, 20px))">
    <header><h1 id="heading"><span style="display:inline-block;width:var(--content)">Title</span></h1></header></div>
  <script type="module">
    import { observe } from '/observe.js';
    const $ = (id) => document.getElementById(id), port = $('port'), reader = port, inner = $('inner');
    $('text').value = Array.from({ length: 60 }, (_, i) => 'line ' + i).join('\\n');
    $('list').append(...Array.from({ length: 60 }, (_, i) => Object.assign(document.createElement('option'), { textContent: 'option ' + i })));
    for (const el of [port, inner, $('hidden'), $('text'), $('list')]) el.scrollTop = 500;
    $('heading').scrollLeft = 500;
    const listed = $('list').scrollTop;
    const { rounds, unsettled } = observe([reader, $('holder'), $('clipper'), $('writer'), $('chooser'), $('headed')], [47, 0, 34, 0]);
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    window.result = { rounds, unsettled: unsettled.length, input: reader.style.getPropertyValue('--exact-safe-area-inset-top'), scrolled: port.scrollTop, inner: inner.scrollTop,
      hidden: $('hidden').scrollTop, text: $('text').scrollTop, tall: $('text').getBoundingClientRect().height, list: $('list').scrollTop, listed, heading: $('heading').scrollLeft };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a round's clamped scroll offset is not kept, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}clamped`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    expect(r.rounds).toBeGreaterThan(1);
    // Within D9's half-pixel residual of 20, a window of one point either side at this slope
    expect(Math.abs(parseFloat(r.input) - 20)).toBeLessThan(1);
    // Where it was: the rounds read each round's offset, and the one they clamped is put back, inside a reader too
    expect(r.scrolled).toBe(500);
    expect(r.inner).toBe(500);
    // A clipping box and a text area, scrolled by a script and by a reader: both put back
    expect(r.hidden).toBe(500);
    expect(r.tall).toBeCloseTo(100, 0);
    expect(r.text).toBe(500);
    // A list box and a clipped heading
    expect(r.list).toBe(r.listed);
    expect(r.listed).toBeGreaterThan(400);
    expect(r.heading).toBe(500);
  } finally {
    await browser.close();
  }
}, 30_000);

// A reader its input moves through several heights in the rounds, watched by a ResizeObserver and an
// IntersectionObserver, which the browser delivers at its rendering step, after the task the rounds run in. A 45-point
// clip shows 15 of its 20 at rest, 0.75, and 32 of 54 once settled, so the settled layout crosses the observer's 0.7
const OBSERVED = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box; position: absolute; left: 0; width: 100px }</style>
  <div style="top:0;height:45px;overflow:hidden"><div id="grows" style="top:calc(30px - 0.5 * var(--exact-safe-area-inset-top, 0px));height:calc(20px + var(--exact-safe-area-inset-top, 0px))"></div></div>
  <script type="module">
    import { observe } from '/observe.js';
    const el = document.getElementById('grows'), sizes = [], crossings = [];
    await new Promise((done) => new ResizeObserver((entries) => { for (const e of entries) sizes.push(e.contentRect.height); done(); }).observe(el));
    await new Promise((done) => new IntersectionObserver((entries) => { for (const e of entries) crossings.push(e.intersectionRect.height); done(); }, { threshold: [0.7] }).observe(el));
    sizes.length = crossings.length = 0;
    const { rounds } = observe([el], [47, 0, 34, 0]);
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const box = el.getBoundingClientRect();
    window.result = { rounds, sizes, crossings, height: box.height, shown: 45 - box.top };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`observers see only the settled layout, not a round's, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}observed`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    // From 0 the rounds moved it through 37 points tall and more to its fixed point, 34 under the band and 54 tall: the
    // resize observer saw that height once, and no round's
    expect(r.rounds).toBeGreaterThan(1);
    expect(r.height).toBeCloseTo(54, 0);
    expect(r.sizes.length).toBe(1);
    expect(r.sizes[0]).toBeCloseTo(r.height, 2);
    // The intersection observer delivered the settled crossing once, 32 shown
    expect(r.crossings.length).toBe(1);
    expect(r.crossings[0]).toBeCloseTo(r.shown, 1);
    expect(r.shown).toBeCloseTo(32, 0);
  } finally {
    await browser.close();
  }
}, 30_000);

// Elements that read an input in their own style, one that only declares one for its children, and one that reads none
const READERS = `<!doctype html>
  <div id="declares" style="--exact-safe-area-inset-top: 30px">
    <div id="pads" style="padding-top: var(--exact-safe-area-inset-top, 5px)"></div>
    <div id="plain" style="padding-top: 5px"></div>
    <div id="keys" style="margin-bottom: calc(var(--exact-keyboard-inset-bottom, 0px) + 8px)"></div>
  </div>
  <script type="module">
    import { readers } from '/observe.js';
    window.result = readers().map((el) => el.id);
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`the readers are the elements whose own style reads an input, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    await page.goto(`${server.url.href}readers`);
    await page.waitForFunction(() => window.result);
    // In document order; the declaring ancestor and the plain one read nothing
    expect(await page.evaluate(() => window.result)).toEqual(['pads', 'keys']);
  } finally {
    await browser.close();
  }
}, 30_000);

// A box whose height transitions from 60 to 0 over two seconds, below a 10-point reader and above another; the rounds
// follow the reader after it up under the 47-point band as the box shrinks, frame by frame: at 1.4 s it lies near 28
const MOTION = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box; width: 100px }</style>
  <div id="before" style="height:10px;margin-left:var(--exact-safe-area-inset-left, 0px)"></div>
  <div id="column">
    <div id="shrinks" style="height:60px;transition:height 2s linear"></div>
    <div id="after" style="height:50px;padding-top:var(--exact-safe-area-inset-top, 0px)"></div>
  </div>
  <script type="module">
    import { observe, moving, follow } from '/observe.js';
    const ids = ['before', 'column', 'shrinks', 'after'], el = (id) => document.getElementById(id);
    const readers = ['before', 'after'].map(el);
    observe(readers, [47, 0, 34, 0]);
    const rest = el('after').style.getPropertyValue('--exact-safe-area-inset-top');
    el('shrinks').getBoundingClientRect();
    el('shrinks').style.height = '0px';
    const animated = moving(readers).map((r) => r.id);
    const stop = follow(readers, [47, 0, 34, 0], 0);
    // In flight, in a frame after the one follow() measured: the input is the band past where the reader now lies, give
    // or take the frame between
    await new Promise((r) => setTimeout(r, 1400));
    const flight = await new Promise((r) => requestAnimationFrame(() => r({ input: parseFloat(el('after').style.getPropertyValue('--exact-safe-area-inset-top')) || 0, top: el('after').getBoundingClientRect().top })));
    await new Promise((r) => setTimeout(r, 1000));
    stop();
    window.result = { rest, animated, flight, settled: el('after').style.getPropertyValue('--exact-safe-area-inset-top'), still: moving(readers).length };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`the rounds follow a reader an animation moves, each frame, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}motion`);
    await page.waitForFunction(() => window.result, null, { timeout: 10_000 });
    const r = await page.evaluate(() => window.result);
    // At rest below the box, clear of the band; the reader after the shrinking box moves, the one before it not
    expect(r.rest).toBe('0px');
    expect(r.animated).toEqual(['after']);
    // In flight inside the band, measured each frame: within two frames' travel, at most 2 points, of the band past it
    expect(r.flight.top).toBeLessThan(40);
    expect(r.flight.top).toBeGreaterThan(12);
    expect(Math.abs(r.flight.input - (47 - r.flight.top))).toBeLessThan(2.5);
    // Once the box has gone, the reader rests at 10, under 37 of the band, measured there without a commit
    expect(r.settled).toBe('37px');
    expect(r.still).toBe(0);
  } finally {
    await browser.close();
  }
}, 30_000);

// A reader an animation moves up from 200 to 20 in 0.3 s, under the 47-point band, and one after an element whose width
// animates for 1.2 s: the first's move ends while the second animation still runs. Before both in the document, a
// positioned block whose first child, a reader, is pinned to its bottom, its height its second child's, which shrinks
// from 100 to 20: the reader rises from 90 to 10, under 37 of the band. First of all, a reader in a centred block of
// its content's width, in a 165-point box, whose later child widens from 10 to 100: the block widens from 20 to 100, so
// the reader moves from 72.5 to 32.5, under 37.5 of a 70-point left band. In a 390-point right-to-left box, a block of
// its content's width, wider than the box, widening from 420 to 480: its reader, 100 into it, moves from 70 to 10,
// under 110 of a 120-point left band. And a block as wide as the page that centres its content (`align-content`), 200
// tall from -20, whose reader comes first and whose later child grows from 20 to 100: the reader rises from 60 to 20,
// under 27 of the band. The boxes come in the document in the order their animations end (0.2, 0.5, 0.8 and 1 s), so
// only its own animation moves each reader at its end
const HANDOFF = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box; position: absolute; width: 100px }</style>
  <div style="top:-20px;left:0;width:390px;height:200px;align-content:center">
    <div id="aligned" style="position:static;width:20px;height:20px;outline-offset:var(--exact-safe-area-inset-top, 0px)"></div>
    <div id="grows" style="position:static;width:20px;height:20px"></div></div>
  <div dir="rtl" style="top:740px;left:0;width:390px"><div dir="ltr" style="position:static;width:max-content">
    <div id="ruled" style="position:static;width:20px;height:20px;margin-left:100px;outline-offset:var(--exact-safe-area-inset-left, 0px)"></div>
    <div id="widens" style="position:static;width:420px;height:10px"></div></div></div>
  <div style="top:700px;left:0;width:165px"><div style="position:static;width:max-content;margin:0 auto">
    <div id="centred" style="position:static;width:20px;height:20px;outline-offset:var(--exact-safe-area-inset-left, 0px)"></div>
    <div id="wide" style="position:static;width:10px;height:10px"></div></div></div>
  <div style="position:absolute;top:0;left:300px;width:50px"><div style="position:relative">
    <div id="pinned" style="bottom:0;height:10px;width:10px;outline-offset:var(--exact-safe-area-inset-top, 0px)"></div>
    <div id="tall" style="position:static;height:100px"></div></div></div>
  <div id="mover" style="top:20px;left:0;height:20px;padding-top:var(--exact-safe-area-inset-top, 0px)"></div>
  <div id="long" style="top:500px;left:200px;height:10px"></div>
  <div id="rider" style="top:600px;left:200px;height:20px;padding-top:var(--exact-safe-area-inset-top, 0px)"></div>
  <script type="module">
    import { follow } from '/observe.js';
    const el = (id) => document.getElementById(id);
    el('mover').animate([{ top: '200px' }, { top: '20px' }], { duration: 300 });
    el('long').animate([{ width: '10px' }, { width: '60px' }], { duration: 1200 });
    el('tall').animate([{ height: '100px' }, { height: '20px' }], { duration: 1000, fill: 'forwards' });
    el('wide').animate([{ width: '10px' }, { width: '100px' }], { duration: 800, fill: 'forwards' });
    el('widens').animate([{ width: '420px' }, { width: '480px' }], { duration: 500, fill: 'forwards' });
    el('grows').animate([{ height: '20px' }, { height: '100px' }], { duration: 200, fill: 'forwards' });
    const stop = follow(['mover', 'rider', 'pinned', 'aligned'].map(el), [47, 0, 34, 0], 0), across = follow([el('centred')], [0, 0, 0, 70], 0);
    const wider = follow([el('ruled')], [0, 0, 0, 120], 0);
    await new Promise((r) => setTimeout(r, 1500));
    stop();
    across();
    wider();
    const top = (id) => el(id).style.getPropertyValue('--exact-safe-area-inset-top');
    const left = (id) => parseFloat(el(id).style.getPropertyValue('--exact-safe-area-inset-left')) || 0;
    window.result = { mover: top('mover'), rider: top('rider'), pinned: top('pinned'), centred: left('centred'), ruled: left('ruled'), aligned: top('aligned') };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a reader whose move ends while another runs is measured where it rests, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}handoff`);
    await page.waitForFunction(() => window.result, null, { timeout: 10_000 });
    // At rest at 20 under 27 of the band; the other clear of it; the pinned child at 10 under 37
    const r = await page.evaluate(() => window.result);
    expect(r).toMatchObject({ mover: '27px', rider: '0px', pinned: '37px', aligned: '27px' });
    // The right-to-left block's reader, moved left as its block widens past the box
    expect(r.ruled).toBeCloseTo(110, 0);
    // The centred block's reader, moved by its later sibling's widening
    expect(r.centred).toBeCloseTo(37.5, 1);
  } finally {
    await browser.close();
  }
}, 30_000);

// A reader at `top: calc(32px - 0.5 * input)` under the 47-point band, whose fixed point is 30: the first round's
// substitution gives 15 and the secant through it 30, in two rounds. A `display: contents` reader new to the page inside
// one that reads 300 of a keyboard, declaring its own length from the keyboard's input with a 7-point fallback, which its
// child's height reads. And a call with no readers
const SOLVE = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box; position: absolute; left: 0; width: 100px }</style>
  <div id="half" style="top:calc(32px - 0.5 * var(--exact-safe-area-inset-top, 0px));height:20px;outline-offset:var(--exact-safe-area-inset-top, 0px)"></div>
  <div id="holder" style="top:500px;height:300px;padding-bottom:var(--exact-keyboard-inset-bottom, 0px)">
    <div id="boxless" style="display:contents;--local:var(--exact-keyboard-inset-bottom, 7px)"><div id="child" style="position:static;height:var(--local)"></div></div></div>
  <script type="module">
    import { observe } from '/observe.js';
    const $ = (id) => document.getElementById(id), mutations = [];
    const watch = new MutationObserver((records) => mutations.push(...records));
    watch.observe(document.documentElement, { childList: true, subtree: true });
    observe([]);
    watch.takeRecords().forEach((r) => mutations.push(r));
    watch.disconnect();
    const { rounds } = observe([$('half')], [47, 0, 0, 0]);
    observe([$('holder'), $('boxless')], [47, 0, 0, 0], 300);
    window.result = { rounds, half: $('half').style.getPropertyValue('--exact-safe-area-inset-top'), holder: $('holder').style.getPropertyValue('--exact-keyboard-inset-bottom'),
      child: $('child').getBoundingClientRect().height, empty: mutations.length };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`the solve takes the secant from its first round, and a new boxless reader reads its fallback, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}solve`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    expect(r.rounds).toBe(2);
    expect(parseFloat(r.half)).toBeCloseTo(30, 1);
    // The holder reads the whole keyboard; the boxless reader's child its own fallback, not the holder's 300
    expect(r.holder).toBe('300px');
    expect(r.child).toBe(7);
    // No reader, no probe put in the document
    expect(r.empty).toBe(0);
  } finally {
    await browser.close();
  }
}, 30_000);

// A centred block of its content's width in a 390-point page, a 20-point reader first and a later child 390 wide, which
// holds that width for a quarter of 0.8 s and then narrows to 190: at first the block fills the page and no animation
// moves the reader, then it narrows and moves the reader from 0 to 100, out of a 70-point left band
const NARROWING = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box }</style>
  <div style="width:max-content;margin:0 auto"><div id="reader" style="width:20px;height:20px;outline-offset:var(--exact-safe-area-inset-left, 0px)"></div>
    <div id="later" style="width:390px;height:10px"></div></div>
  <script type="module">
    import { observe, follow } from '/observe.js';
    const $ = (id) => document.getElementById(id), read = () => $('reader').style.getPropertyValue('--exact-safe-area-inset-left');
    observe([$('reader')], [0, 0, 0, 70]);
    const before = read();
    $('later').animate([{ width: '390px' }, { width: '390px', offset: 0.25 }, { width: '190px' }], { duration: 800, fill: 'forwards' });
    const stop = follow([$('reader')], [0, 0, 0, 70], 0);
    await new Promise((r) => setTimeout(r, 1300));
    stop();
    window.result = { before, after: read(), at: $('reader').getBoundingClientRect().left };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`an animation that comes to move a reader is followed, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}narrowing`);
    await page.waitForFunction(() => window.result, null, { timeout: 10_000 });
    // In the band at 0, then at rest at 100, out of it
    expect(await page.evaluate(() => window.result)).toEqual({ before: '70px', after: '0px', at: 100 });
  } finally {
    await browser.close();
  }
}, 30_000);

// A reader at the bottom under a 300-point keyboard, one above it, asked with the keyboard and with none showing
const KEYBOARD = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box; position: absolute; left: 0; width: 100px }</style>
  <div id="foot" style="top:600px;height:200px;padding-bottom:max(var(--exact-keyboard-inset-bottom, 7px), var(--exact-keyboard-undocked-inset-bottom, 7px))"></div>
  <div id="head" style="top:0;height:100px"></div>
  <script type="module">
    import { observe } from '/observe.js';
    const readers = ['foot', 'head'].map((id) => document.getElementById(id));
    const inputs = (el) => ['keyboard-inset', 'keyboard-undocked-inset'].map((f) => ['top', 'right', 'bottom', 'left'].map((e) => el.style.getPropertyValue('--exact-' + f + '-' + e)));
    observe(readers, [0, 0, 0, 0], 300);
    const shown = { foot: inputs(readers[0]), head: inputs(readers[1]), padding: parseFloat(getComputedStyle(readers[0]).paddingBottom) };
    observe(readers, [0, 0, 0, 0], 0);
    window.result = { shown, none: inputs(readers[0]), padding: parseFloat(getComputedStyle(readers[0]).paddingBottom) };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`each reader takes the keyboard's depth from its own bottom, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}keyboard`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    // The reader at the bottom: the keyboard's 300 on its bottom edge, in both families, not limited by its 200-point
    // height, as a native view's guide is not; the one above: none
    expect(r.shown.foot).toEqual([['', '', '300px', ''], ['', '', '300px', '']]);
    expect(r.shown.padding).toBe(300);
    expect(r.shown.head).toEqual([['0px', '0px', '0px', '0px'], ['0px', '0px', '0px', '0px']]);
    // With no keyboard showing, each is 0, as the kernel gives a placed box; the edges it does not read stay unwritten
    expect(r.none).toEqual([['', '', '0px', ''], ['', '', '0px', '']]);
    expect(r.padding).toBe(0);
  } finally {
    await browser.close();
  }
}, 30_000);

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a round's values start no transition, and the settled inputs one each, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}transitions`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    // The rounds read the layout, not a transition's first frame, though the page's sheet makes the mover's important:
    // a new reader starts at 0 (D9), which puts the mover at 60, clear of the band, where it holds in the first round
    expect(r.rounds).toBe(1);
    expect(r.pads).toEqual([['padding-top', '5px', '47px']]);
    expect(r.moves).toEqual([['top', '0px', '60px']]);
    expect(r.other).toEqual([0]);
    expect(r.fresh).toEqual(['opacity'], "the page's own change before the rounds starts its transition");
    expect(r.marked).toBe(0);
    // A reader that hands its observation to a descendant (a custom property it declares) is marked with its subtree:
    // the descendant takes one transition, to the settled value, and none from a round's
    expect(r.takes).toEqual([['height', '5px', '47px']]);
    expect(r.events).toEqual(['transitionrun height']);
  } finally {
    await browser.close();
  }
}, 30_000);

for (const engine of ['chromium', 'webkit', 'firefox']) test(`each reader takes its own observation, in rounds until it holds or the bound, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    if (engine === 'chromium') {
      const cdp = await page.context().newCDPSession(page);
      await cdp.send('Emulation.setSafeAreaInsetsOverride', { insets: { top: 47, bottom: 34, left: 0, right: 0 } });
    }
    await page.goto(engine === 'chromium' ? server.url.href : `${server.url.href}?band=47,0,34,0`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    expect(r.band).toEqual([47, 0, 34, 0]);
    // Only the edges a reader reads are written
    expect(r.inputs.top).toEqual(['47px', '', '', '']);
    expect(r.padding.top).toBe(47);
    expect(r.inputs.low).toEqual(['0px', '', '', '']);
    expect(r.padding.low).toBe(0);
    // 60 less its own depth under the bar: 13, then 26, 39, 52, past the bar, where it rests
    expect(r.tops.settles).toBe(60);
    expect(r.inputs.settles[0]).toBe('0px');
    // At twice its input, 47 less that input: substitution swings between 0 and 47, the secant steps find 47 / 3
    expect(parseFloat(r.inputs.swings[0])).toBeCloseTo(47 / 3, 0);
    // 47 below 10 and 0 above it, which no input holds: unsettled at the bound
    expect(r.rounds).toBe(24);
    expect(r.unsettled).toEqual(['jumps']);
    // Its own, the bar's 47 from its top, not the 30 an ancestor says
    expect(r.inputs.inside[0]).toBe('47px');
    expect(r.padding.inside).toBe(47);
    // A reader with no box keeps the input it has, here none, so its fallback applies (D9)
    expect(r.gone).toEqual(['', '', '', '']);
  } finally {
    await browser.close();
  }
}, 30_000);

// A reader at the top whose padding's transition has no duration but a one-second delay, which still starts a
// transition (CSS Transitions 1: its combined duration is above 0), so the rounds take the full path and the settled
// input reaches transition processing once, delayed, rather than landing as the rounds wrote it
const DELAYED = `<!doctype html><style>body { margin: 0 }</style>
  <div id="pads" style="position:absolute;top:0;width:50px;height:100px;padding-top:var(--exact-safe-area-inset-top, 5px);transition:padding-top 0s 1s"></div>
  <script type="module">
    import { observe } from '/observe.js';
    const el = document.getElementById('pads');
    getComputedStyle(el).paddingTop;
    observe([el], [47, 0, 34, 0]);
    window.result = { running: el.getAnimations().map((a) => a.transitionProperty), shown: getComputedStyle(el).paddingTop };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a delayed transition with no duration still takes the settled input, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}delayed`);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({ running: ['padding-top'], shown: '5px' });
  } finally {
    await browser.close();
  }
}, 30_000);

// A reader whose four transitioning properties outnumber its two durations and three delays: the lists repeat to the
// properties' length, so its fourth, `padding-top`, takes 5s with no delay and its settled input transitions
const LISTED = `<!doctype html><style>body { margin: 0 }</style>
  <div id="pads" style="position:absolute;top:0;width:50px;height:100px;padding-top:var(--exact-safe-area-inset-top, 5px);transition-property:opacity, color, margin-top, padding-top;transition-duration:0s, 5s;transition-delay:0s, -5s, -5s"></div>
  <script type="module">
    import { observe } from '/observe.js';
    const el = document.getElementById('pads');
    getComputedStyle(el).paddingTop;
    observe([el], [47, 0, 34, 0]);
    window.result = { running: el.getAnimations().map((a) => a.transitionProperty) };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a transition the time lists repeat to still takes the settled input, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}listed`);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({ running: ['padding-top'] });
  } finally {
    await browser.close();
  }
}, 30_000);
