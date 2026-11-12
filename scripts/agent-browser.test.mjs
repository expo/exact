import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { notApplicable } from './coverage.mjs';
import { quitBrowser, pollUntil } from './agent-launch.mjs';

test('a drive asks its Chrome to quit and kills its group only after the bound when it hangs', async () => {
  const fixture = ({ exitsAfter, replies }) => {
    const child = { pid: 1, exitCode: null, signalCode: null };
    const said = [], start = performance.now(), at = () => Math.round(performance.now() - start);
    let exit;
    const exited = new Promise((resolve) => { exit = () => { child.exitCode = 0; said.push(['exit', at()]); resolve(); }; });
    const cdp = { send: (method) => {
      said.push([method, at()]);
      if (exitsAfter != null) setTimeout(exit, exitsAfter);
      return replies ? Promise.resolve({}) : exitsAfter != null ? Promise.reject(new Error('Chrome exited')) : new Promise(() => {});
    } };
    return { child, said, exited, cdp, kill: () => { said.push(['kill', at()]); if (exitsAfter == null) exit(); } };
  };
  const quit = async (f, ms) => { await quitBrowser(f.child, f.cdp, f.exited, { ms, kill: f.kill }); return f.said.map(([what]) => what); };
  // Exits on the ask, answering or closing the pipe first: well inside the bound, without a kill.
  for (const replies of [true, false]) {
    const f = fixture({ exitsAfter: 30, replies });
    expect(await quit(f, 2000)).toEqual(['Browser.close', 'exit']);
    expect(f.said.at(-1)[1]).toBeLessThan(500);
  }
  // Hung: no answer, no exit. Killed at the bound.
  const hung = fixture({});
  expect(await quit(hung, 200)).toEqual(['Browser.close', 'kill', 'exit']);
  expect(hung.said.at(-1)[1]).toBeGreaterThanOrEqual(195);
});

test('a crashed Chrome has its group cleaned up and failed termination retains the profile', async () => {
  const killed = [], cdp = { send: () => Promise.resolve({}) };
  for (const status of [{ exitCode: 1, signalCode: null }, { exitCode: null, signalCode: 'SIGSEGV' }]) {
    await quitBrowser({ pid: 1, ...status }, cdp, Promise.resolve(), { kill: () => killed.push('group') });
  }
  expect(killed).toEqual(['group', 'group']);
  const profile = mkdtempSync(resolve(tmpdir(), 'exact-live-chrome-'));
  try {
    for (const exited of [Promise.resolve(), new Promise(() => {})]) {
      await expect((async () => {
        await quitBrowser({ pid: 42, exitCode: null, signalCode: null }, cdp, exited, { ms: 10, kill: () => {} });
        rmSync(profile, { recursive: true, force: true });
      })()).rejects.toThrow('Chrome 42 did not exit');
      expect(existsSync(profile)).toBe(true);
    }
  } finally { rmSync(profile, { recursive: true, force: true }); }
});

test('a shared CDP browser is replaced as soon as its exit rejects a pending request', () => {
  const run = spawnSync(process.execPath, ['-e', `
import { mock, expect } from 'bun:test';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
const processes = await import('node:child_process'), children = [], forced = [];
process.kill = (...args) => forced.push(args);
mock.module('node:child_process', () => ({ ...processes, spawn: () => {
  const child = new EventEmitter();
  child.pid = 1; child.exitCode = null; child.signalCode = null;
  child.stderr = new PassThrough();
  child.stdio = [null, null, child.stderr, new PassThrough(), new PassThrough()];
  child.finish = () => { child.exitCode = 0; child.emit('exit', 0); };
  child.stdio[3].on('data', data => {
    const msg = JSON.parse(String(data).replace(/\\0$/, ''));
    if (msg.method === 'Fixture.pending') return;
    if (msg.method === 'Browser.close') { child.finish(); return; }
    child.stdio[4].write(JSON.stringify({ id: msg.id, result: { browserContextId: String(msg.id) } }) + '\\0');
  });
  children.push(child); return child;
} }));
const { cdpBrowser } = await import(${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))});
const first = await cdpBrowser();
const pending = first.cdp.send('Fixture.pending');
children[0].finish();
await expect(pending).rejects.toThrow('Chrome exited');
const replacement = await cdpBrowser();
expect(replacement.cdp).not.toBe(first.cdp);
expect(children.length).toBe(2);
expect(forced).toEqual([]);
await replacement.close();
`], { env: { ...process.env, CHROME: process.execPath }, encoding: 'utf8', timeout: 5000 });
  expect(run.stderr).toBe('');
  expect(run.error).toBeUndefined();
  expect(run.status).toBe(0);
});

