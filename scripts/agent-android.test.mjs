// The Android carrier's reader variables, drive setup and reverse mappings (agent-android.mjs).
// Stand-in adb devices exercise relay cleanup without an emulator or a device.
import { test, expect } from 'bun:test';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pollUntil } from './agent-launch.mjs';
import { androidJavaEnv, androidSdk, androidSdkPackages, androidScreenState, reverseRoot } from './agent-android.mjs';

const screenFixtures = resolve(import.meta.dir, 'fixtures/android-screen');
const screenDump = (state, kind) => readFileSync(resolve(screenFixtures, `${state}.${kind}.txt`), 'utf8');

test('the Android reader variable filter and activity allowlist include the auth fixture; loopback URLs select ports', async () => {
  const { readerVariables, reversedPorts } = await import('./agent-android.mjs');
  const vars = readerVariables({ EXACT_AGENT: '1', EXACT_AGENT_STORAGE: 's', EXACT_AUTH_FIXTURE: 'http://127.0.0.1:4791', EXACT_APP_DIR: '/a', HOME: '/h' });
  expect(vars).toEqual({ EXACT_AGENT: '1', EXACT_AGENT_STORAGE: 's', EXACT_AUTH_FIXTURE: 'http://127.0.0.1:4791' });
  expect(reversedPorts(vars)).toEqual(['4791']);
  expect(reversedPorts({ EXACT_AUTH_FIXTURE: 'http://[::1]:4791' })).toEqual(['4791']);
  expect(reversedPorts({ EXACT_DEV_PLAN: 'http://localhost:8080/', EXACT_AUTH_FIXTURE: 'http://127.0.0.1:8080' })).toEqual(['8080']);
  expect(reversedPorts({ EXACT_DEV_PLAN: 'https://example.com/plan' })).toEqual([]);
  const activity = readFileSync(resolve(import.meta.dir, '../host/android/canvas/src/main/java/dev/exact/canvas/ExactActivity.kt'), 'utf8');
  expect(Object.keys(vars).map((name) => `x_${name.slice('EXACT_'.length).toLowerCase()}`).filter((extra) => !activity.includes(`"${extra}"`))).toEqual([]);
});

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
import { appendFileSync, existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
const args = process.argv.slice(2);
appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + '\\n');
const cmd = args.slice(2);
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
  return { port: server.port, app: { id: 'dev.exact.fixture', dir }, bin, env, closed, connected,
    commands: () => readFileSync(calls, 'utf8').trim().split('\n').map(JSON.parse),
    mappings: () => JSON.parse(readFileSync(mappings, 'utf8')), stopSocket: () => server.stop(true),
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
}
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
    const screenCalls = commands.filter(args => args[0] === 'shell' && ['dumpsys', 'input', 'wm'].includes(args[1]));
    const check = [['shell', 'dumpsys', 'power'], ['shell', 'dumpsys', 'window', 'policy']];
    expect(screenCalls).toEqual(wakefulness === 'Awake' ? check : [
      ...check, ['shell', 'input', 'keyevent', 'KEYCODE_WAKEUP'], ...check, ['shell', 'wm', 'dismiss-keyguard'], ...check,
    ]);
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
    expect(commands).not.toContainEqual(['shell', 'dumpsys', 'window']);
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

