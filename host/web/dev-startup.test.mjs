import { afterEach, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { request } from 'node:http';
import { createServer } from 'node:net';
import { resolve } from 'node:path';
import { spawnOwned, stopAll } from '../../scripts/test-children.mjs';
import { claimDevLoop, stopBuildTree } from './dev-startup.mjs';
import { applyStaticChange, applyStaticTreeChange } from './serve.mjs';
import { createHash } from 'node:crypto';

const root = resolve(import.meta.dir, '../..');
afterEach(() => stopAll());

test('assets read by the compiler rebuild before a development generation is published', () => {
  const dir = mkdtempSync(resolve(root, 'dev-svg-')), dist = resolve(dir, 'dist');
  const source = readFileSync(resolve(root, 'host/web/dev.mjs'), 'utf8');
  const body = source.slice(source.indexOf('function pushAssets()'), source.indexOf('// Classification follows'));
  const run = new Function('env', `
    const {app, dist, edits, readinessContracts, resolve, applyStaticChange, applyStaticTreeChange, createHash} = env;
    let assetChanges = edits, assetsNeedRebuild = false, timer, seq = 0;
    const changed = new Set(), clients = new Set(), console = {log() {}};
    const clearTimeout = () => {}, setTimeout = callback => { env.pending.push(callback); };
    const rebuild = () => { env.rebuilt++; };
    const captureGeneration = () => { env.captured++; }, announcement = () => ({});
    const push = message => env.messages.push(message);
    ${body}
    pushAssets();
    return {assetsNeedRebuild, changed: [...changed]};
  `);
  const svg = resolve(dir, 'assets/picture.svg');
  const push = change => {
    const env = {app: {dir}, dist, edits: new Map([[change.name, change]]), readinessContracts: new Set([svg]),
      resolve, applyStaticChange, applyStaticTreeChange, createHash, pending: [], messages: [], rebuilt: 0, captured: 0};
    const result = run(env);
    for (const callback of env.pending) callback();
    return {...result, rebuilt: env.rebuilt, captured: env.captured, messages: env.messages};
  };
  const individual = name => ({name: `assets/${name}`, root: resolve(dir, 'assets'), relative: name});
  const assertRebuild = change => {
    const result = push(change);
    expect(result).toEqual({assetsNeedRebuild: true, changed: [change.name], rebuilt: 1, captured: 0, messages: []});
  };
  try {
    mkdirSync(resolve(dir, 'assets'));
    writeFileSync(svg, '<svg><script/></svg>');
    assertRebuild(individual('picture.svg'));
    rmSync(svg);
    assertRebuild(individual('picture.svg'));
    writeFileSync(svg, '<!DOCTYPE svg [<!ENTITY a "x">]><svg/>');
    assertRebuild(individual('picture.svg'));
    writeFileSync(svg, '<svg><image href="external.png"/></svg>');
    assertRebuild({name: 'assets', root: resolve(dir, 'assets'), targetRoot: 'assets', tree: true});
    writeFileSync(resolve(dir, 'assets/unused.svg'), '<svg/>');
    const unreferenced = push(individual('unused.svg'));
    expect([unreferenced.assetsNeedRebuild, unreferenced.rebuilt, unreferenced.captured, unreferenced.messages.length])
      .toEqual([false, 0, 1, 1]);
  } finally { rmSync(dir, {recursive: true, force: true}); }
});

// Run the real loops with stand-in build tools. Each slow tool has a child
// that ignores SIGTERM and can replace dist after its launcher has exited.
const preload = `
import { mock } from 'bun:test';
import { appendFileSync, writeFileSync } from 'node:fs';
import * as files from 'node:fs';
import * as children from 'node:child_process';
import * as rust from ${JSON.stringify(resolve(root, 'scripts/rust.mjs'))};
import * as serve from ${JSON.stringify(resolve(root, 'host/web/serve.mjs'))};
import * as startup from ${JSON.stringify(resolve(root, 'host/web/dev-startup.mjs'))};
import * as modules from ${JSON.stringify(resolve(root, 'host/web-js/module.mjs'))};
const spawn = children.spawn, spawnSync = children.spawnSync;
const watch = files.watch;
mock.module('node:fs', () => ({ ...files, watch(...args) {
  if (process.env.FIXTURE_PHASE !== 'queued-edit' || args[0] !== process.env.FIXTURE_DIR) return watch(...args);
  const callback = args.at(-1); let signalled = false;
  args[args.length - 1] = (...event) => {
    callback(...event);
    if (!signalled && String(event[1]) === 'app.contract') {
      signalled = true;
      // Stop after the watcher arms its timer, before that timer can fire.
      process.emit(process.env.FIXTURE_SIGNAL);
      writeFileSync(process.env.FIXTURE_DIR + '/.queued-signal', '');
    }
  };
  return watch(...args);
} }));
mock.module(${JSON.stringify(resolve(root, 'host/web-js/module.mjs'))}, () => ({ ...modules,
  webCompiler: () => ({ cmd: 'cargo', pre: ['run', '-p', 'exact-web-js', '--'], done() {} }),
}));
const startupBuilds = startup.startupBuilds;
mock.module(${JSON.stringify(resolve(root, 'host/web/dev-startup.mjs'))}, () => ({ ...startup,
  startupBuilds() {
    const owner = startupBuilds(); let handoffs = 0;
    return { ...owner, handoff(stop) {
      // Dispatch at the last startup instruction, before the serving handler owns shutdown.
      if (++handoffs === 2 && process.env.FIXTURE_PHASE === 'reader-handoff') {
        process.emit(process.env.FIXTURE_SIGNAL);
        writeFileSync(process.env.FIXTURE_DIR + '/.handoff', '');
      }
      owner.handoff(stop);
    } };
  },
}));
mock.module(${JSON.stringify(resolve(root, 'scripts/rust.mjs'))}, () => ({ ...rust, rustInputs: () => [] }));
mock.module(${JSON.stringify(resolve(root, 'host/web/serve.mjs'))}, () => ({ ...serve,
  syncStaticTree: () => false,
  // The resident checks the served shaders at startup, reaching the reflect tool's build when it is missing.
  applyShaderTreeChange: (app, target, validate) => { if (process.env.FIXTURE_PHASE === 'reflect') validate(target); },
}));
mock.module('node:child_process', () => ({ ...children,
  spawn(command, args, options) {
    let kind;
    if (args[0]?.endsWith('/host/web-js/build.mjs')) kind = 'js';
    else if (args[0]?.endsWith('/host/web/build.mjs')) kind = 'host';
    else if (args[0]?.endsWith('/scripts/rust.mjs')) kind = 'rust';
    else if (args.includes('inspect-table')) kind = 'inspect';
    else if (command === 'cargo' && args.includes('exact-js-bake')) kind = 'module-build';
    else if (command.endsWith('/exact-js-bake')) kind = 'module';
    else if (command === 'cargo' && args.includes('exact-web-js')) kind = 'reader';
    else if (command === 'cargo' && args.includes('exact-gpu-reflect')) kind = 'reflect';
    else if (command === 'cargo' && args.some(a => /-gpu(?:-extra)?$/.test(a))) kind = 'gpu';
    else if (command === 'cargo' && args[0] === 'run') kind = 'producer';
    if (!kind) throw new Error('unexpected tool: ' + command + ' ' + args.join(' '));
    if (kind.startsWith('module')) writeFileSync(process.env.FIXTURE_DIR + '/.' + kind + '.json', JSON.stringify({ command, args }));
    const child = spawn(process.execPath, [process.env.FIXTURE_TOOL, kind, ...args], options);
    appendFileSync(process.env.FIXTURE_DIR + '/.pids', child.pid + '\\n');
    return child;
  },
  spawnSync(command, args, options) {
    if (command === 'cargo' && args.includes('exact-js-bake')) return spawnSync(process.execPath, [process.env.FIXTURE_TOOL, 'module-build'], options);
    if (command === 'cargo' && args.includes('exact-gpu-reflect')) return spawnSync(process.execPath, [process.env.FIXTURE_TOOL, 'reflect'], options);
    if (command !== 'cargo' || (args[0] !== 'metadata' && !args.includes('sources'))) return spawnSync(command, args, options);
    const value = args[0] === 'metadata' ? { packages: [] } : { sources: [], consulted: [], packages: [] };
    return { status: 0, stdout: JSON.stringify(value), stderr: '' };
  },
}));
`;

const tool = `
import { spawn } from 'node:child_process';
import { appendFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { replaceBuild } from ${JSON.stringify(resolve(root, 'host/web/serve.mjs'))};
const dir = process.env.FIXTURE_DIR, dist = process.env.EXACT_WEB_DIST;
const kind = process.argv[2];
appendFileSync(resolve(dir, '.pids'), process.pid + '\\n');
if (kind === 'module-build' && process.env.FIXTURE_PHASE === 'module') process.exit(0);
if (kind === 'reflect' && process.env.FIXTURE_PHASE !== 'reflect') process.exit(0);
if (kind === 'host' && !existsSync(resolve(dir, '.built'))) {
  mkdirSync(dist, { recursive: true });
  writeFileSync(resolve(dist, 'bake.json'), JSON.stringify({ version: 1, graph: { version: 1, sources: {} },
    compat: { id: 'fixture', inputs: {}, target: 'web' }, binary: { sha256: 'fixture' } }));
  writeFileSync(resolve(dist, 'index.html'), 'kept');
  writeFileSync(resolve(dist, 'app.wasm'), 'fixture');
  writeFileSync(resolve(dir, '.built'), '');
  if (process.env.FIXTURE_PHASE === 'gpu') {
    const profile = resolve(process.env.CARGO_TARGET_DIR, 'wasm32-unknown-unknown/web'); mkdirSync(profile, { recursive: true });
    for (const kind of ['gpu', 'gpu-extra', 'web']) writeFileSync(resolve(profile, (basename(dir) + '-' + kind).replaceAll('-', '_') + '.d'), 'artifact: ' + resolve(dir, kind === 'web' ? 'runtime.rs' : 'shared.rs') + '\\n');
  }
} else if (kind === 'js' && (process.env.FIXTURE_PHASE.startsWith('reader') || process.env.FIXTURE_PHASE === 'inspect'
  || (process.env.FIXTURE_PHASE === 'queued-edit' && !existsSync(resolve(dir, '.built'))))) {
  writeFileSync(resolve(dir, '.built'), '');
  process.exit(1);
} else if (kind === 'worker') {
  process.on('SIGTERM', () => {});
  writeFileSync(resolve(dir, '.ready-' + process.env.FIXTURE_KIND), '');
  const timer = setInterval(() => {
    if (!existsSync(resolve(dir, '.release'))) return;
    clearInterval(timer);
    if (!['producer', 'rust'].includes(process.env.FIXTURE_KIND)) {
      const stage = resolve(dir, 'late-stage'); mkdirSync(stage);
      writeFileSync(resolve(stage, 'index.html'), 'late replacement');
      replaceBuild(stage, dist);
    }
    process.exit(0);
  }, 50);
} else {
  process.on('SIGTERM', () => { if (process.env.FIXTURE_STUBBORN !== '1') process.exit(0); });
  const worker = spawn(process.execPath, [import.meta.filename, 'worker'], {
    env: { ...process.env, FIXTURE_KIND: kind }, stdio: ['ignore', 'inherit', 'inherit'],
  });
  worker.once('exit', code => process.exit(code));
}
`;

// A stand-in process: `end()` makes it exit as Node reports one.
const fake = pid => Object.assign(new EventEmitter(), { pid, exitCode: null, signalCode: null, kills: 0,
  kill() { this.kills++; },
  end() { this.exitCode = 0; this.emit('exit', 0, null); } });

// On Windows, whose groups cannot be signalled, the tree is ended under the
// launcher's pid; no POSIX group signal is sent.
for (const helperDoes of ['ends the tree', 'hangs', 'cannot start']) {
  test(`a Windows build tree is stopped when taskkill ${helperDoes}`, async () => {
    const child = fake(4242), asked = [], signalled = [], kill = process.kill;
    let helper = null;
    const spawnTool = (command, args, options) => {
      asked.push([command, args, options]);
      if (helperDoes === 'cannot start') throw new Error('spawn taskkill ENOENT');
      helper = fake(4243);
      if (helperDoes === 'ends the tree') queueMicrotask(() => { child.end(); helper.end(); });
      return helper;
    };
    if (helperDoes !== 'ends the tree') child.kill = function () { this.kills++; queueMicrotask(() => this.end()); };
    process.kill = (...args) => { signalled.push(args); };
    const began = Date.now();
    try { await stopBuildTree(child, { platform: 'win32', spawnTool }); }
    finally { process.kill = kill; }
    expect(asked).toEqual([['taskkill', ['/PID', '4242', '/T', '/F'], { windowsHide: true, stdio: 'ignore' }]]);
    expect(signalled).toEqual([]);
    expect({ helperKills: helper?.kills ?? 0, childKills: child.kills, ended: child.exitCode })
      .toEqual({ helperKills: helperDoes === 'hangs' ? 1 : 0, childKills: helperDoes === 'ends the tree' ? 0 : 1, ended: 0 });
    // Each wait is bounded at a second.
    expect(Date.now() - began).toBeLessThan(helperDoes === 'hangs' ? 2_000 : 500);
  });
}

const alive = pid => {
  const state = spawnSync('ps', ['-p', String(pid), '-o', 'stat='], { encoding: 'utf8' }).stdout.trim();
  return state !== '' && !state.startsWith('Z');
};

async function until(pass, what) {
  const end = Date.now() + 15_000;
  while (!pass()) {
    if (Date.now() > end) throw new Error(`timed out: ${what()}`);
    await new Promise(ok => setTimeout(ok, 50));
  }
}

for (const signal of ['SIGINT', 'SIGTERM']) for (const phase of ['js', 'producer', 'rebuild', 'module-build', 'module', 'mixed', 'reader', 'reader-handoff', 'queued-edit', 'inspect', 'gpu', 'reflect']) {
  test(`shutdown on ${signal} during ${phase} owns its descendants and keeps dist`, async () => {
    const dir = mkdtempSync(resolve(root, 'dev-shutdown-')), dist = resolve(dir, 'dist');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Shutdown', app: { id: 'test.shutdown', name: 'Shutdown' },
      ...(phase === 'mixed' ? { rust: { module: { package: 'fixture' } } } : {}),
      ...(phase === 'gpu' ? { gpu: { modules: { extra: ['extra'] } } } : {}) }));
    if (['module-build', 'module', 'mixed'].includes(phase)) writeFileSync(resolve(dir, 'app.ts'), 'export const value = 1;');
    if (phase === 'gpu') for (const name of ['shared.rs', 'runtime.rs']) writeFileSync(resolve(dir, name), '// fixture\n');
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "shutdown"\n');
    writeFileSync(resolve(dir, 'preload.mjs'), preload);
    writeFileSync(resolve(dir, 'tool.mjs'), tool);
    mkdirSync(dist); writeFileSync(resolve(dist, 'index.html'), 'kept');
    if (phase === 'inspect') writeFileSync(resolve(dist, 'app.plan'), 'fixture');
    const socket = createServer(); await new Promise(ok => socket.listen(0, '127.0.0.1', ok));
    const port = socket.address().port; await new Promise(ok => socket.close(ok));
    const env = { ...process.env, BUN: process.execPath, EXACT_WEB_DIST: dist, FIXTURE_DIR: dir, FIXTURE_TOOL: resolve(dir, 'tool.mjs'),
      CARGO_TARGET_DIR: resolve(dir, 'target'), FIXTURE_PHASE: phase, FIXTURE_SIGNAL: signal, FIXTURE_STUBBORN: signal === 'SIGINT' ? '1' : '0',
      EXACT_TEST_TIMING: '1', EXACT_DEV_LISTEN_DELAY_MS: phase === 'producer' ? '60000' : '0' };
    const args = ['--preload', resolve(dir, 'preload.mjs'), resolve(root, 'host/web/dev.mjs'), '--app', dir, '--port', String(port)];
    if (!['js', 'queued-edit', 'inspect'].includes(phase) && !phase.startsWith('reader')) args.push('--wasm');
    if (!['js', 'gpu', 'queued-edit', 'inspect'].includes(phase) && !phase.startsWith('reader')) args.push('--serve-as', String(port));
    const dev = spawnOwned(process.execPath, args, { cwd: root, env, stdio: ['pipe', 'pipe', 'pipe'] });
    const exit = new Promise(ok => dev.once('exit', (code, signal) => ok({ code, signal })));
    let output = ''; for (const stream of [dev.stdout, dev.stderr]) stream.on('data', bytes => { output += bytes; });
    const pids = () => existsSync(resolve(dir, '.pids')) ? [...new Set(readFileSync(resolve(dir, '.pids'), 'utf8').trim().split('\n').map(Number))] : [];
    let inspection = null;
    try {
      if (['queued-edit', 'inspect'].includes(phase)) {
        await until(() => output.includes('plan ready') && existsSync(resolve(dir, '.ready-reader')), () => output);
        if (phase === 'queued-edit') writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "edited"\n');
        else {
          inspection = request(`http://127.0.0.1:${port}/dev-inspect.json`, res => res.resume());
          inspection.on('error', () => {}); inspection.end();
        }
      }
      if (phase === 'rebuild') {
        await until(() => output.includes(`127.0.0.1:${port}/`) && existsSync(resolve(dir, '.ready-producer')), () => output);
        dev.stdin.write('r\n');
      }
      if (phase === 'gpu') {
        await until(() => output.includes(`127.0.0.1:${port}/`) && existsSync(resolve(dir, '.ready-producer')), () => output);
        writeFileSync(resolve(dir, 'shared.rs'), '// changed\n');
      }
      const ready = phase === 'reader-handoff' ? '.handoff' : phase === 'queued-edit' ? '.queued-signal'
        : '.ready-' + (phase === 'rebuild' ? 'host' : phase === 'mixed' ? 'module-build' : phase);
      await until(() => existsSync(resolve(dir, ready)) && (phase !== 'producer' || output.includes('TEST delay')), () => output + ' pids: ' + pids() + ' exit: ' + dev.exitCode + '/' + dev.signalCode);
      if (phase === 'module') {
        const build = JSON.parse(readFileSync(resolve(dir, '.module-build.json')));
        expect(build.args).toContain('--profile');
        expect(build.args[build.args.indexOf('--profile') + 1]).toBe('host-dev');
        expect(build.args).not.toContain('--release');
        const producer = JSON.parse(readFileSync(resolve(dir, '.module.json')));
        expect(producer.command).toBe(resolve(env.CARGO_TARGET_DIR, 'host-dev/exact-js-bake'));
      }
      if (phase === 'mixed') await until(() => existsSync(resolve(dir, '.ready-rust')), () => output);
      if (!['reader-handoff', 'queued-edit'].includes(phase)) dev.kill(signal);
      await until(() => dev.exitCode !== null || dev.signalCode !== null, () => output);
      const owned = pids();
      const survivors = owned.filter(alive);
      // Let any leaked tool finish its write, then observe dist after all
      // recorded work has ended, rather than sleeping for a quiet interval.
      writeFileSync(resolve(dir, '.release'), '');
      await until(() => owned.every(pid => !alive(pid)), () => `survivors: ${owned.filter(alive)}`);
      expect({ survivors, page: readFileSync(resolve(dist, 'index.html'), 'utf8') }).toEqual({ survivors: [], page: 'kept' });
      expect(await exit).toEqual({ code: 0, signal: null });
    } finally {
      inspection?.destroy();
      await dev.stop(1000);
      for (const pid of pids().filter(alive)) { try { process.kill(pid, 'SIGKILL'); } catch {} }
      rmSync(dir, { recursive: true, force: true });
    }
  }, 40_000);
}

