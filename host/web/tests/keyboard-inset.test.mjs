// @ref LLP 1123 D7, D17; LLP 1039 D9 — the page's docked keyboard as the web reads it from the visual viewport: a pinch
// zoom shrinks the visual viewport with nothing below it, so it reads no keyboard; keys below an unzoomed or zoomed visual
// viewport read as the layout viewport's height they cover; and the page's keyboard policy.
import { expect, test } from 'bun:test';
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
  // The VirtualKeyboard API's own rect, where the page overlays the keys
  expect(keyboardInset({ ...page(800, 1), navigator: { virtualKeyboard: { boundingRect: { height: 280 } } } })).toBe(280);
});

test("the keyboard policy is the root's, overlaid where the VirtualKeyboard API says so", () => {
  const root = (value) => ({ document: { querySelector: () => (value ? { getAttribute: () => value } : null) } });
  expect(keyboardPolicy(root(null))).toBe('resizes-visual');
  expect(keyboardPolicy(root('resizes-content'))).toBe('resizes-content');
  expect(keyboardPolicy({ ...root('resizes-visual'), navigator: { virtualKeyboard: { overlaysContent: true } } })).toBe('overlays-content');
});
