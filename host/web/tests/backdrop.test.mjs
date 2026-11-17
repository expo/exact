// A modal dialog's `::backdrop` in every engine (LLP 1035.001 D11): the scrim when its author said nothing, the
// author's colour (`--exact-backdrop-color`, which css.rs writes on the dialog) when said, `transparent` laying
// nothing; it fades in as the dialog opens and out as it leaves with its exit (`data-exiting`, presence-glue.js).
// Read in pixels: WebKit reports neither the pseudo-element's computed colour nor its transition's `pseudoElement`.
import { expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { withLook } from '../look.mjs';
import { decodePng } from '../../../scripts/png.mjs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests();

const sheet = withLook(readFileSync(new URL('../index.html', import.meta.url), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];

for (const engine of ['chromium', 'firefox', 'webkit']) engineTest[engine](`${engine}: a dialog's backdrop is its author's colour, else the scrim, and fades`, async () => {
  const browser = await suiteBrowser(engine);
  const page = await browser.newPage({ viewport: { width: 400, height: 300 } });
  try {
    await page.setContent(`<style>${sheet} body { background: #ffffff; }</style><div id="exact-root">
      <dialog id="unsaid">Unsaid</dialog>
      <dialog id="red" style="--exact-backdrop-color:rgba(255, 0, 0, 0.5)">Red</dialog>
      <dialog id="clear" style="--exact-backdrop-color:rgba(0, 0, 0, 0)">Clear</dialog>
    </div>`);
    const pixel = async () => {
      await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
      const { width, data } = decodePng(await page.screenshot());
      return [...data.slice((10 * width + 10) * 4, (10 * width + 10) * 4 + 3)];
    };
    // Whether the backdrop's colour began a transition, which then ends at once.
    const fades = id => page.evaluate(id => {
      const fading = document.getAnimations().filter(a => a.transitionProperty === 'background-color' && a.effect?.target?.id === id);
      fading.forEach(a => a.finish());
      return fading.length === 1;
    }, id);
    const read = async id => {
      await page.evaluate(id => document.getElementById(id).showModal(), id);
      const opening = await fades(id), shown = await pixel();
      await page.evaluate(id => document.getElementById(id).setAttribute('data-exiting', ''), id);
      const leaving = await fades(id), left = await pixel();
      await page.evaluate(id => { const el = document.getElementById(id); el.removeAttribute('data-exiting'); el.close(); }, id);
      return [opening, shown, leaving, left];
    };
    const near = (got, want) => got.every((v, i) => Math.abs(v - want[i]) <= 2) || got;
    const white = [255, 255, 255];
    const [opening, scrim, leaving, left] = await read('unsaid');
    expect([engine, opening, scrim[0] === scrim[1] && scrim[1] === scrim[2] && scrim[0] < 240 || scrim, leaving, near(left, white)]).toEqual([engine, true, true, true, true]);
    const red = await read('red');
    expect([engine, red[0], near(red[1], [255, 127, 127]), red[2], near(red[3], white)]).toEqual([engine, true, true, true, true]);
    expect([engine, near((await read('clear'))[1], white)]).toEqual([engine, true]);
  } finally {
    await page.close();
  }
}, 60_000);
