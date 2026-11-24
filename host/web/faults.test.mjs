// LLP 1103 on the web: the page's fault table, and the three places that
// consult it — the wasm host's `request`, the JS target's `fetchWith`, and
// `fetchEarly`, which starts no GET a fault will fail.
import { afterEach, expect, test } from 'bun:test';
import { readFileSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { checkpointFaults, drivenCheckpoint, faultSpecOf, faultMatches, faultOp, faultsJson, parseFaults, setFaultLog, takeFault } from './faults.js';
import { request } from './http-body.js';
import { fetchWith } from '../web-js/admission.js';
import { rt } from '../web-js/rt-link.js';
import { runtimeSource } from './tests/stand-ins.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const target = resolve(process.env.CARGO_TARGET_DIR || resolve(ROOT, 'target'), `debug/exact-web-js${process.platform === 'win32' ? '.exe' : ''}`);
const built = spawnSync('cargo', ['build', '-q', '-p', 'exact-web-js'], { cwd: ROOT, encoding: 'utf8' });
if (built.status !== 0) throw new Error(built.stderr || 'grant normalizer build failed');
function normalized(spec) {
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-grants-'));
  const input = resolve(scratch, 'grants.txt');
  writeFileSync(input, spec);
  const result = spawnSync(target, ['normalize-grants', input], { cwd: ROOT, encoding: 'utf8' });
  rmSync(scratch, { recursive: true });
  if (result.status !== 0) throw new Error(result.stderr || 'grant normalizer failed');
  return JSON.parse(result.stdout);
}
afterEach(() => { delete globalThis.__exactFaults; });

test('HTTP fixtures answer both web executors without a request and survive reload', async () => {
  let hits = 0;
  const server = Bun.serve({ port: 0, fetch() { hits++; return new Response('live'); } });
  const prefix = server.url.href, set = normalized(`net.fetch ${server.url.origin}`);
  const response = { status: 201, headers: { 'content-type': 'application/json', 'x-fixture': 'yes' }, body: '{"text":"Stubbed greeting"}' };
  try {
    expect(faultOp({ answer: prefix, response }).error).toBeUndefined();
    const js = await fetchWith(set, prefix);
    expect([js.status, js.headers.get('x-fixture'), await js.json()]).toEqual([201, 'yes', { text: 'Stubbed greeting' }]);
    const wasm = await request({ method: 'GET', url: prefix, headers: [] }, { grantSet: set, controllers: new Set() });
    expect([wasm.kind, wasm.status, new TextDecoder().decode(wasm.body)]).toEqual([0, 201, response.body]);
    expect(hits).toBe(0);
    const { faultSpecOf, faultSpec } = await import('../../scripts/agent-launch.mjs');
    const spec = faultSpec(faultSpecOf(faultsJson()));
    globalThis.__exactFaults.entries = parseFaults(spec);
    expect((await fetchWith(set, prefix)).status).toBe(201);
    faultOp({ fail: prefix });
    expect((await fetchWith(set, prefix).catch(e => e)).kind).toBe('Network');
    faultOp({ answer: prefix, response });
    faultOp({ pass: prefix });
    expect(await (await fetchWith(set, prefix)).text()).toBe('live');
    expect(hits).toBe(1);
  } finally { server.stop(true); }
});

test('delayed fixtures preserve other I/O and stop on abort or the earlier timeout', async () => {
  const url = 'https://goals.example.test/goal', set = normalized('net.fetch https://goals.example.test');
  const response = { status: 200, headers: {}, body: 'old', afterMs: 200 };
  expect(faultOp({ answer: url, response }).error).toBeUndefined();
  const previous = rt.clock;
  const clock = rt.clock = { now: 40, agent: true, timers: [] };
  try {
    const wasm = await request({ method: 'GET', url, headers: [] }, { grantSet: set, controllers: new Set() });
    expect([wasm.afterMs, new TextDecoder().decode(wasm.body)]).toEqual([200, 'old']);
    for (const kind of ['plain', 'held']) {
      const call = { plain: 1, [kind]: kind === 'plain' ? 2 : 1 };
      const abort = new AbortController();
      let waiting;
      const parked = new Promise(resolve => { waiting = resolve; });
      call.recount = () => { if (call.timers) waiting(); };
      const result = fetchWith(set, url, { exactTimeout: 90, signal: abort.signal },
        null, undefined, undefined, 'app', false, undefined, { call, kind }).catch(e => e);
      await parked;
      expect(call.plain).toBe(1);
      const [entry] = clock.timers;
      expect(entry.due).toBe(130);
      if (kind === 'held') {
        abort.abort('cancelled');
        expect(await result).toBe('cancelled');
      } else {
        clock.now = entry.due;
        entry.action();
        expect((await result).kind).toBe('Timeout');
        clock.now = 40;
      }
      expect(clock.timers).toHaveLength(0);
      expect(call.timers).toBe(0);
    }
  } finally { rt.clock = previous; }
});

test('a request-time render leaves delayed fixture delivery for the driven page', async () => {
  const url = 'https://goals.example.test/goal', set = normalized('net.fetch https://goals.example.test');
  faultOp({ answer: url, response: { status: 200, headers: {}, body: 'old', afterMs: 100 } });
  globalThis.__exactRender = true;
  try {
    expect(await fetchWith(set, url).catch(e => e)).toMatchObject({renderDeferred: true});
    expect(faultsJson()[0].hits).toBe(0);
  } finally { delete globalThis.__exactRender; }
});

test('one URL can answer GET and PUT independently on both web executors after reload', async () => {
  const url = 'https://goals.example.test/goal', set = normalized('net.fetch https://goals.example.test');
  for (const [method, body] of [['GET', 'old'], ['PUT', 'saved'], [undefined, 'fallback']]) {
    expect(faultOp({ answer: url, method, response: {status: 200, headers: {}, body} }).error).toBeUndefined();
  }
  const {faultSpec} = await import('../../scripts/agent-launch.mjs');
  globalThis.__exactFaults.entries = parseFaults(faultSpec(faultSpecOf(faultsJson())));
  for (const [method, body] of [['GET', 'old'], ['PUT', 'saved'], ['POST', 'fallback']]) {
    expect(await (await fetchWith(set, new Request(url, {method}))).text()).toBe(body);
    const wasm = await request({ method, url, headers: [] }, {grantSet: set, controllers: new Set()});
    expect([wasm.status, new TextDecoder().decode(wasm.body)]).toEqual([200, body]);
  }
  expect(faultsJson().map(f => f.hits)).toEqual([2, 2, 2]);
  expect(faultOp({pass: url}).faults.every(f => !f.armed)).toBe(true);
});

test('fixture response clones keep the URL used by an app to resolve relative links', async () => {
  const url = 'https://api.test/items/first', set = normalized('net.fetch https://api.test');
  faultOp({answer: url, response: {status: 200, headers: {'content-type': 'application/json'}, body: '{"next":"second"}'}});
  const response = await fetchWith(set, url);
  const originalClone = response.clone.bind(response);
  response.clone = () => originalClone();
  const clone = response.clone().clone();
  expect(new URL((await clone.json()).next, clone.url).href).toBe('https://api.test/items/second');
  expect(response.bodyUsed).toBe(false);
  expect(clone.type).toBe('cors');
  expect(() => response.headers.set('x-added', 'changed')).toThrow(TypeError);
  expect(await response.json()).toEqual({next: 'second'});
});

test('invalid response fixtures leave the existing entry intact, and HEAD has no body', async () => {
  const prefix = 'https://api.test/', response = {status: 200, headers: {'content-type': 'application/json'}, body: '{"text":"Hello 🌍"}'};
  faultOp({answer: prefix, response});
  for (const invalid of [{...response, status: 302}, {...response, status: 204}, {...response, headers: {'x': 'one\r\ntwo'}}, {...response, body: 'x'.repeat(1024 * 1024 + 1)}]) {
    expect(faultOp({answer: prefix, response: invalid}).malformed).toBe(true);
  }
  const set = normalized('net.fetch https://api.test');
  expect(await (await fetchWith(set, prefix)).json()).toEqual({text: 'Hello 🌍'});
  expect(await (await fetchWith(set, prefix, {method: 'HEAD'})).text()).toBe('');
  const before = faultsJson();
  expect((await fetchWith(normalized(''), prefix).catch(e => e)).kind).toBe('Refused');
  expect(faultsJson()).toEqual(before);
  globalThis.exact ??= {};
  const {fetchEarly} = await import('./module-glue.js');
  expect(fetchEarly({method: 'GET', url: prefix, headers: []}, set)).toBeNull();
});

test('response header limits refuse arming without changing the table', () => {
  const prefix = 'https://api.test/';
  const response = {status: 200, headers: Object.fromEntries(Array.from({length: 1024}, (_, i) => [`x-${i}`, 'v'])), body: 'ok'};
  expect(faultOp({answer: prefix, response}).error).toBeUndefined();
  const before = faultsJson();
  response.headers['x-excess'] = 'v';
  expect(faultOp({answer: prefix, response}).malformed).toBe(true);
  expect(faultsJson()).toEqual(before);
  expect(() => parseFaults(`${prefix}\t-\t-\t0\t1\t${JSON.stringify(response)}`)).toThrow('headers');
});

test('the longest live prefix decides, counts run out, pass keeps hits, and lines wait for a journal', () => {
  const lines = [];
  expect(faultOp({ fail: 'https://api.test/' }).faults).toHaveLength(1);
  faultOp({ fail: 'https://api.test/recipes', times: 1 });
  expect(faultMatches('https://api.test/recipes/3')).toBe(true);
  expect(faultsJson().find(f => f.times === 1).left).toBe(1); // matching does not count
  expect(takeFault('https://api.test/recipes/3')).toBe(true);
  expect(faultsJson().find(f => f.times === 1)).toMatchObject({ left: 0, hits: 1 });
  expect(takeFault('https://api.test/recipes/3')).toBe(true); // the shorter prefix, now
  expect(takeFault('https://other.test/')).toBe(false);
  setFaultLog(line => lines.push(line));
  expect(lines).toEqual(['fetch failed (driver fault): https://api.test/recipes/3', 'fetch failed (driver fault): https://api.test/recipes/3']);
  expect(faultOp({ pass: 'https://api.test/' }).faults.find(f => f.prefix === 'https://api.test/')).toMatchObject({ armed: false, hits: 1 });
  expect(takeFault('https://api.test/x')).toBe(false);
  expect(faultOp({ pass: 'https://nothing.test/' }).error).toContain('no fault was armed');
  expect(faultOp({ fail: 'x', times: 0 }).error).toContain('positive integer');
  expect(faultOp({ fail: '' }).error).toContain('prefix');
  expect(parseFaults('https://a.test/\nhttps://b.test/\t2\nhttps://c.test/\t1\t0\t1\t0')).toEqual([
    { prefix: 'https://a.test/', times: null, left: null, hits: 0, failureHits: 0, armed: true },
    { prefix: 'https://b.test/', times: 2, left: 2, hits: 0, failureHits: 0, armed: true },
    { prefix: 'https://c.test/', times: 1, left: 0, hits: 1, failureHits: 1, armed: false },
  ]);
  expect(() => parseFaults('x\t0')).toThrow('positive integer');
});

test('a faulted request on the wasm host and the JS target fails as a refused connection and never goes out', async () => {
  let hits = 0;
  const origin = Bun.serve({ port: 0, fetch() { hits++; return new Response('ok'); } });
  const set = normalized(`net.fetch ${origin.url.origin}`);
  try {
    faultOp({ fail: `${origin.url.origin}/recipes`, times: 2 });
    const r = await request({ method: 'GET', url: `${origin.url}recipes/1`, headers: [] }, { grantSet: set, controllers: new Set() });
    expect([r.kind, new TextDecoder().decode(r.body)]).toEqual([1, `fetch failed (driver fault): ${origin.url}recipes/1`]);
    const error = await fetchWith(set, `${origin.url}recipes/2`).catch(e => e);
    expect([error.name, error.kind, error.message]).toEqual(['FetchError', 'Network', `fetch failed (driver fault): ${origin.url}recipes/2`]);
    expect(hits).toBe(0);
    // Spent: the next goes out, and so does another prefix.
    expect((await fetchWith(set, `${origin.url}recipes/3`)).status).toBe(200);
    const other = await request({ method: 'GET', url: `${origin.url}users`, headers: [] }, { grantSet: set, controllers: new Set() });
    expect(other.status).toBe(200);
    expect(hits).toBe(2);
    // An ungranted fetch is still Refused, not faulted: the grant check comes first.
    faultOp({ fail: 'https://outside.test/' });
    expect((await fetchWith(set, 'https://outside.test/x').catch(e => e)).kind).toBe('Refused');
  } finally { origin.stop(true); }
});

test('fetchEarly starts no GET a fault will fail, and counts nothing', async () => {
  globalThis.exact ??= {};
  const { fetchEarly } = await import(`./module-glue.js?faults=${Date.now()}`);
  const set = normalized('net.fetch https://api.test');
  faultOp({ fail: 'https://api.test/recipes', times: 1 });
  expect(fetchEarly({ method: 'GET', url: 'https://api.test/recipes/1', headers: [] }, set)).toBeNull();
  expect(faultsJson()[0]).toMatchObject({ left: 1, hits: 0 });
});

test('a GET fetchEarly sent before its fault was armed is not failed by it', async () => {
  globalThis.exact ??= {};
  const { claim, fetchEarly } = await import(`./module-glue.js?claimed=${Date.now()}`);
  let hits = 0;
  const origin = Bun.serve({ port: 0, fetch() { hits++; return new Response('ok'); } });
  const set = normalized(`net.fetch ${origin.url.origin}`);
  try {
    const req = { method: 'GET', url: `${origin.url}recipes/1`, headers: [] };
    expect(fetchEarly(req, set)).toBeFunction();
    faultOp({ fail: `${origin.url.origin}/recipes`, times: 1 }); // armed after the GET left
    const r = await request(req, { grantSet: set, controllers: new Set(), moduleLoader: { claim } });
    expect([r.kind, r.status]).toEqual([0, 200]);
    expect(faultsJson()[0]).toMatchObject({ left: 1, hits: 0 });
    expect(hits).toBe(1);
  } finally { origin.stop(true); }
});

for (const host of ['js', 'wasm']) {
  test(`fixtures retain ${host} header validation`, async () => {
    const init = { headers: { 'bad header': 'value' } };
    const url = 'https://api.test/invalid', set = normalized('net.fetch https://api.test');
    faultOp({ answer: url, response: { status: 200, headers: {}, body: 'fixture' } });
    if (host === 'js') expect(await fetchWith(set, url, init).then(() => 'answered', () => 'rejected')).toBe('rejected');
    else {
      const wasm = await request({ method: 'GET', url, headers: Object.entries(init.headers) }, { grantSet: set, controllers: new Set() });
      expect(wasm.kind).not.toBe(0);
    }
    expect(faultsJson()[0].hits).toBe(0);
  });
}

test('long polls and presence ignore answers when choosing a failure prefix', () => {
  const prefix = 'https://api.test/', url = prefix + 'items/stream';
  faultOp({ fail: prefix, times: 2 });
  faultOp({ answer: prefix + 'items', response: { status: 200, headers: {}, body: 'fixture' } });
  expect(takeFault(url)).toBe(true);
  expect(takeFault(url)).toBe(true);
  expect(takeFault(url)).toBe(false);
  expect(faultsJson().map(f => f.hits)).toEqual([2, 0]);
  expect(faultMatches(url)).toBe(true);
  expect(takeFault(url, true).body).toBe('fixture');
});

for (const host of ['js', 'wasm']) for (const cause of ['deadline', 'abort', 'destination', 'ceiling']) {
  test(`a ${host} ${cause} refusal neither counts nor journals a fixture`, async () => {
    const url = 'https://api.test/refused', set = normalized('net.fetch https://api.test\nfs.write app:/tmp');
    faultOp({ answer: url, response: { status: 200, headers: {}, body: 'fixture' } });
    const lines = []; setFaultLog(line => lines.push(line));
    const controller = new AbortController();
    if (cause === 'abort') controller.abort();
    if (host === 'js') {
      const init = { signal: controller.signal, ...(cause === 'destination' ? { exactBodyTo: 'app:/tmp/result' } : {}), ...(cause === 'ceiling' ? { exactIndependentHttp: { maxResponseBytes: 1 } } : {}) };
      expect(await fetchWith(set, url, init, cause === 'deadline' ? { at: -1, ms: 1 } : null).then(() => 'answered', () => 'rejected')).toBe('rejected');
    } else {
      const op = { method: 'GET', url, headers: [], ...(cause === 'destination' ? { bodyTo: 'app:/tmp/result' } : {}), ...(cause === 'ceiling' ? { maxResponseBytes: 1 } : {}), ...(cause === 'deadline' ? { timeoutMs: 1 } : {}) };
      const original = performance.now;
      let ticks = 0;
      if (cause === 'deadline') performance.now = () => ticks++ * 10;
      try {
        const result = await request(op, { grantSet: set, controllers: new Set(), controller, bodyTarget: async () => ({ put() { throw Error('must not write'); }, close() {} }) });
        expect(result.kind).not.toBe(0);
      } finally { performance.now = original; }
    }
    expect(faultsJson()[0].hits).toBe(0);
    expect(lines).toEqual([]);
  });
}

test('app-facing fetch refuses a fixture download before writing its destination', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-fixture-download-'));
  const { hostFixtureFile } = await import('./tests/glue-fixture.mjs');
  try {
    const set = normalized('net.fetch https://api.test\nfs.write app:/tmp');
    writeFileSync(resolve(dir, 'admission.js'), readFileSync(resolve(ROOT, 'host/web-js/admission.js'), 'utf8').replace(/'\.\.\/web\/([a-z-]+\.js)'/g, "'./$1'"));
    for (const name of ['grant-admission.js', 'faults.js', 'address-space.js', 'navigation.js']) writeFileSync(resolve(dir, name), hostFixtureFile(resolve(ROOT, 'host/web', name)));
    for (const name of ['rt-link.js', 'ts-fetch.js', 'ts-body-from.js']) writeFileSync(resolve(dir, name), runtimeSource(name));
    writeFileSync(resolve(dir, 'admission-data.js'), `export const tsGrantSet = ${JSON.stringify(set)};`);
    writeFileSync(resolve(dir, 'storage-fs.js'), `export let writes = 0; export const bodyTarget = async () => ({ put() { writes++; return 7; }, close() {} });`);
    const { fetch } = await import(resolve(dir, 'ts-fetch.js'));
    faultOp({ answer: 'https://api.test/', response: { status: 200, headers: {}, body: 'fixture' } });
    for (const init of [{ exactBodyTo: 'app:/tmp/result' }, Object.create({ exactBodyTo: 'app:/tmp/result' })]) {
      const result = await fetch('https://api.test/file', init).catch(e => e);
      expect(result.kind).toBe('Refused');
      expect(result.message).toContain('answer fetch does not support exactBodyTo');
    }
    expect((await import(resolve(dir, 'storage-fs.js'))).writes).toBe(0);
    expect(faultsJson()[0].hits).toBe(0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Chromium validates fixture requests and the app-facing download path', async () => {
  const { suiteBrowser } = await import('../../scripts/agent-launch.mjs');
  const { hostFixtureFile, glueDeclarations } = await import('./tests/glue-fixture.mjs');
  const grants = normalized('net.fetch https://api.test\nfs.write app:/tmp');
  const server = Bun.serve({ port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/') return new Response('<!doctype html>', { headers: { 'content-type': 'text/html' } });
    let source;
    if (path === '/host/web-js/admission-data.js') source = `export const tsGrantSet = ${JSON.stringify(grants)};`;
    else if (path === '/host/web-js/storage-fs.js') source = 'export let writes = 0; export const bodyTarget = async () => ({ put() { writes++; return 7; }, close() {} });';
    else if (path.startsWith('/host/')) source = hostFixtureFile(resolve(ROOT, '.' + path));
    else return new Response('', { status: 404 });
    return new Response(source, { headers: { 'content-type': 'text/javascript' } });
  } });
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const loader = glueDeclarations(readFileSync(resolve(ROOT, 'host/web/glue.js'), 'utf8'), ['loadAfterPaint'])
      .replace('import.meta.url', JSON.stringify(new URL('host/web/glue.js', server.url).href));
    await page.addScriptTag({content: loader});
    const result = await page.evaluate(async grants => {
      globalThis.exact = {};
      const prepare = await loadAfterPaint('./faults.js', 'drivenCheckpoint');
      const { faultOp, faultsJson } = await import('/host/web/faults.js');
      const { fetchWith } = await import('/host/web-js/admission.js');
      const { request } = await import('/host/web/http-body.js');
      const { fetch } = await import('/host/web-js/ts-fetch.js');
      const url = 'https://api.test/file', results = [];
      faultOp({ answer: url, response: { status: 200, headers: {}, body: 'fixture' } });
      for (const init of [{ method: 'GET', body: 'payload' }, { headers: { 'bad header': 'value' } }]) {
        results.push(await fetchWith(grants, url, init).then(() => 'answered', () => 'rejected'));
        const wasm = await request({ method: 'GET', url, headers: Object.entries(init.headers ?? {}), body: init.body ? btoa(init.body) : '' }, { grantSet: grants, controllers: new Set() });
        results.push(wasm.kind);
      }
      const download = await fetch(url, { exactBodyTo: 'app:/tmp/result' }).catch(e => e);
      const checkpoint = prepare('digest\n' + JSON.stringify({urls: [[url], ['https://other.test/']]})).split('\n')[0];
      return { checkpoint, results, kind: download.kind, writes: (await import('/host/web-js/storage-fs.js')).writes, hits: faultsJson()[0].hits };
    }, grants);
    expect(result).toEqual({ checkpoint: 'digest driven:0', results: ['rejected', 1, 'rejected', 1], kind: 'Refused', writes: 0, hits: 0 });
  } finally { await page.close(); server.stop(true); }
}, 30_000);

