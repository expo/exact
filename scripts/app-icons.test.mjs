// App icons (LLP 1163): what the manifest's icons are, which file each slot takes, and the web's head.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { crc32, deflateSync } from 'node:zlib';
import { appIconProblems, iconFile, opaque, pickIcon, placeRgba, pngAlpha, readPng, resize, webIconLinks, webThemeColors, WINDOWS_ICON_SIZES, windowsIconRes, writeTinted } from './app-icons.mjs';
import { encodePng } from './png.mjs';

const inDir = (body) => { const dir = mkdtempSync(resolve(tmpdir(), 'exact-icons-')); try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); } };
/** A `width` × `height` image of one RGBA colour. */
const image = (width, height, colour = [255, 255, 255, 255]) => ({ width, height, data: Uint8Array.from({ length: width * height * 4 }, (_, i) => colour[i % 4]) });
const put = (dir, src, bytes) => { mkdirSync(resolve(dir, src, '..'), { recursive: true }); writeFileSync(resolve(dir, src), bytes); return resolve(dir, src); };
/** A square PNG under the app, opaque or, with `hole`, with its first pixel transparent. */
const png = (dir, src, side, { hole = false } = {}) => {
  const square = image(side, side);
  if (hole) square.data[3] = 0;
  put(dir, src, encodePng(square));
};
/** An opaque `width` × `height` PNG under the app. */
const rect = (dir, src, width, height) => put(dir, src, encodePng(image(width, height)));
const svg = (dir, src, box = '0 0 24 24') => put(dir, src, `<?xml version="1.0"?>\n<svg xmlns="http://www.w3.org/2000/svg" viewBox="${box}"/>`);
/** A PNG of any form from its header fields, chunks and unfiltered rows, as no encoder here writes. */
const raw = (width, height, depth, colour, rows, { interlace = 0, chunks = [] } = {}) => {
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type, 'latin1'), data]), length = Buffer.alloc(4), crc = Buffer.alloc(4);
    length.writeUInt32BE(data.length); crc.writeUInt32BE(crc32(body));
    return Buffer.concat([length, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width); ihdr.writeUInt32BE(height, 4); ihdr[8] = depth; ihdr[9] = colour; ihdr[12] = interlace;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), ...chunks.map(([type, data]) => chunk(type, Buffer.from(data))),
    chunk('IDAT', deflateSync(Buffer.from(rows.flatMap((row) => [0, ...row])))), chunk('IEND', Buffer.alloc(0))]);
};
const IOS = { purposes: ['maskable', 'any'], px: 1024, formats: ['png'], native: true, opaque: 'refuse', name: 'the iOS icon' };

test('the manifest refuses icons outside assets/, missing or unknown files, sizes and types not their own, and masked dark icons', () => inDir((dir) => {
  png(dir, 'assets/icon.png', 64);
  writeFileSync(resolve(dir, 'assets/notes.txt'), 'not an image');
  png(dir, 'art/icon.png', 64);
  const manifest = {
    icons: [{ src: 'assets/icon.png', sizes: '64x64', type: 'image/png', purpose: 'any maskable' }, { src: 'art/icon.png' }, { src: 'assets/gone.png' },
      { src: 'assets/notes.txt' }, { src: 'assets/icon.png', sizes: '1024x1024', type: 'image/svg+xml' }],
    color_scheme_dark: { icons: [{ src: 'assets/icon.png', purpose: 'maskable' }, { src: 'assets/icon.png' }] },
  };
  assert.deepEqual(appIconProblems(manifest, dir), [
    'icons[1].src: art/icon.png is outside assets/, the directory the web root serves; move it under assets/',
    'icons[2].src: assets/gone.png does not exist',
    'icons[3].src: assets/notes.txt is not a PNG, SVG or ICO',
    'icons[4].sizes: "1024x1024", but assets/icon.png is 64x64; write "64x64" or leave sizes out',
    'icons[4].type: "image/svg+xml", but assets/icon.png is image/png; write "image/png" or leave type out',
    'color_scheme_dark.icons[0].purpose: "maskable"; a dark icon is "any" only, since no host varies a mask or a glyph with the scheme',
  ]);
  // An SVG's root may follow a long prolog, and its width is not its stroke-width.
  put(dir, 'assets/late.svg', `<?xml version="1.0"?>\n<!--${'x'.repeat(5000)}-->\n<svg xmlns="http://www.w3.org/2000/svg" stroke-width="2" width="100" height="50"/>`);
  assert.deepEqual([iconFile(dir, 'assets/late.svg').format, iconFile(dir, 'assets/late.svg').ratio], ['svg', 2]);
  // An ICO's directory gives its sizes.
  const ico = Buffer.alloc(6 + 16 * 3);
  ico.writeUInt16LE(1, 2); ico.writeUInt16LE(3, 4);
  [32, 0, 16].forEach((side, i) => { ico[6 + 16 * i] = ico[7 + 16 * i] = side; });
  put(dir, 'assets/icon.ico', ico);
  assert.equal(iconFile(dir, 'assets/icon.ico').sizes, '32x32 256x256 16x16');
}));

