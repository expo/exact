// The stand-in (`server.ts`) with the module's protocol state speaking to it
// over a real socket, as an app's tests do: who is asking, Convex's HTTP API,
// an app's own routes, and the identity changes of LLP 1157 D7.
import { afterAll, expect, test } from 'bun:test';
import { Sync, type Frame } from '../sync.ts';
import { serve } from './server.ts';

const notes: { by: string | null; body: string }[] = [];
/** A route's response the test holds on to: the stand-in must not change it. */
const kept = Response.json({ kept: true });
const server = serve({
  port: 0,
  queries: {
    'me:whoami': (_args, { identity }) => identity,
    'notes:mine': (_args, { identity }) => notes.filter(n => n.by === identity).map(n => n.body),
  },
  mutations: { 'notes:add': ({ body }, { identity }) => { notes.push({ by: identity, body }); } },
  http: request => {
    const { pathname } = new URL(request.url);
    if (pathname === '/api/auth/token') return Response.json({ token: jwt('alice') });
    if (pathname === '/old') return Response.redirect(`${origin}/api/auth/token`, 302);
    // A proxy: what another route answers, as fetched.
    if (pathname === '/proxied') return fetch(`${origin}/api/auth/token`);
    if (pathname === '/kept') return kept;
    return undefined;
  },
});
afterAll(() => server.stop());
const origin = `http://127.0.0.1:${server.port}`;

/** A token whose `sub` is `who`; the stand-in reads it unchecked. */
function jwt(who: string) {
  const part = (o: object) => btoa(JSON.stringify(o)).replace(/=+$/, '').replace(/\+/g, '-').replace(/\//g, '_');
  return `${part({ alg: 'none' })}.${part({ sub: who })}.`;
}

/** `Sync` over a socket to the stand-in, as `index.ts` drives it. */
function connect(sync: Sync) {
  const ws = new WebSocket(`${origin.replace('http', 'ws')}/api/1.49.0/sync`);
  const send = (frames: Frame[]) => { for (const f of frames) ws.send(JSON.stringify(f)); };
  sync.connecting(crypto.randomUUID());
  ws.onopen = () => send(sync.opened());
  ws.onmessage = e => send(sync.frame(String(e.data)));
  const closed = new Promise<void>(resolve => { ws.onclose = e => { sync.closed(`the socket closed (${e.code})`); resolve(); }; });
  return { send, closed, close: () => { ws.close(); return closed; } };
}

/** The answer to `udfPath` once one is given. */
async function answer(sync: Sync, udfPath: string, send: (frames: Frame[]) => void) {
  for (;;) {
    const asked = sync.ask(sync.rev, udfPath, {});
    send(asked.frames);
    const now = asked.read();
    if (now) return now;
    await asked.wait;
  }
}

test('a function gets who is asking, and a sign-in, a switch and a sign-out across reconnects give each person only their own', async () => {
  const sync = new Sync({ convexToJson: v => v, jsonToConvex: v => v });
  let link = connect(sync);
  expect(await answer(sync, 'me:whoami', link.send)).toEqual({ ok: true, value: null });
  for (const who of ['alice', 'bob', undefined]) {
    await link.close();
    sync.setAuth(who && jwt(who));
    link = connect(sync);
    // Every answer given after the change is the new person's.
    const seen: unknown[] = [];
    while (sync.status !== 'ready' || seen.at(-1) !== (who ?? null)) {
      const asked = sync.ask(sync.rev, 'me:whoami', {});
      link.send(asked.frames);
      const now = asked.read();
      if (now?.ok) seen.push(now.value);
      else await asked.wait;
    }
    expect(seen).toEqual([who ?? null]);
  }
  await link.close();
});

test('a write over the HTTP API, as another client, reaches a socket\'s queries', async () => {
  const sync = new Sync({ convexToJson: v => v, jsonToConvex: v => v });
  sync.setAuth(jwt('carol'));
  const link = connect(sync);
  expect(await answer(sync, 'notes:mine', link.send)).toEqual({ ok: true, value: [] });
  const rev = sync.rev;
  const wrote = await fetch(`${origin}/api/mutation`, { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${jwt('carol')}` }, body: JSON.stringify({ path: 'notes:add', args: { body: 'from outside' }, format: 'json' }) });
  expect(await wrote.json()).toEqual({ status: 'success', value: null, logLines: [] });
  while (sync.rev === rev) await Bun.sleep(5);
  expect(await answer(sync, 'notes:mine', link.send)).toEqual({ ok: true, value: ['from outside'] });
  const read = await fetch(`${origin}/api/query`, { method: 'POST', body: JSON.stringify({ path: 'notes:mine', args: {} }) });
  expect(await read.json()).toEqual({ status: 'success', value: [], logLines: [] });
  await link.close();
});

test('an app\'s own route answers, and every response carries CORS headers', async () => {
  const token = await fetch(`${origin}/api/auth/token`);
  expect(token.headers.get('Access-Control-Allow-Origin')).toBe('*');
  expect(await token.json()).toEqual({ token: jwt('alice') });
  const preflight = await fetch(`${origin}/api/mutation`, { method: 'OPTIONS' });
  expect([preflight.status, preflight.headers.get('Access-Control-Allow-Headers')]).toEqual([204, 'Content-Type, Authorization, Convex-Client']);
  expect((await fetch(`${origin}/elsewhere`)).status).toBe(404);
});

test('a redirect and a fetched response from an app route answer, with CORS headers', async () => {
  const moved = await fetch(`${origin}/old`, { redirect: 'manual' });
  expect([moved.status, moved.headers.get('Location'), moved.headers.get('Access-Control-Allow-Origin')]).toEqual([302, `${origin}/api/auth/token`, '*']);
  const proxied = await fetch(`${origin}/proxied`);
  expect([proxied.status, proxied.headers.get('Access-Control-Allow-Origin')]).toEqual([200, '*']);
  expect(await proxied.json()).toEqual({ token: jwt('alice') });
  // The headers are added to a copy: a redirect's or a fetched response's are immutable under the Fetch
  // standard, so changing the handler's own would throw there.
  expect((await fetch(`${origin}/kept`)).headers.get('Access-Control-Allow-Origin')).toBe('*');
  expect(kept.headers.has('Access-Control-Allow-Origin')).toBe(false);
});

test('an Authenticate at a version the stand-in is not at closes the socket, as Convex does', async () => {
  const ws = new WebSocket(`${origin.replace('http', 'ws')}/api/1.49.0/sync`);
  await new Promise(resolve => (ws.onopen = resolve));
  ws.send(JSON.stringify({ type: 'Connect', sessionId: 's', connectionCount: 0, lastCloseReason: 'InitialConnect', clientTs: 0 }));
  ws.send(JSON.stringify({ type: 'Authenticate', tokenType: 'None', baseVersion: 1 }));
  expect(await new Promise(resolve => (ws.onclose = e => resolve(e.code)))).toBe(1011);
});
