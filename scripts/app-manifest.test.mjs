// app.json keys the web manifest standard defines (scripts/app.schema.json).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';

test('launch modules must provide a part for every declared host', async () => {
  const { buildBake, readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-launch-hosts-'));
  const read = (host, launch = ['observe']) => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Launch', app: { id: 'dev.exact.launch', name: 'Launch' }, host, launch }));
    return readManifest(dir, 'launch');
  };
  try {
    // An app's module shadows the whole SDK module, including missing parts.
    const own = resolve(dir, 'modules/observe');
    mkdirSync(resolve(own, 'linux'), { recursive: true });
    writeFileSync(resolve(own, 'linux/launch.rs'), 'pub fn launch() {}');
    assert.doesNotThrow(() => read({ linux: {} }));
    const selected = { dir, manifest: { launch: ['observe'] } };
    assert.throws(() => buildBake(selected, 'linux', 'aarch64-linux-android', { kind: 'android' }), /observe has no android launch part/);
    assert.throws(() => buildBake(selected, 'windows', 'x86_64-pc-windows-msvc'), /observe has no windows launch part/);
    for (const host of ['android', 'ios', 'macos', 'web', 'windows']) {
      assert.throws(() => read({ linux: {}, [host]: {} }), new RegExp(`app.json launch: observe has no ${host} launch part`));
    }
    rmSync(own, { recursive: true });
    assert.doesNotThrow(() => read({ android: {}, ios: {}, macos: {}, linux: {}, web: {} }));
    assert.throws(() => read({ windows: {} }), /observe has no windows launch part/);
    assert.throws(() => read({ android: {} }, ['missing']), /app.json launch: no module missing/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the native intents fixture declares only hosts its launch module supports', async () => {
  const { resolveApp } = await import('./app.mjs');
  const app = resolveApp(resolve(import.meta.dir, '../host/apple/fixtures/native-intents-app'));
  assert.deepEqual(Object.keys(app.manifest.host).sort(), ['ios', 'macos']);
});

test('Apple Swift products require a URL, exact semantic version and named products', async () => {
  const { readManifest } = await import('./app.mjs');
  const { swiftPackageGraph, assertSwiftLock } = await import('../host/apple/compose.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-swift-products-'));
  const dependency = { url: 'https://github.com/apple/swift-collections.git', exactVersion: '1.1.4', products: ['DequeModule'] };
  const bound = value => [{ extension: 'queue', swiftPackagesSource: '/packages/queue/apple/exact.json', swiftPackages: value }];
  const app = value => ({ dir, manifest: { host: { macos: { swiftPackages: value } } } });
  const read = value => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Swift', app: { id: 'dev.exact.swift', name: 'Swift' },
      host: { macos: { swiftPackages: value }, ios: { swiftPackages: value } } }));
    return readManifest(dir, 'swift');
  };
  try {
    assert.deepEqual(read([dependency]).host.ios.swiftPackages, [dependency]);
    for (const value of [null, {}, [null], [{ ...dependency, exactVersion: '^1.1.4' }], [{ ...dependency, exactVersion: '1.1' }],
      [{ ...dependency, url: 'file:///tmp/package' }], [{ ...dependency, products: [] }], [{ ...dependency, products: [12] }],
      [{ ...dependency, products: [''] }], [{ ...dependency, branch: 'main' }], [{ url: dependency.url }]]) {
      assert.throws(() => read(value), /swiftPackages/);
      assert.throws(() => swiftPackageGraph(app(undefined), bound(value)), /queue.*exact.json.*swiftPackages/);
    }
    const graph = swiftPackageGraph(app([dependency]), bound([{ ...dependency, products: ['OrderedCollections', 'DequeModule'] }]));
    assert.equal(graph.length, 1);
    assert.deepEqual(graph[0].products, ['DequeModule', 'OrderedCollections']);
    assert.throws(() => swiftPackageGraph(app([dependency]), bound([{ ...dependency, exactVersion: '1.2.0' }])), /app.json.*1.1.4.*queue.*1.2.0/);
    assert.throws(() => swiftPackageGraph(app([dependency]), bound([{ ...dependency, url: 'https://example.com/swift-collections.git' }])), /app.json.*queue/);
    assert.throws(() => assertSwiftLock(app([dependency]), graph), /exact.mjs lock/);
    const lock = { version: 3, pins: [{ identity: 'swift-collections', location: dependency.url,
      state: { version: '1.1.3', revision: 'a'.repeat(40) } }] };
    writeFileSync(resolve(dir, 'Package.resolved'), JSON.stringify(lock));
    assert.throws(() => assertSwiftLock(app([dependency]), graph), /differs from the lock/);
    lock.pins[0].state.version = '1.1.4';
    writeFileSync(resolve(dir, 'Package.resolved'), JSON.stringify(lock));
    assert.ok(assertSwiftLock(app([dependency]), graph).length);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Swift lock discovery follows reachable interfaces and inherited dependencies', async () => {
  const { appSwiftPackageGraph } = await import('../host/apple/compose.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'swift-workspace-')), dir = resolve(root, 'apps/fixture');
  const put = (file, value) => {
    const path = resolve(root, file); mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, typeof value === 'string' ? value : JSON.stringify(value));
  };
  const dependency = { url: 'https://github.com/apple/swift-collections.git', exactVersion: '1.1.4', products: ['DequeModule'] };
  const implementation = { kind: 'implementation', implements: '@fixture/queue@1.0.0', platform: 'apple',
    targets: ['macos', 'ios'], sources: { swift: ['Queue.swift'] }, class: 'Queue', swiftPackages: [dependency] };
  const app = { dir, target: resolve(root, 'target'), manifest: { name: 'Queue', app: { id: 'dev.exact.queue', name: 'Queue' } } };
  try {
    put('package.json', { dependencies: { '@fixture/queue': '1.0.0', '@fixture/parent': '1.0.0' } });
    put('apps/fixture/app.json', app.manifest);
    put('apps/fixture/app.contract', 'component App\n  view\n    queue-view\n');
    put('node_modules/@fixture/queue/package.json', { name: '@fixture/queue', version: '1.0.0',
      exports: { '.': './exact.json', './apple': './apple/exact.json' } });
    put('node_modules/@fixture/queue/exact.json', { kind: 'interface', name: '@fixture/queue', names: [], views: ['queue-view'] });
    put('node_modules/@fixture/queue/apple/exact.json', implementation);
    const { lockSwiftPackages } = await import('../host/apple/compose.mjs');
    const game = { ...app, manifest: { name: 'Queue game', game: { audio: true } } };
    put('apps/fixture/app.json', game.manifest);
    assert.equal(lockSwiftPackages(game), null);
    put('apps/fixture/app.json', app.manifest);
    assert.deepEqual(appSwiftPackageGraph(app).map(p => p.identity), ['swift-collections']);
    put('node_modules/@fixture/parent/package.json', { name: '@fixture/parent', version: '1.0.0', exports: './exact.json' });
    put('node_modules/@fixture/parent/exact.json', { ...implementation, targets: ['macos'],
      swiftPackages: [{ ...dependency, products: ['OrderedCollections'] }] });
    assert.deepEqual(appSwiftPackageGraph(app).at(0).products, ['DequeModule', 'OrderedCollections']);
    put('apps/fixture/package.json', { dependencies: { '@fixture/queue': '1.0.0', '@fixture/one': '1.0.0', '@fixture/two': '1.0.0' } });
    for (const name of ['one', 'two']) {
      put(`apps/fixture/node_modules/@fixture/${name}/package.json`, { name: `@fixture/${name}`, version: '1.0.0', exports: './exact.json' });
      put(`apps/fixture/node_modules/@fixture/${name}/exact.json`, { ...implementation, targets: ['ios'] });
    }
    put('apps/fixture/app.contract', 'component App\n  view\n    text "No package views"\n');
    assert.deepEqual(appSwiftPackageGraph(app), []);
    put('node_modules/@fixture/library/package.json', { name: '@fixture/library', version: '1.0.0', exports: './index.contract' });
    put('node_modules/@fixture/library/index.contract', 'component Tile\n  view\n    queue-view\n');
    put('apps/fixture/app.contract', 'use Tile from "@fixture/library"\ncomponent App\n  view\n    text "Unused Tile"\n');
    assert.deepEqual(appSwiftPackageGraph(app), []);
    put('apps/fixture/app.contract', 'use Tile from "@fixture/library"\ncomponent App\n  view\n    Tile()\n');
    assert.throws(() => appSwiftPackageGraph(app), /one.*two.*queue.*ios/);
    put('apps/fixture/app.json', { ...app.manifest, implementations: { '@fixture/queue': { ios: '@fixture/one' } } });
    assert.deepEqual(appSwiftPackageGraph(app).map(p => p.identity), ['swift-collections']);
    put('apps/fixture/app.json', app.manifest);
    const plain = { ...implementation }; delete plain.swiftPackages;
    put('node_modules/@fixture/queue/apple/exact.json', plain);
    for (const name of ['one', 'two']) put(`apps/fixture/node_modules/@fixture/${name}/exact.json`, { ...plain, targets: ['ios'] });
    const { appleImplementations } = await import('../js/bake/src/typescript.mjs');
    const extensions = { queue: '@fixture/queue' }, mac = appleImplementations(dir, 'macos', extensions);
    assert.deepEqual(appSwiftPackageGraph(app, mac, extensions), []);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 120000);

test('Apple metadata dictionaries refuse every key together with grant or generic advice', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-apple-metadata-'));
  const save = host => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({
    name: 'Metadata', app: { id: 'dev.exact.metadata', name: 'Metadata' }, host,
  }));
  try {
    save({ ios: { infoPlist: { NSPhotoLibraryAddUsageDescription: 'Save a photo', VendorKey: true },
      entitlements: { 'aps-environment': 'development', 'com.apple.developer.applesignin': ['Default'], 'vendor.claim': true } },
      macos: { infoPlist: { NSMicrophoneUsageDescription: 'Record', LSUIElement: true }, entitlements: { 'com.apple.security.device.audio-input': true, 'com.apple.developer.aps-environment': 'development' } } });
    assert.throws(() => readManifest(dir, 'metadata'), error => {
      for (const text of [
        'host.ios.infoPlist.NSPhotoLibraryAddUsageDescription: refused; write `device.photos-add purpose.<key>`',
        'host.ios.entitlements.aps-environment: refused; write `device.push` beside `device.notifications purpose.<key>`',
        'com.apple.developer.applesignin: refused; write `auth.apple <client id>`',
        'host.macos.infoPlist.NSMicrophoneUsageDescription: refused; write `device.microphone purpose.<key>`',
        'LSUIElement: refused; Exact writes this key itself from its declarations',
        'host.macos.entitlements.com.apple.security.device.audio-input: refused; Exact writes this key itself from its declarations',
        'host.macos.entitlements.com.apple.developer.aps-environment: refused; Exact writes this key itself from its declarations',
        'feedback note', 'VendorKey', 'vendor.claim',
      ]) assert.ok(error.message.includes(text), text + '\n' + error.message);
      return true;
    });
    for (const value of [null, [], true, 'key']) {
      save({ ios: { infoPlist: value, entitlements: value }, macos: { infoPlist: value, entitlements: value } });
      assert.throws(() => readManifest(dir, 'metadata'), /expected object/);
    }
    save({ ios: { infoPlist: {}, entitlements: {} }, macos: { infoPlist: {}, entitlements: {} } });
    assert.deepEqual(readManifest(dir, 'metadata').host.ios.infoPlist, {});
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a widget declares fixed enum choices and content-index entity choices', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'widget-configuration-'));
  const fixed = { name: 'rows', title: 'Rows', type: 'enum', values: [{ value: 'one', title: '1' }], default: 'one' };
  const entity = { name: 'book', title: 'Book', type: 'entity', entity: 'book' };
  const read = parameters => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Choices', app: { id: 'dev.exact.choices', name: 'Choices' },
      intents: { entities: { book: { title: 'Book' } } }, widgets: [{
        kind: 'card', families: ['systemSmall'], component: 'Card', props: 'Props', refresh: 'refresh', preview: {},
        configuration: { title: 'Card', parameters },
      }] }));
    return readManifest(dir, 'choices').widgets[0].configuration.parameters;
  };
  try {
    assert.deepEqual(read([fixed, entity]), [fixed, entity]);
    for (const mixed of [{ ...entity, values: fixed.values }, { ...entity, default: 'one' }, { ...fixed, entity: 'book' }, { ...fixed, type: 'entity' }, { ...entity, type: 'list' }]) {
      assert.throws(() => read([mixed]), /parameters\[0\]: .* matches 0 of the allowed forms/, JSON.stringify(mixed));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('each painted host admits ink, carries it in native metadata and refuses gpu', async () => {
  const { readManifest, nativeManifestMetadata } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-painter-'));
  const base = { name: 'Painter', app: { id: 'dev.exact.painter', name: 'Painter' } };
  const read = host => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...base, host }));
    return readManifest(dir, 'painter');
  };
  try {
    for (const platform of ['windows', 'linux', 'android']) {
      const manifest = read({ [platform]: { painter: 'ink' } });
      assert.equal(nativeManifestMetadata(manifest, platform).host.painter, 'ink');
      for (const other of ['windows', 'linux', 'android'].filter(p => p !== platform)) {
        assert.equal(nativeManifestMetadata(manifest, other).host.painter, undefined);
      }
      assert.equal(nativeManifestMetadata(read({ [platform]: {} }), platform).host.painter, undefined);
      assert.throws(() => read({ [platform]: { painter: 'gpu' } }), /painter/);
      if (platform === 'windows') {
        assert.equal(nativeManifestMetadata(read({ windows: { painter: 'cpu' } }), platform).host.painter, 'cpu');
      } else {
        assert.throws(() => read({ [platform]: { painter: 'cpu' } }), /painter/);
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('readManifest takes the web manifest\'s description', async () => {
  const { readManifest } = await import('./app.mjs');
  const { mkdtempSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-description-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Tips', app: { id: 'dev.exact.tips', name: 'Tips' }, description: 'Splits a bill and its tip.' }));
    assert.equal(readManifest(dir, 'tips').description, 'Splits a bill and its tip.');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('protocol handlers require one in-app placeholder and distinct schemes', async () => {
  const { readManifest } = await import('./app.mjs');
  const { mkdtempSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-protocol-'));
  const base = { name: 'Links', app: { id: 'dev.exact.links', name: 'Links' } };
  const save = protocol_handlers => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...base, protocol_handlers }));
  try {
    save([{ protocol: 'web+links', url: '/open?u=%s' }]);
    assert.deepEqual(readManifest(dir, 'links').protocol_handlers, [{ protocol: 'web+links', url: '/open?u=%s' }]);
    for (const distribution_only of [true, false]) {
      save([{ protocol: 'web+links', url: '/open?u=%s', distribution_only }]);
      assert.equal(readManifest(dir, 'links').protocol_handlers[0].distribution_only, distribution_only);
    }
    for (const bad of [
      [{ protocol: 'web+links', url: '/open?u=%s', distribution_only: 'true' }],
      [{ protocol: 'web+links', url: '/open' }],
      [{ protocol: 'web+links', url: '/open?u=%s&again=%s' }],
      [{ protocol: 'web+links', url: '//other/open?u=%s' }],
      [{ protocol: 'web+links', url: '/open?u=%s#fragment' }],
      [{ protocol: 'web+links', url: '/open?u=%s' }, { protocol: 'WEB+LINKS', url: '/other?u=%s' }],
    ]) {
      save(bad);
      assert.throws(() => readManifest(dir, 'links'), /protocol_handlers/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Apple bundles register declared protocol schemes and keep their templates', async () => {
  const { macInfoPlist, infoPlist } = await import('../host/apple/build.mjs');
  const app = { id: 'dev.exact.links', displayName: 'Links', manifest: {
    protocol_handlers: [{ protocol: 'web+links', url: '/open?u=%s' }],
    host: { macos: { urlSchemes: ['other'] }, ios: { urlSchemes: ['other'] } },
  } };
  for (const plist of [macInfoPlist(app), infoPlist(app)]) {
    assert.match(plist, /<key>ExactProtocolHandlers<\/key>/);
    assert.match(plist, /<string>web\+links<\/string>/);
    assert.match(plist, /<string>other<\/string>/);
    assert.match(plist, /<string>\/open\?u=%s<\/string>/);
  }
});

test('distribution-only handlers stay out of both Apple registration keys in local builds', async () => {
  const { macInfoPlist, infoPlist } = await import('../host/apple/build.mjs');
  const handler = { protocol: 'packaged', url: '/packaged?u=%s', distribution_only: true };
  const app = { id: 'dev.exact.links', displayName: 'Links', manifest: {
    protocol_handlers: [handler, { protocol: 'ordinary', url: '/ordinary?u=%s', distribution_only: false }],
  } };
  const development = { scheme: 'exact-dev-test', origins: ['http://localhost:8080'], token: 'abc' };
  for (const generate of [options => macInfoPlist(app, options), options => infoPlist(app, true, options)]) {
    const local = generate({ development });
    assert.doesNotMatch(local, /packaged/);
    assert.match(local, /<string>ordinary<\/string>/);
    assert.match(local, /<string>exact-dev-test<\/string>/);
    const shipped = generate({ distributed: true });
    const compact = shipped.replace(/\s+/g, '');
    assert.match(compact, /<key>CFBundleURLSchemes<\/key><array><string>packaged<\/string><string>ordinary<\/string><\/array>/);
    assert.match(compact, /<key>ExactProtocolHandlers<\/key><array><dict><key>protocol<\/key><string>packaged<\/string><key>url<\/key><string>\/packaged\?u=%s<\/string><\/dict>/);
    // Session.swift reads [[String: String]], so a build-only Boolean cannot reach this dictionary.
    assert.doesNotMatch(shipped, /distribution_only/);
  }
  const only = { ...app, manifest: { protocol_handlers: [handler] } };
  for (const local of [macInfoPlist(only), infoPlist(only)]) assert.doesNotMatch(local, /CFBundleURLTypes|ExactProtocolHandlers/);
  assert.match(infoPlist(app, true, { distribution: { ExactBuildKind: 'release' } }), /<string>release<\/string>/);
  assert.doesNotMatch(infoPlist(app, true, { distribution: { ExactBuildKind: 'release' } }), /packaged/);
});

test('http and https come as a pair: a Mac web browser, refused on iOS and dropped from the web manifest (LLP 1030 D2)', async () => {
  const { macInfoPlist, infoPlist } = await import('../host/apple/build.mjs');
  const { readManifest, webAppManifest } = await import('./app.mjs');
  const flat = (plist) => plist.replace(/\s+/g, '');
  const pair = (extra = {}) => [{ protocol: 'https', url: '/open?u=%s', ...extra }, { protocol: 'http', url: '/open?u=%s', ...extra }];
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-web-'));
  const save = (protocol_handlers) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Links', app: { id: 'dev.exact.links', name: 'Links' }, protocol_handlers }));
  try {
    save([...pair(), { protocol: 'links', url: '/open?u=%s' }]);
    assert.equal(readManifest(dir, 'links').protocol_handlers.length, 3);
    for (const bad of [[pair()[0]], [pair()[0], { protocol: 'HTTP', url: '/other?u=%s' }], [pair()[0], { ...pair()[1], distribution_only: true }]]) {
      save(bad);
      assert.throws(() => readManifest(dir, 'links'), /http and https come together/);
    }
    for (const platform of ['ios', 'macos']) {
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Links', app: { id: 'dev.exact.links', name: 'Links' }, host: { [platform]: { urlSchemes: ['links', 'HTTPS'] } } }));
      assert.throws(() => readManifest(dir, 'links'), new RegExp(`host\\.${platform}\\.urlSchemes: HTTPS is the web's own scheme; declare http and https together in protocol_handlers`));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }

  const app = { id: 'dev.exact.links', displayName: 'Links', manifest: { protocol_handlers: [...pair(), { protocol: 'links', url: '/l?u=%s' }], host: { macos: { urlSchemes: ['other'] } } } };
  const mac = flat(macInfoPlist(app));
  assert.match(mac, /<key>CFBundleURLTypes<\/key><array><dict><key>CFBundleURLName<\/key><string>dev.exact.links<\/string><key>CFBundleURLSchemes<\/key><array><string>other<\/string><string>links<\/string><\/array><\/dict><dict><key>CFBundleURLName<\/key><string>WebsiteURL<\/string><key>CFBundleTypeRole<\/key><string>Viewer<\/string><key>CFBundleURLSchemes<\/key><array><string>https<\/string><string>http<\/string><\/array><\/dict><\/array>/);
  assert.match(mac, /<key>CFBundleDocumentTypes<\/key><array><dict><key>CFBundleTypeName<\/key><string>Webpage<\/string><key>CFBundleTypeRole<\/key><string>Viewer<\/string><key>LSHandlerRank<\/key><string>Alternate<\/string><key>LSItemContentTypes<\/key><array><string>public.html<\/string><string>public.xhtml<\/string><\/array><key>ExactWebPage<\/key><true\/><\/dict><\/array>/);
  assert.match(mac, /<key>protocol<\/key><string>https<\/string><key>url<\/key><string>\/open\?u=%s<\/string>/);
  // Beside the app's own documents, after them.
  const documents = flat(macInfoPlist({ ...app, manifest: { ...app.manifest, file_handlers: [{ action: '/', accept: { 'text/markdown': ['.md'] } }] } }));
  assert.match(documents, /net.daringfireball.markdown<\/string><\/array>.*<string>Webpage<\/string>/);
  // iOS registers neither, nor their template; the app's own scheme stays.
  const ios = infoPlist(app);
  assert.doesNotMatch(ios, /<string>https?<\/string>|Web site URL|Web page/);
  assert.match(ios, /<string>links<\/string>/);
  // The iOS and tvOS builds say so, but only when the pair would have been registered.
  const { webRefusal } = await import('../host/apple/documents.mjs');
  assert.match(webRefusal(app, false, false), /^host\/apple: protocol_handlers http and https: iOS registers neither/);
  assert.match(webRefusal(app, false, true), /tvOS registers neither/);
  assert.equal(webRefusal({ ...app, manifest: { protocol_handlers: [{ protocol: 'links', url: '/l?u=%s' }] } }, true, false), null);
  // A distribution-only pair: a local Mac build is no browser, and a local iOS build refuses nothing.
  const packaged = { ...app, manifest: { protocol_handlers: pair({ distribution_only: true }) } };
  assert.doesNotMatch(macInfoPlist(packaged), /https|Web page|CFBundleDocumentTypes/);
  assert.match(macInfoPlist(packaged, { distributed: true }), /Web page/);
  assert.equal(webRefusal(packaged, false, false), null);
  assert.match(webRefusal(packaged, true, false), /iOS registers neither/);
  // The web registers neither (HTML's safelist): the pair goes, the rest stays.
  assert.deepEqual(webAppManifest(app.manifest, 'Links').protocol_handlers, [{ protocol: 'links', url: '/l?u=%s' }]);
  assert.equal(webAppManifest({ protocol_handlers: pair() }, 'Links').protocol_handlers, undefined);
});

test('an iOS app lists the schemes canOpenURL may ask about, and a malformed or 51st one is refused', async () => {
  const { infoPlist } = await import('../host/apple/build.mjs');
  const { readManifest } = await import('./app.mjs');
  const { mkdtempSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const plist = infoPlist({ id: 'dev.exact.maps', displayName: 'Maps', manifest: { host: { ios: { queriesSchemes: ['comgooglemaps', 'waze'] } } } });
  assert.match(plist.replace(/\s+/g, ''), /<key>LSApplicationQueriesSchemes<\/key><array><string>comgooglemaps<\/string><string>waze<\/string><\/array>/);
  assert.doesNotMatch(infoPlist({ id: 'dev.exact.maps', displayName: 'Maps', manifest: {} }), /LSApplicationQueriesSchemes/);
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-queries-'));
  try {
    const save = (queriesSchemes) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Maps', app: { id: 'dev.exact.maps', name: 'Maps' }, host: { ios: { queriesSchemes } } }));
    save(['comgooglemaps', 'waze']);
    assert.deepEqual(readManifest(dir, 'maps').host.ios.queriesSchemes, ['comgooglemaps', 'waze']);
    for (const [schemes, says] of [[['waze://'], /does not match/], [Array.from({ length: 51 }, (_, i) => `s${i}`), /more than 50 items/]]) {
      save(schemes);
      assert.throws(() => readManifest(dir, 'maps'), says);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('an app\'s other names are Siri\'s on iOS, the Mac and its watch app, and a fourth or a blank one is refused (expo/exact#712)', async () => {
  const { infoPlist, macInfoPlist } = await import('../host/apple/build.mjs');
  const { watchInfo } = await import('../host/apple/watch.mjs');
  const { readManifest } = await import('./app.mjs');
  const { mkdtempSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const names = [{ name: 'Oldname', pronunciationHint: 'old name' }, { name: 'Old' }];
  const written = '<key>INAlternativeAppNames</key><array><dict><key>INAlternativeAppName</key><string>Oldname</string><key>INAlternativeAppNamePronunciationHint</key><string>oldname</string></dict><dict><key>INAlternativeAppName</key><string>Old</string></dict></array>';
  const app = (host) => ({ id: 'dev.exact.renamed', displayName: 'Renamed', manifest: { host } });
  assert.ok(infoPlist(app({ ios: { alternativeNames: names } })).replace(/\s+/g, '').includes(written));
  assert.ok(macInfoPlist(app({ macos: { alternativeNames: names } })).replace(/\s+/g, '').includes(written));
  for (const plist of [infoPlist(app({})), infoPlist(app({ ios: { alternativeNames: names } }), false, { tv: true }), infoPlist(app({ ios: { alternativeNames: names } }), false, { clip: true }), macInfoPlist(app({ ios: { alternativeNames: names } }))])
    assert.doesNotMatch(plist, /INAlternativeAppNames/);
  // Siri wants the watch app's names to be the iPhone app's, and the watch inherits none.
  const fields = { group: 'g', entries: [], live: true, generation: 'g' }, copied = [{ INAlternativeAppName: 'Old' }];
  assert.deepEqual(watchInfo(app({}), { ...fields, appInfo: { CFBundleVersion: '1', INAlternativeAppNames: copied } }).INAlternativeAppNames, copied);
  assert.equal(watchInfo(app({}), { ...fields, appInfo: { CFBundleVersion: '1' } }).INAlternativeAppNames, undefined);
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-names-'));
  try {
    const save = (alternativeNames) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Renamed', app: { id: 'dev.exact.renamed', name: 'Renamed' }, host: { ios: { alternativeNames }, macos: { alternativeNames } } }));
    save(names);
    assert.deepEqual(readManifest(dir, 'renamed').host.ios.alternativeNames, names);
    for (const [given, says] of [[[...names, { name: 'A' }, { name: 'B' }], /more than 3 items/], [[{ name: '' }], /alternativeNames\[0\]\.name/], [[{ pronunciationHint: 'x' }], /name/], [[{ name: 'A', hint: 'a' }], /hint/]]) {
      save(given);
      assert.throws(() => readManifest(dir, 'renamed'), says, JSON.stringify(given));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('readManifest admits the app\'s design, `forma` (LLP 1121 D2)', async () => {
  const { readManifest } = await import('./app.mjs');
  const { mkdtempSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const dir = mkdtempSync(resolve(tmpdir(), 'forma-manifest-'));
  try {
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "a"\n');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'D', app: { id: 'com.example.d', name: 'D' }, forma: 'design/app.forma' }));
    assert.equal(readManifest(dir, 'd').forma, 'design/app.forma');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('manifest colours follow the web and retired launch and alias keys are refused', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-manifest-'));
  const manifest = { name: 'Manifest', app: { id: 'test.manifest', name: 'Manifest' }, background_color: '#fff', color_scheme_dark: { background_color: '#123456' } };
  const read = value => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify(value)); return readManifest(dir, 'fixture'); };
  try {
    assert.equal(read(manifest).color_scheme_dark.background_color, '#123456');
    assert.throws(() => read({ ...manifest, background_color_dark: '#123456' }), /background_color_dark/);
    assert.throws(() => read({ ...manifest, launch: { background: '#fff' } }), /launch/);
    assert.throws(() => read({ ...manifest, typescript: { aliases: { '@/*': './*' } } }), /aliases/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('former design files and manifest keys name the required rename', async () => {
  const { readManifest } = await import('./app.mjs');
  const { designPath } = await import('./forma-design-path.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'forma-name-'));
  try {
    writeFileSync(resolve(dir, 'app.tamber'), '');
    for (const read of [() => readManifest(dir, 'test'), () => designPath(dir)]) {
      assert.throws(read, /rename app.tamber to app.forma/);
    }
    writeFileSync(resolve(dir, 'app.forma'), 'forma app\n');
    assert.throws(() => readManifest(dir, 'test'), /rename app.tamber to app.forma/);
    rmSync(resolve(dir, 'app.tamber'));
    writeFileSync(resolve(dir, 'app.json'), '{"tamber":"app.forma"}');
    assert.throws(() => readManifest(dir, 'test'), /rename app.json's tamber key to forma/);
    assert.throws(() => designPath(dir), /rename app.json's tamber key to forma/);
    rmSync(resolve(dir, 'app.json'));
    assert.equal(designPath(dir), resolve(dir, 'app.forma'));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('host.web.head takes script, link and meta entries and refuses the rest by entry', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-manifest-'));
  const read = head => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Head', app: { id: 'test.head', name: 'Head' }, host: { web: { head } } })); return readManifest(dir, 'fixture'); };
  try {
    read([{ script: { async: true, src: 'https://a.example/t.js' } }, { script: { type: 'application/ld+json', text: { '@type': 'WebSite' } } }, { meta: { name: 'author', content: 'J' } }]);
    assert.throws(() => read([{ meta: { name: 'author', content: 'J' } }, { style: { text: 'p{}' } }, { script: {}, link: {} }, { script: { async: false } },
      { link: { rel: 'preconnect', text: 'x' } }, { meta: { property: 'og:title', content: 'x' } }, { link: { rel: 'Canonical', href: '/' } }]), (error) => {
      assert.match(error.message, /host\.web\.head\[1\]: style is not a tag the head takes \(script, link, meta\)/);
      assert.match(error.message, /host\.web\.head\[2\]: \{"script":\{\},"link":\{\}\} is not one tag/);
      assert.match(error.message, /host\.web\.head\[3\]\.script\.async: false; an attribute is a string, or true/);
      assert.match(error.message, /host\.web\.head\[4\]\.link\.text: a link has no content/);
      assert.match(error.message, /host\.web\.head\[5\]: Exact writes this meta itself, from the Contract `head` element/);
      assert.match(error.message, /host\.web\.head\[6\]: Exact writes this link itself/);
      assert.doesNotMatch(error.message, /head\[0\]|matches 0 of the allowed forms/);
      return true;
    });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('host.windows.window takes its size and keepRunning, and refuses a keepRunning that is not a boolean', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-manifest-'));
  const read = window => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Tray', app: { id: 'test.tray', name: 'Tray' }, host: { windows: { window } } }));
    return readManifest(dir, 'fixture');
  };
  try {
    assert.doesNotThrow(() => read({ width: 800, height: 600 }));
    assert.doesNotThrow(() => read({ keepRunning: true }));
    assert.throws(() => read({ keepRunning: 'yes' }), /host\.windows\.window\.keepRunning/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a misspelt key beside host.web.head is refused as any unknown key is', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-manifest-'));
  try {
    for (const key of ['headers', 'heads']) {
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Head', app: { id: 'test.head', name: 'Head' }, host: { web: { [key]: [] } } }));
      assert.throws(() => readManifest(dir, 'fixture'), new RegExp(`host\\.web\\.${key}: not a known key`));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("host.web.head refuses what Exact writes or reads itself, however spelt", async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-manifest-'));
  const read = web => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Head', app: { id: 'test.head', name: 'Head' }, host: { web } })); return readManifest(dir, 'fixture'); };
  try {
    // Another app's files, a file whose name differs only in case, a preload of anything else and another `alternate`
    // are the app's own.
    read({ head: [{ script: { type: 'module', src: 'https://cdn.example/app.js' } }, { link: { rel: 'modulepreload', href: 'https://cdn.example/x.js' } },
      { script: { type: 'module', src: './App.js' } }, { link: { rel: 'preload', href: '/Navigation.js', as: 'script' } },
      { link: { rel: 'preload', href: './font.woff2', as: 'font' } }, { link: { rel: 'alternate', type: 'application/rss+xml', href: '/feed.xml' } },
      { meta: { name: 'author', 'data-x': 'y', 'xml:lang': 'en' } }] });
    const refused = [
      [{ script: { type: 'module', src: './app.js' } }, /\[0\]: Exact writes this script itself/],
      [{ script: { src: '/glue.js?v=1' } }, /\[1\]: Exact writes this script itself/],
      [{ script: { type: 'Application/vnd.exact.checkpoint', text: { answers: [] } } }, /\[2\]: Exact writes this script itself/],
      [{ meta: { name: 'exact-native', content: './x.js' } }, /\[3\]: Exact writes this meta itself/],
      [{ link: { rel: 'preload', href: './app.wasm?v=2', as: 'fetch' } }, /\[4\]: Exact writes this link itself/],
      [{ link: { rel: 'modulepreload', href: 'app.js' } }, /\[5\]: Exact writes this link itself/],
      [{ link: { rel: 'alternate', type: 'application/vnd.exact.envelope+json', href: './exact.json' } }, /\[6\]: Exact writes this link itself/],
      [{ link: { rel: 'shortcut icon', href: '/x.ico' } }, /\[7\]: Exact writes this link itself/],
      [{ link: { rel: 'icon\tshortcut', href: '/x.ico' } }, /\[8\]: Exact writes this link itself/],
      [{ meta: { NAME: 'viewport', content: 'x' } }, /\[9\]: Exact writes this meta itself/],
      [{ meta: { '<x': '1' } }, /\[10\]\.meta: "<x" is not an attribute name/],
      [{ meta: { 'a"b': '1' } }, /\[11\]\.meta: "a\\"b" is not an attribute name/],
      [{ meta: { name: 'x', Name: 'y' } }, /\[12\]\.meta: "Name" is the attribute "name" again/],
      // Resolved against the shells' `<base href="/">`, as the browser does.
      [{ link: { rel: 'modulepreload', href: '../navigation.js' } }, /\[13\]: Exact writes this link itself/],
      [{ script: { type: 'module', src: './x/../app.js?duplicate=1' } }, /\[14\]: Exact writes this script itself/],
      [{ link: { rel: 'modulepreload', href: './foo/../navigation.js' } }, /\[15\]: Exact writes this link itself/],
      [{ link: { rel: 'modulepreload', href: '%2e%2e/navigation.js' } }, /\[16\]: Exact writes this link itself/],
      [{ link: { rel: 'preload', href: '../app.wasm', as: 'fetch' } }, /\[17\]: Exact writes this link itself/],
      [{ script: { src: '%61pp.js' } }, /\[18\]: Exact writes this script itself/],
      // And as serve.mjs answers the request the page sends: `%2f` is a separator, a trailing one is dropped.
      [{ script: { type: 'module', src: '/%2fapp.js' } }, /\[19\]: Exact writes this script itself/],
      [{ script: { type: 'module', src: '/%2Fglue.js' } }, /\[20\]: Exact writes this script itself/],
      [{ script: { type: 'module', src: './app.js/' } }, /\[21\]: Exact writes this script itself/],
      [{ link: { rel: 'modulepreload', href: '/navigation.js%2f' } }, /\[22\]: Exact writes this link itself/],
      [{ script: { type: 'module', src: '/./app.js' } }, /\[23\]: Exact writes this script itself/],
      [{ script: { type: 'module', src: '/%2e/app.js' } }, /\[24\]: Exact writes this script itself/],
      [{ meta: { name: 'Color-Scheme', content: 'dark' } }, /\[25\]: Exact writes this meta itself, from app.json's userInterfaceStyle/],
    ];
    assert.throws(() => read({ head: refused.map(([entry]) => entry) }), (error) => {
      for (const [, says] of refused) assert.match(error.message, says);
      return true;
    });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("host.web.head takes an attribute that names a property of Object, and its own origin's files are Exact's", async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-manifest-'));
  const read = (head, origin) => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Head', app: { id: 'test.head', name: 'Head', ...(origin ? { origin } : {}) }, host: { web: { head } } })); return readManifest(dir, 'fixture'); };
  try {
    read([{ meta: { name: 'author', constructor: 'Person' } }, { script: { src: '/widget.js', __proto__x: 'y', toString: 'z', valueOf: '1' } }]);
    assert.throws(() => read([{ meta: { constructor: 'a', Constructor: 'b' } }]), /\[0\]\.meta: "Constructor" is the attribute "constructor" again/);
    // Another origin's `app.js` is its own; the app's origin, written whole or without its scheme, names Exact's.
    read([{ script: { type: 'module', src: 'https://cdn.example/app.js' } }, { script: { type: 'module', src: 'https://example.com.cdn.example/app.js' } }], 'https://example.com');
    const own = ['https://example.com/app.js', '//example.com/glue.js?v=1', 'https://EXAMPLE.com:443/./app.wasm', 'https://example.com/x/../navigation.js', 'https://example.com/audio-session.js'];
    assert.throws(() => read(own.map((src) => ({ script: { type: 'module', src } })), 'https://example.com'), (error) => {
      own.forEach((_, i) => assert.match(error.message, new RegExp(`\\[${i}\\]: Exact writes this script itself`)));
      return true;
    });
    // With no origin declared, an absolute URL is another site's.
    read([{ script: { type: 'module', src: 'https://example.com/app.js' } }]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// HTML's tokenizer ends a script element only at `</script` followed by whitespace, `/` or `>`, and leaves script data
// for the escaped states only at `<!--` (HTML, 13.2.5 "Tokenization"). A JSON text has those written `\u003C…`; a
// JavaScript text is never rewritten, since `\u003C` in code is a syntax error: one holding either is refused.
test("host.web.head writes a script's text as it is, refuses one HTML would read a tag in, and escapes JSON's", async () => {
  const { webHead, webHeadProblems } = await import('./web-head.mjs');
  const head = (text, type) => ({ host: { web: { head: [{ script: { ...(type ? { type } : {}), text } }] } } });
  const written = (text) => webHead(head(text)).slice('<script>'.length, -'</script>\n'.length);
  for (const code of ['const script = 4; window.ok = 1<script / 2;', 'const scripts = [1]; let n = 0; for (let i = 0; i<scripts.length; i++) n++;',
    'const a = 2; window.lt = a</scripted/.test("scripted");', 'const x = 3; window.y = x<script >2 ? 0 : 1;', "window.seen = '\\x3C/script>' + '<' + '!--';"]) {
    assert.equal(written(code), code);
    new Function(written(code));
  }
  for (const [code, seen] of [['if (a <!--b) {}', '<!--'], ['<!--\nwindow.ok = 1;\n', '<!--'], ['if (a </script>/) {}', '</script'],
    ["window.seen = '</SCRIPT\t';", '</script'], ["x = '</script/'", '</script'], ['{"name": "</script>"}', '</script']]) {
    const problems = webHeadProblems(head(code, code.startsWith('{') ? 'application/ld+json' : undefined));
    assert.equal(problems.length, 1, code);
    assert.match(problems[0], new RegExp(`^host\\.web\\.head\\[0\\]\\.script\\.text: holds "${seen}", .*write it so the parser cannot see it`), code);
  }
  // An attribute's `>` is escaped too, so a reader that ends the start tag at a `>` reads the whole tag.
  assert.equal(webHead({ host: { web: { head: [{ script: { 'data-x': 'a>b"<&', text: 'x()' } }] } } }), '<script data-x="a&gt;b&quot;&lt;&amp;">x()</script>\n');
  // A JSON text reads `\u003C` as `<`.
  const json = webHead(head({ name: '</script><!--<script>' }, 'application/ld+json'));
  assert.equal(JSON.parse(json.slice(json.indexOf('>') + 1, json.lastIndexOf('</script>'))).name, '</script><!--<script>');
});

// LLP 1048.000 D11: the app's part of the render server's CSP. The build refuses only what would not be one directive
// of one policy (CSP3 §2.3); the browser reads the rest.
test('host.web.csp takes lists to add and strings to replace, and refuses what would end a directive or a policy', async () => {
  const { readManifest } = await import('./app.mjs');
  const { webCspFile } = await import('./web-head.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-manifest-'));
  const read = (web) => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Csp', app: { id: 'test.csp', name: 'Csp' }, host: { web } })); return readManifest(dir, 'fixture'); };
  try {
    const web = { csp: { 'script-src': ['https://tags.example'], 'img-src': "exact https://img.example", 'upgrade-insecure-requests': '' }, cspReportOnly: false };
    assert.equal(webCspFile(read(web)), JSON.stringify(web) + '\n');
    assert.equal(webCspFile(read({ csp: false })), '{"csp":false}\n');
    assert.equal(webCspFile(read({})), null);
    const refused = [
      [{ csp: { 'Script-Src': ['a.example'] } }, /host\.web\.csp: "Script-Src" is not a directive name in lower case/],
      [{ csp: { 'img-src': ['a.example b.example'] } }, /host\.web\.csp\.img-src\[0\]: "a\.example b\.example" is not one source/],
      [{ csp: { 'img-src': "'self'; script-src *" } }, /host\.web\.csp\.img-src: .* holds a comma, a semicolon/],
      [{ cspReportOnly: { 'img-src': 'a.example, b.example' } }, /host\.web\.cspReportOnly\.img-src: .* holds a comma/],
      [{ csp: { 'img-src': 'a.example\nX-Injected: 1' } }, /host\.web\.csp\.img-src: .* a line break/],
      [{ csp: true }, /host\.web\.csp: expected false or an object of directives/],
      [{ csp: { 'img-src': 1 } }, /host\.web\.csp\.img-src: expected a list of sources to add, or a string/],
    ];
    for (const [web, says] of refused) assert.throws(() => read(web), says, JSON.stringify(web));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('readManifest names the game.presentation rename instead of an unknown key', async () => {
  const { readManifest } = await import('./app.mjs');
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const dir = mkdtempSync(resolve(tmpdir(), 'render-rename-manifest-'));
  try {
    mkdirSync(resolve(dir, 'logic/src'), { recursive: true });
    writeFileSync(resolve(dir, 'logic/src/lib.rs'), 'impl Game for Island { const ID: &\'static str = "island"; }');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ game: { presentation: { crate: 'island-presentation', type: 'Hooks' } } }));
    assert.throws(() => readManifest(dir, 'island'), /game\.presentation is now game\.render/);
    assert.throws(() => readManifest(dir, 'island'), (e) => !/not a known key/.test(e.message));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the manifest orientation locks the iPhone, and an unknown one is refused', async () => {
  const { infoPlist } = await import('../host/apple/build.mjs');
  const { phoneOrientations } = await import('../host/apple/devices.mjs');
  const plist = (orientation) => infoPlist({ id: 'dev.exact.o', displayName: 'O', manifest: orientation ? { orientation } : {} }).replace(/\s+/g, '');
  assert.match(plist('portrait'), /<key>UISupportedInterfaceOrientations<\/key><array><string>UIInterfaceOrientationPortrait<\/string><\/array>/);
  // iPad keeps all four, as its multitasking requires, whatever the iPhone is locked to
  assert.match(plist('portrait'), /<key>UISupportedInterfaceOrientations~ipad<\/key><array>(<string>UIInterfaceOrientation[A-Za-z]+<\/string>){4}<\/array>/);
  assert.match(plist('landscape-primary'), /<key>UISupportedInterfaceOrientations<\/key><array><string>UIInterfaceOrientationLandscapeRight<\/string><\/array>/);
  for (const free of [undefined, 'any']) assert.doesNotMatch(plist(free), /UISupportedInterfaceOrientations</);
  assert.deepEqual(phoneOrientations('landscape'), ['UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight']);
  assert.deepEqual(phoneOrientations('portrait-secondary'), ['UIInterfaceOrientationPortrait']);
  const { readManifest } = await import('./app.mjs');
  const { mkdtempSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-orientation-'));
  try {
    const save = (orientation) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'O', app: { id: 'dev.exact.o', name: 'O' }, orientation }));
    save('portrait');
    assert.equal(readManifest(dir, 'o').orientation, 'portrait');
    save('upright');
    assert.throws(() => readManifest(dir, 'o'), /is not one of/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('document handlers and TypeScript source mounts accept their manifest shapes and reject guessed ones', async () => {
  const { readManifest, webAppManifest } = await import('./app.mjs');
  const { documentTypes } = await import('../host/apple/documents.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-author-shapes-'));
  const base = { name: 'Reader', app: { id: 'dev.exact.reader', name: 'Reader' } };
  const read = extra => { writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...base, ...extra })); return readManifest(dir, 'reader'); };
  const file_handlers = [{ action: '/', name: 'Documents', accept: {
    'text/markdown': ['.MD', 'markdown'],
    'application/octet-stream': ['.tar.gz', 'c++'],
    'application/x-studio-board+json': ['board'],
  } }];
  const sources = { backend: '../generated', snapback4: '../runtime' };
  try {
    const suffixes = JSON.parse(readFileSync(resolve(import.meta.dir, 'fixtures/document-suffixes.json'), 'utf8'));
    for (const { declaration, canonical, error } of suffixes) {
      if (canonical) {
        assert.equal(read({ file_handlers: [{ action: '/', accept: { 'text/markdown': [declaration] } }] }).file_handlers[0].accept['text/markdown'][0], canonical);
      } else {
        assert.throws(() => read({ file_handlers: [{ action: '/', accept: { 'text/markdown': [declaration] } }] }), new RegExp(error.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
      }
    }
    const manifest = read({ file_handlers, typescript: { sources } });
    assert.deepEqual(manifest.file_handlers[0].accept, {
      'text/markdown': ['.md', '.markdown'],
      'application/octet-stream': ['.tar.gz', '.c++'],
      'application/x-studio-board+json': ['.board'],
    });
    assert.deepEqual(webAppManifest(manifest, 'Reader').file_handlers, manifest.file_handlers);
    assert.deepEqual(documentTypes({ manifest })[0].CFBundleTypeExtensions, ['board']);
    assert.deepEqual(documentTypes({ manifest })[1].CFBundleTypeExtensions, ['md', 'markdown', 'tar.gz', 'c++']);
    assert.deepEqual(manifest.typescript.sources, sources);
    const before = { variant: process.env.EXACT_VARIANT, app: process.env.EXACT_VARIANT_APP };
    try {
      process.env.EXACT_VARIANT = 'dev'; process.env.EXACT_VARIANT_APP = dir;
      const selected = read({ variants: { dev: { idSuffix: '.dev', name: 'Reader Dev' } }, file_handlers });
      assert.equal(selected.app.id, 'dev.exact.reader.dev');
      assert.equal(selected.file_handlers[0].accept['text/markdown'][0], '.md');
      assert.deepEqual(documentTypes({ id: selected.app.id, manifest: selected })[0].LSItemContentTypes, ['dev.exact.reader.studio-board']);
    } finally {
      if (before.variant === undefined) delete process.env.EXACT_VARIANT; else process.env.EXACT_VARIANT = before.variant;
      if (before.app === undefined) delete process.env.EXACT_VARIANT_APP; else process.env.EXACT_VARIANT_APP = before.app;
    }
    for (const [mime, ext, uti] of [
      ['audio/mpeg', '.mp3', 'public.mp3'], ['audio/wav', '.wav', 'com.microsoft.waveform-audio'],
      ['audio/mp4', '.m4a', 'public.mpeg-4-audio'], ['audio/ogg', '.ogg', 'org.xiph.ogg-audio'],
      ['application/xhtml+xml', '.xhtml', 'public.xhtml'],
    ]) {
      const manifest = read({ file_handlers: [{ action: '/', accept: { [mime]: [ext] } }] });
      assert.deepEqual(documentTypes({ manifest })[0].LSItemContentTypes, [uti]);
    }
    assert.throws(() => read({ file_handlers: { 'text/markdown': ['.md'] } }), /file_handlers:.*array/);
    assert.throws(() => read({ file_handlers: [{ action: '/', accept: ['text/markdown'] }] }), /file_handlers\[0\].accept:.*object/);
    for (const extension of ['', '.', '.md.', 'm*d', 'm?d', 'm[d]', 'm/d', 'm\\d', 'm_d', 'm-d', 'mé', '.1234567890123456']) {
      assert.throws(
        () => read({ file_handlers: [{ action: '/', accept: { 'text/markdown': [extension] } }] }),
        /file_handlers\[0\]\.accept\.text\/markdown\[0\]:.*is not a file suffix/,
        JSON.stringify(extension),
      );
    }
    for (const name of ['', 'Markdown\0files', 'Markdown\nfiles']) {
      assert.throws(
        () => read({ file_handlers: [{ action: '/', name, accept: { 'text/markdown': ['.md'] } }] }),
        /file_handlers\[0\]\.name:.*is not a label/,
        JSON.stringify(name),
      );
    }
    assert.throws(() => read({ typescript: { sources: ['../generated'] } }), /typescript.sources:.*object/);
    assert.throws(() => read({ typescript: { sources: { backend: { path: '../generated' } } } }), /typescript.sources.backend:.*string/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('watchFamilies admits only the four accessory families', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-watch-families-'));
  try {
    const write = watchFamilies => writeFileSync(resolve(dir,'app.json'), JSON.stringify({name:'Watch', app:{id:'com.exact.watch', name:'Watch'}, widgets:[{kind:'train',families:['systemSmall'],watchFamilies,component:'Card',props:'Props',refresh:'refresh',preview:{}}]}));
    const families = ['accessoryCircular','accessoryRectangular','accessoryInline','accessoryCorner'];
    write(families); assert.deepEqual(readManifest(dir,'watch').widgets[0].watchFamilies, families);
    for (const family of ['systemSmall','systemMedium','systemLarge','systemExtraLarge']) {
      write([family]); assert.throws(() => readManifest(dir,'watch'), /watchFamilies/);
    }
    // iOS 27 portrait extra-large is admitted in families, guarded in Swift source, and omitted on watchOS (LLP 1156.007 §5.1).
    const { platformWidgetEntries, widgetBundleSource } = await import('../host/apple/widgets.mjs');
    writeFileSync(resolve(dir,'app.json'), JSON.stringify({name:'XL', app:{id:'com.exact.xl', name:'XL'}, widgets:[{kind:'board',families:['systemExtraLarge','systemExtraLargePortrait'],component:'Card',props:'Props',refresh:'refresh',preview:{}}]}));
    const xl = { id: 'com.exact.xl', manifest: readManifest(dir, 'xl') };
    assert.deepEqual(platformWidgetEntries(xl, 'ios').map(e => e.families), [['systemExtraLarge', 'systemExtraLargePortrait']]);
    assert.deepEqual(platformWidgetEntries(xl, 'macos').map(e => e.families), [['systemExtraLarge', 'systemExtraLargePortrait']]);
    assert.deepEqual(platformWidgetEntries(xl, 'watchos'), []);
    assert.match(widgetBundleSource(platformWidgetEntries(xl, 'ios'), { platform: 'ios' }), /if #available\(iOS 27\.0, macOS 27\.0, \*\) \{ families\.append\(\.systemExtraLargePortrait\) \}/);
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('activity labels reject whitespace and control characters in schema validation', async () => {
  const {validate} = await import('./app.mjs');
  const schema = await Bun.file(new URL('./app.schema.json', import.meta.url)).json();
  const activity = schema.properties.live_activities.items;
  for (const key of ['title', 'description']) {
    const label = activity.properties[key];
    assert.deepEqual(validate('Next train 🚆', label, key, schema), []);
    for (const blank of ['', ' ', '   ', '\u00a0', '\u2003']) {
      assert.ok(validate(blank, label, key, schema).length, `${key}: blank label`);
    }
    for (const code of [...Array.from({length:32}, (_, i) => i), ...Array.from({length:33}, (_, i) => i + 127), 0x2028, 0x2029]) {
      for (const value of [`bad${String.fromCodePoint(code)}label`, `label${String.fromCodePoint(code)}`]) {
        assert.ok(validate(value, label, key, schema).length, `${key}: ${code}`);
      }
    }
  }
});


test('requiresSecrets scopes the account placeholder to its widget entry', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-widget-account-'));
  try {
    const widget = { kind: 'card', families: ['systemSmall'], component: 'Card', props: 'Props', refresh: 'refresh', preview: {} };
    const write = requiresSecrets => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Account', app: { id: 'com.exact.account', name: 'Account' }, widgets: [{ ...widget, requiresSecrets }, { ...widget, kind: 'public' }] }));
    write(true); assert.equal(readManifest(dir, 'account').widgets[0].requiresSecrets, true);
    assert.equal(readManifest(dir, 'account').widgets[1].requiresSecrets, undefined);
    write('yes'); assert.throws(() => readManifest(dir, 'account'), /requiresSecrets/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('host.ios.appClip is an object of an id and a name, and an unknown key or a malformed id is refused', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-clip-'));
  try {
    const save = (appClip) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Shop', app: { id: 'dev.exact.shop', name: 'Shop' }, host: { ios: { appClip } } }));
    save({});
    assert.deepEqual(readManifest(dir, 'shop').host.ios.appClip, {});
    save({ id: 'dev.exact.shop.clip', name: 'Shop Clip' });
    assert.deepEqual(readManifest(dir, 'shop').host.ios.appClip, { id: 'dev.exact.shop.clip', name: 'Shop Clip' });
    for (const [appClip, says] of [[{ domains: ['shop.example'] }, /appClip\.domains: not a known key/], [{ id: 'clip' }, /appClip\.id: "clip" does not match/], [true, /appClip/]]) {
      save(appClip);
      assert.throws(() => readManifest(dir, 'shop'), says);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('display_override is the web manifest\'s, written into manifest.json and baked for the Mac', async () => {
  const { readManifest, webAppManifest } = await import('./app.mjs');
  const { macInfoPlist } = await import('../host/apple/build.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-display-override-'));
  const save = display_override => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Row', app: { id: 'dev.exact.row', name: 'Row' }, display: 'standalone', display_override }));
  try {
    save(['window-controls-overlay', 'standalone']);
    const manifest = readManifest(dir, 'row');
    assert.deepEqual(manifest.display_override, ['window-controls-overlay', 'standalone']);
    assert.deepEqual(webAppManifest(manifest, 'Row').display_override, ['window-controls-overlay', 'standalone']);
    const plist = macInfoPlist({ id: 'dev.exact.row', displayName: 'Row', manifest });
    assert.match(plist, /<key>ExactDisplayOverride<\/key>\s*<array>\s*<string>window-controls-overlay<\/string>/);
    save(['borderless']);
    assert.throws(() => readManifest(dir, 'row'), /display_override/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('open_url declares the schemes openURL may hand to the system, and refuses the ones the hosts keep', async () => {
  const { macInfoPlist, infoPlist } = await import('../host/apple/build.mjs');
  const { readManifest } = await import('./app.mjs');
  const app = { id: 'dev.exact.editors', displayName: 'Editors', manifest: { open_url: { schemes: ['vscode', 'zed', 'jetbrains'] } } };
  for (const plist of [macInfoPlist(app), infoPlist(app)]) {
    assert.match(plist.replace(/\s+/g, ''), /<key>ExactOpenURLSchemes<\/key><array><string>vscode<\/string><string>zed<\/string><string>jetbrains<\/string><\/array>/);
  }
  assert.doesNotMatch(infoPlist({ id: 'dev.exact.editors', displayName: 'Editors', manifest: {} }), /ExactOpenURLSchemes/);
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-open-url-'));
  try {
    const save = (open_url) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Editors', app: { id: 'dev.exact.editors', name: 'Editors' }, open_url }));
    save({ schemes: ['vscode-insiders', 'cursor', 'sms'] });
    assert.deepEqual(readManifest(dir, 'editors').open_url.schemes, ['vscode-insiders', 'cursor', 'sms']);
    for (const [open_url, says] of [[{ schemes: ['file'] }, /does not match/], [{ schemes: ['javascript'] }, /does not match/], [{ schemes: ['https'] }, /does not match/], [{ schemes: ['vbscript'] }, /does not match/], [{ schemes: ['view-source'] }, /does not match/], [{ schemes: ['intent'] }, /does not match/],
      [{ schemes: ['vscode://'] }, /does not match/], [{ schemes: [] }, /fewer than 1/], [{ apps: ['vscode'] }, /not a known key/]]) {
      save(open_url);
      assert.throws(() => readManifest(dir, 'editors'), says);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('foreground update intervals are positive seconds and reach both Apple bundles', async () => {
  const { readManifest } = await import('./app.mjs');
  const { macInfoPlist, infoPlist } = await import('../host/apple/build.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-checks-'));
  const manifest = { name: 'Checks', app: { id: 'test.checks', name: 'Checks' }, deploy: { activate: 'app-decides', check: { interval: 3 } } };
  const save = () => writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
  try {
    save();
    assert.equal(readManifest(dir, 'checks').deploy.check.interval, 3);
    const app = { id: 'test.checks', displayName: 'Checks', manifest };
    for (const plist of [macInfoPlist(app), infoPlist(app)]) {
      assert.match(plist, /<key>ExactUpdateCheckInterval<\/key>\s*<integer>3<\/integer>/);
      assert.match(plist, /<key>ExactUpdateActivate<\/key>\s*<string>app-decides<\/string>/);
    }
    for (const interval of [0, -1, '3']) {
      manifest.deploy.check.interval = interval;
      save();
      assert.throws(() => readManifest(dir, 'checks'), /interval/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('shortcuts reach manifest.json without the native bakes\' words, and a remote icon is refused', async () => {
  const { readManifest, webAppManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-shortcuts-'));
  const save = (shortcuts) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Shelf', app: { id: 'dev.exact.shelf', name: 'Shelf' }, shortcuts }));
  try {
    save([{ name: 'Add Book', url: '/add', apple: { symbol: 'plus' }, android: { rank: 0 } }]);
    const manifest = readManifest(dir, 'shelf');
    assert.deepEqual(manifest.shortcuts[0].apple, { symbol: 'plus' });
    assert.deepEqual(webAppManifest(manifest, 'Shelf').shortcuts, [{ name: 'Add Book', url: '/add' }]);
    assert.equal(webAppManifest({ shortcuts: [] }, 'Shelf').shortcuts, undefined);
    for (const bad of [{ name: 'Add', url: 'https://example.com/add' }, { name: '', url: '/add' }, { name: 'Add', url: '/add', icons: [{ src: 'https://example.com/i.png' }] }]) {
      save([bad]);
      assert.throws(() => readManifest(dir, 'shelf'), /shortcuts\[0\]/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('iOS browser opt-in requires a web-scheme pair and preserves ordinary apps', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-ios-browser-'));
  const base = { name: 'Browser', app: { id: 'dev.exact.browser', name: 'Browser' } };
  const pair = ['http', 'https'].map(protocol => ({ protocol, url: '/open?u=%s' }));
  const save = (value) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify(value));
  try {
    for (const flag of [true, false]) {
      save({ ...base, host: { ios: { webBrowser: flag } }, protocol_handlers: pair });
      assert.equal(readManifest(dir, 'browser').host.ios.webBrowser, flag);
    }
    save({ ...base, host: { ios: { webBrowser: true } } });
    assert.throws(() => readManifest(dir, 'browser'), /host.ios.webBrowser: declare http and https/);
    save({ ...base, host: { ios: { webBrowser: 'true' } }, protocol_handlers: pair });
    assert.throws(() => readManifest(dir, 'browser'), /host.ios.webBrowser: expected boolean, got string/);
    save({ ...base, host: { ios: { webBrowser: false } } });
    assert.equal(readManifest(dir, 'browser').host.ios.webBrowser, false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Windows Ink builds require fxc from PATH or the SDK; CPU builds do not', async () => {
  const { prepareShaders } = await import('../host/windows/build.mjs');
  const { mkdirSync } = await import('node:fs');
  const { delimiter } = await import('node:path');
  const dir = mkdtempSync(resolve(tmpdir(), 'windows-fxc-'));
  const env = () => ({ PATH: '', 'ProgramFiles(x86)': dir, EXACT_PAINTER: 'cpu' });
  try {
    for (const painter of [undefined, 'ink']) {
      assert.throws(() => prepareShaders({ host: { windows: { painter } } }, env()), /install the Windows SDK, or select host.windows.painter: "cpu"/);
    }
    assert.doesNotThrow(() => prepareShaders({ host: { windows: { painter: 'cpu' } } }, env()));
    const game = { crate: 'world-logic', type: 'World' };
    assert.doesNotThrow(() => prepareShaders({ game }, env()));
    assert.doesNotThrow(() => prepareShaders({ game, host: { windows: { painter: 'cpu' } } }, env()));
    assert.throws(() => prepareShaders({ game, host: { windows: { painter: 'ink' } } }, env()), /install the Windows SDK/);
    const path = resolve(dir, 'tools'); mkdirSync(path); writeFileSync(resolve(path, 'fxc.exe'), '');
    const onPath = { ...env(), PATH: path }; prepareShaders({}, onPath);
    assert.equal(onPath.PATH, path);
    for (const version of ['10.0.9.0', '10.0.10.0']) {
      const sdk = resolve(dir, 'Windows Kits', '10', 'bin', version, 'x64');
      mkdirSync(sdk, { recursive: true }); writeFileSync(resolve(sdk, 'fxc.exe'), '');
    }
    const sdk = env(); prepareShaders({}, sdk);
    assert.equal(sdk.PATH.split(delimiter)[0], resolve(dir, 'Windows Kits', '10', 'bin', '10.0.10.0', 'x64'));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('source identities match Cargo roots for Windows canonical drive and UNC paths', async () => {
  const { sourcePath } = await import('./cargo-dep-info.mjs');
  const { win32 } = await import('node:path');
  for (const root of [String.raw`C:\app\packages`, String.raw`\\server\share\packages`]) {
    const path = win32.join(root, 'dock', 'windows', 'lib.rs');
    const canonical = win32.resolve(win32.toNamespacedPath(path));
    assert.equal(sourcePath(canonical, 'win32'), path);
    assert.equal(win32.relative(root, sourcePath(canonical, 'win32')), String.raw`dock\windows\lib.rs`);
    assert.equal(sourcePath(path, 'win32'), path);
    assert.equal(sourcePath(canonical, 'darwin'), canonical);
  }
});

test('`userInterfaceStyle` takes Expo\'s three values, refusing others by name, and fixes the Apple binaries\' metadata', async () => {
  const { readManifest, nativeManifestMetadata } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-style-'));
  const read = (style) => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Style', app: { id: 'dev.exact.style', name: 'Style' }, ...(style === undefined ? {} : { userInterfaceStyle: style }) }));
    return readManifest(dir, 'style');
  };
  try {
    for (const style of ['light', 'dark']) {
      for (const platform of ['ios', 'macos']) assert.equal(nativeManifestMetadata(read(style), platform).userInterfaceStyle, style);
      for (const platform of ['android', 'linux', 'windows']) assert.equal(nativeManifestMetadata(read(style), platform).userInterfaceStyle, undefined);
    }
    // `automatic` is the default: a binary built with it is the one built without the key.
    assert.deepEqual(nativeManifestMetadata(read('automatic'), 'ios'), nativeManifestMetadata(read(undefined), 'ios'));
    assert.throws(() => read('Dark'), /userInterfaceStyle: "Dark" is not one of "automatic", "light", "dark"/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a launch colour of a host\'s own or Expo\'s splash is refused for background_color, every host\'s one colour', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-launch-'));
  const read = (extra) => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Launch', app: { id: 'dev.exact.launch', name: 'Launch' }, ...extra }));
    return readManifest(dir, 'launch');
  };
  try {
    for (const [extra, key] of [[{ host: { android: { windowBackground: '#000' } } }, 'host.android.windowBackground'], [{ splash: { backgroundColor: '#000' } }, 'splash'], [{ host: { android: { splashScreen: {} } } }, 'host.android.splashScreen']]) {
      assert.throws(() => read(extra), new RegExp(`${key.replaceAll('.', '\\.')}: not a known key; what shows before the first frame is background_color.*host\\.android\\.launchImage add a picture`));
    }
    assert.equal(read({ background_color: '#102839', color_scheme_dark: { background_color: '#000' } }).background_color, '#102839');
    assert.throws(() => read({ color_scheme_dark: { background_color: 'navy' } }), /color_scheme_dark\.background_color: "navy" does not match.*: write #RGB, #RRGGBB or #RRGGBBAA, or a Display P3 colour/);
    // LLP 1163 D2: one scheme's colours under its own member, Display P3 among them
    assert.deepEqual(read({ color_scheme_light: { background_color: 'color(display-p3 1 0.5 0)', theme_color: '#eee' } }).color_scheme_light, { background_color: 'color(display-p3 1 0.5 0)', theme_color: '#eee' });
    assert.throws(() => read({ color_scheme_light: { background_color: 'color(display-p3 1.5 0 0)' } }), /color_scheme_light\.background_color: .* does not match/);
    assert.throws(() => read({ color_scheme_light: { icons: [] } }), /color_scheme_light\.icons/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('host.android.launchImage takes a PNG in the app up to the starting window\'s 192 dp icon, or false', async () => {
  const { readManifest, nativeManifestMetadata } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-launch-image-'));
  const read = (launchImage) => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Launch', app: { id: 'dev.exact.launch', name: 'Launch' }, background_color: '#B50000', host: { android: { launchImage } } }));
    return readManifest(dir, 'launch');
  };
  try {
    writeFileSync(resolve(dir, 'logo.png'), 'png');
    const manifest = read({ src: 'logo.png', width: 192 });
    // A native input: the Android binary's record names it, so an update cannot change it.
    assert.deepEqual(nativeManifestMetadata(manifest, 'android').host.launchImage, { src: 'logo.png', width: 192 });
    assert.equal(read(false).host.android.launchImage, false);
    assert.throws(() => read(true), /host\.android\.launchImage: true is not a picture/);
    assert.throws(() => read({ src: 'logo.png', width: 200 }), /host\.android\.launchImage\.width: outside the allowed numeric range/);
    assert.throws(() => read({ src: 'logo.svg', width: 100 }), /host\.android\.launchImage\.src: "logo\.svg" does not match/);
    assert.throws(() => read({ src: 'logo.png' }), /host\.android\.launchImage: missing required "width"/);
    assert.throws(() => read({ src: 'gone.png', width: 100 }), /host\.android\.launchImage: gone\.png, the launch screen's picture, does not exist/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('host.android.maven accepts only pinned group:artifact:version coordinates', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-maven-'));
  const read = maven => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Maps', app: { id: 'dev.exact.maps', name: 'Maps' }, host: { android: { maven } } }));
    return readManifest(dir, 'maps');
  };
  try {
    for (const maven of [[], ['org.maplibre.gl:android-sdk:12.0.0', 'androidx.camera:camera-core:1.6.0-alpha01']]) {
      assert.deepEqual(read(maven).host.android.maven, maven);
    }
    for (const bad of [null, 'g:a:1', [12], ['g:a'], ['g:a:'], ['g:a:+'], ['g:a:1.+'], ['g:a:[1,2)'],
      ['g:a:latest.release'], ['g:a:LATEST'], ['g:a:RELEASE'], ['g:a:1-SNAPSHOT'], ['g:a:1:classifier'], ['g:a:1@aar'], ['https://repo/g:a:1'], ['g:a:1\n']]) {
      assert.throws(() => read(bad), /maven/, JSON.stringify(bad));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('preload admits the entry shape and refuses malformed names, fields and bounds', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-preload-'));
  const read = preload => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Fixture', app: { id: 'test.preload', name: 'Fixture' }, preload }));
    return readManifest(dir, 'fixture');
  };
  try {
    const preload = { second: { url: 'https://api.example/{secret:session}' }, first: { url: 'https://api.example', method: 'POST', headers: { Authorization: 'Bearer {secret:session}' }, body: '{{', timeoutMs: 42, when: 'foreground' } };
    assert.deepEqual(read(preload).preload, preload);
    for (const [value, field] of [
      [{ Wrong: { url: 'https://api.example' } }, 'Wrong.name'],
      [{ feed: { url: 1 } }, 'feed.url'],
      [{ feed: { url: 'https://api.example', extra: true } }, 'feed.extra'],
      [{ feed: { url: 'https://api.example', timeoutMs: 1.5 } }, 'feed.timeoutMs'],
      [{ feed: { url: 'https://api.example', timeoutMs: 0 } }, 'feed.timeoutMs'],
      [{ feed: { url: 'https://api.example', timeoutMs: 3600001 } }, 'feed.timeoutMs'],
      [{ feed: { url: 'https://api.example', headers: { Accept: 1 } } }, 'feed.headers.Accept'],
      [{ feed: { url: 'https://api.example', when: 'later' } }, 'feed.when'],
      [Object.fromEntries(Array.from({ length: 9 }, (_, i) => [`request-${i}`, { url: 'https://api.example' }])), 'request-8.name'],
    ]) assert.throws(() => read(value), error => error.message.includes(`preload.${field}:`));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A deploy captures the app and the checkout keeping their relative places, so a generated workspace that named the
// checkout absolutely (an app under another top folder than the checkout's) led its capture back to the live tree.
test('a generated workspace names the checkout relatively, and is rewritten when the app moves', async () => {
  const { ensureShells } = await import('./app-shells.mjs');
  const { existsSync, mkdirSync, readFileSync, realpathSync, renameSync } = await import('node:fs');
  const { dirname, isAbsolute } = await import('node:path');
  const parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'manifest-relative-shell-')));
  const manifest = { name: 'Rel', app: { id: 'test.relative', name: 'Rel' }, host: { linux: {} }, deploy: { store: { web: '0', linux: '0' } } };
  // Each path the workspace and its Linux crate name, resolved where Cargo reads it.
  const paths = (dir) => {
    const shells = resolve(dir, '.exact/shells');
    const named = [[shells, 'Cargo.toml'], [resolve(shells, 'linux'), 'Cargo.toml']].flatMap(([at, file]) =>
      [...readFileSync(resolve(at, file), 'utf8').matchAll(/path = "([^"]+)"/g)].map((m) => [at, m[1]]));
    assert.ok(named.length > 0);
    for (const [at, path] of named) {
      assert.ok(!isAbsolute(path), path);
      assert.ok(existsSync(resolve(at, path)), path);
    }
    return named.map(([, path]) => path);
  };
  try {
    const first = resolve(parent, 'one');
    mkdirSync(first);
    writeFileSync(resolve(first, 'app.json'), JSON.stringify(manifest));
    ensureShells(first, manifest, 'rel');
    const before = paths(first);
    const moved = resolve(parent, 'deeper/two');
    mkdirSync(dirname(moved));
    renameSync(first, moved);
    assert.equal(ensureShells(moved, manifest, 'rel').wrote, true);
    assert.notDeepEqual(paths(moved), before);
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 60000);

test('adding and removing preload regenerates native shells with the baked accessor', async () => {
  const { ensureShells } = await import('./app-shells.mjs');
  const { readFileSync } = await import('node:fs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-preload-shell-'));
  const manifest = { name: 'Preload', app: { id: 'test.preload', name: 'Preload' }, host: { linux: {} }, deploy: { store: { web: '0', linux: '0' } } };
  const generate = () => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
    return ensureShells(dir, manifest, 'preload');
  };
  try {
    const initial = generate();
    const source = resolve(initial.workspace, 'linux/src/main.rs');
    const before = readFileSync(source, 'utf8');
    assert.ok(!before.includes('.with_preloads'));
    manifest.preload = { feed: { url: 'https://api.example' } };
    assert.equal(generate().wrote, true);
    assert.ok(readFileSync(source, 'utf8').includes('.with_preloads(PRELOAD)'));
    assert.equal(generate().wrote, false);
    delete manifest.preload;
    assert.equal(generate().wrote, true);
    assert.equal(readFileSync(source, 'utf8'), before);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30000);

test('Android may declare its own package without changing the shared app identity', async () => {
  const { readManifest, androidPackageId, nativeManifestMetadata } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-android-id-'));
  const save = id => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({
    name: 'Identity', app: { id: 'com.example.ios', name: 'Identity' }, host: { android: { id } },
  }));
  try {
    save('com.example.android_app');
    const manifest = readManifest(dir, 'identity');
    assert.equal(manifest.host.android.id, 'com.example.android_app');
    assert.equal(manifest.app.id, 'com.example.ios');
    assert.equal(androidPackageId(manifest), 'com.example.android_app');
    const { androidAssociation } = await import('../host/web/association.mjs');
    const certificates = [Array(32).fill('AB').join(':')];
    manifest.host.android.certificates = certificates;
    assert.deepEqual(androidAssociation(manifest), {
      relation: ['delegate_permission/common.get_login_creds'],
      target: { namespace: 'android_app', package_name: 'com.example.android_app', sha256_cert_fingerprints: certificates },
    });
    assert.equal(nativeManifestMetadata(manifest, 'android').host.id, 'com.example.android_app');
    assert.equal(nativeManifestMetadata(manifest, 'ios').app.id, 'com.example.ios');
    save(undefined);
    assert.equal(androidPackageId(readManifest(dir, 'identity')), 'com.example.ios');
    const shared = { app: { id: 'com.example.shared-app' } };
    assert.equal(androidPackageId(shared), 'com.example.shared_app');
    assert.equal(androidAssociation(shared).target.package_name, 'com.example.shared_app');
    for (const id of ['', 'single', 'com.example.1app', 'com.example.android-app', null, 42]) {
      save(id);
      assert.throws(() => readManifest(dir, 'identity'), /host.android.id/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the auth fixture origin associates the selected Android package and shared Apple identity', async () => {
  const { associationFiles } = await import('../apps/auth-fixture/origin.mjs');
  const certificates = [Array(32).fill('AB').join(':')];
  const manifest = {
    id: 'https://example.com/', app: { id: 'com.example.shared-app' },
    host: { ios: { team: 'TEAM' }, android: { id: 'com.example.android', certificates } },
  };
  for (const [id, expected] of [['com.example.android', 'com.example.android'], [undefined, 'com.example.shared_app']]) {
    manifest.host.android.id = id;
    const files = associationFiles(manifest);
    assert.deepEqual(files['assetlinks.json'], [{
      relation: ['delegate_permission/common.get_login_creds'],
      target: { namespace: 'android_app', package_name: expected, sha256_cert_fingerprints: certificates },
    }]);
    assert.deepEqual(files['apple-app-site-association'], {
      webcredentials: { apps: ['TEAM.com.example.shared-app', 'SIMULATORX.com.example.shared-app'] },
    });
  }
});

test('app.json variants: a suffix, a name, icons and settings per name, refused where wrong (LLP 1030.008)', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-variants-'));
  const save = (variants, extra = {}) => writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'V', app: { id: 'com.example.v', name: 'V' }, env: ['API'], variants, ...extra }));
  try {
    const dev = { idSuffix: '.dev', name: 'V Dev', env: { API: 'https://staging.example' }, host: { ios: { icon: 'assets/Dev.icon' }, macos: { icon: 'assets/Dev.icon' } } };
    save({ dev, 'qa-2': { idSuffix: '.qa.two', name: 'V QA' } });
    assert.deepEqual(readManifest(dir, 'v').variants.dev, dev);
    assert.equal(readManifest(dir, 'v').app.id, 'com.example.v');
    for (const [variants, problem, extra] of [
      [{ Dev: { idSuffix: '.dev', name: 'D' } }, /variants\.Dev\.name/], [{ dev: { idSuffix: 'dev', name: 'D' } }, /variants\.dev\.idSuffix/],
      [{ dev: { idSuffix: '.1dev', name: 'D' } }, /variants\.dev\.idSuffix/], [{ dev: { idSuffix: '.dev', name: '' } }, /variants\.dev\.name/],
      [{ dev: { name: 'D' } }, /variants\.dev: missing required "idSuffix"/], [{ dev: { idSuffix: '.dev' } }, /variants\.dev: missing required "name"/],
      [{ dev: { ...dev, env: { api: 'x' } } }, /variants\.dev\.env\.api\.name/], [{ dev: { ...dev, env: { API: 1 } } }, /variants\.dev\.env\.API/],
      [{ dev: { ...dev, env: { OTHER: 'x' } } }, /variants\.dev\.env\.OTHER: not a setting the manifest's "env" declares/],
      [{ dev: { ...dev, bundleId: 'x' } }, /variants\.dev\.bundleId: not a known key/],
      // Of `host`, a variant names its Icon Composer projects and nothing else.
      [{ dev: { ...dev, host: { ios: { icon: 'assets/Dev.icon', minimumOS: '18.0' } } } }, /variants\.dev\.host\.ios\.minimumOS: not a known key/],
      [{ dev: { ...dev, host: { android: { id: 'com.example.other' } } } }, /variants\.dev\.host\.android: not a known key/],
      [{ dev: { ...dev, host: { ios: {} } } }, /variants\.dev\.host\.ios: missing required "icon"/],
      [{ dev }, /variants\.dev\.host\.ios\.icon: assets\/Dev\.icon gives the primary icon the name of alternate_icons\[0\] \("dev"\) on Apple; rename one/, { alternate_icons: [{ name: 'dev', icons: [{ src: 'assets/missing.png' }] }] }],
      [{ dev: { ...dev, icons: [{ src: 'assets/missing.png' }] } }, /variants\.dev\.icons\[0\]\.src: assets\/missing\.png does not exist/],
      [{ dev }, /host\.ios\.appClip\.id com\.other\.Clip does not begin with com\.example\.v\./, { host: { ios: { appClip: { id: 'com.other.Clip' } } } }],
      [{ dev, qa: { idSuffix: '.dev', name: 'V QA' } }, /variants\.qa\.idSuffix: variants\.dev has \.dev too/],
      [{ dev: { ...dev, name: 'v' } }, /variants\.dev\.name: "v" installs under the same name as app\.name/],
      [{ dev, qa: { idSuffix: '.qa', name: 'V Dev.' } }, /variants\.qa\.name: "V Dev\." installs under the same name as variants\.dev\.name/],
      [{ dev }, /variant: not a known key/, { variant: { name: 'dev', base: 'com.example.v' } }],
    ]) {
      save(variants, extra);
      assert.throws(() => readManifest(dir, 'v'), problem);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a selected variant resolves the manifest as contract/cli/src/manifest/variant.rs does', async () => {
  const { resolveVariant, takeVariant } = await import('./variants.mjs');
  // The Rust tests' fixture.
  const manifest = {
    app: { id: 'com.example.v', name: 'V' }, icons: [{ src: 'assets/icon.png' }],
    color_scheme_dark: { background_color: '#000000', icons: [{ src: 'assets/dark.png' }] },
    host: { android: { id: 'com.example.v_android' }, ios: { appClip: { id: 'com.example.v.Clip' }, icon: 'assets/v.icon' }, macos: { icon: 'assets/v-mac.icon' } },
    variants: {
      dev: { idSuffix: '.dev', name: 'V Dev', icons: [{ src: 'assets/dev.png' }] }, plain: { idSuffix: '.qa.two', name: 'V QA' },
      project: { idSuffix: '.project', name: 'V Project', host: { ios: { icon: 'assets/project.icon' } } },
      whole: { idSuffix: '.whole', name: 'V Whole', icons: [{ src: 'assets/whole.png' }], host: { macos: { icon: 'assets/whole.icon' } } },
    },
  };
  assert.equal(resolveVariant(manifest, undefined), manifest);
  const dev = resolveVariant(manifest, 'dev');
  assert.deepEqual(dev.app, { id: 'com.example.v.dev', name: 'V Dev' });
  assert.equal(dev.host.android.id, 'com.example.v_android.dev');
  assert.deepEqual(dev.host.ios, { appClip: { id: 'com.example.v.dev.Clip' } });
  assert.deepEqual(dev.host.macos, {});
  assert.deepEqual(dev.icons, [{ src: 'assets/dev.png' }]);
  assert.deepEqual(dev.color_scheme_dark, { background_color: '#000000' });
  assert.deepEqual(dev.variant, { name: 'dev', base: 'com.example.v' });
  assert.equal(manifest.app.id, 'com.example.v', 'the manifest itself is left alone');
  const plain = resolveVariant(manifest, 'plain');
  assert.equal(plain.app.id, 'com.example.v.qa.two');
  assert.equal(plain.host.ios.icon, 'assets/v.icon');
  // A variant's own Icon Composer project replaces the app's on that platform, and nothing else.
  const project = resolveVariant(manifest, 'project');
  assert.deepEqual(project.host.ios, { appClip: { id: 'com.example.v.project.Clip' }, icon: 'assets/project.icon' });
  assert.deepEqual(project.host.macos, { icon: 'assets/v-mac.icon' });
  assert.deepEqual([project.icons, project.color_scheme_dark], [manifest.icons, manifest.color_scheme_dark]);
  // With `icons` too, the app's projects are not read, and the variant's own is.
  const whole = resolveVariant(manifest, 'whole');
  assert.deepEqual(whole.icons, [{ src: 'assets/whole.png' }]);
  assert.deepEqual(whole.color_scheme_dark, { background_color: '#000000' });
  assert.deepEqual(whole.host.ios, { appClip: { id: 'com.example.v.whole.Clip' } });
  assert.deepEqual(whole.host.macos, { icon: 'assets/whole.icon' });
  assert.equal(manifest.host.ios.icon, 'assets/v.icon', 'the manifest itself is left alone');
  assert.throws(() => resolveVariant(manifest, 'preview'), /^Error: variant preview: app\.json has no variants\.preview \(it declares dev, plain, project, whole\)$/);
  // The flag sets EXACT_VARIANT for this process and its children, over an exported value.
  const env = { EXACT_VARIANT: 'plain' }, args = ['--run', '--variant', 'dev', 'shelf'];
  assert.deepEqual(takeVariant(args, env), ['--run', 'shelf']);
  assert.equal(env.EXACT_VARIANT, 'dev');
  assert.throws(() => takeVariant(['--variant'], {}), /--variant needs a name/);
  assert.equal(resolveVariant(dev, 'dev'), dev, 'a resolved manifest is never resolved again');
});

test('a variant selection names the one app the verb resolved; another app reads as it is', async () => {
  const { claimVariant, selectedVariant } = await import('./variants.mjs');
  const one = mkdtempSync(resolve(tmpdir(), 'variant-one-')), other = mkdtempSync(resolve(tmpdir(), 'variant-other-'));
  try {
    const env = { EXACT_VARIANT: 'dev' };
    assert.equal(selectedVariant(one, env), undefined, 'a request alone selects nothing');
    claimVariant(one, () => ({}), env);
    claimVariant(other, () => ({}), env);
    assert.equal(selectedVariant(one, env), 'dev');
    assert.equal(selectedVariant(other, env), undefined);
    assert.equal(selectedVariant(one, {}), undefined);
  } finally { for (const dir of [one, other]) rmSync(dir, { recursive: true, force: true }); }
});

test('controls name a tool or a url, with a title and a symbol (LLP 1126.004 D1)', async () => {
  const {validate} = await import('./app.mjs');
  const schema = await Bun.file(new URL('./app.schema.json', import.meta.url)).json();
  const controls = schema.properties.controls;
  const check = (rows) => validate(rows, controls, 'controls', schema);
  assert.deepEqual(check([
    { kind: 'record', title: 'Record an idea', symbol: 'mic.fill', tool: 'startRecording', authentication: 'always' },
    { kind: 'open-inbox', title: 'Inbox', symbol: 'tray', url: '/inbox' },
  ]), []);
  for (const bad of [
    { title: 'Record', symbol: 'mic.fill', tool: 't' },
    { kind: 'a b', title: 'Record', symbol: 'mic.fill', tool: 't' },
    { kind: 'r', symbol: 'mic.fill', tool: 't' },
    { kind: 'r', title: ' ', symbol: 'mic.fill', tool: 't' },
    { kind: 'r', title: 'R', symbol: 'mic.fill', url: '//elsewhere' },
    { kind: 'r', title: 'R', symbol: 'mic.fill', tool: 't', authentication: 'never' },
    { kind: 'r', title: 'R', symbol: 'mic.fill', tool: 't', arguments: {} },
    { kind: 'r', title: 'R', symbol: 'mic.fill' },
    { kind: 'r', title: 'R', symbol: 'mic.fill', tool: 't', url: '/' },
    { kind: 'r', title: 'R', symbol: 'mic.fill', url: '/', authentication: 'always' },
  ]) assert.ok(check([bad]).length, JSON.stringify(bad));
});

test('the web manifest keeps a file handler’s file types and drops only its folder type', async () => {
  const { webAppManifest } = await import('./app.mjs');
  const file_handlers = [
    { action: '/', name: 'Documents', accept: { 'application/pdf': ['.pdf'], 'inode/directory': [] } },
    { action: '/', name: 'Folders', accept: { 'inode/directory': [] } },
  ];
  assert.deepEqual(webAppManifest({ file_handlers }, 'Docs').file_handlers,
    [{ action: '/', name: 'Documents', accept: { 'application/pdf': ['.pdf'] } }]);
});
