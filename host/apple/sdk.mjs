// SDK additions for the Apple targets Exact builds beyond xtool's default iOS and macOS SDKs.
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { crossSdk } from './toolchain.mjs';
import { plistCommand } from './plist.mjs';
import { spawnSync } from 'node:child_process';

export { iosSimulatorTarget } from './link.mjs';
/** Whether the Darwin cross SDK has `name` (iphoneos, iphonesimulator, …) configured. */
export const hasCrossSdk = (name, env = process.env) => { try { crossSdk(name, env); return true; } catch { return false; } };

export const CROSS_IOS_ROWS = ['Apple cross SDK', 'Swift for Apple cross compilation', 'LLVM Apple tools', 'Apple codesign'];

/** Linux's build prerequisites; checking them never installs or changes the SDK. */
export function appleCrossReport(env = process.env, execute = spawnSync) {
  const row = (name, have, want, ok, fix) => ({ name, have: have || 'missing', want, ok, fix, required: false, need: 'iOS on Linux/WSL' });
  const output = (command, args) => {
    const result = execute(command, args, { env, encoding: 'utf8' });
    return result.status === 0 ? result.stdout.trim() : '';
  };
  let sdk = '', valid = false;
  try {
    const installed = ['iphoneos', 'iphonesimulator'].map(name => crossSdk(name, env));
    sdk = installed.map(sdk => sdk.version).join(' / ');
    valid = installed.every(sdk => Number.parseFloat(sdk.version) >= 27.1 && Number.parseInt(sdk.version) === 27 &&
      [sdk.runtime, resolve(sdk.tools, 'ld64.lld'), resolve(sdk.tools, 'llvm-lib')].every(existsSync));
  } catch (error) { sdk = error.message; }
  const swift = /Swift version ([\d.]+)/.exec(output('swift', ['--version']))?.[1];
  const llvm = env.EXACT_APPLE_LLVM_BIN ? name => resolve(env.EXACT_APPLE_LLVM_BIN, name) : name => name;
  const version = /LLVM version (\d+)/i.exec(output(llvm('llvm-lipo'), ['--version']))?.[1];
  const missing = ['dsymutil', 'llvm-ar', 'llvm-otool', 'llvm-nm', 'llvm-size', 'llvm-strip', 'llvm-install-name-tool']
    .filter(tool => !output(llvm(tool), ['--version']));
  const signer = /apple-codesign ([\d.]+)/.exec(output(env.EXACT_APPLE_CODESIGN ?? 'rcodesign', ['--version']))?.[1];
  return [
    row(CROSS_IOS_ROWS[0], sdk, 'Xcode 27.1 Darwin SDK', valid, 'xtool sdk install --slim /path/to/Xcode_27.1.xip'),
    row(CROSS_IOS_ROWS[1], swift, 'Swift 6.4', /^6\.4\./.test(swift ?? ''), 'install Swift 6.4.2 from https://swift.org/install/linux'),
    row(CROSS_IOS_ROWS[2], missing.length ? `missing: ${missing.join(', ')}` : version, 'LLVM 23 or newer', Number(version) >= 23 && !missing.length,
      'install LLVM 23 from https://apt.llvm.org; put its bin directory on PATH and set EXACT_APPLE_LLVM_BIN to that directory'),
    row(CROSS_IOS_ROWS[3], signer, 'rcodesign 0.29.0', signer === '0.29.0', 'install apple-codesign 0.29.0 from https://github.com/indygreg/apple-platform-rs/releases'),
  ];
}

