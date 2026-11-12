// An Android Hermes bundle built from an Ibex checkout, for work on Ibex itself
// (LLP 1107). An app's library otherwise links the pinned Ibex release's
// `aarch64-linux-android` bundle, which `exact setup` installs. This builds one
// with Ibex's own release script (`aarch64-linux-android` in
// scripts/build-hermes-vanilla-release.sh): the pinned Hermes commit, the NDK,
// static ICU without JNI, and the receipt the
// `hermes-lean-sys` resolver verifies. The bundle unpacks into
// ~/.cache/exact/hermes-android/<archive sha256>/, `current` names the newest,
// and `agent-android.mjs build` hands it to Cargo as
// HERMES_LEAN_SYS_DIR_aarch64_linux_android in place of the pinned one, for the
// target only, so the host's own pinned bundle is untouched.
//
//   bun scripts/hermes-android.mjs build              IBEX_DIR: an Ibex checkout with the Android target
//   bun scripts/hermes-android.mjs install <tar.gz>   cache a bundle built elsewhere
//   bun scripts/hermes-android.mjs path               the installed bundle, or nothing
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readlinkSync, renameSync, rmSync, symlinkSync } from 'node:fs';
import { homedir } from 'node:os';
import { resolve } from 'node:path';

export const HERMES_ANDROID_CACHE = resolve(process.env.EXACT_HERMES_ANDROID_CACHE ?? resolve(homedir(), '.cache/exact/hermes-android'));
export const ANDROID_MIN_API = 29;

/** The installed bundle's directory, or null. */
export function hermesAndroidBundle() {
  const current = resolve(HERMES_ANDROID_CACHE, 'current');
  if (!existsSync(current)) return null;
  const root = resolve(HERMES_ANDROID_CACHE, readlinkSync(current));
  return existsSync(resolve(root, 'hermes-input-receipt.json')) ? root : null;
}

const hasAndroidTarget = (dir) => {
  const script = resolve(dir, 'scripts/build-hermes-vanilla-release.sh');
  return existsSync(script) && readFileSync(script, 'utf8').includes('aarch64-linux-android)');
};

function build() {
  const named = process.env.IBEX_DIR;
  if (named && !hasAndroidTarget(named)) throw new Error(`IBEX_DIR ${named} has no scripts/build-hermes-vanilla-release.sh with the aarch64-linux-android target`);
  const candidates = named ? [named] : [resolve(homedir(), 'projects/ibex2-android-hermes'), resolve(homedir(), 'projects/ibex2')];
  const ibex = candidates.find(hasAndroidTarget);
  if (!ibex) throw new Error(`no Ibex checkout whose scripts/build-hermes-vanilla-release.sh has the aarch64-linux-android target (set IBEX_DIR; looked in ${candidates.join(', ')})`);
  mkdirSync(HERMES_ANDROID_CACHE, { recursive: true });
  // Everything a run writes before publishing is its own: concurrent builds never share a path.
  const work = mkdtempSync(resolve(HERMES_ANDROID_CACHE, '.build-'));
  try {
    const archive = resolve(work, 'hermes-vanilla-aarch64-linux-android.tar.gz');
    const built = spawnSync('bash', ['scripts/build-hermes-vanilla-release.sh', 'aarch64-linux-android', archive], {
      cwd: ibex, stdio: 'inherit', env: { ...process.env, IBEX_ANDROID_API: String(ANDROID_MIN_API) },
    });
    if (built.status !== 0) process.exit(built.status ?? 1);
    console.log(install(archive, work));
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

/** Publish a built bundle archive: unpack it as its digest's directory, then point `current` at it. */
function install(archive, work) {
  const digest = createHash('sha256').update(readFileSync(archive)).digest('hex');
  const root = resolve(HERMES_ANDROID_CACHE, digest);
  // A published digest directory is never replaced: the same digest is the same bundle.
  if (!existsSync(root)) {
    const staging = resolve(work, 'bundle');
    mkdirSync(staging);
    const unpacked = spawnSync('tar', ['-xzf', archive, '-C', staging], { stdio: 'inherit' });
    if (unpacked.status !== 0) process.exit(unpacked.status ?? 1);
    try { renameSync(staging, root); } catch (error) { if (!existsSync(root)) throw error; }
  }
  // `current` moves by renaming a fresh link over it, so a reader always finds one.
  const link = resolve(work, 'current');
  symlinkSync(digest, link);
  renameSync(link, resolve(HERMES_ANDROID_CACHE, 'current'));
  return root;
}

function installArchive(archive) {
  mkdirSync(HERMES_ANDROID_CACHE, { recursive: true });
  const work = mkdtempSync(resolve(HERMES_ANDROID_CACHE, '.install-'));
  try {
    // Hash and unpack one private copy, never the caller's path twice: a file
    // replaced between the two reads would publish its bytes under the other's digest.
    const snapshot = resolve(work, 'archive.tar.gz');
    copyFileSync(resolve(archive), snapshot);
    console.log(install(snapshot, work));
  } finally { rmSync(work, { recursive: true, force: true }); }
}

if (import.meta.main) {
  const verb = process.argv[2];
  if (verb === 'build') build();
  else if (verb === 'install' && process.argv[3]) installArchive(process.argv[3]);
  else if (verb === 'path') console.log(hermesAndroidBundle() ?? '');
  else { console.error('usage: bun scripts/hermes-android.mjs build | install <archive.tar.gz> | path'); process.exit(2); }
}
