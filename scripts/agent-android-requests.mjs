// What the reader carrier does with a driver's request beside relaying it to the app (`agent-android.mjs`'s `relay`).

import { statSync } from 'node:fs';
import { basename, dirname } from 'node:path';

/** Each of a drop's files (`tap <target> drop <path…>`, LLP 1094.001 D6), this Mac's, to a folder of its own under
 * [base] on the device, where the app reads it: the request as the app is to get it, and each push as
 * `[local, remote]`. An enter's files are not read, so they stay where they are. */
export function dropPushes(request, base) {
  if (request?.op !== 'tap' || !Array.isArray(request.drop) || !request.drop.length) return { request, pushes: [] };
  const pushes = request.drop.map((path, i) => [path, `${base}/${dropPushes.next++}-${i}/${basename(path)}`]);
  return { request: { ...request, drop: pushes.map(([, remote]) => remote) }, pushes };
}
dropPushes.next = 0;

/** Puts [request]'s files on the device through [sh] (adb's words, a `spawnSync` result back): the request naming
 * them there, none when it names no file, or an error that names what could not be put there. A folder is refused,
 * since Android's drops take files alone (LLP 1094.001 D5). */
export function pushDrops(request, base, sh) {
  const { request: moved, pushes } = dropPushes(request, base);
  const failed = (r) => r.status !== 0 && (r.stderr || r.stdout || r.error?.message || `exit ${r.status}`).trim();
  for (const [local] of pushes) {
    let stat;
    try { stat = statSync(local); } catch (e) { return { error: `the drop's file ${local} could not be read: ${e.message}` }; }
    if (!stat.isFile()) return { error: `${local} is ${stat.isDirectory() ? 'a folder' : 'no file'}: a drop on Android takes files alone` };
  }
  for (const [local, remote] of pushes) {
    const why = failed(sh('shell', 'mkdir', '-p', `'${dirname(remote)}'`)) || failed(sh('push', local, remote));
    if (why) return { error: `the drop's file ${local} could not be put on the device: ${why}` };
  }
  if (!pushes.length) return {};
  const why = failed(sh('shell', 'chmod', '-R', 'a+rX', `'${base}'`));
  if (why) return { error: `the drop's files could not be made readable on the device: ${why}` };
  return { request: moved };
}

/** The app's reply to [request], after the wait the app asks of a held contact. */
export async function readerReply(request, ask, wait = Bun.sleep, sh) {
  let reply = await ask();
  if (request?.op === 'tap' && sh) {
    for (let edge = 0; edge < 3; edge++) {
      const {moduleMotion, display, x, y} = JSON.parse(reply);
      if (!moduleMotion) break;
      if (!['DOWN', 'MOVE', 'UP', 'CANCEL'].includes(moduleMotion) || !Number.isSafeInteger(display)
          || display < 0 || !Number.isFinite(x) || !Number.isFinite(y)) throw new Error('invalid Android touch coordinates or action');
      const result = sh('shell', 'input', 'touchscreen', '-d', String(display), 'motionevent', moduleMotion, String(x), String(y));
      const error = result.status === 0 ? '' : (result.stderr || result.stdout || 'Android input failed').trim();
      reply = await ask(JSON.stringify({op:'moduleMotion', error}));
    }
  }
  if (request?.op === 'tap' && request.phase === 'hold') {
    // The host owns this decision, including while a capture borrows a seekable drive's clock.
    const { waitMs } = JSON.parse(reply);
    if (waitMs > 0) await wait(waitMs);
  }
  return reply;
}
