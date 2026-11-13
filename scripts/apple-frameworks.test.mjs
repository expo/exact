// Module xcframeworks use the package-library placement and signing path (LLP 1154 D3).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { moduleFrameworks } from '../host/apple/arms.mjs';
import { assembleIOS, assembleMac } from '../host/apple/bundle.mjs';
import { codeDigest, plistFile } from '../host/apple/assets.mjs';
import { encodePng } from './png.mjs';
import { useXcode } from '../host/apple/devices.mjs';

const run = (cmd, args) => {
  const result = spawnSync(cmd, args, { encoding: 'utf8' });
  assert.equal(result.status, 0, `${cmd} ${args.join(' ')}: ${result.stderr}`);
  return result;
};

test.each([...new Set([process.platform, 'linux'])])('module framework classification reads Mach-O headers without otool (%s toolchain)', toolchain => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-framework-headers-'));
  const python = Bun.which('python3');
  assert.ok(python, 'plist conversion needs python3');
  const path = process.env.PATH, host = Object.getOwnPropertyDescriptor(process, 'platform');
  try {
    // Keep both plist converters available, but neither otool nor llvm-otool.
    if (process.platform === 'darwin') symlinkSync('/usr/bin/plutil', resolve(dir, 'plutil'));
    symlinkSync(python, resolve(dir, 'python3'));
    Object.defineProperty(process, 'platform', { value: toolchain });
    process.env.PATH = dir;
    assert.equal(spawnSync('otool', []).error?.code, 'ENOENT');
    assert.equal(spawnSync('llvm-otool', []).error?.code, 'ENOENT');
    const thin = Buffer.alloc(16);
    thin.writeUInt32LE(0xfeedfacf, 0);
    thin.writeUInt32LE(6, 12);
    const fat = Buffer.alloc(48);
    fat.writeUInt32BE(0xcafebabe, 0);
    fat.writeUInt32BE(1, 4);
    fat.writeUInt32BE(32, 16);
    thin.copy(fat, 32);
    for (const [name, bytes, dynamic] of [
      ['Thin', thin, true], ['Universal', fat, true], ['Static', Buffer.from('!<arch>\n'), false],
    ]) {
      const fw = resolve(dir, `${name}.xcframework`), placed = resolve(fw, 'macos-arm64', `${name}.framework`);
      mkdirSync(placed, { recursive: true });
      writeFileSync(resolve(placed, name), bytes);
      writeFileSync(resolve(fw, 'Info.plist'), plistFile({ AvailableLibraries: [{
        LibraryIdentifier: 'macos-arm64', LibraryPath: `${name}.framework`, SupportedPlatform: 'macos', SupportedArchitectures: ['arm64'],
      }] }));
      const selected = moduleFrameworks([fw], { platform: 'macos', arch: 'arm64' });
      assert.deepEqual(selected.libraries, dynamic ? [{ placed, link: name }] : []);
      assert.ok(selected.args.includes('-framework'));
      assert.equal(selected.args.includes('-rpath'), dynamic);
    }
  } finally { Object.defineProperty(process, 'platform', host); process.env.PATH = path; rmSync(dir, { recursive: true, force: true }); }
});

test('web source capture skips versioned tvOS module xcframeworks', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-tvos-framework-capture-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Fixture', app: { id: 'dev.exact.frameworks', name: 'Fixture' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  resource answer = answer() as shape number\n  view\n    text `${answer}`\n');
    writeFileSync(resolve(dir, 'app.ts'), "import type { Answer } from './app.contract.d.ts';\nexport const grants = '';\nexport const answer: Answer = () => 42;\n");
    const framework = resolve(dir, 'tvos/modules/Vendor.xcframework/macos-arm64/Vendor.framework');
    mkdirSync(resolve(framework, 'Versions/A'), { recursive: true });
    symlinkSync('A', resolve(framework, 'Versions/Current'));
    run(process.execPath, [resolve(import.meta.dir, '../host/web-js/build.mjs'), dir, '--check', '--out', resolve(dir, 'dist')]);
    assert.ok(existsSync(resolve(dir, 'dist/.gen/typescript/app.ts')));
    assert.equal(existsSync(resolve(dir, 'dist/.gen/typescript/tvos/modules/Vendor.xcframework')), false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120_000);

