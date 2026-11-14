// What a package's Apple implementation adds to Info.plist: `"plistKeys"` against the allow-list, `"plist"`
// dictionaries, and the merge into `infoPlist` and `macInfoPlist`.
import { test, spyOn } from 'bun:test';
import * as childProcess from 'node:child_process';
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { PLIST_KEYS, appleMetadataProblem, modulePlist, packageKeys } from './plist.mjs';
import { entitlements, infoPlist, macInfoPlist } from './build.mjs';
import { buildPainted } from '../apple-painted/build.mjs';

const packages = resolve(import.meta.dir, '../../packages');
const purchases = JSON.parse(readFileSync(resolve(packages, 'purchases/apple/exact.json'), 'utf8'));
const impl = (plistKeys) => [{ extension: 'purchases', plistKeys }];
const flat = (plist) => plist.replace(/>\s+</g, '><');
const KEY = /<key>SKIncludeConsumableInAppPurchaseHistory<\/key><true\/>/;

test('location metadata advice chooses one grant and keeps prerequisites beside it', () => {
  const key = 'NSLocationWhenInUseUsageDescription';
  for (const platform of ['ios', 'macos']) {
    const path = `host.${platform}.infoPlist.${key}`;
    const background = platform === 'macos'
      ? ' or write `device.geolocation-background purpose.<key>` beside `device.geolocation purpose.<key>`' : '';
    assert.equal(appleMetadataProblem(`${path}: not a known key`),
      `${path}: refused; choose one: write \`device.geolocation purpose.<key>\` or write \`device.geolocation-approximate purpose.<key>\`${background}`);
  }
  const path = 'host.ios.infoPlist.NSLocationAlwaysAndWhenInUseUsageDescription';
  assert.equal(appleMetadataProblem(`${path}: not a known key`),
    `${path}: refused; write \`device.geolocation-background purpose.<key>\` beside \`device.geolocation purpose.<key>\``);
});

test('metadata refusals reserve specific advice for proven grants', () => {
  const refusal = (platform, kind, key) => appleMetadataProblem(`host.${platform}.${kind}.${key}: not a known key`);
  const generic = [
    ['ios', 'entitlements', 'com.apple.developer.associated-domains'],
    ['macos', 'entitlements', 'com.apple.developer.associated-domains'],
    ['macos', 'entitlements', 'com.apple.application-identifier'],
    ['macos', 'entitlements', 'com.apple.developer.team-identifier'],
    ['ios', 'entitlements', 'keychain-access-groups'],
    ...['ios', 'macos'].map(p => [p, 'entitlements', 'com.apple.security.application-groups']),
    ...['ExactAppGroup', 'ExactAppIdentifier', 'ExactWidgetKinds', 'ExactWidgetTypeScript', 'ExactWidgetGeneration', 'ExactKeychainAccessGroup'].map(k => ['ios', 'infoPlist', k]),
    ...['allow-jit', 'allow-unsigned-executable-memory', 'disable-library-validation'].map(k => ['macos', 'entitlements', `com.apple.security.cs.${k}`]),
    ['ios', 'infoPlist', 'ExactCapturesInBackground'],
    ['ios', 'infoPlist', 'UIBackgroundModes'],
    ['macos', 'infoPlist', 'ExactProtocolHandlers'],
    ['ios', 'infoPlist', 'NSLocationTemporaryUsageDescriptionDictionary'],
  ];
  const failures = generic.filter(([platform, kind, key]) => !/refused; Exact writes this key itself from its declarations.*docs\/reference.md.*cannot be set here/.test(refusal(platform, kind, key)));
  assert.deepEqual(failures, []);
  for (const [key, line] of [
    ['aps-environment', 'device.push` beside `device.notifications purpose.<key>'],
    ['com.apple.developer.applesignin', 'auth.apple <client id>'],
  ]) assert.ok(refusal('ios', 'entitlements', key).includes(line));
  for (const key of ['ExactAudioSession', 'NSSupportsLiveActivities', 'NSSupportsLiveActivitiesFrequentUpdates'])
    assert.match(refusal('macos', 'infoPlist', key), /feedback note/);
  assert.match(refusal('ios', 'infoPlist', 'vendor.key'), /feedback note.*vendor.key/);
});

