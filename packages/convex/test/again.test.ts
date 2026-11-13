// A mutation Convex did not answer, sent again as the same request (LLP 1157 D5): Convex answers a request id a
// session has sent before with its first answer, so the write happens once.
import { afterAll, expect, test } from 'bun:test';
import { Sync, type Frame } from '../sync.ts';
import { serve } from './server.ts';

const JSON_ONLY = { convexToJson: (value: unknown) => value, jsonToConvex: (json: unknown) => json };
const T0 = 'AAAAAAAAAAA=', T1 = 'UI2JvzdR5xg=', T2 = '0L3KzX1R5xg=';
const version = (querySet: number, identity: number, ts: string) => ({ querySet, identity, ts });
const transition = (start: object, end: object) => JSON.stringify({ type: 'Transition', startVersion: start, endVersion: end, modifications: [] });
const CLOSED = 'The connection to Convex closed before it answered (the socket closed (1006))';

/** A client on an open connection Convex has answered, with `token` confirmed. */
function connected(token?: string) {
  const sync = new Sync(JSON_ONLY);
  sync.connecting('session-1');
  if (token) sync.setAuth(token);
  return reopened(sync, token);
}
function reopened(sync: Sync, token?: string) {
  sync.connecting('session-1');
  sync.opened();
  if (token) sync.frame(transition(version(0, 0, T0), version(0, 1, T1)));
  sync.frame(transition(version(0, token ? 1 : 0, token ? T1 : T0), version(1, token ? 1 : 0, T1)));
  return sync;
}

test('a mutation the connection ended unanswered is sent again under its own request id, once', async () => {
  const sync = connected();
  const first = sync.request('Mutation', 'messages:send', { body: 'hello' });
  const action = sync.request('Action', 'mail:send', {});
  sync.closed('the socket closed (1006)');
  expect(await first.outcome).toEqual({ ok: false, message: `${CLOSED}; the mutation may have run.`, again: 0 });
  // An action has no first answer for Convex to give again.
  expect(await action.outcome).toEqual({ ok: false, message: `${CLOSED}; the action may have run.` });
  reopened(sync);
  const sent = sync.again(0)!;
  expect(sent.frames).toEqual(first.frames);
  expect(sync.again(0)).toBeUndefined();
  sync.frame(JSON.stringify({ type: 'MutationResponse', requestId: 0, success: true, result: 'sent', ts: T1, logLines: [] }));
  expect(await sent.outcome).toEqual({ ok: true, value: 'sent' });
  // A new request does not take its id.
  expect(sync.request('Mutation', 'messages:send', {}).frames[0].requestId).toBe(2);
});

test('one sent again and left unanswered again carries the same handle', async () => {
  const sync = connected();
  const first = sync.request('Mutation', 'messages:send', {});
  sync.closed('the socket closed (1006)');
  reopened(sync);
  const second = sync.again((await first.outcome as { again: number }).again)!;
  sync.closed('the socket closed (1006)');
  expect(await second.outcome).toMatchObject({ ok: false, again: 0 });
  // A turn that may not send keeps it.
  reopened(sync);
  expect(() => sync.again(0, () => { throw new Error('this turn may not send'); })).toThrow('this turn may not send');
  expect(sync.again(0)!.frames).toEqual(first.frames);
});

test('sent again before Convex confirmed the token, it goes behind the token as any request does', async () => {
  const sync = connected('alice');
  const first = sync.request('Mutation', 'me:note', {});
  sync.closed('the socket closed (1006)');
  expect(await first.outcome).toMatchObject({ again: 0 });
  sync.connecting('session-1');
  sync.opened();
  // Nothing has arrived on this connection: the token's `Authenticate` goes again, at the version after the first.
  expect(sync.again(0)!.frames).toEqual([{ type: 'Authenticate', tokenType: 'User', value: 'alice', baseVersion: 1 }, ...first.frames]);
});

