// @ref LLP 1123 D7, D17; LLP 1039 D9 — the page's docked keyboard as the web reads it from the visual viewport: a pinch
// zoom shrinks the visual viewport with nothing below it, so it reads no keyboard; keys below an unzoomed or zoomed visual
// viewport read as the layout viewport's height they cover, the layout viewport being the root's client box, which a page
// wider than the device does not stretch as it does `innerHeight`; and the page's keyboard policy, its viewport meta's.
import { expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { keyboardInset, keyboardPolicy } from '../navigation/environment.js';

// An 800-point layout viewport and a visual viewport `height` tall at `scale`, panned down by `offsetTop`
const page = (height, scale, offsetTop = 0) => ({ innerHeight: 800, visualViewport: { height, scale, offsetTop } });

test('a pinch zoom reads no keyboard; keys read the layout height they cover', () => {
  // Zoomed to 2 with no keys: the 400-point visual viewport fills the screen
  expect(keyboardInset(page(400, 2))).toBe(0);
  // 300 points of keys, unzoomed, and with the visual viewport panned 100 down, which leaves 200 of the layout under them
  expect(keyboardInset(page(500, 1))).toBe(300);
  expect(keyboardInset(page(500, 1, 100))).toBe(200);
  // The same keys at a zoom of 2 cover 150 points of the layout
  expect(keyboardInset(page(250, 2))).toBe(150);
  // A page twice the device's width: `innerHeight` 1600, the layout viewport 800 and all of it shown
  expect(keyboardInset({ innerHeight: 1600, document: { documentElement: { clientHeight: 800 } }, visualViewport: { height: 800, scale: 1, offsetTop: 0 } })).toBe(0);
  // The VirtualKeyboard API's own rect, where the page overlays the keys
  expect(keyboardInset({ ...page(800, 1), navigator: { virtualKeyboard: { boundingRect: { height: 280 } } } })).toBe(280);
});

test("the keyboard policy is the viewport meta's, overlaid where the VirtualKeyboard API says so", () => {
  const meta = (content) => ({ document: { querySelector: () => (content == null ? null : { content }) } });
  expect(keyboardPolicy(meta(null))).toBe('resizes-visual');
  expect(keyboardPolicy(meta('width=device-width, initial-scale=1'))).toBe('resizes-visual');
  expect(keyboardPolicy(meta('width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content'))).toBe('resizes-content');
  // A value CSS does not define, as the browser takes it
  expect(keyboardPolicy(meta('width=device-width, interactive-widget=nonsense'))).toBe('resizes-visual');
  expect(keyboardPolicy(meta('width=device-width, interactive-widget=overlays-content!'))).toBe('resizes-visual');
  expect(keyboardPolicy({ ...meta('interactive-widget=resizes-visual'), navigator: { virtualKeyboard: { overlaysContent: true } } })).toBe('overlays-content');
});

// A mobile Chromium page 780 wide on a 390-point screen, its body focused and no keys showing: none, whatever the zoom
test('a page wider than the device reads no keyboard in mobile Chromium', async () => {
  const browser = await suiteBrowser('chromium');
  try {
    const context = await browser.newContext({ viewport: { width: 390, height: 800 }, isMobile: true, hasTouch: true });
    const page = await context.newPage();
    const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/environment.js' || /^\/[\w-]+\.js$/.test(path)) return new Response(Bun.file(resolve(import.meta.dir, '../navigation' + path)), { headers: { 'content-type': 'text/javascript' } });
      return new Response('<!doctype html><meta name="viewport" content="width=device-width, initial-scale=1"><body style="margin:0;width:780px;height:2000px" tabindex="-1"></body>', { headers: { 'content-type': 'text/html' } });
    } });
    try {
      await page.goto(server.url.href);
      const read = await page.evaluate(async () => {
        const { keyboardInset } = await import('/environment.js');
        document.body.focus();
        return { keys: keyboardInset(), inner: innerHeight, client: document.documentElement.clientHeight };
      });
      expect(read.keys).toBe(0);
    } finally {
      server.stop(true);
    }
  } finally {
    await browser.close();
  }
}, 30_000);
