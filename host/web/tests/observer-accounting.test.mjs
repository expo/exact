// @ref LLP 1123 D17, "The web: observers" — the host's account of commit samples in Chromium, WebKit and Firefox: a
// `ResizeObserver` and an `IntersectionObserver` delivery that a commit's settled layout caused is accounted for by the
// sample taken in it, and one that also carries an author's change, or a scroll, is outside it.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    if (/^\/[\w-]+\.js$/.test(path)) {
      return new Response(Bun.file(resolve(import.meta.dir, '../navigation' + path)), { headers: { 'content-type': 'text/javascript' } });
    }
    const pages = { '/clipped': CLIPPED, '/edge': EDGE, '/nested': NESTED, '/child': CHILD };
    return new Response(pages[path] ?? PAGE, { headers: { 'content-type': 'text/html' } });
  } });
});
afterAll(() => server.stop(true));

// A reader 20 tall plus its bottom input, with 2 of padding and 1 of border, 20 of its 26 in view above an 800-point
// viewport's bottom under a 34-point band, watched by both observers: the rounds grow it by the band to 60, so 20 of 60
// show, which commit 1 samples; then the author widens it, and then the page scrolls it
const PAGE = `<!doctype html><style>body { margin: 0; height: 3000px }</style>
  <div id="grows" style="position:absolute;top:780px;left:0;width:100px;padding:2px;border:1px solid;box-sizing:content-box;
    height:calc(20px + var(--exact-safe-area-inset-bottom, 0px))"></div>
  <script type="module">
    import { observe } from '/observe.js';
    import { account } from '/accounting.js';
    const el = document.getElementById('grows'), books = account(), frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const resized = [], crossed = [];
    new ResizeObserver((entries) => resized.push(...entries)).observe(el);
    const io = new IntersectionObserver((entries) => crossed.push(...entries), { threshold: [0, 0.25, 0.5, 0.75, 1] });
    io.observe(el);
    await frame();
    resized.length = crossed.length = 0;
    observe([el], [47, 0, 34, 0]);
    books.sample([el], 1);
    await frame();
    // The commit's crossing as an observer with a scroll margin, or one tracking visibility, would deliver it: the
    // account reads only an observer's options, and not every engine has these
    const margined = { root: null, rootMargin: '0px 0px 0px 0px', scrollMargin: '0px 0px 20px 0px' }, tracking = { root: null, rootMargin: '0px', trackVisibility: true };
    const commit = { resized: resized.length, crossed: crossed.length, outside: books.outside([...resized, ...crossed], io).length, unbound: books.outside(crossed).length,
      margined: books.outside(crossed, margined).length, tracking: books.outside(crossed, tracking).length, generation: books.generation(el) };
    resized.length = 0;
    el.style.width = '150px';
    await frame();
    const author = { resized: resized.length, outside: books.outside(resized).length };
    crossed.length = 0;
    scrollTo(0, 60);
    await frame();
    const scrolled = { crossed: crossed.length, outside: books.outside(crossed, io).length };
    window.result = { commit, author, scrolled, height: el.getBoundingClientRect().height };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a commit's sample accounts for the deliveries it caused, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.result, null, { timeout: 10_000 });
    const r = await page.evaluate(() => window.result);
    // Grown by the band's 34 to 54, plus 4 of padding and 2 of border
    expect(r.height).toBe(60);
    // The commit's resize and its crossing of the observer's thresholds are the sample's
    // Given the observer, the account takes both as the commit's; without it, the crossing could be any observer's
    expect(r.commit).toEqual({ resized: 1, crossed: 1, outside: 0, unbound: 1, margined: 1, tracking: 1, generation: 1 });
    // The author's widening is outside it, as is the scroll that moves it partly out of view
    expect(r.author).toEqual({ resized: 1, outside: 1 });
    expect(r.scrolled.crossed).toBeGreaterThan(0);
    expect(r.scrolled.outside).toBe(r.scrolled.crossed);
  } finally {
    await browser.close();
  }
}, 30_000);

// A box in full view inside a clipping box, sampled; then the clip shrinks to 40 of its 100, which changes the box's
// intersection though its own box stays where it was, a change from outside the commit. The same inside a 50-point clip
// that an ancestor scales by 2, half of its 100-point box showing, the clip then narrowed to 25: the sample cannot tell a
// scaled clip, so the crossing is outside. And a fixed box under a clipping ancestor that does not contain it, which
// does not clip it: the sample taken where it was first delivered accounts for that delivery
const CLIPPED = `<!doctype html><style>body { margin: 0 }</style>
  <div id="clip" style="position:absolute;top:0;left:0;width:200px;height:200px;overflow:hidden">
    <div id="box" style="position:absolute;top:0;left:0;width:100px;height:100px"></div></div>
  <div style="position:absolute;top:300px;left:0;transform:scale(2);transform-origin:0 0">
    <div id="scaledclip" style="width:50px;height:50px;overflow:hidden"><div id="scaled" style="width:100px;height:50px"></div></div></div>
  <div style="position:absolute;top:500px;left:0;width:50px;height:50px;overflow:hidden"><div id="escaped" style="position:fixed;top:500px;left:0;width:100px;height:50px"></div></div>
  <script type="module">
    import { account } from '/accounting.js';
    const box = document.getElementById('box'), books = account(), frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const crossed = [];
    const io = new IntersectionObserver((entries) => crossed.push(...entries), { threshold: [0, 0.4, 1] });
    io.observe(box);
    await frame();
    crossed.length = 0;
    books.sample([box], 1);
    document.getElementById('clip').style.height = '40px';
    await frame();
    const result = { crossed: crossed.map((e) => Math.round(e.intersectionRatio * 10) / 10), outside: books.outside(crossed, io).length };
    const $ = (id) => document.getElementById(id), seen = [], at = [];
    const watching = new IntersectionObserver((entries) => seen.push(...entries), { threshold: [0, 0.4, 1] });
    watching.observe($('scaled'));
    watching.observe($('escaped'));
    await frame();
    books.sample([$('scaled'), $('escaped')], 2);
    const first = seen.splice(0).filter((e) => e.target === $('escaped'));
    $('scaledclip').style.width = '25px';
    await frame();
    window.result = { ...result, scaled: seen.map((e) => Math.round(e.intersectionRatio * 100) / 100), scaledOutside: books.outside(seen, watching).length,
      escaped: books.outside(first, watching).length };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a clip's change of a box's intersection is outside the commit, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}clipped`);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({ crossed: [0.4], outside: 1, scaled: [0.25], scaledOutside: 1, escaped: 0 });
  } finally {
    await browser.close();
  }
}, 30_000);

// Clips the sample cannot see, whose changes are outside: a box in a document inside an iframe, which a 50-point clip in
// the parent cuts to half and then, widened, shows whole; a box in a clip that `overflow-clip-margin` widens by 25, then
// by none; and a fixed box under a clip path that hides its right half, then none. Every entry each delivers is outside
const NESTED = `<!doctype html><style>body { margin: 0 }</style>
  <div id="pclip" style="width:50px;height:100px;overflow:hidden"><iframe src="/child" style="width:100px;height:100px;border:0"></iframe></div>
  <div style="position:absolute;top:300px;left:0;overflow:clip;overflow-clip-margin:25px;width:50px;height:50px" id="margin"><div id="wide" style="width:100px;height:50px"></div></div>
  <div style="position:absolute;top:400px;left:0;clip-path:inset(0 50px 0 0);width:100px;height:10px" id="path"><div id="fixed" style="position:fixed;top:600px;left:0;width:100px;height:50px"></div></div>
  <script type="module">
    import { account } from '/accounting.js';
    const $ = (id) => document.getElementById(id), books = account(), frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const seen = [], io = new IntersectionObserver((entries) => seen.push(...entries), { threshold: [0, 0.4, 0.6, 0.9, 1] });
    io.observe($('wide'));
    io.observe($('fixed'));
    await frame();
    books.sample([$('wide'), $('fixed')], 1);
    seen.length = 0;
    $('margin').style.overflowClipMargin = '0px';
    $('path').style.clipPath = 'none';
    await frame();
    window.local = { delivered: seen.length, outside: books.outside(seen, io).length };
  </script>`;
const CHILD = `<!doctype html><style>body { margin: 0 }</style><div id="box" style="width:100px;height:100px"></div>
  <script type="module">
    import { account } from '/accounting.js';
    const box = document.getElementById('box'), books = account(), frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const seen = [], io = new IntersectionObserver((entries) => seen.push(...entries), { threshold: [0, 0.5, 1] });
    io.observe(box);
    await frame();
    books.sample([box], 1);
    seen.length = 0;
    parent.document.getElementById('pclip').style.width = '100px';
    await frame();
    parent.result = { ...parent.local, nested: seen.map((e) => e.intersectionRatio), nestedOutside: books.outside(seen, io).length };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a clip the sample cannot see leaves its deliveries outside, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}nested`);
    await page.waitForFunction(() => window.result && window.local, null, { timeout: 10_000 });
    const r = await page.evaluate(() => window.result);
    // The parent's clip widened: the child's box shows whole, which its document's sample cannot tell
    expect(r.nested).toEqual([1]);
    expect(r.nestedOutside).toBe(1);
    // What the clip margin's and the clip path's changes deliver, as each engine draws them, is all outside
    expect(r.outside).toBe(r.delivered);
  } finally {
    await browser.close();
  }
}, 30_000);

// A box of no area moved from just below the viewport to its edge, which an IntersectionObserver takes as intersecting;
// and a content-box scroller with scrollbars that take room (the engine's own scrollbars styled to), its content box less
// their gutters, which a commit resizes
const EDGE = `<!doctype html><style>body { margin: 0 } #port::-webkit-scrollbar { width: 15px; height: 15px }</style>
  <div id="dot" style="position:absolute;left:0;top:800.4px;width:0;height:0"></div>
  <div id="port" style="position:absolute;top:0;left:100px;width:100px;height:100px;padding:5px;box-sizing:content-box;overflow:scroll"><div style="width:400px;height:400px"></div></div>
  <script type="module">
    import { account } from '/accounting.js';
    const dot = document.getElementById('dot'), port = document.getElementById('port'), books = account();
    const frame = () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    const crossed = [], resized = [];
    const io = new IntersectionObserver((entries) => crossed.push(...entries));
    io.observe(dot);
    new ResizeObserver((entries) => resized.push(...entries)).observe(port);
    await frame();
    crossed.length = resized.length = 0;
    books.sample([dot], 1);
    dot.style.top = '800px';
    port.style.width = '120px';
    books.sample([port], 1);
    await frame();
    window.result = { crossed: crossed.map((e) => e.isIntersecting), outside: books.outside(crossed, io).length, resized: resized.length, resizedOutside: books.outside(resized).length };
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`an edge crossing is outside the commit, and a scroller's gutter is counted once, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}edge`);
    await page.waitForFunction(() => window.result);
    expect(await page.evaluate(() => window.result)).toEqual({ crossed: [true], outside: 1, resized: 1, resizedOutside: 0 });
  } finally {
    await browser.close();
  }
}, 30_000);
