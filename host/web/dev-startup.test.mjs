import { afterEach, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { EventEmitter } from 'node:events';
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, symlinkSync, utimesSync, watch, writeFileSync } from 'node:fs';
import { partitionWebRoots } from './build.mjs';
import { declaredSources } from '../../scripts/file-dependencies.mjs';
import { depInfoOf } from '../../scripts/cargo-dep-info.mjs';
import { EXE } from '../../scripts/paths.mjs';
import { request } from 'node:http';
import { createServer } from 'node:net';
import { homedir } from 'node:os';
import { basename, dirname, resolve, sep } from 'node:path';
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
import * as webBuild from ${JSON.stringify(resolve(root, 'host/web/build.mjs'))};
import * as sweep from ${JSON.stringify(resolve(root, 'scripts/sweep.mjs'))};
const spawn = children.spawn, spawnSync = children.spawnSync;
const watch = files.watch;
mock.module(${JSON.stringify(resolve(root, 'host/web/build.mjs'))}, () => ({ ...webBuild, webSourceRoots: () => [] }));
mock.module(${JSON.stringify(resolve(root, 'scripts/sweep.mjs'))}, () => ({ ...sweep, startSweep() {} }));
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
    for (const kind of ['gpu', 'gpu-extra', 'web']) writeFileSync(resolve(profile, (basename(dir) + '-' + kind).replaceAll('-', '_') + '.d'), 'artifact: ' + resolve(dir, kind === 'web' ? 'runtime.rs' : 'node_modules/fixture/shared.rs') + '\\n');
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
    if (phase === 'gpu') {
      mkdirSync(resolve(dir, 'node_modules/fixture'), { recursive: true });
      for (const name of ['node_modules/fixture/shared.rs', 'runtime.rs']) writeFileSync(resolve(dir, name), '// fixture\n');
    }
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "shutdown"\n');
    writeFileSync(resolve(dir, 'preload.mjs'), preload);
    writeFileSync(resolve(dir, 'tool.mjs'), tool);
    mkdirSync(dist); writeFileSync(resolve(dist, 'index.html'), 'kept');
    if (phase === 'inspect') writeFileSync(resolve(dist, 'app.plan'), 'fixture');
    if (phase.startsWith('reader') || ['queued-edit', 'inspect'].includes(phase)) {
      const debug = resolve(dir, `target/debug/exact-web-js${EXE}`);
      mkdirSync(dirname(debug), { recursive: true });
      writeFileSync(debug, '', { mode: 0o755 });
      writeFileSync(depInfoOf(debug), `reader: ${resolve(root, 'Cargo.toml').replaceAll(' ', '\\ ')}\n`);
    }
    const socket = createServer(); await new Promise(ok => socket.listen(0, '127.0.0.1', ok));
    const port = socket.address().port; await new Promise(ok => socket.close(ok));
    const env = { ...process.env, BUN: process.execPath, EXACT_WEB_DIST: dist, FIXTURE_DIR: dir, FIXTURE_TOOL: resolve(dir, 'tool.mjs'),
      HOME: dir, USERPROFILE: dir, EXACT_TOOLS_ORIGIN: 'none', EXACT_WEB_COMPILER: '',
      CARGO_HOME: process.env.CARGO_HOME ?? resolve(homedir(), '.cargo'), RUSTUP_HOME: process.env.RUSTUP_HOME ?? resolve(homedir(), '.rustup'),
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
        writeFileSync(resolve(dir, 'node_modules/fixture/shared.rs'), '// changed\n');
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

test.each([false, true])('JS receipt recovers from metadata errors and records Cargo reads first seen during build: %s', async firstRead => {
  const { cargoContractReads, changedContractReads, contractReadReceipt } = await import('../../scripts/web-build.mjs');
  const { webChanges } = await import('../../scripts/agent-launch.mjs');
  const source = readFileSync(resolve(root, 'host/web-js/dev.mjs'), 'utf8');
  const body = source.slice(source.indexOf('function build('), source.indexOf('\nconst mtimeMs'));
  const dir = mkdtempSync(resolve(root, 'target/dev-receipt-')), dist = resolve(dir, 'dist');
  const file = resolve(dir, 'cargo.contract'), crate = resolve(dir, 'node_modules/fixture/rust'), rust = resolve(crate, 'src/lib.rs');
  const app = { dir, target: resolve(dir, 'target'), name: 'receipt', manifest: {} };
  let produced = !firstRead, failed = false, edit = null;
  const began = Date.now() + 60_000;
  const env = { resolve, dirname, basename, createHash, rmSync, existsSync, readFileSync, writeFileSync, cargoContractReads, changedContractReads, contractReadReceipt,
    Date: class extends Date { static now() { return began; } },
    root, appSources: () => ({ digest: 'sources' }), installedFileDependencies() {}, designPath: () => null,
    devPackages: () => ({ sources: [], trees: [], shallow: [] }), changedSince: () => false, compilerWarnings: () => [], compilerError: () => false,
    devDesign: () => null, mapDesignSource: () => null, appManifestDigest: () => 'app', buildFileCards: () => ({}), builtSettings: () => ({}),
    replaceBuild: (stage, output) => { rmSync(output, { recursive: true, force: true }); mkdirSync(output, { recursive: true });
      writeFileSync(resolve(output, '.exact-build.json'), readFileSync(resolve(stage, '.exact-build.json'))); },
    webSourceRoots: () => { if (failed) throw new Error('cargo metadata: invalid manifest'); return produced ? [crate, { file }, { file: rust }] : []; },
    spawn(command, args) {
      const stage = args[args.indexOf('--out') + 1];
      mkdirSync(resolve(stage, '.gen'), { recursive: true });
      writeFileSync(resolve(stage, '.gen/app.plan.map.json'), JSON.stringify({ sources: [] }));
      writeFileSync(resolve(stage, '.exact-dev-logic.json'), 'logic');
      produced = true;
      if (edit === 'delete') rmSync(file);
      else if (edit) { writeFileSync(file, edit); utimesSync(file, began / 1000, began / 1000); }
      const child = Object.assign(new EventEmitter(), { pid: 2 ** 30, stdout: new EventEmitter(), stderr: new EventEmitter() });
      queueMicrotask(() => child.emit('exit', 0)); return child;
    },
  };
  const build = new Function('env', `const {${Object.keys(env).join(',')}} = env; let building, lastSources = []; ${body}; return build;`)(env);
  try {
    mkdirSync(resolve(crate, 'src'), { recursive: true });
    writeFileSync(rust, 'pub const RED: bool = false;');
    writeFileSync(file, 'new Cargo include');
    const built = await build(app, dist);
    expect(built.error).toBeNull();
    expect(built.packages.roots).toContain(crate);
    expect(built.packages.sources).toContain(rust);
    expect(built.packages.roots).toContainEqual({ file: rust });
    expect(JSON.parse(readFileSync(resolve(dist, '.exact-build.json'))).reads.cargo[file]).toBe(createHash('sha256').update('new Cargo include').digest('hex'));
    const before = readFileSync(resolve(dist, '.exact-build.json'), 'utf8');
    failed = true;
    expect((await build(app, dist)).error).toBe('cargo metadata: invalid manifest');
    expect(readFileSync(resolve(dist, '.exact-build.json'), 'utf8')).toBe(before);
    failed = false;
    expect((await build(app, dist)).error).toBeNull();
    for (const change of ['edited during compilation', 'delete']) {
      edit = change;
      const result = await build(app, dist);
      expect(result.superseded).toBe(true);
      expect(result.packages.sources).toContain(file);
      expect(readFileSync(resolve(dist, '.exact-build.json'), 'utf8')).toBe(before);
    }
    edit = null;
    writeFileSync(file, 'new Cargo include');
    utimesSync(file, 1, 1);
    expect((await build(app, dist)).error).toBeNull();
    // Give freshness the roots recorded by the successful build, without another Cargo query.
    const marker = JSON.parse(readFileSync(resolve(dist, '.exact-build.json'))); marker.inputRoots = [{ file }];
    writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify(marker)); rmSync(file);
    expect(webChanges(dist, app).all.some(path => path.endsWith('/cargo.contract'))).toBe(true);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Cargo watchers ignore reads and observe edits through installed source links', () => {
  const source = readFileSync(resolve(root, 'host/web-js/dev.mjs'), 'utf8');
  const times = source.slice(source.indexOf('const changedSince ='), source.indexOf('/** The inputs that moved'));
  const changed = source.slice(source.indexOf('  const changed = (base'), source.indexOf('  // The resident loop\'s start'));
  const watchStart = source.indexOf('\n  const packages = ');
  const watches = source.slice(watchStart, source.indexOf('  watchPackages(built.packages);', watchStart));
  const crate = '/app/node_modules/fixture/rust', file = `${crate}/src/lib.rs`, sdk = '/sdk/compiler';
  const entries = new Set([crate, file, `${crate}/src`, sdk, `${sdk}/src/lib.rs`]), handlers = new Map(), edits = new Set();
  let targetTime = 1;
  const old = { mtimeMs: 1, ctimeMs: 1, birthtimeMs: 1, isSymbolicLink: () => false };
  const env = { resolve, dirname, basename, sep, partitionWebRoots, declaredSources: () => () => null, app: { dir: '/app', manifest: {} },
    built: { packages: { roots: [crate, sdk, { file }], sources: [file] } }, edited: edits,
    skipped: /(^|\/)(node_modules|target)(\/|$)/, settingsFile: () => false,
    existsSync: path => entries.has(path),
    lstatSync: path => { if (!entries.has(path)) throw Error('missing'); return { ...old, isSymbolicLink: () => path === file }; },
    statSync: path => { if (!entries.has(path)) throw Error('missing'); return { ...old, mtimeMs: path === file ? targetTime : 1 }; },
    Date: class extends Date { static now() { return 60_000; } },
    watch(path, options, callback) { handlers.set(path, callback ?? options); return { close() {} }; },
    temporaryOfSource: () => false, OUTPUT: /^$/, INPUT_TREE: /^$/, stopYoungBuild() {}, rebuild() {},
    setTimeout: () => 1, clearTimeout() {},
  };
  new Function('env', `const {${Object.keys(env).join(',')}} = env;
    const watchers = [], stopping = false, since = 50_000, movedSinceBuild = new Set(), appeared = new Set();
    let timer = null, saved, inputs = 0, residentChild = null; const starting = {};
    ${times}\n${changed}\n${watches}\nwatchPackages(built.packages);
  `)(env);
  expect(handlers.has(crate)).toBe(true);
  expect(handlers.has(`${crate}/src`)).toBe(true);
  handlers.get(sdk)('change', 'compiler');
  handlers.get(sdk)('change', 'src/lib.rs');
  handlers.get(crate)('change', 'src/lib.rs');
  expect([...edits]).toEqual([]);
  targetTime = 60_000;
  handlers.get(crate)('change', 'src/lib.rs');
  expect([...edits]).toEqual([file]);
  edits.clear(); entries.delete(file);
  handlers.get(`${crate}/src`)('rename', 'lib.rs');
  expect([...edits]).toEqual([file]);
});

// Bun installs a `file:` package outside the app as links to its declared files, so its source is edited where the
// links lead, which a watcher of the install never hears about: the loop watches the declared directory too.
test('a Cargo watcher sees an edit made at a linked file: package\'s declared directory', async () => {
  const source = readFileSync(resolve(root, 'host/web-js/dev.mjs'), 'utf8');
  const times = source.slice(source.indexOf('const changedSince ='), source.indexOf('/** The inputs that moved'));
  const changed = source.slice(source.indexOf('  const changed = (base'), source.indexOf('  // The resident loop\'s start'));
  const watchStart = source.indexOf('\n  const packages = ');
  const watches = source.slice(watchStart, source.indexOf('  watchPackages(built.packages);', watchStart));
  const dir = realpathSync(mkdtempSync(resolve(root, 'target/linked-watch-'))), app = resolve(dir, 'app'), pkg = resolve(dir, 'pkg');
  const crate = resolve(app, 'node_modules/fixture/rust'), file = resolve(crate, 'src/lib.rs'), edits = new Set(), watchers = [];
  try {
    mkdirSync(resolve(pkg, 'rust/src'), { recursive: true });
    mkdirSync(resolve(crate, 'src'), { recursive: true });
    writeFileSync(resolve(pkg, 'package.json'), JSON.stringify({ name: 'fixture' }));
    writeFileSync(resolve(pkg, 'rust/src/lib.rs'), 'before');
    writeFileSync(resolve(app, 'package.json'), JSON.stringify({ dependencies: { fixture: 'file:../pkg' } }));
    symlinkSync(resolve(pkg, 'package.json'), resolve(app, 'node_modules/fixture/package.json'));
    symlinkSync(resolve(pkg, 'rust/src/lib.rs'), file);
    // The declared directory's watcher counts as ready once it reports a probe beside the source, which the loop
    // ignores (no file of that name is installed). The loop's time starts after the setup, so its writes are no edit.
    let ready = false;
    const watching = (at, options, listener) => watch(at, options, (event, name) => {
      if (at === resolve(pkg, 'rust') && String(name).endsWith('probe')) ready = true;
      listener(event, name);
    });
    const env = { resolve, dirname, basename, sep, existsSync, lstatSync, statSync, watch: watching, partitionWebRoots, declaredSources, watchers, edited: edits,
      app: { dir: app, manifest: {} }, built: { packages: { roots: [crate, { file }], sources: [file] } },
      skipped: /(^|\/)(node_modules|target)(\/|$)/, settingsFile: () => false, temporaryOfSource: () => false, OUTPUT: /^$/, INPUT_TREE: /^$/,
      stopYoungBuild() {}, rebuild() {}, setTimeout: () => 1, clearTimeout() {} };
    new Function('env', `const {${Object.keys(env).join(',')}} = env;
      const stopping = false, since = Date.now() + 1, movedSinceBuild = new Set(), appeared = new Set();
      let timer = null, saved, inputs = 0, residentChild = null; const starting = {};
      ${times}\n${changed}\n${watches}\nwatchPackages(built.packages);
    `)(env);
    await until(() => { writeFileSync(resolve(pkg, 'rust/src/probe'), ''); return ready; }, () => 'the declared directory\'s watcher');
    expect([...edits]).toEqual([]);
    writeFileSync(resolve(pkg, 'rust/src/lib.rs'), 'after');
    await until(() => edits.size, () => 'the edit');
    expect([...edits]).toEqual([file]);
  } finally {
    for (const watcher of watchers) watcher.close();
    rmSync(dir, { recursive: true, force: true });
  }
}, 20_000);
