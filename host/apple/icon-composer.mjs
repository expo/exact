// Live CoreUI icon stacks retain the authored SVG layers. Older iOS versions use
// the app's PNG artwork declared in the web manifest's icons field.
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { le, u16, fixed } from './car-format.mjs';
import { bitmapPixels, pngPixels, resizePixels } from './image.mjs';

const string = value => Buffer.from(value + '\0');
const tag = (id, bytes) => Buffer.concat([le(id, bytes.length), bytes]);
const f32 = value => { const bytes = Buffer.alloc(4); bytes.writeFloatLE(value); return bytes.readUInt32LE(); };
const common = opaque => Buffer.concat([tag(1004, le(0, opaque ? f32(1) : 0)), tag(1006, le(1))]);
const csi = (name, layout, tags, body, { flags = 0, width = 0, height = 0, scale = 0, format = 0 } = {}) =>
  Buffer.concat([le(0x43545349, 1, flags, width, height, scale, format, 0, 0), u16(layout, 0), fixed(name, 128), le(tags.length, 1, 0, body.length), tags, body]);

export function composerRecords(path, fallback) {
  const source = readFileSync(resolve(path, 'icon.json')), document = JSON.parse(source);
  const refuse = message => { throw new Error(`host/apple: ${path}: ${message}`); };
  const only = (object, keys) => object && !Array.isArray(object) && typeof object === 'object' && Object.keys(object).every(key => keys.includes(key));
  if (!only(document, ['fill', 'groups', 'supported-platforms']) || !only(document.fill, ['solid']) ||
      !only(document['supported-platforms'], ['squares']) || document['supported-platforms'].squares !== 'shared' ||
      document.groups?.length !== 1 || !only(document.groups[0], ['layers']) || !Array.isArray(document.groups[0].layers) || !document.groups[0].layers.length)
    refuse('portable Icon Composer currently supports one default SVG group with a solid sRGB fill');
  const components = /^srgb:([\d.,]+)$/.exec(document.fill.solid)?.[1].split(',').map(value => value === '' ? NaN : Number(value));
  if (components?.length !== 4 || components.some(value => !Number.isFinite(value) || value < 0 || value > 1) || components[3] !== 1)
    refuse('the icon background must be opaque sRGB');
  const fill = components.map(value => Math.round(value * 1000) / 1000);
  const name = basename(path, '.icon'), assets = name + '_Assets/', group = name + '/Group', gradient = assets + 'Gradient-1';
  const digest = createHash('sha256').update(source), names = new Set();
  const vectors = document.groups[0].layers.toReversed().map(layer => {
    const file = layer?.['image-name'];
    if (!only(layer, ['name', 'image-name']) || typeof file !== 'string' || basename(file) !== file || !file.endsWith('.svg'))
      refuse('each default layer must name an SVG in Assets');
    const name = assets + basename(file, '.svg'), xml = readFileSync(resolve(path, 'Assets', file));
    if (names.has(name)) refuse(`duplicate SVG layer ${file}`); names.add(name); digest.update(xml);
    return { name, xml };
  });
  if (!fallback) refuse('Linux builds require a PNG icons entry for the older-iOS fallback');
  let original = pngPixels(readFileSync(fallback));
  if (original.width !== original.height || original.width < 512) refuse('the PNG fallback must be square and at least 512 pixels');
  if (original.width !== 1024) original = resizePixels(original, 1024, 1024);
  const sourceDigest = digest.digest('hex'), records = [];
  const add = (name, part, appearance, encode) => records.push({ name, kind: 'composer', part, appearance, sourceDigest, encode });
  const colors = [1, 2, 3, 4].map(number => assets + 'Color-' + number);
  for (const [index, space, rgba] of [[0, 6, [1, 1]], [1, 1, fill], [2, 2, [.192, 1]], [3, 2, [.078, 1]]]) {
    const body = Buffer.alloc(16 + rgba.length * 8); le(0x434f4c52, 1, space, rgba.length).copy(body);
    rgba.forEach((value, i) => body.writeDoubleLE(Math.fround(value), 16 + i * 8));
    // Named icon colors carry their color space in the body, as ordinary colors do.
    add(colors[index], 217, 0, () => csi(colors[index], 1009, common(false), body));
  }
  for (const vector of vectors) add(vector.name, 181, 0, () => csi('image.svg', 9, common(true),
    Buffer.concat([Buffer.from('DWAR'), le(0, vector.xml.length), vector.xml]), { flags: 4, format: 0x53564720 }));
  const colorString = name => Buffer.concat([le(Buffer.byteLength(name) + 1), string(name)]);
  const gradientBody = Buffer.concat([Buffer.from('ARGG'), le(2, 1, 0, f32(.5), 0, f32(.5), f32(1), 0), colorString(colors[2]), le(f32(1)), colorString(colors[3])]);
  add(gradient, 247, 0, () => csi(gradient, 1021, common(false), gradientBody));
  const children = (items, ids) => tag(1012, Buffer.concat([le(items.length, 0), ...items.map(([name, part, extent]) =>
    Buffer.concat([le(0, 0, 0, extent, extent, 0, f32(1), 16), u16(1, 85, 2, part, 17, ids.get(name), 0, 0)]))]));
  const layerAttributes = items => tag(1020, Buffer.concat([le(items.length, 0), ...items.map(([type, color = '']) =>
    Buffer.concat([le(type, 0, Buffer.byteLength(color) + 1), string(color)]))]));
  const glass = items => tag(1021, Buffer.concat([le(items.length, 0), ...items.map(item => le(...item))]));
  const empty = Buffer.concat([Buffer.from('DWAR'), le(0, 0)]);
  for (const appearance of [1, 4, 10]) {
    add(group, 246, appearance, ids => csi('IconGroup', 1020, Buffer.concat([
      children(vectors.map(vector => [vector.name, 181, 1024]), ids),
      layerAttributes(vectors.map(() => [1, appearance === 1 ? colors[1] : ''])), glass(vectors.map(() => [0, 0, 0, 0, 0])), common(true),
    ]), empty, { scale: 100, format: 0x44415441 }));
    add(name, 245, appearance, ids => csi(name + '.iconstack', 1019, Buffer.concat([
      children([[appearance === 1 ? gradient : colors[1], appearance === 1 ? 247 : 217, 0], [group, 246, 0]], ids),
      layerAttributes([[0], [2]]), glass([[0, 0, 0, 0, 0], [1, f32(.5), 3, f32(.5), 0]]), tag(1004, le(0, f32(1))),
      tag(1005, Buffer.concat([le(20, 0), string('public.layeredimage')])), tag(1006, le(1)),
    ]), empty, { width: 1024, height: 1024, scale: 100, format: 0x44415441 }));
  }
  for (const idiom of [1, 2]) records.push({ name, kind: 'icon', dark: false, idiom, scale: 1, index: 1, filename: basename(fallback), original, ...bitmapPixels(original) });
  return records;
}
