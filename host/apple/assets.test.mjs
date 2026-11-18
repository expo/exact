import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { appleAssets, appleAssetCatalogInventory, appleRefusals, composerFallback, rasterIconAssets, iosAssets, iosExecutableRefusal, nativeResourceMappings, plistFile, RESERVED_RESOURCE_ROOTS } from './assets.mjs';
import { pngPixels } from './image.mjs';
import { encodePng } from '../../scripts/png.mjs';
import { alternateIcons } from '../../scripts/app-icons.mjs';
import { useXcode } from './devices.mjs';

test.skipIf(process.platform !== 'darwin')('SVG icon catalogs include independent vector renditions and invalidate on an edit', () => {
  useXcode();
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-svg-icons-'));
  try {
    const app = { dir, name: 'Icons', manifest: {}, appleIcons: [{ svg: 'two.svg', viewBox: '0 0 24 24' }] };
    mkdirSync(resolve(dir, 'assets'));
    const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 24 24"><path fill="#ff0000" d="M0 0h12v24H0Z"/><path fill="#0000ff" d="M12 0h12v24H12Z"/></svg>';
    writeFileSync(resolve(dir, 'assets/two.svg'), svg);
    const inventory = appleAssetCatalogInventory(app, 'macos');
    const catalog = resolve(dir, 'Icons.xcassets');
    rasterIconAssets(inventory, app, catalog);
    const set = resolve(catalog, readdirSync(catalog)[0]);
    const images = JSON.parse(readFileSync(resolve(set, 'Contents.json'))).images;
    assert.deepEqual(images.map(i => i.scale), ['1x', '2x', '3x']);
    for (const [i, image] of images.entries()) {
      const pixels = pngPixels(readFileSync(resolve(set, image.filename)));
      assert.equal(pixels.width, 64 * (i + 1));
      assert.equal(pixels.height, 32 * (i + 1));
      const sample = x => [...pixels.data.subarray((pixels.width * 8 + x) * 4, (pixels.width * 8 + x) * 4 + 4)];
      assert.deepEqual(sample(pixels.width * 3 / 8), [255, 0, 0, 255]);
      assert.deepEqual(sample(pixels.width * 5 / 8), [0, 0, 255, 255]);
    }
    const kept = { dir: resolve(dir, 'cache'), stamp: 'fixture' };
    for (const platform of ['macos', 'ios']) {
      const bundle = resolve(dir, platform); mkdirSync(bundle);
      appleAssets(app, bundle, { platform, icons: app.appleIcons, kept });
      const result = spawnSync('xcrun', ['assetutil', '--info', resolve(bundle, 'Assets.car')], { encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      const assets = JSON.parse(result.stdout).filter(a => a.Name === readdirSync(catalog)[0].replace('.imageset', ''));
      assert.deepEqual(assets.map(a => a.Scale).sort(), platform === 'macos' ? [1, 2] : [1, 2, 3]);
    }
    writeFileSync(resolve(dir, 'assets/two.svg'), svg.replace('#ff0000', '#00ff00'));
    assert.notDeepEqual(appleAssetCatalogInventory(app, 'macos'), inventory);
    assert.throws(() => appleAssets(app, dir, { platform: 'macos', icons: app.appleIcons, expected: inventory }), /changed after the bake/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('iOS and App Clip executable names cannot replace the privacy manifest, ignoring case', () => {
  for (const name of ['PrivacyInfo.xcprivacy', 'privacyinfo.xcprivacy', 'PRIVACYINFO.XCPRIVACY']) {
    for (const key of ['app.name', 'host.ios.appClip.name']) {
      assert.throws(() => iosExecutableRefusal({ host: {} }, name, key), error => {
        assert.ok(error.message.includes(`${key} "${name}"`));
        assert.ok(error.message.includes('"PrivacyInfo.xcprivacy"'));
        return true;
      });
    }
  }
});

test('Mac resource trees cannot replace the generated privacy manifest', () => {
  for (const to of ['Resources/PrivacyInfo.xcprivacy', 'Resources/PrivacyInfo.xcprivacy/data']) {
    assert.throws(() => nativeResourceMappings({ host: { macos: { resources: [{ from: 'shipped', to }] } } }), /host\.macos\.resources\.to/);
  }
});

test.skipIf(process.platform !== 'darwin')('the host receives the catalogue launch components without resolving an asset', () => {
  useXcode();
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-launch-components-'));
  try {
    const app = manifest => ({ dir, name: 'fixture', manifest });
    const keys = iosAssets(app({ background_color: '#abc', color_scheme_dark: { background_color: '#12345680' } }), dir, false);
    assert.deepEqual(keys.ExactLaunchColor, { light: [170 / 255, 187 / 255, 204 / 255, 1], dark: [18 / 255, 52 / 255, 86 / 255, 0.502] });
    const lightXML = '<array><real>0.6666666666666666</real><real>0.7333333333333333</real><real>0.8</real><integer>1</integer></array>';
    const darkXML = '<real>0.07058823529411765</real><real>0.20392156862745098</real><real>0.33725490196078434</real>';
    const launchXML = value => plistFile(value).split('<key>ExactLaunchColor</key>')[1];
    assert.equal(launchXML(keys), `<dict>\n    <key>light</key>${lightXML}\n    <key>dark</key><array>${darkXML}<real>0.502</real></array>\n  </dict>\n</dict></plist>\n`);
    const opaque = iosAssets(app({ background_color: '#abc', color_scheme_dark: { background_color: '#123456' } }), dir, false);
    assert.equal(launchXML(opaque), `<dict>\n    <key>light</key>${lightXML}\n    <key>dark</key><array>${darkXML}<integer>1</integer></array>\n  </dict>\n</dict></plist>\n`);
    const white = iosAssets(app({ background_color: '#ffffff' }), dir, false);
    assert.equal(launchXML(white), '<dict>\n    <key>light</key><array><integer>1</integer><integer>1</integer><integer>1</integer><integer>1</integer></array>\n  </dict>\n</dict></plist>\n');
    assert.match(keys.UILaunchScreen.UIColorName, /^ExactLaunch-[0-9a-f]{8}$/);
    assert.deepEqual(iosAssets(app({ background_color: '#01020300' }), dir, false).ExactLaunchColor, { light: [1 / 255, 2 / 255, 3 / 255, 0] });
    assert.deepEqual(iosAssets(app({}), dir, false, { launch: 'grouped' }).ExactLaunchColor,
      { light: [242 / 255, 242 / 255, 247 / 255, 1], dark: [0, 0, 0, 1] });
    assert.equal(iosAssets(app({}), dir, false).ExactLaunchColor, undefined);
    const kept = { dir: resolve(dir, 'kept'), stamp: 'fixture' };
    const manifest = { background_color: '#abc' };
    const first = iosAssets(app(manifest), dir, false, { kept });
    assert.deepEqual(iosAssets(app(manifest), dir, false, { kept }).ExactLaunchColor, first.ExactLaunchColor);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('a resource directory under a root Exact owns is refused by name, with another to use', () => {
  const refused = (from) => () => nativeResourceMappings({ host: { ios: { resources: [{ from, to: 'Resources/x' }] } } }, 'ios');
  assert.throws(refused('data'), /^Error: host\.ios\.resources\.from "data" is under `data\/`, a directory Exact owns \(each of `assets\/`, .*`dist\/` holds sources, assets or build output\); put the files in a directory of their own, such as `data-files\/`$/);
  assert.throws(refused('assets/raw'), /"assets\/raw" is under `assets\/`[^]*such as `assets-files\/`$/);
  assert.throws(refused('../outside'), /must name a separate app-relative resource directory/);
  assert.deepEqual(nativeResourceMappings({ host: { ios: { resources: [{ from: 'data-files', to: 'Resources/data-files' }] } } }, 'ios'), [{ from: 'data-files', to: 'Resources/data-files' }]);
  // The schema's description and the shipped-data recipe list the same roots (LLP 1168.000 D7.1).
  const schema = JSON.parse(readFileSync(new URL('../../scripts/app.schema.json', import.meta.url), 'utf8')).properties.host.properties.ios.properties.resources.description;
  const recipe = readFileSync(new URL('../../docs/recipes/shipped-data.md', import.meta.url), 'utf8');
  for (const root of RESERVED_RESOURCE_ROOTS) for (const [name, text] of [['app.schema.json', schema], ['shipped-data.md', recipe]])
    assert.ok(text.includes(`\`${root}/\``), `${name} names \`${root}/\``);
});

test.skipIf(process.platform !== 'darwin')('alternate icons are the declared roster: generated, a project and a catalog set, each in Assets.car and in its order', () => {
  useXcode();
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-alternate-icons-'));
  try {
    const square = (src, colour, hole = false) => {
      const data = new Uint8Array(1024 * 1024 * 4);
      for (let i = 0; i < data.length; i += 4) data.set(hole && (i / 4) % 1024 < 512 ? [0, 0, 0, 0] : colour, i);
      mkdirSync(resolve(dir, src, '..'), { recursive: true });
      writeFileSync(resolve(dir, src), encodePng({ width: 1024, height: 1024, data }));
    };
    square('assets/icon.png', [200, 40, 40, 255]);
    square('assets/paper.png', [240, 235, 220, 255]);
    square('assets/paper-dark.png', [40, 40, 40, 255]);
    square('assets/paper-glyph.png', [0, 0, 0, 255], true);
    cpSync(new URL('../../scripts/fixtures/Primary.icon', import.meta.url).pathname, resolve(dir, 'art/Night.icon'), { recursive: true });
    const catalog = resolve(dir, 'modules/apple/Assets.xcassets');
    const set = (name) => {
      mkdirSync(resolve(catalog, `${name}.appiconset`), { recursive: true });
      cpSync(resolve(dir, 'assets/paper-dark.png'), resolve(catalog, `${name}.appiconset/icon.png`));
      writeFileSync(resolve(catalog, `${name}.appiconset/Contents.json`), JSON.stringify({ images: [{ filename: 'icon.png', idiom: 'universal', platform: 'ios', size: '1024x1024' }], info: { author: 'x', version: 1 } }));
    };
    set('Card');
    const paper = { name: 'Paper', icons: [{ src: 'assets/paper.png' }, { src: 'assets/paper-glyph.png', purpose: 'monochrome' }], color_scheme_dark: { icons: [{ src: 'assets/paper-dark.png' }] } };
    const night = { name: 'Night', icons: [{ src: 'assets/paper.png' }], host: { ios: { icon: 'art/Night.icon' } } };
    const card = { name: 'Card', icons: [{ src: 'assets/paper.png' }] };
    const app = (alternate_icons, icons = [{ src: 'assets/icon.png' }]) => ({ dir, name: 'Alternates', manifest: { icons, alternate_icons } });
    const keys = iosAssets(app([paper, night, card]), dir, false);
    assert.deepEqual(keys.ExactAlternateIcons, ['Paper', 'Night', 'Card']);
    for (const idiom of ['CFBundleIcons', 'CFBundleIcons~ipad']) assert.deepEqual(Object.keys(keys[idiom].CFBundleAlternateIcons).sort(), ['Card', 'Night', 'Paper']);
    const info = JSON.parse(spawnSync('xcrun', ['assetutil', '--info', resolve(dir, 'Assets.car')], { encoding: 'utf8' }).stdout);
    for (const name of ['AppIcon', 'Paper', 'Night', 'Card']) assert.ok(info.some(asset => asset.Name === name), name);
    // The generated alternate has its dark and tinted appearances, as the primary does.
    assert.deepEqual([...new Set(info.filter(asset => asset.Name === 'Paper').map(asset => asset.Appearance ?? 'any'))].sort(), ['ISAppearanceTintable', 'UIAppearanceDark', 'any']);
    // The project is in the inventory the receipt records, under the alternate's name.
    assert.deepEqual(appleAssetCatalogInventory(app([paper, night, card]), 'ios').filter(entry => entry.alternate).map(entry => [entry.alternate, entry.path]), [['Night', 'art/Night.icon']]);
    assert.deepEqual(appleAssetCatalogInventory(app([paper, night, card]), 'tvos').filter(entry => entry.alternate), []);
    // What the build refuses, before it compiles.
    const refused = (manifest, platform = 'ios') => appleRefusals(manifest, platform).join('\n');
    assert.match(refused(app([paper, night])), /modules\/apple\/Assets\.xcassets\/Card\.appiconset is an app icon that no alternate_icons entry names; declare it there \(\{ "name": "Card"/);
    assert.equal(refused(app([paper, night]), 'tvos'), '');
    assert.match(refused(app([card, night, { ...paper, name: 'card' }])), /alternate_icons\[2\]\.name "card" is also the name of modules\/apple\/Assets\.xcassets\/Card\.appiconset/);
    set('Night');
    assert.match(refused(app([paper, night, card])), /alternate_icons\[1\]\.host\.ios\.icon: art\/Night\.icon conflicts with modules\/apple\/Assets\.xcassets\/Night\.appiconset; select one icon for the alternate/);
    rmSync(resolve(catalog, 'Night.appiconset'), { recursive: true });
    assert.match(refused(app([card, { name: 'Small', icons: [{ src: 'assets/paper-glyph.png' }] }])), /alternate_icons\[1\]\.icons\[0\]: assets\/paper-glyph\.png has transparent pixels, and the iOS icon must be opaque/);
    assert.match(refused(app([card, paper], [])), /alternate icons require a primary/);
    assert.equal(refused(app([card, paper, night])), '');
    // A name actool did not compile is refused, not shipped: a catalog set it cannot read.
    writeFileSync(resolve(catalog, 'Card.appiconset/Contents.json'), JSON.stringify({ images: [{ filename: 'missing.png', idiom: 'universal', platform: 'ios', size: '1024x1024' }], info: { author: 'x', version: 1 } }));
    assert.throws(() => iosAssets(app([paper, night, card]), dir, false), /did not compile the alternate icon Card/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120_000);

test("the portable compiler's fallback for a project is the icons' own square PNG, its size read from the file", () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-composer-fallback-'));
  try {
    const square = (src, side) => { mkdirSync(resolve(dir, 'assets'), { recursive: true }); writeFileSync(resolve(dir, src), encodePng({ width: side, height: side, data: new Uint8Array(side * side * 4).fill(255) })); };
    square('assets/paper.png', 1024);
    square('assets/glyph.png', 1024);
    square('assets/small.png', 256);
    // No `sizes` written, as an alternate's icons may be (LLP 1163 D1).
    const [paper] = alternateIcons({ alternate_icons: [{ name: 'Paper', icons: [{ src: 'assets/glyph.png', purpose: 'monochrome' }, { src: 'assets/paper.png' }], host: { ios: { icon: 'Paper.icon' } } }] });
    assert.equal(composerFallback(paper.manifest, dir), resolve(dir, 'assets/paper.png'));
    assert.equal(composerFallback({ icons: [{ src: 'assets/glyph.png', purpose: 'monochrome' }] }, dir), null);
    assert.throws(() => composerFallback({ icons: [{ src: 'assets/small.png' }] }, dir), /256×256 px, and the older-iOS fallback needs at least 512 px/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
