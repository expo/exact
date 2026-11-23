// The native carriers' wire, shared by the stdio, simulator and phone
// carriers of `agent.mjs`: JSON lines over a duplex, and why an app that
// stopped answering stopped.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { crashSummary } from '../host/apple/devices.mjs';
import { MalformedOperation } from './agent-keys.mjs';

/** simctl can inherit a console FIFO left by an interrupted launch. Retry that exit once. */
export function simulatorConsole(spawn, hostLines, onProcess) {
  const launch = { process: null, done: false, exited: null, pid: null };
  let retried = false, output = '';
  const start = () => {
    launch.done = false; launch.pid = null; output = '';
    const child = launch.process = spawn();
    onProcess?.(child);
    for (const stream of [child.stdout, child.stderr]) stream.on('data', data => {
      output = (output + String(data)).slice(-8192);
      for (const line of String(data).split('\n')) {
        // simctl's first line names the app's process, `<bundle id>: <pid>`.
        const pid = /^[\w.-]+: (\d+)$/.exec(line)?.[1];
        if (pid && launch.pid == null) launch.pid = Number(pid);
        if (line && !/^com\.exact\.\w+: \d+$/.test(line)) hostLines.push('app: ' + line);
      }
    });
    launch.exited = new Promise(resolve => child.on('close', (code, signal) => { launch.done = true; resolve({ code, signal }); }));
  };
  launch.retry = () => {
    if (!launch.done || retried || !/Unable to establish FIFO[\s\S]*launch_console[\s\S]*File exists/i.test(output)) return false;
    retried = true; start(); return true;
  };
  start(); return launch;
}

/** What an op answers while the app is out of the foreground: it is suspended, and answers nothing until it is back. */
export const AWAY = 'the app left the foreground (a share extension or another app took the screen), so it is suspended and every op fails until it is back; `logs` has the host lines up to then';

/** JSON lines over a duplex: each request is answered by the next line the app writes; unmatched lines are host output (`hostLines`). `fail` rejects every pending request (the app is gone).
 * A line the app pushes unasked (`{"pushed":"background"|"foreground"}`, LLP 1143 D1) answers nothing: leaving fails the
 * pending requests at once and every later one until the app is back, and the replies they were owed are dropped when
 * they come. */
export function jsonLines(readable, writable, hostLines) {
  const waiting = [], scene = new EventTarget();
  let failure = null, away = false, owed = 0;
  // A large reply (a `state` carrying a 16 MiB surface record) arrives in
  // many chunks: hold them and look for the newline in each new chunk only.
  let parts = [];
  readable.setEncoding('utf8');
  readable.on('data', (d) => {
    let i;
    while ((i = d.indexOf('\n')) >= 0) {
      parts.push(d.slice(0, i));
      const line = parts.join('');
      parts = [];
      d = d.slice(i + 1);
      if (line.startsWith('{"pushed":')) {
        away = line.includes('"background"');
        if (away) { owed += waiting.length; for (const w of waiting.splice(0)) w.reject(new Error(AWAY)); }
        scene.dispatchEvent(new Event('change'));
        hostLines.push(`app: ${away ? 'left the foreground' : 'back in the foreground'}`);
        continue;
      }
      if (owed) { owed--; hostLines.push('app: a reply owed to an op that failed while the app was away: ' + line.slice(0, 200)); continue; }
      const w = waiting.shift();
      if (!w) { hostLines.push('app: ' + line); continue; }
      try { w.resolve(JSON.parse(line)); } catch { w.reject(new Error('unreadable reply: ' + line)); }
    }
    if (d) parts.push(d);
  });
  const next = () => failure ? Promise.reject(failure) : new Promise((resolve, reject) => waiting.push({ resolve, reject }));
  return {
    next,
    get failure() { return failure; },
    async waitForScene(background, ms, onTimeout) {
      // The notification may have arrived while the OS launch command was running.
      let changed;
      try {
        if (!failure && away !== background) {
          const seen = new Promise(resolve => {
            changed = () => { if (failure || away === background) resolve(); };
            scene.addEventListener('change', changed);
          });
          await waitAtMost(seen, ms, onTimeout);
        }
        if (failure) throw failure;
        return away === background;
      } finally { if (changed) scene.removeEventListener('change', changed); }
    },
    ask: (req) => {
      if (away && !failure) return Promise.reject(new Error(AWAY));
      const p = next(); if (!failure) writable.write(JSON.stringify(req) + '\n'); return p.then(MalformedOperation.reply);
    },
    fail: (why) => { failure ??= new Error(why); for (const w of waiting.splice(0)) w.reject(failure); scene.dispatchEvent(new Event('change')); },
  };
}
/** Switch iOS away and back outside the app. The app cannot answer a request while suspended. */
export async function backgroundIOS(lines, ms, app, activate) {
  const wait = background => lines.waitForScene(background, 20000, () => {
    throw new Error(`background: the original app did not report ${background ? 'leaving' : 'returning to'} the foreground within 20 s`);
  });
  try {
    await activate('com.apple.Preferences');
    await wait(true);
    if (await lines.waitForScene(false, ms)) throw new Error('background: the app returned before the requested interval ended');
  } finally {
    // An ordinary activation after process death would start a different session.
    if (lines.failure) throw lines.failure;
    await activate(app);
    await wait(false);
  }
  return { backgroundMs: ms, foreground: true };
}

/** Activate an installed bundle without terminating its running process. */
export function activateIOS(args, bundle) {
  return promisify(execFile)('xcrun', [...args, bundle], { timeout: 20000 });
}

