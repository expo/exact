import { test } from 'bun:test';
import assert from 'node:assert/strict';
import {copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import { spawnSync } from 'node:child_process';
import { assertActivityRefusals } from './activities.mjs';
import { resolveApp } from '../../scripts/app.mjs';
import { appleComposition, assertCapabilityFrameworks, loadedDylibs } from './link.mjs';
import { systemFrameworks } from './compose.mjs';

test.skipIf(process.platform !== 'darwin')('production share extensions keep source dSYMs outside their signed bundles', async () => {
  const { packageShareExtension } = await import('./extensions.mjs');
  const { plistFile } = await import('./assets.mjs');
  const { useXcode } = await import('./devices.mjs');
  useXcode();
  const dir = mkdtempSync(join(tmpdir(), 'exact-share-symbols-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 32 << 20 });
    assert.equal(r.status, 0, `${command}: ${r.stderr}`);
    return r;
  };
  const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
  try {
    for (const platform of ['ios', 'macos']) for (const production of [false, true]) {
      const directory = join(dir, `${platform}-${production}`), bundle = join(directory, 'Fixture.app');
      const contents = platform === 'macos' ? join(bundle, 'Contents') : bundle;
      mkdirSync(contents, { recursive: true });
      writeFileSync(join(contents, 'Info.plist'), plistFile({ CFBundleVersion: '1', CFBundleShortVersionString: '1.0',
        MinimumOSVersion: '17.0', LSMinimumSystemVersion: '14.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }));
      const sdk = run('xcrun', ['--sdk', platform === 'macos' ? 'macosx' : 'iphonesimulator', '--show-sdk-path']).stdout.trim();
      const arch = process.arch === 'x64' ? 'x86_64' : process.arch;
      const triple = platform === 'macos' ? `${arch}-apple-macosx14.0` : 'arm64-apple-ios17.0-simulator';
      const symbols = join(directory, 'Fixture.dSYMs');
      const app = { id: 'test.symbols', displayName: 'Fixture', manifest: { share_target: { action: '/share', params: { text: 'text' } } } };
      const extension = packageShareExtension({ app,
        bundle, directory, sdk, triple, platform, production, symbols, group: 'group.test.symbols', entitlements: plistFile({}), runApple: run,
        sign(path) { run('codesign', ['--force', '--sign', '-', '--timestamp=none', path]); } });
      const binary = join(extension, platform === 'macos' ? 'Contents/MacOS/ExactShare' : 'ExactShare');
      const names = run('nm', ['-a', binary]).stdout;
      assert.ok(!existsSync(`${binary}.dSYM`));
      assert.match(names, /U _NSExtensionMain$/m);
      assert.match(names, /_OBJC_CLASS_\$_ExactShareViewController$/m);
      if (production) {
        assert.doesNotMatch(names, /^[0-9a-f]+ [a-z] /m, 'local symbols are stripped');
        const dsym = join(symbols, 'ExactShare.appex.dSYM');
        assert.ok(uuid(binary));
        assert.equal(uuid(dsym), uuid(binary));
        assert.match(run('dwarfdump', ['--debug-info', dsym]).stdout, /ShareInbox.swift/);
      } else {
        assert.match(names, /^[0-9a-f]+ [a-z] /m);
        assert.ok(!existsSync(symbols));
      }
      run('codesign', ['--verify', '--strict', extension]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test.skipIf(process.platform !== 'darwin')('widget and watch packaging strips production copies before signing and keeps matching dSYMs', async () => {
  const { packageWidgets, packageAlarmExtension } = await import('./widgets.mjs');
  const { packageWatch } = await import('./watch.mjs');
  const { plistFile, wrapFramework } = await import('./assets.mjs');
  const { useXcode } = await import('./devices.mjs');
  useXcode();
  const dir = mkdtempSync(join(tmpdir(), 'exact-widget-symbols-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8' });
    assert.equal(r.status, 0, `${command}: ${r.stderr}`);
    return r;
  };
  const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
  try {
    const source = join(dir, 'fixture.c');
    writeFileSync(source, 'static int local_value = 42; int answer(void) { return local_value; } int main(void) { return answer() != 42; }');
    for (const [platform, sdkName, triple] of [['ios', 'iphonesimulator', 'arm64-apple-ios17.0-simulator'],
      ['watchos', 'watchsimulator', 'arm64-apple-watchos10.0-simulator'], ['macos', 'macosx', 'arm64-apple-macosx14.0']]) {
      const sdk = run('xcrun', ['--sdk', sdkName, '--show-sdk-path']).stdout.trim();
      for (const production of [false, true]) {
        const directory = join(dir, `${platform}-${production}`), bundle = join(directory, 'Fixture.app');
        const contents = platform === 'macos' ? join(bundle, 'Contents') : bundle;
        const resources = platform === 'macos' ? join(contents, 'Resources') : contents;
        const frameworks = join(contents, 'Frameworks'), symbols = join(directory, 'Fixture.dSYMs');
        mkdirSync(join(resources, 'plans'), { recursive: true });
        mkdirSync(frameworks, { recursive: true });
        const app = { dir, id: 'test.symbols', displayName: 'Fixture', manifest: {} };
        const info = plistFile({ CFBundleIdentifier: app.id, CFBundleExecutable: 'ExactWatch', CFBundleVersion: '1', CFBundleShortVersionString: '1.0',
          MinimumOSVersion: '10.0', LSMinimumSystemVersion: '14.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'], ExactWidgetGeneration: 'fixture' });
        writeFileSync(join(contents, 'Info.plist'), info);
        const object = join(directory, 'fixture.o'), original = join(directory, 'Original');
        run('xcrun', ['--sdk', sdkName, 'clang', '-g', '-target', triple, '-c', source, '-o', object]);
        run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, object, '-o', original]);
        run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, '-dynamiclib', object, '-o', join(frameworks, 'libcore.dylib')]);
        wrapFramework(frameworks, 'libcore.dylib', 'ExactCore', app, platform === 'macos' ? 'MacOSX' : 'WatchSimulator');
        const core = join(frameworks, 'ExactCore.framework'), entries = [{ kind: 'fixture', families: ['accessoryInline'] }];
        const runApple = (command, args) => {
          if (!args.includes('swiftc')) return run(command, args);
          // The fixture replaces the runtime link; the packager still owns symbols and signing.
          const output = args[args.indexOf('-o') + 1];
          if (args.includes('-emit-library')) copyFileSync(join(core, 'ExactCore'), output);
          else {
            assert.equal(args.includes('-g'), production);
            assert.ok(args.includes('_main'), 'WidgetKit keeps its @main entry');
            copyFileSync(original, output);
            if (production) run('dsymutil', [output, '-o', `${output}.dSYM`]);
          }
          return { stdout: '' };
        };
        const sign = path => run('codesign', ['--force', '--sign', '-', '--timestamp=none', path]);
        const widgets = { app, bundle, shared: { framework: core }, directory, sdk, triple, device: false,
          entitlements: plistFile({}), sign, runApple, production, symbols, platform, entries };
        packageWidgets(widgets);
        const extension = join(contents, 'PlugIns/ExactWidgets.appex');
        const binary = join(extension, platform === 'macos' ? 'Contents/MacOS/ExactWidgets' : 'ExactWidgets');
        const check = (binary, dsym, required) => {
          const names = run('nm', ['-a', binary]).stdout;
          assert.match(names, new RegExp(` T _${required}$`, 'm'));
          assert.equal(/ _local_value$/m.test(names), !production);
          assert.equal(existsSync(dsym), production);
          if (production) { assert.ok(uuid(binary)); assert.equal(uuid(dsym), uuid(binary)); }
          assert.ok(!existsSync(`${binary}.dSYM`));
        };
        check(binary, join(symbols, 'ExactWidgets.appex.dSYM'), 'main');
        run('codesign', ['--verify', '--strict', extension]);
        if (platform === 'ios') {
          packageAlarmExtension({ ...widgets, alarm: 'FixtureAlarm' });
          check(binary, join(symbols, 'ExactWidgets.appex.dSYM'), 'main');
          run('codesign', ['--verify', '--strict', extension]);
        }
        if (platform === 'watchos') {
          copyFileSync(original, join(directory, 'ExactWatch'));
          packageWatch({ ...widgets, bin: directory, parent: directory, entries: [], sign: path => { if (path !== directory) sign(path); } });
          check(join(bundle, 'ExactWatch'), join(symbols, 'ExactWatch.app.dSYM'), 'main');
          check(join(core, 'ExactCore'), join(symbols, 'ExactCore.framework.dSYM'), 'answer');
          run('codesign', ['--verify', '--deep', '--strict', bundle]);
        }
        assert.match(run('nm', ['-a', original]).stdout, / _local_value$/m, 'the cached link stays unstripped');
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test.skipIf(process.platform !== 'darwin')('production Mac bundles keep Rust images loadable and preserve their dSYMs before signing', async () => {
  const { assembleMac } = await import('./bundle.mjs');
  const { codeDigest, plistFile, wrapFramework } = await import('./assets.mjs');
  const { placeAppleArtifact } = await import('./build.mjs');
  const { useXcode } = await import('./devices.mjs');
  const { createHash } = await import('node:crypto');
  useXcode();
  const dir = mkdtempSync(join(tmpdir(), 'exact-mac-symbols-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8' });
    assert.equal(r.status, 0, `${command}: ${r.stderr}`);
    return r;
  };
  const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
  try {
    const source = join(dir, 'fixture.c'), object = join(dir, 'fixture.o'), dylib = join(dir, 'fixture.dylib');
    writeFileSync(source, 'static int local_value = 42; int answer(void) { return local_value; } int main(void) { return answer() != 42; }');
    run('clang', ['-g', '-c', source, '-o', object]);
    run('clang', ['-dynamiclib', object, '-o', dylib]);
    const rustSource = join(dir, 'fixture.rs'), rustDylib = join(dir, 'rust.dylib');
    writeFileSync(rustSource, '#[no_mangle] pub extern "C" fn answer() -> i32 { 42 }');
    run('env', ['MACOSX_DEPLOYMENT_TARGET=14.0', 'rustc', '--crate-type', 'cdylib', '-g', '-C', 'strip=none', rustSource, '-o', rustDylib]);
    const loader = join(dir, 'loader.c');
    writeFileSync(loader, `#include <dlfcn.h>
#include <stdio.h>
extern int answer(void);
int main(int argc, char **argv) {
  if (answer() != 42) return 1;
  for (int i = 1; i < argc; i++) {
    void *image = dlopen(argv[i], RTLD_NOW);
    if (!image) { fprintf(stderr, "%s\\n", dlerror()); return 2; }
    int (*value)(void) = dlsym(image, "answer");
    if (!value || value() != 42) return 3;
    dlclose(image);
  }
  return 0;
}`);
    const app = { dir, id: 'test.symbols', displayName: 'Fixture', manifest: {} };
    const libraries = join(dir, 'libraries'); mkdirSync(libraries);
    copyFileSync(rustDylib, join(libraries, 'libfixture.dylib'));
    wrapFramework(libraries, 'libfixture.dylib', 'FixtureLibrary', app, 'MacOSX');
    const library = { placed: join(libraries, 'FixtureLibrary.framework'), link: 'FixtureLibrary' };
    const libraryBytes = readFileSync(join(library.placed, library.link)), rustBytes = readFileSync(rustDylib);
    const plan = Buffer.from('fixture'), compat = { embedded: { plan: { sha256: createHash('sha256').update(plan).digest('hex'), bytes: plan.length }, assets: [] } };
    for (const production of [false, true]) {
      const binDir = join(dir, String(production)), bundle = join(binDir, 'Fixture.app');
      mkdirSync(binDir);
      const bin = join(binDir, 'Fixture');
      run('clang', [loader, '-F', libraries, '-framework', library.link, '-Wl,-rpath,@executable_path/../Frameworks', '-o', bin]);
      const modules = 'libmodules.dylib', web = 'libweb.dylib', service = 'libservice.dylib';
      const rustModules = ['libsvg.dylib', 'libcanvas.dylib', 'libexact_listen.dylib'];
      copyFileSync(dylib, join(binDir, web));
      assembleMac({ app, paths: { executable: 'Fixture', capture: binDir }, binDir, bin, libDir: dir, library,
        serviceDylibs: [{ built: dylib, load: service }], hasWeb: true, webLoadName: web, args: ['--bundle'],
        loadName: 'libgpu.dylib', hasGpu: false, moduleDylibs: [], looseArms: [[true, dylib, web, 'ExactWeb'], [true, rustDylib, rustModules[2], null]],
        hasSvg: true, svgBuilt: rustDylib, svgLoadName: rustModules[0],
        modulesLoadName: modules, modulesBuilt: dylib, canvasGpuBuilt: rustDylib, canvasGpuLoadName: rustModules[1], linkRoot: binDir,
        bakedCompat: compat, bakedPlan: plan, buildReceipt: { binary: { metadata: {} } }, production, products: ['Fixture'], cleanup: [],
      }, { runApple: run, appleArtifacts: () => ({ bundle }), placeAppleArtifact,
        macInfoPlist: () => plistFile({ CFBundleExecutable: 'Fixture', CFBundleIdentifier: app.id, CFBundlePackageType: 'APPL' }),
        copyAppleStaticTrees() {}, writeUsageStrings() {}, receipt: () => '{}', assertAppleIdentity() {},
      });
      const symbols = join(binDir, 'Fixture.dSYMs');
      const rustImages = [join(bundle, 'Contents/Frameworks/FixtureLibrary.framework/FixtureLibrary'),
        ...rustModules.map(name => join(bundle, 'Contents/MacOS', name))];
      run(join(bundle, 'Contents/MacOS/Fixture'), rustImages.slice(1));
      for (const [binary, name] of [[join(bundle, 'Contents/Frameworks/FixtureLibrary.framework/FixtureLibrary'), 'FixtureLibrary.framework'],
        ...[modules, web, service, ...rustModules].map(name => [join(bundle, 'Contents/MacOS', name), name])]) {
        const names = run('nm', ['-a', binary]).stdout;
        assert.match(names, / T _answer$/m);
        if (rustImages.includes(binary)) {
          const original = name === 'FixtureLibrary.framework' ? join(library.placed, library.link) : rustDylib;
          assert.equal(codeDigest(readFileSync(binary)), codeDigest(readFileSync(original)), 'Rust images keep their linker output');
        } else assert.equal(/ _local_value$/m.test(names), !production);
        const dsym = join(symbols, `${name}.dSYM`);
        assert.equal(existsSync(dsym), production);
        if (production) { assert.ok(uuid(binary)); assert.equal(uuid(dsym), uuid(binary)); }
      }
      run('codesign', ['--verify', '--deep', '--strict', bundle]);
    }
    assert.deepEqual(readFileSync(join(library.placed, library.link)), libraryBytes, 'the cached framework stays intact');
    assert.deepEqual(readFileSync(rustDylib), rustBytes, 'the cached Rust module stays intact');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('package frameworks retain category-only dependencies and weak-link APIs newer than the deployment target', () => {
  const packages = [
    { frameworks: { PhotosUI: { ios: '14.0' } } },
    { frameworks: { FoundationModels: { ios: '26.0', macos: '26.0' } } },
    { frameworks: { PhotosUI: { ios: '14.0' } } },
  ];
  assert.deepEqual(systemFrameworks(packages, 'ios', '17.0'), [
    '-Xlinker', '-weak_framework', '-Xlinker', 'FoundationModels', '-framework', 'PhotosUI',
  ]);
  assert.deepEqual(systemFrameworks(packages, 'ios', '26.0'), ['-framework', 'FoundationModels', '-framework', 'PhotosUI']);
  assert.deepEqual(systemFrameworks(packages, 'macos', '14.0'), ['-Xlinker', '-weak_framework', '-Xlinker', 'FoundationModels']);
});

test('production links what the plan uses, the manifest names and the grants open; development all but the photo picker and sign-in sheet, which it links as production would (LLP 1047.001.000 E, F)', () => {
  const graph = { uses: ['markdown', 'picker'] };
  assert.deepEqual(appleComposition(graph, true), { link: 'markdown,picker', products: ['ExactMarkdown'] });
  assert.deepEqual(appleComposition({ uses: [] }, true), { link: 'none', products: [] });
  assert.deepEqual(appleComposition(graph, true, [], true).products, ['ExactMarkdown', 'ExactAuthSession']);
  assert.deepEqual(appleComposition(graph, false).products, ['ExactGroupedLists', 'ExactMarkdown', 'ExactSurfaces', 'ExactDrag']);
  assert.deepEqual(appleComposition({ uses: ['picker', 'media_picker'] }, false, [], true).products,
    ['ExactGroupedLists', 'ExactMarkdown', 'ExactSurfaces', 'ExactDrag', 'ExactMediaPicker', 'ExactAuthSession']);
  assert.deepEqual(appleComposition(graph, false, ['auth_session']).products.at(-1), 'ExactAuthSession');
  assert.deepEqual(appleComposition({ uses: ['numerals'] }, false).products.at(-1), 'ExactNumerals');
  assert.ok(!appleComposition(graph, false).products.includes('ExactNumerals'));
  assert.equal(appleComposition(undefined, false).link, 'all');
});

test('the load commands are read from the file, each slice of a universal one; a binary with no capability framework passes the check', () => {
  assert.ok(loadedDylibs('/bin/ls').includes('/usr/lib/libSystem.B.dylib'));
  assert.doesNotThrow(() => assertCapabilityFrameworks('/bin/ls', []));
});


test('ordinary app executables refuse WidgetKit, ActivityKit and SwiftUI, including versioned Mac frameworks (LLP 1126.001 WP16)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-links-'));
  try {
    for (const framework of ['WidgetKit', 'ActivityKit', 'SwiftUI']) for (const version of ['', 'Versions/A/']) {
      const name = Buffer.from(`/System/Library/Frameworks/${framework}.framework/${version}${framework}\0`);
      const size = (24 + name.length + 7) & ~7, bytes = Buffer.alloc(32 + size);
      bytes.writeUInt32LE(0xfeedfacf, 0); bytes.writeUInt32LE(1, 16); bytes.writeUInt32LE(size, 20);
      bytes.writeUInt32LE(0xc, 32); bytes.writeUInt32LE(size, 36); bytes.writeUInt32LE(24, 40); name.copy(bytes, 56);
      const executable = join(dir, 'App'); writeFileSync(executable, bytes);
      assert.throws(() => assertCapabilityFrameworks(executable, []), new RegExp(framework));
      assert.doesNotThrow(() => assertCapabilityFrameworks(executable, framework === 'SwiftUI' ? [] : ['ExactSnapshot'], framework === 'SwiftUI' ? ['SwiftUI'] : []));
    }
  } finally { rmSync(dir, {recursive:true,force:true}); }
});

test("the ActivityKit arm compiles the checkout's sources when the build runs from an app's own directory", () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-arm-'));
  try {
    // A child process, so the working directory is the app's and not this process's.
    const script = `const { packageActivityArm } = await import(${JSON.stringify(new URL('./activities.mjs', import.meta.url).href)});
try { packageActivityArm({ directory: process.cwd(), device: false, triple: 'arm64-apple-ios17.0-simulator', sdk: '/sdk', runApple: (cmd, args) => { console.log(JSON.stringify(args)); throw new Error('stop'); } }); } catch {}`;
    const r = spawnSync(process.execPath, ['-e', script], { cwd: dir, encoding: 'utf8' });
    const sources = JSON.parse(r.stdout || '[]').filter(arg => arg.endsWith('.swift'));
    assert.equal(sources.length, 2, r.stderr);
    for (const source of sources) assert.ok(existsSync(source), source);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('activity refusals allow native APNs without activities and still scan every activity bundle binary', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-scope-'));
  const plain = { manifest: {} }, empty = { manifest: { live_activities: [] } };
  const activity = resolveApp('caltrain');
  const granted = 'net.fetch https://x/\ndevice.notifications purpose.notifications\ndevice.push';
  const runApple = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  };
  const bytes = readFileSync('/bin/ls');
  const directories = [dir, join(dir, 'PlugIns', 'Widget.appex'), join(dir, 'Frameworks', 'Arm.framework')];
  try {
    for (const directory of directories) {
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, 'Info.plist'), JSON.stringify({ CFBundleExecutable: 'Binary' }));
      writeFileSync(join(directory, 'Binary'), bytes);
    }
    const binaries = [...directories.map(directory => join(directory, 'Binary')), join(dir, 'Frameworks', 'Loose.dylib')];
    writeFileSync(binaries.at(-1), bytes);
    assert.equal(assertActivityRefusals(dir, runApple, activity), binaries.length);
    for (const binary of binaries) {
      for (const symbol of ['registerForRemoteNotifications', 'pushToStartTokenUpdates']) {
        writeFileSync(binary, Buffer.concat([bytes, Buffer.from(`\0${symbol}\0`)]));
        if (symbol === 'registerForRemoteNotifications') for (const app of [plain, empty]) {
          assert.equal(assertActivityRefusals(dir, runApple, app), 0);
        }
        assert.throws(() => assertActivityRefusals(dir, runApple, activity), new RegExp(symbol));
        // With `device.push`, the app registers beside its activities; push-to-start stays refused (LLP 1133.004 §9 P8).
        if (symbol === 'registerForRemoteNotifications') assert.equal(assertActivityRefusals(dir, runApple, activity, granted), binaries.length);
        else assert.throws(() => assertActivityRefusals(dir, runApple, activity, granted), new RegExp(symbol));
      }
      writeFileSync(binary, bytes);
    }
    for (const directory of directories) {
      const plist = join(directory, 'Info.plist');
      writeFileSync(plist, JSON.stringify({ CFBundleExecutable: 'Binary', UIBackgroundModes: ['remote-notification'] }));
      for (const app of [plain, empty]) assert.equal(assertActivityRefusals(dir, runApple, app), 0);
      assert.throws(() => assertActivityRefusals(dir, runApple, activity), /remote-notification/);
      assert.throws(() => assertActivityRefusals(dir, runApple, activity, granted), /remote-notification/);
      writeFileSync(plist, JSON.stringify({ CFBundleExecutable: 'Binary' }));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The async iOS step supplies its just-built bundle. Glue has no assembled iOS fixture yet.
test.skipIf(!process.env.EXACT_ACTIVITY_APP)('the assembled app and extension refuse push-to-start symbols, remote registration without device.push, and its background mode (LLP 1126.001 WP9; LLP 1133.004 §9 P8)', async () => {
  const { cpSync, readFileSync, readdirSync } = await import('node:fs');
  const { spawnSync } = await import('node:child_process');
  const { resolve } = await import('node:path');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-activity-refusals-'));
  const runApple = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  };
  try {
    const app = resolve(process.env.EXACT_ACTIVITY_APP), declaration = resolveApp('caltrain');
    assert.ok(assertActivityRefusals(app, runApple, declaration) >= 3, 'app, extension and ActivityKit arm are scanned');
    cpSync(app, dir, { recursive: true });
    const extension = resolve(dir, 'PlugIns', readdirSync(resolve(dir, 'PlugIns')).find(n => n.endsWith('.appex')));
    for (const directory of [dir, extension]) {
      const plist = resolve(directory, 'Info.plist'), original = readFileSync(plist);
      const info = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', plist]).stdout);
      const binary = resolve(directory, info.CFBundleExecutable), bytes = readFileSync(binary);
      runApple('plutil', ['-replace', 'UIBackgroundModes', '-json', '["remote-notification"]', plist]);
      assert.throws(() => assertActivityRefusals(dir, runApple, declaration), /remote-notification/);
      writeFileSync(plist, original);
      const granted = 'device.notifications purpose.notifications\ndevice.push';
      for (const symbol of ['pushToStartTokenUpdates', 'registerForRemoteNotifications']) {
        writeFileSync(binary, Buffer.concat([bytes, Buffer.from(`\0${symbol}\0`)]));
        assert.throws(() => assertActivityRefusals(dir, runApple, declaration), new RegExp(symbol));
        // `device.push` admits registration beside the activities, never push-to-start (LLP 1133.004 §9 P8).
        if (symbol === 'pushToStartTokenUpdates') assert.throws(() => assertActivityRefusals(dir, runApple, declaration, granted), new RegExp(symbol));
        else assert.ok(assertActivityRefusals(dir, runApple, declaration, granted) >= 3);
      }
      writeFileSync(binary, bytes);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
