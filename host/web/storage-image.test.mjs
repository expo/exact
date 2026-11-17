// `storage.fs.compressImage`'s search on the web (LLP 1069.002 A1.2/A1.7):
// the trials Bluesky's compress.ts makes, recorded in the same table
// `exact_data::image::search` is checked against; and the edit's geometry
// (LLP 1133.019 D14), case for case as `exact_data::image::edit`'s tests.
// `bun test ./host/web/storage-image.test.mjs`.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { compress, contain, editOptions, geometry, headerSize, limits, resized, search, searchLossless, transform } from './storage-image.js';

const recorded = JSON.parse(readFileSync(new URL('../../scripts/fixtures/picker/compress-trials.json', import.meta.url), 'utf8'));
const models = {
  'q*1000': (w, h, q) => q * 1000,
  'floor(w*h/4)+q*1000': (w, h, q) => Math.floor(w * h / 4) + q * 1000,
  'w*h': (w, h) => w * h,
  'floor(w*h*q/1000)': (w, h, q) => Math.floor(w * h * q / 1000),
  'floor(w*h*q/250)': (w, h, q) => Math.floor(w * h * q / 250),
};

test('the search makes Bluesky\'s trials', async () => {
  for (const c of recorded.cases) {
    const trials = [];
    const found = await search(c.width, c.height, { maxDimension: c.maxDimension, maxBytes: c.maxBytes }, async (w, h, quality) => {
      const q = Math.round(quality * 100);
      trials.push([w, h, q]);
      const size = models[c.model](w, h, q);
      return { size, value: size };
    });
    expect(trials, c.name).toEqual(c.trials);
    expect(found && { width: found.width, height: found.height, size: found.value }, c.name).toEqual(c.result);
  }
});

test('contain floors, never upscales, and keeps a pixel', () => {
  expect(contain(6000, 4000, 4000)).toEqual([4000, 2666]);
  expect(contain(3024, 4032, 2000)).toEqual([1500, 2000]);
  expect(contain(800, 601, 4000)).toEqual([800, 601]);
  expect(contain(9000, 2, 2048)).toEqual([2048, 1]);
});

test('a PNG shrinks by 0.8 until it fits, at most four times (A1.11)', async () => {
  const trials = [];
  const found = await searchLossless(1000, 800, { maxDimension: 1000, maxBytes: 300_000 }, async (w, h) => { trials.push([w, h]); return { size: w * h / 2, value: w }; });
  expect(trials).toEqual([[1000, 800], [800, 640]]);
  expect(found).toEqual({ value: 800, width: 800, height: 640 });
  trials.length = 0;
  expect(await searchLossless(1000, 800, { maxDimension: 1000, maxBytes: 1 }, async (w, h) => { trials.push([w, h]); return { size: 2, value: null }; })).toBe(null);
  expect(trials).toEqual([[1000, 800], [800, 640], [640, 512], [512, 409], [409, 327]]);
});

test('limits are integers in range, else a TypeError', () => {
  expect(limits({ maxDimension: 4000, maxBytes: 2e6 })).toEqual({ maxDimension: 4000, maxBytes: 2000000, type: 'image/jpeg' });
  expect(limits({ maxDimension: 8, maxBytes: 9, type: 'image/png' })).toEqual({ maxDimension: 8, maxBytes: 9, type: 'image/png' });
  for (const type of ['image/webp', 'png', null, 7]) expect(() => limits({ maxDimension: 8, maxBytes: 9, type })).toThrow(TypeError);
  for (const bad of [undefined, null, 7, {}, { maxDimension: 0, maxBytes: 1 }, { maxDimension: 8193, maxBytes: 1 },
    { maxDimension: 1.5, maxBytes: 1 }, { maxDimension: 1, maxBytes: 0 }, { maxDimension: 1, maxBytes: 67108865 }, { maxDimension: '10', maxBytes: 1 }])
    expect(() => limits(bad)).toThrow(TypeError);
});

// An explicit quality (LLP 1069.002 A1.12), case for case as
// `exact_data::image`'s tests.
const api = 'storage.fs.compressImage(): ';
test('quality is checked and makes maxBytes optional (A1.12)', () => {
  expect(limits({ maxDimension: 100, quality: 0.5 })).toEqual({ maxDimension: 100, maxBytes: undefined, quality: 0.5, type: 'image/jpeg' });
  expect(limits({ maxDimension: 100, maxBytes: 1000, quality: 0 }).quality).toBe(0);
  expect(limits({ maxDimension: 100, maxBytes: 1000, quality: 1 }).quality).toBe(1);
  for (const quality of [-0.01, 1.01, NaN, Infinity, null, '0.5'])
    expect(() => limits({ maxDimension: 100, maxBytes: 1000, quality })).toThrow(`${api}quality must be a number from 0 to 1`);
  expect(() => limits({ maxDimension: 100 })).toThrow(`${api}needs maxBytes, quality or both`);
  expect(() => limits({ maxDimension: 100, maxBytes: 0, quality: 2 })).toThrow(`${api}maxBytes must be an integer from 1 to 67108864`);
  expect(() => limits({ maxDimension: 100, quality: 0.5, type: 'image/webp' })).toThrow(`${api}type must be "image/jpeg" or "image/png"`);
  expect(() => limits(7)).toThrow(`${api}options must be an object`);
});

