// LLP 1047.002 D2.1, D4.2: the behaviours of navigation and of the input piece a JS target page leaves out (glue.mjs
// `bundleHostModule`'s and `stageHostGlue`'s `unused`). A behaviour left out ships none of its code, and the hourly
// conformance check fails a page that has what one acts on (conform-cut.mjs).
import { expect, test } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { rolldown } from 'rolldown';
import { gateAgent } from '../../web-js/agent-gate.mjs';
import { bundleHostModule, stageHostGlue } from '../../web-js/glue.mjs';
import { suiteBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { LEFT_OUT, uncut } from '../../web-js/conform-cut.mjs';

const ROOT = resolve(import.meta.dir, '../../..');
const NAVIGATION = ['TABS', 'SPLITS', 'SHEETS', 'BARS', 'CLEAR_BARS'];
const INPUT = ['PRESS_PAINT', 'STATE_LOOKS', 'PANS', 'ROVING', 'RADIO_GROUPS', 'SHORTCUTS'];
// A string only each behaviour's code holds.
const NAVIGATION_MARKS = { TABS: 'data-exact-sidebar', SPLITS: 'data-exact-split-row', SHEETS: '--exact-sheet-detent', BARS: 'part="back"', CLEAR_BARS: 'data-exact-clear' };
const INPUT_MARKS = { PRESS_PAINT: '--exact-press-paint', STATE_LOOKS: 'data-exact-looks', PANS: 'pan cancelled', ROVING: 'menuitem', RADIO_GROUPS: 'radiogroup', SHORTCUTS: 'Backquote' };

// What a production page ships of `input`: its exports' closure, minified, the agent left out (`gateAgent`); `load`
// gives a module's text by its id.
async function shipped(input, load = () => null) {
  const bundle = await rolldown({
    input, external: [/navigation\.js$/],
    plugins: [{ name: 'page', resolveId: (id) => load(id) === null ? null : id, load, transform: (code, id) => gateAgent(code, id) }],
  });
  try {
    return (await bundle.generate({ format: 'es', minify: true })).output[0].code;
  } finally {
    await bundle.close();
  }
}

// navigation.js as the runtime imports it (`sheetLeaves`, `panelsOf` and the projection), the rest unused.
const navigation = async (unused) => {
  const module = await bundleHostModule(ROOT, 'navigation.js', { production: true, unused });
  return shipped('page', (id) => id === 'nav' ? module
    : id === 'page' ? "import { navigation, sheetLeaves, panelsOf } from 'nav'; globalThis.used = [navigation, sheetLeaves, panelsOf];" : null);
};
// The input piece as a page stages it (`stageHostGlue`).
const piece = async (unused) => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-left-out-'));
  try {
    stageHostGlue(ROOT, dir, unused);
    return await shipped(resolve(dir, 'input-glue.js'));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

test('a behaviour left out leaves none of its code in a production page', async () => {
  const [keptNavigation, cutNavigation, keptInput, cutInput] = await Promise.all([navigation([]), navigation(NAVIGATION), piece([]), piece(INPUT)]);
  for (const [marks, kept, cut] of [[NAVIGATION_MARKS, keptNavigation, cutNavigation], [INPUT_MARKS, keptInput, cutInput]]) {
    for (const [name, mark] of Object.entries(marks)) {
      expect(kept.includes(mark), name).toBe(true);
      expect(cut.includes(mark), name).toBe(false);
    }
  }
  await expect(bundleHostModule(ROOT, 'navigation.js', { unused: ['TOOLBARS'] })).rejects.toThrow('no longer has: TOOLBARS = true');
  await expect(piece(['TOOLBARS'])).rejects.toThrow('no longer has: TOOLBARS = true');
});

const { unavailable } = chromium();
need('chrome', unavailable)('the hourly check finds a cut token and what each left-out behaviour acts on inside a root, and nothing a page without them has', async () => {
  const all = { tokens: new Set(['t:dialog']), behaviours: Object.keys(LEFT_OUT) };
  const has = `<dialog></dialog><main navigationKey="1" navigationBack="back"><div navigationKey="1" data-navigationbar="transparent"><header><h2>Title</h2></header>
    <div role="tablist"></div><div data-exact-list></div><div role="menu"></div><div role="radiogroup"></div>
    <button aria-keyshortcuts="Control+K" style="--exact-press-color: red" data-exact-looks="1" data-exact-on="press pan">Go</button></div>
    <div navigationKey="2" navigationPresentation="modal"></div></main>`;
  // Look-alikes: a header, a list and a press scale in no route, and a tablist outside the root.
  const lacks = `<div><header><h2>Title</h2></header><div data-exact-list></div><button style="--exact-press: 0.95" data-exact-on="press">Go</button></div>`;
  const suite = await suiteBrowser('chromium', { timeout: 10000 });
  try {
    const page = await suite.newPage();
    const found = async (root) => {
      await page.setContent(`<!doctype html><div id="exact-root">${root}</div><div role="tablist"></div>`);
      return uncut((expression) => page.evaluate(expression), all);
    };
    expect(await found(has)).toEqual(['t:dialog, which its sheet was cut for', ...Object.entries(LEFT_OUT).map(([name, [what]]) => `${what}, which its build left out (${name})`)]);
    expect(await found(lacks)).toEqual([]);
  } finally {
    await suite.close();
  }
}, 60000);
