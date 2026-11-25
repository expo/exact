// Hold each loopback GET until its reply is typed in this terminal.
const waiting = new Map();
const headers = { 'Access-Control-Allow-Origin': '*', 'Cache-Control': 'no-store' };
const server = Bun.serve({
  hostname: '127.0.0.1', port: 4789, idleTimeout: 0,
  fetch(request) {
    const path = new URL(request.url).pathname;
    if (request.method !== 'GET' || !/^\/(rows|history)\/(Stocks|Lend)$/.test(path))
      return new Response(null, { status: 404, headers });
    if (waiting.has(path)) return new Response(null, { status: 409, headers });
    return new Promise(resolve => {
      const reply = status => resolve(new Response(path.slice(1).split('/').reverse().join(' '), { status, headers }));
      waiting.set(path, reply);
      request.signal.addEventListener('abort', () => {
        if (waiting.get(path) === reply) waiting.delete(path);
      }, { once: true });
      console.log(`Held ${path}`);
    });
  },
});
console.log(`Serving ${server.url}. To reply, type a held path and status, e.g. /rows/Stocks 200`);
for await (const line of console) {
  const [path, status] = line.trim().split(/\s+/);
  const reply = waiting.get(path);
  if (!reply || !['200', '503'].includes(status)) {
    console.log('Choose a held path and 200 or 503:', [...waiting.keys()].join(' '));
    continue;
  }
  waiting.delete(path);
  reply(Number(status));
  console.log(`Sent ${status} ${path}`);
}
server.stop(true);
