import { test, expect } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { pngPixels, pngBytes, resizePixels, bitmapPixels } from './image.mjs';
import { appleCommand } from './toolchain.mjs';
import { encodePng } from '../../scripts/png.mjs';

// PngSuite: https://github.com/nothings/stb/tree/master/tests/pngsuite
// Golden RGBA bytes use stbi_load_16, scaled to eight bits with nearest rounding.
const references = {
  "0g01": "661985e83f94a569510ded43e65edb11f4ced1121c611209f7abe9a9c40c71a8",
  "0g02": "166bd68377b119b5e93e73ef554e35de7471bdd2fc3bc2070f0f7bd5be82ae97",
  "0g04": "b05a4bc8e7079c8aa0e491086ccb156dd4bdbc67e57bb8c9d803d7e75778da9e",
  "0g08": "982faa277e83f73ca15b491e67eb41fa25526418ed23e057a9986c4f620eb158",
  "0g16": "f17fa71e5e62a73b92827381f39e2702752ced03372d568b853ae1914b9dd047",
  "2c08": "23a53c674ec50d5a5eb9c3f679b6b19ba5304ae99dff76801bec4939e0f0c99e",
  "2c16": "a9dff6085fe81eea37100681e299a0504206137521dc59d592d87fa73b18c917",
  "3p01": "614996feb597f62b913614a57be5ce64eea97efc57cd55bbba535d2f61716833",
  "3p02": "a383497791948d8b7ae8f9158fb7b4e9fead4693814ee758a97bc426dc9a27cf",
  "3p04": "a7abc212cf1a44c85df377773f3722dc118f0c4159df89fdac2dfe6911abe378",
  "3p08": "b1c3302eceae6738c36edafa98c8054824d9440f3ba53a3f17cc81d29acc32cc",
  "4a08": "76b94a71d3c183a362c2cf6a46ebb50adc9d3a25a89bc0afc46fda6dbb002509",
  "4a16": "e071c0ea344f34b0070c2402329ec5f165c33566b082440aa8268ac69fa86e6d",
  "6a08": "2eb6a2cb3166e9c188add371157e9f81caa18fdf34d218844ed930b53b7431d2",
  "6a16": "3daad02ebc3eb86835c0acee955564e7fd62d2a9f37dd6230632f7655f8f8c1b"
};
test('PNG palettes, grayscale, RGB, alpha, sample depths and Adam7 match independent reference pixels', () => {
  for (const [format, hash] of Object.entries(references)) for (const layout of ['basn', 'basi']) {
    const bytes = readFileSync(new URL(`../../scripts/fixtures/pngsuite/${layout}${format}.png`, import.meta.url));
    const image = pngPixels(bytes);
    expect([image.width, image.height]).toEqual([32, 32]);
    expect(createHash('sha256').update(image.data).digest('hex'), layout + format).toBe(hash);
  }
  const png = encodePng({ width: 1, height: 1, data: Buffer.from([100, 200, 50, 128]) });
  expect(bitmapPixels(pngPixels(png)).pixels.subarray(0, 4)).toEqual(Buffer.from([25, 100, 50, 128]));
  expect(() => pngPixels(png.subarray(0, -1))).toThrow('truncated');
  const broken = Buffer.from(png); broken[44] ^= 1;
  expect(() => pngPixels(broken)).toThrow('checksum');
});

test('resizing preserves constant colors and alpha without leaking hidden transparent colors', () => {
  const uniform = { width: 1, height: 1, data: Buffer.from([255, 0, 0, 128]) };
  const larger = resizePixels(uniform, 4, 3);
  for (let i = 0; i < larger.data.length; i += 4) expect(larger.data.subarray(i, i + 4)).toEqual(uniform.data);
  const edge = resizePixels({ width: 2, height: 1, data: Buffer.from([255, 0, 0, 255, 0, 0, 255, 0]) }, 8, 1);
  for (let i = 0; i < edge.data.length; i += 4) if (edge.data[i + 3]) expect([...edge.data.subarray(i, i + 3)]).toEqual([255, 0, 0]);
  expect(uniform.data).toEqual(Buffer.from([255, 0, 0, 128]));
  expect(() => resizePixels(uniform, 0, 1)).toThrow('dimensions');
});

test('UI Gallery’s Display P3 icon retains its color space and profile through resizing', () => {
  const original = pngPixels(readFileSync(new URL('../../apps/ui-gallery/assets/icon.png', import.meta.url)));
  expect(original.space).toBe(3); expect(original.profile.length).toBeGreaterThan(128);
  const resized = pngPixels(pngBytes(resizePixels(original, 120, 120)));
  expect([resized.width, resized.height, resized.space]).toEqual([120, 120, 3]);
  expect(resized.profile).toEqual(original.profile);
});

test('the portable image command implements square and proportional bundle-image resizing', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-image-'));
  try {
    const source = resolve(dir, 'source.png'), target = resolve(dir, 'resized.png');
    writeFileSync(source, encodePng({ width: 2, height: 1, data: Buffer.from([255, 0, 0, 255, 255, 0, 0, 255]) }));
    for (const [args, dimensions] of [[['-z', '3', '3', source, '--out', target], [3, 3]], [['--resampleWidth', '4', source, '--out', target], [4, 2]]]) {
      const result = spawnSync(...appleCommand('sips', args, { encoding: 'utf8' }, 'linux'));
      expect(result.status, result.stderr).toBe(0);
      const image = pngPixels(readFileSync(target)); expect([image.width, image.height]).toEqual(dimensions);
      for (let i = 0; i < image.data.length; i += 4) expect([...image.data.subarray(i, i + 4)]).toEqual([255, 0, 0, 255]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