test('a refused token leaves what Convex had not answered to send again, with a new token', async () => {
  const sync = connected('alice');
  const unanswered = sync.request('Mutation', 'me:note', {});
  sync.frame(JSON.stringify({ type: 'AuthError', error: 'Convex token identity expired', baseVersion: 1, authUpdateAttempted: false }));
  sync.closed('the socket closed (1005)');
  expect(await unanswered.outcome).toEqual({ ok: false, message: 'Convex refused the token before it answered (Convex token identity expired); the mutation may have run.', again: 0 });
  sync.setAuth('alice-renewed');
  reopened(sync, 'alice-renewed');
  expect(sync.again(0)!.frames).toEqual(unanswered.frames);
});

test('signing out forgets what could be sent again, and what is in flight is not kept when it fails', async () => {
  const sync = connected('alice');
  const kept = sync.request('Mutation', 'me:note', {});
  sync.closed('the socket closed (1006)');
  expect(await kept.outcome).toMatchObject({ again: 0 });
  reopened(sync, 'alice');
  const flying = sync.request('Mutation', 'me:note', {});
  sync.setAuth(undefined);
  expect(sync.again(0)).toBeUndefined();
  sync.closed('the socket closed (1006)');
  expect(await flying.outcome).toEqual({ ok: false, message: `${CLOSED}; the mutation may have run.` });
});

test('signing out after Convex refused the token forgets what could be sent again', async () => {
  const sync = connected('alice');
  const unanswered = sync.request('Mutation', 'me:note', {});
  sync.frame(JSON.stringify({ type: 'AuthError', error: 'Convex token identity expired', baseVersion: 1, authUpdateAttempted: false }));
  sync.closed('the socket closed (1005)');
  expect(await unanswered.outcome).toMatchObject({ again: 0 });
  // Convex's refusal already dropped the token; the app's own sign-out still counts.
  sync.setAuth(undefined);
  expect(sync.again(0)).toBeUndefined();
});

test('a sign-out that could not be sent forgets nothing', async () => {
  const sync = connected('alice');
  const kept = sync.request('Mutation', 'me:note', {});
  sync.closed('the socket closed (1006)');
  await kept.outcome;
  reopened(sync, 'alice');
  expect(() => sync.setAuth(undefined, () => { throw new Error('this turn may not send'); })).toThrow();
  expect(sync.again(0)).toBeDefined();
});

// The stand-in, as Convex: a client's repeated request id gets its first answer and runs nothing.
const notes: string[] = [];
const server = serve({ port: 0, queries: { 'notes:all': () => notes }, mutations: { 'notes:add': ({ body }) => { notes.push(body); return notes.length; } } });
afterAll(() => server.stop());

test('against the stand-in, a mutation whose answer was lost with the socket writes once when sent again', async () => {
  const sync = new Sync(JSON_ONLY);
  const open = () => {
    const ws = new WebSocket(`ws://127.0.0.1:${server.port}/api/1.49.0/sync`);
    const send = (frames: Frame[]) => { for (const f of frames) ws.send(JSON.stringify(f)); };
    sync.connecting('client-1');
    let ready = () => {};
    const first = new Promise<void>(r => { ready = r; });
    // The first connection hears nothing after its mutation: the answer is lost, as when a socket drops.
    ws.onopen = () => { send(sync.opened()); ready(); };
    return { ws, send, first };
  };
  const one = open();
  await one.first;
  const lost = sync.request('Mutation', 'notes:add', { body: 'once' }, one.send);
  // The stand-in runs it; then the socket ends before this client reads the answer.
  while (notes.length < 1) await new Promise(r => setTimeout(r, 5));
  one.ws.close();
  sync.closed('the socket closed (1006)');
  const failure = await lost.outcome as { again: number };
  expect(failure.again).toBe(0);

  const two = open();
  two.ws.onmessage = e => two.send(sync.frame(String(e.data)));
  await two.first;
  const sent = sync.again(failure.again, two.send)!;
  expect(await sent.outcome).toEqual({ ok: true, value: 1 });
  expect(notes).toEqual(['once']);
  two.ws.close();
});
