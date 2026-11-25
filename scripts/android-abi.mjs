// Android's APK ABIs and their NDK toolchains. Hermes overrides are target-specific.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, resolve } from 'node:path';
import { cargoEnvironment, hermesBundle } from './app.mjs';
import { ANDROID_MIN_API as API, hermesAndroidBundle } from './hermes-android.mjs';
import { androidBakeBlobs } from './linked-plan.mjs';

export const ANDROID_TARGET = 'aarch64-linux-android';
export const ANDROID_ABIS = { 'arm64-v8a': ANDROID_TARGET, x86_64: 'x86_64-linux-android' };
export function androidAbis(value = 'arm64-v8a') {
  const abis = [...new Set(value.split(','))];
  if (abis.some(abi => !Object.hasOwn(ANDROID_ABIS, abi))) throw new Error('--abis takes arm64-v8a, x86_64, or arm64-v8a,x86_64');
  return abis;
}
export const androidSdk = (env = process.env) => env.ANDROID_HOME ?? env.ANDROID_SDK_ROOT
  ?? resolve(homedir(), process.platform === 'darwin' ? 'Library/Android/sdk' : 'Android/Sdk');

/** The environment `androidBuild` needs: the NDK's clang as the target's linker and C compiler, the compiler at `API`
 * and the linker at `linkApi`. On a Windows host the NDK's prebuilt folder is `windows-x86_64`, its compilers are
 * `.cmd` scripts and its tools `.exe`. */
export function androidToolchainEnv(env = process.env, platform = process.platform, linkApi = API, target = ANDROID_TARGET) {
  let ndk = env.ANDROID_NDK_HOME;
  if (!ndk) {
    const root = resolve(androidSdk(env), 'ndk');
    const versions = existsSync(root) ? readdirSync(root).filter((v) => /^\d/.test(v)).sort((a, b) => a.localeCompare(b, undefined, { numeric: true })) : [];
    if (!versions.length) throw new Error(`no Android NDK: install one with the SDK manager (ndk;27.1.12297006), or set ANDROID_NDK_HOME (looked in ${root})`);
    ndk = resolve(root, versions.at(-1));
  }
  const windows = platform === 'win32', script = windows ? '.cmd' : '', exe = windows ? '.exe' : '';
  const bin = resolve(ndk, 'toolchains/llvm/prebuilt', { darwin: 'darwin-x86_64', win32: 'windows-x86_64' }[platform] ?? 'linux-x86_64', 'bin');
  const cc = resolve(bin, `${target}${API}-clang${script}`), linker = resolve(bin, `${target}${linkApi}-clang${script}`);
  for (const tool of [cc, linker]) if (!existsSync(tool)) throw new Error(`the NDK at ${ndk} has no ${basename(tool)}`);
  // The pinned bundle is named only against a developer's HERMES_LEAN_SYS_DIR: hermes-lean-sys checks a bundle it finds
  // itself against the pin's digest, and takes a named one as it is.
  const key = target.replaceAll('-', '_'), local = target === ANDROID_TARGET ? hermesAndroidBundle() : null;
  const hermes = env[`HERMES_LEAN_SYS_DIR_${key}`] || local || (env.HERMES_LEAN_SYS_DIR === undefined ? null : androidHermesDir(env, local, target));
  return { ...cargoEnvironment(env), ANDROID_NDK_HOME: ndk, [`CARGO_TARGET_${key.toUpperCase()}_LINKER`]: linker, [`CC_${key}`]: cc,
    [`CXX_${key}`]: resolve(bin, `${target}${API}-clang++${script}`), [`AR_${key}`]: resolve(bin, `llvm-ar${exe}`),
    // Match Hermes's static C++ runtime; c++_shared would need another library in the APK.
    [`CXXSTDLIB_${key}`]: 'c++_static', ...(hermes ? { [`HERMES_LEAN_SYS_DIR_${key}`]: hermes } : {}) };
}

/** A TypeScript data module's lean Hermes VM, for the target only (the host keeps its pinned one): the bundle
 * HERMES_LEAN_SYS_DIR_<target> names, else the arm64 one `hermes-android.mjs` installed, else the pinned
 * release's when it is installed (null when it is not). The pinned one is looked for without HERMES_LEAN_SYS_DIR, a
 * host bundle a developer names, which would otherwise answer for Android too. */
