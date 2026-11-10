// PNG asset pixels and resizing for builds without ImageIO or sips.
import { deflateSync, inflateSync } from 'node:zlib';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { encodePng } from '../../scripts/png.mjs';

const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const crcTable = Uint32Array.from({ length: 256 }, (_, n) => {
  for (let i = 0; i < 8; i++) n = n & 1 ? 0xedb88320 ^ (n >>> 1) : n >>> 1;
  return n >>> 0;
});
const crc32 = bytes => { let n = 0xffffffff; for (const byte of bytes) n = crcTable[(n ^ byte) & 255] ^ (n >>> 8); return (n ^ 0xffffffff) >>> 0; };
const paeth = (a, b, c) => { const p = a + b - c, da = Math.abs(p - a), db = Math.abs(p - b), dc = Math.abs(p - c); return da <= db && da <= dc ? a : db <= dc ? b : c; };

// ICC matrix/TRC profiles can retain their samples under CoreUI's standard
// spaces. Check colorimetry, never a profile's editable description.
function profileSpace(profile) {
  const fail = () => { throw new Error('host/apple: unsupported or malformed PNG color profile'); };
  if (profile.length < 132 || profile.readUInt32BE() !== profile.length || profile.toString('ascii', 36, 40) !== 'acsp' || profile.toString('ascii', 16, 24) !== 'RGB XYZ ') fail();
  const count = profile.readUInt32BE(128), tags = new Map();
  if (132 + count * 12 > profile.length) fail();
  for (let i = 0; i < count; i++) {
    const at = 132 + i * 12, name = profile.toString('ascii', at, at + 4), start = profile.readUInt32BE(at + 4), length = profile.readUInt32BE(at + 8);
    if (start < 132 + count * 12 || start + length > profile.length || tags.has(name) || /^(A2B|B2A)/.test(name)) fail();
    tags.set(name, profile.subarray(start, start + length));
  }
  const near = (actual, expected) => actual.length === expected.length && actual.every((n, i) => Math.abs(n - expected[i]) < 0.0002);
  const numbers = (name, type, offset, count) => {
    const tag = tags.get(name); if (!tag || tag.length < offset + count * 4 || tag.toString('ascii', 0, 4) !== type) fail();
    return Array.from({ length: count }, (_, i) => tag.readInt32BE(offset + i * 4) / 65536);
  };
  if (!near(numbers('wtpt', 'XYZ ', 8, 3), [0.9642, 1, 0.8249])) fail();
  for (const name of ['rTRC', 'gTRC', 'bTRC']) {
    const tag = tags.get(name); if (!tag || tag.length !== 32 || tag.readUInt16BE(8) !== 3 || !near(numbers(name, 'para', 12, 5), [2.4, 1 / 1.055, 0.055 / 1.055, 1 / 12.92, 0.04045])) fail();
  }
  const matrix = ['rXYZ', 'gXYZ', 'bXYZ'].flatMap(name => numbers(name, 'XYZ ', 8, 3));
  // D50-adapted sRGB and Display P3; ICC uses signed 16.16 components.
  if (near(matrix, [0.43607, 0.22250, 0.01393, 0.38506, 0.71688, 0.09710, 0.14308, 0.06062, 0.71417])) return 1;
  if (near(matrix, [0.51513, 0.24119, -0.00105, 0.29199, 0.69223, 0.04188, 0.15712, 0.06657, 0.78419])) return 3;
  fail();
}

function exifOrientation(bytes) {
  const fail = () => { throw new Error('host/apple: malformed PNG EXIF metadata'); };
  if (bytes.length < 8 || !['II', 'MM'].includes(bytes.toString('ascii', 0, 2))) fail();
  const endian = bytes[0] === 73 ? 'LE' : 'BE', u16 = at => bytes[`readUInt16${endian}`](at), u32 = at => bytes[`readUInt32${endian}`](at);
  if (u16(2) !== 42) fail();
  const start = u32(4); if (start < 8 || start + 2 > bytes.length) fail();
  const count = u16(start); if (start + 2 + count * 12 + 4 > bytes.length) fail();
  for (let i = 0; i < count; i++) {
    const at = start + 2 + i * 12;
    if (u16(at) === 0x112) { if (u16(at + 2) !== 3 || u32(at + 4) !== 1) fail(); return u16(at + 8); }
  }
  return 1;
}

