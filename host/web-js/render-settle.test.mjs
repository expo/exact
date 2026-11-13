import { expect, spyOn, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import vm from 'node:vm';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
const scratch = () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const dir = mkdtempSync(resolve(root, 'target/render-settle-test-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Render Settle', app: { id: 'test.render-settle', name: 'Render Settle' }, host: { web: {} } }));
  return dir;
};
// The app in `dir` built for the web with its pages rendered under Bun; its build names no failure.
async function build(dir) {
  const dist = resolve(dir, 'dist');
  const built = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-settle-test', '--out', dist, '--render', 'js'], {
    cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([built.exited, new Response(built.stdout).text(), new Response(built.stderr).text()]);
  expect(status, stdout + stderr).toBe(0);
  expect(stdout + stderr).not.toMatch(/failed|structuredClone|dynamic import/);
  return dist;
}

// The rule a render settles by, as the render host settles (host/render/src/lib.rs `settle`; LLP 1048.000 D9): it
// waits for work in flight (a fetch, a held fetch, an upload, a stream until its first message, an answer's own
// timer) and for nothing else. An answer waiting on nothing the render runs (a promise nothing settles, another
// answer's, a storage call the render refused) stays pending without holding the page, which is complete. The pages
// are the ones the render host was run on for #108, with what it gave (status, settled, pending names, whether it
// answered at once or waited, or waited out the deadline); where this renderer gives another answer, `DIFFERS` says
// which and why.
const NATIVE = {
  '/never': [200, true, ['r'], 'quick'], '/shared': [200, true, ['r', 'other'], 'quick'], '/shared-plain': [200, true, ['r', 'other'], 'quick'],
  '/read': [200, true, ['r'], 'quick'], '/fetch': [200, true, [], 'waited'], '/held-status': [200, true, [], 'waited'],
  '/timer': [503, false, ['r'], 'deadline'], '/stream': [200, true, [], 'quick'], '/second': [200, true, ['r'], 'quick'],
  '/second-plain': [200, true, ['r'], 'quick'], '/upload': [200, true, [], 'waited'], '/body-from': [200, true, ['r'], 'quick'],
  '/body-from-await': [200, true, ['r'], 'quick'], '/ignored': [200, true, [], 'waited'], '/ignored-plain': [200, true, [], 'waited'],
  '/fallback': [200, true, [], 'quick'], '/watch-sync': [200, true, [], 'quick'], '/watch-await': [200, true, [], 'quick'],
  '/watch-then': [200, true, [], 'quick'], '/compress': [200, true, [], 'quick'], '/mapper-before': [200, true, ['r'], 'quick'],
  '/mapper-never': [503, false, ['r'], 'deadline'], '/mapper-inside': [200, true, ['r'], 'quick'],
  // A long poll is a stream: it holds the page until its first message, whatever replies it gets without one (a 204,
  // a 503 to wait out), and the page's own status (410 here) waits for it.
  '/lp-later': [200, true, [], 'waited'], '/lp-busy': [503, false, ['r'], 'deadline'], '/lp-status': [503, false, ['r'], 'deadline'],
};
// Where this renderer differs from the render host, and why (each is listed in QUEUE.md).
const DIFFERS = {
  // The render host rejects a refused storage call, so a source that catches it answers its fallback; here the call
  // never answers and the resource stays pending, as LLP 1048.000 D9 has a refused source (keeps its placeholder).
  '/fallback': [200, true, ['r'], 'quick'],
  // On the web a plain `.then` runs outside its answer, so the reader's page refuses this watch (`watchDocument`): the
  // render refuses it as the page does. On a device the turn's answer is current in a `.then` (LLP 1146 D2).
  '/watch-then': [200, true, [], 'quick'],
  // A mapper awaiting a storage promise made before the stream opened holds the stream in flight to the deadline, as a
  // mapper awaiting a promise nothing settles does (`/mapper-never`). The render host's rejection ends the message.
  '/mapper-before': [503, false, ['r'], 'deadline'],
  // A mapper that calls storage itself: its first message never comes, so the stream holds the page to the deadline,
  // as a stream whose mapper never answers does. A stream holds a page until its first message whatever storage calls
  // are made around it; the render host's rejection of the refused call ends the message (QUEUE.md).
  '/mapper-inside': [503, false, ['r'], 'deadline'],
};
test('a render waits for work in flight and for nothing else, as the render host does', async () => {
  const dir = scratch();
  let polls = 0;
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/lp-later') return polls++ === 0 ? new Response(null, { status: 204 }) : Response.json({ seq: '1', data: 'hello' });
    if (path === '/lp-busy') return new Response('busy', { status: 503, headers: { 'retry-after': '30' } });
    if (path === '/lp-status') return new Response(null, { status: 204 });
    if (path === '/events') return new Response(new ReadableStream({ start(c) {
      const send = d => { try { c.enqueue(new TextEncoder().encode(`data: ${d}\n\n`)); } catch {} };
      send('first'); setTimeout(() => send('second'), 200);
    } }), { headers: { 'content-type': 'text/event-stream' } });
    await request.arrayBuffer();
    await new Promise(r => setTimeout(r, path === '/slow' ? 200 : 100));
    return new Response(path === '/slow' ? 'slow' : 'done');
  } });
  let site, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request
    never "/never" render=request
    shared "/shared" render=request
    read "/read" render=request
    fetched "/fetch" render=request
    heldStatus "/held-status" render=request
    timer "/timer" render=request
    stream "/stream" render=request
    second "/second" render=request
    secondPlain "/second-plain" render=request
    upload "/upload" render=request
    bodyFrom "/body-from" render=request
    ignored "/ignored" render=request
    ignoredPlain "/ignored-plain" render=request
    fallback "/fallback" render=request
    watchSync "/watch-sync" render=request
    watchAwait "/watch-await" render=request
    watchThen "/watch-then" render=request
    compress "/compress" render=request
    mapperBefore "/mapper-before" render=request
    mapperNever "/mapper-never" render=request
    mapperInside "/mapper-inside" render=request
    bodyFromAwait "/body-from-await" render=request
    sharedPlain "/shared-plain" render=request
    lpLater "/lp-later" render=request
    lpBusy "/lp-busy" render=request
    lpStatus "/lp-status" render=request

