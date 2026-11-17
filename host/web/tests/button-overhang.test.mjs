import { expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { withLook } from '../look.mjs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { decodePng } from '../../../scripts/png.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests();

const sheet = withLook(readFileSync(new URL('../index.html', import.meta.url), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
const latin = readFileSync(new URL('../../../scripts/fixtures/fonts/assets/DejaVuSans.ttf', import.meta.url)).toString('base64');
const font = readFileSync(new URL('../../painted/tests/fonts/NotoNastaliqUrdu.ttf', import.meta.url)).toString('base64');

for (const engine of ['chromium', 'firefox', 'webkit']) {
  engineTest[engine](`${engine}: a sheet action's generated title keeps the font line and ink margin`, async () => {
    const browser = await suiteBrowser(engine), page = await browser.newPage();
    try {
      await page.setContent(`<!doctype html><style>${sheet}
        @font-face { font-family: Tall; src: url(data:font/ttf;base64,${font}); }
        #exact-root { font-family: Tall; }
      </style><div id="exact-root">
        <button id="ordinary" data-button-style="bordered"><span data-exact-text>گھرانے</span></button>
        <section navigationPresentation="modal"><header>
          <button id="generated" data-button-style="bordered" aria-label="گھرانے"><img data-symbol-source="symbol:checkmark"></button>
        </header></section>
      </div>`);
      await page.evaluate(() => document.fonts.ready);
      const result = await page.evaluate(() => {
        const title = document.querySelector('#ordinary span'), normal = getComputedStyle(title);
        const generated = getComputedStyle(document.querySelector('#generated'), '::after');
        return { line: [normal.lineHeight, generated.lineHeight], height: [title.getBoundingClientRect().height, parseFloat(generated.height)],
          overflowX: generated.overflowX, margin: [normal.overflowClipMargin, generated.overflowClipMargin], content: generated.content };
      });
      expect(result.content).toBe('"گھرانے"');
      expect(result.line).toEqual(['normal', 'normal']);
      expect(result.height[0]).toBeGreaterThan(20);
      expect(result.height[1]).toBe(result.height[0]);
      expect(result.overflowX).toBe('hidden');
      expect(result.margin[1]).toBe(result.margin[0]);
    } finally { await page.close(); await browser.close(); }
  }, 60000);

  engineTest[engine](`${engine}: a long sheet action paints an ellipsis at the end of its generated title`, async () => {
    const browser = await suiteBrowser(engine), page = await browser.newPage();
    try {
      await page.setContent(`<!doctype html><style>${sheet}
        @font-face { font-family: Pinned; src: url(data:font/ttf;base64,${latin}); }
        #exact-root { font-family: Pinned; }
        #exact-root #long { display: block; width: 130px; text-align: left; }
      </style><div id="exact-root"><section navigationPresentation="modal"><header>
        <button id="long" data-button-style="bordered" aria-label="MMMMMMMMMMMMMMMM"><img data-symbol-source="symbol:checkmark"></button>
      </header></section></div>`);
      await page.evaluate(() => document.fonts.ready);
      const reference = await page.evaluate(() => {
        const button = document.querySelector('#long'), style = getComputedStyle(button, '::after');
        const measure = document.createElement('span');
        measure.style.cssText = `position:absolute;white-space:pre;font:${style.font};letter-spacing:${style.letterSpacing}`;
        document.body.append(measure);
        let prefix = button.getAttribute('aria-label');
        do {
          prefix = prefix.slice(0, -1);
          measure.textContent = prefix + '…';
        } while (measure.getBoundingClientRect().width > parseFloat(style.width));
        measure.remove();
        const box = getComputedStyle(button);
        return { title: prefix + '…', overflowX: style.overflowX, height: button.getBoundingClientRect().height,
          inset: parseFloat(box.borderLeftWidth) + parseFloat(box.paddingLeft) };
      });
      expect(reference.title.length).toBeGreaterThan(1);
      expect(reference.title.length).toBeLessThan(16);
      const actual = decodePng(await page.locator('#long').screenshot());
      // Compare the title's pixels with a literal ellipsis, excluding the rounded border.
      await page.evaluate(title => document.querySelector('#long').setAttribute('aria-label', title), reference.title);
      const expected = decodePng(await page.locator('#long').screenshot());
      expect([actual.width, actual.height]).toEqual([expected.width, expected.height]);
      const differs = actual.data.some((value, i) => {
        const x = Math.floor(i / 4) % actual.width;
        return x >= reference.inset && x < actual.width - reference.inset && value !== expected.data[i];
      });
      expect(differs).toBe(false);
      expect(reference.overflowX).toBe('hidden');
      expect(await page.locator('#long').evaluate(button => button.getBoundingClientRect().height)).toBe(reference.height);
    } finally { await page.close(); await browser.close(); }
  }, 60000);
}