test('PNG decodes every form to RGBA, opacity and the tinted icon read its alpha, resizing premultiplies, and compositing is over', () => inDir((dir) => {
  const at = (bytes) => [...readPng(put(dir, 'x.png', bytes)).data];
  // A palette with transparency, 16-bit grey with alpha, 1-bit grey, and an interlaced 2×2 RGB image.
  assert.deepEqual(at(raw(2, 1, 8, 3, [[0, 1]], { chunks: [['PLTE', [255, 0, 0, 0, 0, 255]], ['tRNS', [128]]] })), [255, 0, 0, 128, 0, 0, 255, 255]);
  assert.deepEqual(at(raw(1, 1, 16, 4, [[0x80, 0, 0xff, 0xff]])), [128, 128, 128, 255]);
  assert.deepEqual(at(raw(2, 1, 1, 0, [[0b10000000]])), [255, 255, 255, 255, 0, 0, 0, 255]);
  // Adam7 on 2×2: pass 1 is (0,0), pass 6 is (1,0), pass 7 is the second row.
  assert.deepEqual(at(raw(2, 2, 8, 2, [[1, 1, 1], [2, 2, 2], [3, 3, 3, 4, 4, 4]], { interlace: 1 })),
    [1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255]);
  png(dir, 'assets/solid.png', 4);
  png(dir, 'assets/holed.png', 4, { hole: true });
  assert.equal(opaque(resolve(dir, 'assets/solid.png')), true);
  assert.equal(opaque(resolve(dir, 'assets/holed.png')), false);
  assert.equal(opaque(put(dir, 'rgb.png', raw(1, 1, 8, 2, [[9, 9, 9]]))), true, 'RGB without tRNS, from its header');
  // White, its alpha ramping 0, 85, 170, 255.
  const ramp = image(4, 1);
  [0, 85, 170, 255].forEach((a, i) => { ramp.data[4 * i + 3] = a; });
  put(dir, 'ramp.png', encodePng(ramp));
  assert.deepEqual([...pngAlpha(resolve(dir, 'ramp.png')).alpha], [0, 85, 170, 255]);
  writeTinted(resolve(dir, 'ramp.png'), resolve(dir, 'tinted.png'));
  assert.deepEqual([...readPng(resolve(dir, 'tinted.png')).data], [0, 0, 0, 255, 85, 85, 85, 255, 170, 170, 170, 255, 255, 255, 255, 255]);
  // Red beside transparent green: the transparent half adds no green.
  const pair = { width: 2, height: 1, data: new Uint8Array([255, 0, 0, 255, 0, 255, 0, 0]) };
  assert.deepEqual([...resize(pair, 1, 1).data], [255, 0, 0, 128]);
  assert.deepEqual([...placeRgba(3, 1, image(1, 1, [0, 0, 255, 255]), 1, 0, [255, 255, 255, 255]).data], [255, 255, 255, 255, 0, 0, 255, 255, 255, 255, 255, 255]);
  assert.deepEqual([...placeRgba(1, 1, image(1, 1, [255, 255, 255, 128])).data], [255, 255, 255, 128], 'half-transparent white over nothing stays white');
}));

