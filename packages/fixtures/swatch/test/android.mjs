// @fixture/swatch on Android: the fixture app made outside the repo and built
// as the Canvas reader's APK, which compiles the package's Kotlin with the
// roster the build writes (`exact.app.ExactExtensions`: its `views`, and its
// `config`, which `ExtensionContext.config` reads). The key the configuration
// carries reaches no build output and no manifest, and a release build
// without it is refused `swatch-key-missing`.
//
// With `--emulator <serial>` it then installs the APK on that running
// emulator and drives it under the agent: the package's Kotlin reports the
// configuration it read, and the key is in no tree, state or log. Otherwise it
// drives nothing; checklist.md says what a person checks on a phone.
//
// usage: bun packages/fixtures/swatch/test/android.mjs [--emulator <serial>]
//   (an NDK of clang 18 or newer in ANDROID_NDK_HOME, and a JDK 17 or newer in JAVA_HOME)
import { existsSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { LEAK, checker, makeFixture, reportedConfig, root, run } from './fixture.mjs';
import { apkBuildDir } from '../../../../scripts/agent-android.mjs';
import { androidPackageId, resolveApp } from '../../../../scripts/app.mjs';

const argv = process.argv.slice(2), emulator = argv.includes('--emulator') ? argv[argv.indexOf('--emulator') + 1] : null;
const { app } = makeFixture(), name = basename(app);
run('bun', ['exact.mjs', 'lock'], app);
const { check, failed } = checker();
const env = { ...process.env, EXACT_APP_DIR: app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1' };
const build = (flags, more = {}) => spawnSync('bun', [resolve(root, 'scripts/agent-android.mjs'), 'build', name, '--apk', ...flags], { cwd: root, env: { ...env, ...more }, encoding: 'utf8', maxBuffer: 1 << 28 });
try {
  const built = build([]), said = built.stdout + built.stderr;
  check('android', 'the APK builds', built.status === 0, said.slice(-3000));
  const out = apkBuildDir(app), generated = resolve(out, 'extensions/exact/app/ExactExtensions.kt');
  const roster = existsSync(generated) ? readFileSync(generated, 'utf8') : '';
  check('android', 'the roster names the package\'s view and its extension', roster.includes('"swatch-box" to "swatch"'), roster || `no ${generated}`);
  check('android', 'the roster carries the extension\'s configuration', /override val config: Map<String, String> = mapOf\("swatch" to "\{/.test(roster), roster);
  check('android', 'the build says nothing of the key', !said.includes(LEAK), 'the key is in the build output');
  const manifests = existsSync(out) ? readdirSync(out, { recursive: true }).map(String).filter((f) => f.endsWith('AndroidManifest.xml') || f.endsWith('manifest-fragment.xml')) : [];
  check('android', 'no manifest carries the key', manifests.length > 0 && manifests.every((f) => !readFileSync(resolve(out, f), 'utf8').includes(LEAK)), manifests.join(', ') || 'no manifest');
  if (emulator) await drive();
  const refused = build(['--release'], { SWATCH_KEY: '' });
  check('android', 'without the variable a release build is refused swatch-key-missing', refused.status !== 0 && refused.stderr.includes('swatch-key-missing'), (refused.stdout + refused.stderr).slice(-1500));
} finally {
  rmSync(dirname(app), { recursive: true, force: true });
}
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);

async function drive() {
  const adb = (...a) => spawnSync('adb', ['-s', emulator, ...a], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout ?? '';
  Object.assign(process.env, env, { ANDROID_SERIAL: emulator, EXACT_ANDROID_READER: '1' });
  const { open } = await import(resolve(root, 'scripts/agent.mjs'));
  adb('logcat', '-c');
  const s = await open({ host: 'android', app: name });
  try {
    const texts = async () => Object.fromEntries(((await s.tree()).nodes ?? []).filter((n) => n.props?.testId).map((n) => [n.props.testId, n.props.text ?? '']));
    let seen = {};
    for (const end = Date.now() + 15000; Date.now() < end && !seen.config; await new Promise((done) => setTimeout(done, 300))) seen = await texts();
    check('emulator', "the package's Kotlin reads its configuration", seen.config === reportedConfig('android'), JSON.stringify(seen));
    const said = JSON.stringify([await s.tree(), await s.state(), await s.logs({ all: true })]) + adb('logcat', '-d');
    check('emulator', 'the key is in no tree, state or log', !said.includes(LEAK), 'the key is in the drive\'s output');
  } finally {
    await s.close();
  }
  // The id the build installed under, its development variant's.
  adb('uninstall', androidPackageId(resolveApp(name).manifest));
}
