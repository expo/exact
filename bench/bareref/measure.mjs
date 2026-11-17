// Compares Caltrain's agent push and the matching UIKit-only push on one iOS simulator.
// EXACT_SIM=<dedicated UUID> bun bench/bareref/measure.mjs <prefix> [15] [3] --build
// --build temporarily instruments Caltrain, builds both apps, and restores its source on exit.
// Traces, timing JSON, view dumps, load and PID records go into target/bareref/.
import { open } from '../../scripts/agent-session.mjs';
import { spawn, spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync, mkdtempSync, rmSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { connect } from 'node:net';
import { exactPush } from './push.mjs';

const udid = process.env.EXACT_SIM;
if (!udid) throw Error('EXACT_SIM must name a dedicated iOS 27 simulator');
process.env.EXACT_SHOW_SIMULATOR = '0';
const prefix = process.argv[2] ?? 'paired';
const count = Number(process.argv[3] ?? 15);
const rounds = Number(process.argv[4] ?? 3);
const out = resolve('target/bareref');
mkdirSync(out, { recursive: true });
const originals = new Map();
const probePath = 'host/apple/Sources/ExactKit/PushProbe.swift';
let installedProbe = false;
function restore() {
  for (const [path, source] of originals) writeFileSync(path, source);
  if (installedProbe) rmSync(probePath, { force: true });
  originals.clear();
  installedProbe = false;
}
process.on('exit', restore);
function replace(path, anchor, replacement) {
  const source = readFileSync(path, 'utf8');
  if (source.split(anchor).length !== 2) throw Error('instrumentation anchor changed: ' + path);
  originals.set(path, source);
  writeFileSync(path, source.replace(anchor, replacement));
}
if (process.argv.includes('--build')) {
  if (existsSync(probePath)) throw Error('temporary PushProbe.swift already exists; preserve or remove it first');
  replace('host/apple/Sources/ExactKit/IOS/AgentIOS.swift',
    '    func tap(_ req: [String: Any]) -> [String: Any] {',
    `    func tap(_ req: [String: Any]) -> [String: Any] {
        if let node = view(req), node.props["testId"] == "change-station", req["hover"] == nil {
            PushProbe.shared.begin()
        }`);
  replace('host/apple/Sources/ExactKit/IOS/PresenterIOS.swift',
    '    func apply(_ batch: Batch) {',
    `    func apply(_ batch: Batch) {
        let probe = PushProbe.shared
        let measuring = probe.active && !ApplyProfile.on
        let started = CACurrentMediaTime()
        if measuring { ApplyProfile.begin() }
        defer {
            if measuring {
                let elapsed = (CACurrentMediaTime() - started) * 1000
                let parts = ApplyProfile.end()
                probe.applies.append(["wallMs": elapsed, "ops": batch.ops.count, "parts": parts])
            }
        }`);
  writeFileSync(probePath, readFileSync('bench/bareref/PushProbe.swift'));
  installedProbe = true;
  writeFileSync(out + '/options.json', JSON.stringify({
    Hangs: { detectPriorityInversions: false, hangsThreshold: 250 },
    'Points of Interest': { excludeOSLogs: false },
    'Time Profiler': { contextSwitchSampling: false, highFrequencySampling: true,
      recordKernelStacks: false, recordWaitingThreads: false },
  }));
  for (const [bin, args] of [
    ['bun', ['host/apple/build.mjs', '--ios', '--sim', udid]],
    ['sh', ['bench/bareref/build.sh']],
    ['xcrun', ['simctl', 'install', udid, out + '/BareUIKit.app']],
  ]) {
    const result = spawnSync(bin, args, { stdio: 'inherit' });
    if (result.status !== 0) throw Error('build failed: ' + bin + ' ' + args.join(' '));
  }
}
function command(args) {
  const p = spawnSync('xcrun', args, { encoding: 'utf8' });
  if (p.status !== 0) throw Error(p.stderr || p.stdout);
  return p.stdout.trim();
}
const container = id => command(['simctl', 'get_app_container', udid, id, 'data']);
const records = path => existsSync(path) ? JSON.parse(readFileSync(path)) : [];
async function committed(path, serial) {
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    const rows = records(path);
    if (rows.length >= serial) return rows.at(-1);
    await Bun.sleep(5);
  }
  throw Error(`first commit not reported: ${path}, serial ${serial}`);
}
const session = await open({ host: 'ios', app: 'caltrain', storage: 'bareref-' + prefix });
let recorder, bareSocket, barePID;
const socketDir = mkdtempSync(tmpdir() + '/bareref-');
async function bareCommand(command) {
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(Error('bare command timed out: ' + command)), 10000);
    bareSocket.once('data', data => {
      clearTimeout(timeout);
      if (data.toString() !== 'ok\n') reject(Error(data.toString())); else resolve();
    });
    bareSocket.write(command + '\n');
  });
}
try {
  await session.clock('settle');
  const exactPID = command(['simctl', 'spawn', udid, 'launchctl', 'list']).split('\n')
    .find(line => line.includes('com.exact.caltrain')).trim().split(/\s+/)[0];
  process.env.SIMCTL_CHILD_BAREREF_SOCKET = socketDir + '/bare.sock';
  const bareLaunch = command(['simctl', 'launch', udid, 'dev.exact.bareref']);
  delete process.env.SIMCTL_CHILD_BAREREF_SOCKET;
  barePID = bareLaunch.split(':').at(-1).trim();
  const deadline = Date.now() + 10000;
  while (!existsSync(socketDir + '/bare.sock')) {
    if (Date.now() > deadline) throw Error('bare app did not open its socket');
    await Bun.sleep(10);
  }
  bareSocket = connect(socketDir + '/bare.sock');
  await new Promise((resolve, reject) => { bareSocket.once('connect', resolve); bareSocket.once('error', reject); });
  const apps = {
    exact: { id: 'com.exact.caltrain', pid: exactPID },
    bare: { id: 'dev.exact.bareref', pid: barePID },
  };
  for (const app of Object.values(apps)) {
    app.path = container(app.id) + '/Documents/bareref.json';
    rmSync(app.path, { force: true });
  }
  writeFileSync(out + '/' + prefix + '-pids.json', JSON.stringify(apps));
  async function foreground() {
    const deadline = Date.now() + 10000;
    while (true) {
      try { await session.clock('settle'); return; }
      catch (error) {
        if (!error.message.includes('left the foreground') || Date.now() >= deadline) throw error;
        await Bun.sleep(10);
      }
    }
  }
  const meta = [];
  for (let round = 1; round <= rounds; round++) {
    for (const name of round % 2 ? ['exact', 'bare'] : ['bare', 'exact']) {
      const app = apps[name], stem = `${out}/${prefix}-${name}-${round}`;
      command(['simctl', 'launch', udid, app.id]);
      if (name === 'exact') await foreground();
      const offset = records(app.path).length;
      const load = spawnSync('uptime', [], { encoding: 'utf8' }).stdout.trim();
      writeFileSync(stem + '-load.txt', load + '\n');
      let output = '';
      recorder = spawn('xcrun', ['xctrace', 'record', '--template', 'Time Profiler', '--device', udid,
        '--attach', app.pid, '--recording-options', out + '/options.json', '--output', stem + '.trace',
        '--time-limit', '45s', '--no-prompt']);
      const ended = new Promise(resolve => recorder.on('exit', resolve));
      await new Promise((resolve, reject) => {
        const accept = chunk => {
          output += chunk.toString();
          writeFileSync(stem + '-live.log', output);
          if (output.includes('Ctrl-C to stop the recording')) resolve();
        };
        recorder.stdout.on('data', accept);
        recorder.stderr.on('data', accept);
        recorder.on('exit', code => reject(Error(`recorder ended ${code}: ${output}`)));
      });
      console.log('recording', name, round, load);
      for (let i = 0; i < count; i++) {
        if (name === 'exact') {
          await exactPush(session, () => committed(app.path, offset + i + 1));
          await session.back();
          await session.clock('settle');
        } else {
          await bareCommand('push');
          await committed(app.path, offset + i + 1);
          await bareCommand('back');
        }
      }
      recorder.kill('SIGINT');
      let timeout;
      const code = await Promise.race([ended, new Promise((_, reject) => {
        timeout = setTimeout(() => reject(Error('recorder did not save within 60 seconds: ' + stem)), 60000);
      })]).finally(() => clearTimeout(timeout));
      recorder = null;
      if (code !== 0) throw Error('recorder failed: ' + code + '\n' + output);
      writeFileSync(stem + '-trace.log', output);
      const rows = records(app.path).slice(offset);
      if (rows.length !== count) throw Error(`expected ${count} pushes, got ${rows.length}`);
      writeFileSync(stem + '.json', JSON.stringify(rows, null, 2));
      meta.push({ name, round, load, pid: app.pid, count: rows.length });
      console.log('completed', name, round, rows.length);
    }
  }
  writeFileSync(out + '/' + prefix + '-meta.json', JSON.stringify(meta, null, 2));
  command(['simctl', 'launch', udid, apps.exact.id]);
  await foreground();
  writeFileSync(out + '/' + prefix + '-final-tree.json', JSON.stringify(await session.tree(), null, 2));
  for (const [name, app] of Object.entries(apps)) {
    for (const file of ['bareref-views.json', 'bareref-home-views.json']) {
      const path = container(app.id) + '/Documents/' + file;
      if (existsSync(path)) writeFileSync(`${out}/${prefix}-${name}-${file}`, readFileSync(path));
    }
  }
  for (const item of meta) {
    const stem = `${out}/${prefix}-${item.name}-${item.round}`;
    for (const [schema, suffix] of [['time-profile', '.xml'], ['os-signpost', '-signposts.xml']]) {
      command(['xctrace', 'export', '--input', stem + '.trace', '--xpath',
        `/trace-toc/run[@number="1"]/data/table[@schema="${schema}"]`, '--output', stem + suffix]);
    }
  }
  if (rounds === 3 && count === 15) {
    const result = spawnSync('python3', ['bench/bareref/analyze.py', prefix], { stdio: 'inherit' });
    if (result.status !== 0) throw Error('sample analysis failed');
  }
} finally {
  if (recorder) recorder.kill('SIGTERM');
  bareSocket?.end();
  if (barePID) { try { process.kill(Number(barePID)); } catch {} }
  rmSync(socketDir, { recursive: true, force: true });
  await session.close();
  restore();
}