test.skipIf(process.platform !== 'darwin')('the painted Café bundle agrees with its plist and verifies after signing', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-painted-sign-'));
  const name = dir.split('/').at(-1), root = resolve(import.meta.dir, '../..');
  const app = { id: 'dev.exact.painted', displayName: 'Café', name, dir, workspace: dir, manifest: {}, crate: () => name };
  const output = resolve(root, 'target/apple-painted', name);
  const binary = resolve(root, 'target/host-dev', `${name}-painted`);
  const spawn = childProcess.spawnSync;
  const compiler = spyOn(childProcess, 'spawnSync').mockImplementation((command, args, options) => {
    assert.equal(command, 'cargo');
    compiler.mockRestore();
    assert.equal(args[args.indexOf('--bin') + 1], `${name}-painted`);
    mkdirSync(resolve(root, 'target/host-dev'), { recursive: true });
    writeFileSync(binary, readFileSync('/usr/bin/true'), { mode: 0o755 });
    return { status: 0 };
  });
  try {
    buildPainted(app, [], { macInfoPlist });
    const bundle = resolve(output, 'host-dev/Cafe\u0301 benchmark carrier.app');
    assert.deepEqual(readdirSync(resolve(output, 'host-dev')), ['Cafe\u0301 benchmark carrier.app']);
    assert.deepEqual(readdirSync(resolve(bundle, 'Contents/MacOS')), ['Cafe\u0301']);
    const info = spawn('plutil', ['-convert', 'json', '-o', '-', resolve(bundle, 'Contents/Info.plist')], { encoding: 'utf8' });
    assert.equal(info.status, 0, info.stderr);
    assert.equal(JSON.parse(info.stdout).CFBundleExecutable, readdirSync(resolve(bundle, 'Contents/MacOS'))[0]);
    const verified = spawn('codesign', ['--verify', '--deep', '--strict', bundle], { encoding: 'utf8' });
    assert.equal(verified.status, 0, verified.stderr);
  } finally {
    compiler.mockRestore();
    for (const path of [dir, output, binary]) rmSync(path, { recursive: true, force: true });
  }
});

/** An app in a temporary directory whose `app.ts` is `source`, with `names` (directories under packages/) linked
 * into its node_modules, or copied when `edit` changes their Apple `exact.json`. */
