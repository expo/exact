// @ref LLP 1123 D7 — a popover route keeps `navigationPresentation="popover"` on the page, its anchor and arrows as
// `data-` attributes, and where it has no anchor to stand beside is the web's sheet: drawn, made modal and dismissed by
// Escape as a `modal` route is. The same app is compiled twice, its presented route a sheet and then a popover whose
// anchor is not there, launched at that route, and the two are held equal in Chromium, WebKit and Firefox. Build exact-web-js once before running this test (docs/testing.md).
import { test, expect } from 'bun:test';
import { EXE } from '../../scripts/paths.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { withLook } from '../web/look.mjs';

const app = (presentation) => `routes nav
  home "/"
    pick "/pick"
component App
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=\`\${e.id}\` testId="route-home"
            button "Pick" id="pick" testId="open"
        if e.name == "pick"
          column navigationKey=\`\${e.id}\` ${presentation} testId="route-pick"
            header
              text "Pick" role="heading" aria-level=2
            text "Choose one"
`;

/** What the presented route is on the page at `/pick`, then whether Escape takes it away. */
async function presented(dir, browser, engineLabel) {
  const root = resolve(import.meta.dir, '../..');
  const style = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname.slice(1);
    if (!path || path === 'pick') return new Response(`<style>${style}</style><link rel="stylesheet" href="/app.css">
      <div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`, { headers: { 'content-type': 'text/html' } });
    const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
    const file = bases.map(base => resolve(base, path)).find(existsSync);
    return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
  } });
  try {
    const page = await (await browser.newContext({ viewport: { width: 400, height: 700 }, reducedMotion: 'reduce' })).newPage();
    await page.goto(`${server.url.href}pick`);
    await page.getByTestId('route-pick').waitFor({ state: 'attached' });
    await page.waitForFunction(() => document.querySelector('[data-testid="route-pick"]')?.getAttribute('role') === 'dialog');
    const shown = await page.evaluate(() => {
      const route = document.querySelector('[data-testid="route-pick"]'), home = document.querySelector('[data-testid="route-home"]');
      const r = route.getBoundingClientRect(), s = getComputedStyle(route);
      return { presentation: route.getAttribute('navigationPresentation'), anchor: route.dataset.navigationpopoveranchor ?? null, frame: [r.x, r.y, r.width, r.height].map(Math.round), position: s.position, radius: s.borderTopLeftRadius, background: s.backgroundColor,
        role: route.getAttribute('role'), modal: route.getAttribute('aria-modal'), homeInert: !!home?.closest('[inert]'), open: route.matches(':popover-open') };
    });
    await page.keyboard.press('Escape');
    await page.waitForFunction(() => !document.querySelector('[data-testid="route-pick"]:not([data-exiting])'), null, { timeout: 5000 });
    return { ...shown, dismissed: true };
  } finally {
    server.stop(true);
  }
}

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a popover route is the web's sheet, in ${engine}`, async () => {
  const root = resolve(import.meta.dir, '../..'), target = resolve(root, 'target');
  mkdirSync(target, { recursive: true });
  const dirs = { modal: mkdtempSync(resolve(target, 'popover-sheet-')), popover: mkdtempSync(resolve(target, 'popover-sheet-')) };
  const said = { modal: 'navigationPresentation="modal"', popover: 'navigationPresentation="popover" navigationPopoverAnchor="missing" navigationPopoverArrows="up"' };
  const browser = await suiteBrowser(engine);
  try {
    const seen = {};
    for (const [kind, dir] of Object.entries(dirs)) {
      writeFileSync(resolve(dir, 'app.contract'), app(said[kind]));
      const compiled = spawnSync(resolve(target, `debug/exact-web-js${EXE}`), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
      expect(compiled.status, compiled.stderr).toBe(0);
      seen[kind] = await presented(dir, browser, engine);
    }
    expect(seen.modal).toMatchObject({ presentation: 'modal', anchor: null, role: 'dialog', modal: 'true', homeInert: true, open: false, dismissed: true });
    expect(seen.popover).toEqual({ ...seen.modal, presentation: 'popover', anchor: 'missing' });
  } finally {
    await browser.close();
    for (const dir of Object.values(dirs)) rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