test('the Windows icon resource holds an RT_ICON PNG per size and RT_GROUP_ICON 1 naming them', () => {
  const res = windowsIconRes(image(512, 512));
  const entries = [];
  for (let at = 0; at < res.length;) {
    const size = res.readUInt32LE(at), header = res.readUInt32LE(at + 4);
    entries.push({ type: res.readUInt16LE(at + 10), name: res.readUInt16LE(at + 14), data: res.subarray(at + header, at + header + size) });
    at += header + size + ((4 - (size % 4)) % 4);
  }
  assert.deepEqual(entries.map((e) => [e.type, e.name]), [[0, 0], ...WINDOWS_ICON_SIZES.map((_, i) => [3, i + 1]), [14, 1]]);
  assert.deepEqual(entries.slice(1, -1).map((e) => e.data.readUInt32BE(16)), WINDOWS_ICON_SIZES, 'each RT_ICON is a PNG of its size');
  const group = entries.at(-1).data;
  assert.deepEqual([group.readUInt16LE(2), group.readUInt16LE(4)], [1, WINDOWS_ICON_SIZES.length]);
  assert.deepEqual(WINDOWS_ICON_SIZES.map((_, i) => [group[6 + 14 * i] || 256, group.readUInt32LE(6 + 14 * i + 8), group.readUInt16LE(6 + 14 * i + 12)]),
    WINDOWS_ICON_SIZES.map((side, i) => [side, entries[i + 1].data.length, i + 1]));
});

test('a slot takes its preferred purpose, then the smallest file large enough, and refuses what it cannot draw', () => inDir((dir) => {
  png(dir, 'assets/tiny.png', 16);
  png(dir, 'assets/small.png', 512);
  png(dir, 'assets/any.png', 2048);
  png(dir, 'assets/exact.png', 1024);
  png(dir, 'assets/masked.png', 1024);
  rect(dir, 'assets/wide.png', 32, 18);
  const pick = (icons, slot = IOS) => pickIcon({ icons }, dir, slot)?.src;
  assert.equal(pick([{ src: 'assets/any.png' }, { src: 'assets/exact.png' }]), 'assets/exact.png', 'the smallest of at least 1024 px');
  assert.equal(pick([{ src: 'assets/exact.png' }, { src: 'assets/masked.png', purpose: 'maskable' }]), 'assets/masked.png', 'maskable before any');
  assert.equal(pick([{ src: 'assets/wide.png' }]), undefined, 'a 16:9 file is no square');
  assert.equal(pick([{ src: 'assets/wide.png' }], { purposes: ['any'], shape: 16 / 9, px: 32, formats: ['png'], native: true }), 'assets/wide.png');
  assert.throws(() => pick([{ src: 'assets/tiny.png' }, { src: 'assets/small.png' }]), /icons\[1\]: assets\/small\.png is 512×512 px, and the iOS icon needs at least 1024 px/, 'the largest is named');
  assert.equal(pick([{ src: 'assets/small.png' }], { ...IOS, native: false }), 'assets/small.png', 'the web links a smaller file');
  png(dir, 'assets/holed.png', 1024, { hole: true });
  assert.throws(() => pick([{ src: 'assets/holed.png' }]), /has transparent pixels, and the iOS icon must be opaque/);
  assert.equal(pick([{ src: 'assets/holed.png' }, { src: 'assets/any.png' }]), 'assets/any.png', 'a transparent file is passed over for an opaque one');
  rect(dir, 'assets/near.png', 1024, 1023);
  assert.equal(pick([{ src: 'assets/near.png' }]), undefined, 'a square is exactly square');
  rect(dir, 'assets/short.png', 640, 359);
  assert.throws(() => pick([{ src: 'assets/short.png' }], { purposes: ['any'], shape: 16 / 9, px: 640, formats: ['png'], native: true, name: 'the banner' }),
    /640×359 px, and the banner needs at least 640×360 px/, 'a slot has a height as well as a width');
  assert.throws(() => pick([{ src: 'assets/exact.png', purpose: 'monochrome' }], { ...IOS, purposes: ['monochrome'], opaque: undefined, glyph: true }), /has no transparent pixels/);
  // The dark list replaces the light one; without it there is no dark icon.
  assert.equal(pickIcon({ icons: [{ src: 'assets/exact.png' }] }, dir, { ...IOS, scheme: 'dark' }), null);
  assert.equal(pickIcon({ icons: [{ src: 'assets/exact.png' }], color_scheme_dark: { icons: [{ src: 'assets/any.png' }] } }, dir, { ...IOS, scheme: 'dark' }).src, 'assets/any.png');
}));