test('a shared CDP browser quits before its test process exits and removes its profile after termination', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-cdp-exit-'));
  const chrome = resolve(dir, 'chrome'), log = resolve(dir, 'browser.json');
  writeFileSync(chrome, `#!${process.execPath}
import { spawn } from 'node:child_process';
import { createReadStream, existsSync, writeSync, writeFileSync } from 'node:fs';
const log = process.env.BROWSER_LOG, mode = process.env.BROWSER_MODE;
const state = { pid: process.pid, profile: process.argv.find(a => a.startsWith('--user-data-dir=')).split('=')[1], methods: [] };
const save = () => writeFileSync(log, JSON.stringify(state));
save(); writeFileSync(log + '.clone', 'bundle');
let buffer = '';
createReadStream('', { fd: 3 }).on('data', chunk => {
  buffer += chunk;
  let end;
  while ((end = buffer.indexOf('\\0')) >= 0) {
    const msg = JSON.parse(buffer.slice(0, end)); buffer = buffer.slice(end + 1);
    state.methods.push(msg.method); save();
    if (msg.method === 'Browser.close') {
      if (mode !== 'hung') {
        const helper = spawn(process.execPath, ['-e', \`
import { rmSync } from 'node:fs';
const timer = setInterval(() => {
  if (process.ppid !== 1) return;
  rmSync(process.env.BROWSER_LOG + '.clone');
  clearInterval(timer);
}, 50);
\`], { stdio: 'ignore' });
        state.helper = helper.pid; state.clean = true; save(); helper.unref();
        setTimeout(() => {
          state.profileAtExit = existsSync(state.profile); save(); process.exit(0);
        }, mode === 'normal' ? 3500 : 0);
      }
    } else writeSync(4, JSON.stringify({ id: msg.id, ...(mode === 'startup'
      ? { error: { message: 'fixture refused context' } } : { result: { browserContextId: String(msg.id) } }) }) + '\\0');
  }
});
`);
  chmodSync(chrome, 0o755);
  const suite = resolve(dir, 'shutdown.test.mjs');
  writeFileSync(suite, `
import { test, expect } from 'bun:test';
import { EventEmitter } from 'node:events';
const once = EventEmitter.prototype.once;
let browser;
EventEmitter.prototype.once = function(event, ...args) { if (this.spawnfile && event === 'exit') browser = this; return once.call(this, event, ...args); };
import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))};
test('suite owns contexts on one browser', async () => {
  const [a, b] = await Promise.all([cdpBrowser(), cdpBrowser()]);
  expect(a.cdp).toBe(b.cdp);
  expect(a.browserContextId).not.toBe(b.browserContextId);
  if (process.env.BROWSER_MODE === 'error') browser.emit('error', new Error('a failed kill'));
  await a.close(); await b.close();
  expect(process.env.BROWSER_MODE).not.toBe('failure');
});
`);
  try {
    for (const mode of ['error', 'normal', 'failure', 'hung', 'startup', 'script', 'script-failure']) {
      const script = mode.startsWith('script');
      const args = script ? ['-e', `import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))};
        const suite = await cdpBrowser(); await suite.close(); process.exitCode = ${mode === 'script-failure' ? 1 : 0};`] : ['test', suite];
      const run = spawnSync(process.execPath, args, {
        env: { ...process.env, CHROME: chrome, BROWSER_LOG: log, BROWSER_MODE: mode }, encoding: 'utf8', timeout: 15000,
      });
      const state = JSON.parse(readFileSync(log, 'utf8'));
      try {
        expect(run.error).toBeUndefined();
        expect(run.status).toBe(['failure', 'startup', 'script-failure'].includes(mode) ? 1 : 0);
        const creates = script ? 1 : 2, disposes = mode === 'startup' ? 0 : creates;
        expect(state.methods).toEqual([...Array(creates).fill('Target.createBrowserContext'),
          ...(mode === 'error' ? ['Browser.close'] : []),
          ...Array(disposes).fill('Target.disposeBrowserContext'), ...(mode === 'error' ? [] : ['Browser.close'])]);
        expect(Boolean(state.clean)).toBe(mode !== 'hung');
        if (mode !== 'hung') {
          expect(state.profileAtExit).toBe(true);
          expect(await pollUntil(() => existsSync(log + '.clone'), value => !value, { bound: 3000 })).toBe(false);
        }
        expect(() => process.kill(state.pid, 0)).toThrow();
        expect(existsSync(state.profile)).toBe(false);
      } finally {
        try { process.kill(state.pid, 'SIGKILL'); } catch {}
        if (state.helper) { try { process.kill(state.helper, 'SIGKILL'); } catch {} }
        rmSync(state.profile, { recursive: true, force: true });
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 45000);

test('the benchmark handles a missing initial page, retries navigation and closes Chrome on failures', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-bench-close-'));
  const chrome = resolve(dir, 'chrome'), log = resolve(dir, 'browser.json');
  writeFileSync(chrome, `#!${process.execPath}
import { createReadStream, existsSync, writeSync, writeFileSync } from 'node:fs';
const state = { pid: process.pid, profile: process.argv.find(a => a.startsWith('--user-data-dir=')).slice(16), methods: [] };
const save = () => writeFileSync(process.env.BROWSER_LOG + '.' + process.pid, JSON.stringify(state));
let buffer = '', evaluations = 0;
save();
createReadStream('', { fd: 3 }).on('data', chunk => {
  buffer += chunk;
  let end;
  while ((end = buffer.indexOf('\\0')) >= 0) {
    const msg = JSON.parse(buffer.slice(0, end)); buffer = buffer.slice(end + 1);
    state.methods.push(msg.method); save();
    if (msg.method === 'Browser.close') { state.clean = true; state.profileAtExit = existsSync(state.profile); save(); process.exit(0); }
    let result = {};
    if (msg.method === 'Target.getTargets') result = { targetInfos: process.env.BROWSER_MODE === 'no-page' ? [] : [{ type: 'page', targetId: 'page' }] };
    if (msg.method === 'Target.createTarget') result = { targetId: 'created-page' };
    if (msg.method === 'Target.attachToTarget') result = { sessionId: 'session' };
    if (msg.method === 'Runtime.evaluate') result = { result: { value: JSON.stringify({ origin: Date.now(), done: true, runtime: true, content: 1, fcp: 1 }) } };
    const error = process.env.BROWSER_MODE === 'startup' ? 'fixture refused startup' :
      process.env.BROWSER_MODE === 'measurement' && msg.method === 'Runtime.evaluate' ? 'fixture evaluation failed' :
      msg.method === 'Runtime.evaluate' && evaluations++ === 0 ? 'Execution context was destroyed.' : null;
    writeSync(4, JSON.stringify({ id: msg.id, ...(error ? { error: { message: error } } : { result }) }) + '\\0');
  }
});
`);
  chmodSync(chrome, 0o755);
  try {
    for (const mode of ['no-page', 'navigation', 'startup', 'measurement']) {
      const succeeds = mode === 'no-page' || mode === 'navigation';
      const run = spawnSync(process.execPath, [resolve(import.meta.dir, '../host/web-js/bench.mjs'), 'fixture=http://127.0.0.1:9', '--profile', 'none', '--runs', '1'], {
        env: { ...process.env, CHROME: chrome, BROWSER_LOG: log + mode, BROWSER_MODE: mode }, encoding: 'utf8', timeout: 15000,
      });
      const states = readdirSync(dir).filter(name => name.startsWith('browser.json' + mode + '.'))
        .map(name => JSON.parse(readFileSync(resolve(dir, name), 'utf8')));
      try {
        expect(run.error).toBeUndefined();
        expect(run.status).toBe(succeeds ? 0 : 1);
        expect(states.length).toBe(succeeds ? 3 : 1);
        for (const state of states) {
          expect(state.methods.includes('Target.createTarget')).toBe(mode === 'no-page');
          expect(state.clean).toBe(true);
          expect(state.profileAtExit).toBe(true);
          expect(() => process.kill(state.pid, 0)).toThrow();
          expect(existsSync(state.profile)).toBe(false);
        }
      } finally {
        for (const state of states) { try { process.kill(state.pid, 'SIGKILL'); } catch {} }
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30000);

const branded = notApplicable(process.platform !== 'darwin' ? 'macOS only' :
  !/Google Chrome( Canary)?\.app\//.test(process.env.CHROME ?? '') ? 'CHROME must name branded Chrome' : null);
branded(
  'branded Chrome removes its bundle copies after script and test shutdown', async () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-branded-close-'));
    const temporary = spawnSync('getconf', ['DARWIN_USER_TEMP_DIR'], { encoding: 'utf8' }).stdout.trim();
    const clones = resolve(temporary, '../X', process.env.CHROME.includes('Canary') ? 'com.google.Chrome.canary.code_sign_clone' : 'com.google.Chrome.code_sign_clone');
    const names = () => existsSync(clones) ? readdirSync(clones) : [];
    const before = new Set(names()), added = () => names().filter(name => !before.has(name));
    const source = `import { test } from 'bun:test';
import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))};
test('browser', async () => { const b = await cdpBrowser(); await b.page(); await b.close(); if (process.env.BROWSER_FAIL) throw new Error('fixture failure'); });`;
    const suite = resolve(dir, 'branded.test.mjs');
    writeFileSync(suite, source);
    try {
      for (const mode of ['script', 'test', 'failure']) {
        const args = mode === 'script' ? ['-e', `import { cdpBrowser } from ${JSON.stringify(resolve(import.meta.dir, 'agent-launch.mjs'))}; const b = await cdpBrowser(); await b.page(); await b.close();`] : ['test', suite];
        const run = spawnSync(process.execPath, args, { env: { ...process.env, BROWSER_FAIL: mode === 'failure' ? '1' : '' }, encoding: 'utf8', timeout: 20000 });
        expect(run.error).toBeUndefined();
        expect(run.status).toBe(mode === 'failure' ? 1 : 0);
        // Chrome's orphaned cleanup helper polls its parent once a second.
        expect(await pollUntil(added, value => value.length === 0, { bound: 5000, interval: 100 })).toEqual([]);
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 60000,
);