for (const [name, response] of [
  ['large body', { status: 200, headers: {}, body: 'a'.repeat(70 * 1024) }],
  ['set-cookie', { status: 200, headers: { 'Set-Cookie': 'sid=1' }, body: '' }],
  ['set-cookie2', { status: 200, headers: { 'SET-cookie2': 'sid=1' }, body: '' }],
]) test(`invalid fixture: ${name} is refused before arming`, () => {
  expect(faultOp({ answer: 'https://api.test/', response }).malformed).toBe(true);
  expect(faultsJson()).toEqual([]);
  expect(() => parseFaults(`https://api.test/\t-\t-\t0\t1\t${JSON.stringify(response)}`)).toThrow('answer fetch');
});

test('web reload validates the entire carried fixture table before navigation', async () => {
  const { withFaults, faultSpecOf } = await import('../../scripts/agent-launch.mjs');
  const spec = faultSpecOf(['a', 'b'].map(prefix => ({ prefix, hits: 0, armed: true, response: { status: 200, headers: {}, body: 'a'.repeat(40000) } })));
  expect(() => withFaults('http://localhost/?agent=1', spec)).toThrow('64 KiB');
});

test('answers that would overflow the reload table leave it unchanged', () => {
  const response = { status: 200, headers: {}, body: 'a'.repeat(40000) };
  expect(faultOp({ answer: 'https://api.test/a', response }).error).toBeUndefined();
  const before = faultsJson();
  expect(faultOp({ answer: 'https://api.test/b', response }).error).toContain('64 KiB');
  expect(faultsJson()).toEqual(before);
  expect(faultOp({ answer: 'https://api.test/a', response: { ...response, body: '\n'.repeat(40000) } }).error).toContain('64 KiB');
  expect(faultsJson()).toEqual(before);
});

