// CoreUI storage version 17 catalogs. The lookup indexes are checked against
// Apple-generated fixtures and UIKit's named-resource APIs.
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { basename, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { plistFile } from './assets.mjs';
import { bitmapPixels, pngPixels, pngBytes, resizePixels } from './image.mjs';
import { BOM, le, u16, fixed } from './car-format.mjs';
import { composerRecords } from './icon-composer.mjs';

function colorRendition(record) {
  const body = Buffer.alloc(16 + record.rgba.length * 8); le(0x434f4c52, 1, record.space, record.rgba.length).copy(body);
  record.rgba.forEach((value, i) => body.writeDoubleLE(value, 16 + i * 8));
  const tvl = le(1004, 8, 0, 0, 1006, 4, 1);
  const header = Buffer.concat([le(0x43545349, 1, 0, 0, 0, 0, 0, 0, 0), u16(1009, 0), fixed(record.name, 128), le(tvl.length, 1, 0, body.length)]);
  return Buffer.concat([header, tvl, body]);
}

function imageRendition(record) {
  const { width, height, pixels, stride } = record;
  const body = Buffer.concat([Buffer.from('MLEC'), le(0, 0, pixels.length), pixels]);
  const tvl = le(1001, 20, 1, 0, 0, width, height, 1003, 28, 1, 0, 0, 0, 0, width, height, 1004, 8, 0, 0x3f800000, 1006, 4, 1, 1007, 4, stride);
  const header = Buffer.concat([le(0x43545349, 1, record.kind === 'icon' ? 0 : 16, width, height, (record.scale ?? 1) * 100, 0x41524742, record.space ?? 1, 0), u16(12, 0), fixed(record.filename, 128), le(tvl.length, 1, 0, body.length)]);
  return Buffer.concat([header, tvl, body]);
}

function multisizeRendition(record) {
  const body = Buffer.concat([Buffer.from('SISM'), le(1, record.sizes.length), ...record.sizes.map(size => le(...size))]);
  const tvl = le(1004, 8, 0, 0, 1006, 4, 1);
  return Buffer.concat([le(0x43545349, 1, 0, 0, 0, 0, 0, 0, 0), u16(1010, 0), fixed(record.name, 128), le(tvl.length, 1, 0, body.length), tvl, body]);
}

export function assetCatalog(records, minimum = '17.0') {
  const iconGroups = new Map();
  for (const record of records.filter(record => record.kind === 'icon')) {
    const key = `${record.name}:${record.idiom}:${Number(record.dark)}`;
    if (!iconGroups.has(key)) iconGroups.set(key, { ...record, kind: 'multisize', sizes: [] });
    iconGroups.get(key).sizes.push([record.width / record.scale, record.height / record.scale, record.index]);
  }
  const renditions = [...records, ...iconGroups.values()], composed = records.some(record => record.kind === 'composer'), dimension = composed || iconGroups.size > 0;
  const names = [...new Set(records.map(record => record.name))].sort();
  if (names.length >= 65536) throw new Error('host/apple: too many named assets');
  const identifiers = new Map(names.map((name, i) => [name, i + 1]));
  const bom = new BOM(), hash = createHash('sha256');
  for (const record of records) { hash.update(JSON.stringify({ ...record, pixels: undefined, original: undefined })); if (record.pixels) hash.update(record.pixels); }
  const header = Buffer.concat([le(0x43544152, 974, 17, 0, renditions.length), fixed('@(#)PROGRAM:CoreUI  PROJECT:CoreUI-974.1', 128), fixed('Exact portable asset compiler', 256), hash.digest().subarray(0, 16), le(0, 2, 1, 2)]);
  bom.variable('CARHEADER', header);
  const part = record => record.part ?? ({ image: 181, icon: 220, multisize: 218 })[record.kind] ?? 217;
  const attributes = [7, 13, 12, 15, 16, ...(dimension ? [9] : []), ...(composed ? [24] : []), 17, 1, 2];
  bom.tree('RENDITIONS', renditions.map(record => [u16(record.appearance ?? (record.dark ? 1 : 0), 0, record.scale ?? 1, record.idiom ?? 0, 0, ...(dimension ? [record.kind === 'icon' ? record.index : 0] : []), ...(composed ? [0] : []), identifiers.get(record.name), 85, part(record)),
    record.kind === 'composer' ? record.encode(identifiers) : record.kind === 'multisize' ? multisizeRendition(record) : ['image', 'icon'].includes(record.kind) ? imageRendition(record) : colorRendition(record)]));
  bom.tree('FACETKEYS', names.map(name => [Buffer.from(name), u16(0, 0, 3, 1, 85, 2, part(records.find(record => record.name === name && record.part === 245) ?? records.find(record => record.name === name)), 17, identifiers.get(name))]));
  const appearances = [[Buffer.from('UIAppearanceAny'), u16(0)], ...(records.some(record => record.dark) || composed ? [[Buffer.from('UIAppearanceDark'), u16(1)]] : []), ...(composed ? [[Buffer.from('UIAppearanceLight'), u16(4)], [Buffer.from('ISAppearanceTintable'), u16(10)]] : [])];
  bom.tree('APPEARANCEKEYS', appearances);
  bom.variable('KEYFORMAT', le(0x6b666d74, 0, attributes.length, ...attributes));
  const metadata = Buffer.alloc(1028); metadata.write('META'); metadata.write(minimum, 260, 255); metadata.write('ios', 516); metadata.write('Exact portable asset compiler', 772);
  bom.variable('EXTENDED_METADATA', metadata);
  bom.tree('BITMAPKEYS', names.map(name => {
    const matching = renditions.filter(record => record.name === name);
    const mask = read => matching.reduce((bits, record) => bits | (1 << read(record)), 0) >>> 0;
    return [identifiers.get(name), le(1, 0, 4 + attributes.length * 4, attributes.length, 0xffffffff, 1, mask(record => record.scale ?? 1), mask(record => record.idiom ?? 0), 1,
      ...(dimension ? [mask(record => record.kind === 'icon' ? record.index : 0)] : []), ...(composed ? [1] : []), 0xffffffff, 0xffffffff, 0xffffffff)];
  }), true, 1024);
  return bom.finish();
}

function component(value, where) {
  if (!['string', 'number'].includes(typeof value) || value === '') throw new Error(`host/apple: invalid color component in ${where}`);
  const number = typeof value === 'string' && /^0x[\da-f]{2}$/i.test(value) ? parseInt(value.slice(2), 16) / 255 : Number(value);
  if (!Number.isFinite(number) || number < 0 || number > 1) throw new Error(`host/apple: invalid color component in ${where}`);
  return number;
}

export function catalogRecords(catalogs, fallback) {
  const records = [], seen = new Set();
  const walk = (path, namespace = '') => {
    const suffix = extname(path), contents = resolve(path, 'Contents.json');
    if (suffix === '.icon') {
      const icon = composerRecords(path, fallback);
      for (const name of new Set(icon.map(record => record.name))) {
        if (seen.has(name)) throw new Error(`host/apple: duplicate named asset ${name}`); seen.add(name);
      }
      records.push(...icon); return;
    }
    if (['.colorset', '.imageset', '.appiconset'].includes(suffix)) {
      const name = namespace + basename(path, suffix), declaration = JSON.parse(readFileSync(contents, 'utf8'));
      if (seen.has(name)) throw new Error(`host/apple: duplicate named asset ${name}`); seen.add(name);
      const variants = new Set();
      if (Object.keys(declaration.properties ?? {}).length) throw new Error(`host/apple: portable asset properties are not implemented yet in ${contents}`);
      for (const entry of declaration.colors ?? declaration.images ?? []) {
        if (suffix !== '.colorset' && !entry.filename) continue;
        const appearances = entry.appearances ?? [];
        if (appearances.some(a => a.appearance !== 'luminosity' || !['light', 'dark'].includes(a.value))) throw new Error(`host/apple: unsupported asset appearance in ${contents}`);
        const dark = appearances.some(a => a.value === 'dark'), idiom = { universal: 0, iphone: 1, ipad: 2 }[entry.idiom ?? 'universal'];
        if (idiom == null) throw new Error(`host/apple: unsupported asset idiom in ${contents}`);
        if (suffix === '.imageset' || suffix === '.appiconset') {
          const scale = { '1x': 1, '2x': 2, '3x': 3 }[entry.scale ?? '1x'], filename = entry.filename;
          if (!scale || basename(filename) !== filename || extname(filename).toLowerCase() !== '.png') throw new Error(`host/apple: unsupported image source in ${contents}`);
          const original = pngPixels(readFileSync(resolve(path, filename)));
          if (suffix === '.appiconset') {
            if (entry.idiom !== 'universal' || entry.platform !== 'ios' || entry.size !== '1024x1024' || original.width !== 1024 || original.height !== 1024 || scale !== 1)
              throw new Error(`host/apple: portable app icons currently require a universal iOS 1024x1024 image in ${contents}`);
            if (variants.has(String(dark))) throw new Error(`host/apple: duplicate icon appearance in ${contents}`); variants.add(String(dark));
            for (const idiom of [1, 2]) records.push({ name, kind: 'icon', dark, idiom, scale, index: 1, filename, original, ...bitmapPixels(original) });
            continue;
          }
          const variant = `${dark}:${idiom}:${scale}`;
          if (variants.has(variant)) throw new Error(`host/apple: duplicate image variant in ${contents}`); variants.add(variant);
          records.push({ name, kind: 'image', dark, idiom, scale, filename, ...bitmapPixels(original) });
          continue;
        }
        const color = entry.color, c = color?.components, space = { srgb: 1, 'gray-gamma-22': 2, 'display-p3': 3 }[color?.['color-space']];
        if (!c || !space) throw new Error(`host/apple: unsupported color encoding in ${contents}`);
        const variant = `${dark}:${idiom}`;
        if (variants.has(variant)) throw new Error(`host/apple: duplicate color variant in ${contents}`); variants.add(variant);
        records.push({ name, dark, idiom, space, rgba: (space === 2 ? [c.white, c.alpha] : [c.red, c.green, c.blue, c.alpha]).map(value => component(value, contents)) });
      }
      if (!variants.size) throw new Error(`host/apple: empty asset set in ${contents}`);
      return;
    }
    if (suffix && suffix !== '.xcassets') throw new Error(`host/apple: the portable asset compiler does not yet implement ${suffix} (${path})`);
    const entries = readdirSync(path, { withFileTypes: true });
    const info = entries.some(entry => entry.name === 'Contents.json') ? JSON.parse(readFileSync(contents, 'utf8')) : {};
    const prefix = namespace + (info.properties?.['provides-namespace'] ? basename(path) + '/' : '');
    for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name, 'en'))) if (entry.isDirectory()) walk(resolve(path, entry.name), prefix);
  };
  for (const path of catalogs) walk(path);
  return records;
}

