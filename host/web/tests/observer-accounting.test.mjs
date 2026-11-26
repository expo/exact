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
    return new Response(PAGE, { headers: { 'content-type': 'text/html' } });
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
    new IntersectionObserver((entries) => crossed.push(...entries), { threshold: [0, 0.25, 0.5, 0.75, 1] }).observe(el);
    await frame();
    resized.length = crossed.length = 0;
    observe([el], [47, 0, 34, 0]);
    books.sample([el], 1);
    await frame();
    const commit = { resized: resized.length, crossed: crossed.length, outside: books.outside([...resized, ...crossed]).length, generation: books.generation(el) };
    resized.length = 0;
    el.style.width = '150px';
    await frame();
    const author = { resized: resized.length, outside: books.outside(resized).length };
    crossed.length = 0;
    scrollTo(0, 60);
    await frame();
    const scrolled = { crossed: crossed.length, outside: books.outside(crossed).length };
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
    expect(r.commit).toEqual({ resized: 1, crossed: 1, outside: 0, generation: 1 });
    // The author's widening is outside it, as is the scroll that moves it partly out of view
    expect(r.author).toEqual({ resized: 1, outside: 1 });
    expect(r.scrolled.crossed).toBeGreaterThan(0);
    expect(r.scrolled.outside).toBe(r.scrolled.crossed);
  } finally {
    await browser.close();
  }
}, 30_000);