/** PNG 3 scanlines, including palettes, transparency, 16-bit samples and Adam7. */
export function pngPixels(bytes) {
  const fail = reason => { throw new Error(`host/apple: invalid PNG: ${reason}`); };
  if (!bytes.subarray(0, 8).equals(signature)) fail('signature');
  let width, height, depth, type, interlace, palette, transparency, gamma, profile, cicp, space, srgb = false, ended = false;
  const data = [];
  for (let offset = 8; offset < bytes.length;) {
    if (offset + 12 > bytes.length) fail('truncated chunk');
    const length = bytes.readUInt32BE(offset), end = offset + 12 + length;
    if (end > bytes.length) fail('truncated chunk data');
    const name = bytes.toString('ascii', offset + 4, offset + 8), value = bytes.subarray(offset + 8, end - 4);
    if (crc32(bytes.subarray(offset + 4, end - 4)) !== bytes.readUInt32BE(end - 4)) fail(`${name} checksum`);
    if (width === undefined && name !== 'IHDR') fail('IHDR must come first');
    if (name === 'IHDR') {
      if (width !== undefined || length !== 13) fail('IHDR');
      width = value.readUInt32BE(0); height = value.readUInt32BE(4); depth = value[8]; type = value[9]; interlace = value[12];
      const depths = { 0: [1, 2, 4, 8, 16], 2: [8, 16], 3: [1, 2, 4, 8], 4: [8, 16], 6: [8, 16] };
      if (!width || !height || width * height > 64 * 1024 * 1024 || !depths[type]?.includes(depth) || value[10] || value[11] || interlace > 1) fail('dimensions or format');
    } else if (name === 'PLTE') {
      if (!length || length % 3 || length > 768) fail('palette'); palette = value;
    } else if (name === 'tRNS') transparency = value;
    else if (name === 'gAMA') { if (length !== 4 || !value.readUInt32BE()) fail('gamma'); gamma = value.readUInt32BE(); }
    else if (name === 'sRGB') { if (length !== 1 || value[0] > 3) fail('sRGB'); srgb = true; }
    else if (name === 'IDAT') data.push(value);
    else if (name === 'IEND') { if (length || end !== bytes.length) fail('IEND'); ended = true; break; }
    else if (name === 'iCCP') {
      const separator = value.indexOf(0); if (profile || separator < 1 || separator > 79 || value[separator + 1] !== 0) fail('color profile');
      profile = inflateSync(value.subarray(separator + 2), { maxOutputLength: 512 * 1024 });
      const target = profileSpace(profile); if (space != null && space !== target) fail('conflicting color spaces'); space = target;
    } else if (name === 'cICP') {
      const target = { 1: 1, 12: 3 }[value[0]];
      if (length !== 4 || !target || value[1] !== 13 || value[2] !== 0 || value[3] !== 1 || space != null && space !== target) throw new Error('host/apple: unsupported PNG cICP color space');
      space = target; cicp = value;
    } else if (name === 'eXIf' && exifOrientation(value) !== 1) throw new Error('host/apple: portable PNG EXIF rotation is not implemented yet');
    else if (name[0] === name[0].toUpperCase()) fail(`unknown critical chunk ${name}`);
    offset = end;
  }
  if (!ended || !data.length || type === 3 && !palette) fail('missing image data or palette');
  if (!profile && !srgb && gamma != null && ![45455, 100000].includes(gamma)) throw new Error(`host/apple: portable PNG gamma ${gamma} is not implemented yet`);
  const channels = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[type], bits = channels * depth, bpp = Math.max(1, Math.ceil(bits / 8));
  const passes = interlace ? [[0, 0, 8, 8], [4, 0, 8, 8], [0, 4, 4, 8], [2, 0, 4, 4], [0, 2, 2, 4], [1, 0, 2, 2], [0, 1, 1, 2]] : [[0, 0, 1, 1]];
  const shapes = passes.map(([x, y, dx, dy]) => ({ x, y, dx, dy, w: Math.max(0, Math.ceil((width - x) / dx)), h: Math.max(0, Math.ceil((height - y) / dy)) }));
  const size = shapes.reduce((sum, p) => sum + (p.w && p.h ? (1 + Math.ceil(p.w * bits / 8)) * p.h : 0), 0);
  const raw = inflateSync(Buffer.concat(data), { maxOutputLength: size });
  if (raw.length !== size) fail('scanline length');
  if (transparency && (type === 0 ? transparency.length !== 2 : type === 2 ? transparency.length !== 6 : type === 3 ? transparency.length > palette.length / 3 : true)) fail('transparency');
  const output = Buffer.alloc(width * height * 4), maximum = 2 ** depth - 1;
  let offset = 0;
  for (const p of shapes) {
    if (!p.w || !p.h) continue;
    const stride = Math.ceil(p.w * bits / 8); let previous = Buffer.alloc(stride);
    for (let y = 0; y < p.h; y++) {
      const filter = raw[offset++], row = Buffer.from(raw.subarray(offset, offset + stride)); offset += stride;
      if (filter > 4) fail('scanline filter');
      for (let i = 0; i < stride; i++) {
        const a = i >= bpp ? row[i - bpp] : 0, b = previous[i], c = i >= bpp ? previous[i - bpp] : 0;
        row[i] = (row[i] + [0, a, b, Math.floor((a + b) / 2), paeth(a, b, c)][filter]) & 255;
      }
      for (let x = 0; x < p.w; x++) {
        const sample = i => depth === 16 ? row.readUInt16BE((x * channels + i) * 2) : depth === 8 ? row[x * channels + i] : row[Math.floor(x * depth / 8)] >> (8 - depth - x * depth % 8) & maximum;
        const values = Array.from({ length: channels }, (_, i) => sample(i));
        const byte = n => Math.round(n * 255 / maximum);
        let rgba;
        if (type === 3) {
          const index = values[0]; if (index * 3 >= palette.length) fail('palette index');
          rgba = [...palette.subarray(index * 3, index * 3 + 3), transparency?.[index] ?? 255];
        } else if (type === 0 || type === 4) {
          rgba = [byte(values[0]), byte(values[0]), byte(values[0]), type === 4 ? byte(values[1]) : transparency && values[0] === transparency.readUInt16BE(0) ? 0 : 255];
        } else {
          rgba = [...values.slice(0, 3).map(byte), type === 6 ? byte(values[3]) : transparency && values.slice(0, 3).every((n, i) => n === transparency.readUInt16BE(i * 2)) ? 0 : 255];
        }
        output.set(rgba, ((p.y + y * p.dy) * width + p.x + x * p.dx) * 4);
      }
      previous = row;
    }
  }
  return { width, height, data: output, profile, cicp, space: space ?? (!srgb && gamma === 100000 ? 5 : 1) };
}

