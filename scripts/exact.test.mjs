// `exact release` signs every nested Mach-O file and bundle, innermost first (#119); `exact contract build`
// refreshes an app's types (LLP 1116.001 D4.5).
// `bun test ./scripts/exact.test.mjs`.
import { test, spyOn } from 'bun:test';
import * as childProcess from 'node:child_process';
import { appVerbs } from './app-cli.mjs';
import { pathFrom } from './app.mjs';
import { encodePng } from './png.mjs';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, relative, resolve } from 'node:path';
import { codeDigest, signingOrder, signRelease, signedGpuProduct, assertArchiveFrameworks, plistFile, wrapFramework } from '../host/apple/assets.mjs';
import { appleEmbedOnly, copyAppleGpuProducts, captureAppleProduct, stripForDistribution, stripProductionExecutable } from '../host/apple/build.mjs';
import { assertLinkedSdk } from '../host/apple/link.mjs';
import { packageWidgets, packageAlarmExtension } from '../host/apple/widgets.mjs';

const plist = (executable) => `<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>${executable}</string></dict></plist>\n`;

function inDir(body) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-release-'));
  try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('the signing order holds every Mach-O by its magic and each nested bundle after its contents', () => inDir((dir) => {
  const app = resolve(dir, 'Fixture.app');
  const put = (path, bytes) => { mkdirSync(dirname(resolve(app, path)), { recursive: true }); writeFileSync(resolve(app, path), bytes); };
  const word = (...words) => Buffer.concat(words.map((w) => { const b = Buffer.alloc(4); b.writeUInt32BE(w >>> 0); return b; }));
  put('Contents/Info.plist', plist('ExactMac'));
  put('Contents/MacOS/ExactMac', word(0xcffaedfe, 0x0c000001)); // the main executable: the bundle's own signature
  put('Contents/MacOS/libexact_web.dylib', word(0xcffaedfe, 0x0c000001));
  put('Contents/MacOS/spawn-helper', word(0xcafebabe, 2)); // universal, two architectures
  put('Contents/Resources/assets/helper', word(0xcffaedfe, 0x0c000001)); // no extension to go by
  put('Contents/Resources/assets/addon.node', word(0xcefaedfe, 7)); // 32-bit
  put('Contents/Resources/assets/Big.class', word(0xcafebabe, 0x00000034)); // Java: the fat magic, a class version
  put('Contents/Resources/assets/notes.dylib', 'not code\n'); // a name is not a format
  put('Contents/Resources/assets/plain.bundle/note.txt', 'not a code bundle\n');
  put('Contents/Resources/assets/short', Buffer.from([0xcf, 0xfa]));
  put('Contents/Helpers/Inner.app/Contents/Info.plist', plist('Inner'));
  put('Contents/Helpers/Inner.app/Contents/MacOS/Inner', word(0xfeedfacf, 0x0100000c));
  put('Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework/Versions/A/Kit', word(0xcffaedfe, 1)); // the framework's own
  put('Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework/Versions/A/Resources/Info.plist', plist('Kit'));
  put('Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework/Versions/A/XPCServices/Svc.xpc/Contents/Info.plist', plist('Svc'));
  put('Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework/Versions/A/XPCServices/Svc.xpc/Contents/MacOS/Svc', word(0xcffaedfe, 2));
  symlinkSync('A', resolve(app, 'Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework/Versions/Current'));
  symlinkSync('Versions/Current/Kit', resolve(app, 'Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework/Kit'));

  const order = signingOrder(app).map((path) => relative(dir, path));
  assert.deepEqual([...order].sort(), [
    'Fixture.app',
    'Fixture.app/Contents/Helpers/Inner.app',
    'Fixture.app/Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework',
    'Fixture.app/Contents/Helpers/Inner.app/Contents/Frameworks/Kit.framework/Versions/A/XPCServices/Svc.xpc',
    'Fixture.app/Contents/MacOS/libexact_web.dylib',
    'Fixture.app/Contents/MacOS/spawn-helper',
    'Fixture.app/Contents/Resources/assets/addon.node',
    'Fixture.app/Contents/Resources/assets/helper',
  ]);
  // Innermost first: everything a bundle holds is signed before the bundle.
  for (const [i, path] of order.entries()) {
    for (const later of order.slice(i + 1)) assert.ok(!later.startsWith(`${path}/`), `${later} is signed after its container ${path}`);
  }
  assert.equal(order.at(-1), 'Fixture.app');
}));

const tools = process.platform === 'darwin' && ['clang', 'codesign'].every((tool) => Bun.which(tool));

test.skipIf(!tools)('release signs an accented main executable only after its nested code', () => inDir(dir => {
  const bundle = resolve(dir, 'Cafe\u0301.app'), binary = resolve(bundle, 'Contents/MacOS/Cafe\u0301');
  const library = resolve(bundle, 'Contents/MacOS/libnested.dylib');
  mkdirSync(dirname(binary), { recursive: true });
  writeFileSync(binary, readFileSync('/usr/bin/true'), { mode: 0o755 });
  writeFileSync(resolve(dir, 'nested.c'), 'void nested(void) {}');
  const built = spawnSync('clang', ['-dynamiclib', resolve(dir, 'nested.c'), '-o', library], { encoding: 'utf8' });
  assert.equal(built.status, 0, built.stderr);
  for (const form of ['NFD', 'NFC']) {
    writeFileSync(resolve(bundle, 'Contents/Info.plist'), plistFile({ CFBundleIdentifier: 'dev.exact.accent', CFBundleExecutable: 'Café'.normalize(form), CFBundlePackageType: 'APPL' }));
    const signed = [], spawn = childProcess.spawnSync;
    const commands = spyOn(childProcess, 'spawnSync').mockImplementation((cmd, args, options) => {
      if (cmd === 'codesign') signed.push(args.at(-1));
      return spawn(cmd, args, options);
    });
    try { signRelease(bundle, { manifest: {}, identity: '-', id: 'dev.exact.accent', scratch: dir, timestamp: ['--timestamp=none'] }); } finally { commands.mockRestore(); }
    assert.deepEqual(signed, [library, bundle]);
    const verified = spawnSync('codesign', ['--verify', '--deep', '--strict', bundle], { encoding: 'utf8' });
    assert.equal(verified.status, 0, verified.stderr);
  }
}));

test.skipIf(!tools)('widget packaging preserves parent platform and device capabilities in device and simulator extensions', () => inDir(dir => {
  const library = resolve(dir, 'fixture.dylib');
  writeFileSync(resolve(dir, 'fixture.c'), 'void fixture(void) {}');
  const compiled = spawnSync('clang', ['-dynamiclib', resolve(dir, 'fixture.c'), '-o', library], { encoding: 'utf8' });
  assert.equal(compiled.status, 0, compiled.stderr);
  const readPlist = path => {
    const result = spawnSync('plutil', ['-convert', 'json', '-o', '-', path], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  };
  for (const device of [true, false]) {
    const platform = device ? 'iphoneos' : 'iphonesimulator';
    const bundle = resolve(dir, platform, 'Fixture.app'), directory = resolve(dir, platform, 'build');
    const framework = resolve(bundle, 'Frameworks/ExactCore.framework');
    mkdirSync(framework, { recursive: true });
    const parent = { CFBundleVersion: '8', CFBundleShortVersionString: '1.0.0', MinimumOSVersion: '17.0',
      UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: [device ? 'iPhoneOS' : 'iPhoneSimulator'], DTPlatformName: platform, ExactWidgetGeneration: 'fixture',
      ...(device ? { UIRequiredDeviceCapabilities: ['arm64'] } : {}) };
    writeFileSync(resolve(bundle, 'Info.plist'), plistFile(parent));
    writeFileSync(resolve(framework, 'Info.plist'), plistFile({ CFBundleIdentifier: 'dev.exact.fixture.runtime' }));
    let signed = null;
    packageWidgets({ app: { dir, id: 'dev.exact.fixture', displayName: 'Fixture', manifest: {} }, bundle,
      shared: { framework, snapshotArchive: resolve(dir, 'snapshot.a') }, directory, sdk: 'fixture-sdk', triple: 'fixture-target',
      device, production: true, entitlements: plistFile({}),
      runApple(command, args) {
        if (command === 'plutil') return { stdout: JSON.stringify(readPlist(args.at(-1))) };
        assert.equal(command, 'xcrun');
        assert.equal(args[2], 'swiftc');
        const output = args[args.indexOf('-o') + 1];
        cpSync(library, output);
        if (args.includes('-g')) {
          const symbols = spawnSync('dsymutil', [output, '-o', `${output}.dSYM`], { encoding: 'utf8' });
          assert.equal(symbols.status, 0, symbols.stderr);
        }
        return { stdout: '' };
      }, sign(extension) { signed = extension; } });
    assert.equal(signed, resolve(bundle, 'PlugIns/ExactWidgets.appex'));
    const info = readPlist(resolve(signed, 'Info.plist'));
    assert.equal(info.DTPlatformName, platform);
    assert.deepEqual(info.UIRequiredDeviceCapabilities, parent.UIRequiredDeviceCapabilities);
    for (const name of ['ExactCore', 'ExactWidgetReload']) {
      assert.deepEqual(readPlist(resolve(bundle, `Frameworks/${name}.framework/Info.plist`)).UIRequiredDeviceCapabilities, parent.UIRequiredDeviceCapabilities, name);
    }
    assert.deepEqual(info.CFBundleSupportedPlatforms, parent.CFBundleSupportedPlatforms);
    assert.equal(info.CFBundleVersion, parent.CFBundleVersion);
  }
}));
for (const device of [true, false]) {
  const platform = device ? 'iphoneos' : 'iphonesimulator';
  for (const [shape, metadata] of [
    ['array', { DTPlatformName: platform, UIRequiredDeviceCapabilities: ['arm64', 'gps'] }],
    ['dictionary', { DTPlatformName: platform, UIRequiredDeviceCapabilities: { arm64: true, gps: false } }],
    ['empty array', { DTPlatformName: platform, UIRequiredDeviceCapabilities: [] }],
    ['empty dictionary', { DTPlatformName: platform, UIRequiredDeviceCapabilities: {} }],
    ['omitted capabilities', { DTPlatformName: platform }],
    ['omitted platform', { UIRequiredDeviceCapabilities: { arm64: true, gps: false } }],
    ['omitted keys', {}],
  ]) test.skipIf(process.platform !== 'darwin')(`alarm-only ${platform} extension preserves ${shape} parent metadata`, () => inDir(dir => {
    const bundle = resolve(dir, 'Fixture.app'), directory = resolve(dir, 'build');
    mkdirSync(bundle);
    mkdirSync(directory);
    const parent = { CFBundleVersion: '8', CFBundleShortVersionString: '1.0.0', MinimumOSVersion: '17.0',
      UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: [device ? 'iPhoneOS' : 'iPhoneSimulator'], ...metadata };
    writeFileSync(resolve(bundle, 'Info.plist'), plistFile(parent));
    const readPlist = path => {
      const result = spawnSync('plutil', ['-convert', 'json', '-o', '-', path], { encoding: 'utf8' });
      assert.equal(result.status, 0, result.stderr);
      return JSON.parse(result.stdout);
    };
    let signed = null;
    packageAlarmExtension({ app: { dir, id: 'dev.exact.fixture', displayName: 'Fixture', manifest: {} }, bundle,
      directory, sdk: 'fixture-sdk', triple: 'fixture-target', device, entitlements: plistFile({}), alarm: 'ExactAlarms',
      runApple(command, args) {
        if (command === 'plutil') return { stdout: JSON.stringify(readPlist(args.at(-1))) };
        assert.equal(command, 'xcrun');
        return { stdout: '' };
      }, sign(extension) { signed = extension; } });
    assert.equal(signed, resolve(bundle, 'PlugIns/ExactWidgets.appex'));
    const info = readPlist(resolve(signed, 'Info.plist'));
    for (const key of ['DTPlatformName', 'UIRequiredDeviceCapabilities', 'CFBundleSupportedPlatforms',
      'CFBundleVersion', 'CFBundleShortVersionString', 'MinimumOSVersion', 'UIDeviceFamily']) {
      assert.equal(Object.hasOwn(info, key), Object.hasOwn(parent, key), key);
      assert.deepEqual(info[key], parent[key], key);
    }
    assert.equal(existsSync(resolve(bundle, 'Frameworks')), false);
  }));
}

test('release refuses code in content-addressed assets before signing changes its digest', () => inDir(dir => {
  const app = resolve(dir, 'Fixture.app'), helper = resolve(app, 'Contents/Resources/assets/helper');
  mkdirSync(dirname(helper), { recursive: true });
  const bytes = Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1]);
  writeFileSync(helper, bytes);
  const compat = { embedded: { assets: [{ name: 'assets/helper', sha256: createHash('sha256').update(bytes).digest('hex') }] } };
  assert.throws(() => signingOrder(app, compat), /assets\/helper.*host.macos.resources/);
}));

test.skipIf(!tools)('release preserves baked GPU signatures and rejects changed products', () => inDir(dir => {
  const app = resolve(dir, 'Fixture.app'), executable = resolve(app, 'Contents/MacOS/Fixture');
  mkdirSync(dirname(executable), { recursive: true });
  writeFileSync(resolve(dir, 'main.c'), 'int main(void){return 0;}\n');
  assert.equal(spawnSync('clang', [resolve(dir, 'main.c'), '-o', executable]).status, 0);
  writeFileSync(resolve(app, 'Contents/Info.plist'), plist('Fixture'));
  const product = resolve(dir, 'gpu.dylib');
  assert.equal(spawnSync('clang', ['-dynamiclib', resolve(dir, 'main.c'), '-o', product]).status, 0);
  const gpu = resolve(app, 'Contents/MacOS/libexact_gpu.dylib');
  cpSync(signedGpuProduct(product, { hardened: true }), gpu);
  const digest = createHash('sha256').update(readFileSync(gpu)).digest('hex');
  const named = resolve(app, 'Contents/MacOS/libexact_gpu_title_sky.dylib');
  cpSync(gpu, named);
  const rust = resolve(app, 'Contents/Resources/rust/module.dylib');
  mkdirSync(dirname(rust), { recursive: true }); cpSync(gpu, rust);
  const code = codeDigest(readFileSync(gpu));
  assert.notEqual(code, digest);
  const compat = { embedded: { assets: [{ name: 'rust/module.dylib', sha256: digest }], gpu: { sha256: code }, gpuModules: { 'title-sky': { sha256: code } } } };
  assert.deepEqual(signingOrder(app, compat), [app]);
  for (const path of signingOrder(app, compat)) assert.equal(spawnSync('codesign', ['--force', '--sign', '-', '--options', 'runtime', '--timestamp=none', path]).status, 0);
  assert.equal(spawnSync('codesign', ['--verify', '--deep', '--strict', app]).status, 0);
  assert.equal(createHash('sha256').update(readFileSync(gpu)).digest('hex'), digest);
  // A re-signer (AppDrop) keeps the GPU card's digest; a change to the code does not.
  const resigned = resolve(dir, 'resigned.dylib');
  cpSync(gpu, resigned);
  writeFileSync(resolve(dir, 'ent.plist'), plist('Fixture').replace('<dict>', '<dict><key>get-task-allow</key><true/><key>application-identifier</key><string>ABCDE12345.a.longer.identifier.than.before</string>'));
  assert.equal(spawnSync('codesign', ['--force', '--sign', '-', '--identifier', 'a.recipient.identifier.that.is.much.longer', '--entitlements', resolve(dir, 'ent.plist'), '--timestamp=none', resigned]).status, 0);
  assert.notEqual(createHash('sha256').update(readFileSync(resigned)).digest('hex'), digest);
  assert.equal(codeDigest(readFileSync(resigned)), code);
  const edited = readFileSync(resigned); edited[64] ^= 1;
  assert.notEqual(codeDigest(edited), code);
  assert.match(spawnSync('codesign', ['-dv', gpu], { encoding: 'utf8' }).stderr, /runtime/);
  const development = signedGpuProduct(product);
  assert.doesNotMatch(spawnSync('codesign', ['-dv', development], { encoding: 'utf8' }).stderr, /runtime/);
  assert.notEqual(createHash('sha256').update(readFileSync(development)).digest('hex'), digest, 'the signing mode invalidates the cached copy');
  const changed = readFileSync(gpu); changed[64] ^= 1;
  writeFileSync(gpu, changed);
  assert.throws(() => signingOrder(app, compat), /baked code.*digest/);
}), 60000);

