import { test, expect } from 'bun:test';
import { resolve } from 'node:path';
import { ticketServer } from './server.mjs';
import { oauth } from '../../../packages/oauth/index.ts';
import { handle } from '../../../packages/oauth/test/fixture.ts';

const issue = async fixture => {
  const response = await fetch(new URL('/events/ticket', fixture.server.url), {
    method: 'POST', headers: { authorization: 'Bearer fixture-only' },
  });
  expect(response.status).toBe(200);
  return (await response.json()).ticket;
};

test('tickets require authorization, expire, and can be redeemed only once', async () => {
  let now = 1000;
  const fixture = ticketServer({ now: () => now });
  try {
    const endpoint = new URL('/events/ticket', fixture.server.url);
    expect((await fetch(endpoint, { method: 'POST' })).status).toBe(401);
    const expired = await issue(fixture); now += 60_001;
    const url = ticket => new URL('/events?ticket=' + encodeURIComponent(ticket), fixture.server.url);
    expect((await fetch(url(expired))).status).toBe(401);
    const ticket = await issue(fixture);
    const socket = new WebSocket(url(ticket).href.replace(/^http/, 'ws'), ['events.v1']);
    await new Promise((resolve, reject) => { socket.onmessage = event => { expect(event.data).toBe('connection 1'); resolve(); }; socket.onerror = reject; });
    expect(socket.protocol).toBe('events.v1'); socket.close();
    expect((await fetch(url(ticket))).status).toBe(401);
    expect(fixture.stats.consumed).toBe(1);
  } finally { fixture.close(); }
});

// The CLI builds and drives this recipe's actual Contract and TypeScript.
// Run one host per invocation so each fixture starts at connection 1.
test('the recipe reconnects with a new ticket and reports the protocol', async () => {
  const fixture = ticketServer();
  try {
    const host = process.env.SOCKET_HOST ?? 'web';
    const child = Bun.spawn([process.execPath, 'exact.mjs', 'test', host], {
      cwd: import.meta.dir,
      env: { ...process.env, SOCKET_API: fixture.server.url.origin, SOCKET_URL: fixture.server.url.origin.replace(/^http/, 'ws'), EXACT_FEEDBACK: '0', CARGO_TARGET_DIR: resolve(import.meta.dir, '../../../target') },
      stdout: 'inherit', stderr: 'inherit',
    });
    expect(await child.exited).toBe(0);
    expect(fixture.stats.issued).toBe(2);
    expect(fixture.stats.consumed).toBe(2);
    expect(fixture.stats.offers).toEqual(['events.v1', 'events.v1']);
  } finally { fixture.close(); }
}, 900000);

async function recipe(fetcher, api = 'https://api.example.com', provider) {
  let source = await Bun.file(new URL('./app.ts', import.meta.url)).text();
  source = source.replaceAll('import.meta.env', JSON.stringify({ SOCKET_API: api, SOCKET_URL: api.replace(/^http/, 'ws') }));
  if (provider) source = source.replace(/^const authorizedFetch = [\s\S]*?\n\nconst sources/m, 'const authorizedFetch = (...args) => provider.fetch(...args);\n\nconst sources');
  source = source.replaceAll('export const ', 'const ');
  const code = new Bun.Transpiler({ loader: 'ts' }).transformSync(source);
  return new Function('fetch', 'provider', code + '\nreturn answer;')(fetcher, provider);
}

test('only the current attempt can retain a ticket, including out-of-order responses', async () => {
  const pending = [], opened = [];
  const answer = await recipe((url, init) => {
    if (init.exactStream) { opened.push(url); return new Promise(() => {}); }
    let finish;
    const body = new Promise(resolve => { finish = ticket => resolve({ ticket }); });
    pending.push(finish);
    return Promise.resolve({ ok: true, json: () => body });
  });
  const first = answer('ticket', [1]);
  const second = answer('ticket', [2]);
  pending[1]('second'); await second;
  pending[0]('first'); await first;
  let stale; try { void answer('events', [1]); } catch (error) { stale = error; }
  expect(stale?.message).toContain('fresh ticket');
  void answer('events', [2]);
  expect(opened).toEqual(['wss://api.example.com/events?ticket=second']);
  const third = answer('ticket', [3]); pending[2]('third'); await third;
  const fourth = answer('ticket', [4]);
  stale = undefined; try { void answer('events', [3]); } catch (error) { stale = error; }
  expect(stale?.message).toContain('fresh ticket');
  pending[3]('fourth'); await fourth;
  void answer('events', [4]);
  expect(opened.at(-1)).toBe('wss://api.example.com/events?ticket=fourth');
});

test('the OAuth substitution passes the source store and obtains an authorized ticket', async () => {
  let authorized = 0;
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    const url = new URL(req.url);
    if (url.pathname === '/events/ticket') {
      const checked = await handle(new Request(url.origin + '/p/spec/api/me', { headers: req.headers }));
      if (!checked.ok) return checked;
      expect(req.method).toBe('POST'); authorized++;
      return Response.json({ ticket: 'oauth-ticket' });
    }
    return handle(req);
  } });
  const oldCallback = globalThis.authCallback, oldSession = globalThis.openAuthSession;
  globalThis.authCallback = () => 'com.example.app:/oauth';
  globalThis.openAuthSession = async url => (await fetch(url, { redirect: 'manual' })).headers.get('location');
  try {
    const api = server.url.origin;
    const provider = oauth({ issuer: api + '/p/spec', clientId: 'socket-recipe', scopes: ['openid', 'offline_access'], key: 'socket-recipe.session', apis: [api] });
    const map = new Map(), store = { get: key => map.get(key) ?? null, set: (key, value) => { map.set(key, value); }, forget: key => { map.delete(key); } };
    expect((await provider.signIn(store)).ok).toBe(true);
    const opened = [];
    const answer = await recipe((url, init) => { opened.push([url, init.exactProtocols]); return new Promise(() => {}); }, api, provider);
    expect(await answer('ticket', [1], store)).toBe(1);
    void answer('events', [1], store);
    expect(authorized).toBe(1);
    expect(opened).toEqual([[api.replace(/^http/, 'ws') + '/events?ticket=oauth-ticket', ['events.v1']]]);
  } finally {
    globalThis.authCallback = oldCallback; globalThis.openAuthSession = oldSession; server.stop(true);
  }
});

test('the launcher finds Bun on PATH without a developer home directory', async () => {
  const line = (await Bun.file(new URL('./exact.mjs', import.meta.url)).text()).split('\n')[1];
  const child = Bun.spawn(['/bin/sh', '-c', "launch() { printf '%s\\n' \"$1\"; exit; }; " + line.replaceAll('exec ', 'launch ')], {
    env: { ...process.env, BUN: '', HOME: '/nonexistent-socket-recipe-home' }, stdout: 'pipe', stderr: 'pipe',
  });
  expect(await child.exited).toBe(0);
  expect((await new Response(child.stdout).text()).trim()).toBe('bun');
});