test('an unreadable launch fixture fails instead of opening the live network', () => {
  const previous = globalThis.location;
  const url = new URL('http://localhost/?agent=1');
  url.searchParams.set('failFetch', '1');
  globalThis.location = url;
  try {
    for (const spec of [undefined, 'https://api.test/\t-\t-\t0\t1\t{broken']) {
      globalThis.__exactLaunchFaults = spec;
      expect(() => faultsJson()).toThrow();
    }
  } finally {
    delete globalThis.__exactLaunchFaults;
    if (previous === undefined) delete globalThis.location; else globalThis.location = previous;
  }
});

test('checkpoint fixtures select only matching answers, including passed entries', () => {
  const cp = {urls: [['https://api.test/greeting'], ['https://api.test/other'], []]};
  faultOp({answer: 'https://api.test/greeting', response: {status: 200, headers: {}, body: 'fixture'}});
  expect(checkpointFaults(cp)).toEqual([0]);
  faultOp({fail: 'https://api.test/greeting'});
  expect(checkpointFaults(cp)).toEqual([0]);
  faultOp({pass: 'https://api.test/greeting'});
  expect(checkpointFaults(cp)).toEqual([0]);
  const original = JSON.stringify(cp);
  expect(drivenCheckpoint(`digest\n${original}`)).toBe(`digest driven:0\n${original}`);
});

