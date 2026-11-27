// @ref LLP 1123 D7 — a popover route as the web's own popover (navigation/route-popovers.js): the route stands beside
// the element its `navigationPopoverAnchor` names, on the side its arrows allow, nonmodal over a live page; a press
// outside reaches what it pressed and closes it, which goes Back once, as Escape does; `closedby="none"` leaves both to
// the app; with no anchor to stand beside it is the sheet. Compiled for the JS target and launched at the route, in
// Chromium, WebKit and Firefox. Build exact-web-js once before running this test (docs/testing.md).
import { test, expect } from 'bun:test';
import { EXE } from '../../scripts/paths.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { withLook } from '../web/look.mjs';

const ROOT = resolve(import.meta.dir, '../..'), TARGET = resolve(ROOT, 'target');
const app = (said) => `routes nav
  home "/"
    pick "/pick"
component App
  state count = 0
  action counted
    count = count + 1
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=\`\${e.id}\` testId="route-home" padding-top=300 align-items="center"
            button "Count" press=counted testId="count"
            text \`\${count}\` testId="counted"
            button "Pick" id="pick" testId="anchor"
        if e.name == "pick"
          column navigationKey=\`\${e.id}\` navigationPresentation="popover" ${said} testId="route-pick" width=240
            header
              text "Pick" role="heading" aria-level=2
            text "Choose one"
`;

const style = withLook(readFileSync(resolve(ROOT, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
/** `said` compiled and served, a page opened at `/pick`: `run(page)`. */
async function at(browser, said, run) {
  mkdirSync(TARGET, { recursive: true });
  const dir = mkdtempSync(resolve(TARGET, 'route-popovers-'));
  writeFileSync(resolve(dir, 'app.contract'), app(said));
  const compiled = spawnSync(resolve(TARGET, `debug/exact-web-js${EXE}`), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
  expect(compiled.status, compiled.stderr).toBe(0);
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname.slice(1);
    if (!path || path === 'pick') return new Response(`<style>${style}</style><link rel="stylesheet" href="/app.css">
      <div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`, { headers: { 'content-type': 'text/html' } });
    const bases = path.startsWith('web/') ? [resolve(ROOT, 'host')] : [dir, import.meta.dir, resolve(ROOT, 'host/web')];
    const file = bases.map(base => resolve(base, path)).find(existsSync);
    return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
  } });
  const context = await browser.newContext({ viewport: { width: 800, height: 700 }, reducedMotion: 'reduce' });
  try {
    const page = await context.newPage();
    await page.goto(`${server.url.href}pick`);
    await page.getByTestId('route-pick').waitFor({ state: 'attached' });
    await page.waitForFunction(() => document.querySelector('[data-testid="route-pick"]')?.getAttribute('role') === 'dialog');
    await run(page);
  } finally {
    await context.close();
    server.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

/** The popover route as the page has it now, beside its anchor. */
const seen = (page) => page.evaluate(() => {
  const route = document.querySelector('[data-testid="route-pick"]:not([data-exiting])'), anchor = document.getElementById('pick');
  if (!route) return null;
  const r = route.getBoundingClientRect(), a = anchor.getBoundingClientRect();
  return { open: route.matches(':popover-open'), below: r.top >= a.bottom - 0.5, above: r.bottom <= a.top + 0.5, width: Math.round(r.width),
    centred: Math.abs((r.left + r.right) / 2 - (a.left + a.right) / 2) < 1, modal: route.getAttribute('aria-modal'), role: route.getAttribute('role'),
    homeInert: !!document.querySelector('[data-testid="route-home"]').closest('[inert]'), scrim: !!document.querySelector('[data-exact-scrim="on"]') };
});
const gone = (page) => page.waitForFunction(() => !document.querySelector('[data-testid="route-pick"]:not([data-exiting])'), null, { timeout: 5000 }).then(() => true);
const counted = (page) => page.evaluate(() => document.querySelector('[data-testid="counted"]').textContent);

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a popover route is the web's own popover, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    // Beside its anchor, below it, centred on it, at its own width; nonmodal: no scrim, the page under it live
    await at(browser, 'navigationPopoverAnchor="pick" navigationPopoverArrows="up"', async (page) => {
      expect(await seen(page)).toEqual({ open: true, below: true, above: false, width: 240, centred: true, modal: null, role: 'dialog', homeInert: false, scrim: false });
      // The agent's tree gives the route's props as the runner's does: the host's `popover` is none of them
      const props = await page.evaluate(async () => {
        // As the build's entry gives it the runtime (build.mjs, `globalThis.exact`)
        const [{ install }, rt] = await Promise.all([import('/agent.js'), import('/rt.js')]);
        const read = install({ ...globalThis.exact, journal: rt.journal, clock: rt.clock, views: rt.Views, viewId: rt.viewId, After: rt.After }, { passive: true });
        const { nodes } = await read({ op: 'tree' });
        return nodes.find((n) => n.props?.testId === 'route-pick')?.props ?? null;
      });
      expect([props?.navigationPresentation, props?.popover]).toEqual(['popover', undefined]);
      // A press outside reaches what it pressed, and the close goes Back
      await page.getByTestId('count').click();
      expect(await gone(page)).toBe(true);
      expect(await counted(page)).toBe('1');
    });
    // Escape closes it through the same Back
    await at(browser, 'navigationPopoverAnchor="pick"', async (page) => {
      expect((await seen(page)).open).toBe(true);
      await page.keyboard.press('Escape');
      expect(await gone(page)).toBe(true);
    });
    // An arrow that points down stands it above its anchor
    await at(browser, 'navigationPopoverAnchor="pick" navigationPopoverArrows="down"', async (page) => {
      expect(await seen(page)).toMatchObject({ open: true, above: true, below: false });
    });
    // `closedby="none"`: a press outside reaches its control and leaves the popover, and so does Escape
    await at(browser, 'navigationPopoverAnchor="pick" closedby="none"', async (page) => {
      await page.getByTestId('count').click();
      await page.keyboard.press('Escape');
      await page.waitForTimeout(100);
      expect([await counted(page), (await seen(page))?.open]).toEqual(['1', true]);
    });
    // No anchor to stand beside: the sheet, modal over an inert page
    await at(browser, 'navigationPopoverAnchor="missing"', async (page) => {
      expect(await seen(page)).toMatchObject({ open: false, modal: 'true', role: 'dialog', homeInert: true, scrim: true });
    });
  } finally {
    await browser.close();
  }
}, 180000);
