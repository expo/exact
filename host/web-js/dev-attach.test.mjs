// The dev loop's attach bridge (dev-attach.mjs) between a driver's channel and a tab's stream, the tab played by
// this test: pairing, the person's answer, one outstanding read and its correlated reply, and revocation when the
// tab's stream ends. The browser half and a real tab are host/web-js/dev-page-streams.test.mjs's.
import { afterEach, expect, test } from 'bun:test';
import { createServer } from 'node:http';
import { connect } from 'node:net';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { admit, devAttach } from './dev-attach.mjs';
import { attachChannel } from '../../scripts/agent-attach.mjs';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const closing = [];
afterEach(() => { for (const c of closing.splice(0)) c(); });

/** A loop with the bridge alone: `/__dev/page` streams frames to `frames`, each stream's token resolved by `tab()`. */
async function loop(limits) {
  const server = createServer();
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const port = server.address().port, streams = [];
  const attach = devAttach({ port, root: ROOT, log: () => {}, ...(limits ? { limits } : {}) });
  server.on('request', async (req, res) => {
    const url = new URL(req.url, 'http://x');
    if (url.pathname === '/__dev/page') {
      res.writeHead(200, { 'content-type': 'text/event-stream' });
      const frames = [], doc = attach.stream(req, (f) => frames.push(f));
      streams.push({ token: doc?.token ?? null, frames, end: () => { attach.ended(doc); res.end(); } });
      res.write('data: {}\n\n');
      return;
    }
    if (!(await attach.handle(req, res, url))) { res.writeHead(404); res.end(); }
  });
  server.on('upgrade', (req, socket, head) => { if (!attach.upgrade(req, socket, head)) socket.destroy(); });
  closing.push(() => { server.closeAllConnections(); server.close(); });
  const base = `http://127.0.0.1:${port}`;
  const tab = async () => {
    const before = streams.length;
    fetch(`${base}/__dev/page`).catch(() => {});
    while (streams.length === before) await new Promise((ok) => setTimeout(ok, 5));
    const s = streams.at(-1);
    s.post = (route, body) => fetch(`${base}/__dev/attach/${route}`, { method: 'POST', headers: { origin: base }, body: JSON.stringify({ doc: s.token, ...body }) });
    s.frame = async (kind) => { for (;;) { const f = s.frames.find((x) => x.kind === kind); if (f) { s.frames.splice(s.frames.indexOf(f), 1); return f; } await new Promise((ok) => setTimeout(ok, 5)); } };
    return s;
  };
  return { port, base, tab };
}

/** A driver's channel: lines out, lines in, '<closed>' once it ends. */
async function driver(port) {
  const socket = await attachChannel(port), lines = [];
  closing.push(() => socket.destroy());
  let buf = '';
  socket.on('data', (d) => { buf += d; let i; while ((i = buf.indexOf('\n')) >= 0) { lines.push(JSON.parse(buf.slice(0, i))); buf = buf.slice(i + 1); } });
  socket.on('end', () => lines.push('<closed>'));
  socket.on('close', () => lines.push('<closed>'));
  return {
    send: (o) => socket.write((typeof o === 'string' ? o : JSON.stringify(o)) + '\n'),
    next: async () => { while (!lines.length) await new Promise((ok) => setTimeout(ok, 5)); return lines.shift(); },
  };
}

test('a tab is read once its person allows it, one request at a time, until its stream ends', async () => {
  const { port, tab } = await loop();
  const page = await tab();
  const { code } = await (await page.post('enable', {})).json();
  expect(code).toMatch(/^[A-Z2-9]{4}(-[A-Z2-9]{4}){4}$/);
  const d = await driver(port);
  // As typed: case and spacing are the person's.
  d.send({ attach: 1, pairing: code.toLowerCase().replaceAll('-', ' ') });
  expect(await d.next()).toMatchObject({ waiting: 'approval' });
  const { conn } = await page.frame('attach-ask');
  // A second driver with the same code, while the first is asked about.
  const e = await driver(port);
  e.send({ attach: 1, pairing: code });
  expect((await e.next()).refused).toBe('busy');
  expect(await (await page.post('answer', { conn, allow: true, title: 'Notes', location: '/n' })).status).toBe(200);
  expect(await d.next()).toMatchObject({ attached: true, window: 'Notes', location: '/n', grants: { read: true, input: false } });
  d.send({ request: 1, op: 'tree', target: 'note' });
  const ask = await page.frame('attach-request');
  expect(ask.read).toEqual({ op: 'tree', target: 'note' });
  // Busy while it is out; a reply to another request, or from another stream, answers nothing.
  d.send({ request: 2, op: 'state' });
  expect(await d.next()).toMatchObject({ request: 2, refused: 'busy' });
  expect((await page.post('reply', { conn, request: 2, reply: { nodes: [] } })).status).toBe(409);
  const other = await tab();
  expect((await other.post('reply', { conn, request: 1, reply: { nodes: [] } })).status).toBe(409);
  expect((await page.post('reply', { conn, request: 1, reply: { nodes: [7] } })).status).toBe(200);
  expect(await d.next()).toEqual({ nodes: [7], request: 1 });
  d.send({ request: 3, op: 'grants' });
  expect(await d.next()).toEqual({ request: 3, read: true, input: false });
  // The code still names its tab, which is taken.
  const g = await driver(port);
  g.send({ attach: 1, pairing: code });
  expect((await g.next()).refused).toBe('busy');
  // The tab reloads: its connection ends, and its code with it.
  page.end();
  expect(await d.next()).toBe('<closed>');
  const f = await driver(port);
  f.send({ attach: 1, pairing: code });
  expect((await f.next()).refused).toBe('handshake');
});

