#!/usr/bin/env bun
// Dev-only: lets a browser build reach Jev without ever holding the key.
// Listens on 127.0.0.1:47913 (the origin the `jev` data source is granted),
// forwards POST /v1/evaluate to the Vercel AI Gateway with the key from this
// process's environment, and answers CORS preflights. Never logs the key.
//
//   export AI_GATEWAY_API_KEY="$(sed -n 's/^[[:space:]]*AI_GATEWAY_API_KEY=//p' ~/Dropbox/APIKeys/vercel.txt)"
//   bun game/games/tennis/jev-proxy.mjs
const PORT = 47913;

/** Whether a request may spend the key. A browser sends `Origin` on every POST
 * and preflight (Fetch, "append a request `Origin` header"), so a page is
 * admitted only when it is served from this machine's loopback, as the dev
 * server's is; any other site, a DNS-rebound name and the opaque `null` are
 * refused. A request without `Origin` comes from a native host or a tool on
 * this machine, not from a page. */
export function admitted(origin) {
  if (origin === null) return true;
  try {
    const url = new URL(origin);
    return url.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname);
  } catch {
    return false;
  }
}

/** The proxy's request handler: forwards to `gateway` with `key`. */
export function proxy({key, gateway, log = console.log}) {
  let served = 0;
  return async function handle(request) {
    const origin = request.headers.get('origin');
    if (!admitted(origin)) return new Response('origin refused', {status: 403});
    const cors = origin === null ? {} : {
      'access-control-allow-origin': origin,
      'access-control-allow-methods': 'POST, OPTIONS',
      'access-control-allow-headers': 'content-type',
      'access-control-allow-private-network': 'true',
      'access-control-max-age': '600',
      vary: 'origin',
    };
    const {pathname} = new URL(request.url);
    if (request.method === 'OPTIONS') return new Response(null, {status: 204, headers: cors});
    if (request.method !== 'POST' || pathname !== '/v1/evaluate') return new Response('not found', {status: 404, headers: cors});
    const body = await request.text();
    if (body.length > 64 * 1024) return new Response('question too large', {status: 413, headers: cors});
    const started = performance.now();
    try {
      const reply = await fetch(`${gateway}/v1/evaluate`, {
        method: 'POST',
        headers: {'content-type': 'application/json', authorization: `Bearer ${key}`},
        body,
      });
      const text = await reply.text();
      log(`jev #${++served} ${reply.status} in ${Math.round(performance.now() - started)} ms`);
      return new Response(text, {status: reply.status, headers: {...cors, 'content-type': 'application/json'}});
    } catch (error) {
      log(`jev #${++served} failed after ${Math.round(performance.now() - started)} ms: ${error.message}`);
      return new Response('gateway unreachable', {status: 502, headers: cors});
    }
  };
}

if (import.meta.main) {
  const gateway = process.env.TENNIS_JEV_GATEWAY ?? 'https://ai-gateway.vercel.sh';
  const key = process.env.AI_GATEWAY_API_KEY?.trim();
  if (!key) {
    console.error('jev-proxy: AI_GATEWAY_API_KEY is not set in this environment');
    process.exit(1);
  }
  const server = Bun.serve({hostname: '127.0.0.1', port: PORT, fetch: proxy({key, gateway})});
  console.log(`jev-proxy: http://${server.hostname}:${server.port}/v1/evaluate → ${gateway} (pid ${process.pid})`);
}