/** Read names and rendition metadata from the compiled file, independently of its source catalog. */
export function assetCatalogInfo(bytes) {
  const fail = () => { throw new Error('host/apple: malformed asset catalog'); };
  const range = (offset, length) => { if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 0 || offset + length > bytes.length) fail(); return bytes.subarray(offset, offset + length); };
  const integer = offset => range(offset, 4).readUInt32BE();
  if (range(0, 8).toString() !== 'BOMStore' || integer(8) !== 1) fail();
  const index = integer(16), count = integer(index); range(index + 4, count * 8);
  const block = id => { if (!id || id >= count) fail(); return range(integer(index + 4 + id * 8), integer(index + 8 + id * 8)); };
  const variables = new Map(), start = integer(24), end = start + integer(28); range(start, end - start);
  let offset = start + 4;
  for (let i = 0; i < integer(start); i++) {
    const id = integer(offset), length = range(offset + 4, 1)[0];
    if (offset + 5 + length > end) fail();
    const name = range(offset + 5, length).toString(); if (variables.has(name)) fail();
    variables.set(name, id); offset += 5 + length;
  }
  const tree = name => {
    const header = block(variables.get(name));
    if (header.length < 29 || header.subarray(0, 4).toString() !== 'tree') fail();
    const rows = [], visited = new Set();
    const visit = (id, depth = 0) => {
      if (visited.has(id) || depth > 32) fail(); visited.add(id);
      const node = block(id); if (node.length < 12) fail();
      const leaf = node.readUInt16BE(), length = node.readUInt16BE(2);
      if (leaf > 1 || 12 + length * 8 > node.length) fail();
      for (let i = 0; i < length; i++) {
        const value = node.readUInt32BE(12 + i * 8), key = node.readUInt32BE(16 + i * 8);
        if (leaf) rows.push([header[20] ? key : block(key), block(value)]); else visit(value, depth + 1);
      }
    };
    visit(header.readUInt32BE(8)); return rows;
  };
  const format = block(variables.get('KEYFORMAT'));
  if (format.length < 12 || format.readUInt32LE() !== 0x6b666d74 || format.readUInt32LE(8) > 64 || format.length !== 12 + format.readUInt32LE(8) * 4) fail();
  const attributes = Array.from({ length: format.readUInt32LE(8) }, (_, i) => format.readUInt32LE(12 + i * 4));
  if (![12, 15, 17, 2].every(attribute => attributes.includes(attribute))) fail();
  const names = new Map();
  for (const [key, value] of tree('FACETKEYS')) {
    if (value.length < 6 || value.length !== 6 + value.readUInt16LE(4) * 4) fail();
    for (let i = 6; i < value.length; i += 4) if (value.readUInt16LE(i) === 17) names.set(value.readUInt16LE(i + 2), key.toString());
  }
  const car = block(variables.get('CARHEADER'));
  if (car.length !== 436 || car.readUInt32LE() !== 0x43544152 || car.readUInt32LE(8) !== 17) fail();
  const info = [{ StorageVersion: car.readUInt32LE(8) }];
  for (const [key, value] of tree('RENDITIONS')) {
    if (key.length !== attributes.length * 2 || value.length < 184 || value.readUInt32LE() !== 0x43545349) fail();
    const token = attribute => key.readUInt16LE(attributes.indexOf(attribute) * 2), name = names.get(token(17));
    const layout = value.readUInt16LE(36), types = { 12: token(2) === 220 ? 'Icon Image' : 'Image', 9: 'Vector', 1009: 'Color', 1010: 'MultiSized Image', 1019: 'Icon Stack', 1020: 'Icon Group', 1021: 'Gradient' };
    if (!name || !types[layout]) fail();
    const body = 184 + value.readUInt32LE(168), length = value.readUInt32LE(180);
    if (body + length !== value.length) fail();
    info.push({ Name: name, NameIdentifier: token(17), AssetType: types[layout], Scale: token(12), Idiom: { 0: 'universal', 1: 'phone', 2: 'pad' }[token(15)],
      ...(layout === 12 ? { PixelWidth: value.readUInt32LE(12), PixelHeight: value.readUInt32LE(16) } : {}) });
  }
  return info;
}

