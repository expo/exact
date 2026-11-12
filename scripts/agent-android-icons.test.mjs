// The Android launcher icon and TV banner an APK packages, from the manifest's `icons` (LLP 1163 D6, agent-android.mjs
// `androidIcons`): which resources are written, their pixels, the adaptive icon's XML and colour, and the refusals.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { androidIcons } from './agent-android.mjs';
import { readPng } from './app-icons.mjs';
import { encodePng } from './png.mjs';

const inDir = (body) => { const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-icons-')); try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); } };
/** A `width` × `height` RGBA PNG under the app whose pixel at x, y is `paint(x, y)`. */
const png = (dir, src, width, height, paint) => {
  const samples = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) samples.set(paint(x, y), (y * width + x) * 4);
  mkdirSync(resolve(dir, src, '..'), { recursive: true });
  writeFileSync(resolve(dir, src), encodePng({ width, height, data: samples }));
};
const files = (res) => existsSync(res) ? readdirSync(res, { recursive: true }).filter((path) => statSync(resolve(res, path)).isFile()).sort() : [];
const pixel = (image, x, y) => [...image.data.subarray((y * image.width + x) * 4, (y * image.width + x) * 4 + 4)];

test('a maskable PNG becomes an adaptive icon, inset so its safe zone is the 66 dp one, with the glyph as its themed layer', () => inDir((dir) => {
  // Red with a blue top-left pixel; the glyph an opaque square in its middle half.
  png(dir, 'assets/maskable.png', 512, 512, (x, y) => x + y === 0 ? [0, 0, 255, 255] : [255, 0, 0, 255]);
  png(dir, 'assets/glyph.png', 432, 432, (x, y) => x >= 108 && x < 324 && y >= 108 && y < 324 ? [0, 0, 0, 255] : [0, 0, 0, 0]);
  png(dir, 'assets/banner.png', 1280, 720, () => [0, 128, 0, 255]);
  const res = resolve(dir, 'res');
  mkdirSync(resolve(res, 'drawable'), { recursive: true });
  writeFileSync(resolve(res, 'drawable/stale.png'), '');
  const icons = [{ src: 'assets/maskable.png', purpose: 'maskable' }, { src: 'assets/glyph.png', purpose: 'monochrome' }, { src: 'assets/banner.png' }];
  assert.deepEqual(androidIcons({ icons }, dir, res), { icon: '@mipmap/exact_icon', banner: '@drawable/exact_banner' });
  assert.deepEqual(files(res), ['drawable-nodpi/exact_icon_foreground.png', 'drawable-nodpi/exact_icon_monochrome.png',
    'drawable-xxxhdpi/exact_banner.png', 'mipmap-anydpi/exact_icon.xml', 'values/exact_icon.xml']);
  assert.equal(readFileSync(resolve(res, 'mipmap-anydpi/exact_icon.xml'), 'utf8'), [
    '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">',
    '    <background android:drawable="@color/exact_icon_background" />',
    '    <foreground android:drawable="@drawable/exact_icon_foreground" />',
    '    <monochrome android:drawable="@drawable/exact_icon_monochrome" />',
    '</adaptive-icon>', ''].join('\n'));
  // Without background_color the layer behind is the image's top-left pixel.
  assert.match(readFileSync(resolve(res, 'values/exact_icon.xml'), 'utf8'), /<color name="exact_icon_background">#FF0000FF<\/color>/);
  // 330 px of 432, centred: 51 px of transparency on each side.
  const foreground = readPng(resolve(res, 'drawable-nodpi/exact_icon_foreground.png'));
  assert.deepEqual([foreground.width, foreground.height], [432, 432]);
  assert.deepEqual([pixel(foreground, 50, 216), pixel(foreground, 51, 216), pixel(foreground, 380, 216), pixel(foreground, 381, 216)],
    [[0, 0, 0, 0], [255, 0, 0, 255], [255, 0, 0, 255], [0, 0, 0, 0]]);
  // The glyph white, by its own alpha, at the same inset: its square spans 82.5 to 247.5 of the 330 px.
  const glyph = readPng(resolve(res, 'drawable-nodpi/exact_icon_monochrome.png'));
  assert.deepEqual([glyph.width, glyph.height], [432, 432]);
  assert.deepEqual([pixel(glyph, 216, 216), pixel(glyph, 51 + 80, 216), pixel(glyph, 51 + 84, 216)], [[255, 255, 255, 255], [255, 255, 255, 0], [255, 255, 255, 255]]);
  const banner = readPng(resolve(res, 'drawable-xxxhdpi/exact_banner.png'));
  assert.deepEqual([banner.width, banner.height, pixel(banner, 0, 0)], [640, 360, [0, 128, 0, 255]]);

  // background_color, in each CSS form, as Android's #AARRGGBB; with no glyph, no <monochrome>.
  for (const [css, android] of [['#0af', '#FF00AAFF'], ['#336699', '#FF336699'], ['#33669980', '#80336699']]) {
    androidIcons({ icons: icons.slice(0, 1), background_color: css }, dir, res);
    assert.match(readFileSync(resolve(res, 'values/exact_icon.xml'), 'utf8'), new RegExp(`>${android}<`));
  }
  assert.deepEqual(files(res), ['drawable-nodpi/exact_icon_foreground.png', 'mipmap-anydpi/exact_icon.xml', 'values/exact_icon.xml']);
  assert.doesNotMatch(readFileSync(resolve(res, 'mipmap-anydpi/exact_icon.xml'), 'utf8'), /monochrome/);
}));

test('without a maskable a square any PNG is the legacy icon at 192 px, and with no icons there is nothing', () => inDir((dir) => {
  png(dir, 'assets/icon.png', 1024, 1024, () => [10, 20, 30, 255]);
  png(dir, 'assets/small-mask.png', 256, 256, () => [0, 0, 0, 255]);
  png(dir, 'assets/glyph.png', 432, 432, (x) => [0, 0, 0, x < 216 ? 255 : 0]);
  const res = resolve(dir, 'res');
  // A glyph without an adaptive icon has no layer to go in, and a maskable too small for it leaves the icon to `any`.
  for (const icons of [[{ src: 'assets/icon.png' }, { src: 'assets/glyph.png', purpose: 'monochrome' }], [{ src: 'assets/small-mask.png', purpose: 'maskable' }, { src: 'assets/icon.png' }]]) {
    assert.deepEqual(androidIcons({ icons }, dir, res), { icon: '@mipmap/exact_icon', banner: null });
    assert.deepEqual(files(res), ['mipmap-xxxhdpi/exact_icon.png']);
    const legacy = readPng(resolve(res, 'mipmap-xxxhdpi/exact_icon.png'));
    assert.deepEqual([legacy.width, legacy.height, pixel(legacy, 96, 96)], [192, 192, [10, 20, 30, 255]]);
  }
  // Dark icons are not Android's: color_scheme_dark.icons is never read.
  assert.deepEqual(androidIcons({ color_scheme_dark: { icons: [{ src: 'assets/icon.png' }] } }, dir, res), { icon: null, banner: null });
  assert.deepEqual(files(res), []);
}));

test('every file too small for its slot, and a glyph with no transparency, is refused in one message before anything is written', () => inDir((dir) => {
  png(dir, 'assets/mask.png', 400, 400, () => [0, 0, 0, 255]);
  png(dir, 'assets/any.png', 128, 128, () => [0, 0, 0, 255]);
  png(dir, 'assets/banner.png', 320, 180, () => [0, 0, 0, 255]);
  png(dir, 'assets/big-mask.png', 432, 432, () => [0, 0, 0, 255]);
  png(dir, 'assets/solid.png', 432, 432, () => [0, 0, 0, 255]);
  const res = resolve(dir, 'res');
  mkdirSync(res);
  writeFileSync(resolve(res, 'kept.xml'), '');
  const refused = (icons) => { try { androidIcons({ icons }, dir, res); } catch (error) { return error.message.split('\n'); } assert.fail('not refused'); };
  assert.deepEqual(refused([{ src: 'assets/mask.png', purpose: 'maskable' }, { src: 'assets/any.png' }, { src: 'assets/banner.png' }]), [
    "icons[0]: assets/mask.png is 400×400 px, and Android's adaptive icon needs at least 432 px; use a larger file",
    "icons[1]: assets/any.png is 128×128 px, and Android's launcher icon needs at least 192 px; use a larger file",
    'icons[2]: assets/banner.png is 320×180 px, and the Android TV banner needs at least 640×360 px; use a larger file',
  ]);
  // A maskable too small with no `any` to fall back to is refused alone.
  assert.deepEqual(refused([{ src: 'assets/mask.png', purpose: 'maskable' }]), [
    "icons[0]: assets/mask.png is 400×400 px, and Android's adaptive icon needs at least 432 px; use a larger file",
  ]);
  assert.deepEqual(refused([{ src: 'assets/big-mask.png', purpose: 'maskable' }, { src: 'assets/solid.png', purpose: 'monochrome' }]), [
    `icons[1]: assets/solid.png is "monochrome" but has no transparent pixels, so Android's themed icon would be a solid square; draw the glyph on a transparent background`,
  ]);
  assert.deepEqual(files(res), ['kept.xml'], 'nothing written or removed');
}));

test('an unchanged icon is not made again, and a new background colour makes it again', () => inDir((dir) => {
  png(dir, 'assets/maskable.png', 432, 432, () => [255, 0, 0, 255]);
  const res = resolve(dir, 'res'), manifest = { icons: [{ src: 'assets/maskable.png', purpose: 'maskable' }] };
  const made = () => statSync(resolve(res, 'drawable-nodpi/exact_icon_foreground.png')).mtimeMs;
  androidIcons(manifest, dir, res);
  const first = made();
  Bun.sleepSync(20);
  assert.deepEqual(androidIcons(manifest, dir, res), { icon: '@mipmap/exact_icon', banner: null });
  assert.equal(made(), first);
  androidIcons({ ...manifest, background_color: '#000' }, dir, res);
  assert.notEqual(made(), first);
  assert.match(readFileSync(resolve(res, 'values/exact_icon.xml'), 'utf8'), /#FF000000/);
}));
