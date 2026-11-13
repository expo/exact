import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { iosAssets, iosExecutableRefusal, nativeResourceMappings, plistFile } from './assets.mjs';
import { useXcode } from './devices.mjs';

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
