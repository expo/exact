import { createServer } from 'node:http';
import { createServer as createSocketServer } from 'node:net';

/** The first free port in the requested range, on both loopback addresses.
 * A probe does not reserve it; a concurrent loop can still take it before listen. */
export async function freePort(from, host, ports = null) {
  const range = ports ?? [from, Math.min(from + 99, 65535)];
  for (let p = range[0]; p <= range[1]; p++) {
    const free = await new Promise(ok => { const t = createServer(); t.once('error', () => ok(false)); t.listen(p, host, () => t.close(() => ok(true))); });
    if (free && await freeOnIPv6Loopback(p)) return p;
  }
  console.error(`ports ${range[0]}–${range[1]} are all in use; --port <n> picks another`);
  process.exit(1);
}

/** A dev loop answers on IPv6's loopback too, the same port: `localhost`
 * resolves to [::1] first, and the design tool frames the page as
 * localhost (another site than the tool's 127.0.0.1). An HTTP server of
 * its own there, each request and upgrade handed to `server`'s handlers:
 * a raw socket handed over instead (`emit('connection')`) loses its
 * requests' `close` under Bun, so a page's stream there never ended and
 * the tool listed every canvas it had framed as connected. Closed, its
 * connections with it, when `server` closes. Resolves once it listens, or
 * when there is no IPv6; rejects (EADDRINUSE) when another server holds
 * the port there, which a localhost URL would reach instead. */
export function alsoOnIPv6Loopback(server, port) {
  const six = createServer((req, res) => server.emit('request', req, res));
  // (An upgrade no one takes is closed, as an HTTP server with no `upgrade` listener closes it.)
  six.on('upgrade', (req, socket, head) => { if (!server.emit('upgrade', req, socket, head)) socket.destroy(); });
  // Its connections ended first: Bun's close() lets go of the native server, after which they cannot be.
  server.on('close', () => { six.closeAllConnections?.(); six.close(); });
  return new Promise((ok, fail) => {
    six.once('error', (e) => (e.code === 'EADDRINUSE' ? fail(e) : ok(null)));
    six.listen(port, '::1', () => ok(six));
  });
}

/** TEST ONLY: a dev loop's wait before it listens, so a test shows its
 * 'plan ready' never comes before the server answers (the gap a loaded
 * machine opened, the agent then navigating to a refused connection).
 * EXACT_DEV_LISTEN_DELAY_MS, read only under EXACT_TEST_TIMING=1: a whole number of ms
 * from 1 to 60000, or `generation`, until `generated` (the first generation) resolves or
 * 60000 ms pass; said on stderr. Anything else, or `generation` without `generated`, is ignored. */
export async function listenDelay(env = process.env, generated = null) {
  if (env.EXACT_TEST_TIMING !== '1') return;
  const said = env.EXACT_DEV_LISTEN_DELAY_MS, until = said === 'generation' ? generated : null, ms = until ? 60_000 : /^\d+$/.test(said ?? '') ? Number(said) : NaN;
  if (!(ms >= 1 && ms <= 60_000)) return;
  console.error(`dev: TEST delay ${until ? 'until the first generation' : `${ms} ms`} before listening (EXACT_TEST_TIMING=1)`);
  let bound; await Promise.race([new Promise(ok => { bound = setTimeout(ok, ms); }), ...(until ? [until] : [])]);
  clearTimeout(bound);
}

/** Whether `port` is free on IPv6's loopback too (no IPv6: yes). */
export const freeOnIPv6Loopback = (port) => new Promise((ok) => {
  const t = createSocketServer();
  t.once('error', (e) => ok(e.code !== 'EADDRINUSE'));
  t.listen(port, '::1', () => t.close(() => ok(true)));
});

/** A dev loop stops when its launcher is gone, including after SIGKILL.
 * Without EXACT_LAUNCHER_PID it runs until stopped. */
export function watchLauncher(stop, pid = Number(process.env.EXACT_LAUNCHER_PID)) {
  if (!Number.isInteger(pid) || pid <= 1) return null;
  const timer = setInterval(() => { try { process.kill(pid, 0); } catch (e) { if (e.code === 'ESRCH') { clearInterval(timer); stop(); } } }, 1000);
  timer.unref?.();
  return timer;
}