test('the web head links a raster then an SVG favicon per scheme, the light ones last, then the touch icon, and the dark theme colour first', () => inDir((dir) => {
  png(dir, 'assets/tab.png', 32);
  png(dir, 'assets/app.png', 1024);
  png(dir, 'assets/app-dark.png', 1024);
  svg(dir, 'assets/mark.svg');
  svg(dir, 'assets/mark-dark.svg');
  const fallback = '<link rel="icon" href="./default.png">';
  const links = (manifest) => webIconLinks(manifest, dir, fallback).split('\n');
  assert.deepEqual(links({}), [fallback]);
  assert.deepEqual(links({ icons: [{ src: 'assets/app.png' }, { src: 'assets/tab.png' }], color_scheme_dark: { icons: [{ src: 'assets/tab.png' }, { src: 'assets/app-dark.png' }] } }), [
    '<link rel="icon" type="image/png" sizes="32x32" href="./assets/tab.png">',
    '<link rel="apple-touch-icon" sizes="1024x1024" href="./assets/app.png">',
  ], 'one file in both schemes is one link');
  assert.deepEqual(links({ icons: [{ src: 'assets/app.png' }, { src: 'assets/mark.svg' }], color_scheme_dark: { icons: [{ src: 'assets/mark-dark.svg' }] } }), [
    '<link rel="icon" type="image/svg+xml" sizes="any" href="./assets/mark-dark.svg" media="(prefers-color-scheme: dark)">',
    '<link rel="icon" type="image/png" sizes="1024x1024" href="./assets/app.png" media="(prefers-color-scheme: light)">',
    '<link rel="icon" type="image/svg+xml" sizes="any" href="./assets/mark.svg" media="(prefers-color-scheme: light)">',
    '<link rel="apple-touch-icon" sizes="1024x1024" href="./assets/app.png">',
  ]);
  assert.deepEqual(links({ color_scheme_dark: { icons: [{ src: 'assets/mark-dark.svg' }] } }), [
    '<link rel="icon" type="image/svg+xml" sizes="any" href="./assets/mark-dark.svg" media="(prefers-color-scheme: dark)">',
    '<link rel="icon" href="./default.png" media="(prefers-color-scheme: light)">',
  ], 'a dark favicon alone keeps the default, last, for light');
  rect(dir, 'assets/wide.png', 320, 120);
  assert.deepEqual(links({ icons: [{ src: 'assets/wide.png' }] }), ['<link rel="icon" type="image/png" sizes="320x120" href="./assets/wide.png">'], 'with no square, another shape');
  assert.deepEqual(webThemeColors({ theme_color: '#ffffff', color_scheme_dark: { theme_color: '#000000' } }), [
    '<meta name="theme-color" content="#000000" media="(prefers-color-scheme: dark)">', '<meta name="theme-color" content="#ffffff">']);
}));

test('the published manifest fills sizes and type and keeps color_scheme_dark\'s W3C members', async () => inDir(async (dir) => {
  const { webAppManifest } = await import('./app.mjs');
  png(dir, 'assets/app.png', 1024);
  svg(dir, 'assets/mark.svg');
  const web = webAppManifest({ icons: [{ src: 'assets/app.png', purpose: 'any maskable' }, { src: 'assets/mark.svg' }], color_scheme_dark: { background_color: '#000000', icons: [{ src: 'assets/mark.svg' }] } }, 'Icons', [], dir);
  assert.deepEqual(web.icons, [{ src: 'assets/app.png', sizes: '1024x1024', type: 'image/png', purpose: 'any maskable' }, { src: 'assets/mark.svg', sizes: 'any', type: 'image/svg+xml' }]);
  assert.deepEqual(web.color_scheme_dark, { background_color: '#000000' });
}));

