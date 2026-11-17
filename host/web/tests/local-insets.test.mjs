// @ref LLP 1123 D9 — `var(--exact-safe-area-inset-*)` on the web: registered
// lengths that inherit, the document's `env()` at the root, and the neutral
// look's sheet's own (none at its top; none at all as a wide window's card).
// Chrome's safe-area override stands in for a phone's insets. LLP 1123 D5:
// an author's custom properties, as css.rs writes them, read as the kernel
// reads them.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';

const WEB = resolve(import.meta.dir, '..');
const style = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
// What css.rs writes for the rows, as the kernel serializes them.
const probe = 'width:10px;height:var(--exact-safe-area-inset-top);padding-bottom:max(4px, var(--exact-safe-area-inset-bottom));margin-top:calc(-1 * var(--exact-safe-area-inset-top))';
let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    // The JS target's declaration updater, as a bound row runs it
    if (new URL(request.url).pathname === '/declarations.js') {
      return new Response(Bun.file(resolve(WEB, '../web-js/declarations.js')), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(`<!doctype html><meta name="viewport" content="width=device-width, viewport-fit=cover"><style>${style}</style>
      <div id="exact-root">
        <div style="padding-top:60px"><div id="page" style="${probe}"></div></div>
        <div navigationPresentation="modal"><div id="sheet" style="${probe}"></div></div>
        <div style="--exact-safe-area-inset-top:calc(var(--exact-safe-area-inset-bottom) + 8px)"><div><div id="own" style="${probe}"></div></div></div>
        <div style="width:200px;--gap:8px;--top:var(--exact-safe-area-inset-top);--w:50%">
          <div id="fallback" style="margin-top:var(--missing, calc(10% + 2px));height:var(--gap)"></div>
          <div style="--exact-safe-area-inset-top:30px;width:100px"><div id="capture" style="height:var(--top);width:var(--w)"></div></div>
        </div>
        <div id="bound" data-row="--gap: calc(1px /*;*/ + 11px); --a\\?: 9px"><div id="bound-reader" style="height:var(--gap, 1px);width:var(--a\\?, 1px)"></div></div>
      </div>
      <script type="module">
        import { declare } from '/declarations.js';
        const bound = document.getElementById('bound');
        declare(bound, bound.dataset.row);
        document.body.dataset.ready = '1';
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
});
afterAll(() => server?.stop(true));

test('a box reads its container\'s insets, a sheet\'s its own, and an author\'s value', async () => {
  const browser = await suiteBrowser('chromium');
  try {
    const read = async (width) => {
      const page = await browser.newPage({ viewport: { width, height: 800 } });
      const cdp = await page.context().newCDPSession(page);
      await cdp.send('Emulation.setSafeAreaInsetsOverride', { insets: { top: 47, bottom: 34, left: 0, right: 0 } });
      await page.goto(server.url.href);
      await page.waitForSelector('body[data-ready]');
      return page.evaluate(() => Object.fromEntries([
        ...['page', 'sheet', 'own'].map((id) => [id, ['height', 'paddingBottom', 'marginTop']]),
        ['fallback', ['marginTop', 'height']],
        ['capture', ['height', 'width']],
        ['bound-reader', ['height', 'width']],
      ].map(([id, props]) => {
        const s = getComputedStyle(document.getElementById(id));
        return [id, props.map(p => parseFloat(s[p]))];
      })));
    };
    // An author's custom properties (LLP 1123 D5): a fallback stands alone, 10% of the 200px block plus 2px; a
    // capture holds the declaring box's inset, not the nearer override's 30; a percentage is the reader's own; the JS
    // target's updater writes a bound row, a comment and an escaped name among it
    const custom = { fallback: [22, 8], capture: [47, 50], 'bound-reader': [12, 9] };
    // Narrow: the page reads env(); the sheet starts below the status bar and reaches the bottom.
    // An author's own value, as css.rs writes it, is what its subtree reads.
    expect(await read(390)).toEqual({ page: [47, 34, -47], sheet: [0, 34, 0], own: [42, 34, -42], ...custom });
    // Wide: the sheet is a card clear of every edge.
    expect(await read(1000)).toEqual({ page: [47, 34, -47], sheet: [0, 4, 0], own: [42, 34, -42], ...custom });
  } finally {
    await browser.close();
  }
});
