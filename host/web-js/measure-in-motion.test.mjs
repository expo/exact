// `measure` while motion plays (GitHub #1461): the web answers what the kernel answers, the box's
// size at `height: auto` with every presented height at its end, and leaves the motion playing.
// A: a box mounted under a clipping ancestor whose `-exact-layout-transition` plays its growth.
// B: a box whose `max-height: 100%` is of an ancestor in a CSS `height` transition. Own: a box in
// a height transition of its own. Pinned: a page `!important` height, as a sheet's detent. Font:
// the one refusal left, under a CSS animation of a font axis. Then the layout transition's own
// hold on a clipping box leaves its layout alone. Build exact-web-js once before running this
// test (docs/testing.md).
import { expect } from 'bun:test';
import { EXE } from '../../scripts/paths.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { withLook } from '../web/look.mjs';
import { engineTests } from '../../scripts/coverage.mjs';

const engineTest = await engineTests(['chromium', 'firefox', 'webkit']);

const SOURCE = `keyframes heavy
  from font-weight=400
  to font-weight=700

component App
  state showA = false
  state showB = false
  state tall = false
  state a = "none"
  state b = "none"
  state own = "none"
  state pinned = "none"
  state font = "none"
  action mountA
    showA = true
  action aSized(w: number, h: number)
    let m = measure("a")
    a = a != "none" ? a : m.unavailable ? "unavailable" : \`\${m.height}\`
  action mountB
    showB = true
  action bSized(w: number, h: number)
    let m = measure("b")
    b = b != "none" ? b : m.unavailable ? "unavailable" : \`\${m.height}\`
  action grow
    tall = true
  action readOwn
    let m = measure("own")
    own = m.unavailable ? "unavailable" : \`\${m.height}\`
  action readFont
    let m = measure("font")
    font = m.unavailable ? "unavailable" : \`\${m.height}\`
  action readPinned
    let m = measure("pinned")
    pinned = m.unavailable ? "unavailable" : \`\${m.height}\`
  view
    column
      button "A" testId="mount-a" press=mountA
      button "B" testId="mount-b" press=mountB
      button "Grow" testId="grow" press=grow
      button "Own" testId="read-own" press=readOwn
      button "Pinned" testId="read-pinned" press=readPinned
      button "Font" testId="read-font" press=readFont
      text a testId="a"
      text b testId="b"
      text own testId="own-read"
      text pinned testId="pinned-read"
      text font testId="font-read"
      column testId="a-parent" overflow="hidden" -exact-layout-transition="20000ms linear"
        if showA
          column id="a" resize=aSized
            text "A" height=60
      column height=(showB ? 160 : 40) transition="height 20000ms linear"
        if showB
          column id="b" max-height="100%" resize=bSized
            text "B" height=200 flex-shrink=0
      column id="own" testId="own" height=(tall ? 300 : 100) transition="height 20000ms linear"
        text "Own" height=150 flex-shrink=0
      // The page pins its height, as a sheet's detent does (neutral.css).
      column id="pinned"
        text "Pinned" height=90 flex-shrink=0
      // A CSS animation of a font axis reflows the text, and its keyframes are the sheet's.
      column id="font" animation="heavy 20s infinite"
        text "Font"
`;

