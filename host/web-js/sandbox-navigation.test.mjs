import { expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { drivable } from '../../scripts/coverage.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { renderedPages } from './rendered-pages.mjs';

const source = `routes nav
  home "/" render=build activate=idle
    detail "/detail" render=build activate=idle
    other "/other"

component App
  action replaceDetail
    nav = replace(nav, "/other")
  action back
    nav = back(nav)
  view
    main navigationKey=\`\${top(nav).id}\` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` padding=20
          if e.name == "home"
            link href=path("detail")
              text "Open detail"
          else
            text e.name testId="shown"
            link href="#section"
              text "Section"
            link href=path("home")
              text "Home"
            button press=replaceDetail
              text "Replace"
            button press=back
              text "Back"
`;

test('a sandboxed frame completes route changes and traverses the entries it wrote', () => renderedPages(source, async (_, html, dist) => {
  const engines = await drivable(['webkit', 'chromium', 'firefox']);
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    const path = new URL(req.url).pathname, headers = { 'Access-Control-Allow-Origin': '*' };
    const document = body => new Response(body, { headers: { ...headers, 'Content-Type': 'text/html' } });
    if (path === '/root') return document('<iframe sandbox="allow-scripts" src="/"></iframe>');
    if (path === '/memory') return document('<iframe sandbox="allow-scripts" src="/?agent=1"></iframe>');
    if (path === '/') return document(html(''));
    if (path === '/embed') return document('<iframe sandbox="allow-scripts" src="/detail/"></iframe>');
    if (path === '/detail/' || path === '/detail') return document(html('detail'));
    const file = Bun.file(resolve(dist, path.slice(1)));
    return await file.exists() ? new Response(file, { headers }) : new Response(null, { status: 404 });
  } });
  try {
    for (const engine of engines) {
      const browser = await suiteBrowser(engine);
      try {
        const page = await browser.newPage();
        await page.goto(new URL('embed', server.url).href);
        const frame = page.frames().find(f => f.parentFrame());
        const shown = frame.getByTestId('shown'), open = frame.getByText('Open detail', { exact: true });
        const at = async (url, name) => {
          await frame.waitForFunction(url => history.state?.url === url, url, { timeout: 5000 });
          expect(await frame.locator('[data-testid=shown]:visible').allTextContents()).toEqual(name ? [name] : []);
          expect(await open.isVisible()).toBe(!name);
        };
        await at('/detail', 'detail');
        const canRewrite = await frame.evaluate(() => {
          const here = location.href;
          try {
            history.replaceState(history.state, '', '/probe');
            history.replaceState(history.state, '', here);
            return true;
          } catch { return false; }
        });
        expect(await frame.evaluate(() => [self.origin, location.pathname])).toEqual(['null', canRewrite ? '/detail' : '/detail/']);
        await frame.getByText('Home', { exact: true }).click();
        await at('/', null);
        await open.click();
        await at('/detail', 'detail');
        await frame.getByText('Section', { exact: true }).click();
        await at('/detail#section', 'detail');
        expect(await frame.evaluate(() => location.pathname + location.hash)).toBe(canRewrite ? '/detail#section' : '/detail/');
        await frame.evaluate(() => history.back());
        await at('/detail', 'detail');
        await frame.evaluate(() => history.forward());
        await at('/detail#section', 'detail');
        await frame.getByText('Replace', { exact: true }).click();
        await at('/other', 'other');
        await frame.getByText('Back', { exact: true }).click();
        await open.waitFor();
        expect(await shown.isVisible()).toBe(false);

        await page.goto(new URL('root', server.url).href);
        const root = page.frames().find(f => f.parentFrame()), link = root.getByText('Open detail', { exact: true });
        await root.waitForFunction(() => !!history.state);
        await link.click();
        await root.getByTestId('shown').waitFor();
        await root.evaluate(() => history.back());
        await link.waitFor();
        await Promise.all([root.waitForNavigation(), root.evaluate(() => location.reload())]);
        await root.waitForFunction(() => history.state?.url === '/' && !!globalThis.exact?.ready);
        await root.evaluate(() => exact.ready);
        await root.locator('body').click({ position: { x: 1, y: 1 } });
        await root.evaluate(() => history.forward());
        await root.getByTestId('shown').waitFor({ timeout: 5000 });
        expect(await link.isVisible()).toBe(false);

        // Some History failures also refuse a stamp with no URL. Drive both halves of the app's stack then.
        await page.addInitScript(() => {
          History.prototype.pushState = History.prototype.replaceState = () => { throw new DOMException('History unavailable', 'SecurityError'); };
        });
        await page.goto(new URL('memory', server.url).href);
        const memory = page.frames().find(f => f.parentFrame());
        const home = memory.getByText('Open detail', { exact: true }), detail = memory.getByTestId('shown');
        await memory.waitForFunction(() => !!globalThis.exact?.agentSettled);
        await home.click();
        await detail.waitFor();
        expect(await home.isVisible()).toBe(false);
        const travel = delta => memory.evaluate(async history => (await exact.agentSettled({ op: 'tap', history })).delivery, delta);
        await memory.getByText('Back', { exact: true }).click();
        await home.waitFor();
        expect(await travel(1)).toBe('memory');
        await detail.waitFor();
        // The link to the frame's unchanged address still goes Home from the route shown.
        await memory.getByText('Home', { exact: true }).click();
        await home.waitFor();
        expect(await detail.isVisible()).toBe(false);
      } finally { await browser.close(); }
    }
  } finally { server.stop(true); }
}), 120000);
