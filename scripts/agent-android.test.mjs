// The Android carrier's reader variables, drive setup and reverse mappings (agent-android.mjs).
// Stand-in adb devices exercise relay cleanup without an emulator or a device.
import { afterAll, test, expect, spyOn } from 'bun:test';
import { ChildProcess, spawn, spawnSync } from 'node:child_process';
import { once, EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { pollUntil } from './agent-launch.mjs';
import { androidJavaEnv, androidSdk, androidSdkPackages, androidScreenState, androidPermissionPrompt, reverseRoot } from './agent-android.mjs';

const focusCommand = "dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'";
const screenFixtures = resolve(import.meta.dir, 'fixtures/android-screen');
const screenDump = (state, kind) => readFileSync(resolve(screenFixtures, `${state}.${kind}.txt`), 'utf8');

test('Android names JDK-8335662 only for affected AArch64 JDK versions', async () => {
  const { androidJdkRisk } = await import('./agent-android.mjs');
  const settings = (version, arch = 'aarch64') => `Property settings:\n    java.version = ${version}\n    os.arch = ${arch}\n`;
  for (const version of ['17', '17.0.6', '17.0.15+6', '18.0.2', '19', '20.0.2', '21', '21.0.7', '22', '23.0.2']) {
    const risk = androidJdkRisk(settings(version));
    expect(risk).toContain(`JDK ${version} on aarch64`);
    expect(risk).toContain('JDK-8335662');
    expect(risk).toContain('17.0.16');
    expect(risk).toContain('21.0.8');
    expect(risk).toContain('24');
    expect(risk).toContain('JAVA_HOME');
  }
  for (const version of ['11.0.20', '16.0.2', '17.0.16', '17.0.16+8', '17.0.17', '21.0.8', '21.0.8.1', '24', '25.0.1'])
    expect(androidJdkRisk(settings(version))).toBeNull();
  for (const arch of ['x86_64', 'amd64', 'x86', 'arm']) expect(androidJdkRisk(settings('17.0.6', arch))).toBeNull();
  expect(androidJdkRisk(settings('21.0.7', 'arm64'))).toContain('arm64');
  for (const text of ['', 'no properties', settings('unknown'), 'java.version = 17.0.6']) expect(androidJdkRisk(text)).toBeNull();
});

test.skipIf(process.platform === 'win32')('setup and Gradle name the selected affected JDK before running Gradle', async () => {
  const { sdkReport, platformReport } = await import('./exact.mjs');
  const { bringPlatform } = await import('./app-shells.mjs');
  const { runGradle } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-jdk-risk-'));
  const errors = spyOn(console, 'error').mockImplementation(() => {});
  try {
    mkdirSync(resolve(dir, 'bin'));
    const java = version => writeFileSync(resolve(dir, 'bin/java'), `#!/bin/sh\necho 'openjdk version "${version}"' >&2\necho '    java.version = ${version}' >&2\necho '    os.arch = aarch64' >&2\n`, { mode: 0o755 });
    writeFileSync(resolve(dir, 'bin/javac'), '#!/bin/sh\nexit 0\n', { mode: 0o755 });
    const env = { ...process.env, JAVA_HOME: dir, JAVA_TOOL_OPTIONS: '-Xint' };
    java('17.0.6');
    const rows = sdkReport(env, { only: 'android' });
    const risk = rows.find(row => row.name === 'Android JDK JIT');
    expect(risk?.ok).toBe(false);
    expect(risk?.fix).toContain('17.0.6');
    const report = await platformReport('android', rows);
    // Other prerequisites are ready: the affected JDK must not refuse a first Android build.
    const ready = report.map(row => row.name === 'Android JDK JIT' ? row : { ...row, ok: true });
    const app = resolve(dir, 'app'); mkdirSync(app);
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Fixture', app: { id: 'dev.exact.fixture', name: 'Fixture' }, host: { web: {} } }));
    writeFileSync(resolve(app, 'Cargo.toml'), '[workspace]\nmembers = []\n');
    expect((await bringPlatform(app, 'android', { report: ready })).refused).toBeUndefined();
    expect(JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8')).host.android).toBeDefined();
    expect(report.some(row => row.name === risk?.name)).toBe(false);
    const scoped = spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), 'setup', '--check', '--platform', 'android'], {
      env, encoding: 'utf8', timeout: 30_000,
    });
    expect(scoped.error).toBeUndefined();
    expect(scoped.stdout).toContain('Android JDK JIT');
    expect(scoped.stdout).toContain('JDK-8335662');
    expect(scoped.stderr).not.toMatch(/Android JDK JIT[^\n]*missing or differ/);
    expect(sdkReport(env, { only: 'web' }).some(row => row.name === 'Android JDK JIT')).toBe(false);
    let called = 0;
    const run = (_command, _args, options) => {
      expect(options.env.JAVA_HOME).toBe(dir);
      expect(errors.mock.calls.flat().join('\n')).toContain('JDK-8335662');
      called++; return { status: 0 };
    };
    expect(runGradle(['help'], { env, cwd: dir }, process.platform, { run })).toBe(true);
    expect(called).toBe(1);
    errors.mockClear();
    java('17.0.16');
    expect(sdkReport(env, { only: 'android' }).some(row => row.name === 'Android JDK JIT')).toBe(false);
    expect(runGradle(['help'], { env, cwd: dir }, process.platform, { run: () => ({ status: 0 }) })).toBe(true);
    expect(errors).not.toHaveBeenCalled();
  } finally { errors.mockRestore(); rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('a failed Gradle run names a matching JVM crash log, including one left by an earlier run', async () => {
  const { runGradle } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-gradle-crash-'));
  const errors = spyOn(console, 'error').mockImplementation(() => {});
  try {
    const log = resolve(dir, 'hs_err_pid123.log');
    const run = status => runGradle(['help'], { cwd: dir, env: { JAVA_HOME: resolve(dir, 'absent'), PATH: '' } }, process.platform, { run: () => ({ status }) });
    writeFileSync(log, '# SIGSEGV in an unrelated library\n');
    expect(run(1)).toBe(false);
    expect(errors).not.toHaveBeenCalled();
    writeFileSync(log, '# Internal Error (assembler_aarch64.hpp:267)\n# guarantee(chk == -1 || chk == 0) failed: Field too big for insn\nV LIR_Assembler::osr_entry()\n');
    expect(run(0)).toBe(true);
    expect(errors).not.toHaveBeenCalled();
    expect(run(1)).toBe(false);
    expect(errors.mock.calls.flat().join('\n')).toContain(log);
    expect(errors.mock.calls.flat().join('\n')).toContain('JDK-8335662');
    expect(errors.mock.calls.flat().join('\n')).toContain('earlier run');
    expect(errors.mock.calls.flat().join('\n')).toContain('JAVA_HOME');
  } finally { errors.mockRestore(); rmSync(dir, { recursive: true, force: true }); }
});

test('the Android reader variable filter and activity allowlist include the auth fixture; loopback URLs select ports', async () => {
  const { readerVariables, reversedPorts } = await import('./agent-android.mjs');
  const vars = readerVariables({ EXACT_AGENT: '1', EXACT_AGENT_STORAGE: 's', EXACT_AUTH_FIXTURE: 'http://127.0.0.1:4791', EXACT_APP_DIR: '/a', HOME: '/h',
    EXACT_UPDATE_ORIGIN: 'http://127.0.0.1:8790/', EXACT_UPDATE_DIR: '/u' });
  expect(vars).toEqual({ EXACT_AGENT: '1', EXACT_AGENT_STORAGE: 's', EXACT_AUTH_FIXTURE: 'http://127.0.0.1:4791', EXACT_UPDATE_ORIGIN: 'http://127.0.0.1:8790/', EXACT_UPDATE_DIR: '/u' });
  expect(reversedPorts(vars)).toEqual(['4791', '8790']);
  expect(reversedPorts({ EXACT_AUTH_FIXTURE: 'http://[::1]:4791' })).toEqual(['4791']);
  expect(reversedPorts({ EXACT_DEV_PLAN: 'http://localhost:8080/', EXACT_AUTH_FIXTURE: 'http://127.0.0.1:8080' })).toEqual(['8080']);
  expect(reversedPorts({ EXACT_DEV_PLAN: 'https://example.com/plan' })).toEqual([]);
  const activity = readFileSync(resolve(import.meta.dir, '../host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt'), 'utf8');
  expect(Object.keys(vars).map((name) => `x_${name.slice('EXACT_'.length).toLowerCase()}`).filter((extra) => !activity.includes(`"${extra}"`))).toEqual([]);
});

// A test that times out never reaches its own `finally`.
const driveFixtures = [];
afterAll(() => { for (const fixture of driveFixtures) fixture.dispose(); });

// The stand-in device reaches the fixture only while its reverse mapping exists.
function androidDriveFixture(reader, destination = null, reacquire = false, failedRemoval = null, stuckShell = false, screen = {}) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-adb-')), calls = resolve(dir, 'calls'), mappings = resolve(dir, 'mappings');
  writeFileSync(mappings, JSON.stringify(destination ? { 'tcp:4791': destination } : {}));
  const display = resolve(dir, 'display');
  writeFileSync(display, JSON.stringify({ wakefulness: 'Awake', showing: false, secure: false, ...screen }));
  let disconnected, connection; const closed = new Promise(done => { disconnected = done; });
  const connected = new Promise(done => { connection = done; });
  const server = Bun.listen({ hostname: '127.0.0.1', port: 0, socket: {
    open(socket) { connection(); if (screen.ready !== false) socket.write('{"ready":true}\n'); },
    data(socket) { socket.write(JSON.stringify({ fixture: JSON.parse(readFileSync(mappings, 'utf8'))['tcp:4791'] === 'tcp:4791' }) + '\n'); },
    close() { disconnected(); },
  } });
  mkdirSync(resolve(dir, 'platform-tools'));
  writeFileSync(resolve(dir, 'platform-tools/adb'), `#!${process.execPath}
import { appendFileSync, existsSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
const args = process.argv.slice(2);
appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + '\\n');
const cmd = args.slice(2), pipedFocus = cmd[0] === 'shell' && cmd[1] === ${JSON.stringify(focusCommand)};
if (pipedFocus) cmd.splice(1, 1, 'dumpsys', 'window');
const display = ${JSON.stringify(display)}, screen = JSON.parse(readFileSync(display, 'utf8'));
if (cmd[0] === 'shell' && cmd[1] === 'dumpsys') {
  if (screen.failDump === cmd[2]) { console.error('dump failed'); process.exit(1); }
  if (cmd[2] === 'power' && screen.waking) {
    screen.wakefulness = 'Awake'; screen.waking = false; writeFileSync(display, JSON.stringify(screen));
  }
  if (screen.dumpMsAfterWake && screen.wakefulness === 'Awake') await Bun.sleep(screen.dumpMsAfterWake);
  const state = screen.wakefulness === 'Asleep' ? (screen.showing ? (screen.secure ? 'asleep-pin' : 'asleep-swipe') : 'asleep-no-lock')
    : screen.showing ? (screen.secure ? 'awake-pin' : 'awake-swipe') : screen.dismissals ? 'awake-dismissed' : 'awake-unlocked';
  if (cmd[2] === 'power') process.stdout.write(screen.power ?? readFileSync(${JSON.stringify(screenFixtures)} + '/' + state + '.power.txt', 'utf8'));
  if (cmd[2] === 'window' && (!cmd[3] || ['windows', 'displays'].includes(cmd[3]))) {
    if (screen.failFocus) { if (screen.focus) process.stdout.write(screen.focus); console.error('focus dump failed'); process.exit(1); }
    const focus = screen.focus ?? 'mCurrentFocus=null\\n  mFocusedApp=null\\n';
    if (pipedFocus) process.stdout.write(focus);
    else if (cmd[3] === 'displays' && (screen.api ?? 29) < 30) process.stdout.write('WINDOW MANAGER DISPLAY CONTENTS\\n  Display: mDisplayId=0\\n');
    else process.stdout.write('WINDOW MANAGER WINDOWS\\n' + ((screen.api ?? 29) < 30 ? '  ' : '  Display: mDisplayId=0\\n    ') + focus);
  }
  if (cmd[2] === 'window' && cmd[3] === 'policy') {
    if (screen.largePolicy) process.stdout.write('Window policy details\\n'.repeat(100000));
    let policy = screen.window ?? readFileSync(${JSON.stringify(screenFixtures)} + '/' + state + '.policy.txt', 'utf8');
    if (screen.delegateState) policy = policy.replace('KeyguardServiceDelegate\\n', 'KeyguardServiceDelegate state:\\n');
    process.stdout.write(policy);
  }
}
if (cmd[0] === 'shell' && cmd[1] === 'input' && cmd[3] === 'KEYCODE_WAKEUP') {
  screen.waking = true; writeFileSync(display, JSON.stringify(screen));
}
if (cmd[0] === 'shell' && cmd[1] === 'wm' && cmd[2] === 'dismiss-keyguard' && !screen.secure && screen.wakefulness === 'Awake') {
  if (screen.dismissMs) await Bun.sleep(screen.dismissMs);
  screen.dismissals = (screen.dismissals ?? 0) + 1;
  if (screen.dismissals >= (screen.dismissAfter ?? 1)) screen.showing = false;
  writeFileSync(display, JSON.stringify(screen));
}
if (cmd[0] === 'reverse') {
  const file = ${JSON.stringify(mappings)}, gate = file + '.' + cmd[1];
  if (existsSync(gate + '.hold')) {
    writeFileSync(gate + '.entered', String(process.ppid));
    while (existsSync(gate + '.hold')) await Bun.sleep(50);
  }
  if (existsSync(gate + '.fail')) { rmSync(gate + '.fail'); process.exit(1); }
  const mappings = JSON.parse(readFileSync(file, 'utf8'));
  if (cmd[1] === '--remove' && ${stuckShell}) {
    const shells = readFileSync(${JSON.stringify(resolve(dir, 'shells'))}, 'utf8').trim().split('\\n');
    const live = shells.filter(pid => {
      const status = spawnSync('ps', ['-o', 'stat=', '-p', pid], { encoding: 'utf8' });
      return status.status === 0 && !status.stdout.trim().startsWith('Z');
    });
    writeFileSync(file + '.live-shells', JSON.stringify(live));
  }
  if (cmd[1] === '--list') console.log(Object.entries(mappings).map(([from, to]) => 'UsbFfs ' + from + ' ' + to).join('\\n'));
  if (cmd[1] === '--no-rebind') { if (mappings[cmd[2]]) process.exit(1); mappings[cmd[2]] = cmd[3]; }
  if (cmd[1] === '--remove' && cmd[2] === 'tcp:4792' && ${JSON.stringify(failedRemoval)} && !existsSync(file + '.failed')) {
    writeFileSync(file + '.failed', '');
    if (${JSON.stringify(failedRemoval)} === 'replaced') { mappings[cmd[2]] = 'tcp:9001'; writeFileSync(file, JSON.stringify(mappings)); }
    process.exit(1);
  }
  if (cmd[1] === '--remove') { if (!mappings[cmd[2]]) process.exit(1); delete mappings[cmd[2]]; if (${reacquire} && !existsSync(file + '.acquired')) { mappings[cmd[2]] = 'tcp:9000'; writeFileSync(file + '.acquired', ''); } }
  if (cmd[1] !== '--list') writeFileSync(file, JSON.stringify(mappings));
}
if (cmd[0] === 'forward' && cmd[1] === 'tcp:0') console.log(${server.port});
if (cmd[0] === 'shell' && cmd[1] === '-T') {
  appendFileSync(${JSON.stringify(resolve(dir, 'shells'))}, process.pid + '\\n');
  if (${stuckShell}) setInterval(() => {}, 1000);
  console.log('{"ready":true}');
  const lines = createInterface({ input: process.stdin });
  lines.on('line', line => {
    if (JSON.parse(line).op === 'quit') process.exit(0);
    console.log(JSON.stringify({ fixture: JSON.parse(readFileSync(${JSON.stringify(mappings)}, 'utf8'))['tcp:4791'] === 'tcp:4791' }));
  });
}
`, { mode: 0o755 });
  const bin = resolve(dir, reader ? 'app.apk' : 'app'); writeFileSync(bin, 'stand-in app');
  // A serial of its own keeps each fixture's ownership record apart (`reverseRoot`).
  const serial = `fixture-${randomUUID()}`;
  const env = { ...process.env, ANDROID_HOME: dir, ANDROID_SERIAL: serial,
    XDG_CACHE_HOME: resolve(dir, 'cache'), EXACT_AGENT: '1', EXACT_AUTH_FIXTURE: 'http://127.0.0.1:4791' };
  for (const name of ['EXACT_DEV_PLAN', 'EXACT_PLAN', 'EXACT_WORLD', 'EXACT_LAUNCH_URL']) delete env[name];
  const fixture = { port: server.port, app: { id: 'dev.exact.fixture', dir }, bin, env, closed, connected,
    commands: () => readFileSync(calls, 'utf8').trim().split('\n').map(JSON.parse),
    mappings: () => JSON.parse(readFileSync(mappings, 'utf8')), stopSocket: () => server.stop(true),
    focus: (focus, failFocus = false) => writeFileSync(display, JSON.stringify({ ...JSON.parse(readFileSync(display, 'utf8')), focus, failFocus })),
    replace: destination => writeFileSync(mappings, JSON.stringify(destination ? { 'tcp:4791': destination } : {})),
    failNextReverse: action => writeFileSync(mappings + '.' + action + '.fail', ''),
    shells: () => readFileSync(resolve(dir, 'shells'), 'utf8').trim().split('\n').map(Number),
    liveShellsAtRemoval: () => JSON.parse(readFileSync(mappings + '.live-shells', 'utf8')),
    holdReverse(action) {
      const gate = mappings + '.' + action;
      writeFileSync(gate + '.hold', '');
      return { entered: () => { try { return Number(readFileSync(gate + '.entered', 'utf8')); } catch { return null; } },
        release: () => rmSync(gate + '.hold', { force: true }) };
    },
    dispose() { server.stop(true); rmSync(dir, { recursive: true, force: true }); rmSync(reverseRoot(serial), { recursive: true, force: true }); },
  };
  driveFixtures.push(fixture);
  return fixture;
}
test('the Android reader uses a variant\'s package for lookup, launch and drive storage, and refuses another package\'s APK', async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const { resolveVariant } = await import('./variants.mjs');
  const fixture = androidDriveFixture(true);
  fixture.app.manifest = resolveVariant({ app: { id: fixture.app.id }, host: { android: { id: 'com.example.android_app' } },
    variants: { dev: { idSuffix: '.dev', name: 'Dev' } } }, 'dev');
  const id = 'com.example.android_app.dev';
  let child;
  try {
    // The deleted knob is refused by name, and an APK built as the plain package is not launched as the variant.
    expect(() => androidDeploy(fixture.app, fixture.bin, { ...fixture.env, ORG_GRADLE_PROJECT_idSuffix: '.dev' })).toThrow(/ORG_GRADLE_PROJECT_idSuffix is deleted/);
    const record = resolve(dirname(fixture.bin), 'exact-build.json');
    writeFileSync(record, JSON.stringify({ package: 'com.example.android_app' }));
    expect(() => androidDeploy(fixture.app, fixture.bin, fixture.env)).toThrow(`installs as com.example.android_app, not ${id}: build it again (bun exact.mjs android --variant dev)`);
    writeFileSync(record, JSON.stringify({ package: id }));
    const deployed = androidDeploy(fixture.app, fixture.bin, fixture.env);
    expect(deployed.dir).toBe(`/data/local/tmp/exact/${id}`);
    child = deployed.spawn();
    const closed = once(child, 'close');
    child.stdout.once('data', () => child.stdin.end('{"op":"quit"}\n'));
    expect(await closed).toEqual([0, null]);
    const commands = fixture.commands().map(args => args.slice(2));
    expect(commands.some(args => args[0] === 'shell' && args[1].includes(`pm path '${id}'`))).toBe(true);
    expect(commands.some(args => args[0] === 'shell' && args[1].includes(`${id}/dev.exact.app.MainActivity`))).toBe(true);
    expect(commands.some(args => args.join(' ').includes(fixture.app.id))).toBe(false);
  } finally {
    if (child?.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
});

test('Android permission detection reads only the focused window and activity', () => {
  const component = 'com.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity';
  for (const line of [`mCurrentFocus=Window{abc u0 ${component}}`, `mFocusedApp=ActivityRecord{abc u0 ${component} t42}`,
    `mCurrentFocus=null\n  mFocusedApp=ActivityRecord{abc u0 ${component} t42}`]) expect(androidPermissionPrompt(line)).toBe(component);
  for (const line of ['', 'mCurrentFocus=null', 'mCurrentFocus=Window{abc u0 dev.exact.app/.MainActivity}',
    `Window #0 Window{abc u0 ${component}}`, `mLastFocus=Window{abc u0 ${component}}`,
    `mCurrentFocus=Window{abc u0 ${component}Extra}`]) expect(androidPermissionPrompt(line)).toBeNull();
});

for (const [api, controller] of [[29, 'com.android.permissioncontroller'], [30, 'com.google.android.permissioncontroller']]) test(`the Android reader reports a surviving API ${api} ${controller} permission dialog`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const component = `${controller}/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity`;
  const fixture = androidDriveFixture(true, null, false, null, false, { api, focus: `mCurrentFocus=Window{123 u0 ${component}}` });
  let child;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close');
    let error = '', output = '';
    child.stderr.on('data', data => { error += data; });
    child.stdout.on('data', data => {
      output += data;
      if (output.trim().split('\n').length === 1) child.stdin.write('{"op":"state"}\n');
      else if (output.trim().split('\n').length === 2) {
        fixture.focus('mCurrentFocus=Window{abc u0 dev.exact.fixture/dev.exact.app.MainActivity}');
        child.stdin.write('{"op":"state"}\n');
      } else if (output.trim().split('\n').length === 3) {
        fixture.focus(null, true);
        child.stdin.write('{"op":"state"}\n');
      } else child.stdin.end('{"op":"quit"}\n');
    });
    expect(await closed).toEqual([0, null]);
    expect(error).toContain(`a permission dialog is up (${component})`);
    expect(output.trim().split('\n').slice(1)).toEqual(Array(3).fill('{"fixture":true}'));
    const commands = fixture.commands().map(args => args.slice(2));
    expect(commands).not.toContainEqual(['shell', 'dumpsys', 'window']);
    const launch = commands.findIndex(args => args[0] === 'shell' && args[1].startsWith('am start '));
    expect(commands.slice(0, launch)).toContainEqual(['shell', focusCommand]);
    expect(commands.slice(launch + 1)).not.toContainEqual(['shell', focusCommand]);
    expect(commands.filter(args => args[0] === 'shell' && args[1] === 'am' && args[2] === 'force-stop').every(args => args[3] === fixture.app.id)).toBe(true);
    expect(commands.some(args => args.join(' ').match(/\b(?:grant|revoke|keyevent|stack remove)\b/))).toBe(false);
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (child?.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
});

for (const screen of [
  { api: 29, focus: 'mCurrentFocus=Window{123 u0 com.android.permissioncontroller/.permission.ui.GrantPermissionsActivity}' },
  { api: 30, focus: 'mFocusedApp=ActivityRecord{123 u0 com.google.android.permissioncontroller/com.android.permissioncontroller.permission.ui.GrantPermissionsActivity t42}' },
  { api: 29, focus: '  mTopFocusedDisplayId=0\n' },
  { api: 30, failFocus: true },
  { api: 29, failFocus: true, focus: 'mCurrentFocus=Window{123 u0 com.android.permissioncontroller/.permission.ui.GrantPermissionsActivity}' },
]) test(`the Android CLI preserves JSON and reports startup focus: ${JSON.stringify(screen)}`, async () => {
  const fixture = androidDriveFixture(true, null, false, null, false, screen);
  writeFileSync(resolve(fixture.app.dir, 'app.json'), JSON.stringify({ name: 'fixture', app: { id: fixture.app.id, name: 'Fixture' } }));
  writeFileSync(resolve(fixture.app.dir, 'app.contract'), 'component App\n  view\n    text "Fixture"\n');
  let driver;
  try {
    driver = spawn(process.execPath, [resolve(import.meta.dir, 'agent.mjs'), 'android', '--reader', '--json', '--app', fixture.app.dir, 'state'],
      { env: { ...fixture.env, EXACT_ANDROID_BIN: fixture.bin }, stdio: ['pipe', 'pipe', 'pipe'] });
    const closed = once(driver, 'close');
    let output = '', error = '';
    driver.stdout.on('data', data => { output += data; });
    driver.stderr.on('data', data => { error += data; });
    expect(await closed).toEqual([0, null]);
    expect(JSON.parse(output).fixture).toBe(true);
    expect(error).toContain(screen.focus?.includes('GrantPermissionsActivity') ? 'a permission dialog is up ('
      : screen.failFocus ? 'focus dump failed' : 'focus was unreadable');
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (driver?.exitCode === null && driver.signalCode === null) { const closed = once(driver, 'close'); driver.kill(); await closed; }
    fixture.dispose();
  }
});

test('an unexpected carrier exit fails a pending request before its pipes close and starts cleanup', async () => {
  const android = await import('./agent-android.mjs');
  const { openStdio } = await import('./agent-stdio.mjs');
  const { waitAtMost } = await import('./agent-wire.mjs');
  const fixture = androidDriveFixture(false), before = process.env.EXACT_ANDROID_BIN;
  writeFileSync(resolve(fixture.app.dir, 'app.json'), JSON.stringify({ name: 'fixture', app: { id: fixture.app.id, name: 'Fixture' } }));
  writeFileSync(resolve(fixture.app.dir, 'app.contract'), 'component App\n  view\n    text "Fixture"\n');
  const child = new EventEmitter();
  Object.assign(child, { stdin: new PassThrough(), stdout: new PassThrough(), stderr: new PassThrough(), exitCode: null, signalCode: null });
  let released, session;
  const cleaned = new Promise(resolve => { released = resolve; });
  const deploy = spyOn(android, 'androidDeploy').mockReturnValue({ spawn: () => child, cleanup: released });
  try {
    process.env.EXACT_ANDROID_BIN = fixture.bin;
    const opening = openStdio({ host: 'android', app: fixture.app.dir, onProcess() {
      setImmediate(() => child.stdout.write('{"ready":true}\n'));
    } });
    session = await opening;
    const request = session.ask({ op: 'state' });
    const ended = spyOn(child.stdin, 'end');
    child.stderr.write('fatal: boom');
    await new Promise(resolve => setImmediate(resolve));
    child.exitCode = 1; child.emit('exit', 1, null);
    const failure = waitAtMost(request, 1000, () => { throw new Error('request waited for the open pipes'); });
    await expect(failure).rejects.toThrow('the app exited (exit code 1)');
    // A fragment with no newline is in the hangup, and the exited child's stdin is left alone.
    await expect(failure).rejects.toThrow('app: fatal: boom');
    expect(ended).not.toHaveBeenCalled();
    expect(child.stdout.destroyed).toBe(false);
    await waitAtMost(cleaned, 6000, () => { throw new Error('unexpected exit did not start cleanup'); });
    expect(child.stdout.destroyed).toBe(true);
    expect(child.stderr.destroyed).toBe(true);
  } finally {
    child.emit('close', 1, null);
    await session?.close();
    for (const stream of [child.stdin, child.stdout, child.stderr]) stream.destroy();
    deploy.mockRestore();
    if (before === undefined) delete process.env.EXACT_ANDROID_BIN; else process.env.EXACT_ANDROID_BIN = before;
    fixture.dispose();
  }
}, 8000);

for (const unexpected of [false, true]) test(`Windows carrier cleanup kills only its recorded pipe owners (unexpected exit: ${unexpected})`, async () => {
  const { windowsCarrier } = await import('./agent-android.mjs');
  const child = new EventEmitter(), killed = [];
  child.kill = signal => { killed.push([10, signal]); child.emit('exit', null, signal); return true; };
  windowsCarrier(child, (pid, signal) => killed.push([pid, signal]));
  child.emit('message', { adbPipe: 11 });
  child.emit('message', { adbPipe: 12 });
  child.emit('message', { adbPipe: 12, exited: true });
  child.emit('message', { reverse: 13 });
  if (unexpected) child.emit('exit', 1, null); else expect(child.kill('SIGKILL')).toBe(true);
  expect(killed).toEqual(unexpected ? [[11, 'SIGKILL']] : [[11, 'SIGKILL'], [10, 'SIGKILL']]);
  // A spawn notification already in flight when the carrier died still owns a pipe.
  child.emit('message', { adbPipe: 14 });
  expect(killed.at(-1)).toEqual([14, 'SIGKILL']);
});

for (const alreadyExited of [false, true]) test(`closing a carrier bounds inherited pipes left open (already exited: ${alreadyExited})`, async () => {
  const { closeStdio } = await import('./agent-stdio.mjs');
  const child = new EventEmitter();
  Object.assign(child, { stdin: new PassThrough(), stdout: new PassThrough(), stderr: new PassThrough(),
    exitCode: alreadyExited ? 1 : null, signalCode: null });
  let killed = false, released = false;
  child.kill = () => { killed = true; child.signalCode = 'SIGKILL'; child.emit('exit', null, 'SIGKILL'); };
  const closed = once(child, 'close');
  try {
    await closeStdio(child, closed, () => { released = true; });
    expect(killed).toBe(!alreadyExited);
    expect(released).toBe(true);
    expect(child.stdout.destroyed).toBe(true);
    expect(child.stderr.destroyed).toBe(true);
  } finally { for (const stream of [child.stdin, child.stdout, child.stderr]) stream.destroy(); child.emit('close'); }
}, 7000);

test('the Android reader logs a failed focused activity read, launches and releases its mappings', async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(true, null, false, null, false, { failFocus: true });
  let child;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close');
    let error = '';
    child.stderr.on('data', data => { error += data; });
    child.stdout.once('data', () => child.stdin.end('{"op":"quit"}\n'));
    expect(await closed).toEqual([0, null]);
    expect(error).toContain('Android focused activity: adb shell dumpsys window: focus dump failed');
    expect(fixture.commands().some(args => args[2] === 'shell' && args[3].startsWith('am start '))).toBe(true);
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (child?.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
});

test('Android screen dumps recognize both delegate headings and stop at the next section', () => {
  for (const [name, wakefulness, showing, secure] of [
    ['awake-unlocked', 'Awake', false, false], ['asleep-no-lock', 'Asleep', false, false],
    ['asleep-swipe', 'Asleep', true, false], ['awake-swipe', 'Awake', true, false],
    ['asleep-pin', 'Asleep', true, true], ['awake-pin', 'Awake', true, true],
    ['awake-dismissed', 'Awake', false, false],
  ]) {
    const power = screenDump(name, 'power'), policy = screenDump(name, 'policy');
    const expected = { wakefulness, showing, secure };
    expect(androidScreenState(power, policy)).toEqual(expected);
    // The captured API 36 image uses the bare heading; AOSP also uses this heading.
    expect(androidScreenState(power, policy.replace('KeyguardServiceDelegate\n', 'KeyguardServiceDelegate state:\n')
      + '    AnotherSection\n      showing=true\n      secure=true\n')).toEqual(expected);
  }
  const power = screenDump('awake-unlocked', 'power');
  expect(androidScreenState(power, 'isKeyguardShowing=true\nisKeyguardSecure=true')).toEqual({ wakefulness: 'Awake', showing: true, secure: true });
  expect(androidScreenState(power, 'mShowingLockscreen=false')).toEqual({ wakefulness: 'Awake', showing: false, secure: null });
  expect(androidScreenState(power, 'some other showing=true')).toEqual({ wakefulness: 'Awake', showing: null, secure: null });
  expect(() => androidScreenState('mIsPowered=true', '')).toThrow('dumpsys power has no mWakefulness');
});

for (const wakefulness of ['Asleep', 'Awake']) test(`the Android reader checks an ${wakefulness} screen before launching`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(true, null, false, null, false, { wakefulness, showing: wakefulness === 'Asleep' });
  let child;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close');
    child.stdout.once('data', () => child.stdin.end('{"op":"quit"}\n'));
    expect(await closed).toEqual([0, null]);
    const commands = fixture.commands().map(args => args.slice(2));
    const launch = commands.findIndex(args => args[0] === 'shell' && args[1].startsWith('am start '));
    const screenCalls = commands.filter(args => args[0] === 'shell' && ['dumpsys', 'input', 'wm', focusCommand].includes(args[1]));
    const check = [['shell', 'dumpsys', 'power'], ['shell', 'dumpsys', 'window', 'policy']];
    expect(screenCalls).toEqual([...(wakefulness === 'Awake' ? check : [
      ...check, ['shell', 'input', 'keyevent', 'KEYCODE_WAKEUP'], ...check, ['shell', 'wm', 'dismiss-keyguard'], ...check,
    ]), ['shell', focusCommand]]);
    expect(commands.indexOf(screenCalls.at(-1))).toBeLessThan(launch);
  } finally {
    if (child?.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
});

for (const [name, screen, dismissals] of [
  ['a large policy dump', { largePolicy: true }, 0],
  ['an asleep screen without a lock', { wakefulness: 'Asleep' }, 0],
  ['a delegate state heading', { showing: true, delegateState: true }, 1],
  ['slow dumps after waking a swipe lock', { wakefulness: 'Asleep', showing: true, dumpMsAfterWake: 1100 }, 1],
  ['a keyguard that needs another dismissal', { showing: true, dismissAfter: 2 }, 2],
  ['a dismissal longer than the poll deadline', { showing: true, dismissMs: 2100 }, 1],
  ['slow dismissals that need another observation budget', { showing: true, dismissMs: 2100, dismissAfter: 2 }, 2],
]) test(`the Android reader launches after ${name}`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(true, null, false, null, false, screen);
  let child;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close'); let error = '';
    child.stderr.on('data', data => { error += data; });
    child.stdout.once('data', () => child.stdin.end('{"op":"quit"}\n'));
    expect({ closed: await closed, error }).toEqual({ closed: [0, null], error: expect.stringContaining('keyguard hidden') });
    const commands = fixture.commands().map(args => args.slice(2));
    expect(commands.filter(args => args[1] === 'wm')).toHaveLength(dismissals);
    expect(commands).toContainEqual(['shell', 'dumpsys', 'window', 'policy']);
    const launch = commands.findIndex(args => args[0] === 'shell' && args[1].startsWith('am start '));
    const lastDismissal = commands.findLastIndex(args => args[1] === 'wm');
    expect(launch).toBeGreaterThan(lastDismissal);
    expect(commands.slice(lastDismissal + 1, launch)).toContainEqual(['shell', 'dumpsys', 'power']);
  } finally {
    if (child?.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const wakefulness of ['Asleep', 'Awake']) test(`the Android reader refuses an ${wakefulness} secure lock screen before launching`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(true, null, false, null, false, { wakefulness, showing: true, secure: true });
  let child;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close'); let error = '';
    child.stderr.on('data', data => { error += data; });
    child.stdout.once('data', () => child.stdin.end('{"op":"quit"}\n'));
    expect((await closed)[0]).not.toBe(0);
    expect(error).toContain('secure lock screen');
    expect(error).toContain('unlock the device and run the drive again');
    const commands = fixture.commands().map(args => args.slice(2));
    expect(commands.some(args => args[0] === 'shell' && args[1].startsWith('am start '))).toBe(false);
    expect(commands).not.toContainEqual(['shell', 'wm', 'dismiss-keyguard']);
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (child?.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
});

for (const dump of ['power', 'window']) test(`the Android reader reports a failed ${dump} dump before launching`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(true, null, false, null, false, { failDump: dump });
  let child;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close'); let error = '';
    child.stderr.on('data', data => { error += data; });
    expect((await closed)[0]).not.toBe(0);
    expect(error).toContain(`Android screen state: adb shell dumpsys ${dump}${dump === 'window' ? ' policy' : ''}: dump failed`);
    expect(fixture.commands().some(args => args[2] === 'shell' && args[3].startsWith('am start '))).toBe(false);
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (child?.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
});

test('an Android readiness timeout includes the screen state observed before launch', async () => {
  const fixture = androidDriveFixture(true, null, false, null, false, { wakefulness: 'Asleep', ready: false });
  writeFileSync(resolve(fixture.app.dir, 'app.json'), JSON.stringify({ name: 'fixture', app: { id: fixture.app.id, name: 'Fixture' } }));
  writeFileSync(resolve(fixture.app.dir, 'app.contract'), 'component App\n  view\n    text "Fixture"\n');
  let driver;
  try {
    driver = spawn(process.execPath, ['--eval', `
      import { openStdio } from ${JSON.stringify(resolve(import.meta.dir, 'agent-stdio.mjs'))};
      const timer = globalThis.setTimeout;
      let expire;
      globalThis.setTimeout = (fn, ms, ...args) => {
        if (ms === 60000) { expire = fn; process.send('armed'); return null; }
        return timer(fn, ms, ...args);
      };
      process.on('message', () => expire());
      try { await openStdio({ host: 'android', app: ${JSON.stringify(fixture.app.dir)} }); process.exit(0); }
      catch (error) { console.error(error.message); process.exit(1); }
    `], { env: { ...fixture.env, EXACT_ANDROID_BIN: fixture.bin }, stdio: ['pipe', 'pipe', 'pipe', 'ipc'] });
    const closed = once(driver, 'close'), armed = once(driver, 'message');
    let error = '';
    driver.stderr.on('data', data => { error += data; });
    await Promise.all([armed, fixture.connected]);
    driver.send('expire readiness deadline');
    expect((await closed)[0]).toBe(1);
    expect(error).toContain('the app never became ready;');
    expect(error).toContain('Android screen before launch: Asleep, keyguard hidden → Awake, keyguard hidden');
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (driver?.exitCode === null) { const closed = once(driver, 'close'); driver.kill(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) for (const present of [false, true]) test(`the Android ${reader ? 'reader' : 'headless'} drive ${present ? 'keeps an existing reverse' : 'adds and removes its reverse'} and passes the fixture`, async () => {
    const { androidDeploy } = await import('./agent-android.mjs');
    const fixture = androidDriveFixture(reader, present ? 'tcp:4791' : null, reader && !present); let child;
    try {
      child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
      const closed = once(child, 'close');
      let out = '', error = '';
      child.stdout.on('data', data => { out += data; });
      child.stderr.on('data', data => { error += data; });
      await once(child.stdout, 'data');
      child.stdin.end('{"op":"quit"}\n');
      expect(await closed).toEqual([0, null]);
      expect(error).toBe(reader ? 'Android screen before launch: Awake, keyguard hidden\n' : ''); expect(out.trim()).toBe('{"ready":true}');
      const commands = fixture.commands();
      expect(commands.every(args => args[0] === '-s' && args[1] === fixture.env.ANDROID_SERIAL)).toBe(true);
      const reverse = commands.map(args => args.slice(2)).filter(args => args[0] === 'reverse');
      expect(reverse).toEqual(present ? [['reverse', '--list']] : [
        ['reverse', '--list'], ['reverse', '--no-rebind', 'tcp:4791', 'tcp:4791'], ['reverse', '--list'], ['reverse', '--remove', 'tcp:4791'],
      ]);
      expect(fixture.mappings()).toEqual(present ? { 'tcp:4791': 'tcp:4791' } : reader ? { 'tcp:4791': 'tcp:9000' } : {});
      const launch = commands.find(args => args[2] === 'shell' && (reader ? args[3].startsWith('am start ') : args[3] === '-T'));
      expect(launch.at(-1)).toContain(reader ? "--es x_auth_fixture 'http://127.0.0.1:4791'" : "EXACT_AUTH_FIXTURE='http://127.0.0.1:4791'");
      // The reader launches in a new task: whatever the last drive left in the app's task goes with it.
      if (reader) expect(launch.at(-1)).toStartWith('am start -W --activity-clear-task -n ');
    } finally {
      if (child && child.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
      fixture.dispose();
    }
  });
for (const reader of [false, true]) test(`the Android ${reader ? 'reader' : 'headless'} drive refuses a conflicting reverse without changing it`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(reader, 'tcp:9000');
  try {
    expect(() => androidDeploy(fixture.app, fixture.bin, fixture.env)).toThrow('tcp:4791 maps to tcp:9000; needed tcp:4791');
    expect(fixture.commands().map(args => args.slice(2)).filter(args => args[0] === 'reverse')).toEqual([['reverse', '--list']]);
    expect(fixture.mappings()).toEqual({ 'tcp:4791': 'tcp:9000' });
  } finally { fixture.dispose(); }
});

function overlappingDrive(fixture, simultaneous = false, unreaped = false) {
  const source = `
    import { androidDeploy } from ${JSON.stringify(resolve(import.meta.dir, 'agent-android.mjs'))};
    import { once } from 'node:events';
    if (${simultaneous}) { process.send('waiting'); await once(process, 'message'); process.disconnect(); }
    const child = androidDeploy(${JSON.stringify(fixture.app)}, ${JSON.stringify(fixture.bin)}, process.env).spawn();
    child.stdout.once('data', () => {
      console.log(JSON.stringify({ ready: true, relay: child.pid, owner: process.pid }));
      child.stdout.pipe(process.stdout);
    });
    child.stderr.pipe(process.stderr);
    process.stdin.pipe(child.stdin);
    child.on('close', code => process.exit(code ?? 1));
  `;
  // Stop the owner's parent so its event loop cannot reap the owner before the other drive releases.
  const program = unreaped ? `
    import { spawn } from 'node:child_process';
    const child = spawn(process.execPath, ['--eval', ${JSON.stringify(source)}], { stdio: 'inherit' });
    process.on('message', () => process.kill(process.pid, 'SIGSTOP'));
    child.on('close', () => process.exit(0));
  ` : source;
  const driver = spawn(process.execPath, ['--eval', program], { env: fixture.env, stdio: ['pipe', 'pipe', 'pipe', ...(simultaneous || unreaped ? ['ipc'] : [])] });
  const closed = once(driver, 'close');
  return { driver, closed, waiting: simultaneous ? once(driver, 'message') : null,
    ready: once(driver.stdout, 'data').then(([data]) => JSON.parse(data)) };
}

async function carrierEnded(pid) {
  expect(await pollUntil(() => {
    const result = spawnSync('ps', ['-o', 'stat=', '-p', String(pid)], { encoding: 'utf8' });
    if (result.error) throw result.error;
    return result.status === 1 || result.stdout.trim().startsWith('Z');
  }, Boolean, { bound: 8000 })).toBe(true);
}

for (const windows of [false, true]) for (const kill of ['child', 'pid']) test(`killing the Android headless ${kill} stops its adb shell before releasing its reverse (Windows cleanup: ${windows})`, async () => {
  const { androidDeploy, windowsCarrier } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(false, null, false, null, true);
  let child, closed, shell;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    if (windows) { child.removeAllListeners('exit'); child.kill = ChildProcess.prototype.kill.bind(child); windowsCarrier(child); }
    const announced = once(child, 'message');
    closed = once(child, 'close');
    await once(child.stdout, 'data');
    [shell] = fixture.shells();
    expect((await announced)[0].adbPipe).toBe(shell);
    const exited = once(child, 'exit');
    if (kill === 'child') child.kill('SIGKILL'); else process.kill(child.pid, 'SIGKILL');
    await exited;
    await carrierEnded(shell);
    await closed;
    expect(fixture.liveShellsAtRemoval()).toEqual([]);
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (shell) { try { process.kill(shell, 'SIGKILL'); } catch {} }
    child?.stdin.end();
    if (child && child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    if (closed) await closed;
    fixture.dispose();
  }
}, 15000);

test.skipIf(process.platform === 'win32')('Android reverse cleanup waits for a signalled adb shell to close', async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(false, null, false, null, true);
  let child, closed, signal;
  const kill = process.kill.bind(process);
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    closed = once(child, 'close');
    await once(child.stdout, 'data');
    // A group signal need not finish every member before the carrier's exit is observed.
    signal = spyOn(process, 'kill').mockImplementation((pid, value) => {
      if (pid !== -child.pid) return kill(pid, value);
      setImmediate(() => { try { kill(pid, value); } catch (error) { if (error.code !== 'ESRCH') throw error; } });
      return true;
    });
    kill(child.pid, 'SIGKILL');
    await closed;
    expect(fixture.liveShellsAtRemoval()).toEqual([]);
    expect(fixture.mappings()).toEqual({});
  } finally {
    signal?.mockRestore();
    if (child?.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    if (closed) await closed;
    fixture.dispose();
  }
}, 15000);

test.skipIf(process.platform !== 'darwin')('Android group cleanup accepts EPERM only when no member is live', async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(false, null, false, null, true);
  let child, closed, signal, refused = true;
  const kill = process.kill.bind(process), denied = Object.assign(new Error('fixture group permission error'), { code: 'EPERM' });
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    closed = once(child, 'close');
    await once(child.stdout, 'data');
    signal = spyOn(process, 'kill').mockImplementation((pid, value) => {
      if (refused && pid === -child.pid) throw denied;
      return kill(pid, value);
    });
    expect(() => child.kill('SIGKILL')).toThrow(denied.message);
    refused = false;
    child.kill('SIGKILL');
    await closed;
    // Darwin can report EPERM while only unreaped zombies retain the group.
    refused = true;
    expect(child.kill('SIGKILL')).toBe(false);
    expect(fixture.mappings()).toEqual({});
  } finally {
    signal?.mockRestore();
    if (child?.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    if (closed) await closed;
    fixture.dispose();
  }
}, 15000);

test.skipIf(process.platform === 'win32')('closing an Android session waits for carrier cleanup', async () => {
  const { openStdio } = await import('./agent-stdio.mjs');
  const fixture = androidDriveFixture(false, null, false, null, true), before = process.env.EXACT_ANDROID_BIN;
  writeFileSync(resolve(fixture.app.dir, 'app.json'), JSON.stringify({ name: 'fixture', app: { id: fixture.app.id, name: 'Fixture' } }));
  writeFileSync(resolve(fixture.app.dir, 'app.contract'), 'component App\n  view\n    text "Fixture"\n');
  let child, closed, signal;
  const kill = process.kill.bind(process);
  try {
    process.env.EXACT_ANDROID_BIN = fixture.bin;
    const session = await openStdio({ host: 'android', app: fixture.app.dir, env: fixture.env,
      onProcess(process) { child = process; closed = once(child, 'close'); } });
    signal = spyOn(process, 'kill').mockImplementation((pid, value) => {
      if (pid !== -child.pid) return kill(pid, value);
      setImmediate(() => { try { kill(pid, value); } catch (error) { if (error.code !== 'ESRCH') throw error; } });
      return true;
    });
    kill(child.pid, 'SIGKILL');
    await session.close();
    expect(fixture.mappings()).toEqual({});
    expect(fixture.liveShellsAtRemoval()).toEqual([]);
  } finally {
    signal?.mockRestore();
    if (child?.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    if (closed) await closed;
    if (before === undefined) delete process.env.EXACT_ANDROID_BIN; else process.env.EXACT_ANDROID_BIN = before;
    fixture.dispose();
  }
}, 15000);

for (const windows of [false, true]) test(`killing an Android carrier group leaves an in-flight reverse mutation running (Windows cleanup: ${windows})`, async () => {
  const fixture = androidDriveFixture(false), gate = fixture.holdReverse('--remove');
  let driver, closed, holder;
  try {
    driver = spawn(process.execPath, ['--eval', `
      import { ChildProcess } from 'node:child_process';
      import { androidDeploy, windowsCarrier } from ${JSON.stringify(resolve(import.meta.dir, 'agent-android.mjs'))};
      const child = androidDeploy(${JSON.stringify(fixture.app)}, ${JSON.stringify(fixture.bin)}, process.env).spawn();
      if (${windows}) { child.removeAllListeners('exit'); child.kill = ChildProcess.prototype.kill.bind(child); windowsCarrier(child); }
      child.stderr.pipe(process.stderr);
      child.stdout.once('data', () => { console.log(JSON.stringify({ carrier: child.pid })); child.stdin.end('{"op":"quit"}\\n'); });
      process.stdin.once('data', () => child.kill('SIGKILL'));
      child.on('close', code => process.exit(code ?? 1));
    `], { env: fixture.env, stdio: ['pipe', 'pipe', 'pipe'] });
    closed = once(driver, 'close');
    const [ready] = await once(driver.stdout, 'data'), { carrier } = JSON.parse(ready);
    holder = await pollUntil(gate.entered, Boolean, { bound: 8000 });
    expect(holder).toBeGreaterThan(0);
    driver.stdin.write('kill');
    // The driver's close handler waits for this held helper and can block stdout from flushing.
    await carrierEnded(carrier);
    const status = spawnSync('ps', ['-o', 'stat=', '-p', String(holder)], { encoding: 'utf8' });
    expect(status.status).toBe(0);
    expect(status.stdout.trim().startsWith('Z')).toBe(false);
    gate.release();
    expect(await closed).toEqual([1, null]);
    await carrierEnded(holder);
    expect(fixture.mappings()).toEqual({});
  } finally {
    gate.release();
    if (driver && driver.exitCode === null && driver.signalCode === null) driver.stdin.end();
    if (closed) await closed;
    if (holder) await carrierEnded(holder);
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) for (const first of [0, 1]) for (const killed of ['neither', 'parent', 'carrier', 'both'])
  test(`overlapping Android ${reader ? 'reader' : 'headless'} drives retain their reverse when owner ${first + 1} ends (${killed} killed)`, async () => {
    const fixture = androidDriveFixture(reader), drives = [];
    try {
      drives.push(overlappingDrive(fixture)); await drives[0].ready;
      drives.push(overlappingDrive(fixture)); await drives[1].ready;
      const leaving = drives[first], remaining = drives[1 - first];
      const { relay } = await leaving.ready;
      if (killed === 'both') {
        leaving.driver.kill('SIGSTOP'); process.kill(relay, 'SIGKILL'); leaving.driver.kill('SIGKILL');
      } else if (killed === 'parent') leaving.driver.kill('SIGKILL');
      else if (killed === 'carrier') process.kill(relay, 'SIGKILL');
      else leaving.driver.stdin.end('{"op":"quit"}\n');
      await leaving.closed;
      await carrierEnded(relay);
      expect(fixture.mappings()).toEqual({ 'tcp:4791': 'tcp:4791' });
      const answer = once(remaining.driver.stdout, 'data');
      remaining.driver.stdin.write('{"op":"fixture"}\n');
      expect(JSON.parse((await answer)[0])).toEqual({ fixture: true });
      remaining.driver.stdin.end('{"op":"quit"}\n');
      expect(await remaining.closed).toEqual([0, null]);
      await pollUntil(() => Object.keys(fixture.mappings()).length === 0, Boolean, { bound: 8000 });
      expect(fixture.mappings()).toEqual({});
      expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
    } finally {
      for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
      fixture.dispose();
    }
  }, 15000);

for (const reader of [false, true]) test(`overlapping Android ${reader ? 'reader' : 'headless'} drives with caches of their own share their reverse`, async () => {
  const fixture = androidDriveFixture(reader), drives = [];
  try {
    drives.push(overlappingDrive(fixture)); await drives[0].ready;
    drives.push(overlappingDrive({ ...fixture, env: { ...fixture.env, XDG_CACHE_HOME: resolve(fixture.app.dir, 'cache-2') } }));
    await drives[1].ready;
    const { relay } = await drives[0].ready;
    drives[0].driver.stdin.end('{"op":"quit"}\n');
    await drives[0].closed;
    await carrierEnded(relay);
    expect(fixture.mappings()).toEqual({ 'tcp:4791': 'tcp:4791' });
    drives[1].driver.stdin.end('{"op":"quit"}\n');
    expect(await drives[1].closed).toEqual([0, null]);
    await pollUntil(() => Object.keys(fixture.mappings()).length === 0, Boolean, { bound: 8000 });
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
  } finally {
    for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) test(`Android ${reader ? 'reader' : 'headless'} drives acquire and release one reverse concurrently`, async () => {
  const fixture = androidDriveFixture(reader), drives = [];
  try {
    drives.push(overlappingDrive(fixture, true), overlappingDrive(fixture, true));
    await Promise.all(drives.map(drive => drive.waiting));
    for (const { driver } of drives) driver.send('start');
    await Promise.all(drives.map(drive => drive.ready));
    expect(fixture.mappings()).toEqual({ 'tcp:4791': 'tcp:4791' });
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--no-rebind')).toHaveLength(1);
    const answers = drives.map(({ driver }) => once(driver.stdout, 'data'));
    for (const { driver } of drives) driver.stdin.write('{"op":"fixture"}\n');
    expect((await Promise.all(answers)).map(([answer]) => JSON.parse(answer))).toEqual([{ fixture: true }, { fixture: true }]);
    for (const { driver } of drives) driver.stdin.end('{"op":"quit"}\n');
    expect(await Promise.all(drives.map(drive => drive.closed))).toEqual([[0, null], [0, null]]);
    expect(fixture.mappings()).toEqual({});
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
  } finally {
    for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) for (const action of ['--no-rebind', '--remove'])
  test(`Android ${reader ? 'reader' : 'headless'} reverse ownership survives a drive killed during ${action}`, async () => {
    const fixture = androidDriveFixture(reader), drives = [];
    let gate, holder;
    try {
      if (action === '--no-rebind') gate = fixture.holdReverse(action);
      drives.push(overlappingDrive(fixture));
      if (action === '--remove') {
        await drives[0].ready;
        gate = fixture.holdReverse(action);
        drives[0].driver.stdin.end('{"op":"quit"}\n');
      }
      holder = await pollUntil(gate.entered, Boolean, { bound: 8000 });
      expect(holder).toBeGreaterThan(0);
      drives[0].driver.kill('SIGKILL');
      if (action === '--remove') process.kill((await drives[0].ready).relay, 'SIGKILL');
      await drives[0].closed;
      drives.push(overlappingDrive(fixture, true));
      await drives[1].waiting; drives[1].driver.send('start');
      gate.release();
      await drives[1].ready;
      await carrierEnded(holder);
      const answer = once(drives[1].driver.stdout, 'data');
      drives[1].driver.stdin.write('{"op":"fixture"}\n');
      expect(JSON.parse((await answer)[0])).toEqual({ fixture: true });
      drives[1].driver.stdin.end('{"op":"quit"}\n');
      expect(await drives[1].closed).toEqual([0, null]);
      expect(fixture.mappings()).toEqual({});
    } finally {
      gate?.release();
      for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
      if (holder) await carrierEnded(holder);
      fixture.dispose();
    }
  }, 15000);

for (const reader of [false, true]) for (const external of [false, true])
  test(`overlapping Android ${reader ? 'reader' : 'headless'} drives preserve ${external ? 'an external' : 'a replaced'} reverse`, async () => {
    const fixture = androidDriveFixture(reader, external ? 'tcp:4791' : null), drives = [];
    try {
      drives.push(overlappingDrive(fixture)); await drives[0].ready;
      drives.push(overlappingDrive(fixture)); await drives[1].ready;
      if (!external) fixture.replace('tcp:9000');
      for (const { driver } of drives) driver.stdin.end('{"op":"quit"}\n');
      expect(await Promise.all(drives.map(drive => drive.closed))).toEqual([[0, null], [0, null]]);
      expect(fixture.mappings()).toEqual({ 'tcp:4791': external ? 'tcp:4791' : 'tcp:9000' });
      expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(0);
    } finally {
      for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
      fixture.dispose();
    }
  }, 15000);

for (const reader of [false, true]) for (const first of [0, 1]) test(`Android ${reader ? 'reader' : 'headless'} drives retain a recreated reverse when owner ${first + 1} exits first`, async () => {
  const fixture = androidDriveFixture(reader), drives = [];
  try {
    drives.push(overlappingDrive(fixture)); await drives[0].ready;
    fixture.replace(null);
    drives.push(overlappingDrive(fixture)); await drives[1].ready;
    const leaving = drives[first], remaining = drives[1 - first];
    leaving.driver.stdin.end('{"op":"quit"}\n');
    expect(await leaving.closed).toEqual([0, null]);
    const answer = once(remaining.driver.stdout, 'data');
    remaining.driver.stdin.write('{"op":"fixture"}\n');
    expect(JSON.parse((await answer)[0])).toEqual({ fixture: true });
    remaining.driver.stdin.end('{"op":"quit"}\n');
    expect(await remaining.closed).toEqual([0, null]);
    expect(fixture.mappings()).toEqual({});
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
  } finally {
    for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) test(`Android ${reader ? 'reader' : 'headless'} cleanup retains live owners while their reverse is missing`, async () => {
  const fixture = androidDriveFixture(reader), drives = [];
  try {
    drives.push(overlappingDrive(fixture)); await drives[0].ready;
    drives.push(overlappingDrive(fixture)); await drives[1].ready;
    fixture.replace(null);
    drives[0].driver.stdin.end('{"op":"quit"}\n');
    expect(await drives[0].closed).toEqual([0, null]);
    drives.push(overlappingDrive(fixture)); await drives[2].ready;
    drives[2].driver.stdin.end('{"op":"quit"}\n');
    expect(await drives[2].closed).toEqual([0, null]);
    const answer = once(drives[1].driver.stdout, 'data');
    drives[1].driver.stdin.write('{"op":"fixture"}\n');
    expect(JSON.parse((await answer)[0])).toEqual({ fixture: true });
    drives[1].driver.stdin.end('{"op":"quit"}\n');
    expect(await drives[1].closed).toEqual([0, null]);
    expect(fixture.mappings()).toEqual({});
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
  } finally {
    for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) test(`Android ${reader ? 'reader' : 'headless'} failed reverse recreation retains the original live owner`, async () => {
  const fixture = androidDriveFixture(reader), drives = [];
  try {
    drives.push(overlappingDrive(fixture)); await drives[0].ready;
    fixture.replace(null);
    fixture.failNextReverse('--no-rebind');
    const failed = spawnSync(process.execPath, ['--eval', `
      import { androidDeploy } from ${JSON.stringify(resolve(import.meta.dir, 'agent-android.mjs'))};
      androidDeploy(${JSON.stringify(fixture.app)}, ${JSON.stringify(fixture.bin)}, process.env);
    `], { env: fixture.env, encoding: 'utf8' });
    expect(failed.status).toBe(1);
    expect(failed.stderr).toContain('adb reverse --no-rebind');
    drives.push(overlappingDrive(fixture)); await drives[1].ready;
    drives[1].driver.stdin.end('{"op":"quit"}\n');
    expect(await drives[1].closed).toEqual([0, null]);
    const answer = once(drives[0].driver.stdout, 'data');
    drives[0].driver.stdin.write('{"op":"fixture"}\n');
    expect(JSON.parse((await answer)[0])).toEqual({ fixture: true });
    drives[0].driver.stdin.end('{"op":"quit"}\n');
    expect(await drives[0].closed).toEqual([0, null]);
    expect(fixture.mappings()).toEqual({});
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
  } finally {
    for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) test(`the final Android ${reader ? 'reader' : 'headless'} drive releases its reverse before a killed owner is reaped`, async () => {
  const fixture = androidDriveFixture(reader), drives = [];
  try {
    drives.push(overlappingDrive(fixture, false, true));
    const { owner, relay } = await drives[0].ready;
    drives.push(overlappingDrive(fixture)); await drives[1].ready;
    const status = pid => spawnSync('ps', ['-o', 'stat=', '-p', String(pid)], { encoding: 'utf8' }).stdout.trim();
    drives[0].driver.send('stop');
    expect(await pollUntil(() => status(drives[0].driver.pid).startsWith('T'), Boolean, { bound: 8000 })).toBe(true);
    process.kill(owner, 'SIGKILL'); process.kill(relay, 'SIGKILL');
    drives[0].driver.stdin.end();
    expect(await pollUntil(() => status(owner).startsWith('Z'), Boolean, { bound: 8000 })).toBe(true);
    await carrierEnded(relay);
    const answer = once(drives[1].driver.stdout, 'data');
    drives[1].driver.stdin.write('{"op":"fixture"}\n');
    expect(JSON.parse((await answer)[0])).toEqual({ fixture: true });
    drives[1].driver.stdin.end('{"op":"quit"}\n');
    expect(await drives[1].closed).toEqual([0, null]);
    expect(status(owner).startsWith('Z')).toBe(true);
    expect(fixture.mappings()).toEqual({});
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
  } finally {
    drives[0]?.driver.kill('SIGCONT');
    for (const { driver, closed } of drives) {
      if (driver.exitCode === null && driver.signalCode === null) driver.stdin.end();
      await closed;
    }
    fixture.dispose();
  }
}, 15000);

for (const reader of [false, true]) for (const killed of ['parent', 'carrier'])
  test(`the final Android ${reader ? 'reader' : 'headless'} owner releases a shared reverse when its ${killed} is killed`, async () => {
    const fixture = androidDriveFixture(reader), drives = [];
    try {
      for (const name of ['ADB_SERVER_SOCKET', 'ANDROID_ADB_SERVER_ADDRESS', 'ANDROID_ADB_SERVER_PORT']) delete fixture.env[name];
      drives.push(overlappingDrive(fixture)); await drives[0].ready;
      fixture.env.ADB_SERVER_SOCKET = 'tcp:127.0.0.1:5037';
      drives.push(overlappingDrive(fixture)); const { relay } = await drives[1].ready;
      drives[0].driver.stdin.end('{"op":"quit"}\n'); await drives[0].closed;
      expect(fixture.mappings()).toEqual({ 'tcp:4791': 'tcp:4791' });
      if (killed === 'parent') drives[1].driver.kill('SIGKILL'); else process.kill(relay, 'SIGKILL');
      await drives[1].closed;
      await carrierEnded(relay);
      expect(fixture.mappings()).toEqual({});
      expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
    } finally {
      for (const { driver, closed } of drives) if (driver.exitCode === null && driver.signalCode === null) { driver.stdin.end(); await closed; }
      fixture.dispose();
    }
  }, 15000);
for (const killed of ['parent', 'relay', 'parent before connect']) test(`the Android reader removes its reverse when the ${killed} is killed`, async () => {
  const fixture = androidDriveFixture(true), early = killed === 'parent before connect';
  if (early) fixture.stopSocket();
  let driver, relayPid;
  try {
    driver = spawn(process.execPath, ['--eval', `
      import { androidDeploy } from ${JSON.stringify(resolve(import.meta.dir, 'agent-android.mjs'))};
      const child = androidDeploy(${JSON.stringify(fixture.app)}, ${JSON.stringify(fixture.bin)}, process.env).spawn();
      child.stderr.pipe(process.stderr);
      if (${early}) console.log(JSON.stringify({ relay: child.pid }));
      else child.stdout.once('data', () => console.log(JSON.stringify({ relay: child.pid })));
      child.on('close', () => process.exit(0));
      process.stdin.on('data', () => child.kill('SIGKILL'));
    `], { env: fixture.env, stdio: ['pipe', 'pipe', 'pipe'] });
    const closed = once(driver, 'close');
    const [ready] = await once(driver.stdout, 'data'); relayPid = JSON.parse(ready).relay;
    if (early) await pollUntil(() => fixture.commands().some(args => args[2] === 'forward'), Boolean, { bound: 10000 });
    if (killed.startsWith('parent')) driver.kill('SIGKILL'); else driver.stdin.write('kill relay');
    await closed;
    if (early) await pollUntil(() => Object.keys(fixture.mappings()).length === 0, Boolean, { bound: 8000 });
    else await fixture.closed; // The relay's process has closed its device socket.
    const commands = fixture.commands().map(args => args.slice(2));
    const forwarded = commands.findIndex(args => args[0] === 'forward');
    expect(commands.slice(forwarded + 1)).toContainEqual(['shell', 'am', 'force-stop', fixture.app.id]);
    const reverse = fixture.commands().map(args => args.slice(2)).filter(args => args[0] === 'reverse');
    expect(reverse).toEqual([
      ['reverse', '--list'], ['reverse', '--no-rebind', 'tcp:4791', 'tcp:4791'], ['reverse', '--list'], ['reverse', '--remove', 'tcp:4791'],
    ]);
    expect(fixture.mappings()).toEqual({});
  } finally {
    if (driver && driver.exitCode === null && driver.signalCode === null) { const closed = once(driver, 'close'); driver.kill(); await closed; }
    if (relayPid) { try { process.kill(relayPid, 'SIGKILL'); } catch {} }
    fixture.dispose();
  }
}, 15000);

for (const eof of [false, true]) test(`the Android reader cleans up before connecting on ${eof ? 'stdin EOF' : 'connect timeout'}`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(true); let child;
  fixture.stopSocket();
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close'); let error = '';
    child.stderr.on('data', data => { error += data; });
    if (eof) { await pollUntil(() => fixture.commands().some(args => args[2] === 'forward'), Boolean, { bound: 10000 }); child.stdin.end(); }
    const [code] = await closed;
    expect(code === 0).toBe(eof);
    if (!eof) expect(error).toContain('the reader opened no agent socket');
    expect(fixture.mappings()).toEqual({});
    expect(fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove')).toHaveLength(1);
    const commands = fixture.commands().map(args => args.slice(2)), forwarded = commands.findIndex(args => args[0] === 'forward' && args[1] === 'tcp:0');
    expect(forwarded).toBeGreaterThan(-1);
    expect(commands.slice(forwarded + 1)).toContainEqual(['forward', '--remove', `tcp:${fixture.port}`]);
    expect(commands.slice(forwarded + 1)).toContainEqual(['shell', 'am', 'force-stop', fixture.app.id]);
  } finally {
    if (child && child.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
}, 15000);

for (const failure of ['retained', 'replaced']) test(`the Android reader retries only its remaining reverse after a failed removal (${failure})`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(true, null, true, failure); let child;
  fixture.env.EXACT_DEV_PLAN = 'http://127.0.0.1:4792';
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    const closed = once(child, 'close');
    await once(child.stdout, 'data'); child.stdin.end('{"op":"quit"}\n');
    expect(await closed).toEqual([1, null]);
    expect(fixture.mappings()).toEqual({ 'tcp:4791': 'tcp:9000', ...(failure === 'replaced' ? { 'tcp:4792': 'tcp:9001' } : {}) });
    const removals = fixture.commands().filter(args => args[2] === 'reverse' && args[3] === '--remove').map(args => args[4]);
    expect(removals.filter(port => port === 'tcp:4791')).toHaveLength(1);
    expect(removals.filter(port => port === 'tcp:4792')).toHaveLength(failure === 'retained' ? 2 : 1);
  } finally {
    if (child && child.exitCode === null) { const closed = once(child, 'close'); child.kill(); await closed; }
    fixture.dispose();
  }
});

test.skipIf(process.platform === 'win32')('Android tries PATH after an unusable Studio JDK but honors JAVA_HOME', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-jdk-fallback-'));
  try {
    const studio = resolve(dir, 'Studio Home'), pathJdk = resolve(dir, 'path-jdk');
    for (const [home, version] of [[studio, 16], [pathJdk, 17]]) {
      mkdirSync(resolve(home, 'bin'), { recursive: true });
      writeFileSync(resolve(home, 'bin/java'), `#!/bin/sh\necho 'openjdk version "${version}.0.1"' >&2\n`, { mode: 0o755 });
      writeFileSync(resolve(home, 'bin/javac'), '#!/bin/sh\nexit 0\n', { mode: 0o755 });
    }
    const env = { PATH: resolve(pathJdk, 'bin') };
    expect(androidJavaEnv(env, 'darwin', studio)).toBe(env);
    expect(androidJavaEnv({ ...env, JAVA_HOME: studio }, 'darwin', pathJdk)).toBeNull();
    expect(androidJavaEnv({ PATH: '' }, 'darwin', studio)).toBeNull();
    writeFileSync(resolve(studio, 'bin/java'), `#!/bin/sh\necho 'openjdk version "21.0.1"' >&2\n`, { mode: 0o755 });
    rmSync(resolve(studio, 'bin/javac'));
    expect(androidJavaEnv(env, 'darwin', studio)).toBe(env);
    expect(androidJavaEnv({ ...env, JAVA_HOME: studio }, 'darwin', pathJdk)).toBeNull();
    const { androidJavaFix } = await import('./agent-android.mjs');
    expect(androidJavaFix({ PATH: '' }, 'darwin', studio)).toContain(studio);
    expect(androidJavaFix({ ...env, JAVA_HOME: pathJdk }, 'darwin', studio)).toContain(pathJdk);
    expect(androidJavaFix({ ...env, JAVA_HOME: pathJdk }, 'darwin', studio)).not.toContain(studio);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The package suites read the roster, manifest and APK from apkBuildDir; under the reader's own build directory an
// outside app's were never found.
test("apkBuildDir holds the APK build --apk makes: an outside app's own target, a checkout app's the reader's", async () => {
  const { androidBinary, apkBuildDir } = await import('./agent-android.mjs');
  const { resolveApp } = await import('./app.mjs');
  const before = { EXACT_APP_DIR: process.env.EXACT_APP_DIR, EXACT_ANDROID_READER: process.env.EXACT_ANDROID_READER, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR };
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-apk-dir-'));
  try {
    delete process.env.EXACT_APP_DIR; delete process.env.CARGO_TARGET_DIR;
    process.env.EXACT_ANDROID_READER = '1';
    writeFileSync(resolve(dir, 'app.contract'), 'component Fixture\n  view\n    text "fixture"\n');
    const outside = resolveApp(dir);
    expect(apkBuildDir(dir)).toBe(resolve(outside.target, 'android/app'));
    expect(androidBinary(outside)).toBe(resolve(apkBuildDir(dir), 'outputs/apk/release/app-release.apk'));
    const root = resolve(import.meta.dir, '..');
    expect(apkBuildDir('caltrain')).toBe(resolve(root, 'host/android/app/build/caltrain'));
    expect(androidBinary(resolveApp('caltrain'))).toBe(resolve(apkBuildDir('caltrain'), 'outputs/apk/release/app-release.apk'));
  } finally {
    for (const [name, value] of Object.entries(before)) if (value === undefined) delete process.env[name]; else process.env[name] = value;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('Android distribution refuses bad metadata and incomplete credentials before compiling', async () => {
  const { androidReleaseEnvironment } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-upload-'));
  try {
    const store = resolve(dir, 'upload.jks'); writeFileSync(store, 'fixture');
    const signed = { EXACT_KEYSTORE: store, EXACT_KEYSTORE_PASSWORD: 'secret', EXACT_KEY_ALIAS: 'upload', EXACT_KEY_PASSWORD: 'secret' };
    expect(androidReleaseEnvironment({}, false).EXACT_BUILD_NUMBER).toBe('1');
    expect(androidReleaseEnvironment(signed, true).EXACT_KEYSTORE).toBe(store);
    expect(() => androidReleaseEnvironment({}, true)).toThrow('EXACT_KEYSTORE');
    for (const key of Object.keys(signed)) {
      const partial = { ...signed }; delete partial[key];
      expect(() => androidReleaseEnvironment(partial)).toThrow(key);
    }
    for (const code of ['0', '-1', '1.2', '2100000001', '', ' 42', '1e2']) {
      expect(() => androidReleaseEnvironment({ EXACT_BUILD_NUMBER: code })).toThrow('EXACT_BUILD_NUMBER');
    }
    expect(() => androidReleaseEnvironment({ ...signed, EXACT_KEYSTORE: store + '.missing' })).toThrow('EXACT_KEYSTORE');
    expect(() => androidReleaseEnvironment({ EXACT_VERSION: '' })).toThrow('EXACT_VERSION');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Android bundles and unattended APKs have explicit command flags', async () => {
  const { guardFlags } = await import('./help.mjs');
  expect(guardFlags('android', ['--bundle', '--json'])).toEqual(['--bundle', '--json']);
  expect(guardFlags('android', ['--no-install', '--release'])).toEqual(['--no-install', '--release']);
  for (const flags of [['--bundle', '--run'], ['--bundle', '--headless'], ['--no-install', '--run']]) {
    const r = spawnSync(process.execPath, [resolve(import.meta.dir, 'agent-android.mjs'), 'build', 'absent-app', ...flags], { encoding: 'utf8' });
    expect(r.status).toBe(1);
    expect(r.stderr).toContain('takes');
    expect(r.stderr).not.toContain('no app at');
  }
});

test('an APK carries each GPU library the bake built, by the name Cargo gave it, unstripped (#306)', async () => {
  const { bakeGpuLibraries, stageGpuLibraries } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-apk-gpu-'));
  try {
    const app = { hasGpu: true, manifest: { gpu: { modules: { extra: ['extra'] } } } };
    // The named module's crate sets `[lib] name = "extra"`: Cargo builds libextra.so, not libfiddl_gpu_extra.so.
    const products = ['libfiddl_gpu.so', 'libextra.so'].map((file) => resolve(dir, file));
    let asked;
    const bake = (...args) => { asked = args; for (const product of products) expect(args[3].prepareGpu(product)).toBe(product); };
    const libraries = bakeGpuLibraries(app, { profile: 'release', env: { A: '1' } }, bake);
    expect(libraries).toEqual(products);
    expect(asked.slice(0, 3)).toEqual([app, 'android', 'aarch64-linux-android']);
    expect(asked[3]).toMatchObject({ profile: 'release', env: { A: '1' }, kind: 'android' });
    for (const library of libraries) writeFileSync(library, library);
    const jniLibs = resolve(dir, 'jniLibs/arm64-v8a');
    mkdirSync(jniLibs, { recursive: true });
    expect(stageGpuLibraries(libraries, jniLibs)).toEqual(['-PgpuModules=libfiddl_gpu.so,libextra.so']);
    for (const library of libraries) expect(readFileSync(resolve(jniLibs, library.split('/').pop()), 'utf8')).toBe(library);
    expect(stageGpuLibraries([], jniLibs)).toEqual([]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('locks are written in a Gradle home with nothing resolved, keeping its configuration and the wrapper', async () => {
  const { coldGradleHome } = await import('./agent-android.mjs');
  const warm = mkdtempSync(resolve(tmpdir(), 'exact-warm-gradle-')), bare = mkdtempSync(resolve(tmpdir(), 'exact-bare-gradle-'));
  try {
    mkdirSync(resolve(warm, 'caches/modules-2/files-2.1/org.junit/junit-bom'), { recursive: true });
    mkdirSync(resolve(warm, 'wrapper/dists/gradle-9.8.0-bin/abc'), { recursive: true });
    mkdirSync(resolve(warm, 'init.d'));
    writeFileSync(resolve(warm, 'gradle.properties'), 'systemProp.https.proxyHost=proxy.invalid\n');
    writeFileSync(resolve(warm, 'init.d/mirror.gradle'), '// a mirror\n');
    writeFileSync(resolve(warm, 'init.gradle'), '// credentials\n');
    writeFileSync(resolve(warm, 'init.gradle.kts'), '// a repository\n');
    const home = coldGradleHome({ GRADLE_USER_HOME: warm });
    expect(existsSync(resolve(home, 'caches'))).toBe(false);
    expect(readFileSync(resolve(home, 'gradle.properties'), 'utf8')).toContain('proxy.invalid');
    expect(existsSync(resolve(home, 'init.d/mirror.gradle'))).toBe(true);
    expect(readFileSync(resolve(home, 'init.gradle'), 'utf8')).toContain('credentials');
    expect(readFileSync(resolve(home, 'init.gradle.kts'), 'utf8')).toContain('repository');
    expect(existsSync(resolve(home, 'wrapper/dists/gradle-9.8.0-bin/abc'))).toBe(true);
    // Removing the home leaves the distributions it linked.
    rmSync(home, { recursive: true, force: true });
    expect(existsSync(resolve(warm, 'wrapper/dists/gradle-9.8.0-bin/abc'))).toBe(true);
    // A home without distributions gets the directory the wrapper downloads into.
    rmSync(coldGradleHome({ GRADLE_USER_HOME: bare }), { recursive: true, force: true });
    expect(existsSync(resolve(bare, 'wrapper/dists'))).toBe(true);
  } finally {
    rmSync(warm, { recursive: true, force: true });
    rmSync(bare, { recursive: true, force: true });
  }
});

test('a locked Gradle failure keeps its output and names lock repair only for dependency failures', async () => {
  const { runGradle, androidLockCommand } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-gradle-advice-'));
  try {
    const script = resolve(dir, 'gradle.mjs'), app = { dir, name: 'fixture' };
    writeFileSync(script, `process.stdout.write('task progress\\n'); const [message, code] = process.argv.slice(2); process.stderr.write(message.slice(0, 9), () => process.stderr.write(message.slice(9) + '\\n', () => { process.exitCode = Number(code); }));`);
    const advice = `fixture: Gradle locks or verification metadata do not match its dependencies; for an intended graph change, run ${androidLockCommand(app)}, then review and commit gradle.lockfile and gradle-verification.xml`;
    for (const [message, code, expected] of [
      ['Dependency verification failed for configuration releaseRuntimeClasspath', 1, true],
      ['Resolved a module which is not part of the dependency lock state', 1, true],
      ['Did not resolve a module which is part of the dependency lock state', 1, true],
      ['Strict dependency locking is enabled and no lock state exists', 1, true],
      ['Compilation failed: unresolved reference', 1, false],
      ['Timeout waiting to lock the build cache', 1, false],
      ['Dependency verification failed in an expected fixture', 0, false],
    ]) {
      let result;
      const passed = runGradle(['--dependency-verification=strict'], { env: process.env }, process.platform, {
        lockAdvice: advice,
        run(command, args, options) {
          // Run the actual stream relay with a Gradle stand-in; no JDK, SDK or network is involved.
          const payload = [process.execPath, ['--no-env-file', script, message, String(code)], advice];
          const childArgs = args[0] === '--no-env-file' ? [...args.slice(0, -1), JSON.stringify(payload)] : ['--no-env-file', script, message, String(code)];
          result = spawnSync(process.execPath, childArgs, { ...options, stdio: 'pipe', encoding: 'utf8' });
          return result;
        },
      });
      expect(passed).toBe(code === 0);
      expect(result.stdout).toBe('task progress\n');
      expect(result.stderr).toContain(message);
      expect(result.stderr.includes(advice)).toBe(expected);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('an Android picker answer carries local file bytes and names to either carrier', async () => {
  const { sessionInput } = await import('./agent-input.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-agent-pick-'));
  try {
    const names = ['notes with spaces.txt', 'photo.png', ...(process.platform === 'win32' ? [] : ['foo\\bar.txt'])];
    const paths = names.map(name => resolve(dir, name));
    const bytes = [Buffer.from('notes\n'), readFileSync(resolve(import.meta.dir, 'fixtures/picker/photo.png')), Buffer.from('backslash\n')];
    paths.forEach((path, i) => writeFileSync(path, bytes[i]));
    const calls = [], s = {
      op: async req => {
        if (req.op === 'state') return { pending: [{ ticket: 7, device: { capability: 'pick' } }] };
        calls.push(req); return {};
      },
      landed: async r => r,
    };
    Object.assign(s, sessionInput(s, { carrier: {}, host: 'android' }));
    await s.answer('type', '@7', paths.join('\n'));
    expect(calls).toEqual([{ op: 'type', ticket: 7, text: paths.join('\n') + '\n',
      files: names.map((name, i) => ({ name, bytes: bytes[i].toString('base64') })),
    }]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('wireless adb entries for one phone select its address; distinct or unreadable devices refuse', async () => {
  const { androidDevice } = await import('./agent-android.mjs');
  const mdns = 'adb-R5CT1234ABC-a1B2c3 (2)._adb-tls-connect._tcp', ip = '192.168.1.20:37099';
  const env = {}, phone = id => ({ id, serial: 'R5CT1234ABC', androidId: 'd423b93a7ac32ef1' });
  let devices = [], calls = [];
  const run = (tool, args) => {
    calls.push(args);
    if (args.join(' ') === 'devices') return { status: 0,
      stdout: 'List of devices attached\n' + devices.map(d => d.id + '\t' + (d.state ?? 'device')).join('\n') };
    const identity = args[3] === 'settings';
    expect(args.slice(2)).toEqual(identity ? ['shell', 'settings', 'get', 'secure', 'android_id'] : ['shell', 'getprop', 'ro.serialno']);
    const d = devices.find(d => d.id === args[1]);
    const value = (identity ? d?.androidId : d?.serial) ?? '';
    return { status: (identity ? d?.identityStatus : d?.status) ?? 0, stdout: `${value}\r\n` };
  };
  const select = listed => {
    devices = listed;
    calls = [];
    return androidDevice(env, run);
  };
  const factory = '0123456789ABCDEF';
  expect(() => select([{ ...phone(ip), serial: factory },
    { ...phone(factory), serial: factory, androidId: 'another-phone' }]))
    .toThrow(`2 Android devices are ready (${ip}, ${factory}): set ANDROID_SERIAL to the one to use`);
  expect(() => select([{ ...phone(ip), serial: factory, androidId: 'null' },
    { ...phone(factory), serial: factory, androidId: 'null' }]))
    .toThrow(`2 Android devices are ready (${ip}, ${factory}): set ANDROID_SERIAL to the one to use`);
  expect(() => select([{ ...phone(ip), serial: 'unknown', androidId: 'null' },
    { ...phone('unknown'), serial: 'unknown', androidId: 'null' }]))
    .toThrow(`2 Android devices are ready (${ip}, unknown): set ANDROID_SERIAL to the one to use`);
  for (const other of [
    { ...phone(mdns), androidId: '' }, { ...phone(mdns), androidId: '   ' },
    { ...phone(mdns), identityStatus: 1 },
  ]) expect(() => select([phone(ip), other]))
    .toThrow(`2 Android devices are ready (${ip}, ${mdns}): set ANDROID_SERIAL to the one to use`);
  for (const ids of [[mdns, ip], [ip, mdns], [mdns, 'R5CT1234ABC', ip]]) {
    expect(select(ids.map(phone))).toBe(ip);
    expect(calls).toEqual([
      ['devices'], ...ids.map(id => ['-s', id, 'shell', 'getprop', 'ro.serialno']),
      ...ids.map(id => ['-s', id, 'shell', 'settings', 'get', 'secure', 'android_id']),
    ]);
  }
  for (const id of [mdns, ip, 'emulator-5554']) {
    expect(select([{ id }, { id: 'offline-phone', state: 'offline' }])).toBe(id);
    expect(calls).toEqual([['devices']]);
  }
  for (const ids of [[mdns, 'R5CT1234ABC'], ['R5CT1234ABC', mdns], [ip, 'R5CT1234ABC']])
    expect(select(ids.map(phone))).toBe(ids.includes(ip) ? ip : 'R5CT1234ABC');
  for (const ids of [
    ['emulator-5554', 'emulator-5556'], [ip, '192.168.1.21:37099'],
    ['USB123', 'USB456'], [mdns, 'USB123'],
    [mdns, 'adb-R5CT1234ABC-other._adb-tls-connect._tcp'],
    [ip, 'adb-OTHER-a1B2c3._adb-tls-connect._tcp'],
  ]) {
    expect(() => select(ids.map(phone))).toThrow(`${ids.length} Android devices are ready (${ids.join(', ')}): set ANDROID_SERIAL to the one to use`);
  }
  for (const other of [
    { id: 'other-phone', serial: 'OTHER' }, { id: 'emulator-5554', serial: 'EMULATOR35X1' },
    { id: mdns }, { id: mdns, serial: '   ' }, { id: mdns, serial: 'R5CT1234ABC', status: 1 },
  ]) {
    expect(() => select([phone(ip), other])).toThrow(`2 Android devices are ready (${ip}, ${other.id}): set ANDROID_SERIAL to the one to use`);
  }
  expect(() => select([])).toThrow('no Android device or emulator is ready');
  calls = [];
  expect(androidDevice({ ...env, ANDROID_SERIAL: mdns }, run)).toBe(mdns);
  expect(calls).toEqual([]);
});

test('android --run --url launches the app following that dev loop, stopping a running one first (#585)', async () => {
  const { launchArgs, reversedPorts } = await import('./agent-android.mjs');
  expect(launchArgs('com.example.app')).toEqual(['shell', 'am', 'start', '-W', '-n', 'com.example.app/dev.exact.app.MainActivity']);
  expect(launchArgs('com.example.app', 'http://127.0.0.1:9147/')).toEqual(['shell', 'am', 'start', '-W', '-S', '-n',
    'com.example.app/dev.exact.app.MainActivity', '--es', 'x_dev_plan', 'http://127.0.0.1:9147/']);
  expect(reversedPorts({ EXACT_DEV_PLAN: 'http://127.0.0.1:9147/' })).toEqual(['9147']);
  const refused = (...args) => spawnSync(process.execPath, [resolve(import.meta.dir, 'agent-android.mjs'), 'build', 'shelf', ...args], { encoding: 'utf8', timeout: 30_000 });
  expect(refused('--url', 'http://127.0.0.1:9147/').stderr).toContain('--url is the dev loop the launched APK follows');
  expect(refused('--run', '--headless', '--url', 'http://127.0.0.1:9147/').stderr).toContain('it goes with --run, not --headless');
  expect(refused('--run', '--url', 'shelf.contract').stderr).toContain("--url takes a dev loop's http(s) URL");
});


test('Android compilers and GPU bakes use the requested x86_64 ABI', async () => {
  const { androidToolchainEnv, bakeGpuLibraries, soundLibraryCommand } = await import('./agent-android.mjs');
  const { ANDROID_PAGE_SIZE: page, androidCargo } = await import('./cargo-check.mjs');
  const [, intelArgs, intelOptions] = androidCargo('cargo', ['build', '--target=x86_64-linux-android'],
    { env: { CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS: '-llog' } });
  expect(intelArgs).toContain(`build.rustflags=["${page}","-Clink-arg=-lc++abi"]`);
  expect(intelOptions.env.CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS).toBe(`-llog ${page} -Clink-arg=-lc++abi`);
  const { androidAbis, androidHermesDir } = await import('./android-abi.mjs');
  expect(androidAbis()).toEqual(['arm64-v8a']);
  expect(androidAbis('arm64-v8a,x86_64,x86_64')).toEqual(['arm64-v8a', 'x86_64']);
  expect(() => androidAbis('armeabi-v7a')).toThrow('--abis');
  expect(androidHermesDir({ HERMES_LEAN_SYS_DIR: '/host' }, '/arm64', 'x86_64-linux-android')).toBeNull();
  expect(androidHermesDir({ HERMES_LEAN_SYS_DIR_x86_64_linux_android: '/intel' }, '/arm64', 'x86_64-linux-android')).toBe('/intel');
  const { guardFlags } = await import('./help.mjs');
  expect(guardFlags('android', ['--abis', 'arm64-v8a,x86_64'])).toEqual(['--abis', 'arm64-v8a,x86_64']);
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-abi-'));
  try {
    const bin = resolve(dir, 'toolchains/llvm/prebuilt/darwin-x86_64/bin');
    mkdirSync(bin, { recursive: true });
    for (const target of ['aarch64-linux-android', 'x86_64-linux-android'])
      writeFileSync(resolve(bin, `${target}29-clang`), '');
    const target = 'x86_64-linux-android';
    const env = androidToolchainEnv({ ANDROID_NDK_HOME: dir }, 'darwin', 29, target);
    expect(env.CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER).toBe(resolve(bin, `${target}29-clang`));
    expect(env.CXX_x86_64_linux_android).toBe(resolve(bin, `${target}29-clang++`));
    expect(env.HERMES_LEAN_SYS_DIR_x86_64_linux_android).toBeUndefined();
    expect(soundLibraryCommand('libexact_sound.so', env, target)[0]).toBe(env.CC_x86_64_linux_android);
    let baked;
    bakeGpuLibraries({}, { target, profile: 'host-dev', env }, (...args) => { baked = args; });
    expect(baked[2]).toBe(target);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('APK rebuilds preserve selected ABIs and freshness covers both libraries', async () => {
  const { apkBuildFirst, apkDirs } = await import('./agent-android.mjs');
  const { apkChanges, androidRebuild } = await import('./agent-stdio.mjs');
  const cp = await import('node:child_process');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-apk-inputs-'));
  const app = { dir, target: resolve(dir, 'target'), name: 'fixture', manifest: { app: { id: 'dev.fixture' } } };
  writeFileSync(resolve(dir, 'exact.mjs'), '');
  const apk = resolve(apkDirs(app).app, 'outputs/apk/release/app-release.apk');
  const record = resolve(dirname(apk), 'exact-build.json');
  const libraries = ['arm64', 'x86_64'].map(abi => resolve(dir, abi + '.so'));
  const sources = ['shared.rs', 'arm64.rs', 'x86_64.rs'].map(file => resolve(dir, file));
  const before = new Date(Date.now() - 60_000), after = new Date(Date.now() + 60_000);
  mkdirSync(dirname(apk), { recursive: true });
  sources.forEach(file => { writeFileSync(file, 'input'); utimesSync(file, before, before); });
  libraries.forEach((file, i) => writeFileSync(file.replace(/\.so$/, '.d'), `${file}: ${sources[0]} ${sources[i + 1]}\n`));
  const run = spyOn(cp, 'spawnSync').mockReturnValue({ status: 0, stdout: '', stderr: '' });
  try {
    for (const abis of [['x86_64'], ['arm64-v8a', 'x86_64']]) {
      writeFileSync(record, JSON.stringify({ started: new Date().toISOString(), abis, libraries, workspace: dir, sources: [] }));
      expect(androidRebuild(app, true)).toEndWith(`android --abis ${abis.join(',')})`);
      expect((await apkBuildFirst(app)).code).toBe(0);
      expect(run.mock.calls.at(-1)[1]).toEqual([resolve(import.meta.dir, 'agent-android.mjs'), 'build', dir, '--apk', '--abis', abis.join(',')]);
      writeFileSync(apk, 'APK');
      utimesSync(sources[0], after, after);
    }
    utimesSync(sources[0], before, before);
    expect(apkChanges(app, apk)).toEqual([]);
    for (const source of sources) {
      utimesSync(source, after, after);
      expect(apkChanges(app, apk)).toHaveLength(1);
      expect(apkChanges(app, apk)[0]).toEndWith(source.split('/').at(-1));
      utimesSync(source, before, before);
    }
    rmSync(sources[2]);
    expect(apkChanges(app, apk)[0]).toEndWith('x86_64.rs');
    writeFileSync(record, '{}');
    expect(apkChanges(app, apk)[0]).toContain('no complete ABI build record');
  } finally { run.mockRestore(); rmSync(dir, { recursive: true, force: true }); }
});


test('TypeScript x86_64 refuses before compiling even with a Hermes override', () => {
  const r = spawnSync(process.execPath, [resolve(import.meta.dir, 'agent-android.mjs'), 'build', 'shelf', '--apk', '--abis', 'x86_64'], {
    encoding: 'utf8', env: { ...process.env, HERMES_LEAN_SYS_DIR_x86_64_linux_android: '/unused-bundle' },
  });
  expect(r.status).toBe(1);
  expect(r.stderr).toContain('vendored Hermes resolver does not support Android x86_64 ICU');
  expect(r.stderr).toContain('Use --abis arm64-v8a');
  expect(r.stderr).not.toContain('Compiling');
}, 30_000);