test('Unicode launch fixtures use the native byte budget', () => {
  const head = 'https://api.test/\t-\t-\t0\t1\t';
  const response = {status: 200, headers: {}, body: '\u2028'};
  response.body += 'a'.repeat(65514 - new TextEncoder().encode(head + JSON.stringify(response)).length);
  const spec = head + JSON.stringify(response);
  expect(new TextEncoder().encode(spec).length).toBe(65514);
  const parsed = parseFaults(spec);
  expect(new TextEncoder().encode(faultSpecOf(parsed)).length).toBe(65516);
  expect(parseFaults(faultSpecOf(parsed))).toEqual(parsed);
  response.body += 'aa';
  expect(() => parseFaults(head + JSON.stringify(response))).toThrow('64 KiB');
});

test('failure evidence survives replacement and reload independently of response hits', () => {
  const prefix = 'https://api.test/';
  faultOp({fail: prefix});
  takeFault(prefix);
  faultOp({answer: prefix, response: {status: 200, headers: {}, body: 'ok'}});
  takeFault(prefix, true);
  globalThis.__exactFaults.entries = parseFaults(faultSpecOf(faultsJson()));
  expect(faultsJson()[0]).toMatchObject({hits: 1, failureHits: 1});
  faultOp({fail: prefix});
  takeFault(prefix);
  expect(parseFaults(faultSpecOf(faultsJson()))[0]).toMatchObject({hits: 1, failureHits: 2});
});