component App
  resource r = probe(top(nav).name) as shape string else empty()
  resource other = other(top(nav).name) as shape string else empty()
  resource third = third(top(nav).name) as shape string else empty()
  view
    main
      if r == "missing" || third == "missing"
        head status=404
      if top(nav).name == "lpStatus"
        head status=410
      text r testId="r"
      text other testId="other"
      text third testId="third"
      text (pending(r) ? "pending" : failed(r) ? "failed" : "answered") testId="rstate"
      text (pending(other) ? "pending" : failed(other) ? "failed" : "answered") testId="otherstate"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'fs.read app:/data\\nfs.write app:/data\\nsqlite.open app:/data/books.db\\nnet.fetch ${api.url.origin}';
const API = '${api.url.origin}';
let queue: Promise<unknown> = Promise.resolve();
const withBooks = (storage: any, key: string) => {
  const run = queue.then(async () => { const db = await storage.sqlite.open('app:/data/books.db'); try { return key; } finally { await db.close(); } });
  queue = run.catch(() => {});
  return run;
};
let messages = 0, release: (v: string) => void = () => {};
const secondMessage = new Promise<string>(resolve => { release = resolve; });
let fetched: Promise<string> | null = null;
const slow = () => fetched ??= fetch(API + '/slow').then(r => r.text());
export const answer = async (name: string, [route]: string[], _store: unknown, storage: any): Promise<any> => {
  if (name === 'other') {
    if (route === 'shared') return withBooks(storage, 'other');
    if (route === 'sharedPlain') return queue.then(() => 'other');
    if (route === 'ignored' || route === 'ignoredPlain') return slow();
    return 'none';
  }
  if (name === 'third') {
    if (route === 'second' || route === 'secondPlain') return fetch(API + '/events', { exactStream: (e: any) => { if (++messages === 2) release('missing'); return e.data; } } as RequestInit);
    return 'none';
  }
  switch (route) {
    case 'never': return new Promise(() => {});
    case 'lpLater': return fetch(API + '/lp-later', { exactStream: (e: any) => e.data, exactLongPoll: { cursor: 'since', from: 'seq' } } as RequestInit);
    case 'lpBusy': return fetch(API + '/lp-busy', { exactStream: (e: any) => e.data, exactLongPoll: { cursor: 'since', from: 'seq' } } as RequestInit);
    case 'lpStatus': return fetch(API + '/lp-status', { exactStream: (e: any) => e.data, exactLongPoll: { cursor: 'since', from: 'seq' } } as RequestInit);
    case 'shared': return withBooks(storage, 'r');
    case 'sharedPlain': { const run = queue.then(() => new Promise(() => {})); queue = run.catch(() => {}); return run; }
    case 'read': return new TextDecoder().decode(await storage.fs.readFile('app:/data/p'));
    case 'fetched': return (await fetch(API + '/slow')).text();
    case 'heldStatus': return (await fetch(API + '/slow', { exactHeld: true } as RequestInit)).text();
    case 'timer': return new Promise(resolve => setTimeout(() => resolve('later'), 50));
    case 'stream': return fetch(API + '/events', { exactStream: (e: any) => e.data } as RequestInit);
    case 'second': void storage.fs.readFile('app:/data/p'); return secondMessage;
    case 'secondPlain': return secondMessage;
    case 'upload': return fetch(API + '/upload', { method: 'POST', body: 'x', exactUploadProgress: true, exactStream: (e: any) => e.type === 'load' ? 'load ' + e.data : e.type } as RequestInit);
    case 'bodyFrom': return (await fetch(API + '/upload', { method: 'POST', exactBodyFrom: 'app:/data/b' } as RequestInit)).text();
    case 'ignored': void storage.fs.readFile('app:/data/p'); return slow();
    case 'ignoredPlain': return slow();
    case 'fallback': try { return new TextDecoder().decode(await storage.fs.readFile('app:/data/p')); } catch { return 'fallback'; }
    case 'watchSync': storage.fs.watch('doc:/notes'); return 'watched';
    case 'watchAwait': await Promise.resolve(); storage.fs.watch('doc:/notes'); return 'watched';
    case 'watchThen': return Promise.resolve().then(() => { storage.fs.watch('doc:/notes'); return 'watched'; });
    case 'mapperBefore': { const body = storage.fs.readFile('app:/data/p'); return fetch(API + '/events', { exactStream: async () => new TextDecoder().decode(await body) } as RequestInit); }
    case 'mapperNever': { const body = new Promise<Uint8Array>(() => {}); return fetch(API + '/events', { exactStream: async () => new TextDecoder().decode(await body) } as RequestInit); }
    case 'mapperInside': return fetch(API + '/events', { exactStream: async () => new TextDecoder().decode(await storage.fs.readFile('app:/data/p')) } as RequestInit);
    case 'bodyFromAwait': await Promise.resolve(); return (await fetch(API + '/upload', { method: 'POST', exactBodyFrom: 'app:/data/b' } as RequestInit)).text();
    case 'compress': try { await storage.fs.compressImage('app:/a', 'app:/b', {}); return 'compressed'; } catch { return 'fallback'; }
  }
  return 'none';
};
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const DEADLINE = 3000, seen = {}, wanted = {};
    for (const [path, native] of Object.entries(NATIVE)) {
      const reply = await render(path, DEADLINE);
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
      const ms = reply.render, timing = ms >= DEADLINE ? 'deadline' : ms >= 90 ? 'waited' : 'quick';
      seen[path] = [reply.status, reply.settled, checkpoint.pending, timing];
      wanted[path] = DIFFERS[path] ?? native;
      if (path === '/watch-await') {
        expect(reply.failed).toEqual([]);
        expect(checkpoint.open.map(k => checkpoint.answers[k][0])).toEqual(['r']);
        expect(/data-testid="r"[^>]*>([^<]*)</.exec(reply.html)[1]).toBe('watched');
      }
      if (path === '/body-from-await') expect(reply.refused).toEqual([['r', 'exactBodyFrom app:/data/b']]);
      if (path === '/shared') site = reply.html;
    }
    expect(seen).toEqual(wanted);
    // Adoption asks the shared queue's sources again and establishes the watch the render deferred.
    const pages = { '/shared': site };
    for (const path of ['/watch-await', '/watch-then']) pages[path] = (await render(path, DEADLINE)).html;
    site = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname;
      if (pages[path]) return new Response(pages[path], { headers: { 'content-type': 'text/html' } });
      const asset = Bun.file(resolve(dist, path.slice(1)));
      return await asset.exists() ? new Response(asset) : new Response(null, { status: 404 });
    } });
    browser = await open({ host: 'web', url: new URL('/shared', site.url).href });
    await browser.clock('settle');
    const journal = await browser.carrier.evaluate('globalThis.exact.journal');
    expect(journal.some(l => l.endsWith('adopted the document')), journal.join('\n')).toBe(true);
    expect(await browser.carrier.evaluate(`['rstate', 'otherstate'].map(id => document.querySelector('[data-testid="' + id + '"]').textContent)`))
      .not.toContain('pending');
    await browser.close();
    browser = await open({ host: 'web', url: new URL('/watch-await', site.url).href });
    await browser.clock('settle');
    expect(await browser.carrier.evaluate(`[document.querySelector('[data-testid="r"]').textContent, document.querySelector('[data-testid="rstate"]').textContent]`))
      .toEqual(['watched', 'answered']);
    await browser.close();
    browser = await open({ host: 'web', url: new URL('/watch-then', site.url).href });
    await browser.clock('settle');
    expect(await browser.carrier.evaluate(`[document.querySelector('[data-testid="r"]').textContent, document.querySelector('[data-testid="rstate"]').textContent]`))
      .toEqual(['', 'failed']);
  } finally {
    await browser?.close(); if (site?.stop) site.stop(true); api.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test.each([false, true])('a render maps upload progress and waits for its reply (storage grants: %s)', async storage => {
  const dir = scratch();
  let complete = false;
  let realm, current;
  const requests = [];
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
    requests.push([request.method, await request.text(), request.headers.get('accept')]);
    // Headers acknowledge the body sent; the reply's body stays open until the render's deadline.
    const body = complete ? 'done' : new ReadableStream({ start(c) { c.enqueue(new TextEncoder().encode('do')); } });
    return new Response(body, { status: 201, headers: { 'content-type': 'text/event-stream' } });
  } });
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=client
    upload "/upload" render=request

