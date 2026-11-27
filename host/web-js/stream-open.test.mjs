import { test, expect } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { createServer } from 'node:net';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

// LLP 1016.000 D12 on the web JS target:
// - `exactStreamOpen: true` on an HTTP(S) event or byte stream delivers one standalone `open` event
//   with `status` and immutable `Headers` before any SSE event or byte chunk.
// - `open` clears `pending` (`clock settle` / initial settlement) without colliding with a server-sent
//   `event: open` (which has `headers === undefined`).
// - `open` does not reset `exactIdleTimeout`.
// - A non-2xx HTTP stream error carries immutable `headers` on its terminal `kind: "Response"` event
//   even when `exactStreamOpen` is omitted.
// - Non-boolean or disallowed combinations reject with `TypeError` before sending.
test('exactStreamOpen delivers status and immutable headers on HTTP streams without resetting idle timeout, and terminal Response errors carry headers', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(root, 'target/stream-open-test-'));
  const bodies = new Set();
  const encode = text => new TextEncoder().encode(text);
  const cors = {
    'access-control-allow-origin': '*',
    'access-control-expose-headers': 'Mcp-Session-Id, X-Dup, WWW-Authenticate',
  };
  const body = (type, chunks, extra = {}) => {
    let timers = [];
    const stream = new ReadableStream({
      start(c) {
        for (const [delay, chunk] of chunks) {
          if (delay === 0) c.enqueue(chunk);
          else timers.push(setTimeout(() => { try { c.enqueue(chunk); } catch {} }, delay));
        }
        bodies.add(() => { timers.forEach(clearTimeout); try { c.close(); } catch {} });
      },
      cancel() { timers.forEach(clearTimeout); },
    });
    return new Response(stream, {
      status: 200,
      headers: { 'content-type': type, 'cache-control': 'no-store', ...cors, ...extra },
    });
  };
  const peer = Bun.serve({
    hostname: '127.0.0.1',
    port: 0,
    idleTimeout: 0,
    fetch(req) {
      const path = new URL(req.url).pathname;
      if (path === '/sse') {
        const headers = new Headers({
          'content-type': 'text/event-stream',
          'cache-control': 'no-store',
          ...cors,
          'Mcp-Session-Id': 'web-sess-1',
        });
        headers.append('X-Dup', 'one');
        headers.append('X-Dup', 'two');
        let timer;
        const stream = new ReadableStream({
          start(c) {
            timer = setTimeout(() => {
              try {
                c.enqueue(encode('event: open\nid: srv-id-1\ndata: srv\n\ndata: hello\n\n'));
              } catch {}
            }, 80);
            bodies.add(() => { clearTimeout(timer); try { c.close(); } catch {} });
          },
          cancel() { clearTimeout(timer); },
        });
        return new Response(stream, { status: 200, headers });
      }
      if (path === '/quiet') {
        return body('text/event-stream', [], { 'Mcp-Session-Id': 'quiet-1' });
      }
      if (path === '/bytes') {
        return body('application/octet-stream', [[80, new Uint8Array([4, 5, 6])]], { 'Mcp-Session-Id': 'bin-1' });
      }
      if (path === '/unauth') {
        return new Response('unauthorized', {
          status: 401,
          headers: {
            'content-type': 'text/plain',
            ...cors,
            'WWW-Authenticate': 'Bearer realm="mcp"',
            'Mcp-Session-Id': 'err-web-1',
          },
        });
      }
      return new Response('not found', { status: 404, headers: cors });
    },
  });
  const sockets = new Set();
  const quietPeer = createServer(socket => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.once('data', () => {
      // Flush HTTP/1.1 headers immediately with zero body bytes so fetch() resolves while the body stays silent.
      socket.write(
        'HTTP/1.1 200 OK\r\n'
        + 'Content-Type: text/event-stream\r\n'
        + 'Cache-Control: no-store\r\n'
        + 'Access-Control-Allow-Origin: *\r\n'
        + 'Access-Control-Expose-Headers: Mcp-Session-Id\r\n'
        + 'Mcp-Session-Id: quiet-1\r\n'
        + 'Transfer-Encoding: chunked\r\n\r\n',
      );
    });
  });
  await new Promise(resolveListen => quietPeer.listen(0, '127.0.0.1', resolveListen));
  const quietBase = `http://127.0.0.1:${quietPeer.address().port}`;
  const base = `http://127.0.0.1:${peer.port}`;
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({
      name: 'StreamOpen',
      app: { id: 'test.streamopen', name: 'StreamOpen' },
      host: { web: {} },
    }));
    writeFileSync(resolve(dir, 'app.contract'), `component StreamOpen
  resource sse = feed("sse") as shape string else empty()
  resource quiet = feed("quiet") as shape string else empty()
  resource bytes = feed("bytes") as shape string else empty()
  resource unauth = feed("unauth") as shape string else empty()
  resource unauthOpen = feed("unauth-open") as shape string else empty()
  resource badBool = feed("bad-bool") as shape string else empty()
  resource noStream = feed("no-stream") as shape string else empty()
  resource ws = feed("ws") as shape string else empty()
  resource upload = feed("upload") as shape string else empty()
  resource poll = feed("poll") as shape string else empty()
  view
    column
      text sse testId="sse"
      text quiet testId="quiet"
      text bytes testId="bytes"
      text unauth testId="unauth"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.streamopen';
export const grants = ${JSON.stringify(`net.fetch ${base}\nnet.fetch ${quietBase}\nnet.websocket ws://127.0.0.1:${peer.port}`)};
function imm(h: Headers): string {
  const out: string[] = [];
  for (const op of ['set', 'append', 'delete'] as const) {
    try {
      if (op === 'delete') h.delete('x');
      else (h as any)[op]('x', '1');
      out.push(op + ':ok');
    } catch (e: any) {
      out.push(op + ':' + e.name);
    }
  }
  return out.join(',');
}
export function answer(_name: string, args: any): any {
  const [kind] = args;
  const base = ${JSON.stringify(base)};
  const quietBase = ${JSON.stringify(quietBase)};
  if (kind === 'bad-bool') return fetch(base + '/sse', { exactStream: () => '', exactStreamOpen: 'yes' as any });
  if (kind === 'no-stream') return fetch(base + '/sse', { exactStreamOpen: true });
  if (kind === 'ws') return fetch(${JSON.stringify(`ws://127.0.0.1:${peer.port}/ws`)}, { exactStream: () => '', exactStreamOpen: true });
  if (kind === 'upload') return fetch(base + '/sse', { method: 'POST', body: 'x', exactStream: () => '', exactUploadProgress: true, exactStreamOpen: true });
  if (kind === 'poll') return fetch(base + '/sse', { exactStream: () => '', exactLongPoll: { cursor: 'since', from: 'seq' }, exactStreamOpen: true });
  let seen = '';
  const extra: any =
    kind === 'quiet' ? { exactStreamOpen: true, exactIdleTimeout: 1500 }
    : kind === 'unauth' ? {}
    : { exactStreamOpen: true };
  const target = kind === 'quiet' ? quietBase + '/quiet' : base + '/' + (kind === 'unauth-open' ? 'unauth' : kind);
  return fetch(target, {
    ...extra,
    exactStream(e: any) {
      if (e.type === 'open' && e.headers !== undefined) {
        seen += 'head:' + e.status + ':' + e.headers.get('Mcp-Session-Id') + ':' + (e.headers.get('x-dup') ?? '') + ':' + imm(e.headers) + '|';
      } else if (e.type === 'open') {
        seen += 'srv-open:' + e.data + ':' + e.lastEventId + ':' + (e.headers === undefined) + '|';
      } else if (e.type === 'message') {
        seen += (typeof e.data === 'string' ? 'msg:' + e.data : 'bytes:' + e.data.byteLength) + '|';
      } else if (e.type === 'error') {
        seen += 'err:' + e.kind + ':' + (e.status ?? '') + ':' + (e.headers ? e.headers.get('WWW-Authenticate') + ':' + e.headers.get('Mcp-Session-Id') + ':' + imm(e.headers) : 'none') + ':' + (e.data ?? '') + '|';
      }
      return seen;
    },
  });
}
`);
    const build = Bun.spawn(
      [process.execPath, 'host/web-js/build.mjs', 'streamopen', '--out', resolve(dir, 'dist'), '--render', 'none'],
      { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe' },
    );
    const [status, stdout, stderr] = await Promise.all([
      build.exited,
      new Response(build.stdout).text(),
      new Response(build.stderr).text(),
    ]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({
      hostname: '127.0.0.1',
      port: 0,
      async fetch(req) {
        const path = new URL(req.url).pathname;
        const file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
        return (await file.exists()) ? new Response(file) : new Response('not found', { status: 404 });
      },
    });
    browser = await open({ host: 'web', url: server.url.href, webDist: resolve(dir, 'dist') });
    const until = async predicate => {
      const deadline = performance.now() + 15000;
      for (;;) {
        const state = await browser.state();
        if (predicate(state)) return state;
        if (performance.now() > deadline) {
          throw new Error('stream-open deadline: ' + JSON.stringify(state.resources) + ' ' + JSON.stringify(state.failed) + '\n' + (await browser.logs()).lines.join('\n'));
        }
        await new Promise(r => setTimeout(r, 50));
      }
    };
    const immStr = 'set:TypeError,append:TypeError,delete:TypeError';
    // Quiet stream delivers `open` first, clearing pending before any body bytes AND before idle timeout!
    const opened = await until(s => s.resources.quiet?.startsWith('head:200:quiet-1:') && s.resources.sse?.includes('msg:hello|') && s.resources.bytes?.includes('bytes:3|') && s.resources.unauth?.includes('err:Response:401:') && s.resources.unauthOpen?.includes('err:Response:401:'));
    expect(opened.resources.quiet).toBe(`head:200:quiet-1::${immStr}|`);
    expect(opened.resources.sse).toBe(`head:200:web-sess-1:one, two:${immStr}|srv-open:srv:srv-id-1:true|msg:hello|`);
    expect(opened.resources.bytes).toBe(`head:200:bin-1::${immStr}|bytes:3|`);
    expect(opened.resources.unauth).toBe(`err:Response:401:Bearer realm="mcp":err-web-1:${immStr}:unauthorized|`);
    expect(opened.resources.unauthOpen).toBe(`err:Response:401:Bearer realm="mcp":err-web-1:${immStr}:unauthorized|`);
    expect(opened.pending).toEqual([]);

    const timedOut = await until(s => s.resources.quiet?.includes('err:Timeout:'));
    expect(timedOut.resources.quiet).toBe(`head:200:quiet-1::${immStr}|err:Timeout:0:none:|`);
    expect(timedOut.failed.badBool).toContain('exactStreamOpen is true or false');
    expect(timedOut.failed.noStream).toContain("exactStreamOpen reports an HTTP stream's response head on open: give exactStream too");
    expect(timedOut.failed.ws).toContain('exactStreamOpen: only an HTTP event or byte stream takes exactStreamOpen');
    expect(timedOut.failed.upload).toContain('exactStreamOpen: only an HTTP event or byte stream takes exactStreamOpen');
    expect(timedOut.failed.poll).toContain('exactStreamOpen: only an HTTP event or byte stream takes exactStreamOpen');
  } finally {
    await browser?.close();
    server?.stop(true);
    bodies.forEach(end => end());
    sockets.forEach(s => s.destroy());
    quietPeer.close();
    peer.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
