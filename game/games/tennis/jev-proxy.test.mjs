import { test, expect } from 'bun:test';
import { proxy } from './jev-proxy.mjs';

// A gateway on an ephemeral loopback port that records each key it is sent.
function fakeGateway() {
  const keys = [];
  const server = Bun.serve({hostname: '127.0.0.1', port: 0, fetch(request) {
    keys.push(request.headers.get('authorization'));
    return Response.json({ok: true});
  }});
  return {keys, url: `http://127.0.0.1:${server.port}`, stop: () => server.stop(true)};
}

const post = origin => new Request('http://127.0.0.1:47913/v1/evaluate', {
  method: 'POST', body: '{}', headers: {'content-type': 'application/json', ...(origin ? {origin} : {})},
});
const preflight = origin => new Request('http://127.0.0.1:47913/v1/evaluate', {
  method: 'OPTIONS', headers: {origin, 'access-control-request-method': 'POST', 'access-control-request-private-network': 'true'},
});

test('a page from another origin never reaches the gateway with the key', async () => {
  const gateway = fakeGateway();
  try {
    const handle = proxy({key: 'secret', gateway: gateway.url, log() {}});
    for (const origin of ['https://evil.example', 'http://evil.example:47913', 'null', 'http://127.0.0.1.evil.example']) {
      const answer = await handle(preflight(origin));
      expect(answer.status).toBe(403);
      expect(answer.headers.get('access-control-allow-private-network')).toBeNull();
      expect(answer.headers.get('access-control-allow-origin')).toBeNull();
      expect((await handle(post(origin))).status).toBe(403);
    }
    expect(gateway.keys).toEqual([]);
  } finally {
    gateway.stop();
  }
});

test('the local dev page and a native host still reach Jev', async () => {
  const gateway = fakeGateway();
  try {
    const handle = proxy({key: 'secret', gateway: gateway.url, log() {}});
    for (const origin of ['http://127.0.0.1:8765', 'http://localhost:8766']) {
      const answer = await handle(preflight(origin));
      expect(answer.status).toBe(204);
      expect(answer.headers.get('access-control-allow-origin')).toBe(origin);
      expect(answer.headers.get('access-control-allow-private-network')).toBe('true');
      expect((await handle(post(origin))).status).toBe(200);
    }
    expect((await handle(post(null))).status).toBe(200);
    expect(gateway.keys).toEqual(['Bearer secret', 'Bearer secret', 'Bearer secret']);
  } finally {
    gateway.stop();
  }
});
