// @exact/device on Android (LLP 1133.031 §12 stage 3): the fixture app made outside the repo and built as the Canvas
// reader's APK, which compiles Device.kt and merges the implementation's manifest fragment. No emulator boots on the
// machine this was built on, so it drives nothing: the device checklist (`checklist.md`) holds the shell commands that
// set each fact on an emulator or a phone.
//
// - The roster names `exact.device.DeviceModule`.
// - The merged manifest holds `ACCESS_NETWORK_STATE`, from the fragment (D8); the fragment's list refuses a
//   permission that is not normal (`scripts/android-manifest.test.mjs`).
//
// usage: bun packages/device/test/android.mjs
//   (an NDK of clang 18 or newer in ANDROID_NDK_HOME, and a JDK 17 or newer in JAVA_HOME)
import { existsSync, readdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { checker, makeFixture, root } from './fixture.mjs';
import { apkBuildDir } from '../../../scripts/agent-android.mjs';

const { app } = makeFixture(), name = basename(app);

/** Every file under `dir` named `file`. */
const found = (dir, file) => !existsSync(dir) ? [] : readdirSync(dir).flatMap((n) => {
  const at = join(dir, n);
  return statSync(at).isDirectory() ? found(at, file) : n === file ? [at] : [];
});

const { check, failed } = checker();
const env = { ...process.env, EXACT_APP_DIR: app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1' };
const out = apkBuildDir(app);
try {
  const built = spawnSync('bun', [resolve(root, 'scripts/agent-android.mjs'), 'build', name, '--apk'], { cwd: root, env, encoding: 'utf8', maxBuffer: 1 << 28 });
  check('android', 'the APK builds, compiling Device.kt', built.status === 0, (built.stdout + built.stderr).slice(-3000));
  const generated = resolve(out, 'extensions/exact/app/ExactExtensions.kt');
  const roster = existsSync(generated) ? readFileSync(generated, 'utf8') : '';
  check('android', 'the roster names the device module', roster.includes('exact.device.DeviceModule'), roster || `no ${generated}`);
  const manifests = found(resolve(out, 'intermediates'), 'AndroidManifest.xml').filter((p) => /merged_manifest/.test(p));
  const merged = manifests.map((p) => readFileSync(p, 'utf8')).join('\n');
  check('android', 'the merged manifest holds ACCESS_NETWORK_STATE', merged.includes('"android.permission.ACCESS_NETWORK_STATE"'), manifests.join(', ') || `no merged manifest under ${out}`);
} finally {
  rmSync(out, { recursive: true, force: true });
  rmSync(dirname(app), { recursive: true, force: true });
}
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