/** Why a native app stopped answering, for the error that ends the run: its pid alive or gone, how it exited, any crash report since launch, and the last 20 lines it wrote — not all of them since the last `logs`. */
export function hangup({ what, pid = null, exit = null, reports = [], hostLines = [] }) {
  const alive = pid == null ? null : (() => { try { process.kill(pid, 0); return true; } catch { return false; } })();
  // simctl reports 0 for an app it launched that then crashed (EXC_BAD_ACCESS): an end no close asked for is said as one.
  const status = exit ? (exit.signal ? `killed by ${exit.signal}` : exit.code === 0 ? 'it ended without a close being asked for; simctl reports exit 0, which a crash also gives' : `exit code ${exit.code}`) : alive == null ? 'no pid known' : `pid ${pid} ${alive ? 'still running' : 'gone'}`;
  return [`${what} (${status})`, ...reports.slice(0, 1).flatMap((f) => crashSummary(f)), ...reports.slice(1).map((f) => `crash report: ${f}`), ...hostLines.slice(-20)].join('\n');
}

/** A native reply's deadline: past `clock settle`'s own 20 s bound, so only a wedged app reaches it (a phone has 45 s). */
export const REPLY_MS = 120000;
/** A refused boot; a format refusal usually means a host binary built before the plan's format changed. */
export const bootRefusal = (error) => 'the app booted with an error: ' + error +
  (/UnsupportedVersion|FormatDigestMismatch|KernelSchemaMismatch/.test(error) ? ' (the plan and the host binary were built from different formats: rebuild the host)' : '');

// Release completed operations' deadlines so a closed driver does not wait for a losing timeout.
export async function waitAtMost(operation, ms, onTimeout) {
  let timer;
  const deadline = new Promise(resolve => { timer = setTimeout(resolve, ms); }).then(onTimeout);
  try { return await Promise.race([operation, deadline]); }
  finally { clearTimeout(timer); }
}

/** An attached session's wire (LLP 1012.000 D1): the handshake, then one request at a time, each reply matched by its
 * `request` number (`id` names a view on this wire). A request past its deadline, a reply over 32 MiB or a reply for
 * another number ends the connection, so a late reply never answers a later request. `onWaiting` hears that the app is asking its person. */
export async function attachWire(socket, { nonce, session, onWaiting, handshakeMs = 15000, approvalMs = 65000, requestMs = 35000, lineLimit = 32 << 20 }) {
  const lines = [], waiters = [];
  let failure = null, parts = [], size = 0, attached = false;
  const fail = (why) => {
    if (failure) return;
    failure = why instanceof Error ? why : new Error(why);
    socket.destroy();
    for (const w of waiters.splice(0)) w.reject(failure);
  };
  // Bytes, so a line's length is checked whole, in bytes, before it is read.
  socket.on('data', (d) => {
    let i;
    while ((i = d.indexOf(10)) >= 0) {
      parts.push(d.subarray(0, i));
      size += i;
      d = d.subarray(i + 1);
      if (size > lineLimit) return fail('the app sent a reply over 32 MiB');
      const line = Buffer.concat(parts).toString('utf8');
      parts = []; size = 0;
      let reply;
      try { reply = JSON.parse(line); } catch { return fail(`the app sent an unreadable line: ${line.slice(0, 200)}`); }
      const w = waiters.shift();
      // Once attached, every line answers the one request outstanding.
      if (w) w.resolve(reply); else if (attached) return fail('the app sent a line no request asked for'); else lines.push(reply);
    }
    if (d.length) { parts.push(d); size += d.length; if (size > lineLimit) fail('the app sent a reply over 32 MiB'); }
  });
  socket.on('error', (e) => fail(`the attach connection failed: ${e.message}`));
  socket.on('close', () => fail('the app closed the attach connection (Detach Agent, the window closed, or the app quit)'));
  const next = (ms, what) => {
    if (lines.length) return Promise.resolve(lines.shift());
    if (failure) return Promise.reject(failure);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => fail(`${what} within ${ms / 1000} s, so the connection was closed`), ms);
      waiters.push({ resolve: (v) => { clearTimeout(timer); resolve(v); }, reject: (e) => { clearTimeout(timer); reject(e); } });
    });
  };
  socket.write(JSON.stringify({ attach: 1, nonce, ...(session ? { session } : {}) }) + '\n');
  let first = await next(handshakeMs, 'the app did not answer the handshake');
  if (first.waiting) {
    onWaiting?.(first);
    first = await next(approvalMs, 'the person did not allow reads');
  }
  if (!first.attached) {
    const refusal = Object.assign(new Error(`attach: ${(first.error ?? 'refused').replace(/^attach: /, '')}`), { reply: first });
    fail(refusal);
    throw refusal;
  }
  attached = true;
  let id = 0, chain = Promise.resolve();
  const ask = (req) => (chain = chain.catch(() => {}).then(async () => {
    if (failure) throw failure;
    const mine = ++id;
    socket.write(JSON.stringify({ ...req, request: mine }) + '\n');
    // Failing from here, the request was sent: what it did is unknown to the caller.
    const { request: answered, ...reply } = await next(requestMs, `the app did not answer ${req.op} (request ${mine})`).catch((e) => { throw Object.assign(new Error(e.message, { cause: e }), { sent: true }); });
    if (answered !== mine) { fail(`the app answered request ${answered} where ${mine} was asked`); throw Object.assign(new Error(failure.message, { cause: failure }), { sent: true }); }
    return reply;
  }));
  return { attached: first, ask, close: () => { socket.end(); }, abandon: (why) => fail(why), get failure() { return failure; } };
}
