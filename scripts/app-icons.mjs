// App icons (LLP 1163): the manifest's `icons` and `color_scheme_dark.icons`,
// what each file is, which file a host's slot takes, and the web's head.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { decodePng, encodePng, shrink } from './png.mjs';

const TYPES = { png: 'image/png', svg: 'image/svg+xml', ico: 'image/x-icon' };
const ICO_TYPES = ['image/x-icon', 'image/vnd.microsoft.icon'];
const DARK = '(prefers-color-scheme: dark)', LIGHT = '(prefers-color-scheme: light)';

const purposes = (entry) => (entry.purpose ?? 'any').split(' ').filter(Boolean);

/** A list's field name and entries: `icons`, or for the dark scheme `color_scheme_dark.icons`,
 * which replaces it there as `color_scheme_dark` replaces each member it names (D2). */
function list(manifest, scheme) {
  return scheme === 'dark'
    ? { field: 'color_scheme_dark.icons', entries: manifest.color_scheme_dark?.icons ?? [] }
    : { field: 'icons', entries: manifest.icons ?? [] };
}

const facts = new Map();
/** What the file at `src` is, read from its bytes: its format, pixel size and the `sizes` a link
 * would give it; null when it is none of PNG, SVG and ICO. An SVG has no pixel size (`sizes: any`)
 * but has its viewBox's shape. */
