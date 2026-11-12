import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { assetCatalog, assetCatalogInfo, catalogRecords, compileCatalogCommand } from './car.mjs';
import { appleCommand } from './toolchain.mjs';
import { plistFile } from './assets.mjs';
import { encodePng } from '../../scripts/png.mjs';
import { pngPixels, pngBytes } from './image.mjs';

function fixture(run) {
  const root = mkdtempSync(resolve(tmpdir(), 'exact-catalog-'));
  const catalog = resolve(root, 'Colors.xcassets'), bundle = resolve(root, 'Colors.bundle');
  mkdirSync(catalog); mkdirSync(bundle);
  const color = (rgba, dark = false) => ({ idiom: 'universal', ...(dark ? { appearances: [{ appearance: 'luminosity', value: 'dark' }] } : {}), color: {
    'color-space': 'srgb', components: Object.fromEntries(['red', 'green', 'blue', 'alpha'].map((name, i) => [name, rgba[i].toFixed(6)])),
  } });
  const put = (name, colors) => {
    const dir = resolve(catalog, `${name}.colorset`); mkdirSync(dir, { recursive: true });
    writeFileSync(resolve(dir, 'Contents.json'), JSON.stringify({ info: { version: 1, author: 'exact' }, colors }));
  };
  put('Demo', [color([0.1, 0.2, 0.3, 1]), color([0.6, 0.7, 0.8, 1], true)]);
  put('Other', [color([0.9, 0.2, 0.3, 1])]);
  put('AccentColor', [color([0, 0.5, 1, 0.75])]);
  const picture = resolve(catalog, 'Picture.imageset'); mkdirSync(picture);
  const images = [1, 2, 3].map(scale => {
    const filename = `picture@${scale}x.png`, width = scale * 2, data = Buffer.alloc(width * width * 4);
    for (let y = 0; y < width; y++) for (let x = 0; x < width; x++) data.set([[255,0,0,255], [0,255,0,128], [0,0,255,255], [255,255,255,0]][Math.floor(y / scale) * 2 + Math.floor(x / scale)], (y * width + x) * 4);
    writeFileSync(resolve(picture, filename), encodePng({ width, height: width, data }));
    return { idiom: 'universal', filename, scale: `${scale}x` };
  });
  writeFileSync(resolve(picture, 'Contents.json'), JSON.stringify({ images, info: { version: 1, author: 'exact' } }));
  const p3 = resolve(catalog, 'P3Picture.imageset'); mkdirSync(p3);
  const profile = pngPixels(readFileSync(new URL('../../apps/ui-gallery/assets/icon.png', import.meta.url))).profile;
  writeFileSync(resolve(p3, 'p3.png'), pngBytes({ width: 1, height: 1, data: Buffer.from([255, 0, 0, 255]), space: 3, profile }));
  writeFileSync(resolve(p3, 'Contents.json'), JSON.stringify({ images: [{ idiom: 'universal', filename: 'p3.png', scale: '1x' }], info: { version: 1, author: 'exact' } }));
  const args = [catalog, '--platform', 'iphonesimulator', '--compile', bundle, '--minimum-deployment-target', '17.0', '--accent-color', 'AccentColor', '--output-partial-info-plist', resolve(root, 'partial.plist')];
  writeFileSync(resolve(bundle, 'Info.plist'), plistFile({ CFBundleIdentifier: 'com.exact.catalog-tests', CFBundleName: 'Colors', CFBundlePackageType: 'BNDL', CFBundleVersion: '1' }));
  try { run({ root, catalog, bundle, args, color, put }); }
  finally { rmSync(root, { recursive: true, force: true }); }
}