const ceilingTrials = async ([width, height], budget, size) => {
  const trials = [];
  const found = await search(width, height, budget, async (w, h, q) => { trials.push([w, h, q]); const n = size(w, h, q); return { size: n, value: n }; });
  return [trials, found && [found.width, found.height, found.value]];
};

test('a ceiling that fits is one encode (A1.12)', async () => {
  const [trials, found] = await ceilingTrials([4000, 3000], { maxDimension: 2000, maxBytes: 1_000_000, quality: 0.855 }, (w, h, q) => Math.floor(q * 1000));
  expect(trials).toEqual([[2000, 1500, 0.855]]);
  expect(found).toEqual([2000, 1500, 855]);
});

test('a ceiling that misses bisects below it, then shrinks (A1.12)', async () => {
  let [trials, found] = await ceilingTrials([4000, 3000], { maxDimension: 2000, maxBytes: 100, quality: 0.8 }, (w, h, q) => Math.floor(q * (w === 2000 ? 1000 : 100)));
  expect(trials).toEqual([[2000, 1500, 0.8], [2000, 1500, 0.4], [2000, 1500, 0.2], [1600, 1200, 0.8]]);
  expect(found).toEqual([1600, 1200, 80]);
  [trials, found] = await ceilingTrials([4000, 3000], { maxDimension: 2000, maxBytes: 700, quality: 0.8 }, (w, h, q) => Math.floor(q * 1000));
  expect(trials.map(t => Math.round(t[2] * 100))).toEqual([80, 40, 60, 70, 75, 73, 72, 71]);
  expect(found).toEqual([2000, 1500, 700]);
  [trials, found] = await ceilingTrials([4000, 3000], { maxDimension: 2000, maxBytes: 1, quality: 0.8 }, () => 2);
  expect(trials.map(t => t[0])).toEqual([2000, 2000, 2000, 1600, 1600, 1600, 1280, 1280, 1280, 1024, 1024, 1024]);
  expect(found).toBe(null);
});

test('the bisection stays under any ceiling (A1.12)', async () => {
  // 0.56 * 100 is 56.00000000000001: the bisection still stops below 56.
  const [trials] = await ceilingTrials([100, 100], { maxDimension: 100, maxBytes: 559, quality: 0.56 }, (w, h, q) => Math.floor(q * 1000));
  expect(trials.map(t => Math.round(t[2] * 100))).toEqual([56, 28, 42, 49, 53, 55]);
  for (const quality of [0, 0.1, 0.56, 0.29, 1]) {
    const [trials] = await ceilingTrials([4000, 3000], { maxDimension: 2000, maxBytes: 1, quality }, () => 2);
    expect(trials.every(t => t[2] <= quality)).toBe(true);
    expect(trials.filter(t => t[2] === quality).length).toBe(4);
  }
});

test('quality alone is one encode (A1.12)', async () => {
  let [trials, found] = await ceilingTrials([4000, 3000], { maxDimension: 1000, quality: 0.7 }, () => 5_000_000);
  expect(trials).toEqual([[1000, 750, 0.7]]);
  expect(found).toEqual([1000, 750, 5_000_000]);
  [trials, found] = await ceilingTrials([4000, 3000], { maxDimension: 1000, quality: 0.7 }, () => 64 * 1024 * 1024 + 1);
  expect([trials.length, found]).toEqual([1, null]);
});

test('a PNG ignores quality (A1.12)', async () => {
  const sizes = [];
  await searchLossless(1000, 800, { maxDimension: 1000, maxBytes: 300_000, quality: 0.3 }, async (w, h) => { sizes.push([w, h]); return { size: w * h / 2, value: w }; });
  expect(sizes).toEqual([[1000, 800], [800, 640]]);
  sizes.length = 0;
  expect(await searchLossless(1000, 800, { maxDimension: 500, quality: 0.3 }, async (w, h) => { sizes.push([w, h]); return { size: 64 * 1024 * 1024 + 1, value: w }; })).toBe(null);
  expect(sizes).toEqual([[500, 400]]);
});