component App
  resource r = upload() as shape string else empty()
  view
    main
      text r testId="r"
`);
    const grants = `${storage ? 'fs.read app:/data\nfs.write app:/data\nsqlite.open app:/data/books.db\n' : ''}net.fetch ${api.url.origin}`;
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = ${JSON.stringify(grants)};
export const answer = (): any => {
  const mapped: string[] = [];
  return fetch('${api.url.origin}/upload', { method: 'POST', body: 'x', exactUploadProgress: true,
    exactStream: (e: any) => {
      mapped.push(e.type === 'progress' ? 'progress ' + e.loaded + '/' + e.total + ' ' + e.lengthComputable
        : e.type === 'load' ? 'load ' + e.status + ' ' + e.data : e.type);
      return mapped.join('; ');
    } } as RequestInit);
};
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const createContext = vm.createContext;
    realm = spyOn(vm, 'createContext').mockImplementation((ctx, options) => {
      const clock = { now: 0 };
      ctx.performance = { now: () => clock.now };
      current = { ctx, clock };
      return createContext(ctx, options);
    });
    const page = async deadline => {
      const reply = await render('/upload', deadline);
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
      return { status: reply.status, settled: reply.settled, pending: checkpoint.pending, failed: reply.failed,
        r: /data-testid="r"[^>]*>([^<]*)</.exec(reply.html)[1] };
    };
    let finished = false;
    const waiting = page(1000).then(reply => { finished = true; return reply; });
    // Only the test moves the render's deadline; the upload may take any number of event-loop turns.
    const until = performance.now() + 10000;
    while (!finished && current?.ctx.document.getElementById('exact-root').textContent !== 'progress 1/1 true') {
      if (performance.now() >= until) throw new Error('upload progress did not commit');
      await new Promise(resolve => setTimeout(resolve, 5));
    }
    expect(finished).toBe(false);
    current.clock.now = 1000;
    expect(await waiting).toEqual({ status: 503, settled: false, pending: ['r'], failed: [], r: 'progress 1/1 true' });
    complete = true;
    expect(await page(3000)).toEqual({ status: 200, settled: true, pending: [], failed: [], r: 'progress 1/1 true; load 201 done' });
    expect(requests).toEqual([['POST', 'x', '*/*'], ['POST', 'x', '*/*']]);
  } finally {
    if (current) current.clock.now = Infinity;
    realm?.mockRestore();
    api.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// A fetch an app module makes as it loads, which the reader's page runs as its answer's wait, is work in flight: the
// render waits for it, on a page with no status too, and the page shows its answer. (A device's executor refuses a
// fetch with no answer running, js/src/fetch.js `request`, so the render host answers this page with `r` pending.)
test('a render waits for a fetch an app module made as it loaded', async () => {
  const dir = scratch();
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Promise(resolve => setTimeout(() => resolve(new Response('loaded')), 200)) });
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request

component App
  resource r = probe() as shape string else empty()
  view
    main
      text r testId="r"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'net.fetch ${api.url.origin}';
const loaded = fetch('${api.url.origin}/').then(r => r.text());
export const answer = (): Promise<string> => loaded;
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const reply = await render('/', 3000);
    const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
    expect({ status: reply.status, settled: reply.settled, pending: checkpoint.pending, r: /data-testid="r"[^>]*>([^<]*)</.exec(reply.html)[1], waited: reply.render >= 150 })
      .toEqual({ status: 200, settled: true, pending: [], r: 'loaded', waited: true });
  } finally {
    api.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// Each shape an answer can take in a render (rt.js `ask`, `send`): a value now (no ticket); a TypeScript answer's
// promise with its call, waited for while its call has I/O or a timer out; a promise with no call, a module source's
// answer forwarded to its implementation, waited for until it settles; a hold, whose call is empty, which waits on a
// person and holds nothing; a stream, until its first message. The other shapes are the table's above.
test('a render waits for a module source\'s answer forwarded to its implementation', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-render-module-')), pkg = join(dir, 'reader'), app = join(dir, 'moduleapp');
  const run = (cmd, args, cwd) => { const r = spawnSync(cmd, args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }); if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${r.stdout}\n${r.stderr}`); };
  try {
    mkdirSync(join(pkg, 'web'), { recursive: true });
    writeFileSync(join(pkg, 'package.json'), JSON.stringify({ name: '@fixture/reader', version: '1.0.0', exports: { '.': './exact.json', './web': './web/exact.json' } }));
    writeFileSync(join(pkg, 'exact.json'), JSON.stringify({ kind: 'interface', name: '@fixture/reader', version: '1.0.0', substitute: 'implementation',
      shapes: { Reading: { level: 'number' } }, names: [{ source: 'reading', answers: 'Reading' }] }));
    writeFileSync(join(pkg, 'web/exact.json'), JSON.stringify({ kind: 'implementation', implements: '@fixture/reader@^1.0', platform: 'web', targets: ['web'], sources: { javascript: ['index.js'] } }));
    writeFileSync(join(pkg, 'web/index.js'), 'export function implementation() { return { reading: () => new Promise(done => setTimeout(() => done({ level: 7 }), 50)) }; }\n');
    run('bun', [resolve(root, 'scripts/exact.mjs'), 'new', app], root);
    writeFileSync(join(app, 'app.contract'), `use reading, Reading from "@fixture/reader"

routes nav
  home "/" render=request

component Fixture
  resource r = reading() as shape Reading else empty()
  view
    text \`\${r.level}\` testId="level"
`);
    rmSync(join(app, 'app.test.contract'), { force: true });
    writeFileSync(join(app, 'app.ts'), "export const grants = '';\nexport const answer = () => { throw new Error('no data source'); };\n");
    const manifest = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8'));
    manifest.dependencies = { ...manifest.dependencies, '@fixture/reader': `file:${pkg}` };
    writeFileSync(join(app, 'package.json'), JSON.stringify(manifest, null, 2));
    run('bun', ['install'], app);
    run('bun', ['exact.mjs', 'web-build', '--out', 'dist'], app);
    const { renderer } = await import('./render.mjs');
    const render = renderer(join(app, 'dist')); render.ahead = false;
    const reply = await render('/', 3000);
    const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
    expect({ status: reply.status, settled: reply.settled, failed: reply.failed, pending: checkpoint.pending }).toEqual({ status: 200, settled: true, failed: [], pending: [] });
    expect(/data-testid="level"[^>]*>([^<]*)</.exec(reply.html)[1]).toBe('7');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 180000);

