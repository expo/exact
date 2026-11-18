import { test, expect } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

// LLP 1016.000 D11 on the JS target: a WebSocket, event or byte stream with `exactIdleTimeout` ends as a Timeout when
// nothing has arrived for that long, in real time. The peer's socket answers `talk` with a message every 100 ms, `once`
// with one message and `hush` with silence; over HTTP, `ehush` is an event stream's head and then silence, `ekeep` an
// event stream's comment every 100 ms, `bonce` two bytes and then silence, and `btalk` two bytes every 100 ms.
test('a quiet stream ends as a Timeout after what it had accepted; a talking one stays; the driver\'s clock ends none', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/idle-timeout-test-'));
  const talking = new Map(), bodies = new Set();
  const body = (type, first, every) => {
    let interval;
    const stream = new ReadableStream({
      start(c) {
        if (first) c.enqueue(first);
        if (every) interval = setInterval(() => c.enqueue(every), 100);
        bodies.add(() => { clearInterval(interval); try { c.close(); } catch {} });
      },
      cancel() { clearInterval(interval); },
    });
    return new Response(stream, { headers: { 'content-type': type, 'cache-control': 'no-store', 'access-control-allow-origin': '*' } });
  };
  const encode = text => new TextEncoder().encode(text);
  const peer = Bun.serve({ hostname: '127.0.0.1', port: 0, idleTimeout: 0,
    fetch(req, server) {
      const path = new URL(req.url).pathname;
      if (path === '/ehush') return body('text/event-stream', null, null);
      if (path === '/ekeep') return body('text/event-stream', null, encode(': keep\n\n'));
      if (path === '/bonce') return body('application/octet-stream', new Uint8Array([1, 2]), null);
      if (path === '/btalk') return body('application/octet-stream', null, new Uint8Array([1, 2]));
      return server.upgrade(req, { data: { path } }) ? undefined : new Response('websocket only');
    },
    websocket: {
      open(ws) {
        if (ws.data.path === '/talk') { let n = 0; talking.set(ws, setInterval(() => ws.send('tick ' + ++n), 100)); }
        if (ws.data.path === '/once') ws.send('only');
      },
      message() {},
      close(ws) { clearInterval(talking.get(ws)); talking.delete(ws); },
    },
  });
  const url = `ws://127.0.0.1:${peer.port}`;
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Idle', app: { id: 'test.idle', name: 'Idle' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component Idle
  resource hush = feed("hush", 1500) as shape string else empty()
  resource once = feed("once", 1500) as shape string else empty()
  resource talk = feed("talk", 1500) as shape string else empty()
  resource ehush = feed("ehush", 1500) as shape string else empty()
  resource ekeep = feed("ekeep", 1500) as shape string else empty()
  resource bonce = feed("bonce", 1500) as shape string else empty()
  resource btalk = feed("btalk", 1500) as shape string else empty()
  resource upload = feed("upload", 1500) as shape string else empty()
  resource short = feed("hush", 0) as shape string else empty()
  view
    column
      text hush testId="hush"
      text once testId="once"
      text talk testId="talk"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.idle';
export const grants = ${JSON.stringify(`net.websocket ${url}\nnet.fetch http://127.0.0.1:${peer.port}`)};
export function answer(name: string, args: any): any {
  const [path, ms] = args;
  let seen = '';
  const http = path[0] === 'e' || path[0] === 'b' || path === 'upload';
  const target = (http ? ${JSON.stringify(`http://127.0.0.1:${peer.port}`)} : ${JSON.stringify(url)}) + '/' + path;
  const upload = path === 'upload' ? { method: 'POST', body: 'x', exactUploadProgress: true } : {};
  return fetch(target, { ...upload, exactIdleTimeout: ms, exactStream(event: any) {
    if (event.type === 'message') seen += (typeof event.data === 'string' ? event.data.split(' ')[0] : 'bytes') + ',';
    else seen += event.type + (event.kind ? ':' + event.kind + ':' + event.message : '') + ',';
    return seen;
  }});
}
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'idle', '--out', resolve(dir, 'dist'), '--render', 'none'], {
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
    const until = async predicate => {
      const deadline = performance.now() + 15000;
      for (;;) {
        const state = await browser.state();
        if (predicate(state)) return state;
        if (performance.now() > deadline) throw new Error('idle state deadline: ' + JSON.stringify(state.resources) + ' ' + JSON.stringify(state.failed) + '\n' + (await browser.logs()).lines.join('\n'));
        await new Promise(r => setTimeout(r, 50));
      }
    };
    // The driver's clock is not the socket's: a jump of a minute ends nothing.
    await until(s => s.resources.hush === 'open,' || s.resources.hush.includes('error'));
    await browser.clock('+60000');
    expect((await browser.state()).resources.hush).toBe('open,');
    // In real time each quiet one ends, once, as a Timeout; what one had accepted came first. An HTTP stream's head
    // is not counted as received, so the quiet event stream shows nothing before its end.
    const quiet = ['hush', 'once', 'ehush', 'bonce'];
    const ended = await until(s => quiet.every(name => s.resources[name]?.includes('error')));
    const timedOut = 'error:Timeout:nothing arrived for 1500 ms,';
    expect(ended.resources.hush).toBe('open,' + timedOut);
    expect(ended.resources.once).toBe('open,only,' + timedOut);
    expect(ended.resources.ehush).toBe(timedOut);
    expect(ended.resources.bonce).toBe('bytes,' + timedOut);
    expect(ended.streams.map(s => s.name).sort()).toEqual(['btalk', 'ekeep', 'talk']);
    // A message every 100 ms keeps the talking ones open well past their 1500 ms, an event stream's comments too,
    // though its mapper sees none of them.
    await new Promise(r => setTimeout(r, 1800));
    const later = await browser.state();
    expect(later.resources.talk.startsWith('open,tick,tick,tick,')).toBe(true);
    expect(later.resources.btalk.startsWith('bytes,')).toBe(true);
    for (const name of ['talk', 'btalk']) expect(later.resources[name].includes('error')).toBe(false);
    expect(later.resources.ekeep ?? '').toBe('');
    expect(later.streams.map(s => s.name).sort()).toEqual(['btalk', 'ekeep', 'talk']);
    expect(later.resources.hush).toBe(ended.resources.hush);
    // Refused before anything is sent: an upload, and a value out of range.
    expect(later.failed.upload).toContain('exactIdleTimeout: only a WebSocket, event or byte stream takes an idle timeout');
    expect(later.failed.short).toContain('exactIdleTimeout must be an integer number of milliseconds from 1 to 3600000');
  } finally {
    await browser?.close(); server?.stop(true); bodies.forEach(end => end()); peer.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
