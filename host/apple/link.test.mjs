import { capturePrivacyManifest } from './privacy.mjs';
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import {copyFileSync, cpSync, existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import { encodePng } from '../../scripts/png.mjs';
import { useXcode } from './devices.mjs';
import { spawnSync } from 'node:child_process';
import { assertActivityRefusals } from './activities.mjs';
import { resolveApp } from '../../scripts/app.mjs';
import { appleComposition, assertCapabilityFrameworks, assertLinkedSdk, compiledTargets, deploymentTargets, iosSimulatorTarget, loadedDylibs } from './link.mjs';
import { composedModule, moduleSeam, systemFrameworks } from './compose.mjs';
import { writeDataKeys } from './data-keys.mjs';

for (const [sdk, ios, tv] of [['appletvsimulator', true, true], ['iphonesimulator', true, false], ['macosx', false, false]]) {
  test.skipIf(process.platform !== 'darwin')(`native-fixture's Apple module typechecks for ${sdk}`, async () => {
    const { moduleTripleFor } = await import('./build.mjs');
    const { useXcode } = await import('./devices.mjs');
    useXcode();
    const root = join(import.meta.dir, '../..'), app = resolveApp(join(root, 'apps/native-fixture'));
    mkdirSync(join(root, 'target'), { recursive: true });
    const dir = mkdtempSync(join(root, 'target/native-fixture-module-'));
    try {
      const keys = join(dir, 'ExactDataKeys.swift'), composed = join(dir, 'ExactComposed.swift');
      writeDataKeys(app, keys, ios ? 'ios' : 'macos');
      writeFileSync(composed, composedModule([], true));
      const built = spawnSync('xcrun', ['--sdk', sdk, 'swiftc', '-typecheck', '-parse-as-library', '-swift-version', '5',
        '-target', moduleTripleFor(app, ios, false, tv), ...moduleSeam(join, root), keys, composed, ...app.modules.apple],
      { encoding: 'utf8', maxBuffer: 8 << 20 });
      assert.equal(built.status, 0, built.stderr);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 60_000);
}

test('a live watch entry in an app with TypeScript reaches the bake', async () => {
  const { buildWatch } = await import('./watch.mjs');
  const dir = mkdtempSync(join(tmpdir(), 'exact-watch-live-')), events = [];
  try {
    writeFileSync(join(dir, 'app.ts'), '');
    const app = { dir, id: 'test.watch', displayName: 'Watch', manifest: { watch: { source: 'watch.contract', component: 'Watch' } } };
    await assert.rejects(buildWatch(app, ['--watchos', '--no-install'], {
      appleBuildLock: () => { events.push('lock'); return () => events.push('release'); },
      buildParent: async () => events.push('parent'),
      appleArtifacts: () => ({ bundle: join(dir, 'Parent.app'), namespace: dir }),
      runApple: () => { throw new Error('watch bake reached'); },
    }), /watch bake reached/);
    assert.deepEqual(events, ['lock', 'parent', 'release']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

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
      run('plutil', ['-lint', join(extension, platform === 'macos' ? 'Contents/Resources/PrivacyInfo.xcprivacy' : 'PrivacyInfo.xcprivacy')]);
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
        const diskSpace = platform === 'watchos' && production;
        writeFileSync(source, 'static int local_value = 42; int answer(void) { return local_value; } int main(void) { return answer() != 42; }');
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
        wrapFramework(frameworks, 'libcore.dylib', 'ExactCore', app, platform === 'macos' ? 'MacOSX' : 'WatchSimulator', { apiManifests: [capturePrivacyManifest(join(dir, 'PrivacyInfo.xcprivacy'))] });
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
        run('plutil', ['-lint', join(extension, platform === 'macos' ? 'Contents/Resources/PrivacyInfo.xcprivacy' : 'PrivacyInfo.xcprivacy')]);
        run('codesign', ['--verify', '--strict', extension]);
        if (platform === 'ios') {
          packageAlarmExtension({ ...widgets, alarm: 'FixtureAlarm' });
          check(binary, join(symbols, 'ExactWidgets.appex.dSYM'), 'main');
          run('plutil', ['-lint', join(extension, 'PrivacyInfo.xcprivacy')]);
          run('codesign', ['--verify', '--strict', extension]);
        }
        if (platform === 'watchos') {
          writeFileSync(join(dir, 'PrivacyInfo.xcprivacy'), plistFile({ NSPrivacyAccessedAPITypes: [{ NSPrivacyAccessedAPIType: 'NSPrivacyAccessedAPICategoryDiskSpace', NSPrivacyAccessedAPITypeReasons: ['E174.1'] }],
            NSPrivacyTracking: true, NSPrivacyTrackingDomains: ['ads.example.com'], NSPrivacyCollectedDataTypes: [{ NSPrivacyCollectedDataType: 'NSPrivacyCollectedDataTypeEmailAddress',
              NSPrivacyCollectedDataTypeLinked: true, NSPrivacyCollectedDataTypeTracking: false, NSPrivacyCollectedDataTypePurposes: ['NSPrivacyCollectedDataTypePurposeAppFunctionality'] }] }));
          if (diskSpace) {
            writeFileSync(source, '#include <sys/mount.h>\nstatic int local_value = 42; int answer(void) { struct statfs s; statfs(".", &s); return local_value; } int main(void) { return answer() != 42; }');
            run('xcrun', ['--sdk', sdkName, 'clang', '-g', '-target', triple, '-c', source, '-o', object]);
            run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, object, '-o', original]);
            run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, '-dynamiclib', object, '-o', join(core, 'ExactCore')]);
          }
          copyFileSync(original, join(directory, 'ExactWatch'));
          packageWatch({ ...widgets, privacyManifests: [capturePrivacyManifest(join(dir, 'PrivacyInfo.xcprivacy'))], bin: directory, parent: directory, entries: [], sign: path => { if (path !== directory) sign(path); } });
          run('plutil', ['-lint', join(bundle, 'PrivacyInfo.xcprivacy')]);
          run('plutil', ['-lint', join(core, 'PrivacyInfo.xcprivacy')]);
          for (const owner of [bundle, core]) assert.deepEqual(JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', join(owner, 'PrivacyInfo.xcprivacy')]).stdout), {
            NSPrivacyAccessedAPITypes: diskSpace ? [{ NSPrivacyAccessedAPIType: 'NSPrivacyAccessedAPICategoryDiskSpace', NSPrivacyAccessedAPITypeReasons: ['E174.1'] }] : [],
          });
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
        bakedCompat: compat, bakedPlan: plan, buildReceipt: { graph: { extensions: {}, contractSources: [] }, binary: { metadata: {} } }, production, products: ['Fixture'], cleanup: [],
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

test('an x64 Linux host can select the matching ARM simulator Rust and Swift targets', () => {
  assert.deepEqual(iosSimulatorTarget('arm64'), { target: 'aarch64-apple-ios-sim', triple: 'arm64-apple-ios17.0-simulator' });
  for (const arch of ['x64', 'x86_64']) assert.deepEqual(iosSimulatorTarget(arch), { target: 'x86_64-apple-ios', triple: 'x86_64-apple-ios17.0-simulator' });
  for (const arch of ['ia32', '__proto__', 'toString']) assert.throws(() => iosSimulatorTarget(arch), /unsupported iOS simulator architecture/);
  const environment = { ...process.env }; delete environment.EXACT_IOS_SIM_ARCH;
  const source = `Object.defineProperty(process,'platform',{value:'linux'}); const {iosSimulatorTarget}=await import('./host/apple/link.mjs'); console.log(JSON.stringify(iosSimulatorTarget()));`;
  const result = spawnSync(process.execPath, ['-e', source], { cwd: join(import.meta.dir, '../..'), encoding: 'utf8', env: environment });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).target, 'aarch64-apple-ios-sim');
});

test('the complete builder uses the selected simulator CPU with an app minimum OS, including native modules', () => {
  for (const [arch, target, cpu] of [['arm64', 'aarch64-apple-ios-sim', 'arm64'], ['x86_64', 'x86_64-apple-ios', 'x86_64']]) {
    const script = `import {iosTarget,iosTripleFor,moduleTripleFor} from './host/apple/build.mjs';
      const app={manifest:{host:{ios:{minimumOS:'18.2'}}}};
      console.log(JSON.stringify([iosTarget,iosTripleFor(app,false),moduleTripleFor(app,true),iosTripleFor(app,true)]));`;
    const r = spawnSync(process.execPath, ['-e', script], { cwd: join(import.meta.dir, '../..'), encoding: 'utf8', env: { ...process.env, EXACT_IOS_SIM_ARCH: arch } });
    assert.equal(r.status, 0, r.stderr);
    assert.deepEqual(JSON.parse(r.stdout), [target, `${cpu}-apple-ios18.2-simulator`, `${cpu}-apple-ios18.2-simulator`, 'arm64-apple-ios18.2']);
  }
});

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
  // TipKit and SwiftUI cost every launch: a tip links them in development too only by use.
  assert.deepEqual(appleComposition({ uses: ['tips'] }, false).products.at(-1), 'ExactTips');
  assert.ok(!appleComposition(graph, false).products.includes('ExactTips'));
  assert.equal(appleComposition(undefined, false).link, 'all');
});

