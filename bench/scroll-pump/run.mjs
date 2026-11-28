import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, realpathSync, renameSync, rmSync, watch, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';

const app = import.meta.dir;
const baselineCommit = 'a30fe74e490e9f1e59cffe26b98776b87b526d7e';
const deviceType = 'com.apple.CoreSimulator.SimDeviceType.iPhone-18-Pro';
const flings = 4;
const argv = process.argv.slice(2);
if (argv.includes('--help')) {
  console.log('Usage: bun bench/scroll-pump/run.mjs [--reuse-simulator] [baseline.app candidate.app]\nWithout bundles, build the known-bad host and the current Exact checkout.\nRun with --stop-simulator to remove the prepared simulator.');
  process.exit(0);
}
const reuse = argv.includes('--reuse-simulator');
const stop = argv.includes('--stop-simulator');
const args = argv.filter(arg => arg !== '--reuse-simulator' && arg !== '--stop-simulator');
assert(!stop || (!reuse && args.length === 0), '--stop-simulator takes no other arguments');
assert(args.length === 0 || args.length === 2, 'Supply both simulator bundles or run without arguments');
assert(process.platform === 'darwin', 'The scroll test needs macOS and Xcode');

const exact = realpathSync(resolve(process.env.EXACT ?? join(app, '../..')));
const work = join(app, '.exact', 'scroll-check');
const runEnv = { ...process.env, CI: '1', GIT_TERMINAL_PROMPT: '0', EXACT_FEEDBACK: '0' };
mkdirSync(work, { recursive: true });
const results = mkdtempSync(join(work, 'run-'));
console.log(`Scroll test artifacts: ${results}`);
const started = performance.now();
const timings = {};
const elapsed = (name, from) => { timings[name] = Math.round(performance.now() - from); };
const { takeLock } = await import(join(exact, 'host/web/build-lock.mjs'));
const { openTouches } = await import(join(exact, 'host/apple/touches.mjs'));