test('the header gives the stored size before anything is decoded', async () => {
  const file = name => new Blob([readFileSync(new URL(`../../scripts/fixtures/picker/${name}`, import.meta.url))]);
  expect(await headerSize(file('oriented-gps.jpg'))).toEqual([64, 48]);
  expect(await headerSize(file('photo.png'))).toEqual([160, 106]);
  expect(await headerSize(file('photo.heic'))).toBe(null); // HEIF: not bounded before decoding on the web
  // The fixture's frame header rewritten to claim 9000 × 9000.
  const jpeg = new Uint8Array(readFileSync(new URL('../../scripts/fixtures/picker/oriented-gps.jpg', import.meta.url)));
  const sof = jpeg.findIndex((b, i) => b === 0xff && jpeg[i + 1] === 0xc0);
  new DataView(jpeg.buffer).setUint16(sof + 5, 9000); new DataView(jpeg.buffer).setUint16(sof + 7, 9000);
  expect(await headerSize(new Blob([jpeg]))).toEqual([9000, 9000]);
  const bytes = (...parts) => new Blob([new Uint8Array(parts.flat())]);
  const le16 = n => [n & 255, n >> 8], le32 = n => [n & 255, (n >> 8) & 255, (n >> 16) & 255, n >>> 24];
  const ascii = s => [...s].map(c => c.charCodeAt(0));
  // GIF: a 1×1 screen whose first image is 9000×9000 is 9000×9000; an
  // extension block before it is skipped.
  const gif = (screen, image) => bytes(ascii('GIF89a'), le16(screen[0]), le16(screen[1]), [0, 0, 0],
    [0x21, 0xf9, 4, 0, 0, 0, 0, 0], [0x2c, 0, 0, 0, 0], le16(image[0]), le16(image[1]), [0, 2, 0]);
  expect(await headerSize(gif([300, 200], [300, 200]))).toEqual([300, 200]);
  expect(await headerSize(gif([1, 1], [9000, 9000]))).toEqual([9000, 9000]);
  expect(await headerSize(gif([0, 0], [0, 0]))).toBe(null);
  // The rectangle of more pixels, not a side from each.
  expect(await headerSize(gif([50000, 2], [2, 40000]))).toEqual([50000, 2]);
  // BMP: signed 32-bit sides, top-down negative; and the 12-byte core header.
  expect(await headerSize(bytes(ascii('BM'), Array(12).fill(0), le32(40), le32(640), le32(-480 >>> 0), Array(8).fill(0)))).toEqual([640, 480]);
  expect(await headerSize(bytes(ascii('BM'), Array(12).fill(0), le32(12), le16(20000), le16(20000), Array(8).fill(0)))).toEqual([20000, 20000]);
  expect(await headerSize(bytes(ascii('BM'), Array(12).fill(0), le32(40), le32(-8 >>> 0), le32(8), Array(8).fill(0)))).toBe(null);
  // WebP: a VP8X canvas and its first frame, the larger of the two.
  const vp8l = (w, h) => { const bits = (w - 1) | ((h - 1) << 14); return [ascii('VP8L'), le32(5), [0x2f], le32(bits)]; };
  const vp8x = (w, h) => [ascii('VP8X'), le32(10), [0, 0, 0, 0], [(w - 1) & 255, ((w - 1) >> 8) & 255, (w - 1) >> 16], [(h - 1) & 255, ((h - 1) >> 8) & 255, (h - 1) >> 16]];
  expect(await headerSize(bytes(ascii('RIFF'), le32(0), ascii('WEBP'), vp8x(8000, 4096).flat(), vp8l(8000, 4096).flat()))).toEqual([8000, 4096]);
  expect(await headerSize(bytes(ascii('RIFF'), le32(0), ascii('WEBP'), vp8x(16, 16).flat(), vp8l(9000, 9000).flat()))).toEqual([9000, 9000]);
  expect(await headerSize(bytes(ascii('RIFF'), le32(0), ascii('WEBP'), vp8l(300, 200).flat(), [0, 0, 0]))).toEqual([300, 200]);
  // An animation: its first frame's own size, not its duration or flags.
  const le24 = n => [n & 255, (n >> 8) & 255, (n >> 16) & 255];
  const anmf = (w, h, duration) => [ascii('ANMF'), le32(16), le24(0), le24(0), le24(w - 1), le24(h - 1), le24(duration), [0]].flat();
  const anim = [ascii('ANIM'), le32(6), [0, 0, 0, 0, 0, 0]].flat();
  expect(await headerSize(bytes(ascii('RIFF'), le32(0), ascii('WEBP'), vp8x(100, 100).flat(), anim, anmf(32, 32, 0xfffff)))).toEqual([100, 100]);
  expect(await headerSize(bytes(ascii('RIFF'), le32(0), ascii('WEBP'), vp8x(16, 16).flat(), anim, anmf(9000, 9000, 100)))).toEqual([9000, 9000]);
  expect(await headerSize(bytes(ascii('not an image at all, not one bit')))).toBe(null);
});