test('catalog compilation preserves named color variants and produces deterministic bytes, refusing bad input before replacement', () => fixture(({ root, catalog, bundle, args, color, put }) => {
  const result = compileCatalogCommand(args);
  expect(result.keys).toEqual({ NSAccentColorName: 'AccentColor' });
  expect(result.records).toHaveLength(8);
  const before = readFileSync(resolve(bundle, 'Assets.car'));
  expect(assetCatalogInfo(before).filter(row => row.AssetType === 'Image')).toHaveLength(4);
  const inspected = spawnSync(...appleCommand('xcrun', ['assetutil', '--info', resolve(bundle, 'Assets.car')], { encoding: 'utf8' }, 'linux'));
  expect(inspected.status, inspected.stderr).toBe(0); expect(JSON.parse(inspected.stdout)).toEqual(assetCatalogInfo(before));
  const broken = Buffer.from(before); broken.writeUInt32BE(0xffffffff, 16);
  expect(() => assetCatalogInfo(broken)).toThrow('malformed');
  compileCatalogCommand(args); expect(readFileSync(resolve(bundle, 'Assets.car'))).toEqual(before);
  expect(readFileSync(resolve(root, 'partial.plist'), 'utf8')).toContain('<key>NSAccentColorName</key>');
  expect(() => catalogRecords([catalog, catalog])).toThrow('duplicate named asset');
  put('Demo', [color([0.1, 0.2, 0.3, 2])]);
  expect(() => compileCatalogCommand(args)).toThrow('invalid color component');
  expect(readFileSync(resolve(bundle, 'Assets.car'))).toEqual(before);
}));

test.skipIf(process.platform !== 'darwin')('Apple reads the compiled color names, appearances, spaces and component values', () => fixture(({ catalog, bundle, args, color, put }) => {
  const p3 = color([0.1, 0.2, 0.3, 0.8]); p3.color['color-space'] = 'display-p3'; put('P3', [p3]);
  put('Gray', [{ idiom: 'universal', color: { 'color-space': 'gray-gamma-22', components: { white: '0.5', alpha: '0.8' } } }]);
  compileCatalogCommand(args);
  const result = spawnSync('xcrun', ['assetutil', '--info', resolve(bundle, 'Assets.car')], { encoding: 'utf8' });
  expect(result.status).toBe(0);
  const info = JSON.parse(result.stdout), colors = info.filter(row => row.AssetType === 'Color');
  expect(info[0].Appearances).toEqual({ UIAppearanceAny: 0, UIAppearanceDark: 1 });
  expect(colors.map(row => row.Name).sort()).toEqual(['AccentColor', 'Demo', 'Demo', 'Gray', 'Other', 'P3']);
  expect(colors.find(row => row.Name === 'Demo' && row.Appearance === 'UIAppearanceDark')['Color components']).toEqual([0.6, 0.7, 0.8, 1]);
  expect(colors.every(row => row.Idiom === 'universal')).toBe(true);
  expect(colors.find(row => row.Name === 'Other').Colorspace).toBe('srgb');
  expect(colors.find(row => row.Name === 'P3').Colorspace).toBe('p3');
  expect(colors.find(row => row.Name === 'Gray')['Color components']).toEqual([0.5, 0.8]);
  expect(catalogRecords([catalog]).find(row => row.name === 'P3').space).toBe(3);
  expect(info.find(row => row.Name === 'P3Picture').Colorspace).toBe('p3');
  const images = info.filter(row => row.AssetType === 'Image' && row.Name === 'Picture');
  expect(images.map(row => row.Scale).sort()).toEqual([1, 2, 3]);
  expect(images.every(row => row.Name === 'Picture' && row.PixelWidth === row.Scale * 2 && row.PixelHeight === row.Scale * 2)).toBe(true);
}));