test('a frontend claim names its live owner, separates targets, and releases after a crash', async () => {
  const dir=mkdtempSync(resolve(root,'dev-loop-claim-'));
  let owner, wasm, next, child;
  try {
    owner=claimDevLoop(dir,'js'); owner.url('http://127.0.0.1:9876');
    expect(()=>claimDevLoop(dir,'js')).toThrow(`already runs at http://127.0.0.1:9876 (pid ${process.pid})`);
    wasm=claimDevLoop(dir,'wasm'); wasm.close(); owner.close();
    const script=`import {claimDevLoop} from ${JSON.stringify(resolve(root,'host/web/dev-startup.mjs'))}; const claim=claimDevLoop(process.argv[1],'js'); claim.url('http://127.0.0.1:9877'); console.log('claimed'); setInterval(()=>{},1000);`;
    child=spawnOwned(process.execPath,['-e',script,dir],{stdio:['ignore','pipe','pipe']});
    await new Promise(ok=>child.stdout.once('data',ok));
    expect(()=>claimDevLoop(dir,'js')).toThrow(`http://127.0.0.1:9877 (pid ${child.pid})`);
    const exit=new Promise(ok=>child.once('exit',ok)); child.kill('SIGKILL'); await exit;
    next=claimDevLoop(dir,'js'); next.url('http://127.0.0.1:9878');
    expect(JSON.parse(readFileSync(resolve(dir,'.exact/web-js-loop.json'),'utf8')).url).toBe('http://127.0.0.1:9878');
  } finally { next?.close(); wasm?.close(); owner?.close(); rmSync(dir,{recursive:true,force:true}); }
});
