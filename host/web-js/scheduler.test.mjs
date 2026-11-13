import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
import { expect, test } from 'bun:test';
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

// A data module's `scheduler` on the JS target (LLP 1141.001 D10), ts-fetch.js as it is: with no
// browser `scheduler.yield` (Bun has none, nor has Safari), a yield resolves in a task of its own, after
// the microtasks queued before it; `postTask` refuses, naming the yield; and an overlay may not yield.
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

test('a yield without the browser scheduler resolves in a later task; postTask and an overlay refuse', async () => {
  expect(globalThis.scheduler).toBeUndefined();
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-scheduler-'));
  cpSync(resolve(ROOT, 'host/web-js/ts-fetch.js'), resolve(dir, 'ts-fetch.js'));
  writeFileSync(resolve(dir, 'admission.js'), `export { ownWithout, redirectOf, signalDeadline, socketProtocols } from ${JSON.stringify(pathToFileURL(resolve(ROOT, 'host/web-js/admission.js')).href)};
export class FetchError extends Error {} export const overlaying = { on: false }; export const inOverlay = what => new Error(what + ' in an overlay');
export const fetchWith = () => Promise.reject(new Error('no fetch'));\n`);
  writeFileSync(resolve(dir, 'admission-data.js'), 'export const tsGrantSet = {};\n');
  try {
    const { scheduler } = await import(pathToFileURL(resolve(dir, 'ts-fetch.js')).href);
    const { overlaying } = await import(pathToFileURL(resolve(dir, 'admission.js')).href);
    const order = [];
    const yielded = scheduler.yield().then(() => order.push('yield'));
    let chained = Promise.resolve();
    for (let i = 0; i < 64; i++) chained = chained.then(() => order.push(i));
    await yielded;
    expect(order.at(-1)).toBe('yield');
    expect(() => scheduler.postTask(() => {})).toThrow('await scheduler.yield()');
    overlaying.on = true;
    await expect(scheduler.yield()).rejects.toThrow('scheduler.yield() in an overlay');
    overlaying.on = false;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('sample history keeps only its requested tail across large clock jumps and hidden spans', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-sample-bound-'));
  cpSync(resolve(ROOT, 'host/web-js/samples.js'), resolve(dir, 'samples.js'));
  writeFileSync(resolve(dir, 'rt.js'), 'export const clock={now:0};');
  try {
    const { samples } = await import(pathToFileURL(resolve(dir, 'samples.js')).href);
    const { clock } = await import(pathToFileURL(resolve(dir, 'rt.js')).href);
    const row = { key: 'motion.acceleration', sub: { samples: { frequency: 'frequency', bound: 'history' }, default: { x: 1 } } };
    for (const history of [0, 3]) {
      clock.now = 0; const source = samples(), args = { frequency: 60, history };
      source.answered(row, 'reader', source.substitute(row, args, 'reader'));
      clock.now = 3_600_000;
      let pushes = 0;
      const original = Array.prototype.push;
      Array.prototype.push = function (...args) { pushes += args.length; return original.apply(this, args); };
      let result;
      try { result = source.substitute(row, args, 'reader'); } finally { Array.prototype.push = original; }
      expect(pushes).toBeLessThanOrEqual(5);
      expect(result.samples).toHaveLength(history);
      expect(result.dropped).toBe(history ? 216000 - history : 0);
      expect(result.sequence).toBe(216000);
    }
    clock.now = 0; const source = samples(), args = { frequency: 10, history: 3 };
    source.answered(row, 'reader', source.substitute(row, args, 'reader'));
    clock.now = 2000; source.visibility(true);
    clock.now = 5000; source.visibility(false);
    clock.now = 10000;
    let result = source.substitute(row, args, 'reader');
    expect(result.samples.map(x => x.timestamp)).toEqual([9800, 9900, 10000]);
    expect([result.dropped, result.interrupted]).toEqual([67, true]);
    clock.now = 9900; source.visibility(true); clock.now = 10000;
    result = source.substitute(row, args, 'reader');
    expect(result.samples.map(x => x.timestamp)).toEqual([9600, 9700, 9800]);
    expect(result.dropped).toBe(65);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('live tool deadlines refresh time and arm a wake without unrelated commits; the agent clock stays explicit', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-call-clock-')), old = globalThis.exact;
  cpSync(resolve(ROOT, 'host/web-js/calls.js'), resolve(dir, 'calls.js'));
  // Keep the production dispatcher: its callback result decides whether an advance continues.
  const runtime = readFileSync(resolve(ROOT, 'host/web-js/clock.js'), 'utf8');
  const advance = runtime.slice(runtime.indexOf('export function advance('), runtime.indexOf('let driving ='));
  expect(advance).toContain('function fire(');
  writeFileSync(resolve(dir, 'rt.js'), `export const clock={now:0,agent:false,timers:[]},wall={now:0},data={},After=[],Clocked=[],Mutations=[],Origin={now:()=>'app'},journal=[],say=x=>journal.push(x),order=()=>0,drive=()=>{};
const Link={},Poisoned=false;let Timing=false,Instant=null,start=0;
export const time=()=>{if(!clock.agent)clock.now=Math.max(clock.now,wall.now)};\n${advance}`);
  writeFileSync(resolve(dir, 'held.js'), 'export const Counting=new Set(),heldTickets=()=>[];');
  try {
    globalThis.exact = {};
    const { tools } = await import(pathToFileURL(resolve(dir, 'calls.js')).href);
    const { clock, wall, After, advance } = await import(pathToFileURL(resolve(dir, 'rt.js')).href);
    const { Counting } = await import(pathToFileURL(resolve(dir, 'held.js')).href);
    tools({ run: [() => () => Counting.add({}), {}, 'reads', [false, true, true], 'run'] });
    const calls = globalThis.exact.calls;
    const first = calls.call({ tool: 'run' }); await Promise.resolve();
    expect(clock.timers.map(t => t.due)).toEqual([10000]);
    wall.now = 10001; clock.timers.shift().action();
    expect(calls.called(first.call).outcome).toBe('timeout');
    let waking = true;
    globalThis.exact.extensions = { wake: () => waking ? {} : null };
    const queued = calls.call({ tool: 'run' }); await Promise.resolve();
    expect(clock.timers.map(t => t.due)).toEqual([15001]);
    wall.now = 15002; clock.timers.shift().action();
    expect(calls.called(queued.call)).toMatchObject({ outcome: 'refused', reason: 'busy' });
    waking = false; Counting.clear(); clock.agent = true;
    const driven = calls.call({ tool: 'run' }); await Promise.resolve();
    wall.now = 1000000;
    for (const f of After) f(); await Promise.resolve();
    expect(calls.called(driven.call).state).toBe('running');
    expect(clock.now).toBe(15002);
    let later = false;
    clock.timers.push({ due: 30000, once: true, data: true, action: () => { later = true; return true; } });
    expect(advance(35000, false)).toBe(false);
    expect(clock.now).toBe(35000);
    expect(later).toBe(true);
    expect(calls.called(driven.call).outcome).toBe('timeout');
  } finally { globalThis.exact = old; rmSync(dir, { recursive: true, force: true }); }
});

test('a queued tool call waits at most five seconds behind a running call', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-call-wait-')), old = globalThis.exact;
  cpSync(resolve(ROOT, 'host/web-js/calls.js'), resolve(dir, 'calls.js'));
  writeFileSync(resolve(dir, 'rt.js'), `export const clock={now:0,agent:true,timers:[]},data={},After=[],Clocked=[],Mutations=[],Origin={now:()=>'app'},say=()=>{},order=()=>0,drive=()=>{},time=()=>{};`);
  writeFileSync(resolve(dir, 'held.js'), 'export const Counting=new Set(),heldTickets=()=>[];');
  try {
    globalThis.exact = {};
    const { tools } = await import(pathToFileURL(resolve(dir, 'calls.js')).href);
    const { clock } = await import(pathToFileURL(resolve(dir, 'rt.js')).href);
    const { Counting } = await import(pathToFileURL(resolve(dir, 'held.js')).href);
    let runs = 0;
    tools({ run: [() => () => { runs++; Counting.add({}); }, {}, 'reads', [false, true, true], 'run'] });
    const calls = globalThis.exact.calls;
    const fire = async now => { clock.now = now; clock.timers.splice(0).forEach(t => t.action()); await Promise.resolve(); };
    const release = async () => { Counting.clear(); clock.timers.splice(0).forEach(t => t.action()); await Promise.resolve(); };
    // Behind a call that settles after the wait: refused at 5 s, never run.
    const first = calls.call({ tool: 'run' }), second = calls.call({ tool: 'run' }), third = calls.call({ tool: 'run' });
    await Promise.resolve();
    expect(clock.timers.map(t => t.due)).toEqual([5000]);
    await fire(5000);
    expect(calls.called(second.call)).toMatchObject({ outcome: 'refused', reason: 'busy', certainty: 'none' });
    expect(calls.called(third.call)).toMatchObject({ outcome: 'refused', reason: 'busy' });
    expect(calls.called(first.call).state).toBe('running');
    expect(clock.timers.map(t => t.due)).toEqual([10000]);
    // Behind a call that times out within its wait: dispatched then.
    clock.now = 7000;
    const fourth = calls.call({ tool: 'run' }); await Promise.resolve();
    expect(clock.timers.map(t => t.due)).toEqual([10000]);
    await fire(10000);
    expect(calls.called(first.call).outcome).toBe('timeout');
    expect(calls.called(fourth.call).state).toBe('running');
    // Behind a call that settles within the wait: dispatched when it ends.
    const fifth = calls.call({ tool: 'run' }); await Promise.resolve();
    clock.now = 14000; await release();
    expect(calls.called(fourth.call).outcome).toBe('ok');
    expect(calls.called(fifth.call).state).toBe('running');
    expect(runs).toBe(3);
    // Settling after the wait ran out does not dispatch the call it held.
    const sixth = calls.call({ tool: 'run' }); await Promise.resolve();
    expect(clock.timers.map(t => t.due)).toEqual([19000]);
    clock.now = 20000; await release();
    expect(calls.called(fifth.call).outcome).toBe('ok');
    expect(calls.called(sixth.call)).toMatchObject({ outcome: 'refused', reason: 'busy' });
    expect(runs).toBe(3);
  } finally { globalThis.exact = old; rmSync(dir, { recursive: true, force: true }); }
});

test('a live wake\'s deadline timer moves with a budget an event raises after delivery', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-wake-budget-'));
  cpSync(resolve(ROOT, 'host/web-js/wakes.js'), resolve(dir, 'wakes.js'));
  writeFileSync(resolve(dir, 'rt.js'), `export const clock={now:0,agent:false},wall={now:0},inflight={n:0},data={},After=[],Clocked=[],Mutations=[],drive=()=>{};
export const time=()=>{clock.now=Math.max(clock.now,wall.now)};`);
  writeFileSync(resolve(dir, 'held.js'), 'export const Counting=new Set(),heldTickets=()=>[];');
  const set = globalThis.setTimeout, clear = globalThis.clearTimeout, timers = new Map();
  let ids = 0;
  try {
    const { wakes } = await import(pathToFileURL(resolve(dir, 'wakes.js')).href);
    const { wall } = await import(pathToFileURL(resolve(dir, 'rt.js')).href);
    const { Counting } = await import(pathToFileURL(resolve(dir, 'held.js')).href);
    globalThis.setTimeout = (f, ms) => { timers.set(++ids, { f, due: wall.now + ms }); return ids; };
    globalThis.clearTimeout = id => timers.delete(id);
    // The wall timers due by `now`, as the browser fires them.
    const at = async now => {
      wall.now = now;
      for (const [id, t] of [...timers].filter(([, t]) => t.due <= now)) { timers.delete(id); t.f(); }
      await Promise.resolve();
    };
    const pull = () => () => Counting.add({});
    const wk = wakes({
      events: { 'sync.short': [pull, 'wake', 16, [], 'short', 1000], 'sync.long': [pull, 'wake', 16, [], 'long', 5000] },
      say: () => {}, value: v => v, run: f => f(), toEpoch: ms => ms, onLine: () => true, lists: new Map(), changed() {},
    });
    const first = wk.raise('sync.short', [], { id: 'task' });
    await Promise.resolve();
    expect(wk.state().deadline).toBe(1000);
    await at(500);
    const second = wk.raise('sync.long', [], { id: 'task' });
    await Promise.resolve();
    expect(wk.state().deadline).toBe(5000);
    await at(1000);
    expect(wk.state().invocations.map(i => i.state)).toEqual(['running', 'queued']);
    await at(5000);
    expect(await first).toBe('expired');
    expect(await second).toBe('expired');
  } finally {
    globalThis.setTimeout = set; globalThis.clearTimeout = clear;
    rmSync(dir, { recursive: true, force: true });
  }
});

test.each([false, true])('a wake after an idle spell has its full budget (agent: %s)', async agent => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/host/web-js/wakes.js') return new Response(Bun.file(resolve(ROOT, 'host/web-js/wakes.js')));
    const local = resolve(ROOT, '.' + path);
    const file = existsSync(local) ? local : resolve(ROOT, 'host/web', path.split('/').at(-1));
    return existsSync(file) ? new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('Not found', { status: 404 });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.clock.install({ time: 1000 });
    await page.clock.pauseAt(1100);
    await page.goto(server.url.href);
    // Mount after navigation's zero timestamp, as on a page with a running clock.
    await page.clock.runFor(1);
    await page.evaluate(async agent => {
      const rt = await import('/host/web-js/rt.js');
      const { wakes } = await import('/host/web-js/wakes.js');
      let reply, done = 0, pulled;
      rt.data.answer = () => ({ promise: new Promise(resolve => { reply = resolve; }) });
      rt.mount(() => {
        pulled = rt.mut('pulled', rt.sig(null), [], 's');
        pulled.then = () => { done++; };
      });
      rt.clock.agent = agent;
      const wk = wakes({
        events: { 'background.sync': [() => () => rt.M(pulled, 'pull', []), 'wake', 16, [], 'heard', 30000] },
        say: rt.say, value: v => v, run: rt.commit, toEpoch: ms => ms,
        onLine: () => true, lists: new Map(), changed() {},
      });
      window.fixture = { rt, wk, done: () => done, reply: () => reply('ok'),
        raise: () => { fixture.outcome = wk.raise('background.sync', []); } };
    }, agent);

    // Move only the browser's time: the idle page has made no commit to sample it.
    await page.clock.runFor(35000);
    expect(await page.evaluate(() => fixture.rt.clock.now)).toBe(0);
    await page.evaluate(() => fixture.raise());
    await page.clock.runFor(500);
    if (agent) await page.evaluate(() => fixture.rt.advance(500, false));
    await page.evaluate(() => fixture.reply());
    if (agent) await page.evaluate(() => fixture.rt.advance(500, false));
    else await page.clock.runFor(1);
    const finished = await page.evaluate(async () => ({
      outcome: await fixture.outcome, done: fixture.done(), state: fixture.wk.state(),
    }));
    expect(finished).toMatchObject({ outcome: 'completed', done: 1,
      state: { started: agent ? 0 : 35000, deadline: agent ? 30000 : 65000 } });

    // Unfinished work still expires at the budget, on the clock that owns the page.
    await page.evaluate(() => fixture.raise());
    const deadline = await page.evaluate(() => fixture.wk.state().deadline);
    if (agent) await page.evaluate(to => fixture.rt.advance(to, false), deadline - 1);
    else await page.clock.fastForward(29999);
    expect(await page.evaluate(() => fixture.wk.state().invocations[0].state)).toBe('running');
    if (agent) await page.evaluate(to => fixture.rt.advance(to, false), deadline);
    else await page.clock.runFor(1);
    expect(await page.evaluate(() => fixture.outcome)).toBe('expired');
  } finally { await browser?.close(); server.stop(true); }
}, 30000);
