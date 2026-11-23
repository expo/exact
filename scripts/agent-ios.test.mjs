// Native test runs reuse their startup check and simulator lookups (expo/exact#1400).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { appleArtifacts, appleRunBinary } from '../host/apple/build.mjs';
import { buildFirst, nativeBuild } from './agent-build.mjs';
import { resolveApp } from './app.mjs';

const root = resolve(import.meta.dir, '..');
function fixture() {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-launch-once-')));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'launch-once', app: { id: 'com.exact.launch-once', name: 'Launch Once' } }));
  writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "hello"\n');
  return dir;
}
function run(script, dir) {
  const env = { ...process.env, EXACT_APP_DIR: dir, DEVELOPER_DIR: '/stub/Xcode' };
  for (const key of ['CARGO_TARGET_DIR', 'EXACT_MAC_BIN', 'EXACT_BUILD_CHECKED', 'EXACT_BUILD_CHECK_OUT', 'EXACT_APPLE_OUTPUT', 'EXACT_BAKE_OUTPUT', 'EXACT_BACKEND_ORIGIN', 'EXACT_SIM', 'EXACT_VARIANT']) delete env[key];
  const r = spawnSync(process.execPath, ['-e', script], { cwd: root, env, encoding: 'utf8', timeout: 20000 });
  assert.equal(r.status, 0, r.stdout + r.stderr + (r.error ?? ''));
}

