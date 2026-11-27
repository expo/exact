import { test, expect, spyOn } from 'bun:test';
import { chmodSync, copyFileSync, existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, readlinkSync, realpathSync, utimesSync, writeFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { relative, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { appleCommand, crossSdk, experimental, spawn as appleSpawn, spawnSync as appleSpawnSync, swiftOptimization, swiftOptimizationKey } from './toolchain.mjs';
import { widgetReload } from './widgets.mjs';
import { appleComposition } from './link.mjs';
import { createHash } from 'node:crypto';
import { decodeProfile, xcodeRefusal } from './devices.mjs';
import { appleCrossReport, CROSS_IOS_ROWS, distributionToolchain, ensureWatchSDK, installWatchSDK } from './sdk.mjs';
import { iosTestRuns } from './xctest.mjs';
import { appleCargo, cargoDiagnosticsCommand } from '../../scripts/cargo-check.mjs';

test.skipIf(process.platform !== 'darwin')('certificate signing names its identity, bounds keychain waits and explains failures', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-signing-'));
  const said = spyOn(console, 'error').mockImplementation(() => {});
  try {
    const tool = resolve(dir, 'codesign');
    writeFileSync(tool, '#!/bin/sh\necho errSecInternalComponent >&2\nexit 1\n');
    chmodSync(tool, 0o755);
    const options = { env: { ...process.env, PATH: `${dir}:${process.env.PATH}` }, encoding: 'utf8', macDevelopmentSign: true };
    const args = ['--force', '--sign', 'SELECTED_IDENTITY', '--timestamp=none', '/fixture/libexact_web.dylib'];
    const [, , settings] = appleCommand('codesign', args, options, 'darwin');
    expect(settings.timeout).toBe(30_000);
    expect(settings.killSignal).toBe('SIGKILL');
    expect(appleSpawnSync('codesign', args, options).status).toBe(1);
    const output = () => said.mock.calls.flat().join('\n');
    expect(output()).toContain('SELECTED_IDENTITY');
    expect(output()).toContain('/fixture/libexact_web.dylib');
    expect(output()).toContain('EXACT_IDENTITY=-');
    said.mockClear();
    expect(appleSpawnSync('codesign', args, { ...options, macDevelopmentSign: false }).status).toBe(1);
    expect(output()).toContain('SELECTED_IDENTITY');
    expect(output()).not.toContain('EXACT_IDENTITY=-');
    said.mockClear();
    writeFileSync(tool, '#!/bin/sh\nexec /bin/sleep 600\n');
    const timed = appleSpawnSync('codesign', args, { ...options, timeout: 20 });
    expect(timed.error?.code).toBe('ETIMEDOUT');
    expect(timed.signal).toBe('SIGKILL');
    expect(output()).toContain('EXACT_IDENTITY=-');
    said.mockClear();
    writeFileSync(tool, '#!/bin/sh\nexit 0\n');
    expect(appleSpawnSync('codesign', args, options).status).toBe(0);
    expect(output()).not.toContain('EXACT_IDENTITY=-');
    const { macDevelopmentSign, ...nativeOptions } = options;
    expect(appleCommand('codesign', ['--sign', '-', '/fixture'], options, 'darwin')[2]).toEqual(nativeOptions);
    expect(appleCommand('codesign', ['--verify', '/fixture'], options, 'darwin')[2]).toEqual(nativeOptions);
  } finally { said.mockRestore(); rmSync(dir, { recursive: true, force: true }); }
});

for (const kind of ['GPU preparation', 'widget frameworks']) test.skipIf(process.platform !== 'darwin')(`${kind} gives the ad hoc hint only for Mac development`, async () => {
  const { prepareGpuProduct, publishMacFramework } = await import('./build.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-signing-callers-')), savedPath = process.env.PATH;
  const said = spyOn(console, 'error').mockImplementation(() => {});
  try {
    writeFileSync(resolve(dir, 'codesign'), '#!/bin/sh\necho errSecInternalComponent >&2\nexit 1\n');
    chmodSync(resolve(dir, 'codesign'), 0o755);
    process.env.PATH = `${dir}:${savedPath}`;
    if (kind === 'GPU preparation') {
      const product = resolve(dir, 'libgpu.dylib'); writeFileSync(product, 'fixture GPU');
      for (const [ios, distribution] of [[false, false], [true, false], [false, true], [true, true]]) {
        said.mockClear();
        expect(() => prepareGpuProduct(product, { gpuArtifacts: [{ built: 'libgpu.dylib' }], sha1: 'SELECTED_IDENTITY',
          app: {}, ios, distribution, preparedGpu: new Map() })).toThrow(/codesign failed/);
        const output = said.mock.calls.flat().join('\n');
        if (!distribution) expect(output).toContain('SELECTED_IDENTITY');
        expect(output.includes('EXACT_IDENTITY=-')).toBe(!ios && !distribution);
      }
    } else {
      const framework = resolve(dir, 'ExactCore.framework'); mkdirSync(framework);
      copyFileSync('/usr/bin/true', resolve(framework, 'ExactCore'));
      for (const distribution of [true, false]) {
        said.mockClear();
        expect(() => publishMacFramework(framework, resolve(dir, 'published'), 'SELECTED_IDENTITY', false, [], !distribution)).toThrow(/codesign failed/);
        const output = said.mock.calls.flat().join('\n');
        expect(output).toContain('SELECTED_IDENTITY');
        expect(output.includes('EXACT_IDENTITY=-')).toBe(!distribution);
      }
    }
  } finally {
    process.env.PATH = savedPath; said.mockRestore(); rmSync(dir, { recursive: true, force: true });
  }
});

