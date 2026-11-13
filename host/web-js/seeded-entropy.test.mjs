import { test, expect, spyOn } from 'bun:test';
import { cpSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { open } from '../../scripts/agent.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';

test('module-body crypto refuses direct calls, helpers and async continuations before drawing browser entropy', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/entropy-module-test-'));
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Entropy module', app: { id: 'test.entropy-module', name: 'Entropy module' } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  resource report = report() as shape string else empty()
  view
    text report testId="report"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = '';
const refused: string[] = [];
const view = new Uint8Array([42]);
try { crypto.randomUUID(); refused.push('UNGUARDED'); }
catch (error) { refused.push(String(error)); }
try { crypto.getRandomValues(view); refused.push('UNGUARDED'); }
catch (error) { refused.push(String(error)); }
function helper(api: string) {
  return api === 'randomUUID' ? crypto.randomUUID() : crypto.getRandomValues(view);
}
for (const api of ['randomUUID', 'getRandomValues']) {
  try { helper(api); refused.push('UNGUARDED'); }
  catch (error) { refused.push(String(error)); }
}
const id = (async () => { await Promise.resolve(); return crypto.randomUUID(); })();
const reaction = Promise.resolve().then(() => crypto.randomUUID());
const pending = [id, reaction].map(work => work.then(() => 'UNGUARDED', error => String(error)));
export async function answer(): Promise<string> {
  const deferred = await Promise.all(pending);
  await Promise.resolve();
  return JSON.stringify({ refused: [...refused, ...deferred], untouched: view[0],
    id: crypto.randomUUID(), bytes: Array.from(crypto.getRandomValues(new Uint8Array(4))) });
}
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'entropy-module', '--out', resolve(dir, 'dist'), '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.addInitScript(() => {
      globalThis.entropyCalls = { randomUUID: 0, getRandomValues: 0 };
      for (const api of Object.keys(globalThis.entropyCalls)) {
        const draw = crypto[api].bind(crypto);
        crypto[api] = (...args) => { globalThis.entropyCalls[api]++; return draw(...args); };
      }
    });
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.querySelector('[data-testid=report]')?.textContent.startsWith('{'));
    const report = JSON.parse(await page.getByTestId('report').textContent());
    const refusal = (api, phase) => `Error: crypto.${api}() is unavailable ${phase}; call it inside an answer`;
    expect(report.refused).toEqual([
      ...Array.from({ length: 2 }, () => ['randomUUID', 'getRandomValues'].map(api => refusal(api, 'during module initialization'))).flat(),
      ...Array.from({ length: 2 }, () => refusal('randomUUID', 'outside an answer')),
    ]);
    expect(report.untouched).toBe(42);
    expect(report.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(report.bytes).toHaveLength(4);
    // Only the answer's two draws reach the browser, after its awaits.
    expect(await page.evaluate(() => globalThis.entropyCalls)).toEqual({ randomUUID: 1, getRandomValues: 1 });
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

test('a crypto draw requires a pending turn and refuses before consuming entropy', async () => {
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/entropy-lifetime-test-'));
  cpSync(resolve(root, 'host/web-js/ts-fetch.js'), resolve(dir, 'ts-fetch.js'));
  writeFileSync(resolve(dir, 'admission-data.js'), 'export const tsGrantSet = {};');
  writeFileSync(resolve(dir, 'admission.js'), `export const overlaying = { on: false };
export const inOverlay = api => new Error(api + ' in an overlay');
export class FetchError extends Error {}
export const socketProtocols = null, fetchWith = null, ownWithout = null, redirectOf = null, signalDeadline = {};
`);
  const values = spyOn(globalThis.crypto, 'getRandomValues'), uuid = spyOn(globalThis.crypto, 'randomUUID');
  const ordinary = Object.getOwnPropertyDescriptor(globalThis, '__exact_ordinary');
  try {
    const { crypto, answering, ordinary, Promise: LocalPromise, Response: LocalResponse } = await import(pathToFileURL(resolve(dir, 'ts-fetch.js')).href);
    const draws = [], journal = [];
    answering.entropy = n => { draws.push(n); return new Uint8Array(n); };
    const refuse = phase => {
      const before = { draws: draws.slice(), journal: journal.slice(), values: values.mock.calls.length, uuid: uuid.mock.calls.length };
      const view = new Uint8Array([42]);
      for (const [api, draw] of [
        ['getRandomValues', () => crypto.getRandomValues(view)],
        ['randomUUID', () => crypto.randomUUID()],
      ]) expect(draw).toThrow(`crypto.${api}() is unavailable ${phase}; call it inside an answer`);
      expect(view[0]).toBe(42);
      expect(values).toHaveBeenCalledTimes(before.values);
      expect(uuid).toHaveBeenCalledTimes(before.uuid);
      expect(draws).toEqual(before.draws);
      expect(journal).toEqual(before.journal);
    };
    refuse('during module initialization');
    // ts-data.js sets the page once the app's module has run.
    answering.page = { journal };
    refuse('outside an answer');
    answering.call = { settled: true };
    refuse('outside an answer');
    answering.call = { ticket: { owner: { ticket: null } } };
    expect(crypto.getRandomValues(new Uint8Array(4))).toHaveLength(4);
    expect(crypto.randomUUID()).toMatch(/^[0-9a-f-]{36}$/);
    expect(draws).toEqual([4, 16]);
    const pair = () => [crypto.randomUUID(), crypto.getRandomValues(new Uint8Array(4)).length];
    const pending = LocalPromise.resolve().then(() => refuse('outside an answer'));
    answering.call = null;
    await pending;
    answering.call = {};
    const cached = LocalPromise.resolve();
    answering.call.settled = true;
    answering.call = {};
    const reused = cached.then(pair);
    answering.call = null;
    expect((await reused)[1]).toBe(4);
    const call = answering.call = {};
    const ended = LocalPromise.resolve().then(() => refuse('outside an answer'));
    answering.call = null; call.settled = true;
    await ended;
    // Storage not awaited by an answer resumes as background work after its value is given.
    const saving = Promise.withResolvers(), owner = answering.call = {};
    const saved = ordinary(() => saving.promise, true).then(pair).then(pair);
    answering.call = null; owner.settled = true; saving.resolve();
    expect((await saved)[1]).toBe(4);
    expect(answering.call).toBeNull();
    // So does what follows storage through a combinator or a rejection's handler.
    for (const combine of [w => LocalPromise.all([w]), w => LocalPromise.allSettled([w]), w => LocalPromise.race([w]), w => LocalPromise.any([w]),
      w => new LocalPromise(resolve => w.then(resolve)), w => w.catch(() => {}).then(() => {})]) {
      const saving = Promise.withResolvers(), owner = answering.call = {};
      const saved = combine(ordinary(() => saving.promise, true)).then(pair);
      answering.call = null; owner.settled = true; saving.resolve();
      expect((await saved)[1]).toBe(4);
    }
    // A combinator over an ended answer's own work is still refused.
    const late = Promise.withResolvers(), lateOwner = answering.call = {};
    const lateAll = LocalPromise.all([ordinary(() => late.promise)]).then(() => refuse('outside an answer'));
    answering.call = null; lateOwner.settled = true; late.resolve();
    await lateAll;
    // A body the module made is read in its answer's turn, as a fetch's is.
    for (const read of [() => new LocalResponse('{}').json(), () => LocalResponse.json({}).clone().text(), () => new LocalResponse('x').arrayBuffer()]) {
      const reader = answering.call = {};
      const body = read().then(pair);
      answering.call = null;
      expect((await body)[1]).toBe(4);
      reader.settled = true;
    }
    expect(new LocalResponse('') instanceof Response).toBe(true);
    expect(() => new LocalPromise(42)).toThrow(TypeError);
    answering.call = {};
    answering.mapperCall = { settled: true };
    refuse('outside an answer');
    answering.mapperCall = null;
    const keys = spyOn(globalThis.crypto.subtle, 'generateKey'), signs = spyOn(globalThis.crypto.subtle, 'sign');
    try {
      answering.call = null; answering.page = null;
      await expect(crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign']))
        .rejects.toThrow('crypto.subtle.generateKey() is unavailable during module initialization');
      expect(keys).not.toHaveBeenCalled();
      await expect(crypto.subtle.sign({ name: 'ECDSA', hash: 'SHA-256' }, null, new Uint8Array()))
        .rejects.toThrow('crypto.subtle.sign() is unavailable during module initialization');
      expect(signs).not.toHaveBeenCalled();
      expect((await crypto.subtle.digest('SHA-256', new Uint8Array())).byteLength).toBe(32);
    } finally { keys.mockRestore(); signs.mockRestore(); }
  } finally {
    if (ordinary) Object.defineProperty(globalThis, '__exact_ordinary', ordinary);
    else delete globalThis.__exact_ordinary;
    values.mockRestore(); uuid.mockRestore(); rmSync(dir, { recursive: true, force: true });
  }
});

// LLP 1069.005 D2b on the JS target: under the driver a TypeScript source draws the launch seed's stream, from its
// first answer at boot. Seed 1's first id is the one a native module draws (js/tests/it/entropy.rs).
test('under the driver a source draws the seeded stream: one seed, one id at every launch, and logs say so once', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/seeded-entropy-test-'));
  let server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Seeded', app: { id: 'test.seeded', name: 'Seeded' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component Seeded
  state n = 0
  state mode = 0
  resource id = id(n) as shape string else empty()
  resource bytes = bytes(n) as shape string else empty()
  resource initialized = initialized() as shape string else empty()
  resource turns = turns(mode) as shape string else empty()
  resource mapped = mapped(mode) as shape string else empty()
  resource mappedAsync = mappedAsync(mode) as shape string else empty()
  resource background = background(mode) as shape string else empty()
  resource letGo = letGo(mode) as shape string else empty()
  resource continuations = continuations(mode) as shape string else empty()
  resource letGoPlain = letGoPlain(mode) as shape string else empty()
  resource combined = combined(mode) as shape string else empty()
  action again
    n = n + 1
  action exercise
    mode = mode + 1
  view
    column
      text id testId="id"
      text bytes testId="bytes"
      button press=again testId="again"
        text "Again"
      button press=exercise testId="exercise"
        text "Exercise"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.seeded';
export const grants = 'net.fetch ORIGIN\\nfs.write app:/data\\nsqlite.open app:/data/entropy.db\\n';
const initialized: string[] = [];
for (const draw of [() => crypto.randomUUID(), () => crypto.getRandomValues(new Uint8Array(4))]) {
  try { draw(); initialized.push('UNGUARDED'); }
  catch (e) { initialized.push(e instanceof Error ? e.message : String(e)); }
}
const pair = () => [crypto.randomUUID(), Array.from(crypto.getRandomValues(new Uint8Array(4))).join(',')];
Promise.resolve().then(() => {
  for (const draw of [() => crypto.randomUUID(), () => crypto.getRandomValues(new Uint8Array(4))]) {
    try { draw(); initialized.push('UNGUARDED'); }
    catch (e) { initialized.push(e instanceof Error ? e.message : String(e)); }
  }
});
let saved: Promise<string>, left: Promise<string>, plain: Promise<string>, cached: Promise<number>, combined: Promise<string>;
const safePair = () => JSON.stringify([
  () => crypto.randomUUID(), () => Array.from(crypto.getRandomValues(new Uint8Array(4))).join(','),
].map(draw => { try { return draw(); } catch (e) { return e instanceof Error ? e.message : String(e); } }));
async function helper() { await Promise.resolve(); }
export async function answer(name: string, [mode]: number[], _store: any, storage: any): Promise<string> {
  if (name === 'continuations') {
    if (mode === 8) return helper().then(safePair);
    if (mode === 9) return fetch('ORIGIN/value').then(response => response.json().then(safePair));
    if (mode === 10) return scheduler.yield().then(safePair);
    if (mode === 11) return new Promise(resolve => queueMicrotask(() => resolve(safePair())));
    if (mode === 12 || mode === 13) { cached ??= Promise.resolve(1); return cached.then(safePair); }
    if (mode === 16) return new Response('{}').json().then(safePair);
    if (mode === 17) return Response.json({}).clone().text().then(safePair);
    return 'idle';
  }
  if (name === 'letGoPlain') {
    if (mode === 14) { plain = fetch('ORIGIN/held', { method: 'POST' }).then(safePair); return plain; }
    if (mode === 15) { await fetch('ORIGIN/release'); return plain; }
    return 'idle';
  }
  if (name === 'combined') {
    if (mode === 18) { combined = Promise.all([storage.fs.writeFile('app:/data/all', new Uint8Array([1]))]).then(safePair); return 'started'; }
    if (mode === 20) {
      combined = storage.sqlite.open('app:/data/entropy.db').then((db: any) => db.query('SELECT 1 AS one').then(safePair).finally(() => db.close()));
      return 'started';
    }
    return mode === 19 || mode === 21 ? combined : 'idle';
  }
  if (name === 'background') {
    if (mode === 4) { saved = storage.fs.writeFile('app:/data/entropy', new Uint8Array([1])).then(() => JSON.stringify(pair())); return 'started'; }
    return mode === 5 ? saved : 'idle';
  }
  if (name === 'letGo') {
    if (mode === 6) {
      left = (async () => { await fetch('ORIGIN/held', { method: 'POST' }); return JSON.stringify(pair()); })();
      return left;
    }
    if (mode === 7) { await fetch('ORIGIN/release'); return left; }
    return 'idle';
  }
  if (['turns', 'mapped', 'mappedAsync'].includes(name) && mode !== ({ turns: 1, mapped: 2, mappedAsync: 3 } as any)[name]) return 'idle';
  if (name === 'turns') return fetch('ORIGIN/value').then(() => Promise.resolve().then(pair)).then(JSON.stringify);
  if (name === 'mapped') return fetch('ORIGIN/events', { exactStream: () => JSON.stringify(pair()) } as any);
  if (name === 'mappedAsync') return fetch('ORIGIN/events', { exactStream: async () => {
    const before = pair();
    await Promise.resolve();
    return JSON.stringify([before, pair()]);
  } } as any);
  if (name === 'initialized') return JSON.stringify(initialized);
  await Promise.resolve();
  if (name === 'id') return crypto.randomUUID();
  if (name === 'bytes') {
    await new Promise(resolve => setTimeout(resolve, 1));
    return Array.from(crypto.getRandomValues(new Uint8Array(4)), b => b.toString(16).padStart(2, '0')).join('');
  }
  throw new Error('no source ' + name);
}
`);
    let release, heldReady;
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      if (path === '/held') return new Promise(resolve => { release = () => resolve(new Response('ok')); heldReady(); });
      if (path === '/release') { release(); return new Response('released'); }
      if (path === '/value') return Response.json({ ok: true });
      if (path === '/events') return new Response('data: event\n\n', { headers: { 'content-type': 'text/event-stream' } });
      const file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    writeFileSync(resolve(dir, 'app.ts'), (await Bun.file(resolve(dir, 'app.ts')).text()).replaceAll('ORIGIN', server.url.origin));
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'seeded', '--out', resolve(dir, 'dist'), '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const launch = async seed => {
      let reached = Promise.withResolvers(); heldReady = reached.resolve;
      const browser = await open({ host: 'web', url: server.url.href, webDist: resolve(dir, 'dist'), app: dir, storage: 'entropy', seed });
      try {
        await browser.clock('settle');
        const first = (await browser.state()).resources;
        await browser.tap('again');
        await browser.clock('settle');
        const second = (await browser.state()).resources;
        const extra = [];
        for (let i = 0; i < 21; i++) {
          if (i === 13) { reached = Promise.withResolvers(); heldReady = reached.resolve; }
          await browser.tap('exercise');
          if (i === 5 || i === 13) await reached.promise;
          else await browser.clock('settle');
          extra.push((await browser.state()).resources);
        }
        const lines = (await browser.logs()).lines;
        const said = lines.filter(line => line.includes("crypto: the driver seeds this launch's randomness"));
        return { first, second, extra, said };
      } finally { await browser.close(); }
    };
    const a = await launch(), b = await launch(), other = await launch(7);
    // A reaction the module queued as it loaded runs after it has loaded, as natively (js/tests/it/entropy.rs).
    expect(JSON.parse(a.first.initialized)).toEqual(['during module initialization', 'outside an answer'].flatMap(phase =>
      ['randomUUID', 'getRandomValues'].map(api => `crypto.${api}() is unavailable ${phase}; call it inside an answer`)));
    expect(a.first.id).toBe('deb201fb-035c-4c32-bbbf-3da08991a485');
    expect(a.first.bytes).toMatch(/^[0-9a-f]{8}$/);
    const checkPair = p => { expect(p[0]).toMatch(/^[0-9a-f-]{36}$/); expect(p[1].split(',')).toHaveLength(4); };
    checkPair(JSON.parse(a.extra[0].turns));
    checkPair(JSON.parse(a.extra[1].mapped));
    JSON.parse(a.extra[2].mappedAsync).forEach(checkPair);
    checkPair(JSON.parse(a.extra[4].background));
    checkPair(JSON.parse(a.extra[6].letGo));
    expect(a.extra.slice(7, 13).map(row => JSON.parse(row.continuations))).toEqual(
      Array.from({ length: 6 }, () => [expect.stringMatching(/^[0-9a-f-]{36}$/), expect.stringMatching(/^\d+(,\d+){3}$/)]));
    // A body the module made, a combinator over background storage and a database's step, as natively
    // (js/tests/it/entropy.rs, js/tests/it/background.rs).
    expect([a.extra[15].continuations, a.extra[16].continuations, a.extra[18].combined, a.extra[20].combined].map(row => JSON.parse(row))).toEqual(
      Array.from({ length: 4 }, () => [expect.stringMatching(/^[0-9a-f-]{36}$/), expect.stringMatching(/^\d+(,\d+){3}$/)]));
    expect(JSON.parse(a.extra[14].letGoPlain)).toEqual(['randomUUID', 'getRandomValues'].map(api =>
      `crypto.${api}() is unavailable outside an answer; call it inside an answer`));
    expect(b.first).toEqual(a.first);
    expect(b.second).toEqual(a.second);
    expect(b.extra.filter((_, i) => i !== 5 && i !== 13)).toEqual(a.extra.filter((_, i) => i !== 5 && i !== 13));
    // The stream goes on: a second ask draws the next bytes.
    expect(a.second.id).not.toBe(a.first.id);
    expect(a.second.bytes).not.toBe(a.first.bytes);
    expect(other.first.id).not.toBe(a.first.id);
    expect(other.first.bytes).not.toBe(a.first.bytes);
    expect(a.said).toEqual([expect.stringContaining('(seed 1)')]);
    expect(other.said).toEqual([expect.stringContaining('(seed 7)')]);
  } finally {
    server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
