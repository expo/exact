import { test, expect } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { open } from '../../scripts/agent.mjs';

// The same module and peer exercise each browser target's request and reply seams.
for (const wasm of [false, true]) test(`socket protocols on the web ${wasm ? 'wasm' : 'JS'} target`, async () => {
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/protocol-test-'));
  const offers = new Map(), wires = new Set();
  const peer = createServer();
  peer.on('upgrade', (req, wire) => {
    wires.add(wire); wire.on('error', () => {}); wire.on('close', () => wires.delete(wire));
    offers.set(req.url, req.headers['sec-websocket-protocol'] ?? '');
    const selected = req.url === '/offered' ? 'chat.v1' : req.url === '/unknown' ? 'unlisted' : '';
    const accept = createHash('sha1').update(req.headers['sec-websocket-key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest('base64');
    wire.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n${selected ? `Sec-WebSocket-Protocol: ${selected}\r\n` : ''}\r\n`);
    wire.write(Buffer.from([0x81, 5, ...Buffer.from('hello')]));
  });
  await new Promise(r => peer.listen(0, '127.0.0.1', r));
  const url = `ws://127.0.0.1:${peer.address().port}`;
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Protocols', app: { id: 'test.protocols', name: 'Protocols' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component Protocols
  resource offered = feed("offered") as shape string else empty()
  resource plain = feed("plain") as shape string else empty()
  resource missing = feed("missing") as shape string else empty()
  resource unknown = feed("unknown") as shape string else empty()
  resource invalid = feed("invalid") as shape string else empty()
  view
    column
      text offered testId="offered"
      text plain
      text missing
      text unknown
      text invalid
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = ${JSON.stringify(`net.websocket ${url}`)};
export function answer(_name: string, [mode]: [string]): any {
  return fetch(${JSON.stringify(url)} + '/' + mode, {
    exactProtocols: mode === 'plain' ? undefined : mode === 'invalid' ? ['bad/name'] : ['chat.v2', 'Chat.v2', 'chat.v1'],
    exactStream(e: any) { return e.type === 'message' ? e.data + ':' + e.protocol
      : e.type === 'error' ? e.type + ': ' + e.kind + ': ' + e.message : e.type; }
  }).catch((e: Error) => e.message);
}
`);
    const dist = resolve(dir, 'dist');
    const build = Bun.spawn([process.execPath, wasm ? 'host/web/build.mjs' : 'host/web-js/build.mjs', 'protocols', ...(wasm ? ['--wasm'] : ['--out', dist, '--render', 'none'])], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir, CARGO_TARGET_DIR: resolve(root, 'target'), EXACT_WEB_DIST: dist }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      const file = Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    browser = await open({ host: 'web', url: server.url.href, webDist: dist });
    await browser.clock('until state offered == "hello:chat.v1" within 15000');
    await browser.clock('until state plain == "hello:" within 15000');
    await browser.clock('until state missing == "error: Network: the socket did not open" within 15000');
    await browser.clock('until state unknown == "error: Network: the socket did not open" within 15000');
    const logs = await browser.logs({ all: true });
    expect(logs.host.filter(line => line.startsWith('exception:'))).toEqual([]);
    const state = await browser.state();
    expect(state.resources.invalid).toContain('bad/name');
    expect(offers.get('/offered').split(',').map(s => s.trim())).toEqual(['chat.v2', 'Chat.v2', 'chat.v1']);
    expect(offers.get('/plain')).toBe('');
    expect(offers.has('/invalid')).toBe(false);
  } catch (error) {
    console.error('socket diagnostics', { offers: [...offers], state: await browser?.state(), logs: await browser?.logs({ all: true }) });
    throw error;
  } finally {
    await browser?.close(); server?.stop(true);
    for (const wire of wires) wire.destroy();
    await new Promise(r => peer.close(r)); rmSync(dir, { recursive: true, force: true });
  }
}, 600000);