function command(bin, argv, { cwd = app, env = runEnv, timeout = 30_000, allowFailure = false } = {}) {
  const proc = spawnSync(bin, argv, { cwd, env, timeout, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (!allowFailure && proc.status !== 0) {
    const reason = proc.error?.message ?? (proc.stderr || proc.stdout || proc.signal || `exit ${proc.status}`).trim().slice(-2000);
    throw new Error(`${bin} ${argv[0]} failed: ${reason}`);
  }
  return (proc.stdout ?? '').trim();
}

const simctl = (argv, options) => command('xcrun', ['simctl', ...argv], options);
const jsonFile = path => JSON.parse(readFileSync(path, 'utf8'));
const save = (name, value) => writeFileSync(join(results, name), JSON.stringify(value, null, 2) + '\n');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const sourceFiles = ['app.contract', 'app.ts', 'app.json'];
const fixture = (directory = app) => hash(Buffer.concat(sourceFiles.map(name => readFileSync(join(directory, name)))));
const id = jsonFile(join(app, 'app.json')).id;
const reusableName = `Exact Scroll Probe ${hash(app).slice(0, 10)}`;
const reusableState = join(work, 'simulator.json');

function stageSources() {
  // An ancestor node_modules would mix the current checkout's packages into the historical build.
  const stage = join(tmpdir(), `exact-scroll-pump-${hash(app).slice(0, 16)}`, 'scroll-pump');
  mkdirSync(stage, { recursive: true, mode: 0o700 });
  for (const name of sourceFiles) {
    const bytes = readFileSync(join(app, name)), path = join(stage, name);
    if (!existsSync(path) || !bytes.equals(readFileSync(path))) writeFileSync(path, bytes);
  }
  return stage;
}

function reusableDevice() {
  if (!existsSync(reusableState)) return;
  const { runtime, udid, runnerPid } = jsonFile(reusableState);
  const { devices } = JSON.parse(simctl(['list', 'devices', '-j']));
  const device = devices[runtime]?.find(entry => entry.udid === udid);
  if (!device) return;
  assert(device.name === reusableName && device.deviceTypeIdentifier === deviceType, 'Prepared simulator identity changed');
  return { runtime, udid, runnerPid };
}

function deleteDevice(udid) {
  simctl(['shutdown', udid], { allowFailure: true });
  simctl(['delete', udid]);
}

async function deleteReusableDevice({ udid, runnerPid }) {
  // The SDK parks xcodebuild between runs. Retire only the PID recorded for this simulator.
  const running = () => Number.isInteger(runnerPid) && runnerPid > 0 &&
    command('/bin/ps', ['-p', String(runnerPid), '-o', 'command='], { allowFailure: true })
      .includes(`exact-touches-${udid}.xctestrun`);
  for (const signal of ['SIGTERM', 'SIGKILL']) {
    if (!running()) break;
    try { process.kill(runnerPid, signal); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
    const until = Date.now() + 3000;
    while (Date.now() < until && running()) await sleep(100);
  }
  assert(!running(), 'Prepared simulator’s XCTest runner did not stop');
  deleteDevice(udid);
}

function logged(bin, argv, { cwd = app, env = runEnv, timeout = 30_000, log }) {
  return new Promise((resolve, reject) => {
    const fd = openSync(log, 'w');
    let child;
    try { child = spawn(bin, argv, { cwd, env, timeout, stdio: ['ignore', fd, fd] }); }
    finally { closeSync(fd); }
    let error;
    child.once('error', failure => { error = failure; });
    child.once('close', (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${bin} ${argv[0]} failed: ${error?.message ?? signal ?? `exit ${code}`} (see ${log})`));
    });
  });
}

async function build(checkout, source, label, destination, inputs) {
  const phase = performance.now();
  console.log(`Building ${label}`);
  if (!existsSync(join(checkout, 'node_modules', '.bin', 'rolldown'))) {
    await logged(process.execPath, ['install', '--frozen-lockfile'], {
      cwd: checkout, timeout: 120_000, log: join(results, `${label}-install.log`),
    });
  }
  const revision = () => command('git', ['rev-parse', 'HEAD'], { cwd: checkout });
  const status = () => command('git', ['status', '--porcelain'], { cwd: checkout });
  const commit = revision(), changes = status();
  const log = join(results, `${label}-build.log`);
  await logged(process.execPath, ['host/apple/build.mjs', '--ios', '--no-install', '--extensions', 'none', '--json'], {
    cwd: checkout, env: { ...runEnv, EXACT: checkout, EXACT2: checkout, EXACT_APP_DIR: source, EXACT_UPDATE_TRUST: 'development' },
    timeout: 20 * 60_000, log,
  });
  const lines = readFileSync(log, 'utf8').split('\n');
  const output = lines.findLast(line => line.startsWith('{"path":'));
  assert(output, `Missing bundle from ${label} build. See ${log}`);
  const { path } = JSON.parse(output);
  assert(revision() === commit && status() === changes, `${label} checkout changed during the build`);
  assert(fixture() === inputs && fixture(source) === inputs, `${label} fixture sources changed during the build`);
  const dir = mkdtempSync(join(work, 'build-'));
  const bundle = join(dir, 'Scroll Pump Probe.app');
  command('ditto', [path, bundle]);
  const plan = jsonFile(join(bundle, 'receipt.json')).build.linkedPlan;
  const identity = {
    checkout,
    commit,
    dirty: Boolean(changes),
    fixture: inputs,
    plan: hash(readFileSync(join(plan, 'app.plan'))),
  };
  writeFileSync(join(dir, 'identity.json'), JSON.stringify(identity, null, 2) + '\n');
  renameSync(dir, destination);
  elapsed(`${label}BuildMs`, phase);
  return { bundle: join(destination, 'Scroll Pump Probe.app'), ...identity };
}

async function calibration(source, inputs) {
  const sdk = command('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-build-version']);
  const cached = join(work, `baseline-${hash(JSON.stringify([baselineCommit, inputs, sdk, process.arch])).slice(0, 16)}`);
  if (existsSync(cached)) {
    const identity = jsonFile(join(cached, 'identity.json'));
    assert.equal(identity.fixture, inputs, 'Cached calibration fixture does not match');
    return { bundle: join(cached, 'Scroll Pump Probe.app'), ...identity };
  }
  const checkout = join(work, 'known-bad');
  if (!existsSync(checkout)) {
    await logged('git', ['worktree', 'add', '--detach', checkout, baselineCommit], {
      cwd: exact, timeout: 120_000, log: join(results, 'baseline-checkout.log'),
    });
  }
  assert.equal(command('git', ['rev-parse', 'HEAD'], { cwd: checkout }), baselineCommit, 'The calibration checkout moved');
  assert.equal(command('git', ['status', '--porcelain'], { cwd: checkout }), '', 'The calibration checkout has local edits');
  return build(checkout, source, 'baseline', cached, inputs);
}

const nearby = (a, b, tolerance = 1) => Math.abs(a - b) <= tolerance;
const center = ([x, y, width, height]) => [x + width / 2, y + height / 2];
const inside = ([x, y, w, h], [a, b, aw, bh]) => a >= x && b >= y && a + aw <= x + w && b + bh <= y + h;
const element = (tree, name) => tree.elements?.find(node => node.identifier === name);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(read, label, timeout = 8000) {
  const end = Date.now() + timeout;
  do {
    const value = await read();
    if (value) return value;
    if (Date.now() < end) await sleep(80);
  } while (Date.now() < end);
  throw new Error(`Timed out waiting for ${label}`);
}

function waitForRecord(path, index, label) {
  return new Promise((resolve, reject) => {
    const watcher = watch(dirname(path), { persistent: false }, (_, name) => {
      if (!name || name === basename(path)) read(false);
    });
    const timeout = setTimeout(() => read(true), 8000);
    function finish(error, row) {
      clearTimeout(timeout);
      watcher.close();
      if (error) reject(error);
      else resolve(row);
    }
    function read(expired) {
      try {
        const lines = readFileSync(path, 'utf8').split('\n');
        if (lines.length > index + 1) return finish(null, JSON.parse(lines[index]));
      } catch (error) {
        if (error.code !== 'ENOENT') return finish(error);
      }
      if (expired) finish(new Error(`Timed out waiting for ${label} to settle`));
    }
    watcher.once('error', finish);
    read(false);
  });
}

function validate(row, strip, point, to) {
  assert(!row.cancelled && row.idle, 'Touch was cancelled or scroller did not settle');
  assert(row.frame.every((value, index) => nearby(value, strip[index], 2)), 'Touch reached the wrong scroller');
  assert([...row.downPoint, ...row.releasePoint].every((value, i) => nearby(value, [...point, ...to][i], 2)), 'UIKit received different touch coordinates');
  const [low, high] = row.bounds;
  assert(nearby(high - low, 128) && row.restBounds.every((value, i) => nearby(value, row.bounds[i])), 'Horizontal bounds changed');
  assert.equal(row.samples.length, row.written, 'Lost offset samples');
  assert.equal(row.starts.length, row.startsWritten, 'Lost display-link samples');
  const direction = to[0] < point[0] ? 1 : -1;
  const [start, edge] = direction === 1 ? [low, high] : [high, low];
  assert(nearby(row.initial, start, .6), 'Fling started at the wrong edge');
  assert(direction * (row.atRelease - start) >= 40 && direction * (edge - row.atRelease) > .7, 'Finger did not lift inside the scroll range');
  const after = row.samples.filter(([time]) => time > row.release);
  assert(after.length >= 12, 'Scroll did not coast after lift');
  const beyond = after.filter(([, offset]) => direction * (offset - edge) > .6);
  assert(beyond.some(([, offset, decelerating]) => decelerating && direction * (offset - edge) > 1), 'No inertial overshoot');
  assert(nearby(row.rest, edge, .6) && nearby(after.at(-1)[1], edge, .6), 'Rebound settled at the wrong edge');
  const last = after.findLastIndex((sample, index) => index && !nearby(sample[1], after[index - 1][1], .01));
  assert(last > 0 && row.restTime - after[last][0] > .10, 'No stable interval after rebound');
  return {
    direction,
    motionMs: Math.round((after[last][0] - row.release) * 1000),
    overshoot: Math.max(...beyond.map(([, offset]) => direction * (offset - edge))),
    startsAfterLift: row.starts.filter(time => time >= row.release).length,
    startsDuringGesture: row.starts.length,
    activeAtDown: row.activeAtDown,
    activeAtRest: row.activeAtRest,
  };
}

class UnqualifiedGesture extends Error {}

function identifyBundle(bundle, label) {
  const bytes = readFileSync(join(bundle, 'receipt.json'));
  const receipt = JSON.parse(bytes);
  assert.equal(receipt.platform, 'ios-simulator', `${label} bundle must target the simulator`);
  assert.equal(receipt.app.id, id, `${label} bundle identifier`);
  assert(receipt.app.origin == null && receipt.build.backendOrigin == null, 'Fixture must launch with embedded content');
  const executable = command('/usr/bin/plutil', ['-extract', 'CFBundleExecutable', 'raw', '-o', '-', join(bundle, 'Info.plist')]);
  return { bundle, receiptSHA256: hash(bytes), executableSHA256: hash(readFileSync(join(bundle, executable))) };
}

async function exercise(runner, device, label, bundle, observer, attempt) {
  const phase = performance.now();
  const record = join(results, `${label}${attempt === 1 ? '' : '-retry'}.jsonl`);
  try {
    simctl(['launch', device, id], { env: {
      ...runEnv, SIMCTL_CHILD_EXACT_SAMPLE_FRAMES: '0',
      SIMCTL_CHILD_DYLD_INSERT_LIBRARIES: observer, SIMCTL_CHILD_SCROLL_PROBE_RECORD: record,
      SIMCTL_CHILD_SCROLL_PROBE_REVEAL_AFTER: String(flings),
    } });
    const ask = async request => {
      const reply = await runner.ask(request, 12_000);
      if (reply.error) throw new Error(`${label} ${request.op}: ${reply.error}`);
      return reply;
    };
    const tree = () => ask({ op: 'tree', max: 100 });
    const start = await waitFor(async () => {
      if ((await ask({ op: 'foreground' })).state !== 'runningForeground') return;
      const state = await tree();
      if (element(state, 'arm-probe') && element(state, 'fling-strip') && element(state, 'outer-scrollport')) return state;
    }, `${label} fixture to appear`);
    assert(!element(start, 'clipped-title'), 'The title must start outside the outer scrollport');
    const arm = center(element(start, 'arm-probe').frame);
    await ask({ op: 'tap', point: arm });
    elapsed(`${label}LaunchAndArmMs`, phase);
    const strip = element(start, 'fling-strip').frame;
    const [x, y] = strip;
    assert(nearby(strip[2], 288) && nearby(strip[3], 80), 'The visible strip must be 288 × 80 points');
    const left = [x + 48, y + 32], right = [x + 173, y + 32];
    const drag = (point, to, velocity) => ask({
      op: 'drag', point, to, press: .025, hold: 0, velocity,
    });
    const motion = performance.now();
    const verified = [];
    for (let index = 0; index < flings; index++) {
      const [point, to] = index % 2 ? [left, right] : [right, left];
      await drag(point, to, 1450);
      const row = await waitForRecord(record, index, `${label} fling ${index + 1}`);
      try {
        verified.push(validate(row, strip, point, to));
      } catch (error) {
        if (error.code !== 'ERR_ASSERTION') throw error;
        throw new UnqualifiedGesture(`${label} fling ${index + 1}: ${error.message}`);
      }
    }
    elapsed(`${label}MotionMs`, motion);
    const reveal = performance.now();
    const port = element(start, 'outer-scrollport').frame;
    const title = await waitFor(async () => {
      const node = element(await tree(), 'clipped-title');
      if (node && inside(port, node.frame)) return node;
    }, `${label} title in the outer scrollport`);
    assert.equal(title.label, 'The clipped title B', `${label}: title did not update when armed`);
    elapsed(`${label}RevealMs`, reveal);
    const records = readFileSync(record, 'utf8').trim().split('\n');
    assert.equal(records.length, flings, `${label}: unexpected fling records`);
    save(`${label}.json`, { bundle, attempt, strip, port, verified, revealed: title });
    console.log(`${label}: ${verified.length} flings and title reveal verified. App link starts after lift ${verified.map(row => row.startsAfterLift).join(', ')}`);
    return verified;
  } finally {
    simctl(['terminate', device, id], { allowFailure: true });
    elapsed(`${label}${attempt === 1 ? 'Drive' : 'Retry'}Ms`, phase);
  }
}

const unlock = takeLock(join(work, 'comparison.flock'), { waitMs: 0 });
if (stop) {
  try {
    const existing = reusableDevice();
    if (existing) await deleteReusableDevice(existing);
    rmSync(reusableState, { force: true });
  } finally { unlock(); }
  console.log('Prepared simulator removed');
  process.exit(0);
}

let device, runner, pending, keptDevice, keep = false, installed = false;
async function prepareSimulator() {
  const bootStart = performance.now();
  const { runtimes } = JSON.parse(simctl(['list', 'runtimes', '-j']));
  const runtime = runtimes.filter(row => row.isAvailable && row.supportedDeviceTypes?.some(type => type.identifier === deviceType))
    .sort((a, b) => b.version.localeCompare(a.version, undefined, { numeric: true }))[0];
  assert(runtime, 'Install an iOS Simulator runtime that supports iPhone 18 Pro');
  const existing = reuse ? reusableDevice() : undefined;
  if (existing && existing.runtime !== runtime.identifier) await deleteReusableDevice(existing);
  device = existing?.runtime === runtime.identifier ? existing.udid : simctl(['create',
    reuse ? reusableName : `Exact Scroll Probe ${process.pid}`, deviceType, runtime.identifier]);
  if (reuse) {
    keptDevice = { runtime: runtime.identifier, udid: device, runnerPid: existing?.udid === device ? existing.runnerPid : undefined };
    writeFileSync(reusableState, JSON.stringify(keptDevice));
  }
  console.log(`${reuse ? 'Preparing reusable' : 'Booting disposable'} iPhone 18 Pro on iOS ${runtime.version}`);
  await logged('xcrun', ['simctl', 'bootstatus', device, '-b'], { timeout: 120_000, log: join(results, 'simulator-boot.log') });
  keep = reuse;
  elapsed('bootMs', bootStart);
}

async function connectRunner(candidateReady) {
  const runnerStart = performance.now();
  try {
    runner = await openTouches({ udid: device, appId: id });
  } catch (error) {
    // Let a concurrent build finish before retrying runner setup on a busy simulator.
    console.warn(`Retrying XCTest setup on ${device}: ${error.message.split('\n')[0]}`);
    await candidateReady;
    runner = await openTouches({ udid: device, appId: id });
  } finally {
    elapsed('runnerMs', runnerStart);
  }
  if (keep) writeFileSync(reusableState, JSON.stringify({ ...keptDevice, runnerPid: runner.pid }));
  timings.keptRunner = runner.kept;
}

async function runArm(label, { bundle }, observer, candidateReady) {
  for (const attempt of [1, 2]) {
    const start = performance.now();
    if (installed || keep) simctl(['uninstall', device, id], { allowFailure: !installed });
    installed = false;
    simctl(['install', device, bundle], { timeout: 60_000 });
    installed = true;
    elapsed(`${label}${attempt === 1 ? 'Install' : 'Reinstall'}Ms`, start);
    if (!runner) await connectRunner(candidateReady);
    try {
      return await exercise(runner, device, label, bundle, observer, attempt);
    } catch (error) {
      if (!(error instanceof UnqualifiedGesture) || attempt === 2) throw error;
      // Replay the arm from launch so record indexes and the reveal still agree.
      console.warn(`${error.message}. Retrying ${label} after the candidate build finishes.`);
      save(`${label}-retry.json`, { reason: error.message });
      await candidateReady;
    }
  }
}

function identify(subject, label) {
  return { ...subject, ...identifyBundle(subject.bundle, label) };
}

function reportFailure(error, phase) {
  const kind = error instanceof UnqualifiedGesture ? 'unqualified-gesture' : 'error';
  const secondary = Boolean(process.exitCode);
  if (!secondary) process.exitCode = kind === 'unqualified-gesture' ? 2 : 1;
  console.error(`${phase}: ${error.message}`);
  save(secondary ? 'cleanup-failure.json' : 'failure.json', { phase, kind, message: error.message });
}

try {
  const prebuilt = args.map((bundle, index) => identifyBundle(resolve(bundle), index ? 'candidate' : 'baseline'));
  const source = prebuilt.length ? null : stageSources();
  const inputs = source ? fixture(source) : null;
  const controlReady = prebuilt.length ? Promise.resolve(prebuilt[0]) : calibration(source, inputs).then(control => identify(control, 'baseline'));
  // Both builds write to the fixture's output directory. Stage the control before rebuilding there.
  const candidateReady = prebuilt.length ? Promise.resolve(prebuilt[1]) : controlReady.then(async () =>
    identify(await build(exact, source, 'candidate', join(results, 'candidate'), inputs), 'candidate'));
  const prepared = prepareSimulator();
  const observer = join(results, 'scroll-observer.dylib');
  const arch = process.arch === 'arm64' ? 'arm64' : 'x86_64';
  const observerReady = logged('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-target', `${arch}-apple-ios18.0-simulator`,
    '-fobjc-arc', '-dynamiclib', '-framework', 'UIKit', '-framework', 'QuartzCore', '-framework', 'Foundation',
    '-Wall', '-Werror', join(app, 'observer.m'), '-o', observer], { log: join(results, 'observer-build.log') });
  pending = Promise.allSettled([controlReady, candidateReady, prepared, observerReady]);

  const [control] = await Promise.all([controlReady, prepared, observerReady]);
  const baseline = await runArm('baseline', control, observer, candidateReady);
  assert(baseline.every(row => row.startsAfterLift > 0), 'The known-bad control stayed quiet. The test could not detect the regression.');

  const candidateWait = performance.now();
  const subject = await candidateReady;
  elapsed('candidateWaitMs', candidateWait);
  const sameExecutable = control.executableSHA256 === subject.executableSHA256;
  if (sameExecutable) console.log('The baseline and candidate executables are identical');
  save('identity.json', { baseline: control, candidate: subject, sameExecutable });
  const candidate = await runArm('candidate', subject, observer, candidateReady);
  assert(candidate.every(row => row.startsDuringGesture === 0 && row.activeAtDown === 0 && row.activeAtRest === 0),
    'The candidate woke or held an app-owned display link while scrolling clipped text');
  save('passed.json', { baseline, candidate });
  console.log('PASS: the known-bad host woke a display link in every fling and the candidate stayed quiet');
} catch (error) {
  reportFailure(error, 'probe');
} finally {
  // Simulator setup also owns resources when a concurrent preparation step fails.
  await pending;
  const cleanupStart = performance.now();
  try {
    try {
      if (keep) await runner?.close();
      else {
        const error = await runner?.end();
        if (error) throw error;
      }
    } finally {
      if (keep) {
        if (installed) simctl(['uninstall', device, id]);
      } else if (device) {
        if (keptDevice) await deleteReusableDevice(keptDevice);
        else deleteDevice(device);
        if (reuse) rmSync(reusableState, { force: true });
      }
    }
  } catch (error) {
    reportFailure(error, 'cleanup');
  } finally {
    unlock();
    elapsed('cleanupMs', cleanupStart);
    elapsed('totalMs', started);
    save('timings.json', timings);
  }
}