test('a request before the Allow ends the connection; Don\'t Allow and a late approval refuse it; a lost reply retires it', async () => {
  const { port, tab } = await loop({ pairingMs: 60000, handshakeMs: 2000, approvalMs: 300, requestMs: 300, writeMs: 2000, request: 1 << 20, reply: 32 << 20, queue: 64 << 20 });
  const page = await tab();
  const open = async () => {
    page.frames.length = 0;
    const { code } = await (await page.post('enable', {})).json();
    const d = await driver(port);
    d.send({ attach: 1, pairing: code });
    await d.next();
    return { d, conn: (await page.frame('attach-ask')).conn };
  };
  let { d } = await open();
  d.send({ request: 1, op: 'tree' });
  expect(await d.next()).toBe('<closed>');
  expect(await page.frame('attach-end')).toMatchObject({ why: 'a request before the person allowed reads in the tab' });
  let conn;
  ({ d, conn } = await open());
  await page.post('answer', { conn, allow: false });
  expect((await d.next()).refused).toBe('declined');
  ({ d } = await open());
  expect((await d.next()).refused).toBe('declined');
  ({ d, conn } = await open());
  await page.post('answer', { conn, allow: true });
  await d.next();
  d.send({ request: 1, op: 'logs' });
  expect(await d.next()).toBe('<closed>');
  expect((await page.frame('attach-end')).why).toMatch(/not answered within/);
});

test('only this machine names the loop by a loopback name, and no browser page drives it', async () => {
  const { port, base } = await loop();
  const upgrade = (headers) => new Promise((ok) => {
    const s = connect(port, '127.0.0.1');
    let got = '';
    s.on('data', (d) => { got += d; });
    s.on('close', () => ok(got));
    s.write(`GET /__dev/attach HTTP/1.1\r\n${Object.entries({ Host: `127.0.0.1:${port}`, Connection: 'Upgrade', Upgrade: 'exact-attach', ...headers }).map(([k, v]) => `${k}: ${v}`).join('\r\n')}\r\n\r\n`);
  });
  expect(await upgrade({ Origin: base })).toMatch(/^HTTP\/1\.1 403[^]*no browser page/);
  expect(await upgrade({ Host: `rebound.example:${port}` })).toMatch(/^HTTP\/1\.1 403[^]*loopback name/);
  expect(await upgrade({ Upgrade: 'websocket' })).toMatch(/^HTTP\/1\.1 426/);
  expect(await upgrade({ 'X-Forwarded-For': '203.0.113.9' })).toMatch(/^HTTP\/1\.1 403/);
  expect(await upgrade({ Forwarded: 'for="_exact-canvas"' })).toMatch(/^HTTP\/1\.1 403/);
  // A handshake sent with the upgrade's headers, in one write, is read.
  const early = await new Promise((ok) => {
    const s = connect(port, '127.0.0.1');
    let got = '';
    s.on('data', (d) => { got += d; if (got.includes('refused')) { s.destroy(); ok(got); } });
    s.write(`GET /__dev/attach HTTP/1.1\r\nHost: 127.0.0.1:${port}\r\nConnection: Upgrade\r\nUpgrade: exact-attach\r\n\r\n{"attach":1,"pairing":"AAAA"}\n`);
  });
  expect(early).toContain('"refused":"handshake"');
  expect((await fetch(`${base}/__dev/attach/enable`, { method: 'POST', headers: { origin: 'http://elsewhere.example' }, body: '{}' })).status).toBe(403);
  expect((await fetch(`${base}/__dev/attach/enable`, { method: 'POST', headers: { origin: base }, body: '{"doc":"made-up"}' })).status).toBe(410);
});

test('a read is admitted only from checked fields', () => {
  const refused = (o) => admit(typeof o === 'string' ? o : JSON.stringify(o)).refusal?.refused;
  expect(admit('{"request":4,"op":"layout","id":3,"plan":true}')).toEqual({ request: 4, read: { op: 'layout', id: 3, plan: true } });
  expect(refused({ request: 1, op: 'screenshot' })).toBe('unsupported');
  for (const op of ['tap', 'type', 'call', 'called', 'prefer', 'resize', 'quit', 'reveal', 'launchEntry']) expect(refused({ request: 1, op })).toBe('unsupported');
  expect(refused({ request: 1, op: 'call', door: 'mcp' })).toBe('door');
  expect(refused({ request: 1, op: 'clock', settle: true })).toBe('live-clock');
  expect(refused({ request: 1, op: 'tree', session: 'b' })).toBe('pinned');
  expect(refused({ request: 1, op: 'state', reveal: true })).toBe('masked');
  expect(refused({ request: 1, op: 'perf', frames: true })).toBe('unsupported');
  expect(refused({ request: 1, op: 'layout', id: 2 ** 32 })).toBe('malformed');
  expect(refused({ request: 1, op: 'layout', picture: true })).toBe('malformed');
  expect(refused('{"request":1,"op":"tree","op":"state"}')).toBe('malformed');
  expect(refused('{"request":1,"op":"tree","\\u006fp":"state"}')).toBe('malformed');
  expect(refused({ request: 0, op: 'tree' })).toBe('malformed');
  expect(refused('[1]')).toBe('malformed');
  // A name every object inherits is no form and no field.
  expect(refused({ request: 1, op: 'constructor' })).toBe('unsupported');
  expect(refused({ request: 1, op: 'tree', toString: 'x' })).toBe('malformed');
});