for (const kill of ['child', 'pid']) test(`killing the Android headless ${kill} stops its adb shell before releasing its reverse`, async () => {
  const { androidDeploy } = await import('./agent-android.mjs');
  const fixture = androidDriveFixture(false, null, false, null, true);
  let child, closed, shell;
  try {
    child = androidDeploy(fixture.app, fixture.bin, fixture.env).spawn();
    closed = once(child, 'close');
    await once(child.stdout, 'data');
    [shell] = fixture.shells();
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

test('killing an Android carrier group leaves an in-flight reverse mutation running', async () => {
  const fixture = androidDriveFixture(false), gate = fixture.holdReverse('--remove');
  let driver, closed, holder;
  try {
    driver = spawn(process.execPath, ['--eval', `
      import { androidDeploy } from ${JSON.stringify(resolve(import.meta.dir, 'agent-android.mjs'))};
      const child = androidDeploy(${JSON.stringify(fixture.app)}, ${JSON.stringify(fixture.bin)}, process.env).spawn();
      child.stdout.pipe(process.stdout); child.stderr.pipe(process.stderr);
      child.stdout.once('data', () => child.stdin.end('{"op":"quit"}\\n'));
      process.stdin.once('data', () => { child.kill('SIGKILL'); console.log('killed'); });
      child.on('close', code => process.exit(code ?? 1));
    `], { env: fixture.env, stdio: ['pipe', 'pipe', 'pipe'] });
    closed = once(driver, 'close');
    await once(driver.stdout, 'data');
    holder = await pollUntil(gate.entered, Boolean, { bound: 8000 });
    expect(holder).toBeGreaterThan(0);
    const killed = once(driver.stdout, 'data');
    driver.stdin.write('kill'); await killed;
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

const releaseJavaEnv = androidJavaEnv(process.env);
const releaseSdk = androidSdk(process.env);
test.skipIf(process.platform !== 'darwin' || !releaseJavaEnv ||
  !androidSdkPackages().every(pkg => existsSync(resolve(releaseSdk, pkg.path))))('Android release metadata and signing reach Gradle without patching its source', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-release-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'dev.exact.ci', name: 'CI' } }));
    writeFileSync(resolve(dir, 'upload.jks'), 'configuration fixture');
    const init = resolve(dir, 'inspect.gradle');
    writeFileSync(init, `allprojects { repositories { maven { url = uri('https://127.0.0.1:9/unused-ci-mirror') } } }
    gradle.projectsEvaluated {
      def app = gradle.rootProject.project(':app')
      app.tasks.register('inspectRelease') {
        doLast {
          def android = app.extensions.getByName('android')
          def signing = android.buildTypes.release.signingConfig
          println('EXACT_RELEASE=' + groovy.json.JsonOutput.toJson([
            code: android.defaultConfig.versionCode, version: android.defaultConfig.versionName,
            signing: signing.name, store: signing.storeFile.name,
            passwords: signing.storePassword == 'store-secret' && signing.keyPassword == 'key-secret',
            alias: signing.keyAlias]))
        }
      }
    }`);
    const result = spawnSync(resolve(import.meta.dir, '../host/android/gradlew'), ['--no-daemon', '--console=plain', '-I', init,
      ':app:inspectRelease', '-Papp=ci', `-PappDir=${dir}`, `-PappBuildDir=${dir}/app`, `-PcanvasBuildDir=${dir}/canvas`, '--project-cache-dir', `${dir}/cache`],
    { cwd: resolve(import.meta.dir, '../host/android'), encoding: 'utf8', timeout: 180000,
      env: { ...releaseJavaEnv, ANDROID_HOME: releaseSdk,
        EXACT_BUILD_NUMBER: '42', EXACT_VERSION: '2.3.4', EXACT_KEYSTORE: resolve(dir, 'upload.jks'),
        EXACT_KEYSTORE_PASSWORD: 'store-secret', EXACT_KEY_ALIAS: 'upload', EXACT_KEY_PASSWORD: 'key-secret' } });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    const line = result.stdout.split('\n').find(line => line.startsWith('EXACT_RELEASE='));
    expect(JSON.parse(line.slice('EXACT_RELEASE='.length))).toEqual({ code: 42, version: '2.3.4', signing: 'upload', store: 'upload.jks', passwords: true, alias: 'upload' });
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 200000);

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

test('a production Android build with the Linux bake store on and no stream is refused before compiling (#48)', async () => {
  const { androidStreamRefusal } = await import('./agent-android.mjs');
  const production = { EXACT_UPDATE_TRUST: 'production' };
  expect(androidStreamRefusal('a', {}, production)).toContain('deploy.store.linux to "0"');
  expect(androidStreamRefusal('a', { deploy: { store: { android: '0' } } }, production)).toContain('deploy.store.android does not');
  expect(androidStreamRefusal('a', { deploy: { store: { linux: '0' } } }, production)).toBeNull();
  expect(androidStreamRefusal('a', {}, {})).toBeNull();
  expect(androidStreamRefusal('a', {}, { ...production, EXACT_UPDATE_GENESIS: '1' })).toBeNull();
  expect(androidStreamRefusal('a', {}, { ...production, EXACT_UPDATE_RECEIPT: '/receipt.json' })).toBeNull();
  expect(androidStreamRefusal('a', {}, { ...production, EXACT_BAKE_ANALYSIS: '1' })).toBeNull();
  // The verb says it in its first second: `--bundle` bakes production trust, and this app's store is on.
  const env = { ...process.env };
  for (const key of ['EXACT_UPDATE_TRUST', 'EXACT_UPDATE_RECEIPT', 'EXACT_UPDATE_GENESIS', 'EXACT_BAKE_ANALYSIS']) delete env[key];
  const r = spawnSync(process.execPath, [resolve(import.meta.dir, 'agent-android.mjs'), 'build', 'auth-fixture', '--bundle'], { encoding: 'utf8', env, timeout: 30000 });
  expect(r.status).toBe(1);
  expect(r.stderr).toContain('set deploy.store.linux to "0" in app.json');
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
  const { bakeGpuLibraries, carriesGpu, stageGpuLibraries } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-apk-gpu-'));
  try {
    const app = { hasGpu: true, manifest: { gpu: { modules: { extra: ['extra'] } } } };
    expect(carriesGpu(app)).toBe(true);
    // An app whose surfaces are all in named modules has no primary, and still bakes them.
    expect(carriesGpu({ ...app, hasGpu: false })).toBe(true);
    expect(carriesGpu({ hasGpu: false, manifest: {} })).toBe(false);
    // The named module's crate sets `[lib] name = "extra"`: Cargo builds libextra.so, not libfiddl_gpu_extra.so.
    const products = ['libfiddl_gpu.so', 'libextra.so'].map((file) => resolve(dir, file));
    let asked;
    const bake = (...args) => { asked = args; for (const product of products) expect(args[3].prepareGpu(product)).toBe(product); };
    const libraries = bakeGpuLibraries(app, { profile: 'release', env: { A: '1' } }, bake);
    expect(libraries).toEqual(products);
    expect(asked.slice(0, 3)).toEqual([app, 'linux', 'aarch64-linux-android']);
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
