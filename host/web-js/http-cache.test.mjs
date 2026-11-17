import { expect, test } from 'bun:test';
import { cachingFetch, FRESH } from './http-cache.mjs';

// An origin that answers from `routes` (URL → headers), numbering its answers, with the cache's clock in the test's hands.
function origin(routes, options = {}) {
  let t = Date.parse('2026-10-30T12:00:00Z'), served = 0;
  const outbound = async (input, init) => {
    const request = new Request(input, init);
    const route = routes[request.url];
    const { status = 200, redirected = false, ...headers } = route;
    const response = new Response(`answer ${++served}`, { status, headers: { date: new Date(t).toUTCString(), ...headers } });
    if (redirected) Object.defineProperty(response, 'redirected', { value: true });
    return response;
  };
  const fetch = cachingFetch(outbound, { now: () => t, ...options });
  const get = async (url, init) => { const r = await fetch(url, init); const body = await r.text(); await fetch.stored(); return { body, age: r.headers.get('age') }; };
  return { get, fetch, advance: s => { t += s * 1000; }, served: () => served };
}

test('an answer is kept for the lifetime its headers give, and given again with its age', async () => {
  const o = origin({
    'http://api.test/a': { 'cache-control': 'public, max-age=60' },
    'http://api.test/s': { 'cache-control': 'max-age=600, s-maxage=10' },
    'http://api.test/e': { expires: 'Fri, 30 Oct 2026 12:00:30 GMT' },
    'http://api.test/aged': { 'cache-control': 'max-age=60', age: '50' },
  });
  // Expires counts from Date: 30 s here.
  await o.get('http://api.test/e'); o.advance(20);
  expect((await o.get('http://api.test/e')).body).toBe('answer 1');
  o.advance(10);
  expect((await o.get('http://api.test/e')).body).toBe('answer 2');
  expect(await o.get('http://api.test/a')).toEqual({ body: 'answer 3', age: null });
  o.advance(59);
  expect(await o.get('http://api.test/a')).toEqual({ body: 'answer 3', age: '59' });
  expect((await o.fetch('http://api.test/a')).url).toBe('http://api.test/a');
  // What a render reads of each answer: how long any reader may still be given it (LLP 1048.007 D8).
  expect((await o.fetch('http://api.test/a'))[FRESH]).toBe(1000);
  o.advance(1);
  expect((await o.get('http://api.test/a')).body).toBe('answer 4');
  // A shared cache reads s-maxage before max-age.
  await o.get('http://api.test/s'); o.advance(9);
  expect((await o.get('http://api.test/s')).body).toBe('answer 5');
  o.advance(1);
  expect((await o.get('http://api.test/s')).body).toBe('answer 6');
  // An answer 50 s old on arrival has 10 s left.
  await o.get('http://api.test/aged'); o.advance(10);
  expect((await o.get('http://api.test/aged')).body).toBe('answer 8');
});

test('what a shared cache must not keep is fetched every time', async () => {
  const kept = { 'cache-control': 'max-age=60' };
  const o = origin({
    'http://api.test/no-store': { 'cache-control': 'no-store, max-age=60' },
    'http://api.test/private': { 'cache-control': 'private, max-age=60' },
    'http://api.test/no-cache': { 'cache-control': 'no-cache, max-age=60' },
    'http://api.test/no-lifetime': {},
    'http://api.test/vary-star': { ...kept, vary: '*' },
    'http://api.test/error': { ...kept, status: 500 },
    'http://api.test/redirected': { ...kept, redirected: true },
    'http://api.test/kept': kept,
    'http://api.test/auth': kept,
    'http://api.test/auth-public': { 'cache-control': 'public, max-age=60' },
  });
  for (const path of ['no-store', 'private', 'no-cache', 'no-lifetime', 'vary-star', 'error', 'redirected']) {
    const url = `http://api.test/${path}`;
    expect((await o.get(url)).body).not.toBe((await o.get(url)).body);
    expect([path, (await o.fetch(url))[FRESH]]).toEqual([path, 0]);
  }
  // A request with credentials, unless the answer says any reader may have it (RFC 9111 §3.5).
  const auth = { headers: { authorization: 'Bearer t' } };
  expect((await o.get('http://api.test/auth', auth)).body).not.toBe((await o.get('http://api.test/auth', auth)).body);
  expect((await o.get('http://api.test/auth-public', auth)).body).toBe((await o.get('http://api.test/auth-public', auth)).body);
  // Only a GET is looked at, and a request may ask past the cache, as a browser's fetch may.
  const first = (await o.get('http://api.test/kept')).body;
  expect((await o.get('http://api.test/kept', { method: 'POST', body: 'x' })).body).not.toBe(first);
  expect((await o.get('http://api.test/kept', { cache: 'no-store' })).body).not.toBe(first);
  expect((await o.get('http://api.test/kept')).body).toBe(first);
  // `reload` and `no-cache` go to the origin and keep what they bring.
  let last = first;
  for (const init of [{ cache: 'reload' }, { headers: { 'cache-control': 'no-cache' } }]) {
    const brought = (await o.get('http://api.test/kept', init)).body;
    expect(brought).not.toBe(last);
    expect((await o.get('http://api.test/kept')).body).toBe(brought);
    last = brought;
  }
});

