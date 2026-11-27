// Live development-loop discovery (LLP 1189 D2).
// Verifies OS flock ownership (build-lock.mjs `lockHeld`) and loopback reachability
// rather than trusting diagnostic PID text left behind after a crash.
import { lstatSync, readFileSync } from 'node:fs';
import { createConnection } from 'node:net';
import { resolve } from 'node:path';
import { lockHeld } from './build-lock.mjs';

const LOOPBACK_HOSTS = new Set(['127.0.0.1', 'localhost', '[::1]', '::1']);

/** Whether `urlText` is an `http:` URL on a loopback host; returns `{ href, host, port }` or null. */
export function parseLoopbackDevURL(urlText) {
  if (typeof urlText !== 'string' || !urlText) return null;
  let parsed;
  try { parsed = new URL(urlText); } catch { return null; }
  if (parsed.protocol !== 'http:' || !LOOPBACK_HOSTS.has(parsed.hostname)) return null;
  const port = Number(parsed.port || 80);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  const normalized = new URL(parsed.origin + '/');
  return { href: normalized.href, host: parsed.hostname === 'localhost' ? '127.0.0.1' : parsed.hostname.replace(/^\[|\]$/g, ''), port };
}

/** Whether a TCP socket answers on `host:port` within `timeoutMs`. */
export function probeLoopback(host, port, timeoutMs = 300) {
  return new Promise((done) => {
    let settled = false;
    const finish = (ok) => { if (settled) return; settled = true; clearTimeout(timer); socket.destroy(); done(ok); };
    const socket = createConnection({ host, port });
    const timer = setTimeout(() => finish(false), timeoutMs);
    timer.unref?.();
    socket.once('connect', () => finish(true));
    socket.once('error', () => finish(false));
  });
}

function isOwnedRegularFile(path, uid) {
  try {
    const st = lstatSync(path);
    return st.isFile() && (typeof uid !== 'number' || st.uid === uid);
  } catch { return false; }
}

function processAlive(pid, kill = process.kill) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try { kill(pid, 0); return true; }
  catch (error) { return error?.code === 'EPERM'; }
}

/** Find a live, listening dev loop for the app in `appDir` (JS target preferred over wasm).
 * Returns `{ target, pid, url, port }` or `null`. */
export async function discoverDevLoop(appDir, {
  probe = true,
  probeFn = probeLoopback,
  lockHeldFn = lockHeld,
  killFn = process.kill,
  uid = typeof process.getuid === 'function' ? process.getuid() : null,
} = {}) {
  for (const target of ['js', 'wasm']) {
    const record = resolve(appDir, '.exact', `web-${target}-loop.json`);
    const lock = `${record}.lock`;
    if (!isOwnedRegularFile(record, uid) || !isOwnedRegularFile(lock, uid)) continue;
    if (!lockHeldFn(lock)) continue;
    let info;
    try { info = JSON.parse(readFileSync(record, 'utf8')); } catch { continue; }
    let lockPid = NaN;
    try { lockPid = Number.parseInt(readFileSync(lock, 'utf8').trim(), 10); } catch {}
    if (!Number.isInteger(info?.pid) || info.pid <= 0 || info.pid !== lockPid) continue;
    if (!processAlive(info.pid, killFn)) continue;
    const parsed = parseLoopbackDevURL(info.url);
    if (!parsed) continue;
    if (probe && !(await probeFn(parsed.host, parsed.port))) continue;
    return { target, pid: info.pid, url: parsed.href, port: parsed.port };
  }
  return null;
}