test.skipIf(!tools)('signed GPU framework upgrades refresh metadata before archive capture', () => inDir(dir => {
  const run = (command, args) => {
    const result = spawnSync(command, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout;
  };
  const app = { id: 'test.gpu', manifest: {} }, name = 'ExactGpu';
  const source = resolve(dir, 'gpu.c'), product = resolve(dir, 'gpu.dylib');
  writeFileSync(source, 'int gpu_fixture(void){return 42;}\n');
  run('xcrun', ['--sdk', 'iphoneos', 'clang', '-target', 'arm64-apple-ios17.0', '-dynamiclib', source, '-o', product]);
  const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
  const sourceDigest = hash(product), options = { app, framework: name, platform: 'iPhoneOS' };
  const frameworks = resolve(dir, 'signed/frameworks/gpu.dylib'), container = resolve(frameworks, `${name}.framework`);
  mkdirSync(frameworks, { recursive: true });
  cpSync(product, resolve(frameworks, 'gpu.dylib'));
  wrapFramework(frameworks, 'gpu.dylib', name, app, options.platform);
  const info = resolve(container, 'Info.plist'), record = `${container}.source`;
  const oldInfo = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', info]));
  delete oldInfo.UIRequiredDeviceCapabilities;
  writeFileSync(info, plistFile(oldInfo));
  run('codesign', ['--force', '--sign', '-', '--timestamp=none', container]);
  run('codesign', ['--verify', '--strict', container]);
  // The cache key from before the plist was the key: the metadata's inputs, not what was written.
  const oldKey = JSON.stringify([sourceDigest, '-', false, name,
    [app.id, '17.0', options.platform, process.env.EXACT_BUILD_NUMBER ?? '1', process.env.EXACT_VERSION ?? '0.1.0']]);
  writeFileSync(record, oldKey);
  const code = codeDigest(readFileSync(resolve(container, name)));
  const signed = signedGpuProduct(product, options);
  assert.equal(signed, resolve(container, name));
  const updated = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', info]));
  assert.deepEqual(updated, { ...oldInfo, UIRequiredDeviceCapabilities: ['arm64'] });
  assert.notEqual(readFileSync(record, 'utf8'), oldKey);
  assert.equal(hash(product), sourceDigest, 'a packaging upgrade leaves Cargo output unchanged');
  assert.equal(codeDigest(readFileSync(signed)), code, 'a packaging upgrade preserves the baked code digest');
  run('codesign', ['--verify', '--strict', container]);
  const digest = hash(signed), inode = statSync(signed).ino;
  assert.equal(signedGpuProduct(product, options), signed);
  assert.equal(statSync(signed).ino, inode, 'the upgraded warm cache reuses the signed file');
  assert.equal(hash(signed), digest);
  const capture = resolve(dir, 'capture'), captured = resolve(capture, `${name}.framework`, name);
  cpSync(container, dirname(captured), { recursive: true });
  rmSync(captured);
  captureAppleProduct({ products: [{ path: signed, bytes: statSync(signed).size, sha256: digest }] }, signed, captured);
  const packaged = resolve(dir, 'Payload/Fixture.app/Frameworks');
  mkdirSync(packaged, { recursive: true });
  copyAppleGpuProducts(capture, packaged, [{ framework: name }], true);
  assertArchiveFrameworks(packaged);
  const copied = resolve(packaged, `${name}.framework`);
  assert.deepEqual(JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', resolve(copied, 'Info.plist')])), updated);
  assert.equal(hash(resolve(copied, name)), digest, 'the archive keeps the captured signed bytes');
  run('codesign', ['--verify', '--deep', '--strict', copied]);
}), 60000);

test('--archive --embed does not stop after embed staging; plain --embed builds no Swift', () => {
  assert.equal(appleEmbedOnly(['--device', '--embed']), true);
  for (const extra of [['--archive', 'out.ipa'], ['--run'], ['--host']])
    assert.equal(appleEmbedOnly(['--device', '--embed', ...extra]), false);
  assert.equal(appleEmbedOnly(['--device', '--archive', 'out.ipa']), false);
});

test.skipIf(!tools)('GPU archives hash the signed framework binary, including declared modules', () => inDir(dir => {
  const app = { id: 'test.gpu', manifest: {} };
  const digests = new Map();
  const source = resolve(dir, 'gpu.c');
  writeFileSync(source, 'int gpu_fixture(void){return 42;}\n');
  for (const name of ['ExactGpu', 'ExactGpu_title_sky']) {
    const product = resolve(dir, `${name}.dylib`);
    const cc = spawnSync('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-target', 'arm64-apple-ios17.0-simulator', '-dynamiclib', source, '-o', product], { encoding: 'utf8' });
    assert.equal(cc.status, 0, cc.stderr);
    const options = { app, framework: name, platform: 'iPhoneSimulator' };
    const signed = signedGpuProduct(product, options);
    assert.equal(signed, resolve(dir, 'signed/frameworks', `${name}.dylib`, `${name}.framework`, name));
    const hash = () => createHash('sha256').update(readFileSync(signed)).digest('hex');
    const digest = hash();
    digests.set(name, digest);
    const info = spawnSync('plutil', ['-extract', 'CFBundleIdentifier', 'raw', '-o', '-', resolve(dirname(signed), 'Info.plist')], { encoding: 'utf8' });
    assert.equal(info.stdout.trim(), `test.gpu.${name.toLowerCase().replaceAll('_', '-')}`);
    const installName = spawnSync('otool', ['-D', signed], { encoding: 'utf8' });
    assert.equal(installName.status, 0, installName.stderr);
    assert.ok(installName.stdout.includes(`@rpath/${name}.framework/${name}`));
    const verify = spawnSync('codesign', ['--verify', '--deep', '--strict', dirname(signed)], { encoding: 'utf8' });
    assert.equal(verify.status, 0, verify.stderr);
    assert.equal(signedGpuProduct(product, options), signed);
    assert.equal(hash(), digest, 'a warm build preserves the baked bytes');
    const otherProduct = resolve(dir, `other-${name}.dylib`);
    cpSync(product, otherProduct);
    const other = signedGpuProduct(otherProduct, { ...options, app: { ...app, id: 'test.other' } });
    assert.notEqual(other, signed, 'apps sharing a Cargo output directory keep separate signed frameworks');
    assert.equal(hash(), digest);
    const packaged = resolve(dir, 'Payload', 'Fixture.app', 'Frameworks');
    mkdirSync(packaged, { recursive: true });
    const capture = resolve(dir, 'capture');
    const captured = resolve(capture, `${name}.framework`, name);
    cpSync(dirname(signed), dirname(captured), { recursive: true });
    rmSync(captured);
    captureAppleProduct({ products: [{ path: signed, bytes: statSync(signed).size, sha256: digest }] }, signed, captured);
    const artifact = { built: `${name}.dylib`, load: name === 'ExactGpu' ? 'libexact_gpu.dylib' : 'libexact_gpu_title_sky.dylib', framework: name };
    const embed = resolve(dir, 'embed');
    mkdirSync(embed, { recursive: true });
    copyAppleGpuProducts(capture, embed, [artifact], true);
    assert.equal(createHash('sha256').update(readFileSync(resolve(embed, `${name}.framework`, name))).digest('hex'), digest);
    assert.equal(spawnSync('codesign', ['--verify', '--deep', '--strict', resolve(embed, `${name}.framework`)]).status, 0);
    assertArchiveFrameworks(embed);
    copyAppleGpuProducts(capture, packaged, [artifact], true);
    assertArchiveFrameworks(packaged);
    assert.equal(createHash('sha256').update(readFileSync(resolve(packaged, `${name}.framework`, name))).digest('hex'), digest);
    const copied = spawnSync('codesign', ['--verify', '--deep', '--strict', resolve(packaged, `${name}.framework`)], { encoding: 'utf8' });
    assert.equal(copied.status, 0, copied.stderr);
    const development = signedGpuProduct(product);
    assert.equal(development, resolve(dir, 'signed', `${name}.dylib`));
    assert.notEqual(createHash('sha256').update(readFileSync(development)).digest('hex'), digest);
    const developmentCapture = resolve(dir, 'development-capture'), developmentEmbed = resolve(dir, 'development-embed');
    mkdirSync(developmentCapture, { recursive: true }); mkdirSync(developmentEmbed, { recursive: true });
    cpSync(development, resolve(developmentCapture, artifact.built));
    copyAppleGpuProducts(developmentCapture, developmentEmbed, [artifact]);
    assert.deepEqual(readFileSync(resolve(developmentEmbed, artifact.load)), readFileSync(development));
    cpSync(development, resolve(packaged, 'libexact_gpu.dylib'));
    assert.throws(() => assertArchiveFrameworks(packaged), /loose libraries.*libexact_gpu.dylib/);
    rmSync(resolve(packaged, 'libexact_gpu.dylib'));
  }
  const bundle = resolve(dir, 'Payload/Fixture.app');
  writeFileSync(resolve(dir, 'main.c'), 'int main(void){return 0;}\n');
  const cc = spawnSync('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-target', 'arm64-apple-ios17.0-simulator', resolve(dir, 'main.c'), '-o', resolve(bundle, 'Fixture')], { encoding: 'utf8' });
  assert.equal(cc.status, 0, cc.stderr);
  writeFileSync(resolve(bundle, 'Info.plist'), plist('Fixture').replace('</dict>', '<key>CFBundleIdentifier</key><string>test.gpu</string><key>CFBundlePackageType</key><string>APPL</string></dict>'));
  assert.equal(spawnSync('codesign', ['--force', '--sign', '-', '--timestamp=none', bundle]).status, 0);
  const verify = spawnSync('codesign', ['--verify', '--deep', '--strict', bundle], { encoding: 'utf8' });
  assert.equal(verify.status, 0, verify.stderr);
  for (const [name, digest] of digests)
    assert.equal(createHash('sha256').update(readFileSync(resolve(bundle, 'Frameworks', `${name}.framework`, name))).digest('hex'), digest, 'the final app seal preserves the baked GPU bytes');
}), 60000);

