// Apple compiler invocations on Linux use xtool's installed Darwin Swift SDK.
import { spawn as systemSpawn, spawnSync as systemSpawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, renameSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { homedir, tmpdir } from 'node:os';
import { basename, dirname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { plistCommand } from './plist.mjs';
import { windowsMetalCommand } from './metal.mjs';
import { resolveAppleLLVM, writtenPath } from '../../scripts/paths.mjs';

export function appleLLVMTool(name, env = process.env) {
  const bin = env.EXACT_APPLE_LLVM_BIN;
  if (!bin) return name;
  const dir = writtenPath(bin);
  return dir + (dir.endsWith('/') || dir.endsWith(sep) ? '' : '/') + name;
}

/** `EXACT_EXPERIMENTAL=1` turns on the Apple build's experimental features; each says where it applies. With any other
 *  value, or unset, a build is byte for byte the one it is without them. */
export const experimental = (env = process.env) => env.EXACT_EXPERIMENTAL === '1';
/** The optimization the Swift a build compiles itself (app modules, packages, extensions) takes. Experimental: `-Onone`
 *  in a `--development` archive, a build to iterate on rather than ship, since an optimized whole-module compile runs
 *  mostly on one core and takes minutes for some packages and extensions. Otherwise `-O`, and so for what bakes
 *  production trust, which build.mjs sets for every distributed build (`--distribution` with `--development` too).
 *  Intents stay `-O`: on Linux their cache key hashes intents.mjs's own source, so any edit there invalidates it. */
export const swiftOptimization = (env = process.env, args = process.argv) =>
  experimental(env) && args.includes('--development') && !args.includes('--distribution') && env.EXACT_UPDATE_TRUST !== 'production' ? '-Onone' : '-O';
/** `swiftOptimization()` as part of a cache key: empty for `-O`, so a key without the experiment is unchanged. */
export const swiftOptimizationKey = (env = process.env, args = process.argv) => swiftOptimization(env, args) === '-O' ? [] : [swiftOptimization(env, args)];

/** The watch app's deployment target: arm64 runs from watchOS 26, and the watch app is built for arm64 alone (LLP 1126.002 D7). */
export const WATCH_MINIMUM_OS = '26.0';
const SDK_KEYS = { iphoneos: 'arm64-apple-ios', iphonesimulator: 'arm64-apple-ios-simulator', macosx: 'arm64-apple-macosx', watchos: 'arm64-apple-watchos', watchsimulator: 'arm64-apple-watchos-simulator' };

export function crossSdk(name, env = process.env) {
  if (!Object.hasOwn(SDK_KEYS, name)) throw new Error(`host/apple: no cross SDK mapping for ${name}`);
  const config = env.XDG_CONFIG_HOME ? resolve(env.XDG_CONFIG_HOME, 'swiftpm') : resolve(env.HOME ?? homedir(), '.swiftpm');
  const root = resolve(env.EXACT_APPLE_SDK ?? resolve(config, 'swift-sdks/darwin.artifactbundle'));
  let definitions;
  try { definitions = JSON.parse(readFileSync(resolve(root, 'swift-sdk.json'), 'utf8')).targetTriples; }
  catch (e) { throw new Error(`host/apple: cannot read the Darwin Swift SDK at ${root}: ${e.message}. Install it with xtool sdk install <Xcode.xip>.`); }
  const definition = definitions?.[SDK_KEYS[name]];
  if (!definition?.sdkRootPath || !definition.swiftResourcesPath) throw new Error(`host/apple: ${root} has no ${name} compiler configuration`);
  const sdk = resolve(root, definition.sdkRootPath);
  const settings = JSON.parse(readFileSync(resolve(sdk, 'SDKSettings.json'), 'utf8'));
  const resources = resolve(root, definition.swiftResourcesPath), runtime = { iphoneos: 'ios', iphonesimulator: 'iossim', macosx: 'osx', watchos: 'watchos', watchsimulator: 'watchossim' }[name];
  return { root, sdk, version: settings.Version, resources, runtime: resolve(resources, `clang/lib/darwin/libclang_rt.${runtime}.a`),
    tools: resolve(root, 'toolset/bin'), platform: resolve(sdk, '../../..'), definition };
}

const valueAfter = (args, name) => args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const withoutOption = (args, name) => args.filter((_, i) => args[i] !== name && args[i - 1] !== name);
const sdkForTriple = triple => /-apple-watchos[\d.]*(?:-simulator)?$/.test(triple ?? '') ? triple.endsWith('-simulator') ? 'watchsimulator' : 'watchos' : /-apple-ios[\d.]*(?:-simulator)?$/.test(triple ?? '')
  ? triple.endsWith('-simulator') ? 'iphonesimulator' : 'iphoneos' : /-apple-macosx[\d.]*$/.test(triple ?? '') ? 'macosx' : null;
const sdkKey = (name, triple) => SDK_KEYS[name].replace(/^arm64/, triple.startsWith('x86_64-') ? 'x86_64' : 'arm64');

function platformVersion(sdk, triple) {
  const match = /-apple-(ios|macosx|watchos)([\d.]+)(-simulator)?$/.exec(triple ?? '');
  return match ? ['-platform_version', match[1] === 'macosx' ? 'macos' : `${match[1]}${match[3] ?? ''}`, match[2], sdk.version] : [];
}

const linkerDriver = sdk => ['-fuse-ld=lld', `--ld-path=${resolve(sdk.tools, 'ld64.lld')}`];
const macroServer = sdk => `${resolve(sdk.platform, 'Developer/usr/lib/swift/host/plugins')}#${resolve(sdk.platform, 'Developer/usr/bin/swift-plugin-server')}`;

function cachedJson(directory, metadata) {
  const contents = JSON.stringify(metadata);
  const file = resolve(directory, `${createHash('sha256').update(contents).digest('hex')}.json`);
  mkdirSync(directory, { recursive: true });
  if (!existsSync(file) || readFileSync(file, 'utf8') !== contents) {
    const temporary = `${file}.${process.pid}.tmp`;
    writeFileSync(temporary, contents); renameSync(temporary, file);
  }
  return file;
}

function swiftDestination(sdk, name, triple, args, options) {
  const definition = { ...sdk.definition };
  for (const key of ['sdkRootPath', 'swiftResourcesPath', 'swiftStaticResourcesPath']) {
    if (definition[key]) definition[key] = resolve(sdk.root, definition[key]);
  }
  for (const key of ['includeSearchPaths', 'librarySearchPaths', 'toolsetPaths']) {
    if (definition[key]) definition[key] = definition[key].map(path => resolve(sdk.root, path));
  }
  // SwiftPM's installed-bundle selector requires an exact, unversioned triple.
  // A single-target destination lets --triple retain the app's minimum OS.
  const cwd = options.cwd ?? process.cwd(), pkg = resolve(cwd, valueAfter(args, '--package-path') ?? '.');
  const scratch = valueAfter(args, '--scratch-path');
  const directory = resolve(scratch ? resolve(cwd, scratch) : resolve(pkg, '.build'), 'exact-cross-sdk');
  // SwiftPM's native backend chooses libtool's -static syntax by basename.
  // xtool calls the same executable llvm-lib for its Swift Build backend.
  mkdirSync(directory, { recursive: true });
  const source = resolve(sdk.tools, 'llvm-lib');
  const librarian = resolve(directory, `${createHash('sha256').update(source).digest('hex')}-libtool`);
  try { symlinkSync(source, librarian); } catch (e) { if (e.code !== 'EEXIST') throw e; }
  if (readlinkSync(librarian) !== source) throw new Error(`host/apple: unexpected cross librarian at ${librarian}`);
  // SDK toolset options apply to the target, unlike global -Xswiftc/-Xlinker
  // options which also reach SwiftPM's Linux-hosted macros and plugins.
  // Linux's Clang has no Darwin compiler runtime (including availability checks).
  const linkerOptions = [...linkerDriver(sdk).flatMap(value => ['-Xclang-linker', value]), '-Xlinker', sdk.runtime,
    '-external-plugin-path', macroServer(sdk),
    ...(args.includes('-platform_version') ? [] : platformVersion(sdk, triple).flatMap(value => ['-Xlinker', value]))];
  definition.toolsetPaths = [...(definition.toolsetPaths ?? []), cachedJson(directory, {
    schemaVersion: '1.0', rootPath: sdk.tools, librarian: { path: librarian }, swiftCompiler: { extraCLIOptions: linkerOptions },
  })];
  return cachedJson(directory, { schemaVersion: '4.0', targetTriples: { [sdkKey(name, triple)]: definition } });
}

function cargoEnvironment(args, options, env) {
  const end = args.indexOf('--'), buildArgs = end < 0 ? args : args.slice(0, end);
  const targets = buildArgs.flatMap((arg, index) => arg === '--target' ? [buildArgs[index + 1]] : arg.startsWith('--target=') ? [arg.slice(9)] : []);
  const apple = targets.filter(target => /^(aarch64|x86_64)-apple-(ios(-sim)?|watchos(-sim)?|darwin)$/.test(target ?? ''));
  if (!apple.length) return options;
  const environment = { ...env };
  resolveAppleLLVM(environment);
  let selectedSdk;
  for (const target of apple) {
    const mac = target.endsWith('-darwin'), watch = /-watchos(-sim)?$/.test(target), sim = target.endsWith('-sim') || target === 'x86_64-apple-ios';
    const name = mac ? 'macosx' : watch ? (sim ? 'watchsimulator' : 'watchos') : sim ? 'iphonesimulator' : 'iphoneos';
    const sdk = crossSdk(name, env), minimum = mac ? 'MACOSX_DEPLOYMENT_TARGET' : watch ? 'WATCHOS_DEPLOYMENT_TARGET' : 'IPHONEOS_DEPLOYMENT_TARGET';
    if (selectedSdk && selectedSdk !== sdk.sdk) throw new Error('host/apple: build different Apple SDK targets in separate Cargo invocations');
    selectedSdk = sdk.sdk;
    const version = env[minimum] ?? (mac ? '14.0' : watch ? WATCH_MINIMUM_OS : '17.0');
    if (!/^\d+(\.\d+){0,2}$/.test(version)) throw new Error(`host/apple: invalid ${minimum}: ${version}`);
    const triple = `${target.startsWith('aarch64-') ? 'arm64' : 'x86_64'}-apple-${mac ? 'macosx' : watch ? 'watchos' : 'ios'}${version}${sim ? '-simulator' : ''}`;
    const linkerArgs = ['clang', `--target=${triple}`, '-isysroot', sdk.sdk, '-Xlinker', sdk.runtime,
      ...linkerDriver(sdk), `-Wl,${platformVersion(sdk, triple).join(',')}`];
    const quote = value => `'${value.replaceAll("'", "'\\''")}'`;
    const script = `#!/bin/sh\nexec ${linkerArgs.map(quote).join(' ')} "$@"\n`;
    // The linker's path is in Cargo's configuration hash of every unit it builds, so it lives where those units do:
    // Cargo's build directory when CARGO_BUILD_BUILD_DIR sets one, else the target directory. A build with its own
    // --target-dir and a shared build directory (the host's module build into target/apple-modules, say) otherwise
    // wrote the units it shares with every other build there (libc, bitflags, objc2, ...) under another hash, and each
    // rebuilt them after the other. A templated build directory ({workspace-root}) keeps the old place.
    const buildDir = env.CARGO_BUILD_BUILD_DIR && !env.CARGO_BUILD_BUILD_DIR.includes('{') ? env.CARGO_BUILD_BUILD_DIR : null;
    const directory = resolve(options.cwd ?? process.cwd(), buildDir ?? valueAfter(buildArgs, '--target-dir') ?? env.CARGO_TARGET_DIR ?? 'target', 'exact-cross-tools');
    const file = resolve(directory, `${createHash('sha256').update(script).digest('hex')}-linker`);
    mkdirSync(directory, { recursive: true });
    if (!existsSync(file) || readFileSync(file, 'utf8') !== script) {
      const temporary = `${file}.${process.pid}.tmp`;
      writeFileSync(temporary, script, { mode: 0o755 }); renameSync(temporary, file);
    }
    chmodSync(file, 0o755);
    environment[`CARGO_TARGET_${target.toUpperCase().replaceAll('-', '_')}_LINKER`] ??= file;
    const ccTarget = target.replaceAll('-', '_');
    environment[`CC_${ccTarget}`] ??= 'clang';
    environment[`CXX_${ccTarget}`] ??= 'clang++';
    environment[`AR_${ccTarget}`] ??= 'llvm-ar';
    environment.SDKROOT ??= sdk.sdk;
    environment[minimum] ??= version;
    if (env.EXACT_APPLE_METAL_TOOLS) {
      // Cargo build scripts invoke xcrun themselves; the JS spawn adapter cannot intercept those calls.
      const script = `#!/bin/sh\nexec ${[process.execPath, fileURLToPath(import.meta.url)].map(quote).join(' ')} "$@"\n`;
      const tools = resolve(directory, createHash('sha256').update(script).digest('hex')), xcrun = resolve(tools, 'xcrun');
      mkdirSync(tools, { recursive: true });
      if (!existsSync(xcrun) || readFileSync(xcrun, 'utf8') !== script) {
        const temporary = `${xcrun}.${process.pid}.tmp`;
        writeFileSync(temporary, script, { mode: 0o755 }); renameSync(temporary, xcrun);
      }
      chmodSync(xcrun, 0o755); environment.PATH = `${tools}:${env.PATH ?? process.env.PATH ?? ''}`;
    }
  }
  return { ...options, env: environment };
}

function portableSigning(args, env) {
  const values = {}, flags = new Set();
  let path;
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (['--sign', '--identifier', '--entitlements', '--options'].includes(arg)) {
      if (args[i + 1] == null) throw new Error(`host/apple: codesign ${arg} needs a value`);
      values[arg] = args[++i];
    } else if (['--force', '--timestamp=none', '--timestamp', '--deep'].includes(arg)) flags.add(arg);
    else if (arg.startsWith('-') || path) throw new Error(`host/apple: unsupported portable codesign argument ${arg}`);
    else path = arg;
  }
  const identity = values['--sign'], signing = [];
  if (identity !== '-') {
    if (!identity || !existsSync(identity)) throw new Error('host/apple: Linux EXACT_IDENTITY must name a PKCS#12 file; a macOS Keychain identity cannot be used here');
    const password = env.EXACT_APPLE_SIGN_PASSWORD_FILE;
    if (!password || !existsSync(password)) throw new Error('host/apple: PKCS#12 signing needs EXACT_APPLE_SIGN_PASSWORD_FILE');
    signing.push('--p12-file', resolve(identity), '--p12-password-file', resolve(password));
  }
  if (!path) throw new Error('host/apple: codesign needs a path');
  return [env.EXACT_APPLE_CODESIGN ?? 'rcodesign', ['sign', '--config-file', '/dev/null', '--timestamp-url', 'none', ...signing,
    ...(flags.has('--deep') ? [] : ['--shallow']),
    ...(values['--identifier'] ? ['--binary-identifier', values['--identifier']] : []),
    ...(values['--entitlements'] ? ['--entitlements-xml-file', `main:${values['--entitlements']}`] : []),
    ...(values['--options'] ?? '').split(',').filter(Boolean).flatMap(flag => ['--code-signature-flags', flag]), path]];
}

/** Return an executable and argv, never a shell command. Mac Swift uses the Xcode selection the build validates. */
export function appleCommand(command, args, { macDevelopmentSign, ...options } = {}, platform = process.platform) {
  if (platform === 'darwin' && (command === 'swift' || command === 'swiftc')) return ['xcrun', [command, ...args], options];
  if (platform === 'darwin' && command === 'codesign' && args.includes('--sign') && args.includes('--timestamp=none') && valueAfter(args, '--sign') !== '-') {
    options = { timeout: 30_000, ...options, killSignal: 'SIGKILL' };
  }
  if (platform !== 'linux') return [command, args, options];
  const env = options.env ?? process.env;
  if (command === 'cargo') return [command, args, cargoEnvironment(args, options, env)];
  if (command === 'ditto') {
    if (args.length !== 6 || ['-c', '-k', '--sequesterRsrc', '--keepParent'].some((flag, i) => args[i] !== flag)) throw new Error('host/apple: unsupported portable archive command');
    const cwd = options.cwd ?? process.cwd(), source = resolve(cwd, args[4]), output = resolve(cwd, args[5]);
    if (basename(source) !== 'Payload') throw new Error('host/apple: portable IPA archives require a Payload directory');
    return ['zip', ['-q', '-r', '-y', output, 'Payload'], { ...options, cwd: dirname(source) }];
  }
  if (command === 'codesign' && args.includes('--sign')) return [...portableSigning(args, env), options];
  if (command === 'plutil') return [...plistCommand(args), options];
  if (command === 'swift' && args[0] === 'build') {
    const triple = valueAfter(args, '--triple'), name = sdkForTriple(triple);
    if (!name) return [command, args, options];
    const sdk = crossSdk(name, env);
    const build = withoutOption(withoutOption(args, '--sdk'), '--build-system');
    const buildEnv = { ...env }; delete buildEnv.SDKROOT;
    buildEnv.PATH = `${sdk.tools}:${env.PATH ?? process.env.PATH ?? ''}`;
    return [command, [...build, '--build-system', 'native', '--destination', swiftDestination(sdk, name, triple, args, options)], { ...options, env: buildEnv }];
  }
  if (command === 'xcrun') {
    args = [...args];
    // Only xcrun's own leading `--sdk` names its SDK; a later `-sdk` is the tool's argument.
    const flag = ['--sdk', '-sdk'].includes(args[0]) ? args[0] : null;
    const name = flag ? args[1] : sdkForTriple(valueAfter(args, '-target')) ?? 'macosx';
    if (flag) args = args.slice(2);
    if (args[0] === '--show-sdk-path' || args[0] === '--show-sdk-version') {
      const sdk = crossSdk(name, env), value = args[0] === '--show-sdk-path' ? sdk.sdk : sdk.version;
      if (!value) throw new Error(`host/apple: ${name} has no SDK version`);
      return ['/usr/bin/printf', ['%s\n', value], options];
    }
    command = args.shift();
    const metal = ['-f', '-find', '--find'].includes(command)
      ? windowsMetalCommand(name, args[0], [], options, true) : windowsMetalCommand(name, command, args, options);
    if (metal) return metal;
    if (command === 'swiftc' && !args.includes('--version') && !args.includes('-version')) {
      const sdk = crossSdk(name, env);
      args.push('-resource-dir', sdk.resources, '-tools-directory', sdk.tools, '-use-ld=lld',
        ...linkerDriver(sdk).flatMap(value => ['-Xclang-linker', value]),
        '-external-plugin-path', macroServer(sdk),
        '-Xfrontend', '-enable-cross-import-overlays');
      if (!args.includes('-sdk')) args.push('-sdk', sdk.sdk);
      if (!args.includes('-c')) args.push('-Xlinker', sdk.runtime);
      if (!args.includes('-c') && !args.includes('-platform_version')) args.push(...platformVersion(sdk, valueAfter(args, '-target')).flatMap(value => ['-Xlinker', value]));
    }
    if ((command === 'clang' || command === 'clang++') && flag) {
      const sdk = crossSdk(name, env);
      if (!args.includes('-isysroot')) args.push('-isysroot', sdk.sdk);
      if (!args.includes('-c')) args.push(...linkerDriver(sdk), '-Xlinker', sdk.runtime);
      if (!args.includes('-c') && !args.some(arg => arg.includes('-platform_version'))) {
        const version = platformVersion(sdk, valueAfter(args, '-target') ?? valueAfter(args, '--target'));
        if (version.length) args.push(`-Wl,${version.join(',')}`);
      }
    }
  }
  if (command === 'actool') return [process.execPath, [fileURLToPath(new URL('./car.mjs', import.meta.url)), ...args], options];
  if (command === 'assetutil' && args[0] === '--info' && args.length === 2) return [process.execPath, [fileURLToPath(new URL('./car.mjs', import.meta.url)), ...args], options];
  if (command === 'sips') return [process.execPath, [fileURLToPath(new URL('./image.mjs', import.meta.url)), ...args], options];
  const llvm = { dsymutil: 'dsymutil', lipo: 'llvm-lipo', otool: 'llvm-otool', nm: 'llvm-nm', size: 'llvm-size', strip: 'llvm-strip', install_name_tool: 'llvm-install-name-tool' };
  if (Object.hasOwn(llvm, command)) command = appleLLVMTool(llvm[command], env);
  return [command, args, options];
}

/** The Swift tool a command runs, `swift` or `swiftc`, directly or under `xcrun`; else null. */
function swiftTool(command, args) {
  if (command === 'swift' || command === 'swiftc') return command;
  const tool = command === 'xcrun' ? args[['--sdk', '-sdk'].includes(args[0]) ? 2 : 0] : null;
  return tool === 'swift' || tool === 'swiftc' ? tool : null;
}

/** The TMPDIR a Swift tool runs with on a Mac, and what to do once it exits. This Xcode's Swift driver
 * leaves a `TemporaryDirectory.*` in TMPDIR each time it hands a run to the frontend (`swift --version`,
 * `-print-target-info`, five or more in each `swift build`). A driver run gets a TMPDIR of its own, removed
 * when it exits. SwiftPM keeps its locks in TMPDIR (the scratch path's, `~/.swiftpm`'s, the manifest
 * cache's), so every SwiftPM run shares one, `exact-swiftpm` in the caller's, and the driver's leavings an
 * hour old are removed from it at each run: no SwiftPM run lasts that long. */
function swiftTemp(command, args, options = {}) {
  const none = { options, done() {} };
  if (process.platform !== 'darwin' || !swiftTool(command, args)) return none;
  const env = options.env ?? process.env, base = env.TMPDIR || tmpdir();
  if (command === 'swift' && ['build', 'test', 'run', 'package'].includes(args[0])) {
    const shared = resolve(base, 'exact-swiftpm'), stale = Date.now() - 3600_000;
    mkdirSync(shared, { recursive: true });
    for (const name of readdirSync(shared).filter(name => name.startsWith('TemporaryDirectory.'))) {
      try { if (statSync(resolve(shared, name)).mtimeMs < stale) rmSync(resolve(shared, name), { recursive: true, force: true }); } catch {}
    }
    return { options: { ...options, env: { ...env, TMPDIR: shared } }, done() {} };
  }
  const own = mkdtempSync(resolve(base, 'exact-swift-'));
  return { options: { ...options, env: { ...env, TMPDIR: own } }, done: () => rmSync(own, { recursive: true, force: true }) };
}

export const spawnSync = (command, args, given) => {
  const temp = swiftTemp(command, args, given), options = temp.options;
  const invocation = appleCommand(command, args, options);
  const signing = process.platform === 'darwin' && command === 'codesign' && args.includes('--sign') && args.includes('--timestamp=none') && valueAfter(args, '--sign') !== '-';
  const identity = signing ? valueAfter(args, '--sign') : null;
  if (signing) console.error(`host/apple: signing ${args.at(-1)} with ${identity} (keychain access; ${invocation[2].timeout / 1000}s limit)`);
  let result;
  try { result = systemSpawnSync(...invocation); } finally { temp.done(); }
  if (signing && result.status !== 0) console.error(`host/apple: codesign with ${identity} ${result.error?.code === 'ETIMEDOUT' ? 'timed out' : 'failed'}. Check that the signing keychain is unlocked and codesign can use its private key.${options?.macDevelopmentSign ? ' For a local macOS build without certificate signing, retry with EXACT_IDENTITY=- (ad hoc; drops extensions and makes stored login-keychain items prompt after rebuilds).' : ''}`);
  return result;
};
export const spawn = (command, args, options) => {
  const temp = swiftTemp(command, args, options), child = systemSpawn(...appleCommand(command, args, temp.options));
  child.once('error', temp.done).once('close', temp.done);
  return child;
};

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = spawnSync('xcrun', process.argv.slice(2), { stdio: 'inherit' });
  if (result.error) console.error(result.error.message);
  process.exit(result.status ?? 1);
}
