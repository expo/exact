// LLP 1047.002 D2.1: the navigation behaviours a JS target page leaves out (glue.mjs `bundleHostModule`'s `unused`).
// A behaviour left out ships none of its code; a development page that meets one says so once in its journal.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { rolldown } from 'rolldown';
import { bundleHostModule } from '../../web-js/glue.mjs';
import { suiteBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { withLook } from '../look.mjs';

const ROOT = resolve(import.meta.dir, '../../..');
const ALL = ['TABS', 'SPLITS', 'SHEETS', 'BARS', 'CLEAR_BARS'];
// A string only each behaviour's code holds, and only the journal's check.
const MARKS = { TABS: 'data-exact-sidebar', SPLITS: 'data-exact-split-row', SHEETS: '--exact-sheet-detent', BARS: 'part="back"', CLEAR_BARS: 'data-exact-clear', journal: 'build left out' };

// navigation.js as a page ships it: what the runtime imports from it, minified with the rest unused.
async function shipped(unused) {
  const navigation = await bundleHostModule(ROOT, 'navigation.js', { production: true, unused });
  const bundle = await rolldown({
    input: 'page',
    plugins: [{
      name: 'page',
      resolveId: (id) => ['page', 'navigation'].includes(id) ? id : null,
      load: (id) => id === 'navigation' ? navigation
        : id === 'page' ? "import { navigation, sheetLeaves, panelsOf } from 'navigation'; globalThis.used = [navigation, sheetLeaves, panelsOf];" : null,
    }],
  });
  try {
    return (await bundle.generate({ format: 'es', minify: true })).output[0].code;
  } finally {
    await bundle.close();
  }
}

test('a behaviour left out leaves none of its code in a production page', async () => {
  const [kept, cut] = await Promise.all([shipped([]), shipped(ALL)]);
  for (const [name, mark] of Object.entries(MARKS)) {
    expect(kept.includes(mark), name).toBe(name !== 'journal');
    expect(cut.includes(mark), name).toBe(false);
  }
  await expect(bundleHostModule(ROOT, 'navigation.js', { unused: ['TOOLBARS'] })).rejects.toThrow('no longer has: TOOLBARS = true');
});

const { unavailable } = chromium();
need('chrome', unavailable)('a development page says once that it has a route\'s header with the app bar left out, and still projects its stack', async () => {
  const navigation = await bundleHostModule(ROOT, 'navigation.js', { unused: ALL });
  const html = withLook(readFileSync(resolve(ROOT, 'host/web/index.html'), 'utf8'))
    .replace('<div id="exact-root"></div>', () => `<div id="exact-root"><main navigationKey="2" navigationBack="back">
      <div navigationKey="1" id="first"><header><h2 role="heading" data-exact-text>First</h2></header></div>
      <div navigationKey="2" id="second"><header><h2 role="heading" data-exact-text>Second</h2></header></div></main></div>`)
    .replace('<script type="module" src="./glue.js"></script>', () => `<script type="module">${navigation}
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