test('test and agent carry one native startup check to every test file on iOS and macOS', () => {
  const dir = fixture();
  for (const name of ['one', 'two']) writeFileSync(resolve(dir, `${name}.test.contract`), 'test "opens"\n');
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import * as processes from 'node:child_process';
    import * as shells from './scripts/app-shells.mjs';
    import * as builds from './scripts/web-build.mjs';
    import { writeFileSync } from 'node:fs';
    const spawn = processes.spawn;
    let checks = 0, launches = 0;
    mock.module('./scripts/app-shells.mjs', () => ({ ...shells, platformOf: () => null }));
    mock.module('./scripts/web-build.mjs', () => ({ ...builds, privateNativeBuild: () => ({ env: {}, seed() {}, close() {} }) }));
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
      if (args[0].endsWith('/scripts/agent-build.mjs')) {
        checks++;
        assert.ok(options.env.EXACT_BUILD_CHECK_OUT);
        writeFileSync(options.env.EXACT_BUILD_CHECK_OUT, 'checked-' + checks);
      } else if (args[0].endsWith('/scripts/agent.mjs')) {
        launches++;
        assert.equal(options.env.EXACT_BUILD_CHECKED, 'checked-' + checks);
      } else assert.fail('unexpected subprocess: ' + args);
      return spawn(command, ['-e', ''], options);
    } }));
    const { run } = await import('./scripts/app-cli.mjs');
    for (const host of ['ios', 'macos']) for (const verb of ['test', 'agent']) {
      const before = [checks, launches];
      assert.equal(await run({ dir: process.env.EXACT_APP_DIR, name: 'launch-once', exact2: process.cwd(),
        args: [verb, host, ...(verb === 'agent' ? ['tree'] : [])] }), 0);
      assert.equal(checks - before[0], 1);
      assert.equal(launches - before[1], verb === 'test' ? 2 : 1);
    }
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('two iOS launches check once and look up each device once; reinstalls refresh the container', () => {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { EventEmitter } from 'node:events';
    import { PassThrough } from 'node:stream';
    import * as processes from 'node:child_process';
    import * as net from 'node:net';
    import * as devices from './host/apple/devices.mjs';
    import * as builds from './host/apple/build.mjs';
    import * as launch from './scripts/agent-launch.mjs';
    import * as wire from './scripts/agent-wire.mjs';
    import * as touches from './host/apple/touches.mjs';
    import * as cleanup from './scripts/sweep.mjs';
    import { existsSync, mkdirSync, readFileSync, utimesSync, writeFileSync } from 'node:fs';
    import { dirname, resolve } from 'node:path';
    import { resolveApp } from './scripts/app.mjs';
    const dir = process.env.EXACT_APP_DIR, app = resolveApp(dir), records = resolve(dir, 'keyboard-records');
    const paths = builds.appleArtifacts(app, { destination: 'ios-simulator' });
    const receipt = resolve(paths.products, 'receipt.json');
    mkdirSync(paths.bundle, { recursive: true }); mkdirSync(dirname(receipt), { recursive: true });
    const time = new Date(Date.now() + 60000);
    writeFileSync(receipt, JSON.stringify({ target: paths.target, started: time.toISOString(), build: { trust: 'development' } }));
    utimesSync(receipt, time, time);
    const realSpawnSync = processes.spawnSync;
    const counts = { check: 0, list: 0, boot: 0, keyboard: {}, container: {} }, settings = { A: '1', B: '0' }, assets = [];
    const states = { A: 'Shutdown', B: 'Booted' };
    let reinstall = false, failRestore = false, generation = 1, now = Date.now();
    Date.now = () => now;
    const container = udid => resolve(dir, udid + '-' + generation + '.app');
    for (const udid of ['A', 'B']) mkdirSync(container(udid));
    const result = stdout => ({ status: 0, stdout: stdout ?? '', stderr: '' });
    const simctl = args => {
      assert.equal(args[0], 'simctl');
      if (args[1] === 'list') {
        counts.list++;
        return result(JSON.stringify({ devices: { 'com.apple.CoreSimulator.SimRuntime.iOS-27-0': ['A', 'B'].map(udid => ({
          udid, name: udid, state: states[udid], deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro',
        })) } }));
      }
      if (args[1] === 'boot') { counts.boot++; states[args[2]] = 'Booted'; return result(); }
      if (args[1] === 'bootstatus') return result();
      if (args[1] === 'get_app_container') {
        counts.container[args[2]] = (counts.container[args[2]] ?? 0) + 1;
        return result(container(args[2]));
      }
      if (args[1] === 'spawn' && args[3] === 'defaults') {
        const udid = args[2], verb = args[4];
        const count = counts.keyboard[udid] ??= { read: 0, delete: 0, write: 0 };
        count[verb]++;
        if (verb === 'read') return result(settings[udid]);
        if (verb === 'delete') { settings[udid] = null; return result(); }
        if (verb === 'write') { if (failRestore) return { status: 1 }; settings[udid] = '1'; return result(); }
      }
      assert.fail('unexpected simctl: ' + args);
    };
    mock.module('./scripts/sweep.mjs', () => ({ ...cleanup, startSweep() {} }));
    mock.module('node:child_process', () => ({ ...processes,
      spawnSync(command, args, options) {
        if (command === 'git') return realSpawnSync(command, args, options);
        assert.equal(command, 'xcrun'); return simctl(args);
      },
      spawn(command, args) {
        assert.equal(command, 'xcrun');
        const child = new EventEmitter(); child.stdout = new PassThrough(); child.kill = () => {};
        const r = simctl(args);
        queueMicrotask(() => { child.stdout.write(r.stdout); child.emit('close', r.status); });
        return child;
      },
    }));
    mock.module('node:net', () => ({ ...net, connect() {
      const socket = new EventEmitter(); socket.end = () => socket.emit('close');
      queueMicrotask(() => socket.emit('connect')); return socket;
    } }));
    const receiptChanges = launch.receiptChanges;
    mock.module('./scripts/agent-launch.mjs', () => ({ ...launch,
      receiptChanges(...args) { counts.check++; return receiptChanges(...args); },
      exclusiveIOS: (udid, id, open) => open(),
    }));
    const withSoftwareKeyboard = devices.withSoftwareKeyboard;
    mock.module('./host/apple/devices.mjs', () => ({ ...devices,
      withSoftwareKeyboard: (dev, open) => withSoftwareKeyboard(dev, open, { records }),
      showSimulator() {}, crashWatch: () => () => [], keepInstalledRelease: () => ({ install: reinstall }),
    }));
    mock.module('./host/apple/build.mjs', () => ({ ...builds, install() {
      generation++; mkdirSync(container(process.env.EXACT_SIM)); reinstall = false;
    } }));
    mock.module('./host/apple/touches.mjs', () => ({ ...touches, withAccessibilityRuntime: (udid, enabled, open) => open() }));
    mock.module('./scripts/agent-wire.mjs', () => ({ ...wire,
      simulatorConsole: () => ({ done: states[process.env.EXACT_SIM] !== 'Booted', retry: () => false,
        exited: Promise.resolve(0), process: { kill() {} } }),
      jsonLines: () => ({ next: async () => ({ ready: true }), ask: async () => ({}), fail() {} }),
    }));
    // The asset root is read at launch, before any fake wire traffic.
    const fsAssets = await import('./host/apple/assets.mjs');
    mock.module('./host/apple/assets.mjs', () => ({ ...fsAssets, simulatorAssetRoot(installed) { assets.push(installed); return installed; } }));
    const { buildFirst, nativeBuild } = await import('./scripts/agent-build.mjs');
    const { openIOS } = await import('./scripts/agent-ios.mjs');
    process.env.EXACT_BUILD_CHECK_OUT = resolve(dir, 'checked');
    assert.equal(await buildFirst('ios', app, { wait() {}, build() { assert.fail('no native build'); } }), 0);
    process.env.EXACT_BUILD_CHECKED = readFileSync(process.env.EXACT_BUILD_CHECK_OUT, 'utf8');
    process.env.EXACT_SIM = 'A';
    for (let i = 0; i < 2; i++) {
      const carrier = await openIOS({ app: dir });
      assert.equal(settings.A, i === 0 ? null : '1');
      assert.equal(existsSync(resolve(records, 'A.json')), i === 0);
      await carrier.close();
      assert.equal(settings.A, '1');
      assert.ok(!existsSync(resolve(records, 'A.json')));
      writeFileSync(resolve(dir, 'app.contract'), 'edited during run');
      utimesSync(resolve(dir, 'app.contract'), new Date(time.getTime() + 10000), new Date(time.getTime() + 10000));
      now += 6000; // Past the old five-second device-list cache, without waiting.
    }
    assert.deepEqual(counts, { check: 1, list: 1, boot: 1, keyboard: { A: { read: 2, delete: 1, write: 1 } }, container: { A: 1 } });
    assert.deepEqual(assets, [container('A'), container('A')]);
    // A second device has its own cache, including a keyboard read that needs no warning.
    process.env.EXACT_SIM = 'B';
    for (let i = 0; i < 2; i++) { const carrier = await openIOS({ app: dir }); await carrier.close(); now += 6000; }
    assert.deepEqual(counts, { check: 1, list: 2, boot: 1, keyboard: { A: { read: 2, delete: 1, write: 1 }, B: { read: 1, delete: 0, write: 0 } }, container: { A: 1, B: 1 } });
    assert.equal(settings.B, '0');
    process.env.EXACT_SIM = 'A'; reinstall = true;
    const old = container('A'), carrier = await openIOS({ app: dir }); await carrier.close();
    assert.ok(existsSync(old)); // Invalidation must work even while the old directory remains.
    assert.notEqual(assets.at(-1), old);
    assert.equal(assets.at(-1), container('A'));
    assert.equal(counts.container.A, 2);
    assert.equal(counts.check, 1);
    // A shutdown fails one launch. Even within five seconds, the next launch re-lists and boots.
    states.A = 'Shutdown';
    await assert.rejects(openIOS({ app: dir }), /simulator launch exited/);
    const recoveredLaunch = await openIOS({ app: dir });
    assert.equal(settings.A, null);
    await recoveredLaunch.close();
    assert.equal(settings.A, '1');
    const nextLaunch = await openIOS({ app: dir }); await nextLaunch.close();
    assert.equal(counts.list, 3);
    assert.equal(counts.boot, 2);
    assert.equal(counts.container.A, 3);
    assert.deepEqual(counts.keyboard.A, { read: 4, delete: 2, write: 2 });
    assert.equal(counts.check, 1);
    assert.ok(nativeBuild('ios', app).changed.some(path => path.endsWith('app.contract')), 'the next run sees the edit');
    const dev = { udid: 'A', bootedHere: true }, record = resolve(records, 'A.json');
    const open = callback => withSoftwareKeyboard(dev, callback, { records });
    await assert.rejects(open(() => { throw new Error('open failed'); }), /open failed/);
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));
    const badClose = await open(() => ({ close() { throw new Error('close failed'); } }));
    await assert.rejects(badClose.close(), /close failed/);
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));
    // The same exit callback still restores an open session.
    const beforeExit = process.listeners('exit');
    const atExit = await open(() => ({ close() {} }));
    process.listeners('exit').filter(fn => !beforeExit.includes(fn)).forEach(fn => fn());
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));
    await atExit.close();
    // A failed write leaves a record until a later session can put the setting back.
    const failedRestore = await open(() => ({ close() {} }));
    failRestore = true; await failedRestore.close();
    assert.equal(settings.A, null); assert.ok(existsSync(record));
    const stillFailed = await open(() => ({ close() {} })); await stillFailed.close();
    assert.ok(existsSync(record));
    failRestore = false;
    const recovered = await open(() => ({ close() {} })); await recovered.close();
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('macOS freshness checks the full product receipt when the bundle carries a reduced receipt', async () => {
  const checkOut = process.env.EXACT_BUILD_CHECK_OUT;
  const previous = process.env.EXACT_APP_DIR, target = process.env.CARGO_TARGET_DIR, dir = fixture();
  const write = (path, text) => writeFileSync(resolve(dir, path), text);
  delete process.env.CARGO_TARGET_DIR;
  try {
    process.env.EXACT_APP_DIR = dir;
    const app = resolveApp(dir), source = resolve(dir, 'source.swift');
    write('source.swift', 'before');
    const paths = appleArtifacts(app), executable = resolve(paths.bundle, 'Contents/MacOS', paths.executable);
    mkdirSync(dirname(executable), { recursive: true }); writeFileSync(executable, '');
    const receipt = resolve(paths.products, 'receipt.json'), time = new Date(Date.now() + 60_000);
    mkdirSync(dirname(receipt), { recursive: true });
    writeFileSync(receipt, JSON.stringify({ target: paths.target, started: time.toISOString(), build: { trust: 'development', binary: { sha256: 'dev',
      inputs: [{ name: 'source.swift', path: source, sha256: createHash('sha256').update('before').digest('hex') }], missing: [], directories: [] } } }));
    utimesSync(receipt, time, time);
    const bundled = resolve(paths.bundle, 'Contents/Resources/receipt.json');
    mkdirSync(dirname(bundled), { recursive: true });
    const shipped = JSON.parse(readFileSync(receipt, 'utf8')); shipped.build.binary = { sha256: 'dev' };
    writeFileSync(bundled, JSON.stringify(shipped));
    assert.equal(nativeBuild('macos', app).receipt, receipt);
    assert.deepEqual(nativeBuild('macos', app).changed, []);
    process.env.EXACT_BUILD_CHECK_OUT = resolve(dir, '.exact/checked');
    mkdirSync(dirname(process.env.EXACT_BUILD_CHECK_OUT), { recursive: true });
    assert.equal(await buildFirst('macos', app, { wait() {} }), 0);
    const checked = readFileSync(process.env.EXACT_BUILD_CHECK_OUT, 'utf8');
    write('source.swift', 'after');
    // Newer than the receipt, so only a run's own check (not the file-time walk) can pass it.
    const later = new Date(time.getTime() + 10_000);
    utimesSync(source, later, later);
    assert.ok(nativeBuild('macos', app).changed.includes('source.swift'));
    for (let launch = 0; launch < 2; launch++) assert.deepEqual(nativeBuild('macos', app, { checked }).changed, []);
    const replaced = JSON.parse(readFileSync(bundled, 'utf8')); replaced.build.binary.sha256 = 'other';
    writeFileSync(bundled, JSON.stringify(replaced));
    assert.deepEqual(nativeBuild('macos', app, { checked }).changed, [bundled]);
    write('source.swift', 'before');
  } finally {
    if (checkOut === undefined) delete process.env.EXACT_BUILD_CHECK_OUT; else process.env.EXACT_BUILD_CHECK_OUT = checkOut;
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    if (target === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = target;
    rmSync(dir, { recursive: true, force: true });
  }
});