export function bitmapPixels(image) {
  const stride = Math.ceil(image.width * 4 / 16) * 16, pixels = Buffer.alloc(stride * image.height);
  for (let y = 0; y < image.height; y++) for (let x = 0; x < image.width; x++) {
    const source = (y * image.width + x) * 4, dest = y * stride + x * 4, alpha = image.data[source + 3];
    for (let c = 0; c < 3; c++) pixels[dest + c] = Math.round(image.data[source + 2 - c] * alpha / 255);
    pixels[dest + 3] = alpha;
  }
  return { width: image.width, height: image.height, stride, pixels, space: image.space ?? 1 };
}

const sinc = value => value === 0 ? 1 : Math.sin(Math.PI * value) / (Math.PI * value);
function contributions(from, to) {
  const ratio = from / to, support = Math.max(1, ratio);
  return Array.from({ length: to }, (_, i) => {
    const center = (i + 0.5) * ratio - 0.5, weights = new Map();
    for (let j = Math.ceil(center - support * 3); j <= Math.floor(center + support * 3); j++) {
      const distance = (center - j) / support, weight = sinc(distance) * sinc(distance / 3), index = Math.max(0, Math.min(from - 1, j));
      weights.set(index, (weights.get(index) ?? 0) + weight);
    }
    const sum = [...weights.values()].reduce((a, b) => a + b, 0);
    return [...weights].map(([index, weight]) => [index, weight / sum]);
  });
}

