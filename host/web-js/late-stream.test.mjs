import { test, expect } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

// LLP 1016.000, amended 2026-11-12 (Follows #82): on the JS target, as natively, an answer may open its stream in a
// later turn: after an `await` in its async function (a token fetched first) or in its own timer. One let go while it
// waited opens nothing, and an answer still streams one request.
test('an answer opens its stream after an await or in its timer, and not once let go', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/late-stream-test-'));
  // The peer: `/token?n=` answers when the test releases it; a socket says `hello <path>` and counts its connections.
  const held = new Map(), connections = new Map();
  const release = n => { const go = held.get(n); held.delete(n); go?.(); return !!go; };
  const peer = Bun.serve({ hostname: '127.0.0.1', port: 0, idleTimeout: 60,
    async fetch(req, server) {
      const url = new URL(req.url);
      if (url.pathname === '/token') {
        await new Promise(go => held.set(url.searchParams.get('n'), go));
        return new Response('t' + url.searchParams.get('n'), { headers: { 'access-control-allow-origin': '*' } });
      }
      return server.upgrade(req, { data: { path: url.pathname + url.search } }) ? undefined : new Response('websocket only');
    },
    websocket: {
      open(ws) { connections.set(ws.data.path, (connections.get(ws.data.path) ?? 0) + 1); ws.send('hello ' + ws.data.path); },
      message() {},
    },
  });
  const ws = `ws://127.0.0.1:${peer.port}`, http = `http://127.0.0.1:${peer.port}`;
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Late', app: { id: 'test.late', name: 'Late' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component Late
  state n = 0
  resource timed = timed() as shape string else empty()
  resource token = token(n) as shape string else empty()
  resource twice = twice() as shape string else empty()
  action bump
    n = n + 1
  view
    column
      text timed testId="timed"
      text token testId="token"
      text twice testId="twice"
      button press=bump testId="bump"
        text "Bump"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.late';
export const grants = ${JSON.stringify(`net.websocket ${ws}\nnet.fetch ${http}`)};
const mapped = (event: any) => event.type === 'message' ? event.data : event.type;
export async function answer(name: string, args: any): Promise<any> {
  if (name === 'timed') {
    await new Promise(resolve => setTimeout(resolve, 1000));
    return fetch(${JSON.stringify(ws)} + '/timed', { exactStream: mapped });
  }
  if (name === 'token') {
    // Held: the server holds it until the test releases it, and no clock waits on it (LLP 1016.000 D9).
    const token = await (await fetch(${JSON.stringify(http)} + '/token?n=' + args[0], { exactHeld: true })).text();
    return fetch(${JSON.stringify(ws)} + '/with?' + token, { exactStream: mapped });
  }
  await Promise.resolve();
  fetch(${JSON.stringify(ws)} + '/twice', { exactStream: mapped });
  return fetch(${JSON.stringify(ws)} + '/again', { exactStream: mapped });
}
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'late', '--out', resolve(dir, 'dist'), '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      const file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    browser = await open({ host: 'web', url: server.url.href, webDist: resolve(dir, 'dist') });
    const until = async (predicate, what) => {
      const deadline = performance.now() + 15000;
      for (;;) {
        const state = await browser.state();
        if (predicate(state)) return state;
        if (performance.now() > deadline) throw new Error(`${what}: ` + JSON.stringify({ resources: state.resources, failed: state.failed, streams: state.streams }) + ' peer: ' + JSON.stringify([...connections]) + '\n' + (await browser.logs()).lines.join('\n'));
        await new Promise(r => setTimeout(r, 50));
      }
    };
    // The timer is the answer's, on the driver's clock: a second later its stream opens and hears the peer.
    await until(s => held.has('0'), 'the first token was not asked');
    expect(connections.get('/timed')).toBeUndefined();
    await browser.clock('+1000');
    const timed = await until(s => s.resources.timed === 'hello /timed', 'the timed stream did not open');
    expect(timed.streams.map(s => s.name)).toContain('timed');
    // An answer streams one request: its second is refused, and the first stands.
    expect(timed.resources.twice).toBe('hello /twice');
    expect(connections.get('/again')).toBeUndefined();
    // A token fetched first, then the stream with it. The answer for n = 0 is let go while it waits for its token:
    // given it, it opens nothing; the answer for n = 1 opens its stream.
    await browser.tap('bump');
    await until(s => held.has('1'), 'the second token was not asked');
    expect(release('0')).toBe(true);
    expect(release('1')).toBe(true);
    const token = await until(s => s.resources.token === 'hello /with?t1', 'the token stream did not open');
    expect(token.streams.filter(s => s.name === 'token')).toHaveLength(1);
    expect(connections.get('/with?t0')).toBeUndefined();
    expect(connections.get('/with?t1')).toBe(1);
  } finally {
    await browser?.close(); server?.stop(true); peer.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// An upload (`exactUploadProgress`, LLP 1016.000 D10) opened after an `await` is an upload as one its source opens:
// marked, so a refresh after it finished sends nothing again, and one the stream's opening would refuse (a
// `ReadableStream` body) rejects its `fetch`, failing the answer rather than leaving it pending.
test('an upload opened after an await is marked as asked, and one refused fails its answer', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/late-upload-test-'));
  const cors = { 'access-control-allow-origin': '*', 'access-control-allow-methods': 'GET, POST', 'access-control-allow-headers': '*' };
  const posts = { upload: 0 };
  const peer = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    if (req.method === 'OPTIONS') return new Response(null, { status: 204, headers: cors });
    const path = new URL(req.url).pathname;
    if (path === '/token') return new Response('t1', { headers: cors });
    posts.upload++;
    return new Response(String((await req.arrayBuffer()).byteLength), { status: 201, headers: cors });
  } });
  const http = `http://127.0.0.1:${peer.port}`;
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Late Upload', app: { id: 'test.lateupload', name: 'Late Upload' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component LateUpload
  resource sent = upload() as shape string else empty()
  resource refused = refused() as shape string else empty()
  action retry
    refresh sent
  view
    column
      text sent testId="sent"
      text refused testId="refused"
      button press=retry testId="retry"
        text "Retry"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.lateupload';
export const grants = ${JSON.stringify(`net.fetch ${http}`)};
export async function answer(name: string): Promise<any> {
  const token = await (await fetch(${JSON.stringify(http)} + '/token')).text();
  const body = name === 'upload' ? new Uint8Array(4096).fill(7) : new ReadableStream({ start(c) { c.enqueue(new Uint8Array(1)); c.close(); } });
  return fetch(${JSON.stringify(http)} + '/upload?' + token, { method: 'POST', body, exactUploadProgress: true,
    exactStream: (e: any) => e.type === 'load' ? 'load ' + e.status + ' ' + e.data : e.type } as RequestInit);
}
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'late-upload', '--out', resolve(dir, 'dist'), '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      const file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    browser = await open({ host: 'web', url: server.url.href, webDist: resolve(dir, 'dist') });
    expect((await browser.clock('settle')).settled).not.toBe(false);
    const done = await browser.state();
    expect(done.resources.sent).toBe('load 201 4096');
    expect(done.failed?.refused).toContain('ReadableStream request bodies are unavailable');
    expect(done.pending.map(p => p.name)).toEqual([]);
    expect(posts.upload).toBe(1);
    // Asked once for its arguments: a refresh after it finished sends nothing again.
    await browser.tap('retry');
    expect((await browser.clock('settle')).settled).not.toBe(false);
    expect(posts.upload).toBe(1);
    expect((await browser.state()).resources.sent).toBe('load 201 4096');
  } finally {
    await browser?.close(); server?.stop(true); peer.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
