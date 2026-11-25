// The Android launcher icon and TV banner an APK packages, from the manifest's `icons` (LLP 1163 D6, agent-android.mjs
// `androidIcons`): which resources are written, their pixels, the adaptive icon's XML and colour, and the refusals.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { androidIcons } from './agent-android.mjs';
import { apkChanges } from './agent-stdio.mjs';
import { placeRgba, readPng, resize } from './app-icons.mjs';
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
/** The build record's entry for an icon the APK packaged: the file and its digest. */
const picked = (dir, src) => ({ file: resolve(dir, src), sha256: createHash('sha256').update(readFileSync(resolve(dir, src))).digest('hex') });
const pixel = (image, x, y) => [...image.data.subarray((y * image.width + x) * 4, (y * image.width + x) * 4 + 4)];

test('a maskable PNG becomes an adaptive icon, inset so its safe zone is the 66 dp one, with the glyph as its themed layer', () => inDir((dir) => {
  // The RGB in a fully transparent corner still supplies the absent launch colour.
  png(dir, 'assets/maskable.png', 512, 512, (x, y) => x + y === 0 ? [0, 0, 255, 0] : [255, 0, 0, 255]);
  png(dir, 'assets/glyph.png', 432, 432, (x, y) => x >= 108 && x < 324 && y >= 108 && y < 324 ? [0, 0, 0, 255] : [0, 0, 0, 0]);
  png(dir, 'assets/banner.png', 1280, 720, () => [0, 128, 0, 255]);
  const res = resolve(dir, 'res');
  mkdirSync(resolve(res, 'drawable'), { recursive: true });
  writeFileSync(resolve(res, 'drawable/stale.png'), '');
  const icons = [{ src: 'assets/maskable.png', purpose: 'maskable' }, { src: 'assets/glyph.png', purpose: 'monochrome' }, { src: 'assets/banner.png' }];
  assert.deepEqual(androidIcons({ icons }, dir, res), { icon: '@mipmap/exact_icon', banner: '@drawable/exact_banner',
    files: ['assets/maskable.png', 'assets/glyph.png', 'assets/banner.png'].map((src) => picked(dir, src)) });
  assert.deepEqual(files(res), ['drawable-nodpi/exact_icon_foreground.png', 'drawable-nodpi/exact_icon_monochrome.png',
    'drawable-xxxhdpi/exact_banner.png', 'mipmap-anydpi/exact_icon.xml', 'values/exact_icon.xml']);
  assert.equal(readFileSync(resolve(res, 'mipmap-anydpi/exact_icon.xml'), 'utf8'), [
    '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">',
    '    <background android:drawable="@color/exact_icon_background" />',
    '    <foreground android:drawable="@drawable/exact_icon_foreground" />',
    '    <monochrome android:drawable="@drawable/exact_icon_monochrome" />',
    '</adaptive-icon>', ''].join('\n'));
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

  // A single subopaque pixel anywhere retains the launch resource, even if it resizes away.
  for (const [x, y, colours, expected] of [
    [123, 0, { background_color: '#0af' }, '#FF00AAFF'],
    [511, 123, { background_color: '#336699' }, '#FF336699'],
    [123, 511, { background_color: '#fff', color_scheme_light: { background_color: '#33669980' } }, '#80336699'],
    [0, 123, {}, '#FF0000FF'],
    [211, 209, { background_color: '#f4f5ef' }, '#FFF4F5EF'],
  ]) {
    png(dir, 'assets/maskable.png', 512, 512, (i, j) => [i % 256, 0, 255, i === x && j === y ? 254 : 255]);
    androidIcons({ icons: icons.slice(0, 1), ...colours }, dir, res);
    assert.deepEqual(files(res), ['drawable-nodpi/exact_icon_foreground.png', 'mipmap-anydpi/exact_icon.xml', 'values/exact_icon.xml']);
    assert.equal(readFileSync(resolve(res, 'values/exact_icon.xml'), 'utf8'),
      `<resources>\n    <color name="exact_icon_background">${expected}</color>\n</resources>\n`);
    assert.doesNotMatch(readFileSync(resolve(res, 'mipmap-anydpi/exact_icon.xml'), 'utf8'), /monochrome/);
    const art = resize(readPng(resolve(dir, 'assets/maskable.png')), 330, 330);
    assert.deepEqual(readFileSync(resolve(res, 'drawable-nodpi/exact_icon_foreground.png')), encodePng(placeRgba(432, 432, art, 51, 51)));
  }
}));