export function compileCatalogCommand(args) {
  const options = {}, catalogs = [];
  const flags = new Set(['--compile', '--platform', '--minimum-deployment-target', '--target-device', '--app-icon', '--alternate-app-icon', '--accent-color', '--output-partial-info-plist', '--output-format', '--exact-icon-fallback']);
  for (let i = 0; i < args.length; i++) {
    if (flags.has(args[i])) { const flag = args[i]; if (args[i + 1] == null) throw new Error(`${flag} needs a value`); const value = args[++i]; options[flag] = flag === '--alternate-app-icon' ? [...(options[flag] ?? []), value] : value; }
    else if (args[i].startsWith('-')) throw new Error(`host/apple: unsupported asset compiler option ${args[i]}`);
    else catalogs.push(resolve(args[i]));
  }
  if (!['iphoneos', 'iphonesimulator'].includes(options['--platform'])) throw new Error('host/apple: the portable catalog compiler currently targets iOS');
  const output = options['--compile'], minimum = options['--minimum-deployment-target'] ?? '17.0';
  if (!output || !catalogs.length || !/^\d+(\.\d+){0,2}$/.test(minimum)) throw new Error('host/apple: asset compilation needs catalogs, an output directory and a valid deployment target');
  const records = catalogRecords(catalogs, options['--exact-icon-fallback']), bytes = assetCatalog(records, minimum);
  const keys = options['--accent-color'] ? { NSAccentColorName: options['--accent-color'] } : {};
  const primary = options['--app-icon'], alternates = options['--alternate-app-icon'] ?? [];
  const icon = name => records.find(record => record.kind === 'icon' && record.name === name);
  for (const name of [primary, ...alternates].filter(Boolean)) if (!icon(name)) throw new Error(`host/apple: no app icon named ${name}`);
  if (primary) {
    const alternateKeys = alternates.length ? { CFBundleAlternateIcons: Object.fromEntries(alternates.map(name => [name, { CFBundleIconName: name }])) } : {};
    for (const [key, files] of [['CFBundleIcons', [`${primary}60x60`]], ['CFBundleIcons~ipad', [`${primary}60x60`, `${primary}76x76`]]])
      keys[key] = { CFBundlePrimaryIcon: { CFBundleIconFiles: files, CFBundleIconName: primary }, ...alternateKeys };
  } else if (alternates.length) throw new Error('host/apple: alternate icons require a primary app icon');
  mkdirSync(output, { recursive: true }); writeFileSync(resolve(output, 'Assets.car'), bytes);
  if (primary) for (const [name, size] of [[`${primary}60x60@2x.png`, 120], [`${primary}76x76@2x~ipad.png`, 152]])
    writeFileSync(resolve(output, name), pngBytes(resizePixels(icon(primary).original, size, size)));
  if (options['--output-partial-info-plist']) writeFileSync(options['--output-partial-info-plist'], plistFile(keys));
  return { records, keys };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv[2] === '--info' && process.argv.length === 4) console.log(JSON.stringify(assetCatalogInfo(readFileSync(process.argv[3]))));
    else compileCatalogCommand(process.argv.slice(2));
  }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
