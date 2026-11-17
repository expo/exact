import { beforeAll, test, expect } from 'bun:test';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { renderedPages, warmRustRenderer } from './rendered-pages.mjs';

beforeAll(warmRustRenderer, 1_800_000);

test('canvases follow parent justify-items before and after adoption and position writes', async () => {
  const source = `routes nav
  home "/" render=build

component CanvasDocument
  state items = "normal"
  state absolute = false
  action toggle
    items = items == "normal" ? "center" : "normal"
  action position
    absolute = not absolute
  view
    column
      button "Toggle" testId="toggle" press=toggle width=100 height=24
      button "Position" testId="position" press=position width=100 height=24
      box justify-items=items width=240 height=40
        canvas testId="bound" height=40
      box justify-items="center" width=240 height=60
        canvas testId="outer" height=50 justify-items="normal"
          canvas testId="inner" height=20
      box position="relative" justify-items="center" width=100 height=20
        canvas testId="absolute" position="absolute" width=20 height=10
      box position="relative" justify-items=items width=100 height=20
        canvas testId="bound-absolute" position="absolute" width=20 height=10
      box position="relative" justify-items="center" width=100 height=20
        canvas testId="position-bound" position=(absolute ? "absolute" : "relative") width=20 height=10
      box position="relative" justify-items="center" width=100 height=20
        canvas testId="inset" position="absolute" left=7 width=20 height=10
      box position="relative" direction="rtl" justify-items="left" width=100 height=20
        canvas testId="rtl-left" position="absolute" direction="ltr" width=20 height=10
      box position="relative" direction="rtl" justify-items="right" width=100 height=20
        canvas testId="rtl-right" position="absolute" direction="ltr" width=20 height=10
      row position="relative" justify-items="center" justify-content="flex-start" width=100 height=20
        canvas testId="flex-canvas" position="absolute" width=20 height=10
        progress testId="flex-progress" position="absolute" width=20 height=10
      box display=(absolute ? "grid" : "flex") position="relative" justify-items=items justify-content="flex-start" width=100 height=20
        canvas testId="display-bound" position="absolute" width=20 height=10
`;
  for (const renderer of ['rust', 'js']) await renderedPages(source, async (_, html, dist) => {
    let browser, suite;
    const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    try {
      const measure = `[
        'bound','outer','inner','absolute','bound-absolute','position-bound','inset',
        'rtl-left','rtl-right','flex-canvas','flex-progress','display-bound',
      ].map(id => {
        const e = document.querySelector('[data-testid="'+id+'"]'), r = e.getBoundingClientRect(), p = e.parentElement.getBoundingClientRect();
        return { x: r.x - p.x, width: r.width };
      })`;
      const frames = (centered, grid = false) => [
        { x: centered ? 80 : 0, width: 80 }, { x: 70, width: 100 }, { x: 0, width: 40 },
        { x: 40, width: 20 }, { x: centered ? 40 : 0, width: 20 }, { x: 40, width: 20 }, { x: 7, width: 20 },
        { x: 80, width: 20 }, { x: 0, width: 20 }, { x: 0, width: 20 }, { x: 0, width: 20 },
        { x: grid && centered ? 40 : 0, width: 20 },
      ];
      suite = await suiteBrowser();
      const page = await suite.newPage({ javaScriptEnabled: false });
      await page.goto(server.url.href);
      expect(await page.evaluate(measure), renderer + ' before adoption').toEqual(frames(false));
      browser = await open({ host: 'web', url: server.url.href });
      expect(await browser.carrier.evaluate(`globalThis.exact.journal.some(l => l.endsWith('adopted the document'))`)).toBe(true);
      expect(await browser.carrier.evaluate(measure), renderer + ' after adoption').toEqual(frames(false));
      await browser.tap('position');
      expect(await browser.carrier.evaluate(measure), renderer + ' absolute').toEqual(frames(false, true));
      await browser.tap('toggle');
      expect(await browser.carrier.evaluate(measure), renderer + ' centered').toEqual(frames(true, true));
      await browser.tap('position');
      expect(await browser.carrier.evaluate(measure), renderer + ' relative').toEqual(frames(true));
      await browser.tap('toggle');
      expect(await browser.carrier.evaluate(measure), renderer + ' reset').toEqual(frames(false));
    } finally {
      await browser?.close(); await suite?.close(); server.stop(true);
    }
  }, {}, {}, renderer);
}, 180000);

// The same CSS also supplies the independently measured kernel cases.
test('calculated grid tracks survive web build, adoption, resize and a bound write', async () => {
  const cards = 'repeat(auto-fill, minmax(min(100%, 200px), 1fr))';
  const source = `routes nav
  home "/" render=build
component App
  state columns = "${cards}"
  state wide = false
  action resize
    wide = not wide
  action change
    columns = "calc(5e-1 * 100% - 10px) calc(5e-1 * 100% - 10px)"
  view
    view
      button "Resize" press=resize testId="resize"
      button "Change" press=change testId="change"
      view display="grid" width=(wide ? 650 : 150) grid-template-columns=columns testId="cards"
        view min-height=30
        view min-height=30
        view min-height=30
      view display="grid" width=100 height=200 grid-template-rows="clamp(20px, 25%, 80px) max(10px, 20%)" testId="rows"
        view
        view
`;
  await renderedPages(source, async (_, html, dist) => {
    expect(html('')).toContain(cards);
    const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    let browser;
    try {
      browser = await open({ host: 'web', url: server.url.href });
      const widths = () => browser.carrier.evaluate(`Array.from(document.querySelector('[data-testid="cards"]').children, e => e.getBoundingClientRect().width)`);
      expect(await widths()).toEqual([150, 150, 150]);
      expect(await browser.carrier.evaluate(`getComputedStyle(document.querySelector('[data-testid="rows"]')).gridTemplateRows`)).toBe('50px 40px');
      await browser.tap('resize');
      expect(await widths()).toEqual([216.65625, 216.671875, 216.671875]);
      await browser.tap('change');
      expect(await widths()).toEqual([315, 315, 315]);
      await browser.tap('resize');
      expect(await widths()).toEqual([65, 65, 65]);
      expect(await browser.carrier.evaluate(`globalThis.exact.journal.filter(l => l.includes('unset grid-template'))`)).toEqual([]);
    } finally { await browser?.close(); server.stop(true); }
  });
}, 180000);
