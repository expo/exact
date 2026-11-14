import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { appleAssets, appleAssetCatalogInventory, rasterIconAssets, iosAssets, iosExecutableRefusal, nativeResourceMappings, plistFile, RESERVED_RESOURCE_ROOTS } from './assets.mjs';
import { pngPixels } from './image.mjs';
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
