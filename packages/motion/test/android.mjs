// @exact/motion on Android (LLP 1133.013 stage 4): the fixture app made outside the repo and built as the Canvas
// reader's APK, which compiles the package's Kotlin, and the roster that names its module. It drives nothing: the
// emulator's sensors are virtual and the agent's substitute answers under it; phone-checklist.md is the phone's.
//
// usage: bun packages/motion/test/android.mjs
//   (an NDK of clang 18 or newer in ANDROID_NDK_HOME, and a JDK 17 or newer in JAVA_HOME)
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { checker, makeFixture, root } from './fixture.mjs';
import { apkBuildDir } from '../../../scripts/agent-android.mjs';

const { app } = makeFixture(), name = basename(app);
const { check, failed } = checker();
const env = { ...process.env, EXACT_APP_DIR: app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1' };
try {
  const built = spawnSync('bun', [resolve(root, 'scripts/agent-android.mjs'), 'build', name, '--apk'], { cwd: root, env, encoding: 'utf8', maxBuffer: 1 << 28 });
  check('android', 'the APK builds, compiling Motion.kt', built.status === 0, (built.stdout + built.stderr).slice(-2000));
  const out = apkBuildDir(app), generated = resolve(out, 'extensions/exact/app/ExactExtensions.kt');
  const roster = existsSync(generated) ? readFileSync(generated, 'utf8') : '';
  check('android', 'the roster names the motion module', roster.includes('exact.motion.MotionModule'), roster || `no ${generated}`);
} finally {
  rmSync(dirname(app), { recursive: true, force: true });
}
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