test.skipIf(process.platform !== 'darwin')('timestamped release signing has no development timeout or ad hoc hint', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-release-signing-'));
  const said = spyOn(console, 'error').mockImplementation(() => {});
  try {
    const tool = resolve(dir, 'codesign');
    writeFileSync(tool, '#!/bin/sh\necho timestamp-server-failure >&2\nexit 1\n');
    chmodSync(tool, 0o755);
    const options = { env: { ...process.env, PATH: `${dir}:${process.env.PATH}` }, encoding: 'utf8' };
    const args = ['--force', '--sign', 'DEVELOPER_ID', '--timestamp', '/fixture/Release.app'];
    expect(appleCommand('codesign', args, options, 'darwin')[2]).toEqual(options);
    expect(appleSpawnSync('codesign', args, options).status).toBe(1);
    expect(said.mock.calls.flat().join('\n')).not.toContain('EXACT_IDENTITY=-');
    expect(said.mock.calls.flat().join('\n')).not.toContain('keychain access;');
  } finally { said.mockRestore(); rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('Swift builds use the selected Xcode compiler even when PATH shadows it', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-swift-selected-'));
  try {
    const bin = resolve(dir, 'bin'), selected = resolve(dir, 'selected');
    mkdirSync(bin); mkdirSync(selected);
    for (const tool of ['swift', 'swiftc']) {
      writeFileSync(resolve(bin, tool), '#!/bin/sh\necho "Apple Swift version 6.2.4"\n', { mode: 0o755 });
      writeFileSync(resolve(selected, tool), '#!/bin/sh\necho "Apple Swift version 6.4.2: $*"\n', { mode: 0o755 });
    }
    writeFileSync(resolve(bin, 'xcrun'), '#!/bin/sh\nif [ "$1" = "--sdk" ]; then echo 26.0; exit 0; fi\ntool="$1"; shift\nexec "$DEVELOPER_DIR/$tool" "$@"\n', { mode: 0o755 });
    const options = { env: { ...process.env, DEVELOPER_DIR: selected, PATH: `${bin}:${process.env.PATH}` }, encoding: 'utf8' };
    expect(xcodeRefusal('macosx', (cmd, args) => spawnSync(cmd, args, options), selected)).toBeNull();
    expect(spawnSync('swift', ['--version'], options).stdout.trim()).toBe('Apple Swift version 6.2.4');
    for (const [tool, args] of [['swift', ['build', '--target', 'ExactKit']], ['swiftc', ['--version']]]) {
      const expected = `Apple Swift version 6.4.2: ${args.join(' ')}\n`;
      const result = appleSpawnSync(tool, args, options);
      expect(result.status).toBe(0);
      expect(result.stdout).toBe(expected);
      const output = await new Promise((done, fail) => {
        const child = appleSpawn(tool, args, { env: options.env });
        let stdout = '';
        child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
        child.once('error', fail);
        child.once('close', code => code === 0 ? done(stdout) : fail(new Error(`${tool} exited ${code}`)));
      });
      expect(output).toBe(expected);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('a Swift driver run leaves nothing in TMPDIR, and SwiftPM runs share one that keeps an hour of leavings', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-swift-temp-')), bin = resolve(dir, 'bin'), base = resolve(dir, 'tmp');
  try {
    mkdirSync(bin); mkdirSync(base);
    // Each stand-in leaves what this Xcode's driver leaves, and says the TMPDIR it was given.
    for (const tool of ['swift', 'swiftc']) {
      writeFileSync(resolve(bin, tool), '#!/bin/sh\nmkdir "$(mktemp -d "$TMPDIR/TemporaryDirectory.XXXXXX")/.keep-directory"\nprintf %s "$TMPDIR"\n');
      chmodSync(resolve(bin, tool), 0o755);
    }
    writeFileSync(resolve(bin, 'xcrun'), '#!/bin/sh\ntool="$1"; shift\nexec "${0%/*}/$tool" "$@"\n', { mode: 0o755 });
    const options = { env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, TMPDIR: base }, encoding: 'utf8' };
    const driver = appleSpawnSync('swiftc', ['--version'], options);
    expect(driver.stdout.startsWith(`${base}/exact-swift-`)).toBe(true);
    expect(readdirSync(base)).toEqual([]);
    const shared = resolve(base, 'exact-swiftpm'), old = resolve(shared, 'TemporaryDirectory.old');
    mkdirSync(old, { recursive: true });
    utimesSync(old, new Date(Date.now() - 2 * 3600_000), new Date(Date.now() - 2 * 3600_000));
    expect(appleSpawnSync('swift', ['build', '--scratch-path', resolve(dir, 'scratch')], options).stdout).toBe(shared);
    const left = readdirSync(shared);
    expect(left.length).toBe(1);
    expect(left[0]).not.toBe('TemporaryDirectory.old');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

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

test.skipIf(process.platform === 'win32')('Linux Apple Cargo resolves the LLVM override before the diagnostics relay changes directory', () => fixture(({ root, env }) => {
  const metadata = resolve(root, 'swift-sdk.json'), sdk = JSON.parse(readFileSync(metadata, 'utf8'));
  sdk.targetTriples['arm64-apple-ios'] = sdk.targetTriples['arm64-apple-ios-simulator'];
  writeFileSync(metadata, JSON.stringify(sdk));
  const tools = resolve(root, 'installed/tools with spaces');
  mkdirSync(tools, { recursive: true });
  mkdirSync(resolve(root, 'installed/version'));
  symlinkSync('installed/version', resolve(root, 'active'));
  const bin = `${relative(process.cwd(), root)}/active/../tools with spaces`;
  const absolute = `${process.cwd()}/${bin}`;
  for (const tool of ['llvm-lipo', 'llvm-strip']) {
    writeFileSync(resolve(tools, tool), '#!/bin/sh\nprintf "%s\\n" "${0##*/}"\n', { mode: 0o755 });
  }
  writeFileSync(resolve(root, 'cargo'), '#!/bin/sh\nprintf "%s\\n" "$EXACT_APPLE_LLVM_BIN"\nexec "$EXACT_APPLE_LLVM_BIN/llvm-lipo"\n', { mode: 0o755 });
  const options = { cwd: root, env: { ...env, PATH: `${root}:${process.env.PATH}`, EXACT_APPLE_LLVM_BIN: bin },
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] };
  const prepare = args => appleCargo('cargo', args, options, 'linux');
  for (const target of [['--target', 'aarch64-apple-ios'], ['--target=aarch64-apple-ios-sim']]) {
    const args = ['build', ...target];
    for (const command of [cargoDiagnosticsCommand(...prepare(args)), appleCommand('cargo', args, options, 'linux')]) {
      const result = spawnSync(...command);
      expect(result.status, result.error?.message).toBe(0);
      expect(result.stdout).toBe(`${absolute}\nllvm-lipo\n`);
    }
  }
  const strip = spawnSync(...appleCommand('strip', [], options, 'linux'));
  expect(strip.status, strip.error?.message).toBe(0);
  expect(strip.stdout).toBe('llvm-strip\n');
  expect(options.env.EXACT_APPLE_LLVM_BIN).toBe(bin);
  for (const target of [['--target', 'aarch64-apple-ios'], ['--target=aarch64-apple-ios-sim']]) {
    const args = ['rustc', '--target', 'x86_64-unknown-linux-gnu', '--', ...target];
    expect(prepare(args)).toEqual(['cargo', args, options]);
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
  const rows = appleCrossReport({ ...env, EXACT_APPLE_LLVM_BIN: `${llvm}/` }, (command, args) => {
    calls.push(command); return spawnSync('/usr/bin/false', args, { encoding: 'utf8' });
  });
  expect(rows.map(row => row.name)).toEqual(CROSS_IOS_ROWS);
  expect(rows.every(row => !row.ok && row.fix)).toBe(true);
  expect(rows[0].have).toContain('no iphoneos compiler configuration');
  expect(calls).toContain(`${llvm}/llvm-lipo`);
  expect(appleCommand('lipo', ['-archs', 'App'], { env: { EXACT_APPLE_LLVM_BIN: llvm } }, 'linux')[0]).toBe(`${llvm}/llvm-lipo`);
}));

test.each([
  ['watchsimulator', 'WatchSimulator', 'arm64-apple-watchos-simulator', 'aarch64-apple-watchos-sim', '-simulator', 'watch simulator', 'watchossim'],
  ['watchos', 'WatchOS', 'arm64-apple-watchos', 'aarch64-apple-watchos', '', 'watchOS device', 'watchos'],
])('the %s SDK is provisioned from Xcode input with Linux macros and both compiler targets', (name, folder, key, target, variant, label, runtime) => fixture(({ root, env }) => {
  const xcode = resolve(root, 'Xcode.app'), platform = resolve(xcode, `Contents/Developer/Platforms/${folder}.platform`);
  const watch = resolve(platform, `Developer/SDKs/${folder}27.1.sdk`);
  // The device SDK shares the iPhone SDK's compiler configuration.
  const phone = resolve(root, 'Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk');
  mkdirSync(phone, { recursive: true }); writeFileSync(resolve(phone, 'SDKSettings.json'), JSON.stringify({ Version: '26.4' }));
  const installed = JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8'));
  installed.targetTriples['arm64-apple-ios'] = { ...installed.targetTriples['arm64-apple-ios-simulator'], sdkRootPath: relative(root, phone) };
  writeFileSync(resolve(root, 'swift-sdk.json'), JSON.stringify(installed));
  const original = JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8'));
  expect(() => installWatchSDK(xcode, name, env)).toThrow(`no ${label} SDK`);
  expect(() => ensureWatchSDK(name, env)).toThrow('without --slim');
  expect(JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8'))).toEqual(original);
  mkdirSync(watch, { recursive: true });
  writeFileSync(resolve(watch, 'SDKSettings.json'), JSON.stringify({ Version: '27.1' }));
  writeFileSync(resolve(watch, 'header.h'), 'int watch_fixture(void);');
  symlinkSync(`${folder}27.1.sdk`, resolve(platform, `Developer/SDKs/${folder}.sdk`));
  writeFileSync(resolve(root, 'OpenAppleMacrosServer'), 'Linux plugin server'); mkdirSync(resolve(root, 'OpenAppleMacrosPlugins'));
  const sdk = ensureWatchSDK(name, env);
  expect(ensureWatchSDK(name, env).sdk).toBe(sdk.sdk);
  expect(sdk.version).toBe('27.1'); expect(readFileSync(resolve(sdk.sdk, 'header.h'), 'utf8')).toBe('int watch_fixture(void);');
  expect(readlinkSync(resolve(sdk.platform, `Developer/SDKs/${folder}.sdk`))).toBe(`${folder}27.1.sdk`);
  expect(realpathSync(resolve(sdk.platform, 'Developer/usr/bin/swift-plugin-server'))).toBe(realpathSync(resolve(root, 'OpenAppleMacrosServer')));
  expect(realpathSync(resolve(sdk.platform, 'Developer/usr/lib/swift/host/plugins'))).toBe(realpathSync(resolve(root, 'OpenAppleMacrosPlugins')));
  const metadata = JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8'));
  for (const phoneKey of ['arm64-apple-ios-simulator', 'arm64-apple-ios']) expect(metadata.targetTriples[phoneKey]).toEqual(original.targetTriples[phoneKey]);
  const cargo = appleCommand('cargo', ['build', '--target', target], { cwd: root, env }, 'linux');
  expect(cargo[2].env.WATCHOS_DEPLOYMENT_TARGET).toBe('26.0'); expect(cargo[2].env.SDKROOT).toBe(sdk.sdk);
  const linker = readFileSync(cargo[2].env[`CARGO_TARGET_${target.toUpperCase().replaceAll('-', '_')}_LINKER`], 'utf8');
  expect(linker).toContain(`arm64-apple-watchos26.0${variant}'`); expect(linker).toContain(`watchos${variant},26.0,27.1`);
  const swift = appleCommand('swift', ['build', '--package-path', root, '--triple', `arm64-apple-watchos26.0${variant}`, '--target', 'ExactCore'], { env }, 'linux');
  const destination = JSON.parse(readFileSync(swift[1].at(-1), 'utf8'));
  expect(destination.targetTriples[key].sdkRootPath).toBe(sdk.sdk);
  const targetOptions = JSON.parse(readFileSync(destination.targetTriples[key].toolsetPaths.at(-1), 'utf8')).swiftCompiler.extraCLIOptions;
  expect(targetOptions).toContain('-external-plugin-path');
  expect(targetOptions).toContain(resolve(sdk.platform, 'Developer/usr/lib/swift/host/plugins') + '#' + resolve(sdk.platform, 'Developer/usr/bin/swift-plugin-server'));
  expect(linker).toContain("'-fuse-ld=lld'"); expect(linker).toContain(`'--ld-path=${resolve(sdk.tools, 'ld64.lld').replaceAll("'", "'\\''")}'`);
  const compiler = appleCommand('xcrun', ['--sdk', name, 'swiftc', '-target', `arm64-apple-watchos26.0${variant}`, '-emit-library', 'Views.swift'], { env }, 'linux');
  expect(compiler[1]).toContain(sdk.runtime); expect(sdk.runtime).toEndWith(`libclang_rt.${runtime}.a`);
  writeFileSync(resolve(watch, 'header.h'), 'int updated_watch_fixture(void);'); installWatchSDK(xcode, name, env);
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

test('Cargo keeps the linker in the build directory when one is set, so every unit there has one configuration', () => fixture(({ root, env }) => {
  const linker = (args, extra = {}) => appleCommand('cargo', args, { cwd: root, env: { ...env, IPHONEOS_DEPLOYMENT_TARGET: '18.2', ...extra } }, 'linux')[2].env.CARGO_TARGET_AARCH64_APPLE_IOS_SIM_LINKER;
  const app = linker(['build', '--target', 'aarch64-apple-ios-sim'], { CARGO_TARGET_DIR: resolve(root, 'target') });
  expect(app.startsWith(resolve(root, 'target/exact-cross-tools') + '/')).toBe(true);
  const modules = ['build', '--target', 'aarch64-apple-ios-sim', '--target-dir', resolve(root, 'target/apple-modules')];
  // Without a build directory a --target-dir build keeps its own linker, as its units are its own.
  expect(linker(modules, { CARGO_TARGET_DIR: resolve(root, 'target') }).startsWith(resolve(root, 'target/apple-modules/exact-cross-tools') + '/')).toBe(true);
  // With one, a --target-dir build's linker is the one every other build in that directory uses.
  expect(linker(modules, { CARGO_TARGET_DIR: resolve(root, 'target'), CARGO_BUILD_BUILD_DIR: resolve(root, 'target') })).toBe(app);
  expect(linker(['build', '--target', 'aarch64-apple-ios-sim'], { CARGO_BUILD_BUILD_DIR: 'target' })).toBe(app);
  expect(linker(modules, { CARGO_BUILD_BUILD_DIR: '{workspace-root}/build' }).startsWith(resolve(root, 'target/apple-modules/exact-cross-tools') + '/')).toBe(true);
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
    const captured = spawnSync(...appleCommand('plutil', ['-convert', 'json', '-o', '-', '-'], { encoding: 'utf8', input: before }, 'linux'));
    expect(captured.status).toBe(0);
    expect(captured.stdout).toBe(ok(['-convert', 'json', '-o', '-', file]));
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

// The built test products and simulator tools are fixtures; no simulator or compiler runs here.
async function iosRunsFixture(body) {
  const root = mkdtempSync(resolve(tmpdir(), 'exact-ios-runs-')), udid = root.split('/').pop();
  const savedPath = process.env.PATH, record = resolve(tmpdir(), `exact-touches-${udid}.runner.json`);
  let opening;
  try {
    const pkg = resolve(root, 'package'), derived = resolve(root, 'derived'), products = resolve(derived, 'Build/Products');
    mkdirSync(resolve(pkg, 'tests/ExactKitTests'), { recursive: true });
    mkdirSync(products, { recursive: true });
    writeFileSync(resolve(pkg, 'tests/ExactKitTests/FixtureTests.swift'), `class FixtureTests: XCTestCase {
      func testHosted() { throw missing("scene", "window") }
      func testTouch() { throw missing("touch", "Back") }
    }`);
    writeFileSync(resolve(products, 'built.xctestrun'), '{}');
    writeFileSync(record, JSON.stringify({ port: 1234, token: 'fixture-token' }));
    writeFileSync(resolve(root, 'plutil'), '#!/bin/sh\nif [ "$4" = "-" ]; then cat "$5"; else cp "$5" "$4"; fi\n');
    writeFileSync(resolve(root, 'xcrun'), '#!/bin/sh\nexit 0\n');
    for (const name of ['plutil', 'xcrun']) chmodSync(resolve(root, name), 0o755);
    process.env.PATH = `${root}:${savedPath}`;
    opening = spyOn(await import('./touches.mjs'), 'openTouches');
    await body({ root, opening, options: { pkg, derived, destination: `id=${udid}`, udid, env: {}, canvasGpu: 'gpu', hostSource: 'host.swift', triple: 'arm64-apple-ios-simulator', run: () => {},
      testingDevices: resolve(root, 'XCTestDevices') } });
  } finally {
    opening?.mockRestore();
    process.env.PATH = savedPath;
    rmSync(record, { force: true });
    // A body that fails while it holds the touch lock (touches.mjs `lock`) leaves the lock behind.
    rmSync(resolve(tmpdir(), `exact-touches-${udid}.lock`), { force: true });
    rmSync(root, { recursive: true, force: true });
  }
}

for (const [prior, bootFails, busy] of [['1', false], ['0', false], [null, false], ['1', true], [null, false, true]]) test(`hosted accessibility preserves ${prior ?? 'an absent setting'} when ${busy ? 'the display is busy during cleanup' : bootFails ? 'simulator boot fails' : 'simulator boot succeeds'}`, () => iosRunsFixture(async ({ root, options }) => {
  writeFileSync(resolve(options.pkg, 'tests/ExactKitTests/FixtureTests.swift'), `class FixtureTests: XCTestCase {
    func testHosted() { throw missing("accessibility", "UIKit") }
  }`);
  const setting = resolve(root, 'setting'), eventsFile = resolve(root, 'events');
  if (prior !== null) writeFileSync(setting, `${prior}\n`);
  writeFileSync(eventsFile, '');
  writeFileSync(resolve(root, 'xcrun'), [
    '#!/bin/sh',
    'fixture=${0%/*}',
    'printf "%s\\n" "$5" >> "$fixture/events"',
    'case "$5" in',
    '  read) [ -f "$fixture/booted" ] && cat "$fixture/setting" ;;',
    '  write) if [ "$9" = "true" ]; then echo 1; else echo 0; fi > "$fixture/setting" ;;',
    '  delete) rm -f "$fixture/setting" ;;',
    'esac',
  ].join('\n'));
  const bootFailure = new Error('simulator boot failed'), ran = [];
  let reported, lock;
  if (busy) {
    const touches = await import('./touches.mjs'), acquire = touches.withAccessibilityLock;
    lock = spyOn(touches, 'withAccessibilityLock').mockImplementation((udid, fn) => {
      if (ran.includes('hosted')) throw new Error('display busy');
      return acquire(udid, fn);
    });
  }
  try {
    await iosTestRuns({ ...options, run: (command, args) => {
      if (args[1] === 'bootstatus') {
        if (bootFails) throw bootFailure;
        writeFileSync(resolve(root, 'booted'), '');
      } else if (args[3] === 'defaults') {
        const result = spawnSync(command, args, { encoding: 'utf8' });
        expect(result.status, result.stderr).toBe(0);
      }
    }, runApple: (command, args) => {
      if (command !== 'xcodebuild') return;
      if (args.includes('-only-testing:ExactKitTests/FixtureTests/testHosted')) {
        ran.push('hosted');
        expect(readFileSync(setting, 'utf8').trim()).toBe('1');
      } else if (args.includes('-only-testing:ExactKitTests/Canvas2DGpuStressTests')) ran.push('gpu');
    } });
  } catch (error) { reported = error; }
  finally { lock?.mockRestore(); }
  expect(ran).toEqual(bootFails ? ['gpu'] : ['hosted', 'gpu']);
  expect(reported).toBe(bootFails ? bootFailure : undefined);
  expect(readFileSync(eventsFile, 'utf8').trim().split('\n').filter(Boolean))
    .toEqual(bootFails ? [] : prior === '1' ? ['read'] : ['read', 'write', prior === null ? 'delete' : 'write']);
  expect(existsSync(setting) ? readFileSync(setting, 'utf8').trim() : null).toBe(prior);
}));

for (const kept of [false, true]) test(`hosted touches keep device ownership through the test and ${kept ? 'leave a reused runner' : 'end their own runner'}`, () => iosRunsFixture(async ({ opening, options }) => {
  const events = [];
  let locked = false;
  opening.mockImplementation(async () => {
    if (locked) throw new Error('touches already held');
    locked = true;
    events.push('open');
    return {
      kept, pid: 123,
      async handoff() { events.push('handoff'); await Promise.resolve(); return { port: 1234, token: 'fixture-token' }; },
      async close() { events.push('close'); locked = false; },
      async end() { events.push('end'); locked = false; return null; },
    };
  });
  await iosTestRuns({ ...options, runApple: (command, args, { env } = {}) => {
    if (!args.includes('-only-testing:ExactKitTests/FixtureTests/testTouch')) return;
    events.push('test');
    expect(locked, 'the hosted fixture still owns the device').toBe(true);
    expect(env.TEST_RUNNER_EXACT_TOUCH_PORT).toBe('1234');
    expect(env.TEST_RUNNER_EXACT_TOUCH_TOKEN).toBe('fixture-token');
    expect(args).toContain('NO');
  } });
  expect(events).toEqual(['open', 'handoff', 'test', kept ? 'close' : 'end']);
  expect(locked).toBe(false);
}));

test('the iOS tests build without coverage into the derived data that every test run names', () => iosRunsFixture(async ({ opening, options }) => {
  opening.mockImplementation(async () => ({ kept: false, pid: 123, handoff: () => ({ port: 1234, token: 'fixture-token' }), close: async () => {}, end: async () => null }));
  const builds = [], runs = [];
  await iosTestRuns({ ...options, runApple: (command, args) => {
    const derived = args[args.indexOf('-derivedDataPath') + 1];
    if (args[0] === 'build-for-testing') builds.push({ derived, coverage: args[args.indexOf('-enableCodeCoverage') + 1] });
    else if (args[0] === 'test-without-building') runs.push(derived);
  } });
  expect(builds).toEqual([{ derived: options.derived, coverage: 'NO' }]);
  expect(runs).toEqual(Array(4).fill(options.derived));
}));

for (const live of [null, 'udid', 'name', 'twin']) test(`the iOS tests delete their simulator's leftover clones ${live ? `only while no xcodebuild tests on ${live === 'twin' ? 'a simulator with its name, runtime and device type' : `it, named by its ${live}`}` : 'even when the tests fail'}`, () => iosRunsFixture(async ({ root, options }) => {
  const devices = options.testingDevices, events = resolve(root, 'events'), runtime = 'com.apple.CoreSimulator.SimRuntime.iOS-27-1';
  const device = (n, name, { state = 'Shutdown', runtime: on = runtime, type = 'iPhone-Duo' } = {}) =>
    ({ udid: `00000000-0000-0000-0000-00000000000${n}`, name, state, runtime: on, deviceTypeIdentifier: `com.apple.CoreSimulator.SimDeviceType.${type}` });
  // Deleted: two clones of the simulator, one of whose deletions fails, and one that another run's cleanup deletes
  // first. Kept: a booted clone, and clones of another simulator and of same-named ones on another runtime or of another
  // type
  const left = device(1, 'Clone 2 of Fixture Duo'), stuck = device(2, 'Clone 2 of Fixture Duo'), raced = device(3, 'Clone 2 of Fixture Duo');
  const booted = device(4, 'Clone 1 of Fixture Duo', { state: 'Booted' }), other = device(5, 'Clone 2 of Other Duo');
  const elsewhere = device(6, 'Clone 2 of Fixture Duo', { runtime: 'com.apple.CoreSimulator.SimRuntime.iOS-27-0' });
  const tablet = device(7, 'Clone 2 of Fixture Duo', { type: 'iPad-Pro-13-inch-M5' }), twin = device(8, 'Fixture Duo'), stranger = device(9, 'Other Duo');
  for (const clone of [left, stuck, raced, booted, other, elsewhere, tablet]) mkdirSync(resolve(devices, clone.udid), { recursive: true });
  const list = (listed) => JSON.stringify({ devices: Object.groupBy(listed, d => d.runtime) });
  writeFileSync(resolve(root, 'simulators.json'), list([{ ...device(0, 'Fixture Duo', { state: 'Booted' }), udid: options.udid }, twin, stranger]));
  writeFileSync(resolve(root, 'clones.json'), list([left, stuck, raced, booted, other, elsewhere, tablet]));
  const xcodebuild = udid => `/Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild test-without-building -destination platform=iOS Simulator,id=${udid}`;
  // Another simulator's test, a build, a test without parallel testing and processes that are not xcodebuild leave the
  // clones to be deleted
  const quiet = [xcodebuild(stranger.udid), `xcodebuild build-for-testing -destination platform=iOS Simulator,id=${options.udid}`,
    `${xcodebuild(options.udid)} -parallel-testing-enabled NO`, `xcrun simctl spawn ${options.udid} log stream`, `make test SIMULATOR=${options.udid}`];
  const running = { udid: `${xcodebuild(options.udid)} -parallel-testing-enabled YES -parallel-testing-worker-count 2`, name: `timeout 2400 xcodebuild test -destination platform=iOS Simulator,name=Fixture Duo`, twin: xcodebuild(twin.udid) };
  writeFileSync(resolve(root, 'processes'), [...quiet, ...(live ? [running[live]] : [])].join('\n'));
  writeFileSync(events, '');
  writeFileSync(resolve(root, 'ps'), '#!/bin/sh\ncat "${0%/*}/processes"\n');
  chmodSync(resolve(root, 'ps'), 0o755);
  writeFileSync(resolve(root, 'xcrun'), [
    '#!/bin/sh',
    'fixture=${0%/*}',
    'if [ "$2" = "--set" ]; then',
    '  case "$4" in',
    '    list) cat "$fixture/clones.json" ;;',
    '    delete) echo "$5" >> "$fixture/events"',
    `      case "$5" in ${stuck.udid}) echo "Unable to delete" >&2; exit 1 ;; ${raced.udid}) rm -r "$3/$5"; echo "Invalid device" >&2; exit 1 ;; esac ;;`,
    '  esac',
    'elif [ "$2" = "list" ]; then cat "$fixture/simulators.json"; fi',
  ].join('\n'));
  const warned = spyOn(console, 'warn').mockImplementation(() => {}), failure = new Error('tests failed');
  try {
    await expect(iosTestRuns({ ...options, only: ['BareTests'], runApple: (command, args) => {
      if (args[0] === 'test-without-building') throw failure;
    } })).rejects.toBe(failure);
    expect(readFileSync(events, 'utf8').trim().split('\n').filter(Boolean)).toEqual(live ? [] : [left, stuck, raced].map(clone => clone.udid));
    expect(warned.mock.calls.flat()).toEqual(live ? [] : [`host/apple: leftover clones of the iOS tests' simulator may remain in ${devices}: \`xcrun simctl delete\` failed for ${stuck.udid} (Unable to delete). List them with \`xcrun simctl --set ${devices} list devices\` and delete one with \`xcrun simctl --set ${devices} delete <udid>\`, or the next iOS test run on this simulator tries again`]);
  } finally {
    warned.mockRestore();
  }
}));

for (const [phase, kept] of [['setup', false], ['handoff', false], ['test and cleanup', false], ['test and cleanup', true]]) test(`the GPU tests still run after a ${kept ? 'reused' : 'new'} touch runner's ${phase} failure`, () => iosRunsFixture(async ({ opening, options }) => {
  const failure = new Error(`touch ${phase}`), cleanupFailure = new Error('touch cleanup'), ran = [];
  let cleaned = false;
  opening.mockImplementation(async () => {
    if (phase === 'setup') throw failure;
    return {
      kept, pid: 123,
      handoff() { if (phase === 'handoff') throw failure; return { port: 1234, token: 'fixture-token' }; },
      async close() { cleaned = true; if (phase === 'test and cleanup') throw cleanupFailure; },
      async end() { cleaned = true; return phase === 'test and cleanup' ? cleanupFailure : null; },
    };
  });
  let reported;
  try {
    await iosTestRuns({ ...options, runApple: (command, args) => {
      if (args[0] !== 'test-without-building') return;
      const selected = args.filter(a => a.startsWith('-only-testing:'));
      const suite = selected.some(a => a.includes('Canvas2DGpuStressTests')) ? 'gpu'
        : selected.some(a => a.endsWith('/testTouch')) ? 'touch'
          : selected.some(a => a.endsWith('/testHosted')) ? 'hosted' : 'bare';
      ran.push(suite);
      if (suite === 'touch' && phase === 'test and cleanup') throw failure;
    } });
  } catch (error) { reported = error; }
  expect(ran).toEqual(['bare', 'hosted', ...(phase === 'test and cleanup' ? ['touch'] : []), 'gpu']);
  expect(cleaned).toBe(phase !== 'setup');
  if (phase === 'test and cleanup') expect(reported.errors).toEqual([failure, cleanupFailure]);
  else expect(reported).toBe(failure);
}));

test('EXACT_EXPERIMENTAL=1 alone turns on experimental features, and only a development archive compiles Swift unoptimized', () => {
  const development = ['bun', 'host/apple/build.mjs', '--device', '--archive', 'a.ipa', '--unsigned', '--development'];
  for (const value of [undefined, '', '0', 'true', 'yes', ' 1']) {
    expect(experimental({ EXACT_EXPERIMENTAL: value })).toBe(false);
    expect(swiftOptimization({ EXACT_EXPERIMENTAL: value }, development)).toBe('-O');
    expect(swiftOptimizationKey({ EXACT_EXPERIMENTAL: value }, development)).toEqual([]);
  }
  const on = { EXACT_EXPERIMENTAL: '1' };
  expect(experimental(on)).toBe(true);
  expect(swiftOptimization(on, development)).toBe('-Onone');
  expect(swiftOptimizationKey(on, development)).toEqual(['-Onone']);
  for (const args of [['bun', 'host/apple/build.mjs', '--ios', '--run'], ['bun', 'host/apple/build.mjs', '--device', '--archive', 'a.ipa'], ['bun', 'host/apple/build.mjs', '--release']]) {
    expect(swiftOptimization(on, args)).toBe('-O');
    expect(swiftOptimizationKey(on, args)).toEqual([]);
  }
  // A distributed development archive bakes production trust, which build.mjs sets: it compiles optimized.
  expect(swiftOptimization(on, [...development, '--distribution'])).toBe('-O');
  expect(swiftOptimization({ ...on, EXACT_UPDATE_TRUST: 'production' }, development)).toBe('-O');
  expect(swiftOptimizationKey({ ...on, EXACT_UPDATE_TRUST: 'production' }, development)).toEqual([]);
});

test('the widget reload dylib compiles and is kept as before without EXACT_EXPERIMENTAL, and unoptimized beside it with it in a development archive', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-experimental-'));
  const was = { flag: process.env.EXACT_EXPERIMENTAL, argv: process.argv };
  try {
    const source = resolve(dir, 'WidgetContainer.swift');
    writeFileSync(source, 'let reload = 1\n');
    const reload = () => {
      const compiles = [];
      const runApple = (tool, args) => { compiles.push(args); const out = args[args.indexOf('-o') + 1]; writeFileSync(out, 'dylib'); };
      const binary = widgetReload({ directory: dir, triple: 'arm64-apple-ios17.0', sdk: '/sdk', device: true, production: false, runApple, source });
      return { binary, compiles };
    };
    process.argv = [...was.argv, '--development'];
    delete process.env.EXACT_EXPERIMENTAL;
    const plain = reload();
    // The digest as it was computed before experimental features existed: its source and [triple, sdk, production].
    const digest = createHash('sha256').update(readFileSync(source)).update(JSON.stringify(['arm64-apple-ios17.0', '/sdk', false])).digest('hex').slice(0, 16);
    expect(plain.binary).toBe(resolve(dir, `widget-reload-${digest}`, 'libexact_widget_reload.dylib'));
    expect(plain.compiles).toHaveLength(1);
    expect(plain.compiles[0]).toContain('-O');
    expect(plain.compiles[0]).not.toContain('-Onone');
    process.env.EXACT_EXPERIMENTAL = '1';
    const unoptimized = reload();
    expect(unoptimized.binary).not.toBe(plain.binary);
    expect(unoptimized.compiles).toHaveLength(1);
    expect(unoptimized.compiles[0]).toContain('-Onone');
    expect(unoptimized.compiles[0]).not.toContain('-O');
    process.argv = was.argv;
    expect(reload()).toEqual({ binary: plain.binary, compiles: [] });
  } finally {
    process.argv = was.argv;
    if (was.flag === undefined) delete process.env.EXACT_EXPERIMENTAL; else process.env.EXACT_EXPERIMENTAL = was.flag;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('EXACT_EXPERIMENTAL=1 links every Swift capability into a development archive, and nothing else changes', () => {
  const graph = { uses: ['markdown', 'picker'] }, baked = { inputs: { linked: ['markdown'] } };
  const was = process.env.EXACT_EXPERIMENTAL;
  try {
    delete process.env.EXACT_EXPERIMENTAL;
    const byUse = appleComposition(graph, false);
    expect(byUse.products.includes('ExactTips')).toBe(false);
    expect(appleComposition(graph, false, [], null, true)).toEqual(byUse);
    process.env.EXACT_EXPERIMENTAL = '1';
    expect(appleComposition(graph, false, [], null, true)).toEqual(appleComposition(undefined, false));
    expect(appleComposition(graph, false, [], null, true).link).toBe('all');
    expect(appleComposition(graph, false)).toEqual(byUse);
    expect(appleComposition(graph, true, [], baked)).toEqual({ link: 'markdown', products: ['ExactMarkdown'] });
  } finally {
    if (was === undefined) delete process.env.EXACT_EXPERIMENTAL; else process.env.EXACT_EXPERIMENTAL = was;
  }
});