test('answer fetch 4xx and 5xx responses increment failureHits across reload while 2xx does not (LLP 1156.007)', async () => {
  const set = normalized('net.fetch https://api.test');
  faultOp({answer: 'https://api.test/ok', response: {status: 200, headers: {}, body: 'ok'}});
  faultOp({answer: 'https://api.test/missing', response: {status: 404, headers: {}, body: 'nope'}});
  faultOp({answer: 'https://api.test/broken', response: {status: 500, headers: {}, body: 'boom'}});
  expect((await fetchWith(set, 'https://api.test/ok')).status).toBe(200);
  expect((await fetchWith(set, 'https://api.test/missing')).status).toBe(404);
  expect((await fetchWith(set, 'https://api.test/broken')).status).toBe(500);
  const byPrefix = Object.fromEntries(faultsJson().map(f => [f.prefix, {hits: f.hits, failureHits: f.failureHits}]));
  expect(byPrefix).toEqual({
    'https://api.test/ok': {hits: 1, failureHits: 0},
    'https://api.test/missing': {hits: 1, failureHits: 1},
    'https://api.test/broken': {hits: 1, failureHits: 1},
  });
  const reloaded = Object.fromEntries(parseFaults(faultSpecOf(faultsJson())).map(f => [f.prefix, {hits: f.hits, failureHits: f.failureHits}]));
  expect(reloaded).toEqual(byPrefix);
});

