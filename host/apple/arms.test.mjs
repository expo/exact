import { test, expect } from 'bun:test';
import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { armArgs, armKey, armSourceKey, armOutput, contentStamp, evictArms, hasIntentSignal, keepRoster, keptRoster, normalizeModuleCacheArgs, packageKeyArgs, probeDigest, rosterRoutes, snapshotSources } from './arms.mjs';
import { staleModuleCache } from './extensions.mjs';
import { gathered, nativeIntentModule } from './intents.mjs';

test('a qualified native roster is bound to the exact probe bytes, including when imported from a simulator run', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-roster-'));
  const probe = resolve(dir, 'probe'), receipt = resolve(dir, 'qualification.json');
  const roster = JSON.stringify({ 'native-view': { firstFrame: true }, extensions: {} });
  const bytes = Buffer.from('first compiled probe'), hash = createHash('sha256').update(bytes).digest('hex');
  const env = { EXACT_NATIVE_ROSTERS: receipt };
  try {
    writeFileSync(probe, bytes);
    expect(keptRoster(probe, {})).toBeUndefined();
    keepRoster(probe, roster); expect(keptRoster(probe, {})).toBe(roster);
    writeFileSync(probe, 'changed compiled probe');
    expect(keptRoster(probe, {})).toBeUndefined();
    writeFileSync(receipt, JSON.stringify({ version: 1, probes: [{ probeSha256: hash, roster }] }));
    expect(keptRoster(probe, env)).toBeUndefined();
    writeFileSync(probe, bytes); writeFileSync(`${probe}.roster`, '{}');
    expect(keptRoster(probe, env)).toBe(roster);
    expect(JSON.parse(readFileSync(`${probe}.roster`, 'utf8')).probeSha256).toBe(hash);
    expect(keptRoster(probe, {})).toBe(roster);
    writeFileSync(`${probe}.roster`, '{}');
    writeFileSync(receipt, JSON.stringify({ version: 1, probes: [{ probeSha256: hash, roster }, { probeSha256: hash, roster }] }));
    expect(() => keptRoster(probe, env)).toThrow('duplicate');
    expect(() => keepRoster(probe, '[]')).toThrow('JSON object');
    expect(() => keepRoster(probe, '{')).toThrow();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a probe qualification covers its launcher, library, packages and frameworks after relocation', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-probe-artifact-'));
  const probe = resolve(dir, 'probe'), moved = resolve(dir, 'moved');
  const files = ['ExactRosterProbe', 'libexact_modules.dylib', 'libPackage.dylib', 'Frameworks/Dependency.framework/Dependency'];
  const roster = JSON.stringify({ 'native-view': { firstFrame: true } });
  try {
    for (const file of files) {
      mkdirSync(resolve(probe, file, '..'), { recursive: true });
      writeFileSync(resolve(probe, file), file);
    }
    const hash = probeDigest(probe);
    cpSync(probe, moved, { recursive: true });
    assert.equal(probeDigest(moved), hash);
    const receipt = resolve(dir, 'qualification.json');
    writeFileSync(receipt, JSON.stringify({ version: 1, probes: [{ probeSha256: hash, roster }] }));
    const env = { EXACT_NATIVE_ROSTERS: receipt };
    assert.equal(keptRoster(moved, env), roster);
    for (const file of files) {
      writeFileSync(resolve(moved, file), 'changed');
      assert.equal(keptRoster(moved, env), undefined, file);
      writeFileSync(resolve(moved, file), file);
    }
    rmSync(resolve(moved, files[2]));
    assert.equal(keptRoster(moved, env), undefined);
    for (let i = 0; i < 4; i++) {
      const cached = resolve(dir, `${i.toString(16).padStart(16, '0')}-probe-artifact`);
      cpSync(probe, cached, { recursive: true });
      keepRoster(cached, roster);
      utimesSync(cached, i + 1, i + 1);
    }
    evictArms(dir, 'probe-artifact');
    assert.equal(readdirSync(dir).filter(f => /-probe-artifact$/.test(f)).length, 3);
    assert.equal(readdirSync(dir).filter(f => /-probe-artifact.roster$/.test(f)).length, 3);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("a device build's roster routes each take the packages compiled for that route, and a route lacks only a missing xcframework slice", () => {
  const routes = (build) => rosterRoutes({ ...build, packageDir: '/b/packages', missingSlice: () => undefined, platform: 'darwin' }).map(r => [r.via, r.lacking, r.packages]);
  assert.deepEqual(routes({ tv: false, device: true }), [
    ['Mac Catalyst', undefined, '/b/packages-maccatalyst'],
    ['iOS Simulator', undefined, '/b/packages-simulator'],
    ['macOS', undefined, '/b/packages-macos'],
  ]);
  // A simulator build's simulator route links the build's own package objects.
  assert.deepEqual(routes({ tv: false, device: false }).map(r => r[2]), ['/b/packages-maccatalyst', '/b/packages', '/b/packages-macos']);
  assert.deepEqual(routes({ tv: true, device: true }), [['macOS', undefined, '/b/packages-macos']]);
  // Only an xcframework without the route's slice makes it lack.
  const lacking = rosterRoutes({ tv: false, device: true, packageDir: '/b/packages', missingSlice: (v) => v === 'maccatalyst' ? '/app/modules/apple/Map.xcframework' : undefined, platform: 'darwin' });
  assert.deepEqual(lacking.map(r => r.lacking ?? null), ['/app/modules/apple/Map.xcframework', null, null]);
});

test('an arm dylib compiles under its own name and is named @rpath/<name>, so two builds link the same bytes', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-arms-'));
  const built = '/b/web/libexact_modules.dylib', library = ['swiftc', '-emit-library', '-module-name', 'ExactAppModules', '-o', built];
  try {
    // The linker names the dylib and hashes its file name into LC_UUID: neither may carry the process id.
    const making = armOutput(dir, '0123456789abcdef', built);
    expect(making.split('/').at(-1)).toBe('libexact_modules.dylib');
    expect(making.endsWith(`/.0123456789abcdef-libexact_modules.dylib.${process.pid}.tmp/libexact_modules.dylib`)).toBe(true);
    // Two arms compiling at once in one build (the module artifact and its probe; two outputs of the same arguments) never share a directory.
    expect(armOutput(dir, 'fedcba9876543210', built)).not.toBe(making);
    expect(armOutput(dir, '0123456789abcdef', '/b/web/probe-libexact_modules.dylib')).not.toStartWith(making.slice(0, making.lastIndexOf('/') + 1));
    // A directory a crashed compile left is emptied before the next one.
    writeFileSync(making, 'half-written');
    expect(existsSync(armOutput(dir, '0123456789abcdef', built))).toBe(false);
    assert.deepEqual(armArgs(library, built).slice(library.length), ['-Xlinker', '-install_name', '-Xlinker', '@rpath/libexact_modules.dylib', '-Xlinker', '-headerpad', '-Xlinker', '0x100']);
    // An install name the arguments give (a manifest's host.ios.link) is kept in every spelling the linker takes: an
    // appended default would win, as the linker takes the last.
    for (const given of [['-Xlinker', '-install_name', '-Xlinker', '@rpath/Custom.framework/Custom'], ['-Xlinker', '-dylib_install_name', '-Xlinker', '@rpath/Custom.framework/Custom'],
      ['-Xlinker', '-dylinker_install_name', '-Xlinker', '@rpath/Custom.framework/Custom'], ['-Xclang-linker', '-Wl,-dylib_install_name,@rpath/Custom.framework/Custom']]) {
      expect(armArgs([...library, ...given], built)).toEqual([...library, ...given]);
    }
    // A header pad the arguments set is theirs.
    const padded = [...library, '-Xlinker', '-headerpad_max_install_names'];
    expect(armArgs(padded, built)).toEqual([...padded, '-Xlinker', '-install_name', '-Xlinker', '@rpath/libexact_modules.dylib']);
    const object = ['clang', '-c', 'sound_mixer.c', '-o', '/b/sound.o'];
    expect(armArgs(object, '/b/sound.o')).toEqual(object);
    // A crashed compile's directory goes after a day, as a crashed copy did; a fresh one stays.
    mkdirSync(resolve(dir, '.aaaaaaaaaaaaaaaa.1.tmp')); writeFileSync(resolve(dir, '.aaaaaaaaaaaaaaaa.1.tmp', 'libexact_modules.dylib'), 'x');
    const old = new Date(Date.now() - 2 * 86_400_000); utimesSync(resolve(dir, '.aaaaaaaaaaaaaaaa.1.tmp'), old, old);
    evictArms(dir, 'libexact_modules.dylib');
    expect(readdirSync(dir).sort()).toEqual([`.0123456789abcdef-libexact_modules.dylib.${process.pid}.tmp`, `.0123456789abcdef-probe-libexact_modules.dylib.${process.pid}.tmp`, `.fedcba9876543210-libexact_modules.dylib.${process.pid}.tmp`]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('every arm compiles anew under the own-name output layout, the ones armArgs leaves unchanged too', () => {
  // The key as it was before arms compiled under their own name: arguments, sources, compiler and extra.
  const before = (args, sources, swiftc, extra) => createHash('sha256').update(JSON.stringify([args, sources, swiftc, extra])).digest('hex').slice(0, 16);
  const named = ['swiftc', '-emit-library', '-Xlinker', '-install_name', '-Xlinker', '@rpath/libexact_modules.dylib', '-o', '<out>'];
  const probe = ['swiftc', '-module-name', 'Probe', '-o', '<out>'];
  for (const args of [named, probe]) {
    expect(armArgs(args, '/b/web/libexact_modules.dylib')).toBe(args);
    expect(armKey(args, ['let x = 1'], 'swiftc-6', '')).not.toBe(before(args, ['let x = 1'], 'swiftc-6', ''));
  }
  expect(armKey(probe, ['let x = 1'], 'swiftc-6', '')).toBe(armKey(probe, ['let x = 1'], 'swiftc-6', ''));
  expect(armKey(probe, ['let x = 2'], 'swiftc-6', '')).not.toBe(armKey(probe, ['let x = 1'], 'swiftc-6', ''));
});

test('relocating generated sources keeps private Swift arms only while all inputs match', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-arm-keys-'));
  const key = (run, { name = 'forma.swift', text = 'let x = 1', flag = '-O',
    swiftc = 'swift-6', sdk = '/sdk', target = 'arm64-apple-ios-simulator', extra = 'dependencies',
    authored = 'modules', userText = 'let y = 1', packageText = null, outside = false, packageSuffix = '' } = {}) => {
    const work = resolve(dir, run), linkRoot = resolve(work, 'link');
    const source = resolve(outside ? resolve(dir, 'other') : linkRoot, name);
    const user = resolve(dir, authored, 'User.swift');
    for (const path of [linkRoot, resolve(dir, 'other'), resolve(dir, authored)]) mkdirSync(path, { recursive: true });
    writeFileSync(source, text);
    writeFileSync(user, userText);
    const snapshot = snapshotSources([user], resolve(work, 'copies'));
    const packageDir = resolve(work, 'packages');
    const extraFiles = [];
    if (packageText !== null) {
      const pkg = resolve(packageDir, 'src-Pkg', '0-Pkg.swift');
      mkdirSync(resolve(packageDir, 'src-Pkg'), { recursive: true });
      writeFileSync(pkg, packageText);
      extraFiles.push(pkg);
    }
    const files = [source, ...snapshot.copies, ...extraFiles];
    const built = resolve(work, 'module.dylib');
    const packages = packageDir + packageSuffix;
    const args = armArgs(['swiftc', '-sdk', sdk, '-target', target, '-emit-library', flag, source, ...snapshot.copies,
      '-I', packages, resolve(packages, 'ExactPackage.o'), '-o', built], built);
    const packageDirs = [packageDir, ...[true, false].flatMap(device =>
      rosterRoutes({ packageDir, device, missingSlice: () => false, platform: 'darwin' }).map(r => r.packages + '-probe'))];
    return armSourceKey(args, files, { built, snapshot, packageDir, packageDirs, linkRoot, swiftc, extra });
  };
  try {
    for (const name of ['forma.swift', 'ExactDataKeys-example.swift', 'ExactComposed-example.swift', 'ExactRosterProbe.swift'])
      expect(key('first', { name })).toBe(key('second', { name }));
    // Package sources that use #filePath compile separately with #sourceLocation and do not invalidate app modules.
    expect(key('first', { packageText: '#sourceLocation(file: "/pkg/Contacts.swift", line: 1)\nlet p = #filePath' }))
      .toBe(key('second', { packageText: '#sourceLocation(file: "/pkg/Contacts.swift", line: 1)\nlet p = #filePath' }));
    // An authored file in scratch or next to a package directory retains its path.
    expect(key('first', { name: 'Other.swift' })).not.toBe(key('second', { name: 'Other.swift' }));
    expect(key('first', { authored: 'first/packages-local' })).not.toBe(key('second', { authored: 'second/packages-local' }));
    expect(key('first', { userText: 'let p = #filePath' })).not.toBe(key('second', { userText: 'let p = #filePath' }));
    for (const packageSuffix of ['-macos-probe', '-simulator-probe', '-maccatalyst-probe', '-probe'])
      expect(key('first', { packageSuffix })).toBe(key('second', { packageSuffix }));
    expect(key('first', { packageSuffix: '-unrelated' })).not.toBe(key('second', { packageSuffix: '-unrelated' }));
    const before = key('first');
    for (const changed of [{ name: 'ExactRosterProbe.swift' }, { text: 'let x = 2' }, { flag: '-Onone' },
      { swiftc: 'swift-7' }, { sdk: '/sdk-2' }, { target: 'arm64-apple-ios' }, { extra: 'changed' },
      { authored: 'other-module' }, { userText: 'let y = 2' }, { outside: true }, { packageSuffix: '-macos-probe' }])
      expect(key('second', changed)).not.toBe(before);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('package probe keys follow the copied source, not the private build directory', () => {
  const key = (run, probe, { module = 'A', target = 'simulator', authored = '/app/Shared.swift' } = {}) => {
    const packageDir = resolve(tmpdir(), run, 'packages'), dir = packageDir + (probe ? '-probe' : '');
    const copied = resolve(packageDir, 'src-' + module, '0-Shared.swift');
    return packageKeyArgs(['swiftc', '-target', target, '-I', dir, copied, authored,
      resolve(dir, `adapter-${module}.swift`), '-o', resolve(dir, `${module}.o`)], dir, packageDir, new Set([copied]), module);
  };
  const app = key('first', false);
  expect(key('first', true)).toEqual(app);
  expect(key('second', true)).toEqual(app);
  expect(key('second', true, { target: 'macabi' })).not.toEqual(app);
  expect(key('second', true, { module: 'B' })).not.toEqual(app);
  expect(key('second', true, { authored: '/other/Shared.swift' })).not.toEqual(app);
});

test('package keys retain unrelated arguments even when they contain the private directory', () => {
  const dir = resolve(tmpdir(), 'packages');
  const other = ['-sdk', dir, '-I', dir + '-external', resolve(dir, 'User.swift'), `-DROOT=${dir}`];
  const outputs = ['-L', dir, '-emit-module-path', resolve(dir, 'M.swiftmodule'), '-o', resolve(dir, 'M.dylib')];
  const key = packageKeyArgs([...other, ...outputs], dir, dir, new Set(), 'M');
  expect(key.slice(0, other.length)).toEqual(other);
  expect(key.slice(other.length)).toEqual(['-L', '<dir>', '-emit-module-path', join('<dir>', 'M.swiftmodule'), '-o', join('<dir>', 'M.dylib')]);
});

test('workspace relocation keeps module-cache, ObjC, root, app, sound-object, and swift-package keys stable (#1612)', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-relocate-1612-'));
  try {
    const buildArmKey = (slot, { userText = 'let v = 1', objcHeader = 'int f(void);', soundHash = '0123456789abcdef' } = {}) => {
      const ws = resolve(dir, slot), root = resolve(ws, 'exact2'), appDir = resolve(ws, 'app');
      const linkRoot = resolve(ws, 'link'), swiftRootDir = resolve(ws, 'swift-pkgs'), pkgDir = resolve(ws, 'packages');
      const webCache = resolve(ws, 'cache', 'webarm-module-cache'), modCache = resolve(ws, 'cache', 'modules-module-cache');
      for (const d of [resolve(root, 'host/apple/modules'), resolve(root, 'host/apple/soundarm'), resolve(appDir, 'modules/apple'), linkRoot, swiftRootDir, pkgDir]) {
        mkdirSync(d, { recursive: true });
      }
      const seam = resolve(root, 'host/apple/modules/ExactNativeModule.swift');
      const header = resolve(root, 'host/apple/soundarm/sound_render.h');
      const soundObj = resolve(ws, 'arms', `sound_mixer-c-${soundHash}.o`);
      const user = resolve(appDir, 'modules/apple/View.swift');
      writeFileSync(seam, 'public enum Seam {}');
      writeFileSync(header, objcHeader);
      writeFileSync(user, userText);
      const snapshot = snapshotSources([user], resolve(ws, 'copies'));
      const built = resolve(ws, 'libexact_modules.dylib');
      const args = armArgs([
        'swiftc', '-sdk', '/sdk', '-target', 'arm64-apple-ios17.0',
        '-module-cache-path', modCache, `-fmodules-cache-path=${webCache}`,
        '-emit-library', '-O', seam, ...snapshot.copies,
        '-I', pkgDir, '-I', resolve(swiftRootDir, 'bin'),
        '-import-objc-header', header, soundObj,
        resolve(pkgDir, 'ExactPackage.o'), '-o', built,
      ], built);
      return armSourceKey(args, [seam, header, ...snapshot.copies], {
        built, snapshot, packageDir: pkgDir, packageDirs: [pkgDir], linkRoot, swiftc: 'swift-6', extra: '', root, appDir, swiftRootDir,
      });
    };
    expect(buildArmKey('ws-a')).toBe(buildArmKey('ws-b'));
    expect(buildArmKey('ws-a', { soundHash: 'fedcba9876543210' })).not.toBe(buildArmKey('ws-b'));
    expect(buildArmKey('ws-a', { userText: 'let p = #filePath' })).not.toBe(buildArmKey('ws-b', { userText: 'let p = #filePath' }));

    // ObjC package keys: -I <dir>/objc-<M>, <dir>/<M>_objc_0.o, -fmodules-cache-path=, and exact2 root/app header dirs normalize.
    const pkgKey = (slot, { pathSensitive = false } = {}) => {
      const ws = resolve(dir, slot), root = resolve(ws, 'exact2'), appDir = resolve(ws, 'app'), swiftRootDir = resolve(ws, 'swift-pkgs');
      const packageDir = resolve(ws, 'tmp-build', 'packages'), outDir = packageDir + '-probe';
      const copy = resolve(packageDir, 'src-Media', '0-Media.swift');
      const headerDir = resolve(root, 'packages/media/apple');
      const objcSrc = resolve(root, 'packages/media/apple/Bridge.m');
      const args = [
        '--sdk', 'iphoneos', 'clang', '-O2', '-fmodules',
        `-fmodules-cache-path=${resolve(ws, 'cache/modules-module-cache')}`,
        '-module-cache-path', resolve(ws, 'cache/modules-module-cache'),
        '-I', resolve(outDir, 'objc-Media'), '-I', headerDir,
        '-I', resolve(swiftRootDir, 'bin'), '-c', objcSrc,
        copy, resolve(outDir, 'Media_objc_0.o'), '-o', resolve(outDir, 'Media.dylib'),
      ];
      return packageKeyArgs(args, outDir, packageDir, new Set([copy]), 'Media', { root, appDir, swiftRootDir, pathSensitive });
    };
    expect(pkgKey('ws-a')).toEqual(pkgKey('ws-b'));
    expect(pkgKey('ws-a', { pathSensitive: true })).not.toEqual(pkgKey('ws-b', { pathSensitive: true }));

    // Linux non-tvOS iOS builds emit only the iOS Simulator route; darwin keeps all three.
    expect(rosterRoutes({ tv: false, device: true, packageDir: '/b/packages', missingSlice: () => undefined, platform: 'linux' }))
      .toEqual([{ via: 'iOS Simulator', variant: true, lacking: undefined, packages: '/b/packages-simulator' }]);
    expect(rosterRoutes({ tv: true, device: true, packageDir: '/b/packages', missingSlice: () => undefined, platform: 'linux' })
      .map(r => r.via)).toEqual(['macOS']);
    expect(rosterRoutes({ tv: false, device: true, packageDir: '/b/packages', missingSlice: () => undefined, platform: 'darwin' })
      .map(r => r.via)).toEqual(['Mac Catalyst', 'iOS Simulator', 'macOS']);

    // staleModuleCache recognizes both Swift (-module-cache-path) and Clang (-fmodules-cache-path=).
    const msg = "fatal error: file.pcm was compiled with module cache path '/old' but the path is currently '/new'";
    expect(staleModuleCache(['swiftc', '-module-cache-path', '/new/swift'], msg)).toBe('/new/swift');
    expect(staleModuleCache(['clang', '-fmodules-cache-path=/new/clang'], msg)).toBe('/new/clang');
    expect(staleModuleCache(['clang', '-fmodules-cache-path=/new/clang'], 'unrelated error')).toBeNull();
    expect(normalizeModuleCacheArgs(['-module-cache-path', '/a', '-fmodules-cache-path=/b', '-O']))
      .toEqual(['-module-cache-path', '<module-cache>', '-fmodules-cache-path=<module-cache>', '-O']);

    // contentStamp tracks file bytes and relative slice paths regardless of mtime or directory prefix.
    const f1 = resolve(dir, 'a/Lib'), f2 = resolve(dir, 'b/Lib');
    mkdirSync(resolve(dir, 'a'), { recursive: true }); mkdirSync(resolve(dir, 'b'), { recursive: true });
    writeFileSync(f1, 'binary-bytes'); writeFileSync(f2, 'binary-bytes');
    utimesSync(f1, 1000, 1000); utimesSync(f2, 9999, 9999);
    expect(contentStamp([['Foo.framework/Headers/a/Foo.h', f1]])).toEqual(contentStamp([['Foo.framework/Headers/a/Foo.h', f2]]));
    expect(contentStamp([['Foo.framework/Headers/a/Foo.h', f1]])).not.toEqual(contentStamp([['Foo.framework/Headers/b/Foo.h', f2]]));
    writeFileSync(f2, 'changed-bytes');
    expect(contentStamp([f1])).not.toEqual(contentStamp([f2]));

    // -fmodule-map-file=, relocated XCFramework -F, -import-objc-header, and #sourceLocation() resets normalize soundly.
    const fwKey = (slot, body = 'let x = 1') => {
      const ws = resolve(dir, slot), root = resolve(ws, 'exact2'), appDir = resolve(ws, 'app'), swiftRootDir = resolve(ws, 'swift-pkgs');
      const linkRoot = resolve(ws, 'link'), packageDir = resolve(ws, 'tmp/packages'), built = resolve(ws, 'out.dylib');
      const user = resolve(appDir, 'modules/apple/User.swift');
      mkdirSync(dirname(user), { recursive: true }); writeFileSync(user, body);
      const snapshot = snapshotSources([user], resolve(ws, 'copies'));
      const args = ['swiftc', '-F', resolve(appDir, 'modules/apple/Map.xcframework/ios-arm64'),
        '-Xcc', `-fmodule-map-file=${resolve(swiftRootDir, 'include/module.modulemap')}`, ...snapshot.copies, '-o', built];
      return armSourceKey(args, snapshot.copies, { built, snapshot, packageDir, packageDirs: [packageDir], linkRoot, swiftc: 'swift-6', extra: '', root, appDir, swiftRootDir });
    };
    expect(fwKey('fw-a')).toBe(fwKey('fw-b'));
    expect(fwKey('fw-a', '#sourceLocation(file: "M/F.swift", line: 1)\n#sourceLocation()\nlet p = #filePath'))
      .not.toBe(fwKey('fw-b', '#sourceLocation(file: "M/F.swift", line: 1)\n#sourceLocation()\nlet p = #filePath'));

    // hasIntentSignal detects AppIntents/protocols and stays false for ordinary view Swift.
    const plain = resolve(dir, 'Plain.swift'), intent = resolve(dir, 'Intent.swift');
    writeFileSync(plain, 'import UIKit\nfinal class MyView: UIView {}');
    writeFileSync(intent, 'import AppIntents\nstruct RunAction: AppIntent {}');
    expect(hasIntentSignal([plain], gathered)).toBe(false);
    expect(hasIntentSignal([plain, intent], gathered)).toBe(true);

    // nativeIntentModule hits cache across fresh temporary build dirs and rebases cached t.file paths on hit.
    const intentDir = resolve(dir, 'native-intents');
    const bin = resolve(dir, 'libexact_modules.dylib');
    writeFileSync(bin, 'fake-dylib');
    let invokes = 0;
    const srcA = resolve(dir, 'tmp-a/User.swift'), srcB = resolve(dir, 'tmp-b/User.swift');
    mkdirSync(resolve(dir, 'tmp-a'), { recursive: true }); mkdirSync(resolve(dir, 'tmp-b'), { recursive: true });
    writeFileSync(srcA, 'struct RunIntent {}'); writeFileSync(srcB, 'struct RunIntent {}');
    const fakeRunApple = (_cmd, args) => {
      invokes++;
      const constPath = args[args.indexOf('-emit-const-values-path') + 1];
      expect(constPath).toBeTruthy();
      expect(args.indexOf('-const-gather-protocols-file')).toBeGreaterThan(-1);
      writeFileSync(constPath, JSON.stringify([{ file: srcA, mangledTypeName: 'RunIntent', typeName: 'ExactLaunchParts.RunIntent' }]));
    };
    const firstIntent = nativeIntentModule({
      moduleName: 'ExactAppModules', binary: bin, sources: [srcA],
      args: ['swiftc', '-module-cache-path', '/tmp/a/cache', '-o', '/tmp/a/out.dylib', srcA],
      directory: intentDir, swiftc: 'swift-6', runApple: fakeRunApple,
    });
    expect(JSON.parse(readFileSync(firstIntent.values, 'utf8'))[0].file).toBe(srcA);
    const secondIntent = nativeIntentModule({
      moduleName: 'ExactAppModules', binary: bin, sources: [srcB],
      args: ['swiftc', '-module-cache-path', '/tmp/b/cache', '-o', '/tmp/b/out.dylib', srcB],
      directory: intentDir, swiftc: 'swift-6', runApple: fakeRunApple,
    });
    expect(JSON.parse(readFileSync(secondIntent.values, 'utf8'))[0].file).toBe(srcB);
    expect(invokes).toBe(1);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
