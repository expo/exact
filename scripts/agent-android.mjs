// The Android carrier (LLP 1107): an app's Linux host built for a phone's ABI
// (`aarch64-linux-android`, the host's Android-gated code, bionic, the phone's
// own fonts) and run headless under `adb shell`, where the agent protocol is
// the Linux host's own (LLP 1012 over stdio, `host/painted/src/agent.rs`). It
// drives the app's runner, kernel and painter on Android; it is not the Canvas
// reader (LLP 1076 §3.3, `host/android`), which paints through its own Kotlin
// view. `--reader` drives that one instead (LLP 1107 D1): the APK, installed
// and launched, answering the same protocol in its own process
// (`host/painted/src/canvas/agent.rs`) over a socket adb forwards.
//
//   bun scripts/agent-android.mjs build <app>              build the app's Linux crate for Android
//   bun scripts/agent.mjs android --app <app> …            drive it on the first adb device or emulator
//   bun scripts/agent-android.mjs build <app> --apk        the app on the Canvas reader, as an APK (host/android/README.md;
//                                                          --release: the release profile, for measuring;
//                                                          --write-locks: write the lock and checksums of the
//                                                          Maven libraries the app's modules name;
//                                                          --install: then onto the device; --run: and launch it)
//   bun scripts/agent.mjs android --reader --app <app> …   drive that APK (EXACT_ANDROID_BIN names another)
//
// An `exact new` app's `bun exact.mjs android` is `build <app> --apk --install` (`--headless`: the Linux host), and its
// `agent android` and `test android` drive the APK.
//
// An app with a TypeScript data module also needs the Android Hermes bundle, which `exact setup` installs from the
// pinned Ibex release (or one built from an Ibex checkout, scripts/hermes-android.mjs).
//
// The NDK comes from ANDROID_NDK_HOME, else the newest under the SDK's ndk/
// (ANDROID_HOME, else ~/Library/Android/sdk or ~/Android/Sdk); ANDROID_SERIAL
// picks a device. Fonts are the phone's (/system/fonts) unless the drive names
// EXACT_FONTS. The painter is the CPU one unless EXACT_PAINTER says
// otherwise: the GPU painter needs a real GPU's Vulkan (an emulator's
// SwiftShader crashes it).
import { writeTamberNames } from './tamber-names.mjs';
import { spawn, spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { inflateRawSync } from 'node:zlib';
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { createConnection } from 'node:net';
import { homedir, tmpdir, userInfo } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { within } from './paths.mjs';
import { HOST_DEV, PINNED_RUST, bakeWatchEnv, buildBake, claimLive, hermesBundle, cargoEnvironment, copyShaders, gpuModules, injectedProfiles, linuxBuild, mavenProblems, resolveApp, wholeProgram } from './app.mjs';
import { ANDROID_MIN_API, hermesAndroidBundle } from './hermes-android.mjs';
import { ANDROID_PAGE_SIZE, androidCargo } from './cargo-check.mjs';
import { guardFlags, reportBuilt } from './help.mjs';
import { androidImplementations, packageConfig } from '../js/bake/src/typescript.mjs';
import { recordBuiltSettings } from '../js/bake/src/__exact_env.mjs';
import { configMetaData, fragmentPermissions, manifestFragment, withMetaData, withAuthCallbacks } from './android-manifest.mjs';
import { androidShareActivity } from './share-target.mjs';
import { pickIcon, placeRgba, readPng, resize } from './app-icons.mjs';
import { encodePng } from './png.mjs';
import { copyStaticTreeIfPresent } from '../host/web/serve.mjs';

export const ANDROID_TARGET = 'aarch64-linux-android';
/** The oldest Android the binary runs on (the NDK's clang wrapper names it). */
const API = ANDROID_MIN_API;
/** What a drive runs: the app's Linux host built for the phone, or on the Canvas reader (`reader`; unsaid,
 * EXACT_ANDROID_READER decides) its APK. */
export const androidBinary = (app, bin, reader = !!process.env.EXACT_ANDROID_READER) => reader ? readerApk(app)
  : resolve(app.target, ANDROID_TARGET, HOST_DEV, bin ?? app.crate('linux'));
export const androidBuild = (app, bin = null) => [...linuxBuild(app, bin), '--target', ANDROID_TARGET];

export const androidSdk = (env = process.env) => env.ANDROID_HOME ?? env.ANDROID_SDK_ROOT
  ?? resolve(homedir(), process.platform === 'darwin' ? 'Library/Android/sdk' : 'Android/Sdk');
/** SDK packages from the APK's Gradle declaration, also used by `exact setup`. */
export function androidSdkPackages(gradle = readFileSync(resolve(import.meta.dir, '../host/android/app/build.gradle.kts'), 'utf8')) {
  const sdk = /compileSdk\s*\{\s*version\s*=\s*release\((\d+)\)\s*\{\s*minorApiLevel\s*=\s*(\d+)/.exec(gradle);
  const tools = /buildToolsVersion\s*=\s*"([\d.]+)"/.exec(gradle);
  if (!sdk || !tools) throw new Error('cannot read compileSdk and buildToolsVersion from host/android/app/build.gradle.kts');
  const platform = `${sdk[1]}${Number(sdk[2]) ? `.${sdk[2]}` : ''}`;
  return [
    { name: 'Android SDK platform', version: platform, id: `platforms;android-${platform}`, path: `platforms/android-${platform}/android.jar` },
    { name: 'Android build-tools', version: tools[1], id: `build-tools;${tools[1]}`, path: `build-tools/${tools[1]}/source.properties` },
  ];
}

const STUDIO_JAVA_HOME = '/Applications/Android Studio.app/Contents/jbr/Contents/Home';

/** Gradle's JDK: JAVA_HOME, else a usable Android Studio JDK on macOS, else PATH. */
export function androidJavaEnv(env = process.env, platform = process.platform, studio = STUDIO_JAVA_HOME) {
  const suffix = platform === 'win32' ? '.exe' : '';
  const check = home => {
    const tool = name => home ? resolve(home, 'bin', name + suffix) : Bun.which(name + suffix, { PATH: env.PATH ?? '' });
    const java = tool('java'), javac = tool('javac');
    if (!java || !javac || !existsSync(java) || !existsSync(javac)) return null;
    const checked = spawnSync(java, ['-version'], { env, encoding: 'utf8', timeout: 5000 });
    const major = /version "(\d+)/.exec(`${checked.stderr ?? ''}${checked.stdout ?? ''}`)?.[1];
    return checked.status === 0 && Number(major) >= 17 ? (home ? { ...env, JAVA_HOME: home } : env) : null;
  };
  if (env.JAVA_HOME) return check(env.JAVA_HOME.replaceAll('"', ''));
  return (platform === 'darwin' ? check(studio) : null) ?? check(null);
}

export function androidJavaFix(env = process.env, platform = process.platform, studio = STUDIO_JAVA_HOME) {
  const rejected = env.JAVA_HOME ? `JAVA_HOME=${env.JAVA_HOME}`
    : platform === 'darwin' && existsSync(resolve(studio, 'bin/java')) ? `Android Studio JDK at ${studio} and java/javac on PATH` : 'java/javac on PATH';
  return `Android needs a JDK 17 or newer (${rejected} is unusable): set JAVA_HOME to a JDK directory${env.JAVA_HOME ? '' : ', or put java and javac on PATH'}`;
}

function sdkManager(env, platform) {
  const root = resolve(androidSdk(env), 'cmdline-tools'), suffix = platform === 'win32' ? '.bat' : '';
  const versions = existsSync(root) ? readdirSync(root).filter(v => v !== 'latest').sort((a, b) => b.localeCompare(a, undefined, { numeric: true })) : [];
  return ['latest', ...versions].map(v => resolve(root, v, `bin/sdkmanager${suffix}`)).find(existsSync)
    ?? Bun.which(`sdkmanager${suffix}`, { PATH: env.PATH ?? '' }) ?? 'sdkmanager';
}

/** A command to run by hand: PowerShell on Windows, POSIX shells elsewhere. Setup never launches it. */
export function androidSdkFix(env = process.env, packages = androidSdkPackages(), platform = process.platform) {
  const windows = platform === 'win32', quoted = windows ? s => `'${String(s).replaceAll("'", "''")}'` : quote;
  return `${windows ? '& ' : ''}${[sdkManager(env, platform), `--sdk_root=${androidSdk(env)}`, ...packages.map(p => p.id)].map(quoted).join(' ')}`;
}

const adb = (env = process.env) => {
  const tool = resolve(androidSdk(env), 'platform-tools/adb');
  return existsSync(tool) ? tool : 'adb';
};

/** The environment `androidBuild` needs: the NDK's clang as the target's linker and C compiler, the compiler at `API`
 * and the linker at `linkApi`. On a Windows host the NDK's prebuilt folder is `windows-x86_64`, its compilers are
 * `.cmd` scripts and its tools `.exe`. */
export function androidToolchainEnv(env = process.env, platform = process.platform, linkApi = API) {
  let ndk = env.ANDROID_NDK_HOME;
  if (!ndk) {
    const root = resolve(androidSdk(env), 'ndk');
    const versions = existsSync(root) ? readdirSync(root).filter((v) => /^\d/.test(v)).sort((a, b) => a.localeCompare(b, undefined, { numeric: true })) : [];
    if (!versions.length) throw new Error(`no Android NDK: install one with the SDK manager (ndk;27.1.12297006), or set ANDROID_NDK_HOME (looked in ${root})`);
    ndk = resolve(root, versions.at(-1));
  }
  const windows = platform === 'win32', script = windows ? '.cmd' : '', exe = windows ? '.exe' : '';
  const bin = resolve(ndk, 'toolchains/llvm/prebuilt', { darwin: 'darwin-x86_64', win32: 'windows-x86_64' }[platform] ?? 'linux-x86_64', 'bin');
  const cc = resolve(bin, `aarch64-linux-android${API}-clang${script}`), linker = resolve(bin, `aarch64-linux-android${linkApi}-clang${script}`);
  for (const tool of [cc, linker]) if (!existsSync(tool)) throw new Error(`the NDK at ${ndk} has no ${basename(tool)}`);
  // The pinned bundle is named only against a developer's HERMES_LEAN_SYS_DIR: hermes-lean-sys checks a bundle it finds
  // itself against the pin's digest, and takes a named one as it is.
  const hermes = env.HERMES_LEAN_SYS_DIR_aarch64_linux_android || hermesAndroidBundle() || (env.HERMES_LEAN_SYS_DIR === undefined ? null : androidHermesDir(env));
  return { ...cargoEnvironment(env), ANDROID_NDK_HOME: ndk, CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER: linker, CC_aarch64_linux_android: cc,
    CXX_aarch64_linux_android: resolve(bin, `aarch64-linux-android${API}-clang++${script}`), AR_aarch64_linux_android: resolve(bin, `llvm-ar${exe}`),
    // One C++ runtime, static, as the Hermes bundle links it (ANDROID_STL=c++_static):
    // the cc crate's Android default, c++_shared, needs libc++_shared.so beside the binary.
    CXXSTDLIB_aarch64_linux_android: 'c++_static',
    ...(hermes ? { HERMES_LEAN_SYS_DIR_aarch64_linux_android: hermes } : {}) };
}

/** A TypeScript data module's lean Hermes VM, for the target only (the host keeps its pinned one): the bundle
 * HERMES_LEAN_SYS_DIR_aarch64_linux_android names, else the one `hermes-android.mjs` installed, else the pinned
 * release's when it is installed (null when it is not). The pinned one is looked for without HERMES_LEAN_SYS_DIR, a
 * host bundle a developer names, which would otherwise answer for Android too. */
export function androidHermesDir(env = process.env, local = hermesAndroidBundle()) {
  const named = env.HERMES_LEAN_SYS_DIR_aarch64_linux_android ?? local;
  if (named) return named;
  const { HERMES_LEAN_SYS_DIR, ...unhosted } = env, pinned = hermesBundle(ANDROID_TARGET, unhosted);
  return pinned.installed ? pinned.root : null;
}

/** The API level an Android Hermes bundle was built for (its receipt's ANDROID_PLATFORM), or null. Its objects call
 * what bionic has from that level on: the pinned bundle's, built at 30, call `pthread_cond_clockwait`. */
export function hermesApi(dir) {
  try {
    const flags = JSON.parse(readFileSync(resolve(dir, 'hermes-input-receipt.json'), 'utf8')).build?.flags ?? [];
    const level = flags.map((flag) => /^-DANDROID_PLATFORM=android-(\d+)$/.exec(flag)?.[1]).find(Boolean);
    return level ? Number(level) : null;
  } catch { return null; }
}

/** The device a drive uses: ANDROID_SERIAL, else the one adb lists as ready. With several and none named it refuses to
 * choose: another lane's emulator or a person's phone may be among them. */
export function androidDevice(env = process.env) {
  if (env.ANDROID_SERIAL) return env.ANDROID_SERIAL;
  const listed = spawnSync(adb(env), ['devices'], { encoding: 'utf8' });
  if (listed.error) throw new Error(`adb: ${listed.error.message} (install the SDK's platform-tools, or set ANDROID_HOME)`);
  const ready = listed.stdout.split('\n').map((l) => l.split('\t')).filter(([, s]) => s?.trim() === 'device').map(([id]) => id);
  if (!ready.length) throw new Error('no Android device or emulator is ready: start one (`emulator -avd <name>`), `adb connect <host:port>` one elsewhere, or attach a phone with USB debugging on');
  if (ready.length > 1) throw new Error(`${ready.length} Android devices are ready (${ready.join(', ')}): set ANDROID_SERIAL to the one to use`);
  return ready[0];
}

const quote = (s) => `'${String(s).replace(/'/g, "'\\''")}'`;

/** Put the binary, the app's assets (and a local `EXACT_PLAN`) in a directory of this drive's own on the
 * device and return how to run it there: `spawn(args)` the process (stdio is the agent's), the device
 * path its screenshots are written to, and `pull(path)` to copy one here. HOME is the app's, kept
 * across drives so a named scratch store (`--storage`) survives, as on the desktop carriers; a run
 * directory is this drive's, so two drives of one app do not share a binary or a screenshot. Only
 * EXACT_* variables cross, with the device's own HOME, assets, native-module directory and fonts. A dev loop and
 * the app's server on this Mac are reached through `adb reverse` for the drive (`reversedPorts`). */
export function androidDeploy(app, bin, env) {
  if (bin.endsWith('.apk')) return readerDeploy(app, bin, env);
  const serial = androidDevice(env), tool = adb(env);
  const base = `/data/local/tmp/exact/${app.id}`, home = `${base}/home`;
  const dir = `${base}/run-${process.pid}-${Date.now().toString(36)}`, exe = basename(bin);
  const run = (args, what) => {
    const r = spawnSync(tool, ['-s', serial, ...args], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(`adb ${what}: ${(r.stderr || r.stdout || r.error?.message || '').trim()}`);
  };
  // A drive removes its own run directory when its process ends (below); one a killed adb orphaned
  // goes after a day. HOME stays.
  run(['shell', `mkdir -p ${quote(home)} ${quote(dir)} && find ${quote(base)} -maxdepth 1 -name 'run-*' -mmin +1440 -exec rm -rf {} + ; true`], 'prepare');
  run(['push', bin, `${dir}/${exe}`], 'push the binary');
  run(['shell', `chmod 755 ${quote(`${dir}/${exe}`)}`], 'chmod');
  if (existsSync(resolve(app.dir, 'assets'))) run(['push', resolve(app.dir, 'assets'), `${dir}/`], 'push the assets');
  const vars = Object.fromEntries(Object.entries(env).filter(([k]) => k.startsWith('EXACT_')));
  // A local plan (`--plan`) is read on the device: copied there with the replacement Rust module the host
  // reads beside it (`rust/`, host/painted/src/delivery.rs), or the baked plan or logic would run instead.
  if (vars.EXACT_PLAN && existsSync(vars.EXACT_PLAN)) {
    const plan = resolve(vars.EXACT_PLAN), rust = resolve(dirname(plan), 'rust');
    run(['push', plan, `${dir}/${basename(plan)}`], 'push the plan');
    if (existsSync(rust)) run(['push', rust, `${dir}/`], 'push the plan\'s Rust module');
    vars.EXACT_PLAN = `${dir}/${basename(plan)}`;
  }
  Object.assign(vars, { HOME: home, EXACT_ASSETS: dir, EXACT_NATIVE_LIBS: dir, EXACT_PAINTER: env.EXACT_PAINTER ?? 'cpu' });
  const assignments = Object.entries(vars).map(([k, v]) => `${k}=${quote(v)}`).join(' ');
  const shot = `${dir}/agent-shot.png`, { lease, release: releasePorts } = reverseForDrive(tool, serial, vars);
  return {
    serial, dir, shot,
    spawn: (args = []) => {
      const command = `cd ${quote(dir)} && env ${assignments} ${[`./${exe}`, ...args].map(quote).join(' ')}; s=$?; cd / && rm -rf ${quote(dir)}; exit $s`;
      const child = lease.ports.length
        ? spawnCarrier(['headless', tool, serial, command, JSON.stringify(lease)], env)
        : spawn(tool, ['-s', serial, 'shell', '-T', command], { stdio: ['pipe', 'pipe', 'pipe'] });
      return child.on('exit', releasePorts).on('error', releasePorts);
    },
    pull(path) {
      const r = spawnSync(tool, ['-s', serial, 'pull', shot, resolve(path)], { encoding: 'utf8' });
      if (r.status !== 0) throw new Error(`adb pull the screenshot: ${(r.stderr || r.stdout).trim()}`);
    },
  };
}

/** The app's `kind` crate's Cargo.toml, beside app.json or in the workspace generated for it (scripts/app-shells.mjs). */
const crateManifest = (app, kind) => [resolve(app.dir, kind, 'Cargo.toml'), resolve(app.workspace, kind, 'Cargo.toml')].find(existsSync);

/** Only the selected package's Android runtime graph links Hermes. Other workspace members and host
 * procedural macros must not raise this binary's minimum API or require an Android bundle. */
export function androidUsesHermes(app, kind) {
  const env = cargoEnvironment();
  const options = { cwd: app.workspace, env, encoding: 'utf8', maxBuffer: 1 << 26 };
  let fetched = false;
  const fetchMissing = error => {
    if (fetched || !/--offline was specified|attempting to make an HTTP request|in the offline mode|offline mode \(via `--offline`\)/.test(error ?? '')) return false;
    fetched = true;
    const result = spawnSync('cargo', ['fetch', '--locked', '--target', ANDROID_TARGET], options);
    if (result.status !== 0) throw new Error(`${error}\ncargo fetch --locked --target ${ANDROID_TARGET}:\n${result.stderr || result.error?.message}`);
    return true;
  };
  // Game preparation resolves the captured lock too, before the package query can run.
  try { app.prepare(false, { target: ANDROID_TARGET, env }); }
  catch (error) {
    if (!fetchMissing(error.message)) throw error;
    app.prepare(false, { target: ANDROID_TARGET, env });
  }
  const args = ['tree', '-p', app.crate(kind), '--target', ANDROID_TARGET, '-e', 'normal,no-proc-macro',
    '--prefix', 'none', '--format', '{p}', '--locked', '--offline'];
  let result = spawnSync('cargo', args, options);
  if (result.status !== 0 && fetchMissing(result.stderr)) result = spawnSync('cargo', args, options);
  if (result.status !== 0) throw new Error(`cargo tree for Android: ${result.stderr || result.error?.message}`);
  return /^exact-js v/m.test(result.stdout);
}

/** The Gradle project that holds the Canvas reader and the shell that packages one app with it. */
const READER = resolve(import.meta.dir, '../host/android');
/** Where one app's Gradle build writes: the shell's build (its staged library and assets, its APK), the reader's, and
 * Gradle's project cache and Kotlin's session files. An app of this checkout's keeps the shell's under
 * host/android/app/build/<name>; an app outside it builds under its own target directory. No two apps share any of it,
 * so builds of two apps at once neither mix outputs nor wait on one project's locks. */
export function apkDirs(app) {
  const own = !within(resolve(READER, '../..'), app.dir), root = resolve(app.target, 'android');
  return own ? { app: resolve(root, 'app'), canvas: resolve(root, 'canvas'), cache: resolve(root, 'gradle'), kotlin: resolve(root, 'kotlin') }
    : { app: resolve(READER, 'app/build', app.name), canvas: resolve(READER, 'canvas/build', app.name), cache: resolve(READER, '.gradle/apps', app.name), kotlin: resolve(READER, '.kotlin/apps', app.name) };
}
/** Where `build <app> --apk` writes the shell's build: the roster it generates (`extensions/`), the merged manifests
 * (`intermediates/`) and the APK (`outputs/apk/`). `app` is a name or a directory, resolved as the build resolves it. */
export const apkBuildDir = (app) => apkDirs(resolveApp(app)).app;
/** The APK `build <app> --apk` makes. */
const readerApk = (app) => resolve(apkDirs(app).app, 'outputs/apk/release/app-release.apk');
/** What `build --apk` records beside the APK, for a drive to tell whether it is older than its sources: when the build
 * began, the library it packaged (whose Cargo dep-info names the Rust, the plan's and the module's sources), and the
 * app's Kotlin. Null without one. */
export function apkRecord(app, apk = readerApk(app)) {
  try { return JSON.parse(readFileSync(resolve(dirname(apk), 'exact-build.json'), 'utf8')); } catch { return null; }
}
/** The reader's own sources, which every APK compiles. */
export const READER_SOURCES = ['canvas/src', 'app/src', 'soundarm', '../apple/soundarm'].map((d) => resolve(READER, d));

// A reader failure whose cleanup succeeded.
const RELAY_CLEANED_ERROR = 70;

/** The reader carrier (LLP 1107 D1): install the APK and return what `androidDeploy` returns, with a process that
 * launches the app under a driver and relays the agent protocol's lines between its stdio and the app (`relay`).
 * The package is the app's id with the suffix the APK was built with (ORG_GRADLE_PROJECT_idSuffix, as the build
 * reads it). The app boots its own baked plan with its own assets, at the phone's size, in the phone's fonts, or a
 * drive's local `--plan`, pushed to the device and named by the launch extra `x_plan` (EXACT_PLAN, which the reader
 * reads as the Linux host does). A drive's `--world`, `--size` and launch URL do not reach it, and all but the size
 * are refused. A dev loop's `--url` does: the reader polls it (LLP 1122 D4), through `adb reverse` when it is this
 * Mac's loopback, as is the app's server a test runs here. */
function readerDeploy(app, apk, env) {
  for (const [name, flag] of [['EXACT_WORLD', '--world'], ['EXACT_LAUNCH_URL', 'a launch URL']])
    if (env[name]) throw new Error(`android --reader: ${flag} does not reach the reader, which boots the plan its APK bakes`);
  if (env.EXACT_PLAN && (!existsSync(env.EXACT_PLAN) || URL.parse(env.EXACT_PLAN)?.protocol?.startsWith('http')))
    throw new Error('android --reader: --plan is a local plan file, which the drive pushes to the phone');
  // A dev loop's URL (`--url`, LLP 1122 D4): the reader polls it, as the Linux display does, through
  // `adb reverse` when it is this Mac's loopback; a file path is no URL a phone reads.
  const devUrl = env.EXACT_DEV_PLAN ? URL.parse(env.EXACT_DEV_PLAN) : null;
  if (env.EXACT_DEV_PLAN && !['http:', 'https:'].includes(devUrl?.protocol)) throw new Error('android --reader: a development plan is a dev loop\'s http(s) URL (--url), not a file');
  if (devUrl) env = { ...env, EXACT_DEV_PLAN: devUrl.href };
  const serial = androidDevice(env), tool = adb(env), id = app.id.replaceAll('-', '_') + (env.ORG_GRADLE_PROJECT_idSuffix ?? ''); // as the Gradle shell names the package
  const dir = `/data/local/tmp/exact/${id}`, shot = `${dir}/reader-shot-${process.pid}.png`;
  const run = (args, what) => {
    const r = spawnSync(tool, ['-s', serial, ...args], { encoding: 'utf8' });
    if (r.status !== 0 || /^Failure/m.test(r.stdout)) throw new Error(`adb ${what}: ${(r.stderr || r.stdout || r.error?.message || '').trim()}`);
  };
  // An APK the device already holds, byte for byte, is not installed again: an install is tens of seconds on an emulator.
  const local = createHash('sha256').update(readFileSync(apk)).digest('hex');
  const held = spawnSync(tool, ['-s', serial, 'shell', `p=$(pm path ${quote(id)} 2>/dev/null | head -n 1); [ -n "$p" ] && sha256sum "\${p#package:}"`], { encoding: 'utf8' }).stdout?.split(/\s/)[0];
  if (held !== local) run(['install', '-r', apk], `install ${apk}`);
  run(['shell', `mkdir -p ${quote(dir)}`], 'prepare');
  // The app reads the plan from the shell's directory, which it may enter but not list.
  if (env.EXACT_PLAN) {
    const plan = `${dir}/plan-${process.pid}.plan`;
    run(['push', env.EXACT_PLAN, plan], 'push the plan');
    run(['shell', `chmod 644 ${quote(plan)}`], 'chmod the plan');
    env = { ...env, EXACT_PLAN: plan };
  }
  const vars = readerVariables(env), { lease, release: releasePorts } = reverseForDrive(tool, serial, vars);
  return {
    serial, dir, shot,
    spawn: () => spawnCarrier(['relay', serial, id, JSON.stringify(vars), JSON.stringify(lease)], env)
      .on('exit', code => {
        if (code !== 0 && code !== RELAY_CLEANED_ERROR) spawnSync(tool, ['-s', serial, 'shell', 'am', 'force-stop', id]);
        releasePorts();
      }).on('error', releasePorts),
    pull(path) {
      const r = spawnSync(tool, ['-s', serial, 'pull', shot, resolve(path)], { encoding: 'utf8' });
      if (r.status !== 0) throw new Error(`adb pull the screenshot: ${(r.stderr || r.stdout).trim()}`);
      spawnSync(tool, ['-s', serial, 'shell', `rm -f ${quote(shot)}`]);
    },
  };
}

/** What the reader reads of a drive's `env`, which reaches it as its `x_<name>` launch extras (`ExactActivity`'s
 * `environment`): EXACT_AGENT and every EXACT_AGENT_* (the store, the epoch, the locale, the seed, the fetch faults),
 * EXACT_STORE (`real`: the platform's stores, a conformance suite's), the auth fixture a test declares
 * (EXACT_AUTH_FIXTURE, LLP 1146 D4), a pushed plan and a dev loop's URL. */
export const readerVariables = (env) => Object.fromEntries(Object.entries(env).filter(([k]) =>
  k === 'EXACT_AGENT' || k.startsWith('EXACT_AGENT_') || ['EXACT_STORE', 'EXACT_AUTH_FIXTURE', 'EXACT_DEV_PLAN', 'EXACT_PLAN'].includes(k)));

/** The ports of this Mac's loopback the reader is to reach through `adb reverse`, for the drive: a dev loop's
 * (`--url`) and the app's server that a test declares its auth fixture (`test android` starts it here). */
export const reversedPorts = (vars) => [...new Set([vars.EXACT_DEV_PLAN, vars.EXACT_AUTH_FIXTURE].map((v) => URL.parse(v ?? ''))
  .filter((url) => url && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname))
  .map((url) => url.port || (url.protocol === 'https:' ? '443' : '80')))];

// SQLite supplies a process lock on every desktop host. The ownership file stays outside its transaction
// so a rollback after SIGKILL cannot discard a claim for an adb change that already happened.
function withReverseState(lease, body) {
  const { Database } = import.meta.require('bun:sqlite');
  mkdirSync(lease.root, { recursive: true });
  const lock = new Database(resolve(lease.root, 'lock')), path = resolve(lease.root, 'owners.json');
  try {
    lock.exec('PRAGMA busy_timeout = 30000; BEGIN EXCLUSIVE');
    const state = existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : {};
    const save = () => {
      const pending = resolve(lease.root, 'owners.pending');
      writeFileSync(pending, JSON.stringify(state)); renameSync(pending, path);
    };
    return body(state, save);
  } finally { lock.close(); }
}

const reverseOwner = () => ({ pid: process.pid, started: new Date().toISOString() });

// Finish the adb command and its ownership write even if the calling drive or its process group is killed.
function reverseCall(action, tool, serial, lease, ports = []) {
  const result = spawnSync(process.execPath, [import.meta.path, 'reverse', action, tool, serial, JSON.stringify(lease), JSON.stringify(ports)], { encoding: 'utf8', detached: true });
  if (result.status !== 0) throw new Error(`Android reverse ${action}: ${(result.stderr || result.error?.message || '').trim()}`);
  const reply = JSON.parse(result.stdout);
  if (reply.error) throw new Error(reply.error);
  return reply.value;
}

/** Where the drives of one adb server and device record who owns its reverse mappings. Drives
 * share the account's adb server whatever cache their environments name, so the record is under
 * the account's home directory, not `XDG_CACHE_HOME` or `HOME`. */
export function reverseRoot(serial) {
  const server = (process.env.ADB_SERVER_SOCKET ?? `tcp:${process.env.ANDROID_ADB_SERVER_ADDRESS ?? 'localhost'}:${process.env.ANDROID_ADB_SERVER_PORT ?? '5037'}`)
    .replace(/^tcp:(?:(?:localhost|127\.0\.0\.1|::1|\[::1\]):)?(\d+)$/, 'tcp:localhost:$1');
  const key = createHash('sha256').update(JSON.stringify([server, serial])).digest('hex');
  return resolve(userInfo().homedir, '.cache/exact/android-reverse', key);
}

// Existing mappings without an ownership record are external and are never removed by a drive.
function reverseForDrive(tool, serial, vars) {
  const ports = reversedPorts(vars);
  const lease = { root: reverseRoot(serial), token: randomUUID(), owner: reverseOwner(), ports: [] };
  if (ports.length) lease.ports = reverseCall('acquire', tool, serial, lease, ports);
  return { lease, release: () => releaseReverse(tool, serial, lease) };
}

function acquireReverse(tool, serial, lease, ports) {
  const run = (...args) => {
    const r = spawnSync(tool, ['-s', serial, 'reverse', ...args], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(`adb reverse ${args.join(' ')}: ${(r.stderr || r.stdout || r.error?.message || '').trim()}`);
    return r.stdout;
  };
  if (ports.length) {
    try {
      withReverseState(lease, (state, save) => {
        const present = new Map(run('--list').trim().split('\n').filter(Boolean).map(line => line.trim().split(/\s+/).slice(1, 3)));
        for (const port of ports.map(port => `tcp:${port}`)) {
          if (present.has(port) && present.get(port) !== port) {
            delete state[port]; save();
            throw new Error(`adb reverse: ${port} maps to ${present.get(port)}; needed ${port}`);
          }
          if (present.has(port) && !state[port]) continue;
          state[port] ??= { generation: randomUUID(), owners: {} };
          state[port].owners[lease.token] = lease.owner;
          lease.ports.push({ port, generation: state[port].generation });
          save(); // Keep the claim if this process dies after adb adds the mapping.
          if (!present.has(port)) {
            try { run('--no-rebind', port, port); }
            catch (error) {
              delete state[port].owners[lease.token];
              if (!Object.keys(state[port].owners).length) delete state[port];
              save();
              throw error;
            }
          }
        }
      });
    } catch (e) { releaseReverseState(tool, serial, lease); throw e; }
  }
  if (!claimLive(JSON.stringify(lease.owner))) releaseReverseState(tool, serial, lease);
  return lease.ports;
}

// Both the carrier and its parent can release a lease. Generations make retries harmless after replacement.
function releaseReverse(tool, serial, lease) {
  return !lease.ports.length || reverseCall('release', tool, serial, lease);
}

function releaseReverseState(tool, serial, lease) {
  if (!lease.ports.length) return true;
  return withReverseState(lease, (state, save) => {
    const owned = lease.ports.filter(({ port, generation }) => state[port]?.generation === generation && state[port].owners[lease.token]);
    if (!owned.length) return true;
    const listed = spawnSync(tool, ['-s', serial, 'reverse', '--list'], { encoding: 'utf8' });
    if (listed.status !== 0) return false;
    const present = new Map(listed.stdout.trim().split('\n').filter(Boolean).map(line => line.trim().split(/\s+/).slice(1, 3)));
    let released = true;
    for (const { port } of owned) {
      const others = Object.entries(state[port].owners).filter(([token, owner]) => token !== lease.token && claimLive(JSON.stringify(owner)));
      if (present.has(port) && present.get(port) !== port) delete state[port];
      else if (others.length) state[port].owners = Object.fromEntries(others);
      else if (!present.has(port) || spawnSync(tool, ['-s', serial, 'reverse', '--remove', port]).status === 0) delete state[port];
      else released = false;
      save();
    }
    return released;
  });
}

// A killed wrapper must stop its adb children before the parent releases its reverse mappings.
function spawnCarrier(args, env) {
  const grouped = process.platform !== 'win32';
  const child = spawn(process.execPath, [import.meta.path, ...args], { env, stdio: ['pipe', 'pipe', 'pipe'], detached: grouped });
  if (grouped) {
    const signalGroup = signal => {
      if (!child.pid) return false;
      try { process.kill(-child.pid, signal); return true; }
      catch (error) { if (error.code !== 'ESRCH') throw error; return false; }
    };
    child.kill = (signal = 'SIGTERM') => {
      const sent = signalGroup(signal);
      if (sent) child.killed = true;
      return sent;
    };
    child.once('exit', () => signalGroup('SIGKILL'));
  }
  return child;
}

async function headless([tool, serial, command, encodedLease]) {
  const lease = JSON.parse(encodedLease);
  const child = spawn(tool, ['-s', serial, 'shell', '-T', command], { stdio: 'inherit' });
  const code = await new Promise(resolve => { child.on('exit', code => resolve(code ?? 1)); child.on('error', () => resolve(1)); });
  process.exit(releaseReverse(tool, serial, lease) ? code : 1);
}

/** How long after an operation's answer a screenshot is taken: the reader draws what the operation painted over
 * the frames that follow, and a window of the top layer comes with its own animation. */
const SHOT_SETTLE_MS = 400;

/** Power and keyguard facts exposed by Android's dumpsys, including Android 10's policy delegate. */
export function androidScreenState(power, window) {
  const wakefulness = /\bmWakefulness=(\w+)/.exec(power)?.[1];
  if (!wakefulness) throw new Error('Android screen state: dumpsys power has no mWakefulness');
  const lines = window.split('\n'), heading = lines.findIndex(line => /^\s*KeyguardServiceDelegate\b/.test(line));
  let delegate = '';
  if (heading >= 0) {
    const indent = lines[heading].match(/^\s*/)[0].length;
    for (const line of lines.slice(heading + 1)) {
      if (line.trim() && line.match(/^\s*/)[0].length <= indent) break;
      delegate += line + '\n';
    }
  }
  const flag = (text, names) => {
    const values = [...text.matchAll(new RegExp(`\\b(?:${names})=(true|false)\\b`, 'g'))].map(match => match[1]);
    return values.length ? values.includes('true') : null;
  };
  const showing = flag(window, 'isKeyguardShowing|mShowingLockscreen') ?? flag(delegate, 'showing');
  const secure = flag(window, 'isKeyguardSecure') ?? flag(delegate, 'secure');
  return { wakefulness, showing, secure };
}

const screenDescription = ({ wakefulness, showing }) => `${wakefulness}, keyguard ${showing == null ? 'unknown' : showing ? 'showing' : 'hidden'}`;

async function prepareAndroidScreen(tool, serial) {
  const run = (...args) => {
    const result = spawnSync(tool, ['-s', serial, 'shell', ...args], { encoding: 'utf8', timeout: 5000, maxBuffer: 4 << 20 });
    if (result.status !== 0) throw new Error(`Android screen state: adb shell ${args.join(' ')}: ${(result.error?.message || result.stderr || result.stdout || `exit ${result.status}`).trim()}`);
    return result.stdout;
  };
  const read = () => androidScreenState(run('dumpsys', 'power'), run('dumpsys', 'window', 'policy'));
  let state = read();
  const initial = screenDescription(state);
  if (state.wakefulness !== 'Awake') run('input', 'keyevent', 'KEYCODE_WAKEUP');
  let dismissed = false;
  let deadline = Date.now() + 2000;
  for (;;) {
    if (state.showing && state.secure !== false) throw new Error(`Android ${state.secure ? 'secure lock screen' : 'lock screen (security unknown)'} is showing (${state.wakefulness}); unlock the device and run the drive again`);
    if (state.wakefulness === 'Awake' && !state.showing) {
      const final = screenDescription(state);
      return initial === final ? final : `${initial} → ${final}`;
    }
    const canDismiss = state.wakefulness === 'Awake' && state.showing && state.secure === false;
    if (Date.now() >= deadline && !(canDismiss && !dismissed)) throw new Error(`Android screen is not ready (${screenDescription(state)}); wake and unlock the device and run the drive again`);
    if (canDismiss) {
      run('wm', 'dismiss-keyguard');
      // Slow wake observations must not consume the first dismissal or its observation budget.
      if (!dismissed) { deadline = Date.now() + 2000; dismissed = true; }
    }
    await new Promise(resolve => setTimeout(resolve, 100));
    state = read();
  }
}

/** The reader carrier's process: launch the app with a driver's socket (the shell's `agent=<name>` extra), have
 * adb forward it, and carry the protocol's lines between this process's stdio and the app. Every line is the app's
 * to answer but `screenshot`, whose picture is the device's (`screencap`), tagged as the app tags a reply. The
 * app's own stderr (logcat's `exact` tag) is this process's. Ends when stdin does, and the app with it. */
async function relay([serial, id, vars, encodedLease]) {
  const tool = adb(), sh = (...args) => spawnSync(tool, ['-s', serial, ...args], { encoding: 'utf8' });
  const name = `exact-agent-${process.pid}`, lease = JSON.parse(encodedLease);
  let port, logs, cleaned = false, released = true, connected = false, turn = Promise.resolve(), buffered = '';
  let input = data => { buffered += data; };
  const cleanup = () => {
    if (cleaned) return released;
    cleaned = true;
    logs?.kill();
    if (port && sh('forward', '--remove', `tcp:${port}`).status !== 0) released = false;
    sh('shell', 'am', 'force-stop', id);
    if (!releaseReverse(tool, serial, lease)) released = false;
    return released;
  };
  const done = code => { process.exit(cleanup() ? (code === 0 ? 0 : RELAY_CLEANED_ERROR) : 1); };
  process.stdin.setEncoding('utf8');
  process.stdin.on('data', data => input(data));
  process.stdin.on('end', () => { if (connected) turn.then(() => done(0)); else done(0); });
  if (process.stdin.readableEnded) return done(0);
  try {
    // EXACT_AGENT_INTENT: `am start` words the launch carries besides the drive's, for a drive that starts the app
    // as another app would (a share's `-a android.intent.action.SEND …`, LLP 1133.024 D5).
    const { EXACT_AGENT_INTENT: intent = '', ...launch } = JSON.parse(vars);
    const extras = Object.entries(launch).flatMap(([k, v]) => ['--es', `x_${k.slice('EXACT_'.length).toLowerCase()}`, quote(v)]);
    console.error(`Android screen before launch: ${await prepareAndroidScreen(tool, serial)}`);
    sh('shell', 'am', 'force-stop', id);
    sh('logcat', '-c');
    // `--activity-clear-task`: a force-stop ends the app's process but leaves its task, and an activity of another
    // app in it (the permission controller's dialog a last drive raised) would cover this launch. `lower=0`: the
    // engine samples every animation, so none plays on the reader's own clock.
    const started = sh('shell', ['am', 'start', '-W', '--activity-clear-task', '-n', `${id}/dev.exact.app.MainActivity`, '--es', 'agent', name, '--es', 'lower', '0', ...extras, intent].join(' '));
    if (started.status !== 0 || /Error/.test(started.stdout + started.stderr)) throw new Error(`am start ${id}: ${(started.stderr || started.stdout).trim()}`);
    port = Number(sh('forward', 'tcp:0', `localabstract:${name}`).stdout.trim());
    if (!port) throw new Error('adb forward gave no port');
    logs = spawn(tool, ['-s', serial, 'logcat', '-v', 'raw', 'exact:V', 'AndroidRuntime:E', '*:S'], { stdio: ['ignore', 'pipe', 'ignore'] });
    logs.stdout.pipe(process.stderr);
    // The app opens its socket as its activity is made; the forward connects when it is there.
    let socket;
    for (let tries = 0; !socket; tries++) {
      socket = await new Promise((r) => { const c = createConnection(port, '127.0.0.1', () => r(c)); c.once('error', () => r(null)); });
      if (!socket && tries > 50) throw new Error(`the reader opened no agent socket (${name}): is the APK built from a tree that has AgentDrive?`);
      if (!socket) await new Promise((r) => setTimeout(r, 100));
    }
    // Lines from the app, in order: each goes to whoever waits for one.
    const waiting = [], ready = [];
    let rest = '';
    socket.setEncoding('utf8');
    socket.on('data', (d) => { const lines = (rest + d).split('\n'); rest = lines.pop(); for (const l of lines) (waiting.shift() ?? ((x) => ready.push(x)))(l); });
    socket.on('close', () => done(1));
    socket.on('error', () => done(1));
    const answer = () => ready.length ? Promise.resolve(ready.shift()) : new Promise((r) => waiting.push(r));
    const say = (line) => process.stdout.write(line + '\n');
    const pollsLanded = async () => {
      for (const end = Date.now() + 2000; Date.now() < end;) {
        socket.write('{"op":"polls"}\n');
        if (!JSON.parse(await answer()).asking) return;
        await new Promise((r) => setTimeout(r, 10));
      }
    };
    say(await answer()); // the app's `ready`
    connected = true;
    let pending = '';
    const one = async (line) => {
      let req = null;
      try { req = JSON.parse(line); } catch {}
      if (req?.op === 'quit') return done(0);
      if (req?.op !== 'screenshot') {
        socket.write(line + '\n');
        const reply = await answer();
        // A jump's asks of polled module sources land before its reply is given (LLP 1133.009 D10 item 7). The
        // reader's extensions answer on its main thread, which takes their asks between requests, so the wait is here.
        if (req?.op === 'clock' && !req.settle) await pollsLanded();
        return say(reply);
      }
      if (req.film) return say(JSON.stringify({ error: 'the reader carrier films nothing: its picture is a screencap of the device' }));
      await new Promise((r) => setTimeout(r, SHOT_SETTLE_MS));
      const shot = sh('shell', `screencap -p ${quote(req.path)}`);
      socket.write('{"op":"tags"}\n');
      const tags = JSON.parse(await answer());
      say(JSON.stringify(shot.status === 0 ? { screenshot: req.path, device: true, ...tags } : { error: `screencap: ${(shot.stderr || shot.stdout).trim()}` }));
    };
    input = d => { const lines = (pending + d).split('\n'); pending = lines.pop(); for (const l of lines) if (l.trim()) turn = turn.then(() => one(l)); };
    input(buffered); buffered = '';
    await new Promise(() => {}); // The socket or stdin ends the relay through done().
  } catch (error) { console.error(error.message); done(1); }
  finally { cleanup(); }
}

/** The build script's output directory of `crate`, where it wrote its plan, `compat.json` and `uses.txt`
 * (`contract::native::android_entry`), found by Cargo's JSON messages. */
function planOut(messages, crate) {
  for (const line of messages.split('\n')) {
    if (!line.startsWith('{')) continue;
    const m = JSON.parse(line);
    if (m.reason === 'build-script-executed' && (m.package_id.includes(`#${crate}@`) || m.package_id.startsWith(`${crate} `))) return m.out_dir;
  }
  return null;
}

/** The SHA-256 of what an ELF64 file loads: its PT_LOAD segments' bytes, past the ELF header. AGP strips a
 * packaged library's debug symbols, which rewrites the header and nothing a segment holds, so this is the same
 * for the library a build staged and the one its APK holds, and differs for another build's (LLP 1159.002 D2). */
export function loadedImage(elf) {
  const headers = Number(elf.readBigUInt64LE(0x20)), size = elf.readUInt16LE(0x36), hash = createHash('sha256');
  for (let i = 0, count = elf.readUInt16LE(0x38); i < count; i++) {
    const at = headers + i * size;
    if (elf.readUInt32LE(at) !== 1) continue;
    const offset = Number(elf.readBigUInt64LE(at + 0x08));
    hash.update(elf.subarray(Math.max(offset, 64), offset + Number(elf.readBigUInt64LE(at + 0x20))));
  }
  return hash.digest('hex');
}

/** One file of a zip archive (an APK), stored or deflated, or null when the archive has none by that name. Read
 * here, with no `unzip`, so the build does the same on a Windows host. */
export function zipEntry(zip, name) {
  let end = zip.length - 22;
  while (end >= 0 && zip.readUInt32LE(end) !== 0x06054b50) end--;
  if (end < 0) return null;
  for (let at = zip.readUInt32LE(end + 16), left = zip.readUInt16LE(end + 10); left > 0 && zip.readUInt32LE(at) === 0x02014b50; left--) {
    const length = zip.readUInt16LE(at + 28);
    if (zip.toString('utf8', at + 46, at + 46 + length) === name) {
      const local = zip.readUInt32LE(at + 42), data = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const bytes = zip.subarray(data, data + zip.readUInt32LE(at + 20));
      return zip.readUInt16LE(at + 10) === 0 ? bytes : inflateRawSync(bytes);
    }
    at += 46 + length + zip.readUInt16LE(at + 30) + zip.readUInt16LE(at + 32);
  }
  return null;
}

/** Why an APK's library is not the one its build staged, or null (LLP 1159.002 D2): Gradle can keep an earlier
 * build's library in its merged intermediates and report the task up to date (QUEUE, "Gradle's file watching"). */
export function keptLibrary(staged, packaged, dirs, name = "libexactcanvas.so") {
  const made = loadedImage(staged), held = packaged ? loadedImage(packaged) : null;
  if (made === held) return null;
  return `the APK holds ${held ? `a ${name} that is not the one this build made (${held.slice(0, 12)} in the APK, ${made.slice(0, 12)} staged, by what each loads)` : `no ${name}`}. Gradle kept an earlier build's: delete ${dirs.join(' and ')} and build again`;
}

/** The smallest alignment of an ELF64 file's PT_LOAD segments (its program headers' p_align). */
export function loadAlignment(path) {
  const elf = readFileSync(path), headers = Number(elf.readBigUInt64LE(0x20)), size = elf.readUInt16LE(0x36);
  let least = Infinity;
  for (let i = 0, count = elf.readUInt16LE(0x38); i < count; i++) {
    if (elf.readUInt32LE(headers + i * size) === 1) least = Math.min(least, Number(elf.readBigUInt64LE(headers + i * size + 0x30)));
  }
  return least;
}

/** The capabilities the plan uses (LLP 1047 D2), one a line in `uses.txt`; none when the crate writes none. */
const planUses = (out) => out && existsSync(resolve(out, 'uses.txt')) ? readFileSync(resolve(out, 'uses.txt'), 'utf8').split('\n').filter(Boolean) : [];

/** The same, where the build ran without JSON (a bake): the crate's build script output that wrote `uses.txt` last,
 * under Cargo's `build` directory. */
function bakedOut(dir, crate) {
  const prefix = `${crate.replaceAll('-', '_')}-`, alt = `${crate}-`;
  let newest = null;
  for (const name of existsSync(dir) ? readdirSync(dir) : []) {
    if (!name.startsWith(prefix) && !name.startsWith(alt)) continue;
    const file = resolve(dir, name, 'out/uses.txt');
    if (existsSync(file) && (!newest || statSync(file).mtimeMs > newest.at)) newest = { out: resolve(dir, name, 'out'), at: statSync(file).mtimeMs };
  }
  return newest?.out ?? null;
}

/** How the reader's Gradle wrapper starts on `platform`. Windows cannot run the POSIX `gradlew`, and Bun starts a `.bat`
 * only through cmd.exe, which would reparse every argument: there the JDK runs the wrapper jar with the arguments
 * `gradlew` gives it, JAVA_OPTS and GRADLE_OPTS among them (JAVA_HOME's java, else the one on PATH). */
export function gradleWrapper(platform = process.platform, env = process.env) {
  if (platform !== 'win32') return [resolve(READER, 'gradlew')];
  // gradlew.bat drops the quotes a `set JAVA_HOME="C:\Program Files\…"` stores in the variable.
  const home = env.JAVA_HOME?.replaceAll('"', '');
  const java = home ? resolve(home, 'bin', 'java.exe') : 'java.exe';
  return [java, '-Xmx64m', '-Xms64m', ...jvmOptions(env.JAVA_OPTS, 'JAVA_OPTS'), ...jvmOptions(env.GRADLE_OPTS, 'GRADLE_OPTS'),
    '-Dorg.gradle.appname=gradlew', '-jar', resolve(READER, 'gradle/wrapper/gradle-wrapper.jar')];
}

/** A JVM options variable as java.exe sees it under gradlew.bat, which expands `%JAVA_OPTS% %GRADLE_OPTS%` unquoted onto
 * java's command line: split by the Microsoft C runtime's rules ("Parsing C command-line arguments", learn.microsoft.com).
 * Spaces and tabs separate words outside double quotes; only `"` quotes, and `""` inside quotes is one `"`; backslashes
 * are literal except before a `"`, where 2n of them are n and the quote still quotes, and 2n+1 are n and a literal `"`
 * (so `\"` is an escaped quote). An apostrophe is an ordinary character. With no cmd.exe between, `|`, `&` and `^` are
 * data too. An unmatched `"` is refused by the variable's name and the quote's position: a value can hold a password. */
export function jvmOptions(value = '', name = 'the JVM options') {
  const words = [];
  let word = null, quoted = false, opened = 0;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (c === '\\') {
      let n = 0;
      while (value[i + n] === '\\') n++;
      if (value[i + n] !== '"') { word = (word ?? '') + '\\'.repeat(n); i += n - 1; }
      else if (n % 2) { word = (word ?? '') + '\\'.repeat(n >> 1) + '"'; i += n; }
      else { word = (word ?? '') + '\\'.repeat(n >> 1); i += n - 1; }
    } else if (c === '"') {
      if (quoted && value[i + 1] === '"') { word += '"'; i++; }
      else { quoted = !quoted; opened = i; word ??= ''; }
    } else if (!quoted && (c === ' ' || c === '\t')) {
      if (word !== null) words.push(word);
      word = null;
    } else word = (word ?? '') + c;
  }
  if (quoted) throw new Error(`${name} has an unmatched " at character ${opened + 1}`);
  if (word !== null) words.push(word);
  return words;
}

/** Relay Gradle's progress unchanged and add lock advice only after a dependency failure. Runs in a child Bun. */
async function gradleWithAdvice(command, args, advice) {
  const { spawn } = await import('node:child_process');
  const child = spawn(command, args, { stdio: ['inherit', 'pipe', 'pipe'] });
  let dependencyFailure = false;
  for (const [source, destination] of [[child.stdout, process.stdout], [child.stderr, process.stderr]]) {
    let tail = '';
    source.on('data', chunk => {
      destination.write(chunk);
      const text = tail + chunk.toString('utf8');
      dependencyFailure ||= /dependency verification failed|dependency lock state|strict dependency locking/i.test(text.replace(/\x1b\[[0-9;]*m/g, ''));
      tail = text.slice(-256);
    });
  }
  const result = await new Promise(resolve => {
    child.once('error', error => { console.error(`could not start Gradle (${command}): ${error.message}`); resolve({ code: 1 }); });
    child.once('close', (code, signal) => resolve({ code, signal }));
  });
  if (result.code && dependencyFailure) console.error(advice);
  if (result.signal) process.kill(process.pid, result.signal);
  else process.exitCode = result.code ?? 1;
}

/** Gradle in the reader with `args`, under `options` (its `env` names the JDK): whether it succeeded. A Gradle that
 * could not be started is said so. */
export function runGradle(args, options, platform = process.platform, { studio, run = spawnSync, lockAdvice } = {}) {
  const env = androidJavaEnv(options.env, platform, studio) ?? options.env;
  const [command, ...prefix] = gradleWrapper(platform, env);
  const invocation = lockAdvice ? [process.execPath, ['--no-env-file', '-e', `await (${gradleWithAdvice})(...JSON.parse(process.argv[1]))`, JSON.stringify([command, [...prefix, ...args], lockAdvice])]] : [command, [...prefix, ...args]];
  const gradle = run(...invocation, { cwd: READER, stdio: 'inherit', ...options, env });
  if (gradle.error) console.error(`could not start Gradle (${command}): ${gradle.error.message}`);
  return gradle.status === 0;
}

// The adaptive icon's layer, 108 dp, and the maskable image on it, 82.5 dp, in pixels at xxxhdpi (4 px a dp). At
// 82.5 dp the image's W3C safe zone (the centred circle of 80% diameter) is the layer's 66 dp one (developer.android.com,
// "Adaptive icons").
const LAYER = 432, MASKABLE = 330;
// What the key of the made resources names besides their sources: a change to how they are made makes them again.
const ANDROID_ICONS = ['icons-v1', LAYER, MASKABLE];
/** A CSS hex colour (#RGB, #RRGGBB or #RRGGBBAA) as an Android colour resource's #AARRGGBB. */
const androidColour = (css) => {
  const hex = css.length === 4 ? [...css.slice(1)].map((c) => c + c).join('') : css.slice(1);
  return `#${hex.slice(6) || 'ff'}${hex.slice(0, 6)}`.toUpperCase();
};

/** The launcher icon and TV banner from the manifest's `icons` (LLP 1163 D6), written as resources under `res`, and
 * the references to them: `icon` for `android:icon` and `android:roundIcon`, `banner` for `android:banner`, each null
 * when the manifest has none. A square `maskable` PNG of at least 432 px becomes an adaptive icon over
 * `background_color`, else over the image's top-left pixel, and a square `monochrome` one of that size, its alpha drawn
 * white, its themed layer. Without one, a square `any` PNG of at least 192 px is the legacy icon, which the launcher
 * encloses itself. A 16:9 `any` PNG of at least 640×360 px is the banner, the size Android TV asks for at xxxhdpi.
 * Every refusal is thrown at once, before anything is written. */
export function androidIcons(manifest, dir, res) {
  const problems = [], slot = (fields) => ({ formats: ['png'], native: true, ...fields });
  const pick = (fields) => { try { return pickIcon(manifest, dir, slot(fields)); } catch (error) { problems.push(error.message); return null; } };
  // A maskable too small for the adaptive icon leaves the icon to a square `any`, as iOS's slot does; with none it is refused.
  let adaptive = null, small = null;
  try { adaptive = pickIcon(manifest, dir, slot({ purposes: ['maskable'], px: LAYER, name: "Android's adaptive icon" })); } catch (error) { small = error.message; }
  const legacy = adaptive ? null : pick({ purposes: ['any'], px: 192, name: "Android's launcher icon" });
  if (small && !legacy) problems.unshift(small);
  const glyph = adaptive && pick({ purposes: ['monochrome'], px: LAYER, glyph: true, name: "Android's themed icon" });
  const banner = pick({ purposes: ['any'], shape: 16 / 9, px: 640, name: 'the Android TV banner' });
  if (problems.length) throw new Error(problems.join('\n'));
  const refs = { icon: adaptive || legacy ? '@mipmap/exact_icon' : null, banner: banner ? '@drawable/exact_banner' : null };
  // Made again only when a picked file or the background colour changes: the key of what was made is kept beside `res`.
  const key = createHash('sha256').update(JSON.stringify([ANDROID_ICONS, [adaptive, legacy, glyph, banner].map((file) => file && [file.src, createHash('sha256').update(readFileSync(file.path)).digest('hex')]), manifest.background_color ?? null])).digest('hex');
  if (existsSync(res) && existsSync(`${res}.key`) && readFileSync(`${res}.key`, 'utf8') === key) return refs;
  rmSync(res, { recursive: true, force: true });
  const write = (path, bytes) => { mkdirSync(dirname(resolve(res, path)), { recursive: true }); writeFileSync(resolve(res, path), bytes); };
  const onLayer = (image) => placeRgba(LAYER, LAYER, resize(image, MASKABLE, MASKABLE), (LAYER - MASKABLE) / 2, (LAYER - MASKABLE) / 2);
  if (adaptive) {
    // The corner's colour without its alpha: the background layer is opaque.
    const image = readPng(adaptive.path), corner = `#${[...image.data.subarray(0, 3)].map((v) => v.toString(16).padStart(2, '0')).join('')}`;
    write('values/exact_icon.xml', `<resources>\n    <color name="exact_icon_background">${androidColour(manifest.background_color ?? corner)}</color>\n</resources>\n`);
    write('drawable-nodpi/exact_icon_foreground.png', encodePng(onLayer(image)));
    if (glyph) {
      const placed = onLayer(readPng(glyph.path)), white = new Uint8Array(LAYER * LAYER * 4).fill(255);
      for (let i = 3; i < white.length; i += 4) white[i] = placed.data[i];
      write('drawable-nodpi/exact_icon_monochrome.png', encodePng({ width: LAYER, height: LAYER, data: white }));
    }
    write('mipmap-anydpi/exact_icon.xml', ['<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">',
      '    <background android:drawable="@color/exact_icon_background" />', '    <foreground android:drawable="@drawable/exact_icon_foreground" />',
      ...(glyph ? ['    <monochrome android:drawable="@drawable/exact_icon_monochrome" />'] : []), '</adaptive-icon>', ''].join('\n'));
  }
  if (legacy) write('mipmap-xxxhdpi/exact_icon.png', encodePng(resize(readPng(legacy.path), 192, 192)));
  if (banner) write('drawable-xxxhdpi/exact_banner.png', encodePng(resize(readPng(banner.path), 640, 360)));
  mkdirSync(dirname(res), { recursive: true });
  writeFileSync(`${res}.key`, key);
  return refs;
}

/** Whether icons in the bars read dark over a CSS hex colour: its CIE lightness above 49, the threshold the painter's
 * `look::ink_on` takes for the ink on a fill. A translucent colour is taken as it would be over white. */
const lightColour = (css) => {
  const hex = css.length === 4 ? [...css.slice(1)].map((c) => c + c).join('') : css.slice(1);
  const alpha = hex.length === 8 ? parseInt(hex.slice(6), 16) / 255 : 1;
  const linear = (i) => {
    const c = (parseInt(hex.slice(i, i + 2), 16) / 255) * alpha + (1 - alpha);
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const y = 0.2126 * linear(0) + 0.7152 * linear(2) + 0.0722 * linear(4);
  return (y > 0.008856 ? 116 * Math.cbrt(y) - 16 : 903.3 * y) > 49;
};

/** The launch window in the app's first-frame background (LLP 1115.001 D3.6): the manifest's `background_color`, and
 * `color_scheme_dark.background_color` in night mode, as the iOS launch screen and the web page's first paint take
 * them (a fixed `userInterfaceStyle` takes its side alone, as there). Written under `res` as `ExactAppLaunch`, the
 * reader's `ExactLaunch` with that colour behind the window and on Android 12's starting window, and the bars' icons
 * dark over a light colour and light over a dark one, until the host says the page's (`ExactActivity.chrome`).
 * Returns the activity's theme: null where the manifest names no colour, which leaves the platform's surface. */
export function androidLaunch(manifest, res) {
  rmSync(res, { recursive: true, force: true });
  const style = manifest.userInterfaceStyle, named = manifest.color_scheme_dark?.background_color;
  let light = manifest.background_color;
  if (!light) return null;
  const dark = style === 'light' ? light : named ?? light;
  if (style === 'dark') light = dark;
  const write = (dir, body) => {
    mkdirSync(resolve(res, dir), { recursive: true });
    writeFileSync(resolve(res, dir, 'exact_launch.xml'), `<?xml version="1.0" encoding="utf-8"?>\n<resources>\n${body}</resources>\n`);
  };
  const scheme = (css) => `    <color name="exact_launch">${androidColour(css)}</color>\n    <bool name="exact_launch_light">${lightColour(css)}</bool>\n`;
  const theme = (splash) => [
    '    <style name="ExactAppLaunch" parent="ExactLaunch">',
    '        <item name="android:windowBackground">@color/exact_launch</item>',
    ...(splash ? ['        <item name="android:windowSplashScreenBackground">@color/exact_launch</item>'] : []),
    '        <item name="android:windowLightStatusBar">@bool/exact_launch_light</item>',
    '        <item name="android:windowLightNavigationBar">@bool/exact_launch_light</item>',
    '    </style>', ''].join('\n');
  write('values', scheme(light) + theme(false));
  write('values-night', scheme(dark));
  write('values-v31', theme(true));
  return '@style/ExactAppLaunch';
}

/** CI inputs shared by APK and App Bundle builds (LLP 1030.003 D6, D8). Refuse incomplete signing
 * before compiling; a bundle must use an upload key. Passwords stay in the environment, never command arguments. */
export function androidReleaseEnvironment(env = process.env, bundle = false) {
  const code = env.EXACT_BUILD_NUMBER ?? '1', version = env.EXACT_VERSION ?? '0.1.0';
  if (!/^[1-9]\d*$/.test(code) || Number(code) > 2100000000) throw new Error('EXACT_BUILD_NUMBER must be an integer from 1 to 2100000000 on Android');
  if (!version.trim()) throw new Error('EXACT_VERSION must not be empty');
  const keys = ['EXACT_KEYSTORE', 'EXACT_KEYSTORE_PASSWORD', 'EXACT_KEY_ALIAS', 'EXACT_KEY_PASSWORD'];
  if (bundle || keys.some(key => env[key] !== undefined)) {
    const missing = keys.filter(key => !env[key]);
    if (missing.length) throw new Error(`Android signing needs ${missing.join(', ')}; set all four upload-keystore variables`);
    if (!existsSync(resolve(env.EXACT_KEYSTORE)) || !statSync(resolve(env.EXACT_KEYSTORE)).isFile()) throw new Error('EXACT_KEYSTORE must name an existing keystore file');
  }
  return { ...env, EXACT_BUILD_NUMBER: code, EXACT_VERSION: version,
    ...(env.EXACT_KEYSTORE ? { EXACT_KEYSTORE: resolve(env.EXACT_KEYSTORE) } : {}) };
}

/** Why a production Android build cannot start, or null. Android is the Linux bake, which refuses production trust with
 * an update store (`deploy.store.linux` not "0") and no stream to join (bake/src/receipt.rs): said here, before minutes
 * of Cargo, as the Apple build says its own. */
export function androidStreamRefusal(name, manifest, env = process.env) {
  if (env.EXACT_UPDATE_TRUST !== 'production' || env.EXACT_BAKE_ANALYSIS === '1' || manifest.deploy?.store?.linux === '0') return null;
  if (env.EXACT_UPDATE_RECEIPT || env.EXACT_UPDATE_GENESIS === '1') return null;
  return `${name}: this Android build bakes production trust, and Android uses the Linux bake, whose update store is on (deploy.store.linux is not "0"), so the build belongs to an update stream. For a binary without Exact updates, set deploy.store.linux to "0" in app.json (deploy.store.android does not turn the bake's store off). To ship into a stream, set EXACT_UPDATE_RECEIPT to its authenticated publisher receipt (scripts/deploy.mjs), or EXACT_UPDATE_GENESIS=1 to start a new one.`;
}

/** Whether an APK carries GPU libraries beside the app's (LLP 1009 D6): the primary module or a `gpu.modules` entry. */
export const carriesGpu = (app) => app.hasGpu || gpuModules(app.manifest).length > 0;

/** The app baked for the phone (`buildBake`), the GPU modules first, and each GPU library the bake built, at the
 * path Cargo gave it: a crate's `[lib] name` names the file, and the bake records that name for the host to open. */
export function bakeGpuLibraries(app, options, bake = buildBake) {
  const libraries = [];
  bake(app, 'linux', ANDROID_TARGET, { ...options, kind: 'android', prepareGpu: (product) => { libraries.push(product); return product; } });
  return libraries;
}

/** Each GPU library copied into `jniLibs` under the name its bake recorded, where the host opens it, and the Gradle
 * argument that has the APK extract them and keep their symbols: the host checks each against its baked digest. */
export function stageGpuLibraries(libraries, jniLibs) {
  for (const library of libraries) copyFileSync(library, resolve(jniLibs, basename(library)));
  return libraries.length ? [`-PgpuModules=${libraries.map((library) => basename(library)).join(',')}`] : [];
}

/** The optional sound arm: a separate ELF, never linked into libexactcanvas. */
export function soundLibraryCommand(library, env) {
  const mixer = resolve(READER, '../apple/soundarm'), arm = resolve(READER, 'soundarm');
  const cc = env.CC_aarch64_linux_android, wrapper = /aarch64-linux-android(\d+)-clang\.cmd$/i.exec(cc);
  // spawnSync cannot execute an NDK .cmd wrapper without a shell.
  const compiler = wrapper ? [cc.replace(wrapper[0], 'clang.exe'), `--target=aarch64-linux-android${wrapper[1]}`] : [cc];
  return [...compiler, '-std=gnu11', '-O2', '-fPIC', '-shared', '-Wall', '-Wextra', '-Werror',
    '-Wl,-z,max-page-size=16384', '-Wl,-soname,libexact_sound.so', '-I', mixer,
    resolve(mixer, 'sound_mixer.c'), resolve(arm, 'sound_wav.c'), resolve(arm, 'sound_arm.c'),
    '-laaudio', '-lm', '-o', library];
}

export function stageSoundLibrary(jniLibs, env) {
  const library = resolve(jniLibs, 'libexact_sound.so');
  mkdirSync(jniLibs, { recursive: true });
  const [command, ...args] = soundLibraryCommand(library, env);
  const built = spawnSync(command, args, { env, stdio: 'inherit' });
  if (built.status !== 0) throw new Error('Android sound arm did not build');
  return library;
}

/** The app on the Canvas reader, as an APK: its `android/` crate (a cdylib over `exact_painted::canvas_jni!`; a game's
 * is generated under `.shells/`) built for the phone's ABI and put, with the app's assets and its icon and banner
 * (`androidIcons`), where the Gradle shell packages them (host/android/app), then `:app:assembleRelease`. An app with
 * GPU modules (LLP 1009) is built as a Linux bake is (`buildBake`): the modules first, then the library with their
 * digests baked in, so each ships beside it and must reach the device byte for byte (`stageGpuLibraries`).
 * Gradle wants a JDK (17 or newer) and the SDK's platform. The APK's path, or null. */
export function buildApk(app, env, profile, writeLocks = false, bundle = false) {
  env = androidReleaseEnvironment(env, bundle);
  const java = androidJavaEnv(env);
  if (!java) throw new Error(androidJavaFix(env));
  env = java;
  const crate = app.crate('android');
  if (!app.manifest.game && !crateManifest(app, 'android')) throw new Error(`${app.dir} has no android/ crate, the app's library for the Canvas host: bun exact.mjs update adds one to an app that names android in app.json (apps/shelf/android is one)`);
  // The icon and banner first, so a refused one costs no compile.
  const dirs = apkDirs(app);
  let icons;
  try { icons = androidIcons(app.manifest, app.dir, resolve(dirs.app, 'icons/res')); }
  catch (error) { for (const line of error.message.split('\n')) console.error(`${app.name}: ${line}`); return null; }
  const launch = androidLaunch(app.manifest, resolve(dirs.app, 'launch/res'));
  const built = (name) => resolve(app.target, ANDROID_TARGET, profile, `lib${name.replaceAll('-', '_')}.so`);
  let gpu = [], out = null;
  if (carriesGpu(app)) {
    try { gpu = bakeGpuLibraries(app, { profile, env }); }
    catch (error) { console.error(error.message); return null; }
    out = bakedOut(resolve(app.target, ANDROID_TARGET, profile, 'build'), crate);
  } else {
    // JSON on stdout, for the crate's build-script output directory; the compiler's own messages still on stderr.
    const cargo = spawnSync(...androidCargo('cargo', ['build', ...injectedProfiles(app), ...wholeProgram('android', profile), '--profile', profile, '-p', crate, '--target', ANDROID_TARGET, '--message-format=json-render-diagnostics'], { cwd: app.workspace ?? app.dir, env, stdio: ['inherit', 'pipe', 'inherit'], maxBuffer: 1 << 28 }));
    if (cargo.status !== 0) return null;
    out = planOut(cargo.stdout.toString(), crate);
  }
  const extensions = JSON.parse(readFileSync(resolve(out, 'artifacts.json'), 'utf8')).extensions;
  const bound = androidImplementations(app.dir, extensions);
  const maven = mavenCoordinates(app, bound);
  if (!writeLocks && !mavenLocksPresent(app, maven)) return null;
  let config;
  try { config = packageConfig(app.dir, 'android', { extensions, release: profile === 'release' }); } catch (error) { console.error(`${app.name}: ${error.message}`); return null; }
  const uses = planUses(out);
  // The page-size argument is in every rustflags source the build makes or Cargo was handed (`androidCargo`), but a
  // table in the app's Cargo configuration takes precedence over the build's own and can leave it out.
  for (const library of [built(crate), ...gpu]) {
    const alignment = loadAlignment(library);
    if (alignment < 16384) {
      console.error(`${basename(library)} is ${alignment / 1024} KB aligned, so a device with 16 KB pages runs it in page-size compatibility mode: a rustflags table in the app's Cargo configuration ([target.aarch64-linux-android] or [target.'cfg(…)']) replaced the build's flags; add "${ANDROID_PAGE_SIZE}" to its rustflags`);
      return null;
    }
  }
  const started = new Date().toISOString(), staged = resolve(dirs.app, 'rust');
  rmSync(staged, { recursive: true, force: true });
  mkdirSync(resolve(staged, 'jniLibs/arm64-v8a'), { recursive: true });
  // The reader loads one name whatever the app; the plan names `assets/…` under the APK's `app/`.
  copyFileSync(built(crate), resolve(staged, 'jniLibs/arm64-v8a/libexactcanvas.so'));
  const gpuArgs = stageGpuLibraries(gpu, resolve(staged, 'jniLibs/arm64-v8a'));
  const sound = stageSoundLibrary(resolve(staged, 'jniLibs/arm64-v8a'), env);
  copyStaticTreeIfPresent(resolve(app.dir, 'assets'), resolve(staged, 'assets/app/assets'));
  // The modules' shader text (LLP 1030 D8), which the host registers from `shaders/` under the asset root.
  if (gpu.length) copyShaders(app, resolve(staged, 'assets/app/shaders'));
  // A game's resolved manifest (with its `app` identity) is the one its shells were generated with.
  // A date or time control's picker is Compose Material 3's, linked only for a plan that has one (LLP 1115.001.000 D2).
  const compose = uses.includes('dates') ? ['-Pcompose=1'] : [];
  const development = JSON.parse(readFileSync(resolve(out, 'compat.json'), 'utf8')).embedded?.trust !== 'production';
  // The token names its hatches read (LLP 1121 D12), beside them in the build.
  if (existsSync(resolve(app.dir, 'modules/android'))) writeTamberNames(app, 'kotlin');
  const graph = mavenGraph(app, writeLocks, maven, env);
  if (!graph) return null;
  let assembled;
  try {
    // Without file watching Gradle reads its inputs to decide what is up to date; a watcher that missed an edit
    // shipped the reader's old classes once (LLP 1159.002 D1). A daemon started in the locks' own Gradle home would
    // outlive the home.
    assembled = runGradle(['--no-watch-fs', ...(env.CI || graph.env.GRADLE_USER_HOME ? ['--no-daemon'] : []), bundle ? ':app:bundleRelease' : ':app:assembleRelease', `-Papp=${app.name}`, `-PappDir=${app.manifest.game ? app.workspace : app.dir}`,
      `-PexactDevelopment=${development ? '1' : '0'}`,
      `-PappBuildDir=${dirs.app}`, `-PcanvasBuildDir=${dirs.canvas}`, `-PndkPath=${env.ANDROID_NDK_HOME}`, '--project-cache-dir', dirs.cache, `-Pkotlin.project.persistent.dir=${dirs.kotlin}`,
      ...gpuArgs, ...(icons.icon ? [`-PappIcon=${icons.icon}`] : []), ...(icons.banner ? [`-PappBanner=${icons.banner}`] : []), ...(launch ? [`-PappTheme=${launch}`] : []), ...compose, ...extensionArgs(app, out, config, bound), ...graph.args], { env: { ...env, ANDROID_HOME: androidSdk(env), ...graph.env } }, process.platform, { lockAdvice: maven.length && !writeLocks ? `${app.name}: Gradle locks or verification metadata do not match its dependencies; for an intended graph change, run ${androidLockCommand(app)}, then review and commit gradle.lockfile and gradle-verification.xml` : null });
    if (assembled) graph.kept();
  } finally {
    graph.done();
  }
  if (!assembled) return null;
  const apk = bundle ? resolve(dirs.app, 'outputs/bundle/release/app-release.aab') : readerApk(app);
  const kept = keptLibrary(readFileSync(resolve(staged, 'jniLibs/arm64-v8a/libexactcanvas.so')), zipEntry(readFileSync(apk), `${bundle ? 'base/' : ''}lib/arm64-v8a/libexactcanvas.so`), [dirs.app, dirs.canvas]);
  if (kept) { console.error(`exact android: ${kept}`); return null; }
  const keptSound = keptLibrary(readFileSync(sound), zipEntry(readFileSync(apk), `${bundle ? 'base/' : ''}lib/arm64-v8a/libexact_sound.so`), [dirs.app], 'libexact_sound.so');
  if (keptSound) { console.error(`exact android sound: ${keptSound}`); return null; }
  writeFileSync(resolve(dirname(apk), 'exact-build.json'), JSON.stringify({ started, library: built(crate), workspace: app.workspace ?? app.dir,
    sources: [...READER_SOURCES, ...[resolve(app.dir, 'modules/android')].filter(existsSync)] }) + '\n');
  // What its bake inlined of each setting (LLP 1153 D4), bound to the APK, for a drive to compare with their sources.
  recordBuiltSettings(apk, app, out);
  return apk;
}

/** Where to run the lock-writing command, through the app's launcher or this repository. */
export function androidLockCommand(app) {
  if (existsSync(resolve(app.dir, 'exact.mjs'))) return `in ${quote(app.dir)}: bun exact.mjs android --write-locks`;
  // The name alone finds only `apps/<name>`; any other directory is named in EXACT_APP_DIR, said in words
  // because a shell and PowerShell assign a variable differently.
  const root = resolve(READER, '../..'), named = resolve(app.dir) === resolve(root, 'apps', app.name);
  return `in ${quote(root)}: bun scripts/agent-android.mjs build ${quote(app.name)} --apk --write-locks${named ? '' : `, with EXACT_APP_DIR set to ${quote(app.dir)}`}`;
}

/** App Kotlin and bound packages share one locked graph (LLP 1133.008 D8 item 5). */
export function mavenCoordinates(app, bound) {
  const declarations = [
    { source: `${resolve(app.dir, 'app.json')} host.android.maven`, values: app.manifest?.host?.android?.maven },
    ...bound.map(b => ({ source: `${b.mavenSource} maven`, values: b.maven })),
  ];
  const versions = new Map();
  for (const { source, values } of declarations) {
    const problems = mavenProblems(values, source);
    if (problems.length) throw new Error(problems.join('\n'));
    for (const coordinate of values ?? []) {
      const artifact = coordinate.split(':').slice(0, 2).join(':'), previous = versions.get(artifact);
      if (previous && previous.coordinate !== coordinate) {
        throw new Error(`Maven version conflict: ${previous.source} names ${previous.coordinate}; ${source} names ${coordinate}. Use one version of ${artifact}.`);
      }
      versions.set(artifact, { coordinate, source });
    }
  }
  return [...versions.values()].map(v => v.coordinate).sort();
}

export function mavenLocksPresent(app, maven) {
  if (!maven.length || ['gradle.lockfile', 'gradle-verification.xml'].every(file => existsSync(resolve(app.dir, file)))) return true;
  console.error(`${app.name}: its modules or own Kotlin name Maven libraries (${maven.join(', ')}); before compiling, write gradle.lockfile and gradle-verification.xml by running ${androidLockCommand(app)}, and keep them with the app`);
  return false;
}

/** The Maven libraries the app and bound modules name (LLP 1133.008 D8 item 5), as Gradle arguments: one graph with the
 * reader's own AndroidX, locked to the app's `gradle.lockfile` and verified against its `gradle-verification.xml`,
 * both beside its other locks. The verification file is Gradle's, which it reads only from the reader's `gradle/`:
 * it is put there for this build (`done` takes it away). `writeLocks` writes both from the graph as it resolves
 * (`kept` copies the verification back), in a Gradle user home of its own (`coldGradleHome`, its variables `env`).
 * Missing locks are refused before Gradle runs. */
function mavenGraph(app, writeLocks, maven, env) {
  const placed = resolve(READER, 'gradle/verification-metadata.xml');
  const none = { args: [], env: {}, kept() {}, done() {} };
  if (!maven.length) return none;
  const lock = resolve(app.dir, 'gradle.lockfile'), checksums = resolve(app.dir, 'gradle-verification.xml');
  rmSync(placed, { force: true });
  if (existsSync(checksums)) copyFileSync(checksums, placed);
  const home = writeLocks ? coldGradleHome(env) : null;
  return {
    args: [`-Pmaven=${maven.join(',')}`, `-PmavenLock=${lock}`, ...(writeLocks ? ['--write-locks', '--write-verification-metadata', 'sha256'] : ['--dependency-verification=strict'])],
    env: home ? { GRADLE_USER_HOME: home } : {},
    kept() { if (writeLocks && existsSync(placed)) copyFileSync(placed, checksums); },
    done() { rmSync(placed, { force: true }); if (home) rmSync(home, { recursive: true, force: true }); },
  };
}

/** A new Gradle user home for writing the locks, with nothing resolved in it, as a fresh machine's is. Gradle writes a
 * checksum for each file it fetches, and from a warm cache it fetches fewer than a fresh machine's strict build
 * verifies (a BOM's `.module` beside its `.pom`, another version's parent POM), so checksums written warm fail there.
 * It keeps what is configuration or the tool, never a dependency: the home's `gradle.properties` and init scripts
 * (`init.gradle`, `init.gradle.kts`, `init.d`: proxies, mirrors, credentials, JVM options) copied, and the wrapper's
 * distributions linked, so Gradle is not downloaded again. The caller removes it. */
export function coldGradleHome(env = process.env) {
  const warm = env.GRADLE_USER_HOME || resolve(homedir(), '.gradle');
  const home = mkdtempSync(resolve(tmpdir(), 'exact-gradle-locks-'));
  for (const kept of ['gradle.properties', 'init.gradle', 'init.gradle.kts', 'init.d']) {
    if (existsSync(resolve(warm, kept))) cpSync(resolve(warm, kept), resolve(home, kept), { recursive: true });
  }
  mkdirSync(resolve(warm, 'wrapper/dists'), { recursive: true });
  mkdirSync(resolve(home, 'wrapper'));
  // A junction on Windows, which needs no privilege to make.
  symlinkSync(resolve(warm, 'wrapper/dists'), resolve(home, 'wrapper/dists'), 'junction');
  return home;
}

/** What the modules the app uses add to the APK (LLP 1133.001 D4, D5; LLP 1133.002 D6): each bound implementation's
 * Kotlin with the roster that names them (`exact.app.ExactExtensions`, written here, never committed), and the
 * permissions the app's grant lines derive (the bake's `reach.android`) with those its names derive, and the
 * components its packages' manifest fragments declare with the meta-data `config` (`packageConfig`'s) carries, as
 * Gradle properties. */
function extensionArgs(app, out, config, bound) {
  const generated = resolve(apkDirs(app).app, 'extensions');
  rmSync(generated, { recursive: true, force: true });
  let permissions = [], services = {}, features = [], callbacks = [];
  try {
    const reach = JSON.parse(readFileSync(resolve(out, 'compat.json'), 'utf8')).reach;
    permissions = reach?.android ?? [];
    callbacks = reach?.auth?.callbacks ?? [];
    // The reader's foreground services the lines declare, with their purposes' text (LLP 1133.033 D2).
    services = reach?.androidServices ?? {};
    // Hardware the lines use where it is present, never required (LLP 1133.037 D17: USB host mode).
    features = reach?.androidFeatures ?? [];
  } catch { /* no bake output: none */ }
  // And what the module names the app calls derive beside their lines (`android_entry`'s `permissions.txt`, LLP 1133.018 D11).
  try { permissions = [...new Set([...permissions, ...readFileSync(resolve(out, 'permissions.txt'), 'utf8').split('\n').filter(Boolean)])].sort(); } catch { /* none */ }
  // And the normal permissions the packages' manifest fragments declare (LLP 1133.031 D12 item 1).
  permissions = [...new Set([...permissions, ...fragmentPermissions(bound)])].sort();
  if (bound.length) {
    mkdirSync(resolve(generated, 'exact/app'), { recursive: true });
    writeFileSync(resolve(generated, 'exact/app/ExactExtensions.kt'), ['// Written by scripts/agent-android.mjs (LLP 1133.001 D5); never edited.', 'package exact.app', '',
      'class ExactExtensions : dev.exact.canvas.ExtensionRoster {',
      '    override val extensions: Map<String, (dev.exact.canvas.ExtensionContext) -> dev.exact.canvas.ExactExtension> = mapOf(',
      ...bound.map((b) => `        ${JSON.stringify(b.extension)} to { c -> ${b.className}(c) },`), '    )',
      // Each view a package serves, by the extension that makes it (LLP 1133.001 D1).
      `    override val views: Map<String, String> = mapOf(${bound.flatMap((b) => (b.views ?? []).map((tag) => `${JSON.stringify(tag)} to ${JSON.stringify(b.extension)}`)).join(', ')})`,
      // The extensions made after the reader's first frame, not at their first call (LLP 1133.015 D16 item 1).
      `    override val startAtLoad: List<String> = listOf(${bound.filter((b) => b.start).map((b) => JSON.stringify(b.extension)).join(', ')})`,
      '}', ''].join('\n'));
  }
  // The queries and components the packages' manifest fragments declare, under the allow-list (LLP 1133.030 D9), with
  // the app's privacy policy for an activity that shows it (LLP 1133.026 D4).
  // And the meta-data the packages' configuration carries (LLP 1133.021 §9 item 7): the Maps key, read from the
  // variable `app.json` names, never from the repo. A development build without it says so and builds.
  for (const [name, targets] of Object.entries(app.manifest?.packages ?? {})) for (const [key, value] of Object.entries(targets?.android ?? {})) {
    if (typeof value?.env === 'string' && !process.env[value.env] && config.has(name.split('/').at(-1))) console.warn(`${app.name}: ${value.env} is unset, so ${name}'s ${key} is absent from this development build (a release build refuses it)`);
  }
  // The share target's filters on the app's activity (LLP 1133.024 D5), inside the same `<application>`.
  const fragment = withAuthCallbacks(withMetaData(manifestFragment(bound, { privacyPolicy: app.manifest?.privacy_policy ?? null }), [...configMetaData(config), ...(androidShareActivity(app.manifest?.share_target) ?? [])]), callbacks), fragmentFile = resolve(apkDirs(app).app, 'manifest-fragment.xml');
  rmSync(fragmentFile, { force: true });
  if (fragment) { mkdirSync(dirname(fragmentFile), { recursive: true }); writeFileSync(fragmentFile, fragment); }
  return [...(bound.length ? [`-PextensionDirs=${[...bound.flatMap((b) => b.dirs), generated].join(',')}`] : []), ...(permissions.length ? [`-Ppermissions=${permissions.join(',')}`] : []),
    ...(fragment ? [`-PmanifestFragment=${fragmentFile}`] : []), ...(Object.keys(services).length ? [`-PforegroundServices=${JSON.stringify(services)}`] : []),
    ...(features.length ? [`-PandroidFeatures=${features.join(',')}`] : [])];
}

/** `--install` (`--run`: and launch it): the APK onto the device `androidDevice` names. With none ready the build
 * stands and says how to install it; with several and no ANDROID_SERIAL it refuses. The exit code. */
function installApk(app, apk, { run = false, built = 0, installed: onInstalled = () => {} } = {}) {
  let serial;
  try { serial = androidDevice(); } catch (error) {
    if (/^no Android device/.test(error.message)) { console.error(`built in ${(built / 1000).toFixed(1)} s; not installed: ${error.message}`); return run ? 1 : 0; }
    console.error(error.message);
    return 1;
  }
  const id = app.id.replaceAll('-', '_') + (process.env.ORG_GRADLE_PROJECT_idSuffix ?? ''), started = Date.now();
  const installed = spawnSync(adb(), ['-s', serial, 'install', '-r', apk], { encoding: 'utf8' });
  if (installed.status !== 0 || /^Failure/m.test(installed.stdout)) { console.error(`adb install: ${(installed.stderr || installed.stdout).trim()}`); return 1; }
  console.error(`built in ${(built / 1000).toFixed(1)} s, installed on ${serial} as ${id} in ${((Date.now() - started) / 1000).toFixed(1)} s`);
  onInstalled(serial);
  if (!run) return 0;
  const launched = spawnSync(adb(), ['-s', serial, 'shell', 'am', 'start', '-W', '-n', `${id}/dev.exact.app.MainActivity`], { encoding: 'utf8' });
  if (launched.status !== 0 || /Error/.test(launched.stdout + launched.stderr)) { console.error(`am start: ${(launched.stderr || launched.stdout).trim()}`); return 1; }
  return 0;
}

/** `verify android`'s build (scripts/verify.mjs): the APK, built when it is missing or older than its sources, as a
 * drive would refuse it. Null when fresh, else what the build said (its last lines when it failed). */
export async function apkBuildFirst(app) {
  const { apkChanges } = await import('./agent-stdio.mjs'), { bakedSettingChanges } = await import('./agent-launch.mjs'), apk = readerApk(app);
  const why = !existsSync(apk) ? 'no APK yet' : apkChanges(app, apk).length ? 'the APK is older than its sources'
    : bakedSettingChanges(apk, app).length ? 'a setting changed since the APK was built' : null;
  if (!why) return null;
  console.error(`building the APK (${why}): bun exact.mjs android`);
  const started = Date.now(), r = spawnSync(process.execPath, [import.meta.path, 'build', app.dir, '--apk'], { encoding: 'utf8', maxBuffer: 256 << 20 });
  const lines = `${r.stdout ?? ''}${r.stderr ?? ''}`.split('\n').filter((l) => l.trim());
  return { ms: Date.now() - started, code: r.status ?? 1, said: r.status ? lines.slice(-20) : [`built the APK in ${((Date.now() - started) / 1000).toFixed(1)} s`], warnings: [] };
}

/** `verify android`'s look at a screen: the host has no `check` observation yet, so each scheme's screen is captured,
 * `unchecked`, as Windows' is (the device's pixels, shrunk to a pixel a point as check keeps an iOS capture). */
export async function androidCaptures(s, { screen, shot, failed }) {
  const [{ decodePng, encodePng, shrink }, { tmpdir }] = await Promise.all([import('./png.mjs'), import('node:os')]);
  const seen = (await s.layout().catch(() => null))?.viewport ?? {}, vp = { w: Math.round(seen.w), h: Math.round(seen.h) }, runs = [];
  for (const scheme of ['light', 'dark']) {
    await s.prefer({ 'prefers-color-scheme': scheme });
    await s.clock('settle');
    const path = shot?.(vp, scheme) ?? resolve(tmpdir(), `exact-verify-${process.pid}-${scheme}.png`);
    await s.carrier.screenshot(path);
    const png = decodePng(readFileSync(path)), scale = vp.w ? png.width / vp.w : 1;
    if (scale > 1) writeFileSync(path, encodePng(shrink(png, Math.round(png.width / scale), Math.round(png.height / scale))));
    runs.push({ screen, size: [vp.w, vp.h], scheme, asked: scheme, unchecked: true, ...(shot?.(vp, scheme) ? { shot: path } : {}), findings: [], accepted: [], lines: failed, hidden: 0, advice: [] });
  }
  await s.prefer({ 'prefers-color-scheme': 'light' });
  return runs;
}

if (import.meta.main && process.argv[2] === 'reverse') {
  const [action, tool, serial, encodedLease, ports] = process.argv.slice(3), lease = JSON.parse(encodedLease);
  try {
    const value = action === 'acquire' ? acquireReverse(tool, serial, lease, JSON.parse(ports)) : releaseReverseState(tool, serial, lease);
    console.log(JSON.stringify({ value }));
  } catch (error) { console.log(JSON.stringify({ error: error.message })); }
} else if (import.meta.main && process.argv[2] === 'headless') {
  await headless(process.argv.slice(3)).catch((e) => { console.error(e.message); process.exit(1); });
} else if (import.meta.main && process.argv[2] === 'relay') {
  // The reader carrier's own process (`readerDeploy`): not a verb a person runs.
  await relay(process.argv.slice(3)).catch((e) => { console.error(e.message); process.exit(1); });
} else if (import.meta.main) {
  const args = guardFlags('android', process.argv.slice(2));
  const [verb, name] = args.filter((arg) => !arg.startsWith('--'));
  if (verb !== 'build' || !name) {
    console.error('usage: bun scripts/agent-android.mjs build <app> [--apk [--install | --run]] [--headless]   (then: bun scripts/agent.mjs android [--reader] --app <app> …; --apk: the app on the Canvas reader, host/android/README.md; --headless: the Linux host for Android, never the APK)');
    process.exit(2);
  }
  // `--headless` wins: an app's `bun exact.mjs android --headless` asks for the Linux host its verb's `--apk` would replace.
  const bundle = args.includes('--bundle');
  if (bundle && (args.includes('--headless') || args.includes('--run'))) throw new Error('--bundle takes neither --headless nor --run');
  if (args.includes('--no-install') && args.includes('--run')) throw new Error('--no-install takes no --run');
  if (bundle) process.env.EXACT_UPDATE_TRUST = 'production';
  const apkWanted = (bundle || args.includes('--apk')) && !args.includes('--headless');
  // The pinned toolchain's targets: the app's workspace builds with it, and rustup in the app's own directory, which
  // pins none, would answer for its default.
  // Without rustup (a toolchain installed by hand) Cargo names a missing target itself.
  const rustup = spawnSync('rustup', ['target', 'list', '--installed', '--toolchain', PINNED_RUST], { encoding: 'utf8' });
  if (rustup.status === 0 && !rustup.stdout.split('\n').includes(ANDROID_TARGET)) {
    console.error(`the pinned toolchain ${PINNED_RUST} has no ${ANDROID_TARGET} target: rustup target add ${ANDROID_TARGET} --toolchain ${PINNED_RUST}`);
    process.exit(1);
  }
  const app = resolveApp(name);
  const unstreamed = androidStreamRefusal(app.name, app.manifest);
  if (unstreamed) { console.error(unstreamed); process.exit(1); }
  // A TypeScript app's library links the Android Hermes bundle: said before minutes of compiling, not at the link.
  const typescript = androidUsesHermes(app, apkWanted ? 'android' : 'linux'), hermes = androidHermesDir();
  if (!hermes && typescript) {
    console.error(`${app.name}'s TypeScript needs the Android Hermes bundle, which is not installed: ${hermesBundle(ANDROID_TARGET).fix} (once; exact setup does it where the Android SDK is)`);
    process.exit(1);
  }
  // An executable resolves every symbol at its link, so the headless host with Hermes links at the bundle's API level
  // when that is above `API`. The APK's library is a shared object, whose symbols the device resolves when it loads it.
  const linkApi = !apkWanted && typescript ? Math.max(API, hermesApi(hermes) ?? API) : API;
  // With what the bake watches named (`bakeWatchEnv`), as every baking build.
  const env = { ...androidToolchainEnv(process.env, process.platform, linkApi), ...bakeWatchEnv(app), CARGO_TARGET_DIR: app.target };
  if (apkWanted) {
    const started = Date.now();
    const apk = buildApk(app, env, bundle || args.includes('--release') ? 'release' : HOST_DEV, args.includes('--write-locks'), bundle);
    if (!apk) process.exit(1);
    console.log(apk);
    // `--json`'s object once the install is done: `installed` names the device, or is null when none was ready.
    const trust = process.env.EXACT_UPDATE_TRUST ?? 'development', install = !bundle && !args.includes('--no-install') && (args.includes('--install') || args.includes('--run'));
    let serial = null;
    const code = install ? installApk(app, apk, { run: args.includes('--run'), built: Date.now() - started, installed: (s) => { serial = s; } }) : 0;
    if (!code && args.includes('--json')) reportBuilt({ path: apk, target: 'android', trust, ...(install ? { installed: serial } : {}) });
    process.exit(code);
  }
  const [cmd, ...rest] = androidBuild(app);
  const built = spawnSync(...androidCargo(cmd, rest, { cwd: app.workspace ?? app.dir, env, stdio: 'inherit' }));
  if (built.status === 0) { recordBuiltSettings(androidBinary(app), app); console.log(androidBinary(app)); if (args.includes('--json')) reportBuilt({ path: androidBinary(app), target: 'android', trust: process.env.EXACT_UPDATE_TRUST ?? 'development' }); }
  process.exit(built.status ?? 1);
}
