// The manifest's colours by colour scheme (LLP 1163 D2, scheme-colours.mjs) and the web's first paint, which writes them
// as CSS (host/web/look.mjs `launchBackground`).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { launchBackground } from '../host/web/look.mjs';
import { parseColour, schemeColours } from './scheme-colours.mjs';

test('a scheme\'s own member wins over the top level, which holds in every scheme, and a fixed style shows its side', () => {
  const both = (manifest, style) => schemeColours(manifest, 'background_color', style);
  assert.deepEqual(both({ background_color: '#fff' }), { light: '#fff', dark: '#fff' });
  assert.deepEqual(both({ color_scheme_light: { background_color: '#eee' } }), { light: '#eee', dark: null });
  assert.deepEqual(both({ color_scheme_dark: { background_color: '#111' } }), { light: null, dark: '#111' });
  assert.deepEqual(both({ background_color: '#888', color_scheme_light: { background_color: '#eee' }, color_scheme_dark: { background_color: '#111' } }), { light: '#eee', dark: '#111' });
  assert.deepEqual(both({}), { light: null, dark: null });
  // theme_color is resolved apart from background_color
  assert.deepEqual(schemeColours({ background_color: '#fff', color_scheme_dark: { theme_color: '#123' } }, 'theme_color'), { light: null, dark: '#123' });
  const fixed = { userInterfaceStyle: 'dark', background_color: '#fff', color_scheme_dark: { background_color: '#000' } };
  assert.deepEqual(both(fixed), { light: '#000', dark: '#000' });
  // A host that follows the device whatever the style says (Android)
  assert.deepEqual(both(fixed, null), { light: '#fff', dark: '#000' });
});

test('a colour is hex or Display P3, with its sRGB form clipped, and anything else is refused with the forms it takes', () => {
  assert.deepEqual(parseColour('#abc', 'f'), { space: 'srgb', rgb: [0xaa / 255, 0xbb / 255, 0xcc / 255], alpha: 1, hex: '#aabbcc' });
  assert.equal(parseColour('#12345680', 'f').hex, '#12345680');
  const red = parseColour('color(display-p3 1 0 0)', 'f');
  assert.deepEqual([red.space, red.rgb, red.alpha, red.hex], ['display-p3', [1, 0, 0], 1, '#ff0000']);
  // Within sRGB, Display P3 converts without clipping: P3's (0.5, 0.5, 0.5) is sRGB's grey of the same value
  assert.equal(parseColour('color(display-p3 50% 50% 50% / 0.5)', 'f').hex, '#80808080');
  for (const bad of ['red', 'color(display-p3 1.5 0 0)', 'color(srgb 1 0 0)', 'color(display-p3 1 0)', '#12345']) {
    assert.throws(() => parseColour(bad, 'background_color'), /background_color is .* which is not a colour app\.json takes: write it as #RGB, #RRGGBB or #RRGGBBAA, or in Display P3/);
  }
});

test('the web\'s first paint is each scheme\'s launch colour, and the shell\'s page where none is named', () => {
  const rule = (color) => `html:has(#exact-root:empty){background-color:${color}}`;
  assert.equal(launchBackground({ background_color: '#fff' }), rule('#fff'));
  assert.equal(launchBackground({ background_color: '#fff', color_scheme_dark: { background_color: 'color(display-p3 0.1 0.2 0.3)' } }), rule('light-dark(#fff,color(display-p3 0.1 0.2 0.3))'));
  assert.equal(launchBackground({ color_scheme_light: { background_color: '#f4efe6' } }), rule('light-dark(#f4efe6,var(--exact-background))'));
  assert.equal(launchBackground({ color_scheme_dark: { background_color: '#14110d' } }), rule('light-dark(var(--exact-background),#14110d)'));
  assert.equal(launchBackground({ userInterfaceStyle: 'light', color_scheme_dark: { background_color: '#14110d' } }), '');
  assert.equal(launchBackground({}), '');
});