test.skipIf(process.platform !== 'darwin' || !process.env.EXACT_CAR_SIM)('UIKit resolves colors and image scales with the portable and Apple catalog compilers', () => fixture(({ root, bundle, args }) => {
  compileCatalogCommand(args);
  const source = resolve(root, 'Check.swift'), executable = resolve(root, 'Check');
  writeFileSync(source, `import UIKit
import Foundation
let bundle = Bundle(url: URL(fileURLWithPath: CommandLine.arguments[1]))!
let cases: [(String, UIUserInterfaceStyle, [CGFloat])] = [
    ("Demo", .light, [0.1,0.2,0.3,1]), ("Demo", .dark, [0.6,0.7,0.8,1]),
    ("Other", .light, [0.9,0.2,0.3,1]), ("Other", .dark, [0.9,0.2,0.3,1]),
    ("AccentColor", .light, [0,0.5,1,0.75]), ("AccentColor", .dark, [0,0.5,1,0.75])]
for (name, style, expected) in cases {
    guard let asset = UIColor(named: name, in: bundle, compatibleWith: nil) else { print("missing \\(name)"); exit(2) }
    let color = asset.resolvedColor(with: UITraitCollection(userInterfaceStyle: style))
    var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
    guard color.getRed(&r, green: &g, blue: &b, alpha: &a), zip([r,g,b,a],expected).allSatisfy({ abs($0.0-$0.1)<0.0001 }) else { print("wrong color \\(name): \\([r,g,b,a])"); exit(3) }
}
print("UIKit catalog variants passed")
for scale in [1.0, 2.0, 3.0] {
    guard let image = UIImage(named: "Picture", in: bundle, compatibleWith: UITraitCollection(displayScale: scale)), let cg = image.cgImage else { print("missing picture"); exit(4) }
    guard image.scale == scale && image.size == CGSize(width: 2, height: 2) && cg.width == Int(scale * 2) && cg.height == Int(scale * 2) else { print("wrong image dimensions: requested \\(scale), got \\(image.scale) \\(image.size), pixels \\(cg.width)x\\(cg.height)"); exit(5) }
    var data = [UInt8](repeating: 0, count: cg.width * cg.height * 4)
    let ok = data.withUnsafeMutableBytes { bytes -> Bool in
        guard let context = CGContext(data: bytes.baseAddress, width: cg.width, height: cg.height, bitsPerComponent: 8, bytesPerRow: cg.width * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
        context.draw(cg, in: CGRect(x: 0, y: 0, width: cg.width, height: cg.height)); return true
    }
    guard ok else { exit(6) }
    let expected: [[UInt8]] = [[255,0,0,255], [0,128,0,128], [0,0,255,255], [0,0,0,0]]
    for y in 0..<cg.height { for x in 0..<cg.width {
        let offset = (y * cg.width + x) * 4, quadrant = (y / Int(scale)) * 2 + (x / Int(scale))
        guard Array(data[offset..<offset+4]) == expected[quadrant] else { print("wrong image pixels"); exit(7) }
    } }
}
print("UIKit image scales and pixels passed")
guard let p3 = UIImage(named: "P3Picture", in: bundle, compatibleWith: nil)?.cgImage else { print("missing P3 image"); exit(8) }
var p3Pixels = [UInt8](repeating: 0, count: 4)
let p3OK = p3Pixels.withUnsafeMutableBytes { bytes -> Bool in
    guard let context = CGContext(data: bytes.baseAddress, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4, space: CGColorSpace(name: CGColorSpace.displayP3)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
    context.draw(p3, in: CGRect(x: 0, y: 0, width: 1, height: 1)); return true
}
guard p3OK && zip(p3Pixels, [255,0,0,255]).allSatisfy({ abs(Int($0.0)-$0.1)<=1 }) else { print("P3 color changed: \\(p3Pixels), \\(String(describing: p3.colorSpace))"); exit(8) }
`);
  const compiled = spawnSync('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', '-target', 'arm64-apple-ios17.0-simulator', source, '-o', executable], { encoding: 'utf8' });
  expect(compiled.status).toBe(0);
  const ran = spawnSync('xcrun', ['simctl', 'spawn', process.env.EXACT_CAR_SIM, executable, bundle], { encoding: 'utf8' });
  expect(ran.stderr).toBe(''); expect(ran.status, ran.stdout).toBe(0); expect(ran.stdout).toContain('UIKit catalog variants passed'); expect(ran.stdout).toContain('UIKit image scales and pixels passed');
  const native = spawnSync('xcrun', ['actool', ...args], { encoding: 'utf8' });
  expect(native.status, native.stderr).toBe(0);
  const reference = spawnSync('xcrun', ['simctl', 'spawn', process.env.EXACT_CAR_SIM, executable, bundle], { encoding: 'utf8' });
  expect(reference.status, reference.stdout + reference.stderr).toBe(0);
  expect(reference.stdout).toBe(ran.stdout);
}), 30000);