test.skipIf(process.platform !== 'darwin')('the iOS icon compiles its light, dark and tinted appearances in a development build', () => inDir((dir) => {
  const { useXcode } = require('../host/apple/devices.mjs');
  const { appIcon, iosAssets } = require('../host/apple/assets.mjs');
  useXcode();
  png(dir, 'assets/icon.png', 1024);
  png(dir, 'assets/icon-dark.png', 1024);
  png(dir, 'assets/glyph.png', 1024, { hole: true });
  const app = { dir, name: 'Icons', manifest: { icons: [{ src: 'assets/icon.png' }, { src: 'assets/glyph.png', purpose: 'monochrome' }], color_scheme_dark: { icons: [{ src: 'assets/icon-dark.png' }] }, host: {} } };
  const catalog = resolve(dir, 'Generated.xcassets');
  appIcon(app, dir, 'ios', { catalog });
  const set = JSON.parse(readFileSync(resolve(catalog, 'AppIcon.appiconset/Contents.json'), 'utf8'));
  assert.deepEqual(set.images.map((image) => image.appearances?.[0].value ?? 'any'), ['any', 'dark', 'tinted']);
  const bundle = resolve(dir, 'Icons.app'); mkdirSync(bundle);
  const keys = iosAssets(app, bundle, false, {});
  assert.equal(keys.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName, 'AppIcon');
  assert.deepEqual(keys.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconFiles, ['AppIcon60x60'], "actool's fallback PNGs for older iOS");
  const info = spawnSync('xcrun', ['assetutil', '--info', resolve(bundle, 'Assets.car')], { encoding: 'utf8' });
  assert.equal(info.status, 0, info.stderr);
  const appearances = new Set(JSON.parse(info.stdout).filter((asset) => asset.Name === 'AppIcon' && asset.AssetType === 'Icon Image').map((asset) => asset.Appearance ?? 'any'));
  assert.deepEqual([...appearances].sort(), ['ISAppearanceTintable', 'UIAppearanceDark', 'any']);
}), 60000);

/** What `assetutil` reads from the bundle's compiled catalog. */
const compiled = (bundle) => {
  const info = spawnSync('xcrun', ['assetutil', '--info', resolve(bundle, 'Assets.car')], { encoding: 'utf8' });
  assert.equal(info.status, 0, info.stderr);
  return JSON.parse(info.stdout);
};

test.skipIf(process.platform !== 'darwin')("tvOS compiles a 5:3 icon as two-layer stacks with the Top Shelf images its files are large enough for, the app's own brand assets instead, and nothing from Caltrain's icon", () => inDir((dir) => {
  const { existsSync } = require('node:fs');
  const { useXcode } = require('../host/apple/devices.mjs');
  const { iosAssets } = require('../host/apple/assets.mjs');
  useXcode();
  rect(dir, 'assets/caltrain.png', 320, 120);
  rect(dir, 'assets/tv.png', 1280, 768);
  rect(dir, 'assets/shelf.png', 3840, 1440);
  rect(dir, 'assets/shelf-wide.png', 2320, 720);
  const app = (names) => ({ dir, name: 'Icons', manifest: { icons: names.map((name) => ({ src: `assets/${name}.png` })), host: {} } });
  const bundle = (name) => { const path = resolve(dir, `${name}.app`); mkdirSync(path); return path; };
  const plain = bundle('Plain');
  assert.deepEqual(iosAssets(app(['caltrain']), plain, false, { tv: true }), {});
  assert.equal(existsSync(resolve(plain, 'Assets.car')), false);
  // A distributed build, which also looks for the icon in Assets.car.
  const tv = app(['caltrain', 'tv', 'shelf', 'shelf-wide']), made = bundle('TV');
  assert.deepEqual(iosAssets(tv, made, true, { tv: true, catalog: true }), {
    CFBundleIcons: { CFBundlePrimaryIcon: 'App Icon' },
    TVTopShelfImage: { TVTopShelfPrimaryImage: 'Top Shelf Image', TVTopShelfPrimaryImageWide: 'Top Shelf Image Wide' },
  });
  const assets = compiled(made);
  const stacks = assets.filter((asset) => asset.AssetType === 'ImageStack').map((stack) => [stack.Name, stack.Idiom,
    stack.Layers.map((layer) => `${layer.Name.split('/')[1]} ${layer.LayerSize} @${layer.Scale}x${layer.Opaque ? '' : ' transparent'}`).sort()]);
  assert.deepEqual(stacks.sort(), [
    ['App Icon', 'marketing', ['Back 1280,768 @1x', 'Front 1280,768 @1x transparent']],
    ['App Icon', 'universal', ['Back 400,240 @1x', 'Back 400,240 @2x', 'Front 400,240 @1x transparent', 'Front 400,240 @2x transparent']],
  ]);
  const shelf = (name) => assets.filter((asset) => asset.Name === name).map((asset) => `${asset.PixelWidth}x${asset.PixelHeight}`).sort();
  assert.deepEqual(shelf('Top Shelf Image'), ['1920x720', '3840x1440']);
  assert.deepEqual(shelf('Top Shelf Image Wide'), ['2320x720'], 'no 2x image from a file smaller than it');
  // The app's own brand assets, here one stack at 1x, are its icon in place of the manifest's.
  const brand = resolve(dir, 'modules/apple/Art.xcassets/Brand.brandassets'), info = { author: 'xcode', version: 1 };
  const contents = (path, value) => { mkdirSync(path, { recursive: true }); writeFileSync(resolve(path, 'Contents.json'), JSON.stringify({ ...value, info })); };
  contents(resolve(dir, 'modules/apple/Art.xcassets'), {});
  contents(brand, { assets: [{ filename: 'Home.imagestack', idiom: 'tv', role: 'primary-app-icon', size: '400x240' }] });
  contents(resolve(brand, 'Home.imagestack'), { layers: [{ filename: 'Front.imagestacklayer' }, { filename: 'Back.imagestacklayer' }] });
  for (const layer of ['Front', 'Back']) {
    contents(resolve(brand, `Home.imagestack/${layer}.imagestacklayer`), {});
    contents(resolve(brand, `Home.imagestack/${layer}.imagestacklayer/Content.imageset`), { images: [{ filename: 'layer.png', idiom: 'tv', scale: '1x' }] });
    writeFileSync(resolve(brand, `Home.imagestack/${layer}.imagestacklayer/Content.imageset/layer.png`), layer === 'Back' ? readFileSync(resolve(dir, 'assets/tv.png')) : encodePng(image(400, 240, [0, 0, 0, 0])));
  }
  const own = bundle('Own');
  assert.equal(iosAssets(tv, own, false, { tv: true }).CFBundleIcons.CFBundlePrimaryIcon, 'Home');
  assert.deepEqual([...new Set(compiled(own).filter((asset) => asset.AssetType === 'ImageStack').map((asset) => asset.Name))], ['Home']);
}), 60000);

