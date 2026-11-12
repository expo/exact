import { test, expect } from 'bun:test';
import { chmodSync, copyFileSync, mkdtempSync, mkdirSync, readFileSync, readlinkSync, realpathSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { appleCommand, crossSdk } from './toolchain.mjs';
import { decodeProfile } from './devices.mjs';
import { appleCrossReport, CROSS_IOS_ROWS, distributionToolchain, ensureWatchSimulatorSDK, installWatchSimulatorSDK } from './sdk.mjs';

test('Windows Metal preserves deployment targets and path operands through Wine, while native hosts keep xcrun', () => {
  const root = mkdtempSync(resolve(tmpdir(), "exact Metal's "));
  try {
    mkdirSync(resolve(root, 'bin')); for (const name of ['metal', 'metallib']) writeFileSync(resolve(root, 'bin', name + '.exe'), 'MZ');
    const env = { EXACT_APPLE_METAL_TOOLS: root, EXACT_APPLE_WINE: '/selected Wine/wine64', WINEPREFIX: '/isolated prefix' };
    const options = { env, cwd: root }, input = resolve(root, 'Shader source.metal'), output = resolve(root, 'Shader.air');
    const windows = path => 'Z:' + path.replaceAll('/', '\\');
    for (const [sdk, minimum, triple] of [['iphoneos', '-mios-version-min=17.0', 'air64-apple-ios17.0'], ['iphonesimulator', '-mios-simulator-version-min=18.2', 'air64-apple-ios18.2-simulator'], ['macosx', '-mmacosx-version-min=14.0', 'air64-apple-macos14.0']]) {
      const args = ['--sdk', sdk, 'metal', '-v', '-std=metal3.0', minimum, '-c', input, '-o', output];
      const [program, translated, settings] = appleCommand('xcrun', args, options, 'linux');
      expect(program).toBe(env.EXACT_APPLE_WINE);
      expect(translated).toEqual([resolve(root, 'bin/metal.exe'), '-target', triple, '-v', '-std=metal3.0', '-c', windows(input), '-o', windows(output)]);
      expect(settings.env.WINEPREFIX).toBe(env.WINEPREFIX);
      expect(appleCommand('xcrun', args, options, 'darwin')).toEqual(['xcrun', args, options]);
    }
    expect(appleCommand('xcrun', ['-sdk', 'iphonesimulator', 'metallib', output, '-o', './Shader.metallib'], options, 'linux')[1])
      .toEqual([resolve(root, 'bin/metallib.exe'), windows(output), '-o', windows(resolve(root, 'Shader.metallib'))]);
    expect(appleCommand('xcrun', ['-sdk', 'macosx', '-f', 'metal'], options, 'linux').slice(0, 2))
      .toEqual(['/usr/bin/printf', ['%s\n', resolve(root, 'bin/metal.exe')]]);
    expect(appleCommand('xcrun', ['-sdk', 'iphoneos', 'metal', '--version'], { env: {} }, 'linux')[0]).toBe('metal');
    expect(() => appleCommand('xcrun', ['-sdk', 'iphoneos', 'metal', '-target', 'air64-apple-ios17.0', '-mios-version-min=18.0'], options, 'linux')).toThrow('one deployment target');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('Cargo subprocesses receive an executable xcrun with the current SDK configuration', () => fixture(({ root, sdk, env }) => {
  const fullEnv = { ...process.env, ...env, EXACT_APPLE_METAL_TOOLS: root };
  const [, , options] = appleCommand('cargo', ['build', '--target', 'aarch64-apple-ios-sim'], { env: fullEnv, cwd: root }, 'linux');
  const xcrun = resolve(options.env.PATH.split(':')[0], 'xcrun');
  expect(readFileSync(xcrun, 'utf8')).toContain('host/apple/toolchain.mjs');
  if (process.platform === 'linux') {
    const result = spawnSync(xcrun, ['--sdk', 'iphonesimulator', '--show-sdk-path'], { env: options.env, encoding: 'utf8' });
    expect(result.status, result.stderr).toBe(0); expect(result.stdout.trim()).toBe(sdk);
  }
}));

test('portable IPA packaging retains the Payload root, executable modes and relative symlinks', () => {
  const root = mkdtempSync(resolve(tmpdir(), "exact IPA's "));
  try {
    const app = resolve(root, 'Payload/Fixture.app'); mkdirSync(resolve(app, 'Frameworks'), { recursive: true });
    const executable = resolve(app, 'Fixture'); writeFileSync(executable, 'fixture executable'); chmodSync(executable, 0o755);
    symlinkSync('../Fixture', resolve(app, 'Frameworks/fixture-link'));
    const archive = resolve(root, 'out/Fixture.ipa'); mkdirSync(resolve(root, 'out'));
    const result = spawnSync(...appleCommand('ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', resolve(root, 'Payload'), archive], { encoding: 'utf8' }, 'linux'));
    expect(result.status).toBe(0);
    const extracted = resolve(root, 'unpacked'); mkdirSync(extracted);
    expect(spawnSync('unzip', ['-q', archive, '-d', extracted]).status).toBe(0);
    expect(readFileSync(resolve(extracted, 'Payload/Fixture.app/Fixture'), 'utf8')).toBe('fixture executable');
    expect(readlinkSync(resolve(extracted, 'Payload/Fixture.app/Frameworks/fixture-link'))).toBe('../Fixture');
    expect(spawnSync('/bin/test', ['-x', resolve(extracted, 'Payload/Fixture.app/Fixture')]).status).toBe(0);
    expect(() => appleCommand('ditto', ['--unknown'], {}, 'linux')).toThrow('unsupported portable archive');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function fixture(run) {
  const root = mkdtempSync(resolve(tmpdir(), "exact cross sdk's "));
  const sdk = resolve(root, 'Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk');
  mkdirSync(sdk, { recursive: true });
  writeFileSync(resolve(sdk, 'SDKSettings.json'), JSON.stringify({ Version: '26.4' }));
  writeFileSync(resolve(root, 'swift-sdk.json'), JSON.stringify({ targetTriples: { 'arm64-apple-ios-simulator': {
    sdkRootPath: 'Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator.sdk',
    swiftResourcesPath: 'Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift',
  } } }));
  try { run({ root, sdk, env: { EXACT_APPLE_SDK: root } }); }
  finally { rmSync(root, { recursive: true, force: true }); }
}

test('SDK discovery uses installed metadata and preserves paths containing spaces', () => fixture(({ root, sdk, env }) => {
  expect(crossSdk('iphonesimulator', env)).toMatchObject({ root, sdk, version: '26.4' });
  const command = appleCommand('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path'], { env, encoding: 'utf8' }, 'linux');
  const answer = spawnSync(...command);
  expect(answer.status).toBe(0);
  expect(answer.stdout).toBe(sdk + '\n');
  expect(() => crossSdk('unknown', env)).toThrow('no cross SDK mapping');
}));

test('Linux setup names missing cross tools and uses the selected LLVM directory', () => fixture(({ env }) => {
  const calls = [], llvm = '/selected LLVM/bin';
  const rows = appleCrossReport({ ...env, EXACT_APPLE_LLVM_BIN: llvm }, (command, args) => {
    calls.push(command); return spawnSync('/usr/bin/false', args, { encoding: 'utf8' });
  });
  expect(rows.map(row => row.name)).toEqual(CROSS_IOS_ROWS);
  expect(rows.every(row => !row.ok && row.fix)).toBe(true);
  expect(rows[0].have).toContain('no iphoneos compiler configuration');
  expect(calls).toContain(resolve(llvm, 'llvm-lipo'));
  expect(appleCommand('lipo', ['-archs', 'App'], { env: { EXACT_APPLE_LLVM_BIN: llvm } }, 'linux')[0]).toBe(resolve(llvm, 'llvm-lipo'));
}));

test('the watch SDK is provisioned from Xcode input with Linux macros and both compiler targets', () => fixture(({ root, env }) => {
  const xcode = resolve(root, 'Xcode.app'), platform = resolve(xcode, 'Contents/Developer/Platforms/WatchSimulator.platform');
  const watch = resolve(platform, 'Developer/SDKs/WatchSimulator27.1.sdk');
  const original = JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8'));
  expect(() => installWatchSimulatorSDK(xcode, env)).toThrow('no watch simulator SDK');
  expect(() => ensureWatchSimulatorSDK(env)).toThrow('without --slim');
  expect(JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8'))).toEqual(original);
  mkdirSync(watch, { recursive: true });
  writeFileSync(resolve(watch, 'SDKSettings.json'), JSON.stringify({ Version: '27.1' }));
  writeFileSync(resolve(watch, 'header.h'), 'int watch_fixture(void);');
  symlinkSync('WatchSimulator27.1.sdk', resolve(platform, 'Developer/SDKs/WatchSimulator.sdk'));
  writeFileSync(resolve(root, 'OpenAppleMacrosServer'), 'Linux plugin server'); mkdirSync(resolve(root, 'OpenAppleMacrosPlugins'));
  const sdk = ensureWatchSimulatorSDK(env);
  expect(ensureWatchSimulatorSDK(env).sdk).toBe(sdk.sdk);
  expect(sdk.version).toBe('27.1'); expect(readFileSync(resolve(sdk.sdk, 'header.h'), 'utf8')).toBe('int watch_fixture(void);');
  expect(readlinkSync(resolve(sdk.platform, 'Developer/SDKs/WatchSimulator.sdk'))).toBe('WatchSimulator27.1.sdk');
  expect(realpathSync(resolve(sdk.platform, 'Developer/usr/bin/swift-plugin-server'))).toBe(realpathSync(resolve(root, 'OpenAppleMacrosServer')));
  expect(realpathSync(resolve(sdk.platform, 'Developer/usr/lib/swift/host/plugins'))).toBe(realpathSync(resolve(root, 'OpenAppleMacrosPlugins')));
  const metadata = JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8'));
  expect(metadata.targetTriples['arm64-apple-ios-simulator']).toEqual(original.targetTriples['arm64-apple-ios-simulator']);
  const cargo = appleCommand('cargo', ['build', '--target', 'aarch64-apple-watchos-sim'], { cwd: root, env }, 'linux');
  expect(cargo[2].env.WATCHOS_DEPLOYMENT_TARGET).toBe('10.0'); expect(cargo[2].env.SDKROOT).toBe(sdk.sdk);
  const linker = readFileSync(cargo[2].env.CARGO_TARGET_AARCH64_APPLE_WATCHOS_SIM_LINKER, 'utf8');
  expect(linker).toContain('arm64-apple-watchos10.0-simulator'); expect(linker).toContain('watchos-simulator,10.0,27.1');
  const swift = appleCommand('swift', ['build', '--package-path', root, '--triple', 'arm64-apple-watchos10.0-simulator', '--target', 'ExactCore'], { env }, 'linux');
  const destination = JSON.parse(readFileSync(swift[1].at(-1), 'utf8'));
  expect(destination.targetTriples['arm64-apple-watchos-simulator'].sdkRootPath).toBe(sdk.sdk);
  const targetOptions = JSON.parse(readFileSync(destination.targetTriples['arm64-apple-watchos-simulator'].toolsetPaths.at(-1), 'utf8')).swiftCompiler.extraCLIOptions;
  expect(targetOptions).toContain('-external-plugin-path');
  expect(targetOptions).toContain(resolve(sdk.platform, 'Developer/usr/lib/swift/host/plugins') + '#' + resolve(sdk.platform, 'Developer/usr/bin/swift-plugin-server'));
  expect(linker).toContain("'-fuse-ld=lld'"); expect(linker).toContain(`'--ld-path=${resolve(sdk.tools, 'ld64.lld').replaceAll("'", "'\\''")}'`);
  const compiler = appleCommand('xcrun', ['--sdk', 'watchsimulator', 'swiftc', '-target', 'arm64-apple-watchos10.0-simulator', '-emit-library', 'Views.swift'], { env }, 'linux');
  expect(compiler[1]).toContain(sdk.runtime); expect(sdk.runtime).toEndWith('libclang_rt.watchossim.a');
  writeFileSync(resolve(watch, 'header.h'), 'int updated_watch_fixture(void);'); installWatchSimulatorSDK(xcode, env);
  expect(readFileSync(resolve(sdk.sdk, 'header.h'), 'utf8')).toContain('updated_watch_fixture');
}));

test('SwiftPM receives a single-target destination while retaining the app deployment target and link arguments', () => fixture(({ root, sdk, env }) => {
  const scratch = resolve(root, 'build scratch');
  const args = ['build', '--triple', 'arm64-apple-ios18.2-simulator', '--sdk', sdk, '--scratch-path', scratch, '--product', 'ExactIOS', '-Xlinker', '-rpath', '-Xlinker', '@executable_path/Frameworks'];
  const invocation = { env: { ...env, SDKROOT: '/host/sdk' } };
  const [command, translated, options] = appleCommand('swift', args, invocation, 'linux');
  expect(command).toBe('swift');
  expect(translated.slice(0, 13)).toEqual(['build', '--triple', 'arm64-apple-ios18.2-simulator', '--scratch-path', scratch, '--product', 'ExactIOS', '-Xlinker', '-rpath', '-Xlinker', '@executable_path/Frameworks', '--build-system', 'native']);
  const destination = translated[translated.indexOf('--destination') + 1];
  const metadata = JSON.parse(readFileSync(destination, 'utf8'));
  expect(metadata).toMatchObject({ schemaVersion: '4.0', targetTriples: { 'arm64-apple-ios-simulator': { sdkRootPath: sdk, swiftResourcesPath: resolve(root, 'Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift') } } });
  expect(appleCommand('swift', args, invocation, 'linux')[1]).toEqual(translated);
  const targetOptions = JSON.parse(readFileSync(metadata.targetTriples['arm64-apple-ios-simulator'].toolsetPaths.at(-1), 'utf8')).swiftCompiler.extraCLIOptions;
  const librarian = JSON.parse(readFileSync(metadata.targetTriples['arm64-apple-ios-simulator'].toolsetPaths.at(-1), 'utf8')).librarian.path;
  expect(librarian).toEndWith('libtool');
  expect(readlinkSync(librarian)).toBe(resolve(root, 'toolset/bin/llvm-lib'));
  expect(targetOptions).toContain('-fuse-ld=lld');
  expect(targetOptions).toContain(`--ld-path=${resolve(root, 'toolset/bin/ld64.lld')}`);
  expect(targetOptions).toContain(resolve(root, 'Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/clang/lib/darwin/libclang_rt.iossim.a'));
  expect(targetOptions.slice(-8)).toEqual(['-Xlinker', '-platform_version', '-Xlinker', 'ios-simulator', '-Xlinker', '18.2', '-Xlinker', '26.4']);
  const compatible = [...args, '-Xlinker', '-platform_version', '-Xlinker', 'ios-simulator', '-Xlinker', '18.2', '-Xlinker', '18.0'];
  const alternate = appleCommand('swift', compatible, invocation, 'linux')[1];
  const alternateMetadata = JSON.parse(readFileSync(alternate[alternate.indexOf('--destination') + 1], 'utf8'));
  const extra = JSON.parse(readFileSync(alternateMetadata.targetTriples['arm64-apple-ios-simulator'].toolsetPaths.at(-1), 'utf8')).swiftCompiler.extraCLIOptions;
  expect(extra).not.toContain('-platform_version');
  expect(Object.hasOwn(options.env, 'SDKROOT')).toBe(false);
  expect(args).toContain('--sdk');
}));

test('direct Swift module compilation uses cross resources and the Linux macro server', () => fixture(({ root, sdk, env }) => {
  const [command, args] = appleCommand('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', '-target', 'arm64-apple-ios17.0-simulator', '-emit-library', 'Module.swift'], { env }, 'linux');
  expect(command).toBe('swiftc');
  expect(args).toContain(sdk);
  expect(args).toContain(resolve(root, 'Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift'));
  expect(args).toContain(resolve(root, 'Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/clang/lib/darwin/libclang_rt.iossim.a'));
  expect(args).toContain(resolve(root, 'Developer/Platforms/iPhoneSimulator.platform/Developer/usr/lib/swift/host/plugins') + '#' + resolve(root, 'Developer/Platforms/iPhoneSimulator.platform/Developer/usr/bin/swift-plugin-server'));
}));

test('native tool invocations and host builds are unchanged', () => {
  const args = ['--sdk', 'iphoneos', 'swiftc', '-emit-library', 'Module.swift'], options = { cwd: '/somewhere' };
  expect(appleCommand('xcrun', args, options, 'darwin')).toEqual(['xcrun', args, options]);
  expect(appleCommand('swift', ['build'], options, 'linux')).toEqual(['swift', ['build'], options]);
  expect(appleCommand('cargo', ['build', '--target', 'x86_64-unknown-linux-gnu'], options, 'linux')).toEqual(['cargo', ['build', '--target', 'x86_64-unknown-linux-gnu'], options]);
  const installer = ['run', '--', '--target', 'aarch64-apple-ios-sim'];
  expect(appleCommand('cargo', installer, options, 'linux')).toEqual(['cargo', installer, options]);
});

test('Linux ad-hoc signing seals each bundle without re-signing its already signed children', () => {
  const options = { env: { EXACT_APPLE_CODESIGN: '/tools/rcodesign' } };
  const args = ['--force', '--sign', '-', '--timestamp=none', '--identifier', 'com.exact.test', '--entitlements', '/a:folder/entitlements.plist', '/path with spaces/App.app'];
  expect(appleCommand('codesign', args, options, 'linux')).toEqual(['/tools/rcodesign', ['sign', '--config-file', '/dev/null', '--timestamp-url', 'none', '--shallow', '--binary-identifier', 'com.exact.test', '--entitlements-xml-file', 'main:/a:folder/entitlements.plist', '/path with spaces/App.app'], options]);
  expect(appleCommand('codesign', args, options, 'darwin')).toEqual(['codesign', args, options]);
  expect(() => appleCommand('codesign', ['--sign', 'keychain-only-identity', '/App.app'], options, 'linux')).toThrow('macOS Keychain');
  expect(() => appleCommand('codesign', ['--sign', '-', '--unknown', '/App.app'], options, 'linux')).toThrow('unsupported portable codesign');
  const verify = ['--verify', '--strict', '/App.app'];
  expect(appleCommand('codesign', verify, options, 'linux')).toEqual(['codesign', verify, options]);
});

test.skipIf(process.platform !== 'darwin' || !process.env.EXACT_APPLE_CODESIGN)('portable signatures pass Apple strict verification, preserve nested signatures, and detect changed resources', () => {
  expect(appleCrossReport().find(row => row.name === 'Apple codesign')).toMatchObject({ have: '0.29.0', ok: true });
  const root = mkdtempSync(resolve(tmpdir(), 'exact-portable-signing-'));
  const app = resolve(root, 'Test.app'), child = resolve(app, 'PlugIns/Child.appex');
  const bundle = (path, id) => {
    mkdirSync(path, { recursive: true }); copyFileSync('/usr/bin/true', resolve(path, 'Program'));
    writeFileSync(resolve(path, 'Info.plist'), `<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>Program</string><key>CFBundleIdentifier</key><string>${id}</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string></dict></plist>`);
  };
  const sign = path => {
    const result = spawnSync(...appleCommand('codesign', ['--force', '--sign', '-', '--timestamp=none', path], { env: process.env, encoding: 'utf8' }, 'linux'));
    expect(result.stderr).not.toContain('error:'); expect(result.status).toBe(0);
  };
  const verify = () => spawnSync('codesign', ['--verify', '--strict', '--deep', app], { encoding: 'utf8' }).status;
  try {
    bundle(app, 'com.exact.portable-signing'); bundle(child, 'com.exact.portable-signing.child');
    const resource = resolve(app, 'sealed.txt'); writeFileSync(resource, 'sealed resource');
    sign(child); const signature = readFileSync(resolve(child, 'Program'));
    sign(app); expect(readFileSync(resolve(child, 'Program'))).toEqual(signature); expect(verify()).toBe(0);
    writeFileSync(resource, 'changed resource'); expect(verify()).not.toBe(0);
    writeFileSync(resource, 'sealed resource'); expect(verify()).toBe(0);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('Cargo links with the selected Apple SDK and minimum OS while preserving target flags and host tools', () => fixture(({ root, sdk, env }) => {
  const directory = resolve(root, "tools with 'quotes'");
  mkdirSync(directory);
  const clang = resolve(directory, 'clang');
  writeFileSync(clang, '#!/usr/bin/env python3\nimport json,sys\nprint(json.dumps(sys.argv[1:]))\n');
  chmodSync(clang, 0o755);
  const flags = '-C debuginfo=1 --cfg=user_configuration';
  const args = ['rustc', '--target', 'aarch64-apple-ios-sim', '--target-dir', "output with 'quotes'", '--', '--crate-type=staticlib'];
  const [command, translated, options] = appleCommand('cargo', args, { cwd: root, env: { ...env, PATH: `${directory}:${process.env.PATH}`, IPHONEOS_DEPLOYMENT_TARGET: '18.2', RUSTFLAGS: flags, CC: '/host/compiler' } }, 'linux');
  expect(command).toBe('cargo'); expect(translated).toEqual(args);
  expect(options.env.SDKROOT).toBe(sdk);
  expect(options.env.RUSTFLAGS).toBe(flags); expect(options.env.CC).toBe('/host/compiler');
  expect(options.env.CC_aarch64_apple_ios_sim).toBe('clang');
  const executable = options.env.CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER;
  const linked = spawnSync(executable, ["object with 'quotes'.o", '-o', 'test executable'], { env: options.env, encoding: 'utf8' });
  expect(linked.status).toBe(0);
  expect(JSON.parse(linked.stdout)).toEqual(['--target=arm64-apple-ios18.2-simulator', '-isysroot', sdk,
    '-Xlinker', resolve(root, 'Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/clang/lib/darwin/libclang_rt.iossim.a'),
    '-fuse-ld=lld', `--ld-path=${resolve(root, 'toolset/bin/ld64.lld')}`, '-Wl,-platform_version,ios-simulator,18.2,26.4', "object with 'quotes'.o", '-o', 'test executable']);
  expect(appleCommand('cargo', args, { cwd: root, env: { ...env, IPHONEOS_DEPLOYMENT_TARGET: '18.2' } }, 'linux')[2].env.CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER).toBe(executable);
}));

test('Linux plist tools preserve typed XML and binary bundle metadata and report missing keys', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-plist-'));
  const file = resolve(dir, 'Info.plist');
  const run = args => spawnSync(...appleCommand('plutil', args, { encoding: 'utf8' }, 'linux'));
  const ok = args => { const r = run(args); expect(r.stderr).toBe(''); expect(r.status).toBe(0); return r.stdout; };
  try {
    writeFileSync(file, JSON.stringify({ CFBundleExecutable: 'App (Exact)', 'key.with.dots': 'kept', Enabled: true, Counts: [1, 2] }));
    ok(['-convert', 'xml1', file]);
    expect(readFileSync(file, 'utf8')).toContain('<plist');
    expect(ok(['-extract', 'CFBundleExecutable', 'raw', '-o', '-', file])).toBe('App (Exact)');
    expect(ok(['-extract', 'key\\.with\\.dots', 'raw', '-o', '-', file])).toBe('kept');
    ok(['-insert', 'ExactWidgetTypeScript', '-bool', 'true', file]);
    ok(['-replace', 'Enabled', '-bool', 'NO', file]);
    ok(['-convert', 'binary1', file]);
    expect(readFileSync(file).subarray(0, 8).toString()).toBe('bplist00');
    expect(JSON.parse(ok(['-convert', 'json', '-o', '-', file]))).toEqual({ CFBundleExecutable: 'App (Exact)', 'key.with.dots': 'kept', Enabled: false, Counts: [1, 2], ExactWidgetTypeScript: true });
    const before = readFileSync(file);
    expect(run(['-extract', 'Missing', 'raw', '-o', '-', file]).status).toBe(1);
    expect(run(['-insert', 'Enabled', '-bool', 'true', file]).status).toBe(1);
    expect(readFileSync(file)).toEqual(before);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('Linux archives record the actual SDK build without inventing an Xcode or macOS version', () => fixture(({ sdk, env }) => {
  const system = resolve(sdk, 'System/Library/CoreServices'); mkdirSync(system, { recursive: true });
  const file = resolve(system, 'SystemVersion.plist');
  writeFileSync(file, '<?xml version="1.0"?><plist version="1.0"><dict><key>ProductBuildVersion</key><string>23E214</string></dict></plist>');
  const metadata = distributionToolchain('iphonesimulator', env, 'linux');
  expect(metadata).toEqual({ DTCompiler: 'com.apple.compilers.llvm.clang.1_0', DTPlatformBuild: '23E214', DTPlatformVersion: '26.4',
    DTSDKBuild: '23E214', DTSDKName: 'iphonesimulator26.4' });
  writeFileSync(file, '<?xml version="1.0"?><plist version="1.0"><dict/></plist>');
  expect(() => distributionToolchain('iphonesimulator', env, 'linux')).toThrow('no ProductBuildVersion');
}));


test.skipIf(!process.env.EXACT_APPLE_CODESIGN || (process.platform !== 'darwin' && !process.env.EXACT_SIGNING_FIXTURE))('portable certificate signing and CMS profile decoding use file credentials and verify their cryptographic signatures', () => {
  const root = mkdtempSync(resolve(tmpdir(), 'exact-p12-signing-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8' });
    expect(r.status, r.stderr).toBe(0); return r;
  };
  try {
    const cert = resolve(root, 'certificate.pem'), key = resolve(root, 'key.pem'), p12 = resolve(root, 'identity.p12'), password = resolve(root, 'password');
    writeFileSync(password, 'qualification-only-password', { mode: 0o600 });
    const config = resolve(root, 'openssl.cnf'); writeFileSync(config, '[req]\ndistinguished_name=dn\n[dn]\nCN=Exact Signing Qualification\n[signing]\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\n');
    run('openssl', ['req', '-x509', '-config', config, '-extensions', 'signing', '-newkey', 'rsa:2048', '-nodes', '-sha256', '-days', '1', '-subj', '/CN=Exact Signing Qualification/OU=EXACTTEST1', '-keyout', key, '-out', cert]);
    run('openssl', ['pkcs12', '-export', '-inkey', key, '-in', cert, '-out', p12, '-passout', `file:${password}`, '-keypbe', 'PBE-SHA1-3DES', '-certpbe', 'PBE-SHA1-3DES', '-macalg', 'sha1']);
    const binary = resolve(root, 'Signed'); copyFileSync(process.env.EXACT_SIGNING_FIXTURE ?? '/usr/bin/true', binary);
    const options = { env: { ...process.env, EXACT_APPLE_SIGN_PASSWORD_FILE: password }, encoding: 'utf8' };
    const command = appleCommand('codesign', ['--force', '--sign', p12, '--timestamp=none', binary], options, 'linux');
    expect(command[1]).not.toContain('qualification-only-password');
    const signed = spawnSync(...command); expect(signed.status, signed.stderr).toBe(0);
    run(process.env.EXACT_APPLE_CODESIGN, ['verify', '--config-file', '/dev/null', binary]);
    if (process.env.EXACT_SIGNING_EVIDENCE) copyFileSync(binary, process.env.EXACT_SIGNING_EVIDENCE);
    if (process.platform === 'darwin') run('codesign', ['--verify', '--strict', binary]);
    const profile = resolve(root, 'profile.plist'), cms = resolve(root, 'profile.mobileprovision');
    writeFileSync(profile, '<?xml version="1.0"?><plist version="1.0"><dict><key>Name</key><string>Portable qualification</string><key>TeamIdentifier</key><array><string>EXACTTEST1</string></array><key>Platform</key><array><string>iOS</string></array><key>ExpirationDate</key><date>2099-01-01T00:00:00Z</date><key>Entitlements</key><dict><key>application-identifier</key><string>EXACTTEST1.dev.exact.fixture</string><key>get-task-allow</key><true/></dict></dict></plist>');
    run('openssl', ['smime', '-sign', '-binary', '-nodetach', '-in', profile, '-signer', cert, '-inkey', key, '-outform', 'DER', '-out', cms]);
    expect(decodeProfile(cms, 'linux')).toMatchObject({ name: 'Portable qualification', team: 'EXACTTEST1', appId: 'EXACTTEST1.dev.exact.fixture', dev: true, platforms: ['iOS'], method: 'development' });
    const bytes = readFileSync(cms); const at = bytes.indexOf(Buffer.from('Portable qualification')); expect(at).toBeGreaterThan(0); bytes[at] ^= 1; writeFileSync(cms, bytes);
    expect(() => decodeProfile(cms, 'linux')).toThrow('cannot decode provisioning profile');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