test('primary and alternate icons retain phone and tablet renditions beside ordinary assets', () => fixture(({ root, catalog, bundle, args }) => {
  for (const [name, color] of [['Primary', [255, 0, 0, 255]], ['Alternate', [0, 0, 255, 255]]]) {
    const set = resolve(catalog, `${name}.appiconset`); mkdirSync(set);
    const data = Buffer.alloc(1024 * 1024 * 4);
    for (let i = 0; i < data.length; i += 4) data.set(color, i);
    writeFileSync(resolve(set, 'icon.png'), encodePng({ width: 1024, height: 1024, data }));
    writeFileSync(resolve(set, 'Contents.json'), JSON.stringify({ images: [{ filename: 'icon.png', idiom: 'universal', platform: 'ios', size: '1024x1024' }], info: { version: 1, author: 'exact' } }));
  }
  const command = [...args, '--app-icon', 'Primary', '--alternate-app-icon', 'Alternate'];
  const { keys } = compileCatalogCommand(command);
  expect(keys.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName).toBe('Primary');
  expect(keys.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconFiles).toEqual(['Primary60x60']);
  expect(readFileSync(resolve(bundle, 'Primary60x60@2x.png')).subarray(1, 4).toString()).toBe('PNG');
  expect(keys['CFBundleIcons~ipad'].CFBundleAlternateIcons).toEqual({ Alternate: { CFBundleIconName: 'Alternate' } });
  const file = resolve(bundle, 'Assets.car'), rows = assetCatalogInfo(readFileSync(file));
  expect(rows.filter(row => row.AssetType === 'Icon Image')).toHaveLength(4);
  expect(rows.filter(row => row.AssetType === 'MultiSized Image')).toHaveLength(4);
  if (process.platform !== 'darwin' || !process.env.EXACT_CAR_SIM) return;
  const source = resolve(root, 'IconProbe.m'), executable = resolve(root, 'IconProbe');
  writeFileSync(source, readFileSync(new URL('../../scripts/fixtures/catalog-icon.m', import.meta.url), 'utf8').replaceAll('@"AppIcon"', '@"Primary"'));
  const compiled = spawnSync('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-target', 'arm64-apple-ios17.0-simulator', '-fobjc-arc', '-framework', 'UIKit', '-framework', 'CoreGraphics', '-framework', 'Foundation', source, '-o', executable], { encoding: 'utf8' });
  expect(compiled.status, compiled.stderr).toBe(0);
  const inspect = () => {
    const ran = spawnSync('xcrun', ['simctl', 'spawn', process.env.EXACT_CAR_SIM, executable, file], { encoding: 'utf8' });
    expect(ran.status, ran.stdout + ran.stderr).toBe(0);
    expect(ran.stdout).toContain('Mixed catalog colors and image scales passed');
    return ran.stdout;
  };
  const ours = inspect();
  const native = spawnSync('xcrun', ['actool', ...command], { encoding: 'utf8' });
  expect(native.status, native.stderr).toBe(0);
  expect(inspect()).toBe(ours);
}), 30000);


