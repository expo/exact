// Android's APK ABIs and their NDK toolchains. Hermes overrides are target-specific.
import { existsSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, resolve } from 'node:path';
import { cargoEnvironment, hermesBundle } from './app.mjs';
import { ANDROID_MIN_API as API, hermesAndroidBundle } from './hermes-android.mjs';

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
  // The pinned release has no x86_64 Android bundle; never substitute the arm64 or host bundle.
  if (target !== ANDROID_TARGET) return null;
  const { HERMES_LEAN_SYS_DIR, ...unhosted } = env, pinned = hermesBundle(target, unhosted);
  return pinned.installed ? pinned.root : null;
}