export function iconFile(dir, src) {
  const path = resolve(dir, src);
  if (!existsSync(path)) return null;
  const { mtimeMs, size } = statSync(path), key = `${path}\0${mtimeMs}\0${size}`;
  if (!facts.has(key)) facts.set(key, readFacts(path));
  return facts.get(key);
}
function readFacts(path) {
  const bytes = readFileSync(path);
  if (bytes.length >= 24 && bytes.readUInt32BE(0) === 0x89504e47 && bytes.toString('latin1', 12, 16) === 'IHDR') {
    const width = bytes.readUInt32BE(16), height = bytes.readUInt32BE(20);
    return { path, format: 'png', width, height, sizes: `${width}x${height}` };
  }
  if (bytes.length >= 6 && bytes.readUInt16LE(0) === 0 && bytes.readUInt16LE(2) === 1) {
    const count = bytes.readUInt16LE(4), sides = [];
    for (let i = 0; i < count && 6 + 16 * (i + 1) <= bytes.length; i++) sides.push([bytes[6 + 16 * i] || 256, bytes[7 + 16 * i] || 256]);
    if (!sides.length) return null;
    const width = Math.max(...sides.map(([w]) => w)), height = Math.max(...sides.map(([, h]) => h));
    return { path, format: 'ico', width, height, sizes: [...new Set(sides.map(([w, h]) => `${w}x${h}`))].join(' ') };
  }
  // The root element may follow a long prolog, comments or an editor's metadata.
  const svg = /<svg\b[^>]*>/.exec(bytes.toString('utf8'));
  if (!svg) return null;
  const view = /\sviewBox\s*=\s*["']\s*[-\d.e]+[\s,]+[-\d.e]+[\s,]+([\d.e]+)[\s,]+([\d.e]+)/.exec(svg[0]);
  // The attribute itself, not one ending in its name (`stroke-width`).
  const attr = (name) => Number(new RegExp(`\\s${name}\\s*=\\s*["']\\s*([\\d.]+)(?:px)?\\s*["']`).exec(svg[0])?.[1]);
  const [w, h] = view ? [Number(view[1]), Number(view[2])] : [attr('width'), attr('height')];
  return { path, format: 'svg', width: null, height: null, ratio: w > 0 && h > 0 ? w / h : null, sizes: 'any' };
}

/** The manifest's icon problems, every one in one pass (LLP 1163 D8): each entry is a PNG, SVG or
 * ICO under `assets/` (the directory the web root serves) whose written `sizes` and `type` are its
 * own, and a dark entry is `any` only. */
export function appIconProblems(manifest, dir) {
  return ['light', 'dark'].flatMap((scheme) => {
    const { field, entries } = list(manifest, scheme);
    return entries.flatMap((entry, i) => {
      const at = `${field}[${i}]`, problems = [];
      if (scheme === 'dark' && purposes(entry).some((p) => p !== 'any'))
        problems.push(`${at}.purpose: "${entry.purpose}"; a dark icon is "any" only, since no host varies a mask or a glyph with the scheme`);
      if (!entry.src.startsWith('assets/') || entry.src.split('/').includes('..'))
        return [...problems, `${at}.src: ${entry.src} is outside assets/, the directory the web root serves; move it under assets/`];
      if (!existsSync(resolve(dir, entry.src))) return [...problems, `${at}.src: ${entry.src} does not exist`];
      const file = iconFile(dir, entry.src);
      if (!file) return [...problems, `${at}.src: ${entry.src} is not a PNG, SVG or ICO`];
      if (entry.sizes !== undefined && entry.sizes.split(' ').filter(Boolean).sort().join(' ') !== file.sizes.split(' ').sort().join(' '))
        problems.push(`${at}.sizes: "${entry.sizes}", but ${entry.src} is ${file.sizes}; write "${file.sizes}" or leave sizes out`);
      const types = file.format === 'ico' ? ICO_TYPES : [TYPES[file.format]];
      if (entry.type !== undefined && !types.includes(entry.type))
        problems.push(`${at}.type: "${entry.type}", but ${entry.src} is ${types[0]}; write "${types[0]}" or leave type out`);
      return problems;
    });
  });
}

/** The file a host slot takes (LLP 1163 D5), or null when nothing fits. `slot` names the
 * `scheme` ('light' or 'dark'), `purposes` in order of preference, `shape` ('square', 'any', or a
 * width-to-height ratio such as 16 / 9), `px` (its width in pixels), `formats`, and `name` for
 * messages; `native` refuses a file smaller than the slot (the web links it, and the browser
 * scales it), `opaque` ('refuse' or 'skip') a file with transparent pixels, and `glyph` one
 * with none. */
export function pickIcon(manifest, dir, slot) {
  const { field, entries } = list(manifest, slot.scheme ?? 'light'), shape = slot.shape ?? 'square';
  // A raster fills the slot when it is at least the slot's width, and its height at the slot's shape.
  const tall = shape === 'any' ? 0 : Math.round(slot.px / (shape === 'square' ? 1 : shape));
  const fills = ({ file }) => file.width >= slot.px && file.height >= tall;
  const candidates = entries.map((entry, index) => ({ entry, index, file: iconFile(dir, entry.src) })).filter(({ file }) => file && slot.formats.includes(file.format) && fits(file, shape));
  let small = null, clear = null;
  for (const purpose of slot.purposes) {
    let usable = candidates.filter(({ entry }) => purposes(entry).includes(purpose));
    // Where the slot needs an opaque file, one with transparent pixels is passed over, and refused only when
    // nothing opaque fills the slot.
    if (slot.opaque) usable = usable.filter((candidate) => {
      if (candidate.file.format !== 'png' || opaque(candidate.file.path)) return true;
      if (slot.opaque === 'refuse' && fills(candidate)) clear ??= candidate;
      return false;
    });
    const rasters = usable.filter(({ file }) => file.format !== 'svg');
    const large = rasters.filter(fills).sort((a, b) => a.file.width - b.file.width || a.index - b.index);
    const pick = usable.find(({ file }) => file.format === 'svg') ?? large[0]
      ?? (slot.native ? null : rasters.sort((a, b) => b.file.width - a.file.width || a.index - b.index)[0]);
    if (!pick) {
      // The refusal names the largest candidate, the one nearest the slot.
      for (const raster of rasters) if (!small || raster.file.width > small.file.width) small = raster;
      continue;
    }
    if (slot.glyph && opaque(pick.file.path))
      throw new Error(`${field}[${pick.index}]: ${pick.entry.src} is "monochrome" but has no transparent pixels, so ${slot.name} would be a solid square; draw the glyph on a transparent background`);
    return { ...pick.file, src: pick.entry.src, index: pick.index, field };
  }
  if (clear) throw new Error(`${field}[${clear.index}]: ${clear.entry.src} has transparent pixels, and ${slot.name} must be opaque; use an opaque file of at least ${slot.px} px`);
  if (small) throw new Error(`${field}[${small.index}]: ${small.entry.src} is ${small.file.width}×${small.file.height} px, and ${slot.name} needs at least ${slot.px}${tall && tall !== slot.px ? `×${tall}` : ''} px; use a larger file`);
  return null;
}
function fits(file, shape) {
  if (shape === 'any') return true;
  if (file.format === 'svg') return file.ratio === null ? shape === 'square' : Math.abs(file.ratio - (shape === 'square' ? 1 : shape)) < 0.01;
  return shape === 'square' ? file.width === file.height : Math.abs(file.width / file.height - shape) < 0.01;
}

/** The PNG at `path` as { width, height, data }, its pixels in 8-bit RGBA (`decodePng`). */
export const readPng = (path) => decodePng(readFileSync(path));
/** The PNG's alpha per pixel, row by row. */
export function pngAlpha(path) {
  const { width, height, data } = readPng(path), alpha = new Uint8Array(width * height);
  for (let i = 0; i < alpha.length; i++) alpha[i] = data[4 * i + 3];
  return { width, height, alpha };
}

/** `image` shrunk to `width` × `height` by area (`shrink`) in premultiplied alpha, so a transparent pixel's colour
 * does not bleed into its neighbours. */
export function resize(image, width, height) {
  const premultiplied = new Uint8Array(image.data.length);
  for (let i = 0; i < premultiplied.length; i += 4) {
    const a = image.data[i + 3];
    for (let c = 0; c < 3; c++) premultiplied[i + c] = Math.round((image.data[i + c] * a) / 255);
    premultiplied[i + 3] = a;
  }
  const out = shrink({ width: image.width, height: image.height, data: premultiplied }, width, height);
  for (let i = 0; i < out.data.length; i += 4) {
    const a = out.data[i + 3];
    for (let c = 0; c < 3; c++) out.data[i + c] = a ? Math.min(255, Math.round((out.data[i + c] * 255) / a)) : 0;
  }
  return out;
}
/** A `width` × `height` canvas of one colour ([r, g, b, a]), with `image` composited over it at `x`, `y`. */
export function placeRgba(width, height, image = null, x = 0, y = 0, colour = [0, 0, 0, 0]) {
  const data = new Uint8Array(width * height * 4);
  for (let i = 0; i < width * height; i++) data.set(colour, 4 * i);
  if (image) for (let row = 0; row < image.height; row++) for (let column = 0; column < image.width; column++) {
    if (y + row < 0 || y + row >= height || x + column < 0 || x + column >= width) continue;
    const to = ((y + row) * width + x + column) * 4, from = (row * image.width + column) * 4;
    const over = image.data[from + 3] / 255, under = (data[to + 3] / 255) * (1 - over), out = over + under;
    for (let c = 0; c < 3; c++) data[to + c] = out ? Math.round((image.data[from + c] * over + data[to + c] * under) / out) : 0;
    data[to + 3] = Math.round(out * 255);
  }
  return { width, height, data };
}
const opacity = new Map();
/** Whether every pixel of the PNG is opaque: from its header when it has no alpha channel and no tRNS chunk, else
 * from its pixels. */
export function opaque(path) {
  const { mtimeMs, size } = statSync(path), key = `${path}\0${mtimeMs}\0${size}`;
  if (!opacity.has(key)) {
    const bytes = readFileSync(path), colour = bytes[25];
    let keyed = false;
    for (let at = 8; at + 8 <= bytes.length && !keyed;) {
      const type = bytes.toString('latin1', at + 4, at + 8);
      if (type === 'IDAT') break;
      keyed = type === 'tRNS';
      at += 12 + bytes.readUInt32BE(at);
    }
    opacity.set(key, (colour === 0 || colour === 2) && !keyed ? true : pngAlpha(path).alpha.every((a) => a === 255));
  }
  return opacity.get(key);
}

/** The tinted iOS icon from a `monochrome` PNG (LLP 1163 D3): an opaque grey image whose grey is the glyph's alpha,
 * white on black, the grayscale image Apple asks for. */
export function writeTinted(source, out) {
  const image = readPng(source), data = new Uint8Array(image.data.length);
  for (let i = 0; i < data.length; i += 4) { data.fill(image.data[i + 3], i, i + 3); data[i + 3] = 255; }
  writeFileSync(out, encodePng({ width: image.width, height: image.height, data }));
}

/** Each icon's source and the SHA-256 of its bytes, both schemes: what a cache of built icons keys on. */
export function iconDigests(manifest, dir) {
  return ['light', 'dark'].flatMap((scheme) => list(manifest, scheme).entries.map((entry) => {
    const path = resolve(dir, entry.src);
    return [scheme, entry.src, entry.purpose ?? null, existsSync(path) ? createHash('sha256').update(readFileSync(path)).digest('hex') : null];
  }));
}

/** `icons` as `manifest.json` publishes them: `sizes` and `type` from each file where the
 * author left them out. */
export function publishedIcons(entries, dir) {
  return entries.map((entry) => {
    const file = iconFile(dir, entry.src);
    return file ? { src: entry.src, sizes: entry.sizes ?? file.sizes, type: entry.type ?? TYPES[file.format], ...(entry.purpose ? { purpose: entry.purpose } : {}) } : entry;
  });
}

const esc = (t) => String(t).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/"/g, '&quot;');
/** The web's icon links (LLP 1163 D7): a favicon per scheme, the light one last, so a browser
 * that ignores `media` takes it (HTML, "the last one declared"), then the Apple touch icon.
 * `fallback` is the shell's own favicon link, kept when the manifest has none. */
export function webIconLinks(manifest, dir, fallback) {
  // HTML asks no shape of an icon, so with no square one the favicon takes another.
  const pick = (scheme, formats) => ['square', 'any'].reduce((found, shape) => found ?? pickIcon(manifest, dir, { scheme, purposes: ['any'], shape, px: 32, formats, name: 'the favicon' }), null);
  // A raster, then an SVG: a browser that draws SVG icons takes the later link, one that does not the raster.
  const favicons = (scheme) => [pick(scheme, ['png', 'ico']), pick(scheme, ['svg'])].filter(Boolean);
  const link = (file, media) => `<link rel="icon" type="${TYPES[file.format]}" sizes="${file.sizes}" href="./${esc(file.src)}"${media ? ` media="${media}"` : ''}>`;
  const light = favicons('light'), dark = favicons('dark'), same = light.map((f) => f.src).join() === dark.map((f) => f.src).join();
  // With a dark favicon and no light one, the shell's default is the light one.
  const icons = !dark.length || same ? (light.length ? light.map((f) => link(f)) : [fallback])
    : [...dark.map((f) => link(f, DARK)), ...(light.length ? light.map((f) => link(f, LIGHT)) : [fallback.replace(/>$/, ` media="${LIGHT}">`)])];
  const touch = pickIcon(manifest, dir, { purposes: ['maskable', 'any'], px: 180, formats: ['png'], opaque: 'skip', name: 'the Apple touch icon' });
  return [...icons, ...(touch ? [`<link rel="apple-touch-icon" sizes="${touch.sizes}" href="./${esc(touch.src)}">`] : [])].join('\n');
}

/** The page's theme colours: the dark one first, since a browser takes the first `theme-color`
 * whose `media` matches (HTML, "obtain a page's theme color"). A fixed `userInterfaceStyle` takes
 * its side alone, as the page does. */
export function webThemeColors(manifest) {
  const dark = manifest.color_scheme_dark?.theme_color, light = manifest.theme_color, style = manifest.userInterfaceStyle;
  if (style === 'light' || style === 'dark') {
    const colour = style === 'dark' ? dark ?? light : light;
    return colour ? [`<meta name="theme-color" content="${esc(colour)}">`] : [];
  }
  return [...(dark ? [`<meta name="theme-color" content="${esc(dark)}" media="${DARK}">`] : []), ...(light ? [`<meta name="theme-color" content="${esc(light)}">`] : [])];
}

/** The sizes of the Windows executable's icon: the shell's at 100% to 400% scale (Microsoft, "Construct your
 * Windows app's icon"). */
export const WINDOWS_ICON_SIZES = [16, 20, 24, 32, 40, 48, 64, 256];
/** The Windows executable's icon as a compiled resource file, the `.res` form `rc.exe` writes and MSVC's linker
 * takes as an input: an RT_ICON per size, each a PNG, and RT_GROUP_ICON 1 naming them, which Explorer shows and
 * the host loads for its window (LLP 1163 D6). `image` is the picked icon's pixels. */
export function windowsIconRes(image) {
  const pngs = WINDOWS_ICON_SIZES.map((side) => encodePng(resize(image, side, side)));
  const resource = (type, name, flags, data, language = 0x0409) => {
    const header = Buffer.alloc(32);
    header.writeUInt32LE(data.length, 0); header.writeUInt32LE(32, 4);
    header.writeUInt16LE(0xffff, 8); header.writeUInt16LE(type, 10);
    header.writeUInt16LE(0xffff, 12); header.writeUInt16LE(name, 14);
    header.writeUInt16LE(flags, 20); header.writeUInt16LE(language, 22);
    return Buffer.concat([header, data, Buffer.alloc((4 - (data.length % 4)) % 4)]);
  };
  const group = Buffer.alloc(6 + 14 * pngs.length);
  group.writeUInt16LE(1, 2); group.writeUInt16LE(pngs.length, 4);
  WINDOWS_ICON_SIZES.forEach((side, i) => {
    const at = 6 + 14 * i;
    group[at] = group[at + 1] = side >= 256 ? 0 : side;
    group.writeUInt16LE(1, at + 4); group.writeUInt16LE(32, at + 6);
    group.writeUInt32LE(pngs[i].length, at + 8); group.writeUInt16LE(i + 1, at + 12);
  });
  // RT_ICON is 3 and RT_GROUP_ICON 14; a .res file starts with an empty entry.
  return Buffer.concat([resource(0, 0, 0, Buffer.alloc(0), 0), ...pngs.map((png, i) => resource(3, i + 1, 0x1010, png)), resource(14, 1, 0x1030, group)]);
}
