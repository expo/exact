import { expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { drivable } from '../../scripts/coverage.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { renderedPages } from './rendered-pages.mjs';

const source = `routes nav
  home "/" render=build activate=interaction
  docs "/docs" render=build activate=interaction
  start "/docs/guide/start" render=build activate=interaction

component App
  state count = 0
  action add
    count = count + 1
  view
    button press=add
      text \`Count \${count}\`
`;

test('a direct visit loads its runtime from the asset base and replays the first press', async () => {
  const engines = await drivable(['chromium', 'firefox', 'webkit']);
  for (const base of ['/', '/site/']) await renderedPages(source, async (_, html, dist) => {
    // Serve both spellings directly, as a static host that does not redirect directory URLs.
    const routes = ['', 'docs', 'docs/guide/start'];
    const pages = new Map();
    for (const route of routes) {
      const page = html(route);
      pages.set(base + route, page);
      if (route) pages.set(base + route + '/', page);
    }
    if (base !== '/') pages.set(base.slice(0, -1), html(''));
    const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      if (pages.has(path)) return new Response(pages.get(path), { headers: { 'Content-Type': 'text/html' } });
      const file = path.startsWith(base) && Bun.file(resolve(dist, path.slice(base.length)));
      return file && await file.exists() ? new Response(file) : new Response(null, { status: 404 });
    } });
    try {
      for (const engine of engines) {
        const browser = await suiteBrowser(engine);
        try {
          const page = await browser.newPage();
          for (const path of pages.keys()) {
            await page.goto(new URL(path, server.url).href);
            await page.getByRole('button', { name: 'Count 0' }).click();
            await page.getByRole('button', { name: 'Count 1' }).waitFor({ timeout: 5000 });
            expect(await page.evaluate(() => !!self.exact.ready)).toBe(true);
          }
        } finally { await browser.close(); }
      }
    } finally { server.stop(true); }
  }, {}, { host: { web: { base } } });
}, 120000);