test.skipIf(!tools)('signing in that order seals a bundle with an unsigned helper and an unsigned nested app', () => inDir((dir) => {
  const app = resolve(dir, 'Fixture.app');
  const source = resolve(dir, 'main.c');
  writeFileSync(source, 'int main(void){return 0;}\n');
  const cc = (out, ...flags) => {
    mkdirSync(dirname(resolve(app, out)), { recursive: true });
    const r = spawnSync('clang', [...flags, '-o', resolve(app, out), source], { encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
  };
  cc('Contents/MacOS/ExactMac');
  writeFileSync(resolve(app, 'Contents/Info.plist'), plist('ExactMac'));
  cc('Contents/Resources/assets/helper', '-Wl,-no_adhoc_codesign');
  cc('Contents/Helpers/Inner.app/Contents/MacOS/Inner', '-Wl,-no_adhoc_codesign');
  writeFileSync(resolve(app, 'Contents/Helpers/Inner.app/Contents/Info.plist'), plist('Inner'));
  // A binary plist, as Xcode writes one: its executable is still the bundle's own.
  assert.equal(spawnSync('plutil', ['-convert', 'binary1', resolve(app, 'Contents/Helpers/Inner.app/Contents/Info.plist')]).status, 0);
  // A versioned framework holding an unsigned XPC service: signing the
  // framework's executable on its own would seal it before the service.
  const kit = 'Contents/Frameworks/Kit.framework';
  cc(`${kit}/Versions/A/Kit`, '-dynamiclib', '-Wl,-no_adhoc_codesign');
  mkdirSync(resolve(app, kit, 'Versions/A/Resources'), { recursive: true });
  writeFileSync(resolve(app, kit, 'Versions/A/Resources/Info.plist'), plist('Kit').replace('</dict>', '<key>CFBundleIdentifier</key><string>dev.exact.kit</string><key>CFBundlePackageType</key><string>FMWK</string></dict>'));
  cc(`${kit}/Versions/A/XPCServices/Svc.xpc/Contents/MacOS/Svc`, '-Wl,-no_adhoc_codesign');
  writeFileSync(resolve(app, kit, 'Versions/A/XPCServices/Svc.xpc/Contents/Info.plist'), plist('Svc'));
  symlinkSync('A', resolve(app, kit, 'Versions/Current'));
  for (const link of ['Kit', 'Resources', 'XPCServices']) symlinkSync(`Versions/Current/${link}`, resolve(app, kit, link));

  for (const path of signingOrder(app)) {
    const r = spawnSync('codesign', ['--force', '--sign', '-', '--options', 'runtime', path], { encoding: 'utf8' });
    assert.equal(r.status, 0, `${path}: ${r.stderr}`);
  }
  const verify = spawnSync('codesign', ['--verify', '--deep', '--strict', app], { encoding: 'utf8' });
  assert.equal(verify.status, 0, verify.stderr);
  const helper = spawnSync('codesign', ['-dv', resolve(app, 'Contents/Resources/assets/helper')], { encoding: 'utf8' });
  assert.match(helper.stderr, /flags=0x10002\(adhoc,runtime\)/);
}), 60000);

// LLP 1069.008 D9: a runtime grant's entitlement reaches the one executable it names.
test.skipIf(!tools)('release signs each helper a runtime grant names with exactly its exceptions, and other code with none', () => inDir((dir) => {
  const app = resolve(dir, 'Fixture.app');
  // Exits 0 when the hardened runtime lets it map JIT memory, 1 when it refuses.
  writeFileSync(resolve(dir, 'jit.c'), '#include <sys/mman.h>\nint main(void){return mmap(0,16384,PROT_READ|PROT_WRITE|PROT_EXEC,MAP_PRIVATE|MAP_ANON|MAP_JIT,-1,0)==MAP_FAILED;}\n');
  const cc = (out, ...flags) => {
    mkdirSync(dirname(resolve(app, out)), { recursive: true });
    const r = spawnSync('clang', [...flags, '-o', resolve(app, out), resolve(dir, 'jit.c')], { encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
  };
  cc('Contents/MacOS/Fixture');
  writeFileSync(resolve(app, 'Contents/Info.plist'), plist('Fixture'));
  cc('Contents/Helpers/node/bin/node', '-Wl,-no_adhoc_codesign');
  cc('Contents/Helpers/node/bin/npx', '-Wl,-no_adhoc_codesign');
  cc('Contents/Helpers/node/lib/addon.dylib', '-dynamiclib', '-Wl,-no_adhoc_codesign');
  cc('Contents/Helpers/Tool.app/Contents/MacOS/Tool', '-Wl,-no_adhoc_codesign');
  writeFileSync(resolve(app, 'Contents/Helpers/Tool.app/Contents/Info.plist'), plist('Tool'));
  cc('Contents/Helpers/Tool.app/Contents/MacOS/sub', '-Wl,-no_adhoc_codesign'); // a second executable in a granted bundle
  // A name the plist spells with an entity: the walk's quick read takes `A&amp;B`, Foundation and codesign `A&B`.
  cc('Contents/Helpers/Amp.app/Contents/MacOS/A&B', '-Wl,-no_adhoc_codesign');
  writeFileSync(resolve(app, 'Contents/Helpers/Amp.app/Contents/Info.plist'), plist('A&amp;B'));
  // A staging file the resources tree carries; codesign takes only code under Helpers/, so data goes under Resources/.
  mkdirSync(resolve(app, 'Contents/Resources/stamp'), { recursive: true });
  writeFileSync(resolve(app, 'Contents/Resources/stamp/channel'), 'beta\n');
  const manifest = { host: { macos: { resources: [{ from: 'node', to: 'Helpers/node' }, { from: 'tool', to: 'Helpers/Tool.app' }, { from: 'stamp', to: 'Resources/stamp' }, { from: 'trick', to: 'Helpers/Trick.app' },
    { from: 'amp', to: 'Helpers/Amp.app' }, { from: 'plat', to: 'Helpers/Plat.app' }] } } };
  const outside = `is not inside a host.macos.resources destination (${manifest.host.macos.resources.map(({ to }) => to).join(', ')})`;
  const [jit, memory, library] = ['allow-jit', 'allow-unsigned-executable-memory', 'disable-library-validation'].map((name) => `com.apple.security.cs.${name}`);
  const microphone = 'com.apple.security.device.audio-input'; // the app's own, from a device grant
  const compat = { reach: { entitlements: [microphone], helperEntitlements: {
    'Helpers/node/bin/node': [jit, memory], 'Helpers/Tool.app/Contents/MacOS/Tool': [jit, library],
    'Helpers/Amp.app/Contents/MacOS/A&B': [jit] } } };
  const sign = (c) => signRelease(app, { compat: c, manifest, identity: '-', id: 'dev.exact.fixture', scratch: dir, timestamp: ['--timestamp=none'] });
  const keys = (path) => {
    const r = spawnSync('codesign', ['-d', '--entitlements', '-', '--xml', resolve(app, path)], { encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
    return [...r.stdout.matchAll(/<key>([^<]+)<\/key>/g)].map((m) => m[1]);
  };
  const runs = (path) => spawnSync(resolve(app, path)).status;

  sign(compat);
  const verify = spawnSync('codesign', ['--verify', '--deep', '--strict', app], { encoding: 'utf8' });
  assert.equal(verify.status, 0, verify.stderr);
  assert.deepEqual(keys('Contents/Helpers/node/bin/node'), [jit, memory]);
  assert.deepEqual(keys('Contents/Helpers/Tool.app'), [jit, library], 'a nested app carries its executable\'s grant');
  assert.deepEqual(keys('Contents/Helpers/Amp.app'), [jit], 'the bundle signed after its executable keeps the grant');
  assert.deepEqual(keys('.'), [microphone], 'no runtime exception reaches the app');
  for (const path of ['Contents/Helpers/node/bin/npx', 'Contents/Helpers/node/lib/addon.dylib', 'Contents/Helpers/Tool.app/Contents/MacOS/sub']) assert.deepEqual(keys(path), [], path);
  for (const path of ['Contents/Helpers/node/bin/node', 'Contents/Helpers/node/bin/npx', 'Contents/Helpers/Tool.app'])
    assert.match(spawnSync('codesign', ['-dv', resolve(app, path)], { encoding: 'utf8' }).stderr, /flags=0x10002\(adhoc,runtime\)/, path);
  // The hardened runtime itself: JIT for the granted helper and refused to its undeclared sibling.
  assert.equal(runs('Contents/Helpers/node/bin/node'), 0);
  assert.equal(runs('Contents/Helpers/Tool.app/Contents/MacOS/Tool'), 0);
  assert.equal(runs('Contents/Helpers/Amp.app/Contents/MacOS/A&B'), 0);
  assert.equal(runs('Contents/Helpers/node/bin/npx'), 1);
  assert.equal(runs('Contents/Helpers/Tool.app/Contents/MacOS/sub'), 1, 'a bundle\'s grant stays on the executable it runs');
  assert.equal(runs('Contents/MacOS/Fixture'), 1, 'the app carries no runtime exception');
  writeFileSync(resolve(app, 'Contents/Resources/stamp/channel'), 'stable\n');
  assert.notEqual(spawnSync('codesign', ['--verify', '--deep', '--strict', app]).status, 0, 'the stamp is inside the seal');

  // An app that declares none: every signature carries no entitlements, as before D9.
  sign({ reach: { entitlements: [] } });
  assert.equal(spawnSync('codesign', ['--verify', '--deep', '--strict', app]).status, 0);
  for (const path of ['.', 'Contents/Helpers/node/bin/node', 'Contents/Helpers/Tool.app']) assert.deepEqual(keys(path), [], path);
  assert.equal(runs('Contents/Helpers/node/bin/node'), 1);

  // Every target that is not a helper executable the walk signs is refused in one error, before anything is signed.
  symlinkSync('node', resolve(app, 'Contents/Helpers/node/bin/link'));
  // A bundle whose Info.plist hides a decoy name in a comment: Apple's parser reads `Other`, so a grant for `Granted` must not reach the bundle.
  cc('Contents/Helpers/Trick.app/Contents/MacOS/Granted', '-Wl,-no_adhoc_codesign');
  cc('Contents/Helpers/Trick.app/Contents/MacOS/Other', '-Wl,-no_adhoc_codesign');
  writeFileSync(resolve(app, 'Contents/Helpers/Trick.app/Contents/Info.plist'), plist('Other').replace('<dict>', '<dict><!-- <key>CFBundleExecutable</key><string>Granted</string> -->'));
  // Foundation prefers Info-macos.plist, so this bundle runs `Other` whatever Info.plist names.
  cc('Contents/Helpers/Plat.app/Contents/MacOS/Granted', '-Wl,-no_adhoc_codesign');
  cc('Contents/Helpers/Plat.app/Contents/MacOS/Other', '-Wl,-no_adhoc_codesign');
  writeFileSync(resolve(app, 'Contents/Helpers/Plat.app/Contents/Info.plist'), plist('Granted'));
  writeFileSync(resolve(app, 'Contents/Helpers/Plat.app/Contents/Info-macos.plist'), plist('Other'));
  // A signature written to one hard link is every link's.
  linkSync(resolve(app, 'Contents/Helpers/node/bin/npx'), resolve(app, 'Contents/Helpers/node/bin/npx-twin'));
  const bad = Object.fromEntries(['Helpers/node/lib/addon.dylib', 'Helpers/node/bin/missing', 'Helpers/node/bin/link', 'Frameworks/node/bin/node',
    'Helpers/Trick.app/Contents/MacOS/Granted', 'app', 'Helpers/Plat.app/Contents/MacOS/Granted', 'Helpers/node/bin/npx'].map((path) => [path, [jit]]));
  assert.throws(() => sign({ reach: { helperEntitlements: bad } }), (error) => {
    assert.deepEqual(error.message.split('\n').map((line) => line.replace(/^runtime grant: Contents\/\S+ /, '')), [
      'is not a Mach-O executable', 'is not a file in the bundle', 'is not a file in the bundle',
      outside, 'is not code the release signs', outside, 'is not code the release signs',
      'has another hard link, which its signature would reach']);
    return true;
  });
  assert.deepEqual(keys('Contents/Helpers/node/bin/node'), [], 'a refusal signs nothing');
}), 60000);

test('native resource trees keep scoped names, executable modes, links and files above the bake cap', () => inDir(dir => {
  const { chmodSync, ftruncateSync, closeSync, openSync, readFileSync, readlinkSync, statSync } = require('node:fs');
  const { copyNativeResources, nativeResourceInventory } = require('../host/apple/assets.mjs');
  const { readManifest, pendingBuildInputs } = require('./app.mjs');
  const manifest = { name: 'Resources', app: { id: 'test.resources', name: 'Resources' }, host: { macos: { resources: [{ from: 'server', to: 'Resources/server' }] } } };
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
  mkdirSync(resolve(dir, 'server/node_modules/@scope/a package'), { recursive: true });
  writeFileSync(resolve(dir, 'server/node_modules/@scope/a package/index.js'), 'module.exports = 42;\n');
  writeFileSync(resolve(dir, 'server/helper'), '#!/bin/sh\necho helper ran\n');
  chmodSync(resolve(dir, 'server/helper'), 0o755);
  symlinkSync('helper', resolve(dir, 'server/helper-link'));
  const fd = openSync(resolve(dir, 'server/large.bin'), 'w'); ftruncateSync(fd, 65 * 1024 * 1024); closeSync(fd);
  const app = { dir, manifest: readManifest(dir, 'resources') };
  const contents = resolve(dir, 'Fixture.app/Contents'); mkdirSync(contents, { recursive: true });
  const before = nativeResourceInventory(app);
  copyNativeResources(app, contents);
  assert.equal(statSync(resolve(contents, 'Resources/server/helper')).mode & 0o777, 0o755);
  assert.equal(readlinkSync(resolve(contents, 'Resources/server/helper-link')), 'helper');
  assert.equal(spawnSync(resolve(contents, 'Resources/server/helper-link'), [], { encoding: 'utf8' }).stdout, 'helper ran\n');
  assert.equal(readFileSync(resolve(contents, 'Resources/server/node_modules/@scope/a package/index.js'), 'utf8'), 'module.exports = 42;\n');
  const copied = nativeResourceInventory({ dir: resolve(contents, 'Resources'), manifest: { host: { macos: { resources: [{ from: 'server', to: 'Resources/server' }] } } } });
  assert.deepEqual(copied, before);
  const build = { binary: { nativeResourceApp: app, metadata: { nativeResources: before }, inputs: [], directories: [], missing: [] } };
  assert.deepEqual(pendingBuildInputs(build), []);
  chmodSync(resolve(dir, 'server/helper'), 0o744);
  assert.throws(() => copyNativeResources(app, resolve(dir, 'changed'), before), /changed after the binary receipt/);
  assert.deepEqual(pendingBuildInputs(build), ['host.macos.resources']);
}));

test('native resource mappings refuse traversal, overlapping assets and escaping links', () => inDir(dir => {
  const { nativeResourceMappings, nativeResourceInventory, copyNativeResources } = require('../host/apple/assets.mjs');
  const manifest = resources => ({ host: { macos: { resources } } });
  for (const from of ['../server', '/server', 'assets', 'target/server', 'server/../other'])
    assert.throws(() => nativeResourceMappings(manifest([{ from, to: 'Resources/server' }])));
  for (const to of ['../escape', 'MacOS/ExactMac', 'Resources/assets/anything', 'Resources/receipt.json'])
    assert.throws(() => nativeResourceMappings(manifest([{ from: 'server', to }])));
  assert.throws(() => nativeResourceMappings(manifest([{ from: 'server', to: 'Resources/server' }, { from: 'server/child', to: 'Helpers/child' }])));
  mkdirSync(resolve(dir, 'server'));
  writeFileSync(resolve(dir, 'outside'), 'outside');
  symlinkSync('../outside', resolve(dir, 'server/link'));
  const app = { dir, manifest: manifest([{ from: 'server', to: 'Resources/server' }]) };
  assert.throws(() => nativeResourceInventory(app), /escapes/);
  // A file that is not code, placed under Contents/Helpers outside a nested bundle, is refused when the resources are
  // gathered, not at the final codesign; a helper app's own plist is its bundle's (LLP 1156.004 D2).
  mkdirSync(resolve(dir, 'helpers/Inner.app/Contents/MacOS'), { recursive: true });
  writeFileSync(resolve(dir, 'helpers/Inner.app/Contents/Info.plist'), '<plist/>');
  writeFileSync(resolve(dir, 'helpers/Inner.app/Contents/MacOS/Inner'), Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1]));
  assert.equal(nativeResourceInventory({ dir, manifest: manifest([{ from: 'helpers', to: 'Helpers/kit' }]) }).length, 6);
  writeFileSync(resolve(dir, 'helpers/config.json'), '{}');
  assert.throws(() => nativeResourceInventory({ dir, manifest: manifest([{ from: 'helpers', to: 'Helpers/kit' }]) }), /helpers\/config\.json is not code.*Resources\//);
  rmSync(resolve(dir, 'server/link'));
  mkdirSync(resolve(dir, 'bundle/Resources'), { recursive: true });
  symlinkSync(resolve(dir, 'server'), resolve(dir, 'bundle/Resources/server'));
  assert.throws(() => copyNativeResources(app, resolve(dir, 'bundle')), /already exists/);
}));

test('iOS resource trees are copied alone under bundle/, at the path their to names, and an edit asks for a new binary', () => inDir(dir => {
  const { readFileSync, readdirSync, statSync, chmodSync } = require('node:fs');
  const { copyNativeResources, nativeResourceInventory, IOS_RESOURCE_ROOT } = require('../host/apple/assets.mjs');
  const { readManifest, pendingBuildInputs } = require('./app.mjs');
  const tree = [{ from: 'shipped', to: 'Resources/shipped' }];
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Shipped', app: { id: 'test.shipped', name: 'Shipped' }, host: { ios: { resources: tree }, macos: { resources: tree } } }));
  mkdirSync(resolve(dir, 'shipped/cities'), { recursive: true });
  writeFileSync(resolve(dir, 'shipped/cities/list.json'), '["Oslo"]');
  chmodSync(resolve(dir, 'shipped/cities/list.json'), 0o640);
  const app = { dir, manifest: readManifest(dir, 'shipped') };
  const bundle = resolve(dir, 'Shipped.app'); mkdirSync(bundle);
  writeFileSync(resolve(bundle, 'Info.plist'), '<plist/>');
  // An app named "bundle" has its executable there, which the folder no longer meets.
  writeFileSync(resolve(bundle, 'bundle'), 'executable');
  const before = nativeResourceInventory(app, null, 'ios');
  copyNativeResources(app, resolve(bundle, IOS_RESOURCE_ROOT), before, 'ios');
  // `to` under bundle/, as under the Mac's Contents: `bundle:/<to>` on both (the runtime's reading is host/apple/tests/it/bundle_root.rs).
  assert.equal(readFileSync(resolve(bundle, IOS_RESOURCE_ROOT, 'Resources/shipped/cities/list.json'), 'utf8'), '["Oslo"]');
  assert.equal(statSync(resolve(bundle, IOS_RESOURCE_ROOT, 'Resources/shipped/cities/list.json')).mode & 0o777, 0o640);
  assert.deepEqual(readdirSync(resolve(bundle, IOS_RESOURCE_ROOT)), ['Resources']);
  assert.deepEqual(readdirSync(resolve(bundle, IOS_RESOURCE_ROOT, 'Resources')), ['shipped']);
  const build = { binary: { nativeResourceApp: { dir, manifest: app.manifest, platform: 'ios' }, metadata: { nativeResources: before }, inputs: [], directories: [], missing: [] } };
  assert.deepEqual(pendingBuildInputs(build), []);
  writeFileSync(resolve(dir, 'shipped/cities/list.json'), '["Oslo","Bergen"]');
  assert.deepEqual(pendingBuildInputs(build), ['host.ios.resources']);
  // Nothing declared: no bundle/ folder, so the host names no bundle: root.
  const none = { dir, manifest: { host: {} } }, bare = resolve(dir, 'Bare.app'); mkdirSync(bare);
  copyNativeResources(none, resolve(bare, IOS_RESOURCE_ROOT), [], 'ios');
  assert.equal(existsSync(resolve(bare, IOS_RESOURCE_ROOT)), false);
}));

test('iOS resource trees refuse code, links and a place outside Resources/; the build refuses a bundle: line no tree serves', () => inDir(dir => {
  const { nativeResourceMappings, nativeResourceInventory, iosBundleGrants } = require('../host/apple/assets.mjs');
  const manifest = resources => ({ host: { ios: { resources } } });
  for (const to of ['Helpers/server', 'Frameworks/server', '../escape', '/abs', 'Resources', 'Resources/a/../b'])
    assert.throws(() => nativeResourceMappings(manifest([{ from: 'server', to }]), 'ios'), /host\.ios\.resources/);
  assert.throws(() => nativeResourceMappings(manifest([{ from: 'server', to: 'Resources/a' }, { from: 'other', to: 'Resources/a/b' }]), 'ios'), /must not overlap/);
  mkdirSync(resolve(dir, 'server'));
  writeFileSync(resolve(dir, 'server/data.json'), '{}');
  const app = { dir, manifest: manifest([{ from: 'server', to: 'Resources/server' }]) };
  assert.equal(nativeResourceInventory(app, null, 'ios').length, 2);
  symlinkSync('data.json', resolve(dir, 'server/link'));
  assert.throws(() => nativeResourceInventory(app, null, 'ios'), /server\/link is a link, and bundle: follows none/);
  rmSync(resolve(dir, 'server/link'));
  writeFileSync(resolve(dir, 'server/tool'), Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0, 0, 1]));
  assert.throws(() => nativeResourceInventory(app, null, 'ios'), /server\/tool is Mach-O code/);
  // A read line under a declared tree, or above one, is served; any other bundle: line is refused at build.
  iosBundleGrants(app.manifest, 'fs.read bundle:/Resources/server\nfs.read bundle:/Resources\nfs.read app:/data\nnet.fetch https://x.test');
  // By whole names, empty ones dropped, as the grant parser reads them; a quoted target too.
  iosBundleGrants(app.manifest, 'fs.read bundle://Resources//server/\nfs.read "bundle:/Resources/server/a b"');
  assert.throws(() => iosBundleGrants(app.manifest, 'fs.read bundle:/Resources/serverx'), /serverx/);
  assert.throws(() => iosBundleGrants(app.manifest, 'fs.read bundle:/resources/server'), /resources\/server/);
  assert.throws(() => iosBundleGrants(app.manifest, 'fs.read bundle:/Resources/other'), /`fs\.read bundle:\/Resources\/other`: on iOS and tvOS, bundle:\/ holds only.*bundle:\/Resources\/server.*host\.ios\.resources/);
  assert.throws(() => iosBundleGrants({ host: {} }, 'fs.read bundle:/Resources/server'), /none declared/);
  // The grants are every host's: a line on the Mac's Contents, what the Mac build writes, or a tree only the Mac
  // declares is left to fail at run time on iOS, as a home: line is.
  const mac = { host: { ...app.manifest.host, macos: { resources: [{ from: 'kit', to: 'Helpers/kit' }, { from: 'mac', to: 'Resources/maconly' }] } } };
  iosBundleGrants(mac, 'fs.read bundle:/Info.plist\nfs.read bundle:/Helpers/kit\nfs.read bundle:/Resources/maconly/x\nfs.read bundle:/Resources/receipt.json\nfs.read bundle:/');
  // Every name the Mac build writes under Contents/Resources, a widget app's plans among them, and a usage string's .lproj.
  const { MAC_RESOURCES_WRITTEN } = require('../host/apple/assets.mjs');
  iosBundleGrants(app.manifest, ['plans', 'typescript', 'rust', 'activities', 'Metadata.appintents', 'en.lproj'].map(n => `fs.read bundle:/Resources/${n}`).join('\n'));
  // ...which the Mac's packaging names from that one list: each name it writes under `resources` is on it.
  const { readFileSync } = require('node:fs');
  const packaging = ['bundle.mjs', 'intents.mjs'].map(f => readFileSync(resolve(__dirname, '../host/apple', f), 'utf8')).join('\n');
  const written = [...packaging.matchAll(/resolve\(resources, '([^']+)'\)/g)].map(m => m[1]);
  assert.ok(written.length >= 2, 'the packaging names what it writes');
  for (const name of [...written, 'Assets.car']) assert.ok(MAC_RESOURCES_WRITTEN.includes(name), `the Mac writes Resources/${name}`);
  assert.match(packaging, /copyAppleStaticTrees\(paths\.capture, resources\)/);
  assert.throws(() => nativeResourceMappings({ host: { macos: { resources: [{ from: 'server', to: 'Resources/plans' }] } } }), /host\.macos\.resources\.to/);
  // An executable named as the folder, or as what every bundle holds, is refused, ignoring case; any other name is not.
  const { iosExecutableRefusal } = require('../host/apple/assets.mjs');
  assert.throws(() => iosExecutableRefusal(app.manifest, 'exactresources'), /app\.name "exactresources".*"ExactResources"/);
  assert.throws(() => iosExecutableRefusal(app.manifest, 'ExactResources', 'host.ios.appClip.name'), /host\.ios\.appClip\.name "ExactResources"/);
  assert.throws(() => iosExecutableRefusal({ host: {} }, 'frameworks'), /"Frameworks"/);
  iosExecutableRefusal({ host: {} }, 'ExactResources');
  for (const name of ['Bundle', 'bundle', 'Deck', 'Shipped']) iosExecutableRefusal(app.manifest, name);
  // A bundle beside the app's gets the folder when trees are declared, never an executable an app named so leaves there.
  const { shareIosResources, IOS_RESOURCE_ROOT } = require('../host/apple/assets.mjs');
  for (const [name, manifest, shared] of [['named', { host: {} }, false], ['declared', app.manifest, true]]) {
    const main = resolve(dir, `${name}.app`), clip = resolve(main, 'AppClips/Lite.app');
    mkdirSync(clip, { recursive: true });
    if (shared) mkdirSync(resolve(main, IOS_RESOURCE_ROOT, 'Resources/server'), { recursive: true });
    else writeFileSync(resolve(main, IOS_RESOURCE_ROOT), 'the executable of an app named ExactResources');
    shareIosResources(manifest, main, clip);
    assert.equal(existsSync(resolve(clip, IOS_RESOURCE_ROOT, 'Resources/server')), shared, name);
    if (!shared) assert.equal(existsSync(resolve(clip, IOS_RESOURCE_ROOT)), false);
  }
  // Neither platform's trees are TypeScript sources (host/web-js/build.mjs's capture; js/bake's reads the same keys),
  // known by identity: a sibling directory never is one, and a name the volume resolves to the tree is.
  const { fileIdentity, resourceIdentities } = require('../host/apple/assets.mjs');
  mkdirSync(resolve(dir, 'kept')); mkdirSync(resolve(dir, 'other'));
  const ids = resourceIdentities(dir, { host: { ios: { resources: [{ from: 'kept', to: 'Resources/kept' }] }, macos: { resources: [{ from: 'server', to: 'Resources/server' }] } } });
  assert.deepEqual([...ids].sort(), [fileIdentity(resolve(dir, 'kept')), fileIdentity(resolve(dir, 'server'))].sort());
  assert.equal(ids.has(fileIdentity(resolve(dir, 'other'))), false);
  assert.notEqual(fileIdentity(resolve(dir, 'kept')), fileIdentity(resolve(dir, 'other')));
  if (existsSync(resolve(dir, 'KEPT'))) assert.equal(ids.has(fileIdentity(resolve(dir, 'KEPT'))), true, 'this volume ignores case');
}));

test.skipIf(process.platform !== 'darwin')('native resources sign and execute a Mach-O helper with its dylib in the assembled bundle', () => inDir(dir => {
  const { copyNativeResources } = require('../host/apple/assets.mjs');
  mkdirSync(resolve(dir, 'server'), { recursive: true });
  writeFileSync(resolve(dir, 'lib.c'), 'int answer(void) { return 42; }\n');
  writeFileSync(resolve(dir, 'main.c'), '#include <stdio.h>\nextern int answer(void); int main(void) { if(answer()!=42) return 1; puts("helper ran"); return 0; }\n');
  const command = (cmd, args) => { const r = spawnSync(cmd, args, { encoding: 'utf8' }); assert.equal(r.status, 0, `${cmd}: ${r.stderr}`); return r; };
  command('cc', ['-dynamiclib', resolve(dir, 'lib.c'), '-Wl,-install_name,@loader_path/addon.node', '-Wl,-no_adhoc_codesign', '-o', resolve(dir, 'server/addon.node')]);
  command('cc', [resolve(dir, 'main.c'), resolve(dir, 'server/addon.node'), '-Wl,-no_adhoc_codesign', '-o', resolve(dir, 'server/helper')]);
  const app = resolve(dir, 'Fixture.app'), contents = resolve(app, 'Contents');
  mkdirSync(resolve(contents, 'MacOS'), { recursive: true });
  writeFileSync(resolve(contents, 'Info.plist'), plist('ExactMac'));
  writeFileSync(resolve(dir, 'host.c'), 'int main(void) { return 0; }\n');
  command('cc', [resolve(dir, 'host.c'), '-o', resolve(contents, 'MacOS/ExactMac')]);
  copyNativeResources({ dir, manifest: { host: { macos: { resources: [{ from: 'server', to: 'Resources/server' }] } } } }, contents);
  for (const path of signingOrder(app)) command('codesign', ['--force', '--sign', '-', '--timestamp=none', path]);
  command('codesign', ['--verify', '--deep', '--strict', app]);
  assert.equal(command(resolve(contents, 'Resources/server/helper'), []).stdout, 'helper ran\n');
}));

test.skipIf(!tools)('production executables keep matching dSYMs without an IPA; development keeps its symbols', () => inDir(dir => {
  const source = resolve(dir, 'main.c'), executable = resolve(dir, 'Demo');
  writeFileSync(source, 'static int value(void){return 42;}\nint main(void){return value();}\n');
  const cc = spawnSync('clang', ['-g', '-o', executable, source], { encoding: 'utf8' });
  assert.equal(cc.status, 0, cc.stderr);
  rmSync(`${executable}.dSYM`, { recursive: true, force: true });
  const original = readFileSync(executable);
  assert.equal(stripProductionExecutable(executable, { production: false }), null);
  assert.deepEqual(readFileSync(executable), original);
  assert.ok(!existsSync(`${executable}.dSYM`));
  const uuid = path => {
    const result = spawnSync('dwarfdump', ['--uuid', path], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return /UUID: ([A-F0-9-]+)/.exec(result.stdout)?.[1];
  };
  const before = uuid(executable);
  assert.ok(before);
  const stripped = stripProductionExecutable(executable, { production: true });
  assert.equal(stripped.dsym, `${executable}.dSYM`);
  assert.ok(stripped.saved > 0);
  assert.equal(uuid(executable), before);
  assert.equal(uuid(stripped.dsym), before);
  writeFileSync(executable, original);
  const archive = stripProductionExecutable(executable, { production: true, ipa: resolve(dir, 'archives/Release.ipa') });
  assert.equal(archive.dsym, resolve(dir, 'archives/Release.dSYM'));
  assert.ok(archive.saved > 0);
  assert.equal(uuid(archive.dsym), before);
}));

// An executable is named for its app (955b0463d), and an app may be named
// "T3 Code (Exact)": otool reads a path ending in `name(member)` as an
// archive's member, so the SDK check read nothing and refused the build (#234).
test.skipIf(!tools)('an executable whose name ends in parentheses is read as a file, its SDK checked and its release stripped', () => inDir((dir) => {
  writeFileSync(resolve(dir, 'main.c'), 'int main(void){return 0;}\n');
  const executable = resolve(dir, 'Demo (Beta)');
  // The macOS 15 SDK recorded, as a design-compatible build links it.
  const r = spawnSync('clang', ['-g', '-mmacosx-version-min=14.0', '-Wl,-platform_version,macos,14.0,15.0', '-o', executable, resolve(dir, 'main.c')], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  assertLinkedSdk(executable, '15.0');
  assert.throws(() => assertLinkedSdk(executable, '27.0'), /Demo \(Beta\) records SDK 15\.0, not 27\.0/);
  // A file the tool cannot read says so, not that it records no SDK.
  assert.throws(() => assertLinkedSdk(resolve(dir, 'Gone (Beta)'), '15.0'), /cannot read .*Gone \(Beta\)/);
  // dsymutil and strip take the same path as a file.
  const { saved } = stripForDistribution(executable, resolve(dir, 'Demo (Beta).dSYM'));
  assert.ok(existsSync(resolve(dir, 'Demo (Beta).dSYM/Contents/Resources/DWARF/Demo (Beta)')), 'dsymutil wrote the dSYM');
  assert.ok(saved > 0, 'strip took symbols off');
  assertLinkedSdk(executable, '15.0');
  // A binary whose bytes must not change (an archive's GPU framework) gets its dSYM alone.
  const bytes = readFileSync(executable);
  assert.equal(stripForDistribution(executable, resolve(dir, 'Kept.dSYM'), { strip: false }).saved, 0);
  assert.ok(existsSync(resolve(dir, 'Kept.dSYM/Contents/Resources/DWARF/Demo (Beta)')), 'dsymutil wrote the dSYM');
  assert.ok(readFileSync(executable).equals(bytes), 'the binary is unchanged');
}));

// Catalogs are native inputs even when the app declares no native view tags.
function composerFixture(dir) {
  cpSync(resolve(import.meta.dir, 'fixtures/Primary.icon'), resolve(dir, 'Primary.icon'), { recursive: true });
  return { dir, name: 'Composer', manifest: { host: { ios: { icon: 'Primary.icon' }, macos: { icon: 'Primary.icon' } } } };
}

test('Icon Composer selection is an explicit per-platform manifest field', () => inDir(dir => {
  const { readManifest } = require('./app.mjs');
  const app = composerFixture(dir);
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: app.name, app: { id: 'com.exact.composer', name: app.name }, ...app.manifest }));
  assert.deepEqual(readManifest(dir, 'composer').host, app.manifest.host);
}));

test('Icon Composer layers are native receipt inputs; unselected projects and tvOS are excluded', () => inDir(dir => {
  const { appleAssetCatalogInventory } = require('../host/apple/assets.mjs');
  const { pendingBuildInputs } = require('./app.mjs');
  const app = composerFixture(dir);
  const inventory = appleAssetCatalogInventory(app, 'ios');
  const build = { binary: { assetCatalogApp: { ...app, platform: 'ios' }, metadata: { appleAssetCatalogs: inventory }, inputs: [], directories: [], missing: [] } };
  assert.deepEqual(pendingBuildInputs(build), []);
  writeFileSync(resolve(dir, 'Primary.icon/Assets/square.svg'), '<svg/>');
  assert.deepEqual(pendingBuildInputs(build), ['Apple asset catalogs']);
  assert.notDeepEqual(appleAssetCatalogInventory(app, 'ios'), inventory);
  assert.deepEqual(inventory.map(entry => entry.path), ['Primary.icon']);
  assert.deepEqual(appleAssetCatalogInventory(app, 'tvos'), []);
  delete app.manifest.host.ios.icon;
  assert.deepEqual(appleAssetCatalogInventory(app, 'ios'), []);
  assert.deepEqual(appleAssetCatalogInventory(app, 'macos').map(entry => entry.path), ['Primary.icon']);
}));

for (const [name, path, prepare, error] of [
  ['missing', 'Missing.icon', () => {}, /host\.ios\.icon.*Missing\.icon.*does not exist/],
  ['file', 'File.icon', dir => writeFileSync(resolve(dir, 'File.icon'), '{}'), /host\.ios\.icon.*File\.icon.*\.icon directory/],
  ['extension', 'Folder', dir => mkdirSync(resolve(dir, 'Folder')), /host\.ios\.icon.*Folder.*\.icon directory/],
  ['conflict', 'Primary.icon', dir => { mkdirSync(resolve(dir, 'modules/apple/Art.xcassets/AppIcon.appiconset'), { recursive: true }); writeFileSync(resolve(dir, 'modules/apple/Art.xcassets/AppIcon.appiconset/Contents.json'), '{}'); }, /Primary\.icon.*Art\.xcassets\/AppIcon\.appiconset/],
  ['same-named catalog conflict', 'Primary.icon', dir => { mkdirSync(resolve(dir, 'modules/apple/Art.xcassets/Primary.appiconset'), { recursive: true }); writeFileSync(resolve(dir, 'modules/apple/Art.xcassets/Primary.appiconset/Contents.json'), '{}'); }, /Primary\.icon.*Art\.xcassets\/Primary\.appiconset/],
  ['empty catalog conflict', 'Primary.icon', dir => mkdirSync(resolve(dir, 'modules/apple/Art.xcassets/AppIcon.appiconset'), { recursive: true }), /Primary\.icon.*Art\.xcassets\/AppIcon\.appiconset/],
]) test(`Icon Composer refuses a selected ${name} by name`, () => inDir(dir => {
  const { appleAssetCatalogInventory } = require('../host/apple/assets.mjs');
  const app = composerFixture(dir);
  app.manifest.host.ios.icon = path;
  prepare(dir);
  assert.throws(() => appleAssetCatalogInventory(app, 'ios'), error);
}));

test('Icon Composer keeps an alternate app icon set beside the selected project', () => inDir(dir => {
  const { appleAssetCatalogInventory } = require('../host/apple/assets.mjs');
  const app = composerFixture(dir);
  mkdirSync(resolve(dir, 'modules/apple/Art.xcassets/Brand.appiconset'), { recursive: true });
  writeFileSync(resolve(dir, 'modules/apple/Art.xcassets/Brand.appiconset/Contents.json'), '{}');
  assert.deepEqual(appleAssetCatalogInventory(app, 'ios').map(entry => entry.path), ['modules/apple/Art.xcassets', 'Primary.icon']);
}));

test('Icon Composer refuses paths outside the app even through an alias back inside', () => inDir(dir => {
  const { appleAssetCatalogInventory } = require('../host/apple/assets.mjs');
  const folder = resolve(dir, 'app'); mkdirSync(folder);
  const app = composerFixture(folder);
  symlinkSync(folder, resolve(dir, 'alias'));
  app.manifest.host.ios.icon = '../alias/Primary.icon';
  assert.throws(() => appleAssetCatalogInventory(app, 'ios'), /host\.ios\.icon.*inside the app/);
}));

test.skipIf(process.platform !== 'darwin')('Icon Composer supplies the primary icon, caches layer bytes, and removal restores raster icons', () => inDir(dir => {
  const { useXcode } = require('../host/apple/devices.mjs');
  const { appleAssets, iosAssets, appleAssetCatalogInventory, appIcon } = require('../host/apple/assets.mjs');
  const { readdirSync } = require('node:fs');
  useXcode();
  const app = composerFixture(dir), out = resolve(dir, 'out'), second = resolve(dir, 'second'), mac = resolve(dir, 'mac');
  for (const folder of [out, second, mac]) mkdirSync(folder);
  const kept = { dir: resolve(dir, 'cache'), stamp: 'fixture-toolchain' };
  const inventory = appleAssetCatalogInventory(app, 'ios');
  const keys = iosAssets(app, out, false, { catalog: true, kept, expected: inventory });
  assert.equal(keys.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName, 'Primary');
  const inspect = folder => {
    const result = spawnSync('xcrun', ['assetutil', '--info', resolve(folder, 'Assets.car')], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  };
  assert.ok(inspect(out).some(asset => asset.AssetType === 'IconImageStack'));
  const cached = () => readdirSync(kept.dir).find(name => name.startsWith('assets-ios-'));
  const first = cached(), mtime = statSync(resolve(kept.dir, first, 'Assets.car'), { bigint: true }).mtimeNs;
  assert.deepEqual(iosAssets(app, second, false, { catalog: true, kept, expected: inventory }), keys);
  assert.equal(statSync(resolve(kept.dir, first, 'Assets.car'), { bigint: true }).mtimeNs, mtime);
  assert.deepEqual(readFileSync(resolve(out, 'Assets.car')), readFileSync(resolve(second, 'Assets.car')));
  const layer = resolve(dir, 'Primary.icon/Assets/square.svg');
  writeFileSync(layer, readFileSync(layer, 'utf8').replace('#ffffff', '#ff0000'));
  assert.throws(() => iosAssets(app, second, false, { catalog: true, kept, expected: inventory }), /changed after the bake/);
  iosAssets(app, second, false, { catalog: true, kept });
  assert.notEqual(cached(), first);
  assert.notDeepEqual(readFileSync(resolve(out, 'Assets.car')), readFileSync(resolve(second, 'Assets.car')));
  const macKeys = appleAssets(app, mac, { platform: 'macos', kept });
  assert.equal(macKeys.CFBundleIconName, 'Primary');
  assert.ok(inspect(mac).some(asset => asset.AssetType === 'IconImageStack'));
  writeFileSync(resolve(dir, 'raster.png'), encodePng({ width: 1024, height: 1024, data: new Uint8Array(1024 * 1024 * 4).fill(255) }));
  app.manifest.icons = [{ src: 'raster.png', sizes: '1024x1024' }];
  assert.deepEqual(appIcon(app, mac, 'macos'), {}, 'the raster must not overwrite the selected Mac icon');
  delete app.manifest.host.ios.icon;
  assert.throws(() => iosAssets(app, second, false, { catalog: true, expected: inventory }), /changed after the bake/);
  assert.equal(iosAssets(app, second, false, { catalog: true, kept }).CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName, 'AppIcon');
  assert.ok(inspect(second).some(asset => asset.Name === 'AppIcon'));
  delete app.manifest.host.macos.icon;
  assert.deepEqual(appleAssets(app, mac, { platform: 'macos', kept }), {});
  assert.ok(!existsSync(resolve(mac, 'Assets.car')));
  assert.deepEqual(appIcon(app, mac, 'macos'), { CFBundleIconFile: 'AppIcon' });
}), 60000);

test.skipIf(process.platform !== 'darwin')('iOS alternate icon sets compile beside raster and Composer primaries for both device families', () => inDir(dir => {
  const { useXcode } = require('../host/apple/devices.mjs');
  const { iosAssets } = require('../host/apple/assets.mjs');
  useXcode();
  const primary = resolve(dir, 'icon.png');
  writeFileSync(primary, encodePng({ width: 1024, height: 1024, data: new Uint8Array(1024 * 1024 * 4).fill(255) }));
  const set = resolve(dir, 'modules/apple/Icons.xcassets/Nested/Alternate.appiconset');
  mkdirSync(set, { recursive: true });
  const sized = spawnSync('sips', ['-z', '1024', '1024', primary, '--out', resolve(set, 'icon.png')], { encoding: 'utf8' });
  assert.equal(sized.status, 0, sized.stderr);
  writeFileSync(resolve(set, 'Contents.json'), JSON.stringify({ info: { author: 'xcode', version: 1 }, images: [{ idiom: 'universal', platform: 'ios', size: '1024x1024', filename: 'icon.png' }] }));
  const app = composerFixture(dir);
  app.manifest.icons = [{ src: 'icon.png', sizes: '1024x1024' }];
  for (const [composer, catalog, device, folder] of [[true, true, false, 'composer'], [false, true, false, 'raster'], [false, false, false, 'development'], [false, true, true, 'device']]) {
    if (!composer) delete app.manifest.host.ios.icon;
    const out = resolve(dir, folder); mkdirSync(out);
    const keys = iosAssets(app, out, device, { catalog });
    for (const family of ['CFBundleIcons', 'CFBundleIcons~ipad']) {
      assert.equal(keys[family].CFBundlePrimaryIcon.CFBundleIconName, composer ? 'Primary' : 'AppIcon');
      assert.deepEqual(keys[family].CFBundleAlternateIcons, { Alternate: { CFBundleIconName: 'Alternate' } });
    }
    const inspected = spawnSync('xcrun', ['assetutil', '--info', resolve(out, 'Assets.car')], { encoding: 'utf8' });
    assert.equal(inspected.status, 0, inspected.stderr);
    assert.ok(JSON.parse(inspected.stdout).some(asset => asset.Name === 'Alternate'), 'the declared alternate must exist in Assets.car');
  }
}), 60000);

test('Apple asset catalogs select platform modules and invalidate native receipts on additions and deletions', () => inDir(dir => {
  const { appleAssetCatalogInventory } = require('../host/apple/assets.mjs');
  const { pendingBuildInputs } = require('./app.mjs');
  const app = { dir, platform: 'ios', manifest: {} };
  const build = { binary: { assetCatalogApp: app, metadata: {}, inputs: [], directories: [], missing: [] } };
  const put = (path, text) => { mkdirSync(dirname(resolve(dir, path)), { recursive: true }); writeFileSync(resolve(dir, path), text); };
  assert.deepEqual(pendingBuildInputs(build), []);
  put('modules/apple/Shared.xcassets/Contents.json', '{"info":{"version":1,"author":"xcode"}}');
  assert.deepEqual(pendingBuildInputs(build), ['Apple asset catalogs']);
  const { appleRunBinary, appleArtifacts } = require('../host/apple/build.mjs');
  const launchApp = { ...app, id: 'com.exact.catalog', name: 'catalog', displayName: 'Catalog', target: resolve(dir, 'target') };
  const paths = appleArtifacts(launchApp);
  assert.equal(appleRunBinary(launchApp), resolve(paths.bundle, 'Contents/MacOS', paths.executable));
  const shared = appleAssetCatalogInventory(app, 'ios');
  assert.equal(shared[0].path, 'modules/apple/Shared.xcassets');
  build.binary.metadata.appleAssetCatalogs = shared;
  assert.deepEqual(pendingBuildInputs(build), []);
  put('modules/apple/Shared.xcassets/Contents.json', '{"info":{"version":1,"author":"exact"}}');
  assert.deepEqual(pendingBuildInputs(build), ['Apple asset catalogs']);
  put('ios/modules/Local.xcassets/Contents.json', '{}');
  assert.deepEqual(appleAssetCatalogInventory(app, 'ios').map(entry => entry.path), ['ios/modules/Local.xcassets']);
  assert.deepEqual(appleAssetCatalogInventory(app, 'macos').map(entry => entry.path), ['modules/apple/Shared.xcassets']);
  rmSync(resolve(dir, 'ios/modules'), { recursive: true });
  assert.equal(appleAssetCatalogInventory(app, 'ios')[0].path, shared[0].path);
  rmSync(resolve(dir, 'modules/apple/Shared.xcassets'), { recursive: true });
  assert.deepEqual(pendingBuildInputs(build), ['Apple asset catalogs']);
  assert.equal(appleRunBinary(launchApp), resolve(paths.bundle, 'Contents/MacOS', paths.executable));
  launchApp.manifest.widgets = [{ kind: 'card', families: ['systemSmall'] }];
  mkdirSync(paths.products, { recursive: true });
  writeFileSync(resolve(paths.products, 'receipt.json'), JSON.stringify({ entitlements: null }));
  assert.equal(appleRunBinary(launchApp), resolve(paths.bundle, 'Contents/MacOS', paths.executable), 'a widget declaration without an identity still launches the bundled app');
  writeFileSync(resolve(paths.products, 'receipt.json'), JSON.stringify({ entitlements: plistFile({ 'com.apple.security.application-groups': ['TEAM.com.exact.catalog.widgets'] }) }));
  assert.equal(appleRunBinary(launchApp), resolve(paths.bundle, 'Contents/MacOS', paths.executable));
  assert.equal(appleRunBinary(launchApp, true), appleArtifacts(launchApp, { host: true }).binary);
  mkdirSync(resolve(dir, 'modules/apple/Linked.xcassets'));
  symlinkSync(resolve(dir, 'outside'), resolve(dir, 'modules/apple/Linked.xcassets/Contents.json'));
  assert.throws(() => appleAssetCatalogInventory(app, 'ios'), /symlinks/);
}));

test.skipIf(process.platform !== 'darwin')('the iOS launch screen is the app\'s own ground: its colours, else the grouped grey under a first route that scrolls a grouped list, else the system\'s (LLP 1116.011 D2.3)', () => inDir(dir => {
  const { useXcode } = require('../host/apple/devices.mjs');
  const { iosAssets } = require('../host/apple/assets.mjs');
  useXcode();
  let n = 0;
  const made = (manifest, options) => {
    const out = resolve(dir, `app${n++}.app`); mkdirSync(out);
    const keys = iosAssets({ dir, name: 'Launch', manifest }, out, false, options);
    const result = spawnSync('xcrun', ['assetutil', '--info', resolve(out, 'Assets.car')], { encoding: 'utf8' });
    const colours = keys.UILaunchScreen ? JSON.parse(result.stdout).filter(asset => asset.Name === keys.UILaunchScreen.UIColorName)
      .map(asset => [asset.Appearance ?? 'light', asset['Color components'].map(c => Math.round(c * 255))]) : [];
    return { keys, colours };
  };
  // Stopwatch, round 13: a first route on the grouped grey, whose launch screen was the system's white.
  const grouped = made({}, { launch: 'grouped' });
  assert.match(grouped.keys.UILaunchScreen.UIColorName, /^ExactLaunch-[0-9a-f]{8}$/);
  assert.deepEqual(grouped.colours, [['light', [242, 242, 247, 255]], ['UIAppearanceDark', [0, 0, 0, 255]]]);
  // A page of the platform's own stays the system's launch screen (`systemBackground`, light and dark).
  for (const options of [{ launch: 'system' }, {}]) assert.deepEqual(made({}, options).keys, {});
  // The author's colours name it whatever the first route stands on.
  const named = made({ background_color: '#ffeedd', color_scheme_dark: { background_color: '#102030' } }, { launch: 'grouped' });
  assert.deepEqual(named.colours, [['light', [255, 238, 221, 255]], ['UIAppearanceDark', [16, 32, 48, 255]]]);
}), 60000);

test.skipIf(process.platform !== 'darwin')('Apple asset catalogs retain named images and colours, share the iOS icon pass, and reuse unchanged compiles', () => inDir(dir => {
  const { useXcode } = require('../host/apple/devices.mjs');
  const { appleAssets, iosAssets, appleAssetCatalogInventory } = require('../host/apple/assets.mjs');
  const { readFileSync, readdirSync, statSync } = require('node:fs');
  useXcode();
  const put = (path, bytes) => { mkdirSync(dirname(resolve(dir, path)), { recursive: true }); writeFileSync(resolve(dir, path), bytes); };
  const json = (path, value) => put(path, JSON.stringify(value));
  const info = { version: 1, author: 'xcode' };
  const source = 'modules/apple/Artwork.xcassets';
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=', 'base64');
  json(`${source}/Contents.json`, { info });
  put(`${source}/Badge.imageset/image.png`, png);
  json(`${source}/Badge.imageset/Contents.json`, { info, images: [{ idiom: 'universal', filename: 'image.png' }] });
  const colour = red => ({ info, colors: [{ idiom: 'universal', color: { 'color-space': 'srgb', components: { red, green: '0.0', blue: '0.0', alpha: '1.0' } } }] });
  json(`${source}/Ink.colorset/Contents.json`, colour('0.2'));
  put('icon.png', encodePng({ width: 1024, height: 1024, data: new Uint8Array(1024 * 1024 * 4).fill(255) }));
  const app = { dir, name: 'Catalog', manifest: { icons: [{ src: 'icon.png', sizes: '1024x1024' }], background_color: '#fff', color_scheme_dark: { background_color: '#123456' } } };
  const cache = { dir: resolve(dir, 'cache'), stamp: 'fixture-toolchain' };
  const inventory = appleAssetCatalogInventory(app, 'ios');
  const ios = resolve(dir, 'iOS.app'); mkdirSync(ios);
  const keys = iosAssets(app, ios, false, { catalog: true, kept: cache, expected: inventory });
  assert.equal(keys.CFBundleIcons.CFBundlePrimaryIcon.CFBundleIconName, 'AppIcon');
  const launch = keys.UILaunchScreen.UIColorName;
  assert.match(launch, /^ExactLaunch-[0-9a-f]{8}$/);
  assert.equal(keys.NSAccentColorName, undefined, 'no AccentColor set, no accent named');
  const inspect = path => {
    const result = spawnSync('xcrun', ['assetutil', '--info', resolve(path, 'Assets.car')], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  };
  const first = inspect(ios);
  for (const name of ['Badge', 'Ink', 'AppIcon', launch]) assert.ok(first.some(asset => asset.Name === name), name);
  assert.equal(first.filter(asset => asset.Name === launch).length, 2);
  const held = readdirSync(cache.dir).find(name => name.startsWith('assets-ios-'));
  const before = statSync(resolve(cache.dir, held, 'Assets.car'), { bigint: true }).mtimeNs;
  const second = resolve(dir, 'Second.app'); mkdirSync(second);
  assert.deepEqual(iosAssets(app, second, false, { catalog: true, kept: cache, expected: inventory }), keys);
  assert.equal(statSync(resolve(cache.dir, held, 'Assets.car'), { bigint: true }).mtimeNs, before);
  assert.deepEqual(readFileSync(resolve(second, 'Assets.car')), readFileSync(resolve(ios, 'Assets.car')));
  json(`${source}/Ink.colorset/Contents.json`, colour('0.8'));
  assert.throws(() => iosAssets(app, second, false, { kept: cache, expected: inventory }), /changed after the bake/);
  iosAssets(app, second, false, { catalog: true, kept: cache });
  assert.notEqual(readdirSync(cache.dir).find(name => name.startsWith('assets-ios-')), held);
  assert.notDeepEqual(readFileSync(resolve(second, 'Assets.car')), readFileSync(resolve(ios, 'Assets.car')));
  // An `AccentColor` set is named the app's accent, as Xcode's default
  json(`${source}/AccentColor.colorset/Contents.json`, colour('0.5'));
  const accented = resolve(dir, 'Accented.app'); mkdirSync(accented);
  assert.equal(iosAssets(app, accented, false, { catalog: true }).NSAccentColorName, 'AccentColor');
  const cached = resolve(dir, 'Cached.app'); mkdirSync(cached);
  assert.equal(iosAssets(app, cached, false, { catalog: true, kept: cache }).NSAccentColorName, 'AccentColor', 'the cached path names it too');
  assert.ok(inspect(accented).some(asset => asset.Name === 'AccentColor'));
  const accentedMac = resolve(dir, 'AccentedMac.bundle/Contents/Resources'); mkdirSync(accentedMac, { recursive: true });
  assert.equal(appleAssets(app, accentedMac, { platform: 'macos' }).NSAccentColorName, 'AccentColor', 'macOS names it too');
  rmSync(resolve(dir, source, 'AccentColor.colorset'), { recursive: true, force: true });
  const mac = resolve(dir, 'Catalog.bundle/Contents/Resources'); mkdirSync(mac, { recursive: true });
  put('Catalog.bundle/Contents/Info.plist', '<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.exact.catalog.test</string></dict></plist>');
  appleAssets(app, mac, { platform: 'macos', kept: cache });
  assert.ok(inspect(mac).some(asset => asset.Name === 'Badge'));
  assert.ok(readdirSync(cache.dir).some(name => name.startsWith('assets-ios-')), 'macOS must not evict the iOS cache');
  put('Lookup.swift', 'import AppKit\nlet bundle = Bundle(path: CommandLine.arguments[1])!\nguard bundle.image(forResource: "Badge") != nil, let colour = NSColor(named: "Ink", bundle: bundle)?.usingColorSpace(.sRGB), abs(colour.redComponent - 0.8) < 0.01 else { fatalError("named asset lookup failed") }\nprint("named assets loaded")\n');
  const lookup = resolve(dir, 'lookup');
  const compile = spawnSync('xcrun', ['swiftc', resolve(dir, 'Lookup.swift'), '-o', lookup], { encoding: 'utf8' });
  assert.equal(compile.status, 0, compile.stderr);
  const loaded = spawnSync(lookup, [resolve(dir, 'Catalog.bundle')], { encoding: 'utf8' });
  assert.equal(loaded.status, 0, loaded.stderr);
  assert.match(loaded.stdout, /named assets loaded/);
  rmSync(resolve(dir, source), { recursive: true });
  appleAssets(app, mac, { platform: 'macos', kept: cache });
  assert.ok(!existsSync(resolve(mac, 'Assets.car')), 'removing all catalogs clears the previous output');
}), 60000);

// LLP 1116 D6: the app.ts check's warnings (here a caught `new Function`, which throws on every host, #128) are said
// after the refusals, and `--errors-only` leaves them out in either form, the array and the text.
test('contract build --errors-only leaves out the app.ts check\'s warnings, with --json and without', () => inDir(dir => {
  writeFileSync(resolve(dir, 'app.contract'), 'shape Greeting\n  text: string\ncomponent App\n  resource greeting = greeting("x") as shape Greeting\n  view\n    text greeting.text\n');
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Strings', app: { id: 'com.example.strings', name: 'Strings' } }));
  writeFileSync(resolve(dir, 'app.ts'), "import type { Answer, Result, Sources } from './app.contract.d.ts';\nexport const grants = '';\n"
    + "const sources: Sources = { greeting: ([name]): Result<'greeting'> => { try { new Function(''); } catch {} return { text: name }; } };\n"
    + 'export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);\n');
  const build = (...flags) => spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), 'contract', 'build', 'app.contract', ...flags], { cwd: dir, encoding: 'utf8', timeout: 600_000 });
  const warning = /^app\.ts:3:\d+ warning \[code-from-string\] uses 'Function', which builds code from a string/m;
  let r = build();
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, warning);
  r = build('--errors-only');
  assert.equal(r.status, 0, r.stderr);
  assert.doesNotMatch(r.stderr, /code-from-string/);
  r = build('--json');
  assert.deepEqual(JSON.parse(r.stdout).map((d) => [d.id, d.severity, d.file, d.line]), [['code-from-string', 'warning', 'app.ts', 3]]);
  r = build('--json', '--errors-only');
  assert.deepEqual([r.status, r.stdout], [0, '[]\n']);
  // With an error too (a type error, which the same check finds): the error first, then the warning; under
  // --errors-only, the error alone.
  writeFileSync(resolve(dir, 'app.ts'), readFileSync(resolve(dir, 'app.ts'), 'utf8') + "export const count: number = 'one';\n");
  const refused = /^app\.ts[:(]5[:,]\d+.*TS2322/m;
  r = build();
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stderr, refused);
  assert.match(r.stderr, warning);
  assert.ok(r.stderr.search(refused) < r.stderr.search(warning), r.stderr);
  r = build('--errors-only');
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stderr, refused);
  assert.doesNotMatch(r.stderr, /code-from-string/);
  const said = (r) => JSON.parse(r.stdout).map((d) => [d.severity, d.file, d.line]);
  r = build('--json');
  assert.deepEqual(said(r), [['error', 'app.ts', 5], ['warning', 'app.ts', 3]]);
  r = build('--json', '--errors-only');
  assert.deepEqual(said(r), [['error', 'app.ts', 5]]);
}), 600_000);