test('opaque flat and gradient artwork supply the adaptive background at every source size', () => inDir((dir) => {
  const src = 'assets/mask.png', res = resolve(dir, 'res');
  const manifest = { icons: [{ src, purpose: 'maskable' }], background_color: '#f4f5ef' };
  const foreground = resolve(res, 'drawable-nodpi/exact_icon_foreground.png');
  for (const size of [432, 512, 660, 1024]) {
    png(dir, src, size, size, () => [18, 145, 72, 255]);
    androidIcons(manifest, dir, res);
    assert.match(readFileSync(resolve(res, 'values/exact_icon.xml'), 'utf8'), />#FF129148</);
    assert.equal(files(res).length, 3);
    png(dir, src, size, size, (x, y) => {
      const t = (0.35 * x + 0.65 * y) / (size - 1);
      return [Math.round(12 + 103 * t), Math.round(68 + 169 * t), Math.round(42 + 59 * t), 255];
    });
    androidIcons(manifest, dir, res);
    assert.match(readFileSync(resolve(res, 'mipmap-anydpi/exact_icon.xml'), 'utf8'), /"@drawable\/exact_icon_background"/);
    const art = readPng(foreground);
    const bg = readPng(resolve(res, 'drawable-xxxhdpi/exact_icon_background.png'));
    // Every exposed pixel, including corners, meets the colour at the nearest foreground edge.
    let worst = 0;
    for (let y = 0; y < 432; y++) for (let x = 0; x < 432; x++) {
      const sx = Math.max(51, Math.min(x, 380)), sy = Math.max(51, Math.min(y, 380));
      if (sx === x && sy === y) continue;
      for (let c = 0; c < 3; c++) worst = Math.max(worst, Math.abs(bg.data[4 * (y * 432 + x) + c] - art.data[4 * (sy * 432 + sx) + c]));
    }
    assert.ok(worst <= 1, `${size} px gradient differs by ${worst}`);
  }
  utimesSync(foreground, 1, 1);
  androidIcons({ icons: manifest.icons }, dir, res);
  assert.equal(statSync(foreground).mtimeMs, 1000, 'removing the launch colour preserves opaque artwork');
}));

test('the bleed averages fine stripes and rims and follows a broad border', () => inDir((dir) => {
  const src = 'assets/mask.png', res = resolve(dir, 'res');
  const manifest = { icons: [{ src, purpose: 'maskable' }], background_color: '#f4f5ef' };
  function generate(grey) {
    png(dir, src, 660, 660, (x, y) => { const v = grey(x, y); return [v, v, v, 255]; });
    androidIcons(manifest, dir, res);
  }
  for (const grey of [
    (_, y) => y === 0 ? 254 : y === 1 ? 2 : 128,
    (x, y) => (x + y) % 2 * 255,
  ]) {
    generate(grey);
    assert.match(readFileSync(resolve(res, 'values/exact_icon.xml'), 'utf8'), />#FF808080</);
  }
  for (const paint of [x => [1, 1, 128, 128, 255, 255][x % 6], x => x < 6 ? 1 : x >= 654 ? 255 : 128]) {
    generate(paint);
    for (const [density, size] of Object.entries({ ldpi: 81, mdpi: 108, tvdpi: 144, hdpi: 162, xhdpi: 216, xxhdpi: 324, xxxhdpi: 432 })) {
      const bg = readPng(resolve(res, `drawable-${density}/exact_icon_background.png`));
      assert.deepEqual([bg.width, bg.height], [size, size]);
      assert.ok(bg.data.every((v, i) => i % 4 === 3 ? v === 255 : Math.abs(v - 128) <= 3), density);
    }
  }
  // One extra rendered row of ink changes the local box mean by six levels.
  generate((x, y) => y < (x < 330 ? 20 : 22) ? 0 : 128);
  const stepped = readPng(resolve(res, 'drawable-xxxhdpi/exact_icon_background.png'));
  assert.ok(Math.abs(pixel(stepped, 180, 25)[0] - 66) <= 3);
  assert.ok(Math.abs(pixel(stepped, 252, 25)[0] - 60) <= 3);
  generate(x => x < 42 ? 80 : 176);
  const broad = readPng(resolve(res, 'drawable-xxxhdpi/exact_icon_background.png'));
  assert.deepEqual(pixel(broad, 0, 216), [80, 80, 80, 255]);
}));

test('an app with no icons builds into a directory that does not exist yet', () => inDir((dir) => {
  const res = resolve(dir, 'build/app/icons/res');
  assert.deepEqual(androidIcons({}, dir, res), { icon: null, banner: null, files: [] });
  assert.deepEqual(androidIcons({}, dir, res), { icon: null, banner: null, files: [] });
  assert.deepEqual(files(res), []);
}));

test('without a maskable a square any PNG is the legacy icon at 192 px, and with no icons there is nothing', () => inDir((dir) => {
  png(dir, 'assets/icon.png', 1024, 1024, () => [10, 20, 30, 255]);
  png(dir, 'assets/small-mask.png', 256, 256, () => [0, 0, 0, 255]);
  png(dir, 'assets/glyph.png', 432, 432, (x) => [0, 0, 0, x < 216 ? 255 : 0]);
  const res = resolve(dir, 'res');
  // A glyph without an adaptive icon has no layer to go in, and a maskable too small for it leaves the icon to `any`.
  for (const icons of [[{ src: 'assets/icon.png' }, { src: 'assets/glyph.png', purpose: 'monochrome' }], [{ src: 'assets/small-mask.png', purpose: 'maskable' }, { src: 'assets/icon.png' }]]) {
    assert.deepEqual(androidIcons({ icons }, dir, res), { icon: '@mipmap/exact_icon', banner: null, files: [picked(dir, 'assets/icon.png')] });
    assert.deepEqual(files(res), ['mipmap-xxxhdpi/exact_icon.png']);
    const legacy = readPng(resolve(res, 'mipmap-xxxhdpi/exact_icon.png'));
    assert.deepEqual([legacy.width, legacy.height, pixel(legacy, 96, 96)], [192, 192, [10, 20, 30, 255]]);
  }
  // Dark icons are not Android's: color_scheme_dark.icons is never read.
  assert.deepEqual(androidIcons({ color_scheme_dark: { icons: [{ src: 'assets/icon.png' }] } }, dir, res), { icon: null, banner: null, files: [] });
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

test('an unchanged icon is not made again, and a new launch colour reaches a transparent edge', () => inDir((dir) => {
  png(dir, 'assets/maskable.png', 432, 432, (x) => [255, 0, 0, x === 0 ? 0 : 255]);
  const res = resolve(dir, 'res'), manifest = { icons: [{ src: 'assets/maskable.png', purpose: 'maskable' }] };
  const made = () => statSync(resolve(res, 'drawable-nodpi/exact_icon_foreground.png')).mtimeMs;
  androidIcons(manifest, dir, res);
  const first = made();
  Bun.sleepSync(20);
  assert.deepEqual(androidIcons(manifest, dir, res), { icon: '@mipmap/exact_icon', banner: null, files: [picked(dir, 'assets/maskable.png')] });
  assert.equal(made(), first);
  androidIcons({ ...manifest, background_color: '#000' }, dir, res);
  assert.notEqual(made(), first);
  assert.match(readFileSync(resolve(res, 'values/exact_icon.xml'), 'utf8'), /#FF000000/);
}));

test("an APK is stale when an icon it packaged changed or is gone, and not when one was only touched", () => inDir((dir) => {
  png(dir, 'assets/icon.png', 192, 192, () => [10, 20, 30, 255]);
  const { files: sources } = androidIcons({ icons: [{ src: 'assets/icon.png' }] }, dir, resolve(dir, 'res'));
  const apk = resolve(dir, 'build/app-release.apk'), icon = resolve(dir, 'assets/icon.png');
  mkdirSync(resolve(dir, 'build'));
  writeFileSync(resolve(dir, 'build/exact-build.json'), JSON.stringify({ started: new Date().toISOString(), abis: ['arm64-v8a'], libraries: [resolve(dir, 'libapp.so')], workspace: dir, sources }));
  const app = { dir };
  assert.deepEqual(apkChanges(app, apk), []);
  const later = new Date(Date.now() + 60_000);
  utimesSync(icon, later, later);
  assert.deepEqual(apkChanges(app, apk), [], 'the same bytes at a later time');
  png(dir, 'assets/icon.png', 192, 192, () => [30, 20, 10, 255]);
  assert.deepEqual(apkChanges(app, apk), [icon]);
  rmSync(icon);
  assert.deepEqual(apkChanges(app, apk), [icon]);
}));
