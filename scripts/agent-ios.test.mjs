// Native test runs reuse their startup check and simulator lookups (expo/exact#1400).
import { spyOn, test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { appleArtifacts } from '../host/apple/build.mjs';
import { buildFirst, nativeBuild } from './agent-build.mjs';
import { foldTaken, rotateSimulator } from './duo/hinge.mjs';
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

function withPlanBuild(script) {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { createHash } from 'node:crypto';
    import { existsSync, mkdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
    import { dirname, resolve } from 'node:path';
    import * as builds from './host/apple/build.mjs';
    import * as devices from './host/apple/devices.mjs';
    import { buildFirst, nativeBuild, planOverride } from './scripts/agent-build.mjs';
    import { resolveApp } from './scripts/app.mjs';
    import { linkedBakeDir } from './scripts/linked-plan.mjs';
    const dir = process.env.EXACT_APP_DIR, app = resolveApp(dir);
    const check = process.env.EXACT_BUILD_CHECK_OUT = resolve(dir, '.exact/checked');
    process.env.EXACT_BAKE_OUTPUT = resolve(app.target, 'bake');
    process.env.EXACT_UPDATE_TRUST = 'development';
    const source = resolve(dir, 'source.swift'), contract = resolve(dir, 'app.contract');
    const hash = bytes => createHash('sha256').update(bytes).digest('hex');
    const module = JSON.stringify({ appId: app.id, abi: 1, bytecodeVersion: 96 });
    const write = (path, bytes) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, bytes); };
    const unexpected = () => assert.fail('the drive reached the native build or carrier');
    mock.module('./host/apple/devices.mjs', () => ({ ...devices, simulator: unexpected }));
    mock.module('./host/apple/build.mjs', () => ({ ...builds, assertAppleIdentity: unexpected }));
    const setup = host => {
      const paths = builds.appleArtifacts(app, { destination: host === 'ios' ? 'ios-simulator' : 'macos' });
      const linked = linkedBakeDir(process.env, host, paths.target);
      write(source, 'before');
      write(resolve(linked, 'app.plan'), 'original plan');
      write(resolve(linked, 'app.module.json'), module);
      if (host === 'macos') write(builds.appleRunBinary(app), 'executable');
      const time = new Date(Date.now() + 60_000);
      const receipt = { target: paths.target, started: time.toISOString(), build: { trust: 'development',
        compat: { inputs: { platform: host }, embedded: { plan: { sha256: hash('original plan') } } },
        binary: { inputs: [{ path: source, name: 'source.swift', sha256: hash('before') }], missing: [], directories: [] } } };
      const receiptPath = resolve(paths.bundle, host === 'ios' ? 'receipt.json' : 'Contents/Resources/receipt.json');
      write(receiptPath, JSON.stringify(receipt)); utimesSync(receiptPath, time, time);
      utimesSync(contract, new Date(time.getTime() + 10_000), new Date(time.getTime() + 10_000));
      const bake = async (app, into) => {
        write(resolve(into, 'app.plan'), 'new plan');
        write(resolve(into, 'app.module.json'), module);
        write(resolve(into, 'app.hbc'), 'bytecode');
        return { code: 0 };
      };
      return callback => buildFirst(host, app, { wait() {}, say() {}, build: unexpected,
        bake: async (...args) => { const result = await bake(...args); callback?.(); return result; } });
    };
    ${script}
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('a native edit during a plan bake prevents the startup check', () => withPlanBuild(`
  const drive = setup('ios');
  await assert.rejects(drive(() => write(source, 'changed during the bake')), /ios build is stale: .*source\\.swift/);
  assert.ok(!existsSync(check));
`));

test.each(['ios', 'macos'])('%s refuses an explicit plan that displaces the checked override', host => withPlanBuild(`
  const host = ${JSON.stringify(host)};
  const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
    : (await import('./scripts/agent-stdio.mjs')).openStdio;
  const plan = resolve(dir, 'fixture.plan');
  write(plan, 'bare plan');
  utimesSync(plan, 1, 1);
  assert.equal(await setup(host)(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  for (const options of [{ plan }, { env: { EXACT_PLAN: plan } }]) {
    await assert.rejects(open({ host, app: dir, ...options }), error =>
      error.stale === true && error.message.includes('the checked sources require the baked plan override'));
  }
`));

test.each(['app.module.json', 'app.hbc', 'app.plan'])('a checked override requires %s as a file', name => withPlanBuild(`
  assert.equal(await setup('ios')(), 0);
  const checked = readFileSync(check, 'utf8'), plan = JSON.parse(checked).plan;
  const state = () => nativeBuild('ios', app, { checked });
  assert.equal(state().checked, true);
  const name = ${JSON.stringify(name)}, file = resolve(dirname(plan), name);
  rmSync(file);
  if (name === 'app.plan') mkdirSync(file);
  const missing = state();
  assert.equal(missing.checked, undefined);
  assert.ok(missing.changed.some(path => path.endsWith('app.contract')));
  assert.equal(planOverride('ios', app, missing), null);
`));

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
    import { basename } from 'node:path';
    const spawn = processes.spawn;
    let checks = 0, launches = 0, iosFiles = 0;
    mock.module('./scripts/app-shells.mjs', () => ({ ...shells, platformOf: () => null }));
    mock.module('./scripts/web-build.mjs', () => ({ ...builds, privateNativeBuild: () => ({ env: {}, seed() {}, close() {} }) }));
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
      if (basename(args[0]) === 'agent-build.mjs') {
        checks++;
        assert.ok(options.env.EXACT_BUILD_CHECK_OUT);
        writeFileSync(options.env.EXACT_BUILD_CHECK_OUT, 'checked-' + checks);
      } else if (basename(args[0]) === 'agent.mjs') {
        launches++;
        assert.equal(options.env.EXACT_BUILD_CHECKED, 'checked-' + checks);
        if (args.includes('ios') && args.includes('--test')) {
          assert.equal(options.env.EXACT_IOS_TEST_CHECKED, iosFiles++ === 0 ? '' : 'sim:warned');
          return spawn(command, ['-e', 'process.send({ checkedSimulatorAccessibility: "sim", status: "warned" })'], options);
        }
      } else assert.fail('unexpected subprocess: ' + args);
      return spawn(command, ['-e', ''], options);
    } }));
    const { run } = await import('./scripts/app-cli.mjs');
    const platforms = new Set([process.platform, 'win32']);
    for (const platform of platforms) for (const host of ['ios', 'macos']) for (const verb of ['test', 'agent']) {
      Object.defineProperty(process, 'platform', { value: platform });
      const before = [checks, launches];
      const windows = platform === 'win32';
      assert.equal(await run({ dir: process.env.EXACT_APP_DIR, name: 'launch-once', exact2: process.cwd(),
        args: [verb, host, ...(verb === 'agent' ? ['tree'] : [])] }), windows ? 2 : 0);
      assert.equal(checks - before[0], windows ? 0 : 1);
      assert.equal(launches - before[1], windows ? 0 : verb === 'test' ? 2 : 1);
    }
    assert.equal(iosFiles, platforms.size === 1 ? 0 : 2);
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('test ios checks a warm seed once, keeps a checked bake after test failure, and refuses a reusable seed for another backend', () => {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { EventEmitter } from 'node:events';
    import * as processes from 'node:child_process';
    import * as shells from './scripts/app-shells.mjs';
    import * as launch from './scripts/agent-launch.mjs';
    import * as sweep from './scripts/sweep.mjs';
    import { createHash } from 'node:crypto';
    import { appleArtifacts } from './host/apple/build.mjs';
    import { resolveApp } from './scripts/app.mjs';
    import { linkedBakeDir } from './scripts/linked-plan.mjs';
    import { mkdirSync, readFileSync, utimesSync, writeFileSync } from 'node:fs';
    import { basename, resolve } from 'node:path';
    const dir = process.env.EXACT_APP_DIR;
    process.env.CARGO_TARGET_DIR = resolve(dir, 'target');
    process.env.EXACT_UPDATE_TRUST = 'development';
    const app = resolveApp(dir);
    const hash = bytes => createHash('sha256').update(bytes).digest('hex');
    const origin = 'http://127.0.0.1:4400', other = 'http://127.0.0.1:4401';
    const source = resolve(dir, 'source.swift'), contract = resolve(dir, 'app.contract');
    const module = JSON.stringify({ appId: app.id, abi: 1, bytecodeVersion: 96 });
    writeFileSync(resolve(dir, 'app.ts'), "export const origin = '" + origin + "';\\n");
    writeFileSync(source, 'original native source');
    let checks = 0, launches = 0, builds = 0, bakes = 0, testCode = 0;
    mock.module('./scripts/app-shells.mjs', () => ({ ...shells, platformOf: () => null }));
    mock.module('./scripts/sweep.mjs', () => ({ ...sweep, startSweep() {} }));
    const receiptChanges = launch.receiptChanges;
    mock.module('./scripts/agent-launch.mjs', () => ({ ...launch,
      receiptChanges(...args) { checks++; return receiptChanges(...args); },
    }));
    const { buildFirst, nativeBuild, planOverride, refuseBuild } = await import('./scripts/agent-build.mjs');
    const build = () => {
      builds++;
      const paths = appleArtifacts(app, { destination: 'ios-simulator' });
      const receipt = resolve(paths.bundle, 'receipt.json'), time = new Date(Date.now() + 60_000);
      const linked = linkedBakeDir(process.env, 'ios', paths.target);
      mkdirSync(linked, { recursive: true });
      writeFileSync(resolve(linked, 'app.plan'), 'original plan');
      writeFileSync(resolve(linked, 'app.module.json'), module);
      mkdirSync(paths.bundle, { recursive: true });
      writeFileSync(receipt, JSON.stringify({ target: paths.target, started: time.toISOString(),
        build: { trust: 'development', backendOrigin: process.env.EXACT_BACKEND_ORIGIN ?? origin,
          compat: { inputs: { platform: 'ios' }, embedded: { plan: { sha256: hash('original plan') } } },
          binary: { inputs: [{ path: source, name: 'source.swift', sha256: hash(readFileSync(source)) }], missing: [], directories: [] } } }));
      utimesSync(receipt, time, time);
      return { code: 0, output: '' };
    };
    const bake = (app, into) => {
      bakes++;
      mkdirSync(into, { recursive: true });
      writeFileSync(resolve(into, 'app.plan'), 'updated plan');
      writeFileSync(resolve(into, 'app.module.json'), module);
      writeFileSync(resolve(into, 'app.hbc'), 'bytecode');
      return { code: 0, output: '' };
    };
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
      const child = new EventEmitter();
      Promise.resolve().then(async () => {
        switch (basename(args[0])) {
          case 'agent-build.mjs': return buildFirst('ios', app, { say() {}, wait() {}, build, bake });
          case 'agent.mjs': {
            launches++;
            const state = nativeBuild('ios', app, { checked: options.env.EXACT_BUILD_CHECKED });
            assert.equal(state.checked, true);
            if (state.plan) assert.equal(planOverride('ios', app, state)?.plan, state.plan);
            else refuseBuild('ios', state, app);
            return testCode;
          }
          default: assert.fail('unexpected subprocess: ' + args);
        }
      }).then(code => child.emit('exit', code), error => { console.error(error); child.emit('exit', 1); });
      return child;
    } }));
    const { run } = await import('./scripts/app-cli.mjs');
    const drive = async (expected = 0, ...args) => {
      const listeners = process.listeners('exit');
      const code = await run({ dir, name: app.name, exact2: process.cwd(), args: ['test', 'ios', ...args] });
      for (const fn of process.listeners('exit').filter(fn => !listeners.includes(fn))) {
        process.off('exit', fn);
        fn(code);
      }
      assert.equal(code, expected);
    };
    await drive();
    assert.equal(builds, 1);
    checks = launches = 0;
    await drive();
    assert.deepEqual({ checks, launches, builds }, { checks: 1, launches: 1, builds: 1 });

    const cache = resolve(app.target, 'native-reuse', hash(app.dir).slice(0, 16), 'ios/development/current/artifact.json');
    const before = readFileSync(cache, 'utf8');
    await drive(1, '--backend', other);
    assert.equal(readFileSync(cache, 'utf8'), before);
    assert.equal(launches, 1);
    writeFileSync(contract, 'component App\\n  text "changed"\\n');
    const later = new Date(Date.now() + 120_000);
    utimesSync(contract, later, later);
    await drive(1, '--backend', other);
    assert.equal(readFileSync(cache, 'utf8'), before);
    assert.deepEqual({ launches, builds, bakes }, { launches: 1, builds: 1, bakes: 0 });

    delete process.env.EXACT_BACKEND_ORIGIN;
    testCode = 1;
    await drive(1);
    await drive(1);
    assert.deepEqual({ launches, builds, bakes }, { launches: 3, builds: 1, bakes: 1 });
    assert.notEqual(readFileSync(cache, 'utf8'), before);

    const override = readFileSync(cache, 'utf8');
    utimesSync(contract, 1, 1);
    writeFileSync(source, 'changed native source');
    await drive(1, '--backend', other);
    assert.notEqual(readFileSync(cache, 'utf8'), override);
    testCode = 0;
    await drive(0, '--backend', other);
    assert.deepEqual({ launches, builds, bakes }, { launches: 5, builds: 2, bakes: 1 });
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
    import { existsSync, mkdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
    import { dirname, resolve } from 'node:path';
    import { appleExecutableName, resolveApp } from './scripts/app.mjs';
    import * as tools from './scripts/tools.mjs';
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
    let failingBoot, transientBoot = false, failInstall = false, replacement = 'A';
    Date.now = () => now;
    const container = udid => resolve(dir, udid + '-' + generation + '.app');
    for (const udid of ['A', 'B']) mkdirSync(container(udid));
    for (const bundle of [paths.bundle, container('B')]) {
      mkdirSync(resolve(bundle, '_CodeSignature'));
      for (const part of [appleExecutableName(app, true), 'receipt.json', '_CodeSignature/CodeResources']) {
        const file = resolve(bundle, part);
        writeFileSync(file, 'installed'); utimesSync(file, 1, 1);
      }
    }
    const result = stdout => ({ status: 0, stdout: stdout ?? '', stderr: '' });
    const simctl = args => {
      assert.equal(args[0], 'simctl');
      if (args[1] === 'list') {
        counts.list++;
        return result(JSON.stringify({ devices: { 'com.apple.CoreSimulator.SimRuntime.iOS-27-0': ['A', 'B'].map(udid => ({
          udid, name: failingBoot && udid === replacement ? 'replacement' : udid, state: states[udid], deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro',
        })) } }));
      }
      if (args[1] === failingBoot && args[2] === replacement) return { status: 1, stderr: 'device became unavailable' };
      if (args[1] === 'boot' && transientBoot) {
        transientBoot = false; return { status: 1, stderr: 'temporary boot failure' };
      }
      if (args[1] === 'boot') { counts.boot++; states[args[2]] = 'Booted'; return result(); }
      if (args[1] === 'bootstatus' || args[1] === 'shutdown') return result();
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
    const file = resolve(dir, 'recovery.test.contract');
    const authored = ['first', 'shutdown', 'third'].map(name => ({ name, steps: [{ op: 'clock', arg: 'data', line: 2 }] }));
    mock.module('./scripts/tools.mjs', () => ({ ...tools, contractTool: () => ({ cmd: 'contract', pre: [] }) }));
    mock.module('./scripts/sweep.mjs', () => ({ ...cleanup, startSweep() {} }));
    mock.module('node:child_process', () => ({ ...processes,
      spawnSync(command, args, options) {
        if (command === 'git') return realSpawnSync(command, args, options);
        if (command === 'contract') {
          assert.deepEqual(args, ['test', file, '--app', dir]);
          return result(JSON.stringify({ tests: authored, properties: [] }));
        }
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
    const { withSoftwareKeyboard, keepInstalledRelease } = devices;
    mock.module('./host/apple/devices.mjs', () => ({ ...devices,
      withSoftwareKeyboard: (dev, open) => withSoftwareKeyboard(dev, open, { records }),
      showSimulator() {}, crashWatch: () => () => [],
      keepInstalledRelease: (udid, id, bundle, executable, options) => udid === 'B'
        ? keepInstalledRelease(udid, id, bundle, executable, { ...options, installRecords: resolve(dir, 'install-records') })
        : { install: reinstall },
    }));
    mock.module('./host/apple/build.mjs', () => ({ ...builds, install() {
      if (failInstall) throw new Error('simctl install failed');
      generation++; mkdirSync(container(process.env.EXACT_SIM)); reinstall = false;
    } }));
    mock.module('./host/apple/touches.mjs', () => ({ ...touches, withAccessibilityRuntime: (udid, enabled, open) => open() }));
    mock.module('./scripts/agent-wire.mjs', () => ({ ...wire,
      simulatorConsole: () => ({ done: states[process.env.EXACT_SIM] !== 'Booted', retry: () => false,
        exited: Promise.resolve(0), process: { kill() {} } }),
      jsonLines: () => ({ next: async () => ({ ready: true }), ask: async () => ({ mutationRequests: 0 }), fail() {} }),
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
    for (let i = 0; i < 2; i++) { const carrier = await openIOS({ app: dir }); await carrier.close(); }
    assert.deepEqual(counts, { check: 1, list: 2, boot: 1, keyboard: { A: { read: 2, delete: 1, write: 1 }, B: { read: 1, delete: 0, write: 0 } }, container: { A: 1, B: 1 } });
    assert.equal(settings.B, '0');
    const lastUsed = resolve(dir, '.exact/simulator');
    assert.equal(readFileSync(lastUsed, 'utf8').trim(), 'B');
    process.env.EXACT_SIM = 'A'; reinstall = true;
    const old = container('A'), carrier = await openIOS({ app: dir }); await carrier.close();
    assert.ok(existsSync(old)); // Invalidation must work even while the old directory remains.
    assert.notEqual(assets.at(-1), old);
    assert.equal(assets.at(-1), container('A'));
    assert.equal(counts.container.A, 2);
    assert.equal(readFileSync(lastUsed, 'utf8').trim(), 'A');
    assert.equal(counts.check, 1);
    rmSync(container('A'), { recursive: true });
    generation++; mkdirSync(container('A'));
    const replaced = await openIOS({ app: dir }); await replaced.close();
    assert.equal(assets.at(-1), container('A'));
    assert.equal(counts.container.A, 3);
    // The shutdown falls within the life of B's device list. A new list must see A shut down.
    const { runTests } = await import('./scripts/agent-test.mjs');
    const recovery = await runTests({ host: 'ios', app: dir, file, atResult(test) {
      if (test.name === 'first') states.A = 'Shutdown';
    } });
    assert.deepEqual(recovery.results.map(test => [test.name, test.failures.length]), [['first', 0], ['shutdown', 1], ['third', 0]], JSON.stringify(recovery.results));
    assert.match(recovery.results[1].failures[0], /simulator launch exited/);
    assert.equal(settings.A, '1');
    const nextLaunch = await openIOS({ app: dir }); await nextLaunch.close();
    assert.equal(counts.list, 4);
    assert.equal(counts.boot, 2);
    assert.equal(counts.container.A, 4);
    assert.deepEqual(counts.keyboard.A, { read: 4, delete: 2, write: 2 });
    assert.equal(counts.check, 1);
    assert.ok(nativeBuild('ios', app).changed.some(path => path.endsWith('app.contract')), 'the next run sees the edit');

    devices.forgetSimulator('A');
    failInstall = reinstall = true;
    const completed = [];
    await assert.rejects(runTests({ host: 'ios', app: dir, file, atResult: test => completed.push(test.name) }), /simctl install failed/);
    assert.deepEqual(completed, []);
    failInstall = reinstall = false;

    authored.push({ ...authored[0], name: 'after boot' });
    const bootRecovery = await runTests({ host: 'ios', app: dir, file, atResult(test) {
      if (test.name === 'first') { states.A = 'Shutdown'; transientBoot = true; }
    } });
    assert.deepEqual(bootRecovery.results.map(test => test.failures.length), [0, 1, 1, 0], JSON.stringify(bootRecovery.results));
    assert.match(bootRecovery.results[2].failures[0], /simctl boot: temporary boot failure/);

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
    const readCount = counts.keyboard.A.read;
    const recovered = await withSoftwareKeyboard({ udid: 'A' }, () => ({ close() {} }), { records }); await recovered.close();
    assert.equal(counts.keyboard.A.read, readCount + 1);
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));

    for (const command of ['boot', 'bootstatus']) {
      const options = { cached: true, app: command };
      failingBoot = command; replacement = 'A'; states.A = 'Shutdown'; states.B = 'Booted';
      devices.forgetSimulator('A');
      assert.throws(() => devices.simulator('replacement', options), new RegExp('simctl ' + command + ': device became unavailable'));
      replacement = 'B';
      assert.equal(devices.simulator('replacement', options).udid, 'B');
    }
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

test('rotate issues the turn on the new display and confirms the scene after landing', async () => {
  const sample = { orientation: 'portrait', device: 3, inner: true, ready: true };
  const reads = [
    { ...sample, inner: false, orientation: 'landscapeLeft' },
    { ...sample, ready: false }, sample,
    { ...sample, orientation: 'landscapeLeft' }, sample, { ...sample, orientation: 'landscapeLeft' },
  ];
  let count = 0, turns = 0, lands = 0;
  const result = await rotateSimulator({
    duo: true, fold: 180,
    ask({ op }) { assert.equal(op, 'layout'); return Promise.resolve(reads[count++]); },
    issue(turn) {
      assert.equal(count, 3);
      assert.equal(turn.helper, 'portrait');
      turns++;
      throw new Error('setter timed out');
    },
    land(result) { lands++; return Promise.resolve(result); },
  }, 'landscapeLeft');
  assert.deepEqual(result, { orientation: 'landscapeLeft' });
  assert.deepEqual([count, turns, lands], [6, 1, 1]);
});

test('a lost Duo turn is reissued from the reported pose at most twice', async () => {
  const turns = [], messages = [];
  let now = 0;
  const clock = spyOn(performance, 'now').mockImplementation(() => now);
  const log = spyOn(console, 'error').mockImplementation(message => messages.push(message));
  try {
    await assert.rejects(rotateSimulator({
      duo: true, fold: 0,
      async ask() {
        if (turns.length) now += 3100;
        return { orientation: 'portrait', device: 1, ready: true, inner: false, allowed: ['landscape-secondary'] };
      },
      issue(turn) { turns.push(turn.device); },
      land() { assert.fail('the scene never turned'); },
    }, 'landscapeLeft'), /device portrait .*scene stayed portrait on cover display/);
    assert.deepEqual(turns, ['landscapeRight', 'portrait', 'landscapeRight', 'portrait', 'landscapeRight']);
    assert.equal(messages.length, 2);
    assert.match(messages[0], /reissue 1\/2/);
    assert.match(messages[1], /reissue 2\/2/);
  } finally {
    clock.mockRestore();
    log.mockRestore();
  }
});

test('a scene that turns at the deadline gets the same landing as any input', async () => {
  let issued = false, now = 0;
  const clock = spyOn(performance, 'now').mockImplementation(() => now);
  try {
    const result = await rotateSimulator({
      duo: false,
      async ask() {
        return { orientation: issued ? 'landscapeLeft' : 'portrait', device: 1 };
      },
      issue() { issued = true; now = 10000; return 'landscapeRight'; },
      async land(result) { await Bun.sleep(1050); return result; },
    }, 'landscapeLeft');
    assert.deepEqual(result, { orientation: 'landscapeLeft' });
  } finally { clock.mockRestore(); }
});

test('a partial fold can report the new hinge angle on the outgoing cover', async () => {
  const sample = { angle: 127.78, inner: false, ready: true, device: 4, orientation: 'landscapeLeft' };
  const samples = [sample, sample, sample, { ...sample, inner: true, ready: false }, { ...sample, inner: true, orientation: 'portrait' },
    { ...sample, inner: true }, { ...sample, inner: true }];
  let reads = 0;
  await rotateSimulator({
    duo: true, fold: 130, display: () => reads === 1 ? {} : { inner: true, angle: 127.8 },
    async ask() { return samples[reads++]; },
    issue(turn) { assert.equal(reads, 5); assert.equal(turn.device, 'portrait'); },
    async land(result) { return result; },
  }, 'landscapeLeft');
  assert.equal(reads, samples.length);
});

test('a fold lands once UIKit has told the app of it, moved it to the display and divided its window', async () => {
  const flat = { 'horizontal-viewport-segments': 1, 'vertical-viewport-segments': 1 }, book = { ...flat, 'horizontal-viewport-segments': 2 };
  // `samples`: what the host says each time, its own facts and then the app's segments; the first is from before the fold
  const fold = async (degrees, [before, ...samples], { display = () => assert.fail('SpringBoard was asked'), still, within } = {}) => {
    let reads = 0;
    const at = () => samples[Math.min(reads, samples.length) - 1];
    const taken = await foldTaken({ degrees, display, before, still, within, quiet: 50, pause: 1, ask: async ({ simulatorRotation }) => {
      if (simulatorRotation) reads++;
      return simulatorRotation ? at() : { env: at().env, viewport: at().viewport };
    } });
    return [taken, reads];
  };
  const wide = { w: 951, h: 669 }, narrow = { w: 466, h: 678 };
  const open = { angle: 180, inner: true, ready: true, env: flat, viewport: wide }, half = { ...open, angle: 127.78, env: book };
  const closed = { angle: 0, inner: false, ready: true, env: flat, viewport: narrow };
  assert.deepEqual(await fold(130, [open, open, { ...half, env: flat }, half]), [true, 3]); // the division comes after the hinge
  assert.deepEqual(await fold(0, [open, open, { ...open, angle: 0 }, { ...closed, ready: false }, { ...closed, viewport: wide }, closed]), [true, 5]); // and the display, and its size
  assert.deepEqual(await fold(180, [closed, closed, { ...closed, angle: 180 }, open]), [true, 3]);
  // Between two partial angles, only the new one is taken, unless the driver folded to this one; a first fold can find it there
  assert.deepEqual(await fold(90, [half, half, { ...half, angle: 88 }], { still: false }), [true, 2]);
  assert.equal((await fold(90, [half, half], { still: false, within: 100 }))[0], false);
  assert.deepEqual(await fold(130, [half, half], { still: true }), [true, 1]);
  assert.equal((await fold(130, [half, half], { display: () => ({}) }))[0], true);
  assert.equal((await fold(0, [open, { ...closed, angle: null }], { within: 100 }))[0], false);
  // From the cover, or nearly closed, SpringBoard says which display has the app
  assert.deepEqual(await fold(130, [closed, { ...closed, angle: 127.78 }, half], { display: () => ({ inner: true, fresh: true }) }), [true, 2]);
  assert.deepEqual(await fold(20, [open, { ...half, angle: 18 }, { ...closed, angle: 18 }], { display: () => ({ inner: false, fresh: true }) }), [true, 2]);
  // A first fold to another partial angle waits for SpringBoard's
  assert.equal((await fold(90, [half, half], { display: () => ({ angle: 88 }), within: 100 }))[0], false);
  // Its word from before this fold is not of it: the display the app is on is taken once it has been quiet
  const [taken, reads] = await fold(130, [closed, { ...closed, angle: 127.78 }, half], { display: () => ({ inner: false, fresh: false }) });
  assert.ok(taken && reads > 2);
  assert.equal((await fold(20, [closed, { ...closed, angle: 18 }], { display: () => ({ inner: false, fresh: false }) }))[0], true);
  assert.equal((await fold(130, [open, open], { within: 100 }))[0], false);
});

test('losing the verification read cannot undo a turn the scene reported', async () => {
  const replies = [{ orientation: 'portrait' }, { orientation: 'landscapeRight' }, { error: 'connection closed' }];
  const result = await rotateSimulator({
    duo: false, async ask() { return replies.shift(); }, issue() {},
    async land(result) { return { ...result, epoch: 3 }; },
  }, 'landscapeRight');
  assert.deepEqual(result, { orientation: 'landscapeRight', epoch: 3 });
});