// Work that outlives the render, on a page that cannot declare a status (so the render does not wait for it): a source
// that answers at once and goes on to a slow fetch whose continuation writes the store. The checkpoint asks the
// reader's page to ask every answer again, and there the write lands and the store's reader shows it.
test('a page whose answered source still has I/O out is asked again by the reader\'s page', async () => {
  const dir = scratch();
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Promise(resolve => setTimeout(() => resolve(new Response('done', { headers: { 'access-control-allow-origin': '*' } })), 300)) });
  let site, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request

component App
  resource started = start() as shape string else empty()
  resource seen = seen() as shape string else empty()
  view
    main
      text started testId="started"
      text seen testId="seen"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'secret.keep k\\nnet.fetch ${api.url.origin}';
export const answer = (name: string, _args: unknown[], store: any): any => {
  if (name === 'start') { void fetch('${api.url.origin}/').then(r => r.text()).then(text => store.set('k', text), () => {}); return 'started'; }
  return store.get('k') ?? 'none';
};
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const reply = await render('/', 3000);
    expect({ status: reply.status, settled: reply.settled, revalidate: reply.html.includes('"revalidate":true'), seen: /data-testid="seen"[^>]*>([^<]*)</.exec(reply.html)[1] })
      .toEqual({ status: 200, settled: true, revalidate: true, seen: 'none' });
    site = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/') return new Response(reply.html, { headers: { 'content-type': 'text/html' } });
      const asset = Bun.file(resolve(dist, path.slice(1)));
      return await asset.exists() ? new Response(asset) : new Response(null, { status: 404 });
    } });
    browser = await open({ host: 'web', url: site.url.href });
    // The store's reader shows the write once the source's fetch, asked again by the reader's page, has answered.
    const seen = () => browser.carrier.evaluate(`document.querySelector('[data-testid="seen"]').textContent`);
    for (let until = Date.now() + 10000; await seen() !== 'done' && Date.now() < until;) { await browser.clock('settle'); await new Promise(r => setTimeout(r, 100)); }
    const journal = await browser.carrier.evaluate('globalThis.exact.journal');
    expect(await seen(), journal.join('\n')).toBe('done');
  } finally {
    await browser?.close(); site?.stop(true); api.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