test('an answer kept for an anonymous request is given to one with credentials only if marked shareable', async () => {
  const o = origin({
    'http://api.test/anon': { 'cache-control': 'max-age=60' },
    'http://api.test/public': { 'cache-control': 'public, max-age=60' },
    'http://api.test/shared': { 'cache-control': 's-maxage=60' },
    'http://api.test/must': { 'cache-control': 'max-age=60, must-revalidate' },
  });
  const auth = { headers: { authorization: 'Bearer t' } };
  const anon = (await o.get('http://api.test/anon')).body;
  expect((await o.get('http://api.test/anon', auth)).body).not.toBe(anon);
  // The answer to the request with credentials did not replace the anonymous one.
  expect((await o.get('http://api.test/anon')).body).toBe(anon);
  for (const path of ['public', 'shared', 'must']) {
    const url = `http://api.test/${path}`, first = (await o.get(url)).body;
    expect([path, (await o.get(url, auth)).body]).toEqual([path, first]);
  }
});

test('Vary keys the answer on the request headers it names', async () => {
  const o = origin({ 'http://api.test/v': { 'cache-control': 'max-age=60', vary: 'Accept-Language' } });
  const fr = { headers: { 'accept-language': 'fr' } }, de = { headers: { 'accept-language': 'de' } };
  const a = (await o.get('http://api.test/v', fr)).body;
  expect((await o.get('http://api.test/v', fr)).body).toBe(a);
  expect((await o.get('http://api.test/v', de)).body).not.toBe(a);
});

test('a stale answer within stale-while-revalidate is given while one refetch replaces it', async () => {
  const o = origin({ 'http://api.test/swr': { 'cache-control': 'max-age=10, stale-while-revalidate=30' } });
  await o.get('http://api.test/swr');
  o.advance(15);
  // The refetch has run by the time `get` returns (it awaits `stored`), and only one went out for both reads.
  const [x, y] = await Promise.all([o.fetch('http://api.test/swr'), o.fetch('http://api.test/swr')]);
  expect([await x.text(), await y.text()]).toEqual(['answer 1', 'answer 1']);
  await o.fetch.stored();
  expect(o.served()).toBe(2);
  expect((await o.get('http://api.test/swr')).body).toBe('answer 2');
  o.advance(45);
  expect((await o.get('http://api.test/swr')).body).toBe('answer 3');
});

test('a stale answer that must be revalidated is never given, whatever grace it or the request names', async () => {
  const o = origin({
    'http://api.test/must': { 'cache-control': 'max-age=1, must-revalidate, stale-while-revalidate=60' },
    'http://api.test/proxy': { 'cache-control': 'max-age=1, proxy-revalidate, stale-while-revalidate=60' },
    'http://api.test/shared': { 'cache-control': 's-maxage=1, stale-while-revalidate=59' },
  });
  for (const path of ['must', 'proxy']) {
    const url = `http://api.test/${path}`, first = (await o.get(url)).body;
    o.advance(2);
    expect((await o.get(url)).body).not.toBe(first);
    o.advance(2);
    expect((await o.get(url, { headers: { 'cache-control': 'max-stale' } })).body).not.toBe(first);
  }
  // s-maxage implies proxy-revalidate (RFC 9111 §5.2.2.10), but the pair asks to be served stale.
  const first = (await o.get('http://api.test/shared')).body;
  o.advance(2);
  expect((await o.get('http://api.test/shared')).body).toBe(first);
});

test('a request\'s max-age, min-fresh and max-stale bound the age of the answer it is given', async () => {
  const o = origin({ 'http://api.test/a': { 'cache-control': 'max-age=60' } });
  const age = seconds => ({ headers: { 'cache-control': `max-age=${seconds}` } });
  const kept = (await o.get('http://api.test/a')).body;
  o.advance(10);
  expect((await o.get('http://api.test/a', age(20))).body).toBe(kept);
  const younger = (await o.get('http://api.test/a', age(1))).body;
  expect(younger).not.toBe(kept);
  // What a request too strict for the kept answer brings is kept in its place.
  expect((await o.get('http://api.test/a')).body).toBe(younger);
  o.advance(50);
  expect((await o.get('http://api.test/a', { headers: { 'cache-control': 'min-fresh=5' } })).body).toBe(younger);
  const fresher = (await o.get('http://api.test/a', { headers: { 'cache-control': 'min-fresh=20' } })).body;
  expect(fresher).not.toBe(younger);
  // Past its lifetime, a request that accepts staleness is given it while one refetch replaces it.
  o.advance(70);
  expect((await o.get('http://api.test/a', { headers: { 'cache-control': 'max-stale=5' } })).body).not.toBe(fresher);
  const now = (await o.get('http://api.test/a')).body;
  o.advance(65);
  const served = o.served();
  expect((await o.get('http://api.test/a', { headers: { 'cache-control': 'max-stale=10' } })).body).toBe(now);
  expect(o.served()).toBe(served + 1);
  expect((await o.get('http://api.test/a')).body).not.toBe(now);
});

