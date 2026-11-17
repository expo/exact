// LLP 1047.002 D2.1, D4.2: the behaviours of navigation and of the input piece a JS target page leaves out (glue.mjs
// `bundleHostModule`'s and `stageHostGlue`'s `unused`). A behaviour left out ships none of its code; a development
// page that meets one says so once in its journal.
import { expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { rolldown } from 'rolldown';
import { bundleHostModule, gateAgent, stageHostGlue } from '../../web-js/glue.mjs';
import { suiteBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { withLook } from '../look.mjs';

const ROOT = resolve(import.meta.dir, '../../..');
const NAVIGATION = ['TABS', 'SPLITS', 'SHEETS', 'BARS', 'CLEAR_BARS'];
const INPUT = ['PRESS_PAINT', 'STATE_LOOKS', 'PANS', 'ROVING', 'RADIO_GROUPS', 'SHORTCUTS'];
// A string only each behaviour's code holds, and only the journal's check.
const NAVIGATION_MARKS = { TABS: 'data-exact-sidebar', SPLITS: 'data-exact-split-row', SHEETS: '--exact-sheet-detent', BARS: 'part="back"', CLEAR_BARS: 'data-exact-clear', journal: 'build left out' };
const INPUT_MARKS = { PRESS_PAINT: '--exact-press-paint', STATE_LOOKS: 'data-exact-looks', PANS: 'pan cancelled', ROVING: 'menuitem', RADIO_GROUPS: 'radiogroup', SHORTCUTS: 'Backquote', journal: 'build left out' };

// The closure of `input`'s exports; `load` gives a module's text by its id. A production page's is minified, with
// the agent left out (`gateAgent`); a development page's is neither.
async function bundled(input, { load = () => null, production = true } = {}) {
  const bundle = await rolldown({
    input, external: production ? [/navigation\.js$/] : [],
    resolve: { alias: { './navigation.js': resolve(ROOT, 'host/web/navigation.js') } },
    plugins: [{ name: 'page', resolveId: (id) => load(id) === null ? null : id, load, transform: (code, id) => production ? gateAgent(code, id) : code }],
  });
  try {
    return (await bundle.generate({ format: 'es', minify: production })).output[0].code;
  } finally {
    await bundle.close();
  }
}

// navigation.js as the runtime imports it (`sheetLeaves`, `panelsOf` and the projection), the rest unused.
const navigation = async (unused) => {
  const module = await bundleHostModule(ROOT, 'navigation.js', { production: true, unused });
  return bundled('page', { load: (id) => id === 'nav' ? module
    : id === 'page' ? "import { navigation, sheetLeaves, panelsOf } from 'nav'; globalThis.used = [navigation, sheetLeaves, panelsOf];" : null });
};
// The input piece as a page stages it (`stageHostGlue`).
const piece = async (unused, production = true) => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-left-out-'));
  try {
    stageHostGlue(ROOT, dir, unused);
    return await bundled(resolve(dir, 'input-glue.js'), { production });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

test('a behaviour left out leaves none of its code in a production page', async () => {
  const [keptNavigation, cutNavigation, keptInput, cutInput] = await Promise.all([navigation([]), navigation(NAVIGATION), piece([]), piece(INPUT)]);
  for (const [marks, kept, cut] of [[NAVIGATION_MARKS, keptNavigation, cutNavigation], [INPUT_MARKS, keptInput, cutInput]]) {
    for (const [name, mark] of Object.entries(marks)) {
      expect(kept.includes(mark), name).toBe(name !== 'journal');
      expect(cut.includes(mark), name).toBe(false);
    }
  }
  await expect(bundleHostModule(ROOT, 'navigation.js', { unused: ['TOOLBARS'] })).rejects.toThrow('no longer has: TOOLBARS = true');
  await expect(piece(['TOOLBARS'])).rejects.toThrow('no longer has: TOOLBARS = true');
});

const { unavailable } = chromium();
need('chrome', unavailable)('a development page says once that it has a route\'s header with the app bar left out, and still projects its stack', async () => {
  const module = await bundleHostModule(ROOT, 'navigation.js', { unused: NAVIGATION });
  const html = withLook(readFileSync(resolve(ROOT, 'host/web/index.html'), 'utf8'))
    .replace('<div id="exact-root"></div>', () => `<div id="exact-root"><main navigationKey="2" navigationBack="back">
      <div navigationKey="1" id="first"><header><h2 role="heading" data-exact-text>First</h2></header></div>
      <div navigationKey="2" id="second"><header><h2 role="heading" data-exact-text>Second</h2></header></div></main></div>`)
    .replace('<script type="module" src="./glue.js"></script>', () => `<script type="module">${module}
      const shell = document.getElementById('exact-root'); globalThis.journal = [];
      navigation.connect(shell, () => false, (line) => journal.push(line));
      navigation.project(shell, () => {}); navigation.project(shell, () => {}); globalThis.projected = true;</script>`);
  const suite = await suiteBrowser('chromium', { timeout: 10000 });
  try {
    const page = await suite.newPage();
    await page.setContent(html);
    await page.waitForFunction(() => globalThis.projected);
    expect(await page.evaluate(() => ({
      journal,
      first: document.getElementById('first').style.visibility,
      back: !!document.querySelector('#second > header').shadowRoot,
    }))).toEqual({
      journal: ['navigation: the page has a route\'s header, which this build left out: its plan makes none (LLP 1047.002 D2)'],
      first: 'hidden',
      back: false,
    });
  } finally {
    await suite.close();
  }
}, 60000);

need('chrome', unavailable)('a development page says once, as its routes arrive, that it has a radio group and a keyboard shortcut its input piece left out', async () => {
  const input = await piece(INPUT, false);
  const html = `<!doctype html><div id="exact-root"><button aria-keyshortcuts="Control+K">Find</button><div role="radiogroup"><button role="radio">A</button></div></div>
    <script type="module">${input}
      globalThis.journal = [];
      const handlers = createInputHandlers({ root: document.getElementById('exact-root'), views: new Map(), retiredViews: new Set(), ready: () => true, inertAncestor: () => null, editMenu: () => false, dispatch: () => {}, log: (line) => journal.push(line) });
      handlers.arrive(); handlers.arrive(); globalThis.installed = true;</script>`;
  const suite = await suiteBrowser('chromium', { timeout: 10000 });
  try {
    const page = await suite.newPage();
    await page.setContent(html);
    await page.waitForFunction(() => globalThis.installed);
    expect(await page.evaluate(() => journal)).toEqual([
      'input: the page has a radio group, which this build left out: its plan makes none (LLP 1047.002 D4.2)',
      'input: the page has a keyboard shortcut, which this build left out: its plan makes none (LLP 1047.002 D4.2)',
    ]);
  } finally {
    await suite.close();
  }
}, 60000);