function withApp(source, names, run, edit = null) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-plist-keys-'));
  try {
    mkdirSync(resolve(dir, 'node_modules/@exact'), { recursive: true });
    for (const name of names) {
      const at = resolve(dir, 'node_modules/@exact', name);
      if (!edit) { symlinkSync(resolve(packages, name), at, 'dir'); continue; }
      cpSync(resolve(packages, name), at, { recursive: true });
      const json = resolve(at, 'apple/exact.json');
      writeFileSync(json, JSON.stringify(edit(JSON.parse(readFileSync(json, 'utf8')))));
    }
    writeFileSync(resolve(dir, 'app.ts'), source);
    mkdirSync(resolve(dir, 'strings'));
    writeFileSync(resolve(dir, 'strings/en.json'), JSON.stringify({ nav: 'Turn-by-turn directions need your precise location.' }));
    return run({ id: 'com.example.plist', displayName: 'Plist', dir, manifest: { host: {} } });
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test("purchases' key is allowed on each platform the list names", () => {
  assert.deepEqual(purchases.plistKeys, { SKIncludeConsumableInAppPurchaseHistory: true });
  for (const target of ['ios', 'macos', 'tvos']) assert.deepEqual(packageKeys(impl(purchases.plistKeys), target), [{ extension: 'purchases', key: 'SKIncludeConsumableInAppPurchaseHistory', value: true }]);
  assert.deepEqual(packageKeys([{ extension: 'purchases', plistKeys: null }], 'ios'), []);
});

test('the allow-list refuses, naming the package, the key and what is allowed', () => {
  const refuses = (plistKeys, what) => assert.throws(() => packageKeys(impl(plistKeys), 'ios'),
    (e) => e.message.includes("purchases's Apple implementation") && e.message.includes(what) && e.message.includes('SKIncludeConsumableInAppPurchaseHistory: true'));
  refuses({ NSAppTransportSecurity: { NSAllowsArbitraryLoads: true } }, 'names NSAppTransportSecurity, which is not on the list');
  refuses({ CFBundleIdentifier: 'com.example.other' }, 'names CFBundleIdentifier');
  refuses({ SKIncludeConsumableInAppPurchaseHistory: false }, 'gives SKIncludeConsumableInAppPurchaseHistory = false');
  refuses({ SKIncludeConsumableInAppPurchaseHistory: 'YES' }, 'gives SKIncludeConsumableInAppPurchaseHistory = "YES"');
  refuses([], 'is not an object');
});

test('a package key that collides with a key the build writes is refused, naming the package', () => {
  const app = { id: 'com.example.plist', displayName: 'Plist', manifest: { host: {} } };
  const collides = (reach) => {
    assert.throws(() => infoPlist(app, false, { reach }), /geolocation's Apple implementation declares the Info.plist key CFBundleIdentifier, which the build writes/);
    assert.throws(() => macInfoPlist(app, { reach }), /geolocation's Apple implementation declares the Info.plist key CFBundleIdentifier/);
  };
  collides({ packages: [{ extension: 'geolocation', key: 'CFBundleIdentifier', value: {} }] });
  // A listed key the build also wrote would be refused the same way.
  PLIST_KEYS.set('UILaunchScreen', { value: true, platforms: ['ios'] });
  try {
    const reach = { packages: packageKeys([{ extension: 'purchases', plistKeys: { UILaunchScreen: true } }], 'ios') };
    assert.throws(() => infoPlist(app, false, { reach }), /purchases's Apple implementation declares the Info.plist key UILaunchScreen, which the build writes/);
  } finally { PLIST_KEYS.delete('UILaunchScreen'); }
});

test('an app that grants no device still gets its packages\' keys and dictionaries, and only from packages it uses', () => {
  withApp('import { purchase } from "@exact/purchases";\n', ['purchases'], (app) => {
    for (const plist of [infoPlist(app, false, { reach: modulePlist(app, null, 'ios', { purchases: '@exact/purchases' }) }), macInfoPlist(app, { reach: modulePlist(app, null, 'macos', { purchases: '@exact/purchases' }) })]) {
      assert.match(flat(plist), KEY);
      assert.doesNotMatch(plist, /UsageDescription|CFBundleLocalizations/);
    }
    // Purchases has no tvOS implementation.
    assert.doesNotMatch(infoPlist(app, false, { reach: modulePlist(app, null, 'tvos', { purchases: '@exact/purchases' }), tv: true }), /SKInclude/);
  });
  withApp('export const nothing = true;\n', ['purchases'], (app) => assert.doesNotMatch(infoPlist(app, false, { reach: modulePlist(app, null, 'ios', {}) }), /SKInclude/));
  withApp('import { requestPreciseLocation } from "@exact/geolocation";\nrequestPreciseLocation({ purposeKey: "nav" });\n', ['geolocation'], (app) => {
    assert.match(flat(infoPlist(app, false, { reach: modulePlist(app, null, 'ios', { geolocation: '@exact/geolocation' }) })),
      /<key>NSLocationTemporaryUsageDescriptionDictionary<\/key><dict><key>nav<\/key><string>Turn-by-turn directions need your precise location.<\/string><\/dict>/);
    writeFileSync(resolve(app.dir, 'strings/en.json'), JSON.stringify({ nav: '  ' }));
    assert.throws(() => modulePlist(app, null, 'ios', { geolocation: '@exact/geolocation' }), /names "nav", which strings\/en.json lacks or leaves blank/);
  });
});

test('a literal purpose key in a loaded Contract library reaches the plist', () => {
  withApp('', ['geolocation'], (app) => {
    const lib = resolve(app.dir, 'node_modules/@repro/lib');
    mkdirSync(lib, { recursive: true });
    writeFileSync(resolve(lib, 'package.json'), JSON.stringify({ name: '@repro/lib', version: '1.0.0', exports: { '.': './index.contract' }, dependencies: { '@exact/geolocation': '1.0.0' } }));
    writeFileSync(resolve(app.dir, 'package.json'), JSON.stringify({ dependencies: { '@exact/geolocation': '1.0.0', '@repro/lib': '1.0.0' } }));
    writeFileSync(resolve(lib, 'index.contract'), 'use requestPreciseLocation, GeolocationStatus from "@exact/geolocation"\ncomponent Locate\n  mutation precise as shape GeolocationStatus\n  action ask\n    send precise = requestPreciseLocation(purposeKey="nav")\n  view\n    button press=ask\n      text "Locate"\n');
    writeFileSync(resolve(app.dir, 'app.contract'), 'use Locate from "@repro/lib"\n');
    const compiler = resolve(import.meta.dir, '../../target/debug/contract');
    const run = childProcess.spawnSync(compiler, ['sources', resolve(app.dir, 'app.contract')], { encoding: 'utf8' });
    assert.equal(run.status, 0, run.stderr);
    const compiled = childProcess.spawnSync(compiler, ['build', resolve(app.dir, 'app.contract'), '-o', resolve(app.dir, 'app.plan')], { encoding: 'utf8' });
    assert.equal(compiled.status, 0, compiled.stderr);
    const files = JSON.parse(run.stdout).sources.map(s => s.path);
    const reach = modulePlist(app, null, 'ios', { geolocation: '@exact/geolocation' }, files);
    assert.match(flat(infoPlist(app, false, { reach })), /<key>NSLocationTemporaryUsageDescriptionDictionary<\/key><dict><key>nav<\/key>/);
    const source = readFileSync(resolve(lib, 'index.contract'), 'utf8');
    writeFileSync(resolve(lib, 'index.contract'), source.replace('component Locate\n', 'component Locate\n  state purpose = "nav"\n').replace('purposeKey="nav"', 'purposeKey=purpose'));
    assert.doesNotMatch(infoPlist(app, false, { reach: modulePlist(app, null, 'ios', { geolocation: '@exact/geolocation' }, files) }), /NSLocationTemporaryUsageDescriptionDictionary/);
  });
});

test('a plist entry that is not "<call>.<option>" is refused, naming the package and the field', () => {
  for (const plist of [{ SKIncludeConsumableInAppPurchaseHistory: true }, { NSLocationTemporaryUsageDescriptionDictionary: 'requestPreciseLocation' }, ['x']]) {
    withApp('import { purchase } from "@exact/purchases";\n', ['purchases'], (app) => {
      assert.throws(() => modulePlist(app, null, 'ios', { purchases: '@exact/purchases' }), /purchases's Apple implementation: its plist (gives|is not an object)/);
    }, (json) => ({ ...json, plist }));
  }
});

for (const displayName of ['Plain', 'Caf\u00e9', 'Cafe\u0301', '東京', '', '... ']) {
  test.skipIf(process.platform !== 'darwin')(`Apple bundles keep ${JSON.stringify(displayName)} as their display name and verify after signing`, async () => {
    const { spawnSync } = await import('node:child_process');
    const { appleArtifacts, appClip } = await import('./build.mjs');
    const { executableName, windowsFile } = await import('../../scripts/app.mjs');
    const { executableIn, installedAt } = await import('../../scripts/exact.mjs');
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-unicode-sign-'));
    const app = { id: 'com.exact.signing', displayName, dir, target: dir, manifest: { host: { ios: { appClip: { name: displayName } } } }, crate: () => 'fixture-windows' };
    const run = (command, args) => {
      const result = spawnSync(command, args, { encoding: 'utf8' });
      assert.equal(result.status, 0, `${command}: ${result.stderr}`);
      return result.stdout;
    };
    try {
      for (const destination of ['macos', 'ios-simulator']) {
        const paths = appleArtifacts(app, { destination });
        const contents = destination === 'macos' ? resolve(paths.bundle, 'Contents') : paths.bundle;
        const binaries = destination === 'macos' ? resolve(contents, 'MacOS') : contents;
        mkdirSync(binaries, { recursive: true });
        const binary = resolve(binaries, paths.executable);
        writeFileSync(binary, readFileSync('/usr/bin/true'), { mode: 0o755 });
        const plist = resolve(contents, 'Info.plist');
        writeFileSync(plist, destination === 'macos' ? macInfoPlist(app) : infoPlist(app));
        const info = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', plist]));
        assert.equal(info.CFBundleDisplayName, displayName);
        assert.equal(info.CFBundleName, displayName);
        assert.equal(info.CFBundleExecutable, paths.executable);
        assert.equal(info.CFBundleIdentifier, app.id);
        if (destination === 'macos') {
          assert.equal(executableIn(paths.bundle, app), binary);
          assert.equal(installedAt(app).split('/').at(-1), `${paths.executable}.app`);
        }
        run('codesign', ['--force', '--sign', '-', paths.bundle]);
        run('codesign', ['--verify', '--deep', '--strict', paths.bundle]);
        const seal = run('plutil', ['-extract', 'files2', 'xml1', '-o', '-', resolve(contents, '_CodeSignature/CodeResources')]);
        assert.ok(!seal.includes(`<key>${destination === 'macos' ? 'MacOS/' : ''}${paths.executable}</key>`), 'the main executable is code, not a sealed resource');
        assert.equal(paths.executable, executableName(app).normalize('NFD'));
        assert.equal(appClip(app).executable, executableName({ ...app, id: app.id + '.Clip' }).normalize('NFD'));
      }
      assert.equal(windowsFile(app, 'fixture-windows.exe'), `${executableName(app)}.exe`);
      if (displayName === 'Caf\u00e9') assert.equal(executableName(app), displayName);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
}

test('module settings keep their keys escaped, and a value a property list cannot hold is refused by its path', async () => {
  const { appleRefusals, plistFile } = await import('./assets.mjs');
  const { spawnSync } = await import('node:child_process');
  const moduleConfig = { analytics: { 'q&a': 'on', 'a<b': [1, 2.5] } };
  const app = { name: 'plist', id: 'com.example.plist', displayName: 'Plist', dir: tmpdir(), manifest: { host: {}, moduleConfig } };
  for (const plist of [infoPlist(app), macInfoPlist(app)]) {
    assert.match(flat(plist), /<key>q&amp;a<\/key><string>on<\/string><key>a&lt;b<\/key><array><integer>1<\/integer><real>2.5<\/real><\/array>/);
    if (process.platform !== 'darwin') continue;
    const read = spawnSync('plutil', ['-convert', 'json', '-o', '-', '-'], { input: plist, encoding: 'utf8' });
    assert.equal(read.status, 0, read.stderr);
    assert.deepEqual(JSON.parse(read.stdout).ExactModuleConfig, moduleConfig);
  }
  assert.deepEqual(appleRefusals(app, 'macos').filter((r) => r.includes('app.json')), []);
  const refused = (dsn, path, what) => {
    assert.throws(() => plistFile({ ExactModuleConfig: { sentry: { dsn } } }),
      { message: `plist: ExactModuleConfig.sentry.${path} is ${what}, which a property list cannot hold` });
    assert.ok(appleRefusals({ ...app, manifest: { host: {}, moduleConfig: { sentry: { dsn } } } }, 'macos')
      .includes(`plist: app.json's moduleConfig.sentry.${path} is ${what}, which a property list cannot hold`));
  };
  for (const [value, what] of [[null, 'null'], [NaN, 'NaN'], [Infinity, 'Infinity'], [1e21, '1e+21']]) refused(value, 'dsn', what);
  refused(['a', null], 'dsn[1]', 'null');
});

test('every app plist takes the version inputs, defaulting only unset variables', () => {
  const app = { id: 'com.exact.version', displayName: 'Version', manifest: {} };
  const saved = [process.env.EXACT_VERSION, process.env.EXACT_BUILD_NUMBER];
  try {
    for (const [version, build] of [[undefined, undefined], ['2.3.4', '56'], ['', '']]) {
      for (const [name, value] of [['EXACT_VERSION', version], ['EXACT_BUILD_NUMBER', build]]) {
        if (value === undefined) delete process.env[name]; else process.env[name] = value;
      }
      for (const plist of [infoPlist(app), infoPlist(app, true), infoPlist(app, false, { tv: true }), infoPlist(app, false, { development: { scheme: 'exact-version-dev', origins: ['http://localhost:7777'], token: 'fixture' } }), macInfoPlist(app)]) {
        assert.ok(flat(plist).includes(`<key>CFBundleShortVersionString</key><string>${version ?? '0.1.0'}</string>`));
        assert.ok(flat(plist).includes(`<key>CFBundleVersion</key><string>${build ?? '1'}</string>`));
      }
    }
  } finally {
    for (const [i, name] of ['EXACT_VERSION', 'EXACT_BUILD_NUMBER'].entries()) {
      if (saved[i] === undefined) delete process.env[name]; else process.env[name] = saved[i];
    }
  }
});