test('a deadline already past decodes nothing', async () => {
  const fixture = new Blob([readFileSync(new URL('../../scripts/fixtures/picker/oriented-gps.jpg', import.meta.url))]);
  let decodes = 0;
  const before = globalThis.createImageBitmap;
  globalThis.createImageBitmap = async () => { decodes++; throw new Error('decoded'); };
  try {
    const error = await compress(fixture, { maxDimension: 4000, maxBytes: 2_000_000 }, 0).then(() => null, e => e);
    expect([error?.code, decodes]).toEqual(['timeout', 0]);
  } finally { globalThis.createImageBitmap = before; }
});

const apply = ([a, b, c, d, e, f], x, y) => [a * x + c * y + e, b * x + d * y + f];
const near = (p, q) => expect(Math.hypot(p[0] - q[0], p[1] - q[1])).toBeLessThan(1e-9);

test('an edit turns, flips and crops in D14\'s order, as the Rust codec does', () => {
  const quarter = geometry(48, 64, { rotate: 90 });
  expect([quarter.turned, quarter.crop]).toEqual([[64, 48], [0, 0, 64, 48]]);
  near(apply(quarter.toOutput(64, 48), 0, 0), [64, 0]);
  near(apply(quarter.toOutput(64, 48), 0, 64), [0, 0]);
  expect(quarter.toOutput(64, 48).slice(0, 4)).toEqual([0, 1, -1, 0]); // exact: nothing resampled
  expect(geometry(100, 50, { rotate: 30 }).turned).toEqual([112, 94]);
  near(apply(geometry(100, 50, { rotate: 30 }).toOutput(112, 94), 50, 25), [56, 47]);
  near(apply(geometry(48, 64, { rotate: 90, flip: 'horizontal' }).toOutput(64, 48), 0, 0), [0, 0]);
  near(apply(geometry(48, 64, { flip: 'vertical' }).toOutput(48, 64), 0, 0), [0, 64]);
  const crop = (x, y, width, height) => geometry(48, 64, { crop: { x, y, width, height } }, 'compressImage').crop;
  expect(crop(0, 0, 24, 64)).toEqual([0, 0, 24, 64]);
  expect(crop(-10, 60, 30, 30)).toEqual([0, 60, 20, 4]);
  expect(crop(0.4, 0.6, 10.2, 10.2)).toEqual([0, 1, 11, 10]);
  expect(crop(47.9, 0, 0.05, 1)).toEqual([47, 0, 1, 1]);
  const missed = (() => { try { crop(48, 0, 10, 10); } catch (e) { return e; } })();
  expect([missed.code, missed.message]).toEqual(['outside', 'filesystem: compressImage: outside: the crop misses the 48×64 image']);
  const turnedCrop = geometry(48, 64, { rotate: 90, crop: { x: 10, y: 0, width: 20, height: 48 } });
  expect(turnedCrop.size).toEqual([20, 48]);
  near(apply(turnedCrop.toOutput(10, 24), 0, 64), [-5, 0]);
});

test('the size is createImageBitmap\'s', () => {
  expect(resized(64, 48)).toEqual([64, 48]);
  expect(resized(64, 48, 32)).toEqual([32, 24]);
  expect(resized(64, 48, undefined, 10)).toEqual([14, 10]);
  expect(resized(64, 48, 5, 500)).toEqual([5, 500]);
});

test('an edit\'s options are checked as the prelude checks them', () => {
  expect(editOptions('t', { rotate: -90, flip: 'vertical', crop: { x: 1, y: 2, width: 3, height: 4 }, resizeWidth: 8, metadata: 'none', quality: 0.5 }, true))
    .toEqual({ rotate: -90, flip: 'vertical', crop: { x: 1, y: 2, width: 3, height: 4 }, resizeWidth: 8, metadata: 'none', quality: 0.5 });
  expect(editOptions('t', { maxDimension: 9, resizeWidth: 8 }, false)).toEqual({});
  for (const bad of [{ rotate: '90' }, { flip: 'both' }, { crop: { x: 0, y: 0, width: 0, height: 1 } }, { crop: [0, 0, 1, 1] },
    { resizeWidth: 0 }, { resizeHeight: 1.5 }, { quality: 2 }, { metadata: 'some' }, { resizeQuality: 'best' }, { type: 7 }, 7])
    expect(() => editOptions('t', bad, true)).toThrow(TypeError);
});

test('a transform past its deadline decodes nothing', async () => {
  const fixture = new Blob([readFileSync(new URL('../../scripts/fixtures/picker/oriented-gps.jpg', import.meta.url))]);
  const error = await transform(fixture, { rotate: 90 }, 0).then(() => null, e => e);
  expect([error?.code, error?.message.startsWith('filesystem: transformImage: timeout: ')]).toEqual(['timeout', true]);
});
