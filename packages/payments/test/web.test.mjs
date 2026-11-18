import { expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests(['chromium', 'firefox']);
for (const engine of ['chromium', 'firefox']) {
  engineTest[engine]('the fallback payment label stays centred under the host reset', async () => {
    const browser = await suiteBrowser(engine);
    try {
      const page = await browser.newPage({ viewport: { width: 390, height: 600 } });
      await page.setContent('<div id="pay" style="width:240px;height:48px"></div>');
      await page.addStyleTag({ content: readFileSync(new URL('../../../host/web/index.html', import.meta.url), 'utf8').match(/<style>([\s\S]*?)<\/style>/)[1] });
      await page.evaluate(async source => {
        const { roster } = await import(`data:text/javascript,${encodeURIComponent(source)}`);
        window.presses = 0;
        new roster['exact-payment-button'].view({ element: document.getElementById('pay'),
          props: { buttonType: 'checkout' }, events: { press() { presses++; } } });
      }, readFileSync(new URL('../web/index.js', import.meta.url), 'utf8'));
      const button = page.locator('button');
      const offset = await button.evaluate(button => {
        const range = document.createRange();
        range.selectNodeContents(button);
        const text = range.getBoundingClientRect(), box = button.getBoundingClientRect();
        return { x: text.x + text.width / 2 - box.x - box.width / 2,
          y: text.y + text.height / 2 - box.y - box.height / 2 };
      });
      expect(Math.abs(offset.x)).toBeLessThan(1);
      expect(Math.abs(offset.y)).toBeLessThan(1);
      await button.click();
      expect(await page.evaluate(() => presses)).toBe(1);
    } finally {
      await browser.close();
    }
  });
}