test('Chromium gives fixtures the real response properties and request-mode refusals', async () => {
  const { suiteBrowser } = await import('../../scripts/agent-launch.mjs');
  const { hostFixtureFile } = await import('./tests/glue-fixture.mjs');
  const headers = {'content-type': 'application/json', 'x-fixture': 'yes', 'access-control-allow-origin': '*', 'access-control-expose-headers': '*'};
  const backend = Bun.serve({port: 0, fetch() { return new Response('{"next":"second"}', {status: 201, headers}); }});
  const server = Bun.serve({port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    return path.startsWith('/host/')
      ? new Response(hostFixtureFile(resolve(ROOT, '.' + path)), {headers: {'content-type': 'text/javascript'}})
      : new Response('<!doctype html>', {headers: {'content-type': 'text/html'}});
  }});
  const set = normalized(`net.fetch ${backend.url.origin}`);
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const results = await page.evaluate(async ({set, url, headers}) => {
      globalThis.exact = {};
      const {fetchWith} = await import('/host/web-js/admission.js');
      const {faultOp} = await import('/host/web/faults.js');
      const read = async mode => {
        try {
          const r = await fetchWith(set, url, {mode}), clone = r.clone().clone();
          let immutable = false;
          try { clone.headers.set('x-new', 'bad'); } catch (e) { immutable = e instanceof TypeError; }
          const before = clone.bodyUsed, body = await clone.json();
          return {url: clone.url, next: new URL(body.next, clone.url).href, type: clone.type, status: clone.status,
            ok: clone.ok, redirected: clone.redirected, header: clone.headers.get('x-fixture'), immutable, before, after: clone.bodyUsed, originalUsed: r.bodyUsed};
        } catch (e) { return {kind: e.kind}; }
      };
      const modes = ['cors', 'no-cors', 'same-origin'];
      const live = await Promise.all(modes.map(read));
      faultOp({answer: url.split('#')[0], response: {status: 201, headers, body: '{"next":"second"}'}});
      const fixture = await Promise.all(modes.map(read));
      return {live, fixture};
    }, {set, url: new URL('items/first#fragment', backend.url).href, headers});
    expect(results.fixture).toEqual(results.live);
    expect(results.fixture[0]).toMatchObject({type: 'cors', immutable: true, before: false, after: true, originalUsed: false});
    expect(results.fixture.slice(1)).toEqual([{kind: 'Refused'}, {kind: 'Network'}]);
  } finally { await page.close(); server.stop(true); backend.stop(true); }
}, 30_000);
