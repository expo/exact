// The browser supplies Mod when it adopts the server's page, including a later bound value.
import { beforeAll, test, expect } from 'bun:test';
import { resolve } from 'node:path';
import { drivable } from '../../scripts/coverage.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { renderedPages, warmRustRenderer } from './rendered-pages.mjs';

const source = `routes nav
  home "/" render=build

component App
  state hits = 0
  action open
    hits = hits + 1
  view
    column
      button "Open" aria-keyshortcuts="Mod+Shift+O" press=open testId="open"
      text formatShortcut("Mod+Shift+O") testId="cap"
      for label in ["Page"] key=label
        button label aria-keyshortcuts="Control+S Mod+P" press=open testId="row"
      button "Next" aria-keyshortcuts=(hits == 0 ? "Control+J" : "Shift+Mod+K") press=open testId="next"
      text \`\${hits}\` testId="hits"
`;

beforeAll(warmRustRenderer, 1_800_000);

test('Mod is resolved for the browser before a shortcut is announced or pressed', async () => {
  const engines = await drivable(['chromium', 'firefox', 'webkit']);
  for (const renderer of ['js', 'rust']) await renderedPages(source, async (_, html, dist) => {
    const open = html('').match(/<button\b[^>]*data-testid="open"[^>]*>/)?.[0];
    expect(open).toBeDefined();
    expect(open).not.toContain('aria-keyshortcuts');
    expect(html('')).toContain('aria-keyshortcuts="Control+J"');
    const cap = html('').match(/<[^>]+data-testid="cap"[^>]*>([^<]*)</);
    expect(cap?.[1]).toBe('');
    const remember = `<script>globalThis.serverButton = document.querySelector('[data-testid="open"]');</script>`;
    const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname.slice(1);
      return path ? new Response(Bun.file(resolve(dist, path)))
        : new Response(html('') + remember, { headers: { 'Content-Type': 'text/html' } });
    } });
    try {
      for (const engine of engines) {
        const browser = await suiteBrowser(engine);
        try {
          for (const [platform, hint, primary] of [['MacIntel', '', 'Meta'], ['MacIntel', 'Windows', 'Control']]) {
            const page = await browser.newPage();
            await page.addInitScript(([platform, hint]) => {
              Object.defineProperty(navigator, 'platform', { value: platform });
              Object.defineProperty(navigator, 'userAgentData', { value: { platform: hint } });
            }, [platform, hint]);
            await page.goto(server.url.href);
            const chord = primary + '+Shift+O';
            const attr = 'aria-keyshortcuts';
            await page.waitForFunction(([attr, chord]) => document.querySelector('[data-testid="open"]')?.getAttribute(attr) === chord, [attr, chord]);
            expect(await page.evaluate(() => globalThis.serverButton === document.querySelector('[data-testid="open"]'))).toBe(true);
            expect(await page.locator('[data-testid="cap"]').textContent()).toBe(primary === 'Meta' ? '⇧⌘O' : 'Ctrl+Shift+O');
            expect(await page.locator('[data-testid="row"]').getAttribute(attr)).toBe('Control+S ' + primary + '+P');
            await page.evaluate(() => globalThis.__exactPressFeedback());
            const hits = page.locator('[data-testid="hits"]');
            await page.keyboard.press((primary === 'Meta' ? 'Control' : 'Meta') + '+Shift+O');
            expect(await hits.textContent()).toBe('0');
            await page.keyboard.press(chord);
            await page.waitForFunction(() => document.querySelector('[data-testid="hits"]').textContent === '1');
            expect(await page.locator('[data-testid="next"]').getAttribute(attr)).toBe('Shift+' + primary + '+K');
            await page.close();
          }
        } finally { await browser.close(); }
      }
    } finally { server.stop(true); }
  }, {}, {}, renderer);
}, 300_000);
