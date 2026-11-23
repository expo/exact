// The dev loop's attach bridge (LLP 1012.000 D2, the web row): an agent reads a tab a person has open on this loop.
// The person turns Agent Attach on in the tab (⌥⇧A), which shows a pairing code that works once, for two minutes; the
// agent's driver presents it on the attach channel, and the person allows the connection in the tab before anything
// is read. The channel is the native carriers' wire (host/apple/Sources/ExactKit/AgentAttach.swift): JSON lines over
// an HTTP upgrade to `exact-attach`, a handshake, then `{"request":n,"op":…}` answered `{"request":n,…}`. A request
// goes to its tab on the tab's own `/__dev/page` stream and comes back by POST to `/__dev/attach/reply`. Only this
// server and `attach-dev.js`, the tab's half it serves, carry any of it: no build does.
import { spawnSync } from 'node:child_process';
import { randomBytes, timingSafeEqual } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const VERSION = 1;
export const LIMITS = { pairingMs: 120000, handshakeMs: 10000, approvalMs: 60000, requestMs: 30000, writeMs: 10000, request: 1 << 20, reply: 32 << 20, queue: 64 << 20 };
const LOOPBACK = new Set(['127.0.0.1', '::1', '::ffff:127.0.0.1']);
// Base32 without the letters read as digits: a code a person can read out.
const ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';

/** A pairing code: 100 random bits in five groups of four. */
export function pairingCode(bytes = randomBytes(20)) {
  const chars = [...bytes].map((b) => ALPHABET[b & 31]).join('');
  return chars.match(/.{4}/g).join('-');
}
const normal = (code) => (typeof code === 'string' && code.length <= 64 ? code.toUpperCase().replace(/[\s-]/g, '') : '');

// What an attached tab answers (LLP 1012.000 D3): its reads, each field checked. Input, the clock and everything
// that substitutes for the page's world are refused before the tab hears of them.
const FORMS = {
  tree: { target: 'target', shallow: 'flag', plan: 'flag' },
  state: {},
  logs: { since: 'count', all: 'flag' },
  layout: { id: 'count', plan: 'flag' },
  perf: { target: 'target' },
  tags: {},
  grants: {},
};
const WITHHELD = new Set(['prefer', 'post', 'pick', 'share', 'midi', 'back', 'close', 'resize', 'background']);
const INPUT = new Set(['tap', 'type', 'call', 'called']);
const count = (v) => (Number.isInteger(v) && v >= 0 && v <= 0xffffffff ? v : null);

/** Whether a JSON object's top-level keys are distinct, compared decoded: `JSON.parse` keeps the last of a repeated
 *  key silently. `line` is already known to be one JSON object. */
export function distinctKeys(line) {
  const keys = new Set();
  let depth = 0, keyNext = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"') {
      let j = i + 1;
      while (j < line.length && line[j] !== '"') j += line[j] === '\\' ? 2 : 1;
      if (depth === 1 && keyNext) {
        const key = JSON.parse(line.slice(i, j + 1));
        if (keys.has(key)) return false;
        keys.add(key);
      }
      keyNext = false;
      i = j;
    } else if (c === '{' || c === '[') { depth++; keyNext = depth === 1; }
    else if (c === '}' || c === ']') depth--;
    else if (c === ',' && depth === 1) keyNext = true;
  }
  return true;
}

/** One request line admitted, as the native carriers admit it: `{request, read}` with the read built from checked
 *  fields, or `{refusal}` to send back. */
