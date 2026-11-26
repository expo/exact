// The widget core every app of a Swift scratch shares (widgets.mjs `sharedWidgetCore`, `placeCoreFramework`).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { assertAppleIdentity, entitlements, moduleTripleFor } from './build.mjs';
import { nativeExtras } from './native-extras.mjs';
import { copyWidgetFonts, widgetExtensionInfo } from './widgets.mjs';

test('apps of one scratch share the widget core: another archive relinks it, leaves its modules, and each copy names its app', async () => {
  if (process.platform !== 'darwin') return;
  const { sharedWidgetCore, placeCoreFramework } = await import('./widgets.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-core-shared-'));
  const run = (command, args) => { const r = spawnSync(command, args, { encoding: 'utf8' }); assert.equal(r.status, 0, r.stderr); return r; };
  try {
    const bin = resolve(dir, 'bin'), base = resolve(dir, 'core'), source = resolve(dir, 'core.c');
    const triple = moduleTripleFor({ manifest: {} }, true);
    const prepared = { base, args: [], triple, platform: 'ios', device: false, framework: resolve(base, 'ExactCore.framework'), xcframework: resolve(base, 'ExactCore.xcframework') };
    const runApple = (command, args) => {
      if (command !== 'swift') return run(command, args);
      if (args.includes('--show-bin-path')) return { stdout: bin };
      if (args.includes('ExactCore')) run('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-target', triple, '-dynamiclib', source, '-o', resolve(bin, 'libExactCore.dylib')]);
      return { stdout: '' };
    };
    for (const name of ['ExactCore', 'ExactSnapshot']) {
      mkdirSync(resolve(bin, 'Modules', `${name}.swiftmodule`), { recursive: true });
      writeFileSync(resolve(bin, 'Modules', `${name}.swiftmodule`, 'arm64-apple-ios-simulator.swiftmodule'), `${name} module`);
    }
    const module = resolve(base, 'ExactCore.xcframework/ios-arm64-simulator/ExactCore.framework/Modules/ExactCore.swiftmodule/arm64-apple-ios-simulator.swiftmodule');
    let first;
    for (const [id, value] of [['com.exact.first', 1], ['com.exact.second', 2]]) {
      const compatibilityId = String(value).repeat(32);
      const identity = JSON.stringify({ id: compatibilityId, inputs: { app: id } });
      writeFileSync(source, `int exact_value(void) { return ${value}; } const char *exact_identity(void) { return ${JSON.stringify(identity)}; }`);
      writeFileSync(resolve(dir, 'libfixture.a'), `archive ${value}`);
      const app = { id, manifest: {} };
      sharedWidgetCore({ app, prepared, libDir: dir, crate: 'fixture', env: process.env, runApple });
      const { ino, mtimeMs } = statSync(module);
      first ??= { ino, mtimeMs };
      assert.deepEqual({ ino, mtimeMs }, first, 'an unchanged module is not rewritten');
      const slice = resolve(base, 'ExactCore.xcframework/ios-arm64-simulator/ExactCore.framework/ExactCore');
      assert.deepEqual(readFileSync(slice), readFileSync(resolve(prepared.framework, 'ExactCore')), 'the slice links this archive');
      const bundle = resolve(dir, `${id}.app`), frameworks = resolve(bundle, 'Frameworks');
      const copy = placeCoreFramework(prepared.framework, resolve(frameworks, 'ExactCore.framework'), app);
      const info = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', resolve(copy, 'Info.plist')]).stdout);
      assert.equal(info.CFBundleIdentifier, `${id}.runtime`);
      const executable = resolve(bundle, 'Fixture (Preview)'), consumer = resolve(dir, 'app.c');
      writeFileSync(consumer, 'extern int exact_value(void); int main(void) { return exact_value(); }');
      run('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-target', triple, consumer, '-F', frameworks, '-framework', 'ExactCore', '-o', executable]);
      assert.equal(assertAppleIdentity(app, executable, compatibilityId, resolve(copy, 'ExactCore')), id);
      assert.equal(assertAppleIdentity(app, executable, compatibilityId), id);
      assert.throws(() => assertAppleIdentity({ ...app, id: 'com.exact.wrong' }, executable), /embedded app identity/);
      assert.throws(() => assertAppleIdentity(app, executable, '0'.repeat(32)), /embedded app identity/);
    }
    assert.notEqual(JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', resolve(prepared.framework, 'Info.plist')]).stdout).CFBundleIdentifier, 'com.exact.second.runtime', 'the shared framework names no app');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('widget entitlements omit associated-domains and widget packaging bundles app fonts (LLP 1156.008.000 D6.2, D6.8)', () => {
  const manifest = {
    app: { id: 'com.exact.demo', name: 'Demo' },
    host: { ios: { associatedDomains: ['applinks:demo.example.com'] } },
    widgets: [{ kind: 'card', component: 'Card', props: 'P', families: ['systemSmall'], refresh: 'r', preview: {} }],
  };
  const appPlist = entitlements({ id: 'com.exact.demo', manifest }, 'TEAM123', true);
  assert.match(appPlist, /com\.apple\.developer\.associated-domains/);
  assert.match(appPlist, /applinks:demo\.example\.com/);
  const widgetPlist = entitlements({ id: 'com.exact.demo.widgets', widgetAppId: 'com.exact.demo', manifest }, 'TEAM123', true);
  assert.doesNotMatch(widgetPlist, /com\.apple\.developer\.associated-domains/);
  assert.doesNotMatch(widgetPlist, /applinks:/);

  const dir = mkdtempSync(resolve(tmpdir(), 'exact-widget-fonts-'));
  try {
    const res = resolve(dir, 'app-res'), ext = resolve(dir, 'ext-res');
    mkdirSync(resolve(res, 'assets/fonts'), { recursive: true });
    writeFileSync(resolve(res, 'assets/fonts/Brand.ttf'), 'font-bytes');
    writeFileSync(resolve(res, 'assets/logo.png'), 'not-a-font');
    const fonts = copyWidgetFonts(res, ext);
    assert.deepEqual(fonts, ['assets/fonts/Brand.ttf']);
    assert.equal(readFileSync(resolve(ext, 'assets/fonts/Brand.ttf'), 'utf8'), 'font-bytes');
    const iosInfo = widgetExtensionInfo({ id: 'com.exact.demo', displayName: 'Demo', dir, manifest }, { CFBundleVersion: '1', CFBundleShortVersionString: '1.0', MinimumOSVersion: '17.0', UIDeviceFamily: [1], CFBundleSupportedPlatforms: ['iPhoneOS'] }, 'ios', fonts);
    assert.match(iosInfo, /<key>UIAppFonts<\/key>/);
    assert.match(iosInfo, /<string>assets\/fonts\/Brand\.ttf<\/string>/);
    const macInfo = widgetExtensionInfo({ id: 'com.exact.demo', displayName: 'Demo', dir, manifest }, { CFBundleVersion: '1', CFBundleShortVersionString: '1.0', LSMinimumSystemVersion: '14.0' }, 'macos', fonts);
    assert.match(macInfo, /<key>ATSApplicationFontsPath<\/key>/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('nativeExtras hashes app.forma rather than retired app.tamber (LLP 1156.008.000 D6.1)', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-native-extras-forma-'));
  try {
    mkdirSync(resolve(dir, 'modules/apple'), { recursive: true });
    const mod = resolve(dir, 'modules/apple/Mod.swift');
    writeFileSync(mod, '// module\n');
    const app = { dir, manifest: {}, modulesFor: () => ({ apple: [mod], frameworks: [], tags: [] }) };
    writeFileSync(resolve(dir, 'app.forma'), 'color brand = #112233\n');
    const first = nativeExtras(app, 'ios', [], []);
    writeFileSync(resolve(dir, 'app.forma'), 'color brand = #445566\n');
    const second = nativeExtras(app, 'ios', [], []);
    assert.notEqual(first, second);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
