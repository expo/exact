// LLP 1057.003 D5 on the JS target: the timelines fixture (host/web/tests/fixtures/timelines.contract) built by this
// target runs every scroll and view timeline as the browser's own, and each consumer shows what the browser recorded
// for the wasm target's build of the same app (browser-timelines.json, `parity.mjs --timelines`), which the engine is
// held to as well. Without the fixture's drag timeline the plan has none, so every name is CSS's own row and no motion
// piece is needed; the recording itself is of the fixture whole, where the motion piece hands each name that finds a
// scroll or view timeline to the browser (motion-glue.js `followTimelines`). A browser with no scroll-driven animations
// (Firefox's) shows each consumer's own values. Build exact-web-js once before running this test (docs/testing.md).
import { afterAll, test, expect } from 'bun:test';
import { EXE } from '../../scripts/paths.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { withLook } from '../web/look.mjs';

const root = resolve(import.meta.dir, '../..'), fixtures = resolve(root, 'host/web/tests/fixtures');
const target = resolve(root, 'target');
const steps = JSON.parse(readFileSync(resolve(fixtures, 'timelines-steps.json'), 'utf8'));
const recorded = JSON.parse(readFileSync(resolve(fixtures, 'browser-timelines.json'), 'utf8')).samples;
// The fixture without its drag timeline: a plan with none.
const noDrag = readFileSync(resolve(fixtures, 'timelines.contract'), 'utf8').replace(/^.*-exact-drag-timeline.*\n/m, '');
const browsers = new Set();
afterAll(async () => { for (const b of browsers) await b.close(); });

/** `source` built by this target and opened in `engine`; `close` ends it. */
async function open(source, engine, ready) {
  mkdirSync(target, { recursive: true });
  const dir = mkdtempSync(resolve(target, 'scroll-timelines-'));
  const style = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
  writeFileSync(resolve(dir, 'app.contract'), source);
  const compiled = spawnSync(resolve(target, `debug/exact-web-js${EXE}`), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
  expect(compiled.status, compiled.stderr).toBe(0);
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname.slice(1);
    if (!path) return new Response(`<style>${style}</style><link rel="stylesheet" href="/app.css">
      <div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`,
    { headers: { 'content-type': 'text/html' } });
    const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
    const file = bases.map(base => resolve(base, path)).find(existsSync);
    return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
  } });
  const browser = await suiteBrowser(engine);
  browsers.add(browser);
  const context = await browser.newContext({ viewport: { width: 420, height: 900 } });
  const page = await context.newPage();
  await page.goto(server.url.href);
  await page.waitForFunction((id) => document.querySelector(`[data-testid="${id}"]`), ready);
  return { page, close: async () => { await context.close(); server.stop(true); rmSync(dir, { recursive: true, force: true }); } };
}

/** Scroll `scroller` to `at` on `axis`, then each of `read`'s computed opacity once two frames have run. */
const sample = (page, scroller, axis, at, read) => page.evaluate(async ({ scroller, axis, at, read }) => {
  document.querySelector(`[data-testid="${scroller}"]`)[axis === 'x' ? 'scrollLeft' : 'scrollTop'] = at;
  await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
  return Object.fromEntries(read.map((id) => [id, +getComputedStyle(document.querySelector(`[data-testid="${id}"]`)).opacity]));
}, { scroller, axis, at, read });

test('scroll and view timelines in a plan with no drag timeline show what the browser recorded', async () => {
  expect(noDrag).not.toContain('-exact-drag-timeline');
  const { page, close } = await open(noDrag, 'chromium', 'vsub');
  try {
    const wrong = [];
    for (const step of steps) {
      for (const at of step.at) {
        const got = await sample(page, step.scroller, step.axis, at, step.read);
        const want = recorded.find((s) => s.scroller === step.scroller && s.at === at)?.values ?? {};
        for (const id of step.read) {
          if (!(Math.abs(got[id] - want[id]) <= 1e-3)) wrong.push(`${step.scroller}@${at} ${id}: ${got[id]}, recorded ${want[id]}`);
        }
      }
    }
    expect(wrong).toEqual([]);
  } finally { await close(); }
}, 120000);

// A bound `animation` whose shorthand the page writes again resets the timeline and the range, which are written
// again after it, from none and from another animation alike: the consumer follows its scroller, not the clock.
const DYNAMIC = `keyframes fade
  from opacity=0
  to opacity=1
component App
  state on = false
  state slow = false
  action toggle
    on = not on
  action slower
    slow = not slow
  view
    column
      button "Toggle" press=toggle testId="toggle"
      button "Slower" press=slower testId="slower"
      column timeline-scope="--t"
        row testId="s" width=200 height=40 overflow-x="auto" overflow-y="hidden" scroll-timeline="--t x"
          view width=800 height=40 flex-shrink=0
        view testId="c" width=10 height=4 opacity=0.25 animation=(on ? (slow ? "fade 2s linear both" : "fade 1s linear both") : "none") animation-timeline="--t" animation-range="0px 300px"
`;

test('a bound animation under a literal timeline and range follows its scroller once the page writes it', async () => {
  const { page, close } = await open(DYNAMIC, 'chromium', 'c');
  try {
    expect(await sample(page, 's', 'x', 150, ['c'])).toEqual({ c: 0.25 });
    await page.click('[data-testid="toggle"]');
    expect(await page.evaluate(() => getComputedStyle(document.querySelector('[data-testid="c"]')).animationTimeline)).toBe('--t');
    const at = (x) => sample(page, 's', 'x', x, ['c']).then((v) => v.c);
    expect(await at(150)).toBeCloseTo(0.5, 3);
    expect(await at(75)).toBeCloseTo(0.25, 3);
    expect(await at(300)).toBeCloseTo(1, 3);
    // Active to active: another duration, the same timeline and range.
    await page.click('[data-testid="slower"]');
    const shown = () => page.evaluate(() => {
      const s = getComputedStyle(document.querySelector('[data-testid="c"]'));
      return [s.animationDuration, s.animationTimeline, s.animationRangeStart, s.animationRangeEnd];
    });
    expect(await shown()).toEqual(['2s', '--t', '0px', '300px']);
    expect(await at(150)).toBeCloseTo(0.5, 3);
    expect(await at(30)).toBeCloseTo(0.1, 3);
  } finally { await close(); }
}, 120000);

test('a browser with no scroll-driven animations shows every consumer its own values', async () => {
  for (const [source, ready, groups] of [
    [noDrag, 'vsub', steps],
    [DYNAMIC, 'c', [{ scroller: 's', axis: 'x', at: [0, 150, 300], read: ['c'], toggle: true }]],
  ]) {
    const { page, close } = await open(source, 'firefox', ready);
    try {
      expect(await page.evaluate(() => CSS.supports('animation-timeline', 'scroll()'))).toBe(false);
      const wrong = [];
      for (const step of groups) {
        if (step.toggle) await page.click('[data-testid="toggle"]');
        for (const at of step.at) {
          const got = await sample(page, step.scroller, step.axis, at, step.read);
          for (const id of step.read) if (got[id] !== 0.25) wrong.push(`${step.scroller}@${at} ${id}: ${got[id]}`);
        }
      }
      expect(wrong).toEqual([]);
    } finally { await close(); }
  }
}, 240000);