export function androidHermesDir(env = process.env, local = hermesAndroidBundle(), target = ANDROID_TARGET) {
  const named = env[`HERMES_LEAN_SYS_DIR_${target.replaceAll('-', '_')}`] ?? (target === ANDROID_TARGET ? local : null);
  if (named) return named;
  const { HERMES_LEAN_SYS_DIR, ...unhosted } = env;
  try {
    const pinned = hermesBundle(target, unhosted);
    return pinned.installed ? pinned.root : null;
  } catch (error) {
    if (error.message !== `hermes-lean-sys has no pinned bundle for ${target}`) throw error;
    return null;
  }
}

/** The Canvas build and the APK use the same free name beside the GPU libraries. */
export function androidBakeLibraryName(libraries) {
  const used = new Set(libraries.map(library => library.toLowerCase()));
  let name = 'libexactbaked.so';
  for (let index = 1; used.has(name); index++) name = `libexactbaked_${index}.so`;
  return name;
}

/** Link the baked data into a read-only image the APK's native linker can map. */
export function stageAndroidBake(out, jniLibs, toolchain, target, widgets, name) {
  const blobs = [
    ...androidBakeBlobs(out).map(file => ['exact_' + file.replaceAll('.', '_'), file]),
    ...widgets.map(({ kind }) => ['exact_widget_' + kind, `plans/${kind}.plan`]),
  ];
  const assembly = ['.section .rodata,"a",%progbits', ...blobs.flatMap(([symbol, file], i) => [
    '.balign 16', `.globl "${symbol}"`, `"${symbol}":`, `.quad .Lend${i} - .Lstart${i}, 0`,
    `.Lstart${i}:`, `.incbin "${file}"`, `.Lend${i}:`,
  ])].join('\n') + '\n';
  const library = resolve(jniLibs, name);
  const wrapper = toolchain[`CC_${target.replaceAll('-', '_')}`];
  const clang = resolve(wrapper, '..', process.platform === 'win32' ? 'clang.exe' : 'clang');
  const args = [`--target=${target}${API}`, '-shared', '-nostdlib', '-Wl,-z,max-page-size=16384',
    `-Wl,-soname,${name}`, '-x', 'assembler', '-', '-o', library];
  const built = spawnSync(clang, args, { cwd: out, input: assembly, encoding: 'utf8' });
  if (built.status !== 0) throw new Error(`Android bake data link failed: ${built.error?.message ?? built.stderr.trim()}`);
  return library;
}

/** The optional sound arm: a separate ELF, never linked into libexactcanvas. */
export function soundLibraryCommand(library, env, target = ANDROID_TARGET) {
  const mixer = resolve(import.meta.dir, '../host/apple/soundarm'), arm = resolve(import.meta.dir, '../host/android/soundarm');
  const cc = env[`CC_${target.replaceAll('-', '_')}`], wrapper = /((?:aarch64|x86_64)-linux-android\d+)-clang\.cmd$/i.exec(cc);
  // spawnSync cannot execute an NDK .cmd wrapper without a shell.
  const compiler = wrapper ? [cc.replace(wrapper[0], 'clang.exe'), `--target=${wrapper[1]}`] : [cc];
  return [...compiler, '-std=gnu11', '-O2', '-fPIC', '-shared', '-Wall', '-Wextra', '-Werror',
    '-Wl,-z,max-page-size=16384', '-Wl,-soname,libexact_sound.so', '-I', mixer,
    resolve(mixer, 'sound_mixer.c'), resolve(arm, 'sound_wav.c'), resolve(arm, 'sound_arm.c'),
    '-laaudio', '-lm', '-o', library];
}

export function stageSoundLibrary(jniLibs, env, target = ANDROID_TARGET) {
  const library = resolve(jniLibs, 'libexact_sound.so');
  mkdirSync(jniLibs, { recursive: true });
  const [command, ...args] = soundLibraryCommand(library, env, target);
  const built = spawnSync(command, args, { env, stdio: 'inherit' });
  if (built.status !== 0) throw new Error('Android sound arm did not build');
  return library;
}