test('the load commands are read from the file, each slice of a universal one; a binary with no capability framework passes the check', () => {
  assert.ok(loadedDylibs('/bin/ls').includes('/usr/lib/libSystem.B.dylib'));
  assert.doesNotThrow(() => assertCapabilityFrameworks('/bin/ls', []));
});

test('SDK checks read thin, universal and fat64 executables on any build host, including every slice', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-sdk-'));
  const thin = (major, minor, patch = 0) => {
    const b = Buffer.alloc(56);
    b.writeUInt32LE(0xfeedfacf, 0);
    b.writeUInt32LE(1, 16); b.writeUInt32LE(24, 20);
    b.writeUInt32LE(0x32, 32); b.writeUInt32LE(24, 36);
    b.writeUInt32LE(7, 40); b.writeUInt32LE((major << 16) | (minor << 8) | patch, 48);
    return b;
  };
  const fat = (slices, wide) => {
    const width = wide ? 32 : 20, header = Buffer.alloc(8 + slices.length * width);
    header.writeUInt32BE(wide ? 0xcafebabf : 0xcafebabe, 0); header.writeUInt32BE(slices.length, 4);
    let offset = header.length;
    slices.forEach((slice, i) => {
      const at = 8 + i * width;
      if (wide) { header.writeBigUInt64BE(BigInt(offset), at + 8); header.writeBigUInt64BE(BigInt(slice.length), at + 16); }
      else { header.writeUInt32BE(offset, at + 8); header.writeUInt32BE(slice.length, at + 12); }
      offset += slice.length;
    });
    return Buffer.concat([header, ...slices]);
  };
  const path = join(dir, 'App (Exact)');
  const check = (bytes, passes) => {
    writeFileSync(path, bytes);
    if (passes) assert.doesNotThrow(() => assertLinkedSdk(path, '26.4'));
    else assert.throws(() => assertLinkedSdk(path, '26.4'), /records SDK .*not 26\.4/);
  };
  try {
    check(thin(26, 4, 1), true);
    check(thin(26, 5), false);
    for (const wide of [false, true]) {
      check(fat([thin(26, 4), thin(26, 4, 2)], wide), true);
      check(fat([thin(26, 4), thin(26, 5)], wide), false);
      const truncatedSlice = fat([thin(26, 4), thin(26, 4)], wide).subarray(0, -1);
      writeFileSync(path, truncatedSlice);
      assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    }
    const missing = thin(26, 4); missing.writeUInt32LE(0x1b, 32); check(missing, false);
    const oversized = thin(26, 4); oversized.writeUInt32LE(64, 36);
    writeFileSync(path, oversized);
    assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    for (const bytes of [Buffer.alloc(0), Buffer.alloc(4), thin(26, 4).subarray(0, 40)]) {
      writeFileSync(path, bytes);
      assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
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

test('a framework two capabilities may bring passes when either is linked, and a stray one names them both (#364)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-shared-framework-'));
  try {
    const name = Buffer.from('/System/Library/Frameworks/SwiftUI.framework/SwiftUI\0');
    const size = (24 + name.length + 7) & ~7, bytes = Buffer.alloc(32 + size);
    bytes.writeUInt32LE(0xfeedfacf, 0); bytes.writeUInt32LE(1, 16); bytes.writeUInt32LE(size, 20);
    bytes.writeUInt32LE(0xc, 32); bytes.writeUInt32LE(size, 36); bytes.writeUInt32LE(24, 40); name.copy(bytes, 56);
    const executable = join(dir, 'App'); writeFileSync(executable, bytes);
    const frameworks = [['SwiftUI', ['ExactNumerals', 'ExactOther']]];
    assert.doesNotThrow(() => assertCapabilityFrameworks(executable, ['ExactOther'], [], frameworks));
    assert.doesNotThrow(() => assertCapabilityFrameworks(executable, ['ExactNumerals'], [], frameworks));
    assert.throws(() => assertCapabilityFrameworks(executable, [], [], frameworks), /SwiftUI \(only ExactNumerals or ExactOther may\)/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("the ActivityKit arm compiles the checkout's sources when the build runs from an app's own directory", () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-arm-'));
  try {
    // A child process, so the working directory is the app's and not this process's.
    const script = `const { packageActivityArm } = await import(${JSON.stringify(new URL('./activities.mjs', import.meta.url).href)});
try { packageActivityArm({ shared: { base: process.cwd() }, device: false, triple: 'arm64-apple-ios17.0-simulator', sdk: '/sdk', runApple: (cmd, args) => { console.log(JSON.stringify(args)); throw new Error('stop'); } }); } catch {}`;
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

test('outside production trust the SDK compiles for iOS 17.0, and the app keeps its minimumOS', () => {
  const app = { manifest: { host: { ios: { minimumOS: '18.0' }, macos: { minimumOS: '15.0' } } } };
  assert.deepEqual(deploymentTargets(app), { ios: '18.0', macos: '15.0' });
  assert.deepEqual(compiledTargets(app, 'development'), { ios: '17.0', macos: '15.0' });
  assert.deepEqual(compiledTargets(app, 'production'), { ios: '18.0', macos: '15.0' });
  assert.deepEqual(compiledTargets({ manifest: {} }, 'development'), deploymentTargets({ manifest: {} }));
  const launching = { manifest: { launch: ['player'], host: { ios: { minimumOS: '18.0' } } } };
  assert.deepEqual(compiledTargets(launching, 'development', 'darwin'), deploymentTargets(launching), 'on a Mac, launch Swift compiles in the host package at the app minimum');
  // The native build system (the Linux cross build) gives the launch modules and the executable that imports them
  // the app's own target, so the host keeps the floor.
  assert.deepEqual(compiledTargets(launching, 'development', 'linux'), { ...deploymentTargets(launching), ios: '17.0' });

  // One spelling per version, so apps that write it differently share one Swift scratch (expo/exact-build#94).
  for (const minimumOS of ['18', '18.0', '18.0.0', 18]) assert.equal(deploymentTargets({ manifest: { host: { ios: { minimumOS } } } }).ios, '18.0', String(minimumOS));
  assert.equal(deploymentTargets({ manifest: { host: { ios: { minimumOS: '18.4.1' } } } }).ios, '18.4.1');
  assert.equal(deploymentTargets({ manifest: { host: { macos: { minimumOS: '15' } } } }).macos, '15.0');
});

test.skipIf(process.platform !== 'darwin' || !['clang', 'codesign', 'sips', 'dsymutil', 'nm'].every(tool => Bun.which(tool))).each([false, true])('production iOS bundles keep symbols and owning privacy declarations (disk-space service: %s)', async diskService => {
  useXcode();
  const { createHash } = await import('node:crypto');
  const { assembleIOS } = await import('./bundle.mjs');
  const { stripProductionExecutable } = await import('./link.mjs');
  const { shippedReceipt, copyAppleGpuProducts } = await import('./build.mjs');
  const { wrapFramework, signedGpuProduct, codeDigest, plistFile } = await import('./assets.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-ios-package-'));
  const run = (cmd, args) => {
    const r = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(r.status, 0, `${cmd}: ${r.stderr}`);
    return r;
  };
  try {
    const source = resolve(dir, 'module.c'), executable = resolve(dir, 'Fixture'), dylib = resolve(dir, 'module.dylib');
    writeFileSync(source, '#include <sys/stat.h>\nstatic int local_value = 42; int answer(void){struct stat s; stat(".", &s); return local_value;} int main(void){return answer()!=42;}\n');
    run('clang', ['-g', source, '-o', executable]);
    run('clang', ['-g', '-dynamiclib', source, '-o', dylib]);
    assert.match(run('nm', ['-a', dylib]).stdout, / _local_value$/m);
    const icon = resolve(dir, 'icon.png');
    writeFileSync(icon, encodePng({ width: 1024, height: 1024, data: new Uint8Array(1024 * 1024 * 4).fill(255) }));
    const app = { dir, id: 'com.exact.fixture', name: 'Fixture', manifest: {} };
    const api = (name, reasons) => ({ NSPrivacyAccessedAPIType: `NSPrivacyAccessedAPICategory${name}`, NSPrivacyAccessedAPITypeReasons: reasons });
    const declarations = { NSPrivacyAccessedAPITypes: [api('DiskSpace', ['E174.1'])], NSPrivacyTracking: true,
      NSPrivacyTrackingDomains: ['ads.example.com'], NSPrivacyCollectedDataTypes: [{ NSPrivacyCollectedDataType: 'NSPrivacyCollectedDataTypeEmailAddress',
        NSPrivacyCollectedDataTypeLinked: true, NSPrivacyCollectedDataTypeTracking: false, NSPrivacyCollectedDataTypePurposes: ['NSPrivacyCollectedDataTypePurposeAppFunctionality'] }] };
    writeFileSync(resolve(dir, 'PrivacyInfo.xcprivacy'), plistFile(declarations));
    const service = resolve(dir, 'service.dylib');
    writeFileSync(source, diskService ? '#include <sys/mount.h>\nstatic int local_value = 42; int answer(void){struct statfs s; statfs(".", &s); return local_value;}\n' : readFileSync(source));
    run('clang', ['-g', '-dynamiclib', source, '-o', service]);
    const libraries = resolve(dir, 'libraries'); mkdirSync(libraries);
    copyFileSync(dylib, resolve(libraries, 'library.dylib'));
    wrapFramework(libraries, 'library.dylib', 'FixtureLibrary', app);
    const library = { placed: resolve(libraries, 'FixtureLibrary.framework'), link: 'FixtureLibrary' };
    const plan = Buffer.from('fixture'), compat = { embedded: { plan: { sha256: createHash('sha256').update(plan).digest('hex'), bytes: plan.length }, assets: [] } };
    const pairs = [['libweb.dylib', 'ExactWeb'], ['libmodules.dylib', 'ExactModules'], ['libsvg.dylib', 'ExactSvg'], ['libcanvas.dylib', 'ExactCanvasGpu'], ['libservice.dylib', 'ExactService']];
    const build = { trust: 'production', linkedPlan: '/private/plan', binary: { sha256: 'binary', inputs: [{ path: '/private/source' }], metadata: {}, privacyManifests: [capturePrivacyManifest(resolve(dir, 'PrivacyInfo.xcprivacy'))] }, products: [{ path: '/private/product' }], graph: { artifacts: [], extensions: {}, contractSources: [] } };
    writeFileSync(resolve(dir, 'PrivacyInfo.xcprivacy'), plistFile({ NSPrivacyTracking: false }));
    for (const [production, device, archived] of [[true, true, false], [true, true, true], [true, false, false], [false, true, false]]) {
      app.manifest = archived ? { icons: [{ src: 'icon.png', sizes: '1024x1024' }] } : {};
      const binDir = resolve(dir, `${production}-${device}-${archived}`); mkdirSync(binDir);
      const bin = resolve(binDir, 'ExactClip');
      copyFileSync(executable, bin);
      const capture = resolve(binDir, 'capture'); mkdirSync(capture);
      const gpu = { built: 'gpu.dylib', load: 'libgpu.dylib', framework: 'ExactGpu' };
      copyFileSync(dylib, resolve(capture, gpu.built));
      if (production) {
        const signed = signedGpuProduct(resolve(capture, gpu.built), { framework: gpu.framework, app, privacyManifest: build.binary.privacyManifests[0], platform: device ? 'iPhoneOS' : 'iPhoneSimulator' });
        cpSync(resolve(signed, '..'), resolve(capture, 'ExactGpu.framework'), { recursive: true });
      }
      const gpuSource = resolve(capture, production ? 'ExactGpu.framework/ExactGpu' : gpu.built);
      run('codesign', ['--force', '--sign', '-', '--timestamp=none', production ? resolve(capture, 'ExactGpu.framework') : gpuSource]);
      const ipa = archived ? resolve(binDir, 'Archive.ipa') : null;
      const symbols = stripProductionExecutable(bin, { production, ipa });
      const bundle = resolve(binDir, 'Fixture.app');
      const whole = JSON.stringify({ build, target: 'aarch64-apple-ios', development: production ? null : { token: 'kept' } });
      const clipBin = resolve(binDir, 'clip', 'Clip');
      mkdirSync(resolve(binDir, 'clip'));
      copyFileSync(bin, clipBin);
      const result = assembleIOS({ app, library, paths: { bundle, executable: 'ExactClip', capture }, binDir, bin, device,
        clipBin, production, distribution: archived, ipa, bakedCompat: compat, bakedPlan: plan, buildReceipt: build,
        libDir: capture, gpuArtifacts: [gpu], looseArms: [[true, dylib, ...pairs[0]]],
        modulesBuilt: dylib, modulesLoadName: pairs[1][0], hasSvg: true, svgBuilt: dylib, svgLoadName: pairs[2][0],
        canvasGpuBuilt: dylib, canvasGpuLoadName: pairs[3][0], serviceDylibs: [{ built: service, load: pairs[4][0], framework: pairs[4][1] }],
        args: [], unsigned: true, sha1: '-', linkRoot: binDir, sdkName: 'iphoneos' }, {
        runApple: run, infoPlist: () => plistFile({ CFBundleIdentifier: app.id, CFBundleExecutable: 'ExactClip', CFBundlePackageType: 'APPL' }),
        copyAppleStaticTrees() {}, writeUsageStrings() {}, assertAppleIdentity() {}, distributionKeys: () => ({}),
        receipt: () => whole, shippedReceipt, copyAppleGpuProducts, entitlements: () => plistFile({}), appClip: () => ({ id: app.id + '.Clip', name: 'Clip', executable: 'Clip' }),
      });
      for (const owner of [bundle, resolve(bundle, 'AppClips/Clip.app'), ...(production ? [...pairs, [null, 'ExactGpu']].map(([, name]) => resolve(bundle, 'Frameworks', `${name}.framework`)) : [])]) {
        const privacy = resolve(owner, 'PrivacyInfo.xcprivacy'); assert.equal(run('plutil', ['-lint', privacy]).status, 0);
        const usesDisk = diskService && (owner.endsWith('/ExactService.framework') || (!production && owner.endsWith('.app')));
        const reasons = [...(usesDisk ? [api('DiskSpace', ['E174.1'])] : []), ...(diskService && owner.endsWith('/ExactService.framework') ? [] : [api('FileTimestamp', ['3B52.1', 'C617.1'])])];
        assert.deepEqual(JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', privacy]).stdout), {
          ...(owner.endsWith('.app') ? declarations : {}), NSPrivacyAccessedAPITypes: reasons,
        }, owner);
      }
      const clipBytes = readFileSync(resolve(bundle, 'AppClips/Clip.app/Clip'));
      assert.equal(codeDigest(clipBytes), codeDigest(readFileSync(device ? bin : clipBin)), 'the clip uses its platform executable, including when the parent is named ExactClip');
      assert.equal(codeDigest(readFileSync(resolve(bundle, 'ExactClip'))), codeDigest(readFileSync(bin)));
      if (symbols) {
        const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
        assert.ok(uuid(symbols.dsym));
        assert.equal(uuid(symbols.dsym), uuid(resolve(bundle, 'ExactClip')));
        assert.equal(uuid(symbols.dsym), uuid(resolve(bundle, 'AppClips/Clip.app/Clip')));
      }
      const bundled = readFileSync(resolve(bundle, 'receipt.json'), 'utf8');
      assert.equal(bundled, production ? shippedReceipt(whole) : whole);
      if (production) {
        const sidecar = resolve(binDir, ipa ? 'Archive.receipt.json' : 'ExactClip.receipt.json');
        assert.equal(readFileSync(sidecar, 'utf8'), whole);
        assert.equal(result.frameworkSymbols.count, pairs.length + 2);
        for (const [loose, name] of [...pairs, [null, 'FixtureLibrary']]) {
          const binary = resolve(bundle, 'Frameworks', `${name}.framework`, name);
          if (loose) assert.equal(existsSync(resolve(bundle, 'Frameworks', loose)), false);
          const preserved = ['ExactSvg', 'ExactCanvasGpu', 'FixtureLibrary'].includes(name), names = run('nm', ['-a', binary]).stdout;
          assert.match(names, / T _answer$/m);
          assert.equal(/ _local_value$/m.test(names), preserved, `${name} symbol policy`);
          const symbols = resolve(result.frameworkSymbols.dir, `${name}.framework.dSYM`);
          assert.equal(/UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', symbols]).stdout)?.[1], /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', binary]).stdout)?.[1]);
        }
        const packageBinary = resolve(bundle, 'Frameworks/FixtureLibrary.framework/FixtureLibrary');
        assert.equal(codeDigest(readFileSync(packageBinary)), codeDigest(readFileSync(resolve(library.placed, library.link))), 'the package library keeps its linker output');
        assert.equal(codeDigest(readFileSync(resolve(bundle, 'Frameworks/ExactGpu.framework/ExactGpu'))), codeDigest(readFileSync(gpuSource)), 'the baked GPU digest is preserved');
        assert.ok(existsSync(resolve(result.frameworkSymbols.dir, 'ExactGpu.framework.dSYM')));
      } else {
        assert.equal(result.frameworkSymbols.count, 0);
        for (const [loose] of pairs) assert.match(run('nm', ['-a', resolve(bundle, 'Frameworks', loose)]).stdout, / _local_value$/m);
        assert.equal(existsSync(resolve(binDir, 'ExactClip.receipt.json')), false);
      }
      run('codesign', ['--verify', '--deep', '--strict', bundle]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);