export function admit(line) {
  const refuse = (request, code, why) => ({ refusal: { request: request ?? null, error: `${code}: ${why}`, refused: code } });
  let object;
  try { object = JSON.parse(line); } catch { return refuse(null, 'malformed', 'a request is one JSON object'); }
  if (!object || typeof object !== 'object' || Array.isArray(object)) return refuse(null, 'malformed', 'a request is one JSON object');
  if (!distinctKeys(line)) return refuse(count(object.request), 'malformed', 'a field is named twice');
  const id = count(object.request);
  if (!id) return refuse(null, 'malformed', 'a request needs a positive integer request number');
  const { op } = object;
  if (typeof op !== 'string') return refuse(id, 'malformed', 'a request names its op');
  if ('session' in object) return refuse(id, 'pinned', 'this connection reads the tab the person allowed; a request names no session');
  if (op === 'clock') return refuse(id, 'live-clock', 'an attached tab keeps its own wall clock: it is never taken, seeked, settled or landed');
  if (op === 'call' && ('door' in object || 'cold' in object)) return refuse(id, 'door', 'the host chooses a call\'s door; a request names none');
  if (op === 'prefer') {
    const substitute = ['faults', 'extension', 'permission', 'midi'].find((k) => k in object);
    if (substitute) return refuse(id, 'unsupported', `prefer ${substitute} substitutes for the app's world, which an attached session never does`);
  }
  if (WITHHELD.has(op)) return refuse(id, 'unsupported', `${op} is not offered to an attached session`);
  if (INPUT.has(op)) return refuse(id, 'unsupported', `${op} is not offered in a browser tab: a tap or a type a page makes is no person's (trusted) input, and an attached tab is read, not driven (LLP 1012.000 D2)`);
  if (op === 'screenshot') return refuse(id, 'unsupported', 'a browser tab cannot take its own picture: that needs a connection the browser authorizes for this tab (LLP 1012.000 D2)');
  const fields = Object.hasOwn(FORMS, op) ? FORMS[op] : null;
  if (!fields) return refuse(id, 'unsupported', `${op} is not offered to an attached session`);
  if (op === 'state' && 'reveal' in object) return refuse(id, 'masked', 'an attached session reads state masked');
  if (op === 'perf') {
    const control = ['frames', 'capture', 'hold', 'track', 'report', 'health', 'live', 'source', 'streams', 'hatches', 'start'].find((k) => k in object);
    if (control) return refuse(id, 'unsupported', `perf ${control} is not offered to an attached tab`);
  }
  const read = { op };
  for (const [key, value] of Object.entries(object)) {
    if (key === 'request' || key === 'op') continue;
    const kind = Object.hasOwn(fields, key) ? fields[key] : null;
    if (!kind) return refuse(id, 'malformed', `${op} takes no field ${key}`);
    if (kind === 'flag') { if (typeof value !== 'boolean') return refuse(id, 'malformed', `${key} is true or false`); }
    else if (kind === 'count') { if (count(value) === null) return refuse(id, 'malformed', `${key} is a whole number up to ${0xffffffff}`); }
    else if (count(value) === null && !(typeof value === 'string' && value && Buffer.byteLength(value) <= 256)) return refuse(id, 'malformed', `${key} is a view id or a short string`);
    read[key] = value;
  }
  return { request: id, read };
}

/** The bridge, for a dev loop on `port`. `stream` and `ended` follow each `/__dev/page` stream; `handle` takes the
 *  tab's routes, `upgrade` the driver's channel. */