for (const engine of ['chromium', 'firefox', 'webkit']) {
  engineTest[engine](`${engine}: measure answers through a layout transition, an ancestor's height transition and its own`, async () => {
    const app = await open(engine, SOURCE);
    try {
      const { page } = app;
      const read = id => page.waitForFunction(id => {
        const text = document.querySelector(`[data-testid="${id}"]`).textContent;
        return text !== 'none' && text;
      }, id, { timeout: 10000 }).then(handle => handle.jsonValue());
      const playing = id => page.evaluate(id => document.querySelector(`[data-testid="${id}"]`).getAnimations({ subtree: true }).filter(a => a.playState === 'running').length, id);

      await page.click('[data-testid="mount-a"]');
      // Read in the box's first size event, while the parent's layout transition plays its growth.
      expect(await read('a')).toBe('60');
      expect(await playing('a-parent')).toBeGreaterThan(0);

      await page.click('[data-testid="mount-b"]');
      // The parent is at 40 going to 160: the box's `max-height` is of 160.
      expect(await read('b')).toBe('160');
      expect(await page.evaluate(() => parseFloat(getComputedStyle(document.getElementById('b').parentElement).height))).toBeLessThan(160);

      await page.click('[data-testid="read-pinned"]');
      expect(await read('pinned-read')).toBe('90');

      await page.click('[data-testid="read-font"]');
      expect(await read('font-read')).toBe('unavailable');

      await page.click('[data-testid="grow"]');
      await page.waitForFunction(() => document.querySelector('[data-testid="own"]').getAnimations().length === 1);
      await page.click('[data-testid="read-own"]');
      expect(await read('own-read')).toBe('150');
      // The read left the box's own transition playing.
      expect(await page.evaluate(() => {
        const el = document.querySelector('[data-testid="own"]'), [t] = el.getAnimations();
        return [el.getAnimations().length, t?.playState, parseFloat(getComputedStyle(el).height) < 300];
      })).toEqual([1, 'running', true]);
    } finally { await app.close(); }
  }, 60000);
}

const CLIPS = `component App
  state wide = false
  state gone = false
  action go
    wide = true
  action hide
    gone = true
  view
    column
      button "Go" testId="go" press=go
      button "Hide" testId="hide" press=hide
      column height=120
        column testId="flex" overflow="hidden" width=(wide ? 200 : 100) -exact-layout-transition="20000ms linear"
          text "Clip" height=300 flex-shrink=0
        text "after" testId="after-flex"
      box
        box testId="block" overflow="hidden" display=(gone ? "none" : "block") width=(wide ? 200 : 100) -exact-layout-transition="20000ms linear"
          box margin-top=20 height=10
        text "after" testId="after-block"
`;

for (const engine of ['chromium', 'firefox', 'webkit']) {
  engineTest[engine](`${engine}: a clipping box stops clipping for its layout transition and keeps its layout`, async () => {
    const app = await open(engine, CLIPS);
    try {
      const { page } = app;
      const boxes = () => page.evaluate(() => ['flex', 'after-flex', 'block', 'after-block'].map(id => {
        const r = document.querySelector(`[data-testid="${id}"]`).getBoundingClientRect();
        return [r.y, r.height];
      }));
      const before = await boxes();
      await page.click('[data-testid="go"]');
      await page.waitForFunction(() => document.querySelector('[data-testid="flex"]').getAnimations().length > 0);
      expect(await page.evaluate(() => getComputedStyle(document.querySelector('[data-testid="flex"]')).overflow)).toBe('visible');
      expect(await boxes()).toEqual(before);
      // A restyle mid-move is the box's own: hidden, it is gone, and its move with it.
      const standIns = () => page.evaluate(() => document.querySelectorAll('#exact-root [data-exiting]').length);
      expect(await standIns()).toBe(2);
      await page.click('[data-testid="hide"]');
      expect(await page.evaluate(() => document.querySelector('[data-testid="block"]').getClientRects().length)).toBe(0);
      expect(await standIns()).toBe(1);
    } finally { await app.close(); }
  }, 60000);
}

/** `source` compiled to the JS target and opened in `engine` with its glue loaded. */
async function open(engine, source) {
  const root = resolve(import.meta.dir, '../..'), target = resolve(root, 'target');
  const dir = mkdtempSync(resolve(target, 'measure-in-motion-'));
  const style = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
  let server, browser;
  const close = async () => { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); };
  try {
    writeFileSync(resolve(dir, 'app.contract'), source);
    const compiled = spawnSync(resolve(target, `debug/exact-web-js${EXE}`), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(compiled.status, compiled.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<style>${style} #pinned { height: 40px !important; }</style><link rel="stylesheet" href="/app.css"><div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`, { headers: { 'content-type': 'text/html' } });
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response(null, { status: 404 });
    } });
    browser = await suiteBrowser(engine);
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.evaluate(async () => { const { pieces } = await import('/rt.js'); await pieces(); });
    await page.waitForFunction(() => globalThis.exact?.presenceLive);
    return { page, close };
  } catch (e) {
    await close();
    throw e;
  }
}