test.skipIf(process.platform !== 'darwin')('the watch icon is the square icon, or the Icon Composer project once it lists watchOS among its circles', () => inDir((dir) => {
  const { cpSync, existsSync } = require('node:fs');
  const { useXcode } = require('../host/apple/devices.mjs');
  const { appleAssets } = require('../host/apple/assets.mjs');
  useXcode();
  png(dir, 'assets/icon.png', 1024);
  rect(dir, 'assets/caltrain.png', 320, 120);
  cpSync(resolve(import.meta.dir, 'fixtures/Primary.icon'), resolve(dir, 'Primary.icon'), { recursive: true });
  const watch = (name, manifest) => { const bundle = resolve(dir, `${name}.app`); mkdirSync(bundle); return [appleAssets({ dir, name: 'Icons', manifest }, bundle, { platform: 'watchos' }), bundle]; };
  const [none, plain] = watch('Plain', { icons: [{ src: 'assets/caltrain.png' }] });
  assert.deepEqual(none, {});
  assert.equal(existsSync(resolve(plain, 'Assets.car')), false);
  const manifest = { icons: [{ src: 'assets/caltrain.png' }, { src: 'assets/icon.png' }], host: { ios: { icon: 'Primary.icon' } } };
  const [squares, square] = watch('Squares', manifest);
  assert.equal(squares.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName, 'AppIcon', 'a project that draws no circle is not the watch icon');
  assert.deepEqual(compiled(square).filter((asset) => asset.AssetType === 'Icon Image').map((asset) => [asset.Name, asset.Idiom, asset.PixelWidth, asset.PixelHeight]), [['AppIcon', 'watch', 1024, 1024]]);
  const json = resolve(dir, 'Primary.icon/icon.json');
  writeFileSync(json, JSON.stringify({ ...JSON.parse(readFileSync(json, 'utf8')), 'supported-platforms': { circles: ['watchOS'], squares: 'shared' } }));
  const [circles, circle] = watch('Circles', manifest);
  assert.equal(circles.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName, 'Primary');
  const assets = compiled(circle);
  assert.ok(assets.some((asset) => asset.Name === 'Primary' && asset.AssetType === 'IconImageStack' && asset.Idiom === 'watch'));
  assert.ok(!assets.some((asset) => asset.Name === 'AppIcon'), "the manifest's icon is not compiled beside it");
}), 60000);
