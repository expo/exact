// @ref LLP 1123 D9 — `var(--exact-safe-area-inset-*)` on the web: registered
// lengths that inherit, the document's `env()` at the root, and the neutral
// look's sheet's own (none at its top; none at all as a wide window's card).
// Chrome's safe-area override stands in for a phone's insets.
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
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch() {
    return new Response(`<!doctype html><meta name="viewport" content="width=device-width, viewport-fit=cover"><style>${style}</style>
      <div id="exact-root">
        <div style="padding-top:60px"><div id="page" style="${probe}"></div></div>
        <div navigationPresentation="modal"><div id="sheet" style="${probe}"></div></div>
        <div style="--exact-safe-area-inset-top:calc(var(--exact-safe-area-inset-bottom) + 8px)"><div><div id="own" style="${probe}"></div></div></div>
      </div>`, { headers: { 'content-type': 'text/html' } });
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
      return page.evaluate(() => Object.fromEntries(['page', 'sheet', 'own'].map((id) => {
        const s = getComputedStyle(document.getElementById(id));
        return [id, [s.height, s.paddingBottom, s.marginTop].map(parseFloat)];
      })));
    };
    // Narrow: the page reads env(); the sheet starts below the status bar and reaches the bottom.
    // An author's own value, as css.rs writes it, is what its subtree reads.
    expect(await read(390)).toEqual({ page: [47, 34, -47], sheet: [0, 34, 0], own: [42, 34, -42] });
    // Wide: the sheet is a card clear of every edge.
    expect(await read(1000)).toEqual({ page: [47, 34, -47], sheet: [0, 4, 0], own: [42, 34, -42] });
  } finally {
    await browser.close();
  }
});