test('the least recently given answer goes first past the bounds', async () => {
  const o = origin(new Proxy({}, { get: () => ({ 'cache-control': 'max-age=60' }) }), { entries: 2 });
  const a = (await o.get('http://api.test/1')).body;
  await o.get('http://api.test/2');
  await o.get('http://api.test/1');
  await o.get('http://api.test/3');
  expect((await o.get('http://api.test/1')).body).toBe(a);
  expect(o.served()).toBe(3);
  expect((await o.get('http://api.test/2')).body).toBe('answer 4');
});

test('oversized chunked and underdeclared bodies stop cache collection without draining the caller', async () => {
  for (const declared of [null, '8']) {
    let pulls = 0;
    const cache = cachingFetch(async () => new Response(new ReadableStream({
      pull(c) { c.enqueue(new Uint8Array(256)); if (++pulls === 64) c.close(); },
    }), { headers: { 'cache-control': 'max-age=60', ...(declared ? { 'content-length': declared } : {}) } }), { entryBytes: 1024 });
    const response = await cache('https://cache.test/large');
    await cache.stored();
    expect(pulls).toBeLessThan(64);
    // Only the cache branch was stopped: the render can still read the whole answer.
    expect((await response.arrayBuffer()).byteLength).toBe(64 * 256);
  }
});

test('concurrent pending entries share the cache byte budget and release it after rejection', async () => {
  const controls = [], responses = [];
  const cache = cachingFetch(async () => new Response(new ReadableStream({ start(c) { controls.push(c); } }),
    { headers: { 'cache-control': 'max-age=60' } }), { entryBytes: 1024, bytes: 1024 });
  responses.push(await cache('https://cache.test/a'), await cache('https://cache.test/b'));
  controls[0].enqueue(new Uint8Array(768));
  controls[1].enqueue(new Uint8Array(768));
  controls[0].close(); controls[1].close();
  await cache.stored();
  // Neither oversized pending copy fits alongside its contiguous copy, so both go out again.
  const again = await cache('https://cache.test/a');
  expect(controls).toHaveLength(3);
  controls[2].enqueue(new Uint8Array(128)); controls[2].close();
  await cache.stored();
  const kept = await cache('https://cache.test/a');
  expect(controls).toHaveLength(3);
  expect((await kept.arrayBuffer()).byteLength).toBe(128);
  await Promise.all([...responses, again].map(r => r.arrayBuffer()));
});

test('compiled JS renders abort stalled response headers and bodies when their deadline ends', async () => {
  const { mkdtempSync, writeFileSync, rmSync } = await import('node:fs');
  const { createServer } = await import('node:http');
  const { resolve, join } = await import('node:path');
  const { tmpdir } = await import('node:os');
  const { renderer } = await import('./render.mjs');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(join(tmpdir(), 'exact-render-owner-'));
  let body = false, began = 0, closed = 0, released;
  const sockets = new Set(), api = createServer((req, res) => {
    began++; res.on('close', () => { closed++; released?.(); });
    if (body) { res.writeHead(200, { 'content-type': 'text/plain' }); res.write('partial'); }
  });
  api.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
  await new Promise(ok => api.listen(0, '127.0.0.1', ok));
  const url = `http://127.0.0.1:${api.address().port}`;
  try {
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Render Owner', app: { id: 'test.render-owner', name: 'Render Owner' }, host: { web: {} } }));
    writeFileSync(join(dir, 'app.contract'), 'routes nav\n  home "/" render=request\ncomponent App\n  resource text = text() as shape string else empty()\n  view\n    text text\n');
    writeFileSync(join(dir, 'app.ts'), `export const appId='test.render-owner';export const grants=${JSON.stringify('net.fetch ' + url)};export async function answer(){return await(await fetch(${JSON.stringify(url)})).text()}`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-owner', '--out', join(dir, 'dist'), '--render', 'js'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, out, err] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? out + err : '' }).toEqual({ status: 0, error: '' });
    const render = renderer(join(dir, 'dist')); render.ahead = false;
    for (body of [false, true]) for (let i = 0; i < 3; i++) {
      let timer;
      const ended = new Promise((ok, no) => { released = ok; timer = setTimeout(() => no(Error('render left its upstream open')), 2000); });
      try { expect((await render('/', 100)).status).toBe(503); await ended; }
      finally { clearTimeout(timer); released = null; }
      expect(closed).toBe(began);
    }
    expect([began, closed]).toEqual([6, 6]);
  } finally { for (const socket of sockets) socket.destroy(); api.close(); rmSync(dir, { recursive: true, force: true }); }
}, 60_000);
