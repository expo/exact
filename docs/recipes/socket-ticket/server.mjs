// A fixture identity, never a production authorization service.
export function ticketServer({ port = 0, now = Date.now } = {}) {
  const tickets = new Map(), sockets = new Set();
  const stats = { issued: 0, consumed: 0, refused: 0, offers: [] };
  const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': 'authorization', 'access-control-allow-methods': 'POST, OPTIONS', 'cache-control': 'no-store' };
  const server = Bun.serve({ hostname: '127.0.0.1', port,
    fetch(req, server) {
      const url = new URL(req.url);
      if (req.method === 'OPTIONS') return new Response(null, { headers: cors });
      if (url.pathname === '/events/ticket' && req.method === 'POST') {
        if (req.headers.get('authorization') !== 'Bearer fixture-only') return new Response('unauthorized', { status: 401, headers: cors });
        const ticket = crypto.randomUUID();
        // Prune expired tickets on issuance; this fixture has no background timer.
        for (const [key, expires] of tickets) if (expires <= now()) tickets.delete(key);
        tickets.set(ticket, now() + 60_000); stats.issued++;
        return Response.json({ ticket }, { headers: cors });
      }
      if (url.pathname === '/events') {
        const ticket = url.searchParams.get('ticket'), expires = tickets.get(ticket);
        // Consume before upgrading: two requests cannot redeem the same ticket.
        tickets.delete(ticket);
        if (!expires || expires <= now()) { stats.refused++; return new Response('invalid ticket', { status: 401 }); }
        stats.consumed++; stats.offers.push(req.headers.get('sec-websocket-protocol'));
        return server.upgrade(req, { headers: { 'sec-websocket-protocol': 'events.v1' }, data: { number: stats.consumed } })
          ? undefined : new Response('upgrade required', { status: 400 });
      }
      return new Response('not found', { status: 404 });
    },
    websocket: {
      open(ws) { sockets.add(ws); ws.send(`connection ${ws.data.number}`); },
      message() {},
      close(ws) { sockets.delete(ws); },
    },
  });
  return { server, stats, close() { for (const ws of sockets) ws.close(); server.stop(true); } };
}
if (import.meta.main) {
  const fixture = ticketServer({ port: Number(process.env.PORT ?? 4798) });
  console.log(`SOCKET_API=http://127.0.0.1:${fixture.server.port}`);
}