export function devAttach({ port, root, limits = LIMITS, log = (line) => console.log(line) }) {
  const hosts = new Set([`127.0.0.1:${port}`, `localhost:${port}`, `[::1]:${port}`]);
  // An actual loopback connection, naming this loop by a loopback name (no other Host: a rebinding name or a
  // tunnel's), with no proxy's mark (`forwarded`, `x-forwarded-*`: the design tool's canvas gateway forwards a page
  // from loopback), so a LAN peer, a forwarded request or a site in the person's browser is never a party.
  const loopback = (req) => LOOPBACK.has(req.socket?.remoteAddress) && hosts.has(req.headers.host)
    && !Object.keys(req.headers).some((k) => k === 'forwarded' || k.startsWith('x-forwarded-'));
  const docs = new Map(), codes = new Map();
  let build;
  const buildOf = () => build ??= (() => {
    const git = (...a) => spawnSync('git', ['-C', root, ...a], { encoding: 'utf8' });
    const commit = git('rev-parse', 'HEAD').stdout?.trim() || null;
    return { commit, dirty: commit ? git('status', '--porcelain', '--untracked-files=no').stdout.trim() !== '' : null, kind: 'dev-loop' };
  })();

  /** A page's stream opened: the tab it serves may be attached, if it came over loopback. Returns its record. */
  function stream(req, write) {
    if (!loopback(req)) return null;
    const doc = { token: randomBytes(18).toString('base64url'), id: randomBytes(6).toString('hex'), code: null, conn: null, open: true };
    doc.write = (frame) => { if (doc.open) write(frame); };
    docs.set(doc.token, doc);
    return doc;
  }
  /** The stream ended: its tab reloaded, navigated, closed or lost the loop. Its pairing and connection end. */
  function ended(doc) {
    if (!doc || !docs.delete(doc.token)) return;
    doc.open = false;
    forget(doc);
    doc.conn?.retire('the tab reloaded, navigated or closed');
  }
  const forget = (doc) => { if (doc.code) codes.delete(doc.code.key); doc.code = null; };

  /** The tab's POSTs: its own origin, over loopback, naming its stream's token. */
  async function handle(req, res, url) {
    if (url.pathname === '/__dev/attach.js' && (req.method === 'GET' || req.method === 'HEAD')) {
      res.writeHead(200, { 'content-type': 'text/javascript; charset=utf-8', 'cache-control': 'no-store' });
      res.end(req.method === 'HEAD' ? undefined : readFileSync(resolve(root, 'host/web-js/attach-dev.js')));
      return true;
    }
    const route = /^\/__dev\/attach\/(enable|answer|reply|detach)$/.exec(url.pathname)?.[1];
    if (!route) return false;
    const answer = (status, body) => { res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' }); res.end(JSON.stringify(body)); };
    if (req.method !== 'POST') { answer(405, { error: 'POST' }); return true; }
    if (!loopback(req) || req.headers.origin !== `http://${req.headers.host}`) { req.resume(); answer(403, { error: 'attach: only the loop\'s own page, over loopback' }); return true; }
    const limit = route === 'reply' ? limits.reply + 4096 : 4096;
    let size = 0, over = false;
    const chunks = [];
    for await (const chunk of req) { size += chunk.length; if (size > limit) over = true; else chunks.push(chunk); }
    if (over) { answer(413, { error: 'too large' }); return true; }
    let body;
    try { body = JSON.parse(Buffer.concat(chunks).toString('utf8')); } catch { answer(400, { error: 'one JSON object' }); return true; }
    const doc = typeof body?.doc === 'string' ? docs.get(body.doc) : null;
    if (!doc) { answer(410, { error: 'this tab\'s stream has ended' }); return true; }
    if (route === 'enable') {
      if (body.driven) { answer(409, { error: 'this tab is the driver\'s (?agent)' }); return true; }
      if (doc.conn) { answer(409, { error: 'an agent is attached to this tab or asking to be' }); return true; }
      forget(doc);
      const code = pairingCode(), key = normal(code);
      doc.code = { key, expires: performance.now() + limits.pairingMs, used: false };
      codes.set(key, doc);
      answer(200, { code, ms: limits.pairingMs });
      return true;
    }
    if (route === 'detach') { forget(doc); doc.conn?.retire('the person chose Detach in the tab'); answer(200, {}); return true; }
    const conn = doc.conn;
    if (!conn || conn.id !== body.conn) { answer(409, { error: 'not this tab\'s connection' }); return true; }
    if (route === 'answer') { answer(conn.answered(body) ? 200 : 409, {}); return true; }
    answer(conn.replied(body) ? 200 : 409, {});
    return true;
  }

  /** The driver's channel: `GET /__dev/attach` upgraded to `exact-attach`, `head` the bytes read past its headers.
   *  False for another path. */
  function upgrade(req, socket, head) {
    if (req.url !== '/__dev/attach') return false;
    const refuse = (status, why) => { socket.end(`HTTP/1.1 ${status}\r\ncontent-type: text/plain\r\nconnection: close\r\n\r\nattach: ${why}\n`); };
    if (!loopback(req)) return refuse('403 Forbidden', `the attach channel takes a connection from this machine (127.0.0.1, ::1) naming ${port} by a loopback name, not ${req.headers.host ?? 'no Host'} from ${req.socket.remoteAddress}`), true;
    // A browser sends an Origin with every upgrade it makes: no page, of this loop or another site, is a driver.
    if (req.headers.origin !== undefined) return refuse('403 Forbidden', `the attach channel takes no browser page (Origin ${req.headers.origin})`), true;
    if (String(req.headers.upgrade).toLowerCase() !== 'exact-attach') return refuse('426 Upgrade Required', 'upgrade to exact-attach'), true;
    socket.write('HTTP/1.1 101 Switching Protocols\r\nupgrade: exact-attach\r\nconnection: upgrade\r\n\r\n');
    connection(socket, head);
    return true;
  }

  function connection(socket, head) {
    const conn = { id: randomBytes(9).toString('base64url'), state: 'handshake', doc: null, outstanding: null, retired: false };
    let pending = Buffer.alloc(0), timer = setTimeout(() => refuseAndClose({ error: `handshake: none arrived within ${limits.handshakeMs / 1000} s`, refused: 'handshake' }), limits.handshakeMs);
    let drain = null;
    const write = (object, final = false) => {
      if (conn.retired && !final) return;
      const line = (typeof object === 'string' ? object : JSON.stringify(object)) + '\n';
      if (socket.writableLength + Buffer.byteLength(line) > limits.queue) return retire(`the agent's unread replies passed ${limits.queue >> 20} MiB`);
      if (!socket.write(line) && !drain) drain = setTimeout(() => retire(`the agent did not read its replies within ${limits.writeMs / 1000} s`), limits.writeMs);
    };
    socket.on('drain', () => { clearTimeout(drain); drain = null; });
    const retire = conn.retire = (why, flush = false) => {
      if (conn.retired) return;
      conn.retired = true;
      clearTimeout(timer); clearTimeout(drain); clearTimeout(conn.outstanding?.timer);
      if (conn.doc?.conn === conn) {
        conn.doc.conn = null;
        // Its code, kept to tell a second driver the tab is taken, is spent with it.
        if (conn.doc.code?.used) forget(conn.doc);
        conn.doc.write({ kind: 'attach-end', conn: conn.id, why });
      }
      // A refusal goes out first and the socket half-closes; else it is shut at once and queued replies are dropped.
      if (flush) { socket.end(); setTimeout(() => socket.destroy(), 2000).unref?.(); } else socket.destroy();
      log(`agent attach ended: ${why}`);
    };
    const refuseAndClose = (object) => { write(object, true); retire(object.error ?? 'refused', true); };
    socket.on('error', () => retire('the agent\'s connection failed'));
    // (Under Bun an upgraded socket reports the driver's hangup as 'end' alone.)
    socket.on('end', () => retire('the agent hung up'));
    socket.on('close', () => retire('the agent hung up'));
    const take = (data) => {
      pending = Buffer.concat([pending, data]);
      let i;
      while (!conn.retired && (i = pending.indexOf(10)) >= 0) {
        const line = pending.subarray(0, i);
        pending = pending.subarray(i + 1);
        if (line.length > limits.request) return retire(`a request over ${limits.request >> 20} MiB`);
        receive(line.toString('utf8'));
      }
      if (pending.length > limits.request) retire(`a request over ${limits.request >> 20} MiB`);
    };
    socket.on('data', take);
    const receive = (line) => {
      if (conn.state === 'handshake') return handshake(line);
      if (conn.state !== 'attached') return retire('a request before the person allowed reads in the tab');
      const { request, read, refusal } = admit(line);
      if (refusal) return write(refusal);
      if (conn.outstanding) return write({ request, error: 'busy: one request at a time', refused: 'busy' });
      if (read.op === 'grants') return write({ request, read: true, input: false });
      conn.outstanding = { request, since: performance.now(), timer: setTimeout(() => retire(`request ${request} was not answered within ${limits.requestMs / 1000} s`), limits.requestMs) };
      conn.doc.write({ kind: 'attach-request', conn: conn.id, request, read });
    };
    const handshake = (line) => {
      let object = null;
      try { object = JSON.parse(line); } catch { /* refused below */ }
      const keys = object && typeof object === 'object' && !Array.isArray(object) ? Object.keys(object) : [];
      if (!keys.length || !keys.every((k) => k === 'attach' || k === 'pairing') || !distinctKeys(line) || object.attach !== VERSION || typeof object.pairing !== 'string') {
        return refuseAndClose({ error: `handshake: expected {"attach":${VERSION},"pairing":"<the code the tab shows>"} as the first line; a tab has one session, which its code chooses`, refused: 'handshake' });
      }
      const key = normal(object.pairing), doc = codes.get(key), code = doc?.code;
      const same = code && Buffer.byteLength(key) === Buffer.byteLength(code.key) && timingSafeEqual(Buffer.from(key), Buffer.from(code.key));
      if (!same || performance.now() > code.expires) {
        return refuseAndClose({ error: 'handshake: no tab on this dev loop shows that pairing code: a code works for one connection, for 2 minutes, while its tab stays loaded; press ⌥⇧A in the tab for a new one', refused: 'handshake' });
      }
      // A used code is kept while its connection lasts, so another driver is told the tab is taken.
      if (code.used) return refuseAndClose({ error: 'busy: another agent is attached to this tab or asking to be (Detach, in the tab, ends it)', refused: 'busy' });
      code.used = true;
      clearTimeout(timer);
      conn.state = 'asking'; conn.doc = doc; conn.asked = performance.now(); doc.conn = conn;
      timer = setTimeout(() => refuseAndClose({ error: `attach: not allowed in the tab within ${limits.approvalMs / 1000} s`, refused: 'declined' }), limits.approvalMs);
      write({ attach: VERSION, waiting: 'approval', document: doc.id });
      doc.write({ kind: 'attach-ask', conn: conn.id });
    };
    // The person's answer, from the tab (its title, location and plan as it said them, which only now are the agent's).
    conn.answered = (body) => {
      if (conn.state !== 'asking' || conn.retired) return false;
      // An Allow its timer has not yet refused, past the deadline all the same, allows nothing.
      if (body.allow === true && performance.now() - conn.asked > limits.approvalMs) { refuseAndClose({ error: `attach: not allowed in the tab within ${limits.approvalMs / 1000} s`, refused: 'declined' }); return false; }
      if (body.allow !== true) { refuseAndClose({ error: 'attach: the person did not allow it', refused: 'declined' }); return true; }
      clearTimeout(timer);
      conn.state = 'attached';
      const text = (v, n) => (typeof v === 'string' ? v.slice(0, n) : null);
      write({ attached: true, document: conn.doc.id, session: 'main', window: text(body.title, 512), location: text(body.location, 2048), grants: { read: true, input: false }, build: buildOf(), planDigest: text(body.planDigest, 128) });
      return true;
    };
    // A read's answer from the tab: the one outstanding, else nothing.
    conn.replied = (body) => {
      const o = conn.outstanding;
      if (conn.retired || conn.state !== 'attached' || !o || body.request !== o.request || !body.reply || typeof body.reply !== 'object' || Array.isArray(body.reply)) return false;
      if (performance.now() - o.since > limits.requestMs) { retire(`request ${o.request} was not answered within ${limits.requestMs / 1000} s`); return false; }
      clearTimeout(o.timer);
      conn.outstanding = null;
      const line = JSON.stringify({ ...body.reply, request: o.request });
      write(Buffer.byteLength(line) > limits.reply ? { request: o.request, error: `the reply is over ${limits.reply >> 20} MiB`, refused: 'reply-size' } : line);
      return true;
    };
    if (head?.length) take(head);
  }

  return { stream, ended, handle, upgrade };
}