/** Lanczos3 in premultiplied space keeps transparent borders free of color fringes. */
export function resizePixels(image, width, height) {
  if (![width, height].every(n => Number.isInteger(n) && n > 0) || width * height > 64 * 1024 * 1024) throw new Error('host/apple: invalid resized image dimensions');
  if (width === image.width && height === image.height) return image;
  const xs = contributions(image.width, width), ys = contributions(image.height, height);
  const horizontal = new Float32Array(width * image.height * 4), data = Buffer.alloc(width * height * 4);
  for (let y = 0; y < image.height; y++) for (let x = 0; x < width; x++) for (const [sx, weight] of xs[x]) {
    const from = (y * image.width + sx) * 4, to = (y * width + x) * 4, alpha = image.data[from + 3];
    for (let c = 0; c < 4; c++) horizontal[to + c] += image.data[from + c] * (c === 3 ? 1 : alpha / 255) * weight;
  }
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const channels = [0, 0, 0, 0], to = (y * width + x) * 4;
    for (const [sy, weight] of ys[y]) for (let c = 0; c < 4; c++) channels[c] += horizontal[(sy * width + x) * 4 + c] * weight;
    data[to + 3] = Math.max(0, Math.min(255, Math.round(channels[3])));
    for (let c = 0; c < 3; c++) data[to + c] = data[to + 3] ? Math.max(0, Math.min(255, Math.round(channels[c] * 255 / channels[3]))) : 0;
  }
  return { width, height, data, profile: image.profile, cicp: image.cicp, space: image.space ?? 1 };
}

export function resizeImageCommand(args) {
  let width, height, source, output;
  if (args.length === 6 && args[0] === '-z' && args[4] === '--out') {
    height = Number(args[1]); width = Number(args[2]); source = args[3]; output = args[5];
  } else if (args.length === 5 && args[0] === '--resampleWidth' && args[3] === '--out') {
    width = Number(args[1]); source = args[2]; output = args[4];
  } else throw new Error('host/apple: unsupported portable image resize arguments');
  const image = pngPixels(readFileSync(source));
  height ??= Math.max(1, Math.round(image.height * width / image.width));
  const result = resizePixels(image, width, height);
  writeFileSync(output, pngBytes(result));
}

export function pngBytes(image) {
  const png = encodePng(image);
  if (image.profile) {
    const data = Buffer.concat([Buffer.from('Exact source profile\0\0'), deflateSync(image.profile)]), chunk = Buffer.alloc(data.length + 12);
    chunk.writeUInt32BE(data.length); chunk.write('iCCP', 4); data.copy(chunk, 8); chunk.writeUInt32BE(crc32(chunk.subarray(4, -4)), chunk.length - 4);
    return Buffer.concat([png.subarray(0, 33), chunk, png.subarray(33)]);
  }
  if (image.cicp) {
    const chunk = Buffer.alloc(16); chunk.writeUInt32BE(4); chunk.write('cICP', 4); image.cicp.copy(chunk, 8); chunk.writeUInt32BE(crc32(chunk.subarray(4, 12)), 12);
    return Buffer.concat([png.subarray(0, 33), chunk, png.subarray(33)]);
  }
  if (image.space === 5) {
    const chunk = Buffer.alloc(16); chunk.writeUInt32BE(4); chunk.write('gAMA', 4); chunk.writeUInt32BE(100000, 8); chunk.writeUInt32BE(crc32(chunk.subarray(4, 12)), 12);
    return Buffer.concat([png.subarray(0, 33), chunk, png.subarray(33)]);
  }
  return png;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { resizeImageCommand(process.argv.slice(2)); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