/** Install the watch simulator SDK from the same Xcode input used for `xtool sdk install`. */
export function installWatchSimulatorSDK(xcode, env = process.env) {
  const base = crossSdk('iphonesimulator', env), source = resolve(xcode, 'Contents/Developer/Platforms/WatchSimulator.platform');
  const sdks = resolve(source, 'Developer/SDKs');
  if (!existsSync(sdks)) throw new Error(`host/apple: ${xcode} has no watch simulator SDK`);
  const choices = readdirSync(sdks, { withFileTypes: true }).filter(entry => entry.isDirectory() && entry.name.endsWith('.sdk'))
    .map(entry => ({ name: entry.name, settings: JSON.parse(readFileSync(resolve(sdks, entry.name, 'SDKSettings.json'), 'utf8')) }))
    .sort((a, b) => String(b.settings.Version).localeCompare(String(a.settings.Version), 'en', { numeric: true }));
  const selected = choices[0];
  if (!selected) throw new Error(`host/apple: ${sdks} has no versioned SDK directory`);
  const target = resolve(base.root, 'Developer/Platforms/WatchSimulator.platform');
  const metadataPath = resolve(base.root, 'swift-sdk.json'), metadata = JSON.parse(readFileSync(metadataPath, 'utf8'));
  const stage = mkdtempSync(resolve(base.root, '.exact-watch-sdk-')), staged = resolve(stage, 'WatchSimulator.platform');
  const backup = resolve(stage, 'previous'), temporary = resolve(stage, 'swift-sdk.json');
  let replaced = false, published = false;
  try {
    cpSync(source, staged, { recursive: true, verbatimSymlinks: true, preserveTimestamps: true });
    for (const [name, path] of [['Developer/usr/bin/swift-plugin-server', 'OpenAppleMacrosServer'], ['Developer/usr/lib/swift/host/plugins', 'OpenAppleMacrosPlugins']]) {
      const copy = resolve(staged, name), final = resolve(target, name);
      mkdirSync(dirname(copy), { recursive: true }); rmSync(copy, { recursive: true, force: true });
      symlinkSync(relative(dirname(final), resolve(base.root, path)), copy);
    }
    const developer = resolve(target, 'Developer');
    metadata.targetTriples['arm64-apple-watchos-simulator'] = { ...base.definition,
      sdkRootPath: relative(base.root, resolve(developer, 'SDKs', selected.name)),
      includeSearchPaths: [relative(base.root, resolve(developer, 'usr/lib'))],
      librarySearchPaths: [relative(base.root, resolve(developer, 'usr/lib'))],
    };
    writeFileSync(temporary, JSON.stringify(metadata));
    mkdirSync(dirname(target), { recursive: true });
    if (existsSync(target)) { renameSync(target, backup); replaced = true; }
    renameSync(staged, target); published = true;
    renameSync(temporary, metadataPath);
  } catch (error) {
    if (published) rmSync(target, { recursive: true, force: true });
    if (replaced) renameSync(backup, target);
    throw error;
  } finally { rmSync(stage, { recursive: true, force: true }); }
  return crossSdk('watchsimulator', env);
}

/** A full xtool installation retains Xcode.app; copy its watch SDK only when a watch build needs it. */
export function ensureWatchSimulatorSDK(env = process.env) {
  try { return crossSdk('watchsimulator', env); } catch {
    const base = crossSdk('iphonesimulator', env), xcode = resolve(base.root, 'Xcode.app');
    if (!existsSync(resolve(xcode, 'Contents/Developer/Platforms/WatchSimulator.platform/Developer/SDKs')))
      throw new Error('host/apple: watch companions need the retained WatchSimulator SDK; reinstall with `xtool sdk install /path/to/Xcode_27.1.xip` without --slim');
    return installWatchSimulatorSDK(xcode, env);
  }
}

/** Distribution metadata describes the SDK actually used. Linux has no Xcode or macOS build to stamp. */
export function distributionToolchain(name, env = process.env, os = process.platform) {
  const read = (command, args) => {
    const result = spawnSync(command, args, { env, encoding: 'utf8' });
    if (result.status !== 0) throw new Error(`host/apple: ${command} failed: ${result.stderr || result.error?.message}`);
    return result.stdout.trim();
  };
  let version, build, native = {};
  if (os === 'linux') {
    const sdk = crossSdk(name, env);
    const system = JSON.parse(read(...plistCommand(['-convert', 'json', '-o', '-', resolve(sdk.sdk, 'System/Library/CoreServices/SystemVersion.plist')])));
    version = sdk.version; build = system.ProductBuildVersion;
    if (typeof build !== 'string' || !build) throw new Error(`host/apple: ${name} SDK has no ProductBuildVersion`);
  } else {
    const xcode = read('xcodebuild', ['-version']);
    const [major, minor = '0', patch = '0'] = (/Xcode (\d+)(?:\.(\d+))?(?:\.(\d+))?/.exec(xcode) ?? []).slice(1);
    if (!major) throw new Error('host/apple: cannot read the selected Xcode version');
    version = read('xcrun', ['--sdk', name, '--show-sdk-version']);
    build = read('xcrun', ['--sdk', name, '--show-sdk-build-version']);
    native = { DTXcode: `${major.padStart(2, '0')}${minor}${patch}`, DTXcodeBuild: /Build version (\S+)/.exec(xcode)?.[1] ?? '',
      BuildMachineOSBuild: read('sw_vers', ['-buildVersion']) };
  }
  return { DTCompiler: 'com.apple.compilers.llvm.clang.1_0', DTPlatformBuild: build, DTPlatformVersion: version,
    DTSDKBuild: build, DTSDKName: `${name}${version}`, ...native };
}