test('large catalogs retain all names across internal and leaf index nodes', () => fixture(({ root, bundle }) => {
  const records = Array.from({ length: 700 }, (_, i) => ({ name: `Color${i}`, space: 1, rgba: [i / 700, 0.25, 0.8, 1] }));
  const file = resolve(bundle, 'Assets.car'); writeFileSync(file, assetCatalog(records));
  expect(assetCatalogInfo(readFileSync(file)).filter(row => row.AssetType === 'Color')).toHaveLength(700);
  if (process.platform !== 'darwin') return;
  const info = spawnSync('xcrun', ['assetutil', '--info', file], { encoding: 'utf8' });
  expect(info.status, info.stderr).toBe(0); expect(JSON.parse(info.stdout).filter(row => row.AssetType === 'Color')).toHaveLength(700);
  if (!process.env.EXACT_CAR_SIM) return;
  const source = resolve(root, 'Large.swift'), executable = resolve(root, 'Large');
  writeFileSync(source, `import UIKit
import Foundation
let bundle=Bundle(url: URL(fileURLWithPath: CommandLine.arguments[1]))!
for i in 0..<700 {
    let name=String(format: "Color%d",i)
    guard let color=UIColor(named:name,in:bundle,compatibleWith:nil) else { print("missing \\(name)"); exit(2) }
    var r:CGFloat=0,g:CGFloat=0,b:CGFloat=0,a:CGFloat=0
    guard color.getRed(&r,green:&g,blue:&b,alpha:&a), abs(r-CGFloat(i)/700)<0.0001, abs(g-0.25)<0.0001, abs(b-0.8)<0.0001, a==1 else { print("wrong \\(name)"); exit(3) }
}
print("700 named colors passed")
`);
  const compiled=spawnSync('xcrun',['--sdk','iphonesimulator','swiftc','-target','arm64-apple-ios17.0-simulator',source,'-o',executable],{encoding:'utf8'});
  expect(compiled.status,compiled.stderr).toBe(0);
  const ran=spawnSync('xcrun',['simctl','spawn',process.env.EXACT_CAR_SIM,executable,bundle],{encoding:'utf8'});
  expect(ran.status,ran.stdout+ran.stderr).toBe(0);expect(ran.stdout).toContain('700 named colors passed');
}),30000);


test('Icon Composer retains live SVG stacks and explicit legacy artwork beside ordinary assets', () => fixture(({ root, bundle, args }) => {
  const project = resolve(import.meta.dir, '../../apps/duo-lab/art/DuoLab.icon'), fallback = resolve(project, '../../assets/icon.png');
  const command = [...args, project, '--app-icon', 'DuoLab', '--exact-icon-fallback', fallback];
  compileCatalogCommand(command);
  const bytes = readFileSync(resolve(bundle, 'Assets.car')), info = assetCatalogInfo(bytes);
  expect(info.filter(row => row.Name === 'DuoLab' && row.AssetType === 'Icon Stack')).toHaveLength(3);
  expect(info.filter(row => row.Name === 'DuoLab' && row.AssetType === 'Icon Image')).toHaveLength(2);
  expect(info.filter(row => row.AssetType === 'Vector')).toHaveLength(2);
  expect(info.some(row => row.Name === 'AccentColor' && row.AssetType === 'Color')).toBe(true);
  expect(pngPixels(readFileSync(resolve(bundle, 'DuoLab60x60@2x.png')))).toMatchObject({ width: 120, height: 120 });
  compileCatalogCommand(command); expect(readFileSync(resolve(bundle, 'Assets.car'))).toEqual(bytes);
  expect(() => compileCatalogCommand([...args, project])).toThrow('PNG icons entry');
  expect(readFileSync(resolve(bundle, 'Assets.car'))).toEqual(bytes);
  if (process.platform === 'darwin') {
    const native = spawnSync('xcrun', ['assetutil', '--info', resolve(bundle, 'Assets.car')], { encoding: 'utf8' });
    expect(native.status, native.stderr).toBe(0);
    const inspected = JSON.parse(native.stdout);
    expect(inspected.some(row => row.Name === 'DuoLab')).toBe(true);
  }
}));