test.skipIf(process.platform !== 'darwin')('module xcframeworks embed and sign only dynamic slices, with resolvable run paths in development and distribution', () => {
  useXcode();
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-module-frameworks-'));
  const arch = process.arch === 'arm64' ? 'arm64' : 'x86_64';
  const targets = [
    ['macos', undefined, 'macosx', `${arch}-apple-macos14.0`, arch],
    ['ios', 'simulator', 'iphonesimulator', 'arm64-apple-ios17.0-simulator', 'arm64'],
    ['ios', undefined, 'iphoneos', 'arm64-apple-ios17.0', 'arm64'],
    ['tvos', 'simulator', 'appletvsimulator', 'arm64-apple-tvos17.0-simulator', 'arm64'],
    ['tvos', undefined, 'appletvos', 'arm64-apple-tvos17.0', 'arm64'],
  ];
  try {
    const modules = resolve(dir, 'modules/apple'); mkdirSync(modules, { recursive: true });
    const frameworks = ['Dynamic', 'Static'].map(name => resolve(modules, `${name}.xcframework`));
    for (const [i, name] of ['Dynamic', 'Static'].entries()) {
      const source = resolve(dir, `${name}.c`), slices = [];
      writeFileSync(source, `static int value = ${i ? 5 : 37}; int ${name.toLowerCase()}_value(void) { return value; }\n`);
      for (const [platform, variant, sdk, triple] of targets) {
        const framework = resolve(dir, 'target', sdk, `${name}.framework`), versioned = platform === 'macos';
        const base = versioned ? resolve(framework, 'Versions/A') : framework;
        const resources = versioned ? resolve(base, 'Resources') : base;
        mkdirSync(resources, { recursive: true });
        const binary = resolve(base, name), info = { CFBundleIdentifier: `dev.exact.${name}`, CFBundleExecutable: name, CFBundlePackageType: 'FMWK', CFBundleVersion: '1', CFBundleShortVersionString: '1.0' };
        writeFileSync(resolve(resources, 'Info.plist'), plistFile(info));
        writeFileSync(resolve(resources, 'fixture.txt'), `${platform}/${variant ?? 'device'}`);
        const clang = ['--sdk', sdk, 'clang', '-target', triple, '-g'];
        if (!i) run('xcrun', [...clang, '-dynamiclib', source, '-Wl,-install_name,@rpath/' + name + '.framework/' + (versioned ? 'Versions/A/' : '') + name, '-o', binary]);
        else {
          const obj = resolve(dir, 'static.o');
          run('xcrun', [...clang, '-c', source, '-o', obj]);
          run('xcrun', ['libtool', '-static', obj, '-o', binary]);
        }
        if (versioned) {
          symlinkSync('A', resolve(framework, 'Versions/Current'));
          for (const part of [name, 'Resources']) symlinkSync(`Versions/Current/${part}`, resolve(framework, part));
        }
        if (!i) run('codesign', ['--force', '--sign', '-', '--timestamp=none', framework]);
        slices.push('-framework', framework);
      }
      run('xcodebuild', ['-create-xcframework', ...slices, '-output', frameworks[i]]);
    }
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Fixture', app: { id: 'dev.exact.frameworks', name: 'Fixture' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  resource answer = answer() as shape number\n  view\n    text `${answer}`\n');
    writeFileSync(resolve(dir, 'app.ts'), "import type { Answer } from './app.contract.d.ts';\nexport const grants = '';\nexport const answer: Answer = () => 42;\n");
    // Capture the app before embedding: even an iOS build sees the Mac slice's links.
    run(process.execPath, [resolve(import.meta.dir, '../host/web-js/build.mjs'), dir, '--check', '--out', resolve(dir, 'dist')]);
    assert.ok(existsSync(resolve(dir, 'dist/.gen/typescript/app.ts')));
    assert.equal(existsSync(resolve(dir, 'dist/.gen/typescript/modules/apple/Dynamic.xcframework')), false);
    const source = resolve(dir, 'module.c'), main = resolve(dir, 'main.c');
    writeFileSync(source, 'extern int dynamic_value(void); extern int static_value(void); int answer(void) { return dynamic_value() + static_value(); }');
    writeFileSync(main, '#include <dlfcn.h>\n#include <stdio.h>\nint main(void) { void *m = dlopen("@executable_path/libexact_modules.dylib", RTLD_NOW); if (!m) { puts(dlerror()); return 1; } int (*answer)(void) = dlsym(m, "answer"); return !answer || answer() != 42; }');
    const plan = Buffer.from('fixture'), compat = { embedded: { plan: { sha256: createHash('sha256').update(plan).digest('hex'), bytes: plan.length }, assets: [] } };
    writeFileSync(resolve(dir, 'icon.png'), encodePng({ width: 1024, height: 1024, data: new Uint8Array(1024 * 1024 * 4).fill(255) }));
    const app = { dir, id: 'dev.exact.frameworks', name: 'Fixture', manifest: { icons: [{ src: 'icon.png', sizes: '1024x1024' }] } };
    const buildReceipt = { binary: { metadata: {} }, graph: { artifacts: [] } };
    for (const [platform, variant, sdk, triple, arch] of targets) {
      const selection = moduleFrameworks(frameworks, { platform, variant, arch });
      assert.equal(selection.libraries.length, 1);
      assert.equal(selection.libraries[0].link, 'Dynamic');
      if (platform === 'macos') {
        const probe = resolve(dir, 'probe');
        const probeSource = resolve(dir, 'probe.c');
        writeFileSync(probeSource, 'extern int answer(void); int main(void) { return answer() != 42; }');
        run('xcrun', ['clang', '-target', triple, probeSource, source, ...selection.args, ...selection.probeArgs, '-o', probe]);
        run(probe, []);
      }
      assert.equal(moduleFrameworks([frameworks[1]], { platform, variant, arch }).args.includes('-rpath'), false);
      for (const production of [false, true]) {
        const binDir = resolve(dir, `${sdk}-${production}`); mkdirSync(binDir);
        const bin = resolve(binDir, 'Fixture'), modulesBuilt = resolve(dir, 'libexact_modules.dylib');
        run('xcrun', ['--sdk', sdk, 'clang', '-target', triple, '-g', '-dynamiclib', source, ...selection.args, '-o', modulesBuilt]);
        run('xcrun', ['--sdk', sdk, 'clang', '-target', triple, main, '-o', bin]);
        const bundle = resolve(binDir, 'Fixture.app');
        const input = { app, moduleLibraries: selection.libraries, paths: { bundle, executable: 'Fixture', capture: dir },
          binDir, bin, modulesBuilt, modulesLoadName: 'libexact_modules.dylib', serviceDylibs: [], looseArms: [], gpuArtifacts: [], moduleDylibs: [],
          hasSvg: false, svgLoadName: 'libsvg.dylib', canvasGpuLoadName: 'libcanvas.dylib', loadName: 'libgpu.dylib',
          production, distribution: production && platform !== 'tvos', bakedCompat: compat, bakedPlan: plan, buildReceipt, args: ['--bundle'],
          sha1: '-', unsigned: true, linkRoot: binDir, sdkName: sdk, device: !variant, tv: platform === 'tvos', products: ['Fixture'], cleanup: [] };
        const plist = () => plistFile({ CFBundleIdentifier: app.id, CFBundleExecutable: 'Fixture', CFBundlePackageType: 'APPL' });
        const tools = { runApple: run, infoPlist: plist, macInfoPlist: plist, copyAppleStaticTrees() {}, copyAppleGpuProducts() {},
          writeUsageStrings() {}, assertAppleIdentity() {}, distributionKeys: () => ({}), receipt: () => '{}', shippedReceipt: value => value,
          entitlements: () => plistFile({}), appClip: () => null, appleArtifacts: () => ({ bundle }),
          placeAppleArtifact: (from, to) => cpSync(from, to, { recursive: true, verbatimSymlinks: true }) };
        if (!production && ['macos', 'ios'].includes(platform) && !variant) {
          const first = selection.libraries[0], second = { ...first, placed: resolve(dir, 'other/Dynamic.framework') };
          cpSync(first.placed, second.placed, { recursive: true, verbatimSymlinks: true });
          const core = resolve(dir, 'host/ExactCore.framework'), moduleCore = resolve(dir, 'module/ExactCore.framework');
          for (const changed of [
            { library: first, moduleLibraries: [second] },
            { moduleLibraries: [first, second] },
            { shared: { framework: core }, moduleLibraries: [{ placed: moduleCore, link: 'ExactCore' }] },
          ]) {
            const sources = changed.shared ? [core, moduleCore] : [first.placed, second.placed];
            assert.throws(() => (platform === 'macos' ? assembleMac : assembleIOS)({ ...input, ...changed }, tools), error =>
              sources.every(source => error.message.includes(source)) && error.message.includes('framework'), `${platform}: names both colliding sources`);
          }
        }
        if (platform === 'macos') assembleMac(input, tools);
        else assembleIOS(input, tools);
        const contents = platform === 'macos' ? resolve(bundle, 'Contents') : bundle;
        const embedded = resolve(contents, 'Frameworks/Dynamic.framework');
        assert.ok(existsSync(embedded), `${sdk}: dynamic framework is embedded`);
        assert.equal(existsSync(resolve(contents, 'Frameworks/Static.framework')), false);
        assert.equal(readFileSync(resolve(embedded, platform === 'macos' ? 'Resources/fixture.txt' : 'fixture.txt'), 'utf8'), `${platform}/${variant ?? 'device'}`);
        assert.equal(codeDigest(readFileSync(resolve(embedded, 'Dynamic'))), codeDigest(readFileSync(resolve(selection.libraries[0].placed, 'Dynamic'))), `${sdk}/${production}: prebuilt code is preserved`);
        if (platform === 'macos') assert.equal(codeDigest(readFileSync(resolve(embedded, 'Versions/A/Dynamic'))), codeDigest(readFileSync(resolve(selection.libraries[0].placed, 'Versions/A/Dynamic'))), 'the embedded version is the linked binary');
        run('codesign', ['--verify', '--strict', embedded]);
        run('codesign', ['--verify', '--deep', '--strict', bundle]);
        const dylib = resolve(contents, platform === 'macos' ? 'MacOS/libexact_modules.dylib' : production ? 'Frameworks/ExactModules.framework/ExactModules' : 'Frameworks/libexact_modules.dylib');
        const rpaths = [...run('otool', ['-l', dylib]).stdout.matchAll(/cmd LC_RPATH\s+cmdsize \d+\s+path (\S+)/g)].map(match => match[1]);
        const expected = platform === 'macos' ? '@loader_path/../Frameworks' : '@executable_path/Frameworks';
        assert.ok(rpaths.includes(expected));
        const install = /(@rpath\/Dynamic\.framework\/\S+)/.exec(run('otool', ['-L', dylib]).stdout)[1];
        const search = expected.replace('@loader_path', dirname(dylib)).replace('@executable_path', platform === 'macos' ? resolve(contents, 'MacOS') : bundle);
        assert.ok(existsSync(install.replace('@rpath', search)), 'the module dependency resolves inside the bundle');
        if (platform === 'macos') run(resolve(contents, 'MacOS/Fixture'), []);
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120_000);