// `exact.mjs docs <words>`: the section that holds the words as whole words, most densely, first; printed from its
// heading when the heading is on the words, else from the paragraph that mentions them. A word may start with `-`.
test('docs prints the section on its words first, matching whole words', () => {
  const root = resolve(import.meta.dir, '..');
  const first = (...words) => {
    const r = spawnSync(process.execPath, [resolve(root, 'scripts/exact.mjs'), 'docs', ...words], { cwd: root, encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
    const [where, ...text] = r.stdout.split('\n');
    return { where, text: text.slice(0, 40).join('\n') };
  };
  for (const [words, file, shows] of [
    [['exit', 'animation'], 'docs/recipes/motion.md', /-exact-exit-animation/],
    [['-exact-exit-animation'], 'docs/recipes/motion.md', /-exact-exit-animation/],
    [['call'], 'docs/recipes/native-module.md', /func call/],
    [['later'], 'docs/reference.md', /Answer::Later/],
    [['screenshot'], 'docs/recipes/ios-driving.md', /screenshot\s+\.exact\/x\.png compare/],
    [['scheme', 'fixed'], 'docs/recipes/theme.md', /scheme=fixed/],
    [['tabular', 'numbers'], 'docs/contract-for-agents.md', /^### Tabular numbers/],
    [['grant'], 'docs/recipes/files-and-grants.md', /^# Recipe: files, SQLite and grants/],
    [['sheet'], 'docs/recipes/navigation-stack.md', /Cancel\/Save sheet/],
    [['sheets'], 'docs/recipes/navigation-stack.md', /Cancel\/Save sheet/],
    // A whole word: `logs` is the driver's op, not "catalogs".
    [['logs'], 'docs/contract-for-agents.md', /^The driver has ten operations[^]*`logs`/],
    [['colours'], 'docs/recipes/theme.md', /^# Recipe: theme: colours/],
    // A heading that alone holds the word prints from the heading.
    [['accessibility'], 'docs/contract-for-agents.md', /^### Native views: accessibility\n+\S/],
    [['optimistic'], 'docs/contract-for-agents.md', /^### Optimistic writes: the overlay\n+\S/],
    // What an agent porting an iOS app types. A piece of a longer hyphenated name is not the word (`fonts` is not
    // `font-variant-numeric`), a plural never stems onto a type keyword (`strings` is app.json's key, not `: string`),
    // and a query's own section (TOPICS) comes first however it says the words ("dark mode": light and dark).
    [['fonts'], 'docs/recipes/app-json.md', /a `font` block's/],
    [['strings'], 'docs/recipes/app-json.md', /`strings`/],
    [['push'], 'docs/recipes/navigation-stack.md', /^routes nav$/m],
    [['deploy'], 'docs/recipes/porting-and-release.md', /Name the port when you make it/],
    [['haptics'], 'docs/start-here.md', /haptic\("selection"\)/],
    [['platform', 'checks'], 'docs/start-here.md', /agent macos --chrome platform "clock data" "screenshot \.exact\/mac\.png window"/],
    [['glass'], 'docs/contract-for-agents.md', /glass/],
    [['detent'], 'docs/contract-for-agents.md', /detent/],
    [['tab', 'bar'], 'docs/recipes/tabs.md', /iOS's bar/],
    [['back', 'button'], 'docs/recipes/navigation-stack.md', /`button id="back"`/],
    [['search'], 'docs/recipes/list-rows.md', /search/],
    [['menu'], 'docs/recipes/list-rows.md', /menu/],
    [['swipe'], 'docs/recipes/list-rows.md', /swipe/],
    [['list'], 'docs/recipes/list-rows.md', /^# Recipe: list rows/],
    [['safe', 'area'], 'docs/contract-grammar.md', /safe area/],
    [['keyboard'], 'docs/recipes/keyboard-and-focus.md', /^# Recipe: the keyboard and focus/],
    [['dark', 'mode'], 'docs/recipes/theme.md', /follows light, dark/],
    [['image'], 'docs/recipes/app-json.md', /image "assets\/logo.png"/],
    [['svg'], 'docs/recipes/theme.md', /svg "icons\/play\.svg"/],
    [['animation'], 'docs/recipes/motion.md', /animation/],
    [['transition'], 'docs/recipes/motion.md', /transition/],
    [['timer'], 'docs/recipes/timers.md', /^# Recipe: timers/],
    [['storage'], 'docs/recipes/files-and-grants.md', /storage/],
    [['share'], 'docs/contract-grammar.md', /`share`/],
    [['MIDI'], 'docs/contract-grammar.md', /exactMIDI/],
    // A table of contents names every section's words and is no section's answer.
    [['sound', 'midi', 'and', 'media'], 'docs/contract-for-agents.md', /^### Sound, MIDI and media/],
    // A CSS name's words together are the phrase, though its pieces alone are not the words.
    [['text', 'overflow'], 'docs/agent-pitfalls.md', /text-overflow/],
    [['background', 'color'], 'docs/recipes/theme.md', /background-color/],
  ]) {
    const { where, text } = first(...words);
    assert.ok(where.startsWith(`${file}:`), `docs ${words.join(' ')}: ${where}`);
    assert.match(text, shows, `docs ${words.join(' ')} prints from the lines that hold them:\n${text}`);
  }
});


test.skipIf(!tools)('release keeps Mac widget entitlements, including the provisioned app group (LLP 1126.001 WP16)', async () => {
  const { macWidgetEntitlements } = await import('../host/apple/widgets.mjs');
  inDir(dir => {
    const bundle = resolve(dir, 'Widget.app'), extension = resolve(bundle, 'Contents/PlugIns/ExactWidgets.appex');
    const app = { id: 'dev.exact.widget', manifest: {} }, team = '0123456789';
    const run = (command, args, input) => { const r = spawnSync(command, args, { encoding: 'utf8', input }); assert.equal(r.status, 0, r.stderr); return r.stdout; };
    writeFileSync(resolve(dir, 'main.c'), 'int main(void) { return 0; }');
    for (const [path, id] of [[bundle, app.id], [extension, app.id + '.widgets']]) {
      mkdirSync(resolve(path, 'Contents/MacOS'), { recursive: true });
      run('clang', ['-O', resolve(dir, 'main.c'), '-o', resolve(path, 'Contents/MacOS/Fixture')]);
      writeFileSync(resolve(path, 'Contents/Info.plist'), plistFile({ CFBundleIdentifier: id, CFBundleExecutable: 'Fixture', CFBundlePackageType: path === bundle ? 'APPL' : 'XPC!' }));
    }
    const dump = path => JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', '-'], run('codesign', ['-d', '--entitlements', '-', '--xml', path])));
    for (const provisioned of [false, true]) {
      const profiles = provisioned ? { app: { prefix: 'PREFIX' }, keychain: 'PREFIX.' + app.id } : null;
      writeFileSync(resolve(bundle, 'Contents/Info.plist'), plistFile({ CFBundleIdentifier: app.id, CFBundleExecutable: 'Fixture', CFBundlePackageType: 'APPL',
        ExactAppGroup: team + '.' + app.id + '.widgets', ...(profiles ? { ExactKeychainAccessGroup: profiles.keychain } : {}) }));
      if (profiles) writeFileSync(resolve(bundle, 'Contents/embedded.provisionprofile'), 'profile presence: this fixture tests signing, not profile authorization');
      const ent = resolve(dir, 'widget.plist'); writeFileSync(ent, macWidgetEntitlements({ app, team, profiles, extension: true }));
      run('codesign', ['--force', '--sign', '-', '--entitlements', ent, extension]);
      const before = dump(extension);
      signRelease(bundle, { compat: { reach: { entitlements: ['com.apple.security.device.audio-input'] } }, manifest: {}, identity: '-', id: app.id, scratch: dir, timestamp: ['--timestamp=none'] });
      assert.deepEqual(dump(extension), before);
      const own = dump(bundle);
      assert.deepEqual(own['com.apple.security.application-groups'], [team + '.' + app.id + '.widgets']);
      assert.equal(own['com.apple.security.device.audio-input'], true);
      assert.equal(own['com.apple.security.app-sandbox'], undefined);
      assert.equal(own['com.apple.security.network.client'], undefined);
      assert.equal(own['com.apple.application-identifier'], profiles?.keychain);
      assert.deepEqual(own['keychain-access-groups'], profiles ? [profiles.keychain] : undefined);
      run('codesign', ['--verify', '--deep', '--strict', bundle]);
    }
  });
}, 60000);

test('setup refuses the removed game warmup flag before provisioning', () => {
  const result=spawnSync(process.execPath,[resolve(import.meta.dir,'exact.mjs'),'setup','--game'],{encoding:'utf8'});
  assert.notEqual(result.status,0);
  assert.match(result.stderr,/Usage: exact setup \[--check \[--platform/);
  assert.doesNotMatch(result.stdout,/warming|cargo fetch|rustup toolchain install/);
});

for (const scenario of ['home proxy missing toolchain', 'symlinked rustup proxy', 'symlinked proxy missing toolchain', 'symlinked proxy rejected', 'symlinked proxy absent', 'candidate unrelated failure', 'skip rejected candidate', 'reject every candidate', 'copied proxy missing toolchain', 'unrelated probe failure', 'Windows exe candidate', 'Windows fallback', 'Windows Path environment']) {
  test.skipIf(process.platform === 'win32')(`cargo proxy advice: ${scenario}`, async () => {
    const { cargoProxyFault } = await import('./exact.mjs');
    inDir(dir => {
      const pin = '1.97.0', calls = [];
      const reject = 'echo \'error: no such command: `+1.97.0`\' >&2\nexit 101';
      const missing = `echo "error: toolchain '1.97.0-aarch64-apple-darwin' is not installed" >&2\nexit 1`;
      const exe = scenario === 'Windows exe candidate' || scenario === 'Windows fallback' ? '.exe' : '';
      const program = (folder, name, body) => {
        const path = resolve(dir, folder, name + exe);
        mkdirSync(dirname(path), { recursive: true });
        writeFileSync(path, `#!/bin/sh\n${body}\n`, { mode: 0o755 });
        return path;
      };
      const cargo = program('first', 'cargo', scenario === 'copied proxy missing toolchain' ? missing : scenario === 'unrelated probe failure' ? 'echo broken >&2\nexit 1' : reject);
      const rustup = program(scenario.startsWith('symlinked') ? 'first' : 'second', 'rustup', 'exit 0');
      const candidate = program('second', 'cargo', scenario === 'skip rejected candidate' || scenario === 'reject every candidate' || scenario === 'Windows fallback' ? reject : 'exit 0');
      const home = program('home/bin', 'cargo', scenario === 'reject every candidate' || scenario === 'Windows fallback' ? reject : 'exit 0');
      const linked = scenario.startsWith('symlinked');
      const target = linked ? realpathSync(program('keg/bin', 'rustup', 'exit 0')) : null;
      let replacement = home;
      if (linked) {
        rmSync(rustup); symlinkSync(target, rustup); rmSync(candidate); rmSync(home);
        if (scenario !== 'symlinked proxy absent') replacement = program('keg/bin', 'cargo', scenario === 'symlinked proxy rejected' ? reject : scenario === 'symlinked proxy missing toolchain' ? missing : 'exit 0');
      } else if (scenario === 'home proxy missing toolchain' || scenario === 'candidate unrelated failure') {
        writeFileSync(candidate, `#!/bin/sh\n${reject}\n`);
        writeFileSync(home, `#!/bin/sh\n${scenario === 'home proxy missing toolchain' ? missing : 'echo broken >&2\nexit 1'}\n`);
      }
      if (scenario === 'copied proxy missing toolchain') cpSync(cargo, rustup);
      const path = `${dirname(cargo)}:${dirname(rustup)}`;
      const env = { HOME: resolve(dir, 'user'), CARGO_HOME: resolve(dir, 'home'), [scenario === 'Windows Path environment' ? 'Path' : 'PATH']: path };
      if (scenario === 'home proxy missing toolchain') {
        delete env.CARGO_HOME;
        replacement = program('user/.cargo/bin', 'cargo', missing); rmSync(home);
      }
      // These are shell stand-ins on macOS, even when the selection rules are Windows's.
      const execute = (command, args, options) => {
        calls.push([command, ...args]);
        return spawnSync(command === 'cargo' ? cargo : command, args, options);
      };
      const os = scenario.startsWith('Windows') || scenario === 'copied proxy missing toolchain' ? 'win32' : 'darwin';
      const fault = cargoProxyFault(pin, env, execute, os);
      if (scenario === 'copied proxy missing toolchain' || scenario === 'unrelated probe failure') {
        assert.equal(fault, null);
      } else if (['reject every candidate', 'Windows fallback', 'symlinked proxy rejected', 'symlinked proxy absent', 'candidate unrelated failure'].includes(scenario)) {
        assert.match(fault, /no working rustup cargo proxy found/i);
        assert.doesNotMatch(fault, /: put /);
        for (const folder of [dirname(cargo), dirname(rustup), dirname(home), ...(linked ? [dirname(target)] : [])]) assert.ok(fault.includes(folder), fault);
        assert.match(fault, /Repair or install rustup's proxies/);
        assert.doesNotMatch(fault, /brew/);
      } else {
        const recommended = linked ? realpathSync(replacement) : scenario === 'home proxy missing toolchain' ? replacement : scenario === 'skip rejected candidate' ? home : candidate;
        assert.equal(fault, `${cargo} is not rustup's cargo and refuses +${pin}: put ${dirname(recommended)} before ${dirname(cargo)} on PATH`);
        assert.ok(calls.some(call => call[0] === recommended && call[1] === `+${pin}` && call[2] === '--version'), JSON.stringify(calls));
        assert.equal(spawnSync(recommended, [`+${pin}`, '--version'], { env }).status, scenario.includes('missing toolchain') ? 1 : 0);
      }
    });
  });
}

test.skipIf(process.platform === 'win32')("setup names rustup's directory when another cargo, which refuses +toolchain, is first on PATH", async () => {
  const { cargoProxyFault } = await import('./exact.mjs');
  inDir((dir) => {
    // rustup's cargo is a link to rustup; another's is a program of its own, which Homebrew links beside a linked rustup.
    const bin = (name, body) => {
      mkdirSync(resolve(dir, name), { recursive: true });
      for (const tool of ['rustup', 'cargo']) if (body[tool]) writeFileSync(resolve(dir, name, tool), `#!/bin/sh\n${body[tool]}\n`, { mode: 0o755 });
      if (body.rustup && !body.cargo) symlinkSync('rustup', resolve(dir, name, 'cargo'));
      return resolve(dir, name);
    };
    const reject = 'echo \'error: no such command: `+1.97.0`\' >&2\nexit 101';
    const rustup = bin('rustup', { rustup: 'exit 0' }), brew = bin('brew', { cargo: reject }), linked = bin('linked', { cargo: reject, rustup: 'exit 0' });
    const env = (PATH, CARGO_HOME = resolve(dir, 'none')) => ({ PATH, CARGO_HOME });
    assert.equal(cargoProxyFault('1.97.0', env(`${rustup}:${brew}`)), null);
    assert.equal(cargoProxyFault('1.97.0', env(`${brew}:${rustup}`)), `${brew}/cargo is not rustup's cargo and refuses +1.97.0: put ${rustup} before ${brew} on PATH`);
    // Sharing rustup's directory, the cargo is still named, with the directory rustup-init makes ($CARGO_HOME/bin) when it holds rustup's.
    mkdirSync(resolve(dir, 'home'));
    symlinkSync(rustup, resolve(dir, 'home/bin'));
    assert.equal(cargoProxyFault('1.97.0', env(`${linked}:${rustup}`, resolve(dir, 'home'))), `${linked}/cargo is not rustup's cargo and refuses +1.97.0: put ${resolve(dir, 'home/bin')} before ${linked} on PATH`);
    assert.match(cargoProxyFault('1.97.0', env(linked)), /no working rustup cargo proxy found/);
    // rustup's own cargo failing (a toolchain not installed) is not this fault.
    writeFileSync(resolve(rustup, 'rustup'), '#!/bin/sh\nexit 1\n');
    assert.equal(cargoProxyFault('1.97.0', env(rustup)), null);
  });
});

test("setup's Cargo crates row fails on a registry without exact2's locked crates and names the fetch (#48)", async () => {
  const { cratesRow } = await import('./exact.mjs');
  // A workspace whose lock names no registry crate has every source on an empty Cargo home.
  inDir((dir) => {
    const home = resolve(dir, 'home'), crate = resolve(dir, 'crate');
    mkdirSync(home); mkdirSync(resolve(crate, 'src'), { recursive: true });
    writeFileSync(resolve(crate, 'Cargo.toml'), '[package]\nname = "fixture"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n');
    writeFileSync(resolve(crate, 'src/lib.rs'), '');
    writeFileSync(resolve(crate, 'Cargo.lock'), 'version = 4\n\n[[package]]\nname = "fixture"\nversion = "0.1.0"\n');
    const row = cratesRow({ ...process.env, CARGO_HOME: home }, resolve(crate, 'Cargo.toml'));
    assert.equal(row.ok, true, row.have);
    assert.equal(row.have, 'in Cargo cache');
  });
  // exact2's lock on a clean CI worker's: Cargo's home has no index and no crates.
  inDir((home) => {
    const row = cratesRow({ ...process.env, CARGO_HOME: home });
    assert.equal(row.ok, false);
    assert.ok(row.required);
    assert.match(row.have, /no matching package named|--offline was specified/);
    assert.ok(row.fix.startsWith(`cargo fetch --locked --manifest-path ${resolve(import.meta.dir, '../Cargo.toml')}`), row.fix);
  });
}, 60_000);

test.skipIf(process.platform === 'win32')("the setup report's Rust row and exact contract name another's cargo, rustup's rustc working", async () => {
  const { sdkReport } = await import('./exact.mjs');
  const pin = Bun.TOML.parse(readFileSync(resolve(import.meta.dir, '../rust-toolchain.toml'), 'utf8')).toolchain.channel;
  inDir((brew) => {
    writeFileSync(resolve(brew, 'cargo'), `#!/bin/sh\necho 'error: no such command: \`+${pin}\`' >&2\nexit 101\n`, { mode: 0o755 });
    const proxy = resolve(brew, 'proxy'); mkdirSync(proxy);
    for (const tool of ['cargo', 'rustup', 'rustc']) writeFileSync(resolve(proxy, tool), `#!/bin/sh\necho '${tool} ${pin}'\n`, { mode: 0o755 });
    const env = { ...process.env, PATH: `${brew}:${proxy}:${process.env.PATH}` };
    assert.equal(spawnSync(resolve(proxy, 'cargo'), [`+${pin}`, '--version'], { env }).status, 0);
    const fault = `${brew}/cargo is not rustup's cargo and refuses +${pin}: put ${proxy} before ${brew} on PATH`;
    const rust = sdkReport(env, { only: 'web' }).find(row => row.name === 'Rust');
    assert.equal(rust.ok, false);
    assert.equal(rust.fix, fault);
    const vocab = spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), 'contract', 'vocab', 'button'], { env, encoding: 'utf8' });
    assert.equal(vocab.status, 101);
    assert.ok(vocab.stderr.includes(`exact contract: ${fault}`), vocab.stderr);
  });
}, 60_000);

test("without CHROME, the setup report's Chrome row wants both of Playwright's pinned builds and names the missing one", () => inDir((browsers) => {
  const { CHROME, ...env } = process.env;
  // With `shell`, a stand-in headless shell where the pinned Playwright looks for it; the full build is never there.
  const row = (shell) => {
    const script = `const { chromium } = await import('./scripts/agent-launch.mjs'), { mkdirSync, writeFileSync } = await import('node:fs'), { dirname } = await import('node:path');
      if (${shell}) { const { executable } = chromium(); mkdirSync(dirname(executable), { recursive: true }); writeFileSync(executable, '', { mode: 0o755 }); }
      const { sdkReport } = await import('./scripts/exact.mjs'); console.log(JSON.stringify(sdkReport(process.env, { only: 'web' }).find(row => row.name === 'Chrome')))`;
    const result = spawnSync(process.execPath, ['-e', script], { cwd: resolve(import.meta.dir, '..'), encoding: 'utf8', env: { ...env, PLAYWRIGHT_BROWSERS_PATH: browsers } });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout.trim().split('\n').at(-1));
  };
  const none = row(false);
  assert.equal(none.ok, false);
  assert.match(none.fix, /^exact setup, or set CHROME to a browser \(docs\/testing\.md, Browsers\): Chromium headless shell is missing at .*; Chromium is missing at /);
  const shellOnly = row(true);
  assert.equal(shellOnly.ok, false, 'the full build a GPU drive or perf run launches is missing');
  assert.match(shellOnly.fix, /: Chromium is missing at [^;]*$/);
}), 60_000);

// LLP 1153 D1: an app `exact new` makes has no crate directories; its platforms are app.json's `host` keys and its
// crates are generated, reading the stubs from modules/. `hatch` writes for those platforms.
test('exact hatch writes the stubs of an app whose crates are generated', async () => {
  const { hatch } = await import('./exact.mjs');
  const { existsSync, readFileSync } = await import('node:fs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-hatch-new-'));
  const was = process.env.EXACT_APP_DIR;
  try {
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    column hatch="avatar"\n');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Fresh', app: { id: 'com.example.fresh', name: 'Fresh' }, host: { web: {}, ios: {}, macos: {}, linux: {}, android: {} } }));
    process.env.EXACT_APP_DIR = dir;
    const made = hatch(['avatar'], () => {});
    assert.deepEqual(made.platforms, ['ios', 'macos', 'web', 'linux', 'android']);
    for (const file of ['modules/apple/Hatches.swift', 'modules/web/index.js', 'modules/linux/hatches.rs', 'modules/android/Hatches.kt']) assert.ok(existsSync(resolve(dir, file)), file);
    assert.deepEqual(made.todo, []);
    assert.deepEqual(JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')).hatches, { avatar: ['ios', 'macos', 'web', 'linux', 'android'] });
  } finally {
    if (was === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = was;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('exact add says how each grant line is written, from the device table', async () => {
  const { grantAdvice } = await import('./exact.mjs');
  // A row nothing asks the person about is written alone, beside the row it needs (LLP 1133.004 §9 P2).
  assert.equal(grantAdvice(['device.push']), '`device.push` alone, beside `device.notifications purpose.<key>`');
  assert.equal(grantAdvice(['device.notifications']), '`device.notifications purpose.<key>`, a strings key whose text is the purpose');
  assert.equal(grantAdvice(['device.attestation']), '`device.attestation` alone');
  assert.match(grantAdvice(['device.geolocation', 'device.geolocation-approximate']), /^one of `device.geolocation purpose.<key>`.*; `device.geolocation-approximate purpose.<key>`/);
  // Every other namespace as its own grammar has it, never in the device table's form
  // (bake/src/reach.rs `grant_lines_parse` parses each package's lines with the real parsers).
  assert.equal(grantAdvice(['auth.apple', 'auth.google']), 'one of `auth.apple <client id>`, a client id the provider issued; `auth.google <client id>`, a client id the provider issued');
  assert.equal(grantAdvice(['payment.apple-pay', 'payment.google-pay']), 'one of `payment.apple-pay <merchant id>`, an Apple Pay merchant ID; `payment.google-pay` alone');
  assert.equal(grantAdvice(['browser.automation']), '`browser.automation` alone');
  assert.equal(grantAdvice(['net.fetch']), '`net.fetch <its target>`, what it reaches');
});

test('add writes a relative package specifier within a shared tree, including through symlinks', async () => {
  const { packageSpecifier } = await import('./exact.mjs');
  inDir(dir => {
    const app = resolve(dir, 'nested/shop'), sdk = resolve(dir, 'sdk');
    mkdirSync(app, { recursive: true });
    mkdirSync(resolve(sdk, 'packages/in-app-browser'), { recursive: true });
    assert.equal(packageSpecifier(app, 'in-app-browser', sdk), 'file:../../sdk/packages/in-app-browser');
    symlinkSync(app, resolve(dir, 'linked-shop'), 'dir');
    symlinkSync(sdk, resolve(dir, 'linked-sdk'), 'dir');
    assert.equal(packageSpecifier(resolve(dir, 'linked-shop'), 'in-app-browser', resolve(dir, 'linked-sdk')), 'file:../../sdk/packages/in-app-browser');
  });
});

test('add installs a standard package by pathFrom from an outside app with an absolute EXACT', () => inDir(dir => {
  const root = resolve(import.meta.dir, '..'), app = resolve(dir, 'nested/shop');
  const made = spawnSync(process.execPath, [resolve(root, 'scripts/exact.mjs'), 'new', app], { encoding: 'utf8' });
  assert.equal(made.status, 0, made.stdout + made.stderr);
  const added = spawnSync(process.execPath, ['exact.mjs', 'add', '@exact/in-app-browser'], {
    cwd: app, encoding: 'utf8', env: { ...process.env, EXACT: root },
  });
  assert.equal(added.status, 0, added.stdout + added.stderr);
  assert.equal(added.stdout.trim().split('\n').at(-1), `For Android, after importing the package, run: in '${realpathSync(app)}': bun exact.mjs android --write-locks`);
  const spec = JSON.parse(readFileSync(resolve(app, 'package.json'), 'utf8')).dependencies['@exact/in-app-browser'];
  assert.equal(spec, `file:${pathFrom(app, resolve(root, 'packages/in-app-browser'))}`);
  assert.equal(JSON.parse(readFileSync(resolve(app, 'node_modules/@exact/in-app-browser/package.json'), 'utf8')).name, '@exact/in-app-browser');
  const again = spawnSync(process.execPath, ['exact.mjs', 'add', '@exact/in-app-browser'], { cwd: app, encoding: 'utf8' });
  assert.equal(again.status, 0, again.stdout + again.stderr);
  assert.equal(JSON.parse(readFileSync(resolve(app, 'package.json'), 'utf8')).dependencies['@exact/in-app-browser'], spec);
  const frozen = spawnSync(process.execPath, ['install', '--frozen-lockfile'], { cwd: app, encoding: 'utf8' });
  assert.equal(frozen.status, 0, frozen.stdout + frozen.stderr);
  // Each command add's advice names, on success and on a refusal, is a verb of the app's exact.mjs, spelled as the app
  // runs it (`bun exact.mjs <verb>`), never as `exact <verb>`, which needs a shim the app does not have.
  const geolocation = spawnSync(process.execPath, ['exact.mjs', 'add', 'geolocation'], { cwd: app, encoding: 'utf8' });
  assert.equal(geolocation.status, 0, geolocation.stdout + geolocation.stderr);
  const refused = spawnSync(process.execPath, ['exact.mjs', 'add', 'no-such-module'], { cwd: app, encoding: 'utf8' });
  assert.notEqual(refused.status, 0, refused.stdout + refused.stderr);
  const said = `${geolocation.stdout}${refused.stderr}`, named = [...said.matchAll(/bun exact\.mjs ([a-z-]+)/g)].map((m) => m[1]);
  assert.deepEqual(said.match(/(?<![\w./-])exact [a-z-]+/g), null);
  assert.ok(named.includes('add') && named.includes('docs'), said);
  assert.deepEqual(named.filter((verb) => !(verb in appVerbs('shop', app))), []);
  rmSync(resolve(app, 'exact.mjs'));
  const fromRepo = spawnSync(process.execPath, [resolve(root, 'scripts/exact.mjs'), 'add', '@exact/in-app-browser'], {
    cwd: root, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: app },
  });
  assert.equal(fromRepo.status, 0, fromRepo.stdout + fromRepo.stderr);
  assert.equal(fromRepo.stdout.trim().split('\n').at(-1), `For Android, after importing the package, run: in '${root}': bun scripts/agent-android.mjs build 'shop' --apk --write-locks, with EXACT_APP_DIR set to '${realpathSync(app)}'`);
}));

test.skipIf(process.platform === 'win32')('setup reports Gradle SDK packages and their install commands without running sdkmanager', async () => {
  const { sdkReport, platformRowNames } = await import('./exact.mjs');
  const { androidJavaEnv, androidSdkPackages, androidSdkFix } = await import('./agent-android.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-sdk-'));
  try {
    const sdk = resolve(root, 'SDK with spaces'), jdk = resolve(root, 'JDK with spaces');
    mkdirSync(resolve(jdk, 'bin'), { recursive: true });
    writeFileSync(resolve(jdk, 'bin/java'), '#!/bin/sh\necho \'openjdk version "21.0.1"\' >&2\n', { mode: 0o755 });
    writeFileSync(resolve(jdk, 'bin/javac'), '#!/bin/sh\nexit 0\n', { mode: 0o755 });
    const tool = resolve(sdk, 'cmdline-tools/19.0/bin/sdkmanager');
    mkdirSync(dirname(tool), { recursive: true });
    writeFileSync(tool, '#!/bin/sh\nexit 99\n', { mode: 0o755 });
    const env = { ...process.env, ANDROID_HOME: sdk, JAVA_HOME: jdk }, packages = androidSdkPackages();
    assert.deepEqual(androidSdkPackages('compileSdk { version = release(42) { minorApiLevel = 3 } }\nbuildToolsVersion = "41.0.2"').map(p => p.id), ['platforms;android-42.3', 'build-tools;41.0.2']);
    assert.throws(() => androidSdkPackages(''), /cannot read/);
    const rows = () => sdkReport(env, { only: 'android' }).filter(r => packages.some(p => p.name === r.name));
    assert.deepEqual(rows().map(r => [r.name, r.ok, r.want]), packages.map(p => [p.name, false, p.version]));
    for (const [i, row] of rows().entries()) {
      assert.equal(row.fix, `'${tool}' '--sdk_root=${sdk}' '${packages[i].id}'`);
      mkdirSync(dirname(resolve(sdk, packages[i].path)), { recursive: true });
      writeFileSync(resolve(sdk, packages[i].path), 'installed');
    }
    assert.ok(rows().every(r => r.ok));
    assert.ok(!sdkReport(env, { only: 'web' }).some(r => r.name.startsWith('Android')));
    assert.ok(!platformRowNames('web').some(name => name.startsWith('Android')));
    assert.equal(sdkReport(env, { only: 'android' }).find(r => r.name === 'Android JDK')?.ok, true);
    assert.equal(sdkReport({ ...env, JAVA_HOME: resolve(root, 'absent') }, { only: 'android' }).find(r => r.name === 'Android JDK')?.ok, false);
    assert.equal(androidJavaEnv(env, 'linux').JAVA_HOME, jdk);
    assert.equal(androidJavaEnv({ PATH: '', JAVA_HOME: resolve(root, 'missing-jdk') }, 'linux'), null);
    assert.equal(androidJavaEnv({ PATH: '' }, 'darwin', jdk).JAVA_HOME, jdk);
    assert.equal(androidJavaEnv({ PATH: '' }, 'linux', jdk), null, 'Studio is a macOS fallback');
    writeFileSync(resolve(jdk, 'bin/java'), '#!/bin/sh\necho \'openjdk version "16.0.1"\' >&2\n', { mode: 0o755 });
    assert.equal(androidJavaEnv({ PATH: '' }, 'darwin', jdk), null, 'Studio needs JDK 17 or newer');
    rmSync(tool);
    const pathTool = resolve(root, 'sdkmanager');
    writeFileSync(pathTool, '#!/bin/sh\nexit 99\n', { mode: 0o755 });
    assert.ok(androidSdkFix({ ...env, PATH: root }).startsWith(`'${pathTool}' `), 'PATH-only command-line tools are found');
    assert.equal(androidSdkFix({ ...env, PATH: '' }, packages), `'sdkmanager' '--sdk_root=${sdk}' ${packages.map(p => `'${p.id}'`).join(' ')}`);
    assert.equal(androidSdkFix({ ...env, PATH: '', ANDROID_HOME: "C:/SDK's folder" }, packages, 'win32'),
      `& 'sdkmanager' '--sdk_root=C:/SDK''s folder' ${packages.map(p => `'${p.id}'`).join(' ')}`);
    const absentEnv = { ...env, ANDROID_HOME: resolve(root, 'absent') };
    const absent = sdkReport(absentEnv, { only: 'android' });
    assert.ok(packages.every(p => absent.some(r => r.name === p.name && !r.ok)));
    assert.ok(!sdkReport(absentEnv).some(r => packages.some(p => p.name === r.name)), 'without an SDK the general report omits its packages');
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 60_000);

// These recipes use job values directly; EAS also supports inputs on run steps.
test('EAS recipes export resolved job versions directly and upload the native artifacts', () => {
  for (const [platform, version, artifact] of [
    ['ios', 'buildNumber', 'out.ipa'],
    ['android', 'versionCode', 'target/android/app/outputs/bundle/release/app-release.aab'],
  ]) {
    const doc = readFileSync(resolve(import.meta.dir, `../docs/recipes/eas-${platform}.md`), 'utf8');
    const { build } = Bun.YAML.parse(/```yaml\n([\s\S]*?)```/.exec(doc)[1]);
    const steps = build.steps;
    assert.equal(steps[0], 'eas/checkout');
    const resolved = steps.indexOf('eas/resolve_build_config');
    assert.ok(steps.indexOf('eas/install_node_modules') < resolved);
    const buildStep = steps.findIndex(step => step.run?.command.includes('export EXACT_BUILD_NUMBER='));
    assert.ok(buildStep > resolved);
    const expression = '${ eas.job.version.' + version + ' }';
    assert.ok(steps[buildStep].run.command.includes('export EXACT_BUILD_NUMBER="' + expression + '"'));
    for (const step of steps.filter(step => step.run)) {
      assert.equal(Object.hasOwn(step.run, 'inputs'), false, 'keep job expressions directly in these recipes');
      const command = step.run.command.replaceAll(expression, '42');
      assert.doesNotMatch(command, /\$\{\s*(inputs|eas|steps)\./);
    }
    assert.deepEqual(steps.at(-1), { 'eas/upload_artifact': { inputs: { type: 'application-archive', path: artifact } } });
  }
});

test.skipIf(!existsSync('/bin/sh'))('EAS recipes have valid shell syntax after job versions resolve', () => {
  for (const platform of ['ios', 'android']) {
    const doc = readFileSync(resolve(import.meta.dir, `../docs/recipes/eas-${platform}.md`), 'utf8');
    const { build } = Bun.YAML.parse(/```yaml\n([\s\S]*?)```/.exec(doc)[1]);
    for (const { run } of build.steps.filter(step => step.run)) {
      const command = run.command.replaceAll(/\$\{ eas\.job\.version\.(buildNumber|versionCode) }/g, '42');
      const parsed = spawnSync('/bin/sh', ['-n'], { input: command, encoding: 'utf8' });
      assert.equal(parsed.status, 0, parsed.stderr);
    }
  }
});
