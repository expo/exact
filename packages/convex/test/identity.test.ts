// Who is asking (LLP 1157 D7): which answers a token change withholds, and
// what a request sent across one carries. Frames marked "recorded" were sent
// by Convex's open-source backend (`npx convex dev`, 2026-11-12) to a client
// speaking 1.49.0's protocol, with a `customJwt` provider and `whoami`, a
// query that answers the token's subject.
import { expect, test } from 'bun:test';
import { Sync } from '../sync.ts';

const JSON_ONLY = { convexToJson: (value: unknown) => value, jsonToConvex: (json: unknown) => json };
const T0 = 'AAAAAAAAAAA=', T1 = 'UI2JvzdR5xg=', T2 = '0L3KzX1R5xg='; // recorded, in order
const version = (querySet: number, identity: number, ts: string) => ({ querySet, identity, ts });
const transition = (start: object, end: object, modifications: object[] = []) => JSON.stringify({ type: 'Transition', startVersion: start, endVersion: end, modifications });
const updated = (queryId: number, value: unknown) => ({ type: 'QueryUpdated', queryId, value, logLines: [], journal: null });
const settled = async (p: Promise<unknown>) => Promise.race([p, new Promise<'pending'>(r => setTimeout(() => r('pending'), 5))]);

/** A client asking `me:whoami` (query 0) and `me:constant` (query 1), connected with `token`, its results arrived. */
function connected(token?: string) {
  const sync = new Sync(JSON_ONLY);
  sync.connecting('session-1');
  if (token) sync.setAuth(token);
  sync.ask(0, 'me:whoami', {});
  sync.ask(0, 'me:constant', {});
  sync.opened();
  if (token) sync.frame(transition(version(0, 0, T0), version(0, 1, T1))); // recorded
  sync.frame(transition(version(0, token ? 1 : 0, token ? T1 : T0), version(1, token ? 1 : 0, T1), [updated(0, token ?? null), updated(1, 'same for everyone')])); // recorded
  return sync;
}
/** Both queries asked at the current `rev`, as a screen showing both asks them; each keeps the other alive. */
const both = (sync: Sync) => [sync.ask(sync.rev, 'me:whoami', {}).read(), sync.ask(sync.rev, 'me:constant', {}).read()];
const whoami = (sync: Sync) => both(sync)[0];
const constant = (sync: Sync) => both(sync)[1];

/** The socket closes, the token changes while it is closed, and a new socket opens: what it sends. */
function reconnect(sync: Sync, token: string | undefined) {
  sync.closed('the socket closed (1006)');
  sync.setAuth(token);
  sync.connecting('session-2');
  return sync.opened();
}

test('signing in across a reconnect gives none of the signed-out answers as the person\'s', () => {
  const sync = connected();
  expect(whoami(sync)).toEqual({ ok: true, value: null });
  expect(reconnect(sync, 'alice').map(f => f.type)).toEqual(['Connect', 'Authenticate', 'ModifyQuerySet']);
  expect(whoami(sync)).toBeUndefined();
  // The first Transition of a connection opened with a token moves the identity and names no query: the server
  // has no query yet. It confirms nothing about the answers held from the last connection.
  sync.frame(transition(version(0, 0, T0), version(0, 1, T1))); // recorded
  expect(both(sync)).toEqual([undefined, undefined]);
  sync.frame(transition(version(0, 1, T1), version(1, 1, T1), [updated(0, 'alice'), updated(1, 'same for everyone')])); // recorded
  expect(both(sync)).toEqual([{ ok: true, value: 'alice' }, { ok: true, value: 'same for everyone' }]);
});

test('one person to another across a reconnect gives none of the first\'s answers', () => {
  const sync = connected('alice');
  expect(whoami(sync)).toEqual({ ok: true, value: 'alice' });
  reconnect(sync, 'bob');
  sync.frame(transition(version(0, 0, T0), version(0, 1, T1)));
  expect(whoami(sync)).toBeUndefined();
  sync.frame(transition(version(0, 1, T1), version(1, 1, T1), [updated(0, 'bob'), updated(1, 'same for everyone')]));
  expect(whoami(sync)).toEqual({ ok: true, value: 'bob' });
});

test('signing out across a reconnect gives none of the person\'s answers as signed out', () => {
  const sync = connected('alice');
  expect(reconnect(sync, undefined).map(f => f.type)).toEqual(['Connect', 'ModifyQuerySet']);
  expect(both(sync)).toEqual([undefined, undefined]);
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, null), updated(1, 'same for everyone')])); // recorded
  expect(whoami(sync)).toEqual({ ok: true, value: null });
});

test('the same person across a reconnect keeps what the last connection delivered', () => {
  const sync = connected('alice');
  reconnect(sync, 'alice');
  expect(whoami(sync)).toEqual({ ok: true, value: 'alice' });
  sync.frame(transition(version(0, 0, T0), version(0, 1, T1)));
  expect(whoami(sync)).toEqual({ ok: true, value: 'alice' });
});

test('on one connection, a token change withholds every answer until Convex confirms it, then gives the ones it did not name', async () => {
  const sync = connected('alice');
  expect(sync.setAuth('bob')).toEqual([{ type: 'Authenticate', tokenType: 'User', value: 'bob', baseVersion: 1 }]);
  const asked = sync.ask(sync.rev, 'me:constant', {});
  expect([whoami(sync), asked.read()]).toEqual([undefined, undefined]);
  // A write elsewhere, computed for the person Convex still has: withheld too.
  sync.frame(transition(version(1, 1, T1), version(1, 1, T2), [updated(0, 'alice')]));
  expect(whoami(sync)).toBeUndefined();
  expect(await settled(asked.wait)).toBe('pending');
  sync.frame(transition(version(1, 1, T2), version(1, 2, T2), [updated(0, 'bob')])); // recorded, as Convex names only the changed
  await asked.wait;
  expect([whoami(sync), asked.read()]).toEqual([{ ok: true, value: 'bob' }, { ok: true, value: 'same for everyone' }]);
});

test('a request sent before Convex confirmed a token carries the token again, so it runs as the person or not at all', async () => {
  const sync = connected();
  expect(sync.setAuth('alice')).toEqual([{ type: 'Authenticate', tokenType: 'User', value: 'alice', baseVersion: 0 }]);
  const { frames, outcome } = sync.request('Mutation', 'me:note', { body: 'guarded' });
  expect(frames).toEqual([
    { type: 'Authenticate', tokenType: 'User', value: 'alice', baseVersion: 1 },
    { type: 'Mutation', requestId: 0, udfPath: 'me:note', args: [{ body: 'guarded' }] },
  ]);
  sync.frame(transition(version(1, 0, T1), version(1, 1, T1), [updated(0, 'alice')])); // recorded
  sync.frame(transition(version(1, 1, T1), version(1, 2, T1))); // recorded
  sync.frame(JSON.stringify({ type: 'MutationResponse', requestId: 0, success: true, result: 'alice', ts: T2, logLines: [] })); // recorded
  sync.frame(transition(version(1, 2, T1), version(1, 2, T2))); // recorded
  expect(await outcome).toEqual({ ok: true, value: 'alice' });
  // Confirmed: the next request goes alone.
  expect(sync.request('Mutation', 'me:note', {}).frames.map(f => f.type)).toEqual(['Mutation']);
});

test('a request that could not be sent counts no token ahead of it', () => {
  const sync = connected();
  sync.setAuth('alice');
  expect(() => sync.request('Mutation', 'me:note', {}, () => { throw new Error('this turn may not send'); })).toThrow('this turn may not send');
  expect(sync.request('Mutation', 'me:note', {}).frames).toEqual([
    { type: 'Authenticate', tokenType: 'User', value: 'alice', baseVersion: 1 },
    { type: 'Mutation', requestId: 0, udfPath: 'me:note', args: [{}] },
  ]);
});

test('a token whose frame never left leaves no request running as the person before', async () => {
  // `setAuth(undefined)` in a mutation that then fetched: the runner refused it, and its frame was not sent.
  const sync = connected('alice');
  expect(sync.setAuth(undefined)).toEqual([{ type: 'Authenticate', tokenType: 'None', baseVersion: 1 }]);
  expect(whoami(sync)).toBeUndefined();
  // Called again, it sends again, since Convex has not confirmed it.
  expect(sync.setAuth(undefined)).toEqual([{ type: 'Authenticate', tokenType: 'None', baseVersion: 2 }]);
  // A later request carries the sign-out ahead of it, at the version counting the frames that never left.
  const { frames, outcome } = sync.request('Mutation', 'me:note', { body: 'must not be alice' });
  expect(frames[0]).toEqual({ type: 'Authenticate', tokenType: 'None', baseVersion: 3 });
  // Convex is at version 1, refuses the version and closes the socket before it runs the mutation (recorded:
  // nothing was written for alice). The next connection opens signed out.
  sync.closed('the socket closed (1011): InternalServerError');
  expect(await outcome).toMatchObject({ ok: false });
  sync.connecting('session-2');
  expect(sync.opened().map(f => f.type)).toEqual(['Connect', 'ModifyQuerySet']);
});

test('a token whose frame never left is sent again once Convex has taken a later query set without it', () => {
  const sync = connected('alice');
  sync.setAuth(undefined); // never left
  expect(whoami(sync)).toBeUndefined();
  // The signed-out screen asks a query of its own; Convex answers that query set at alice's identity.
  const asked = sync.ask(sync.rev, 'me:public', {});
  sync.ask(sync.rev, 'me:whoami', {});
  expect(asked.frames).toEqual([{ type: 'ModifyQuerySet', baseVersion: 1, newVersion: 2, modifications: [{ type: 'Add', queryId: 2, udfPath: 'me:public', args: [{}] }] }]);
  expect(sync.frame(transition(version(1, 1, T1), version(2, 1, T1), [updated(2, 'hello')]))).toEqual([
    { type: 'Authenticate', tokenType: 'None', baseVersion: 1 },
  ]);
  expect(asked.read()).toBeUndefined();
  sync.frame(transition(version(2, 1, T1), version(2, 2, T1), [updated(0, null)]));
  expect([asked.read(), whoami(sync)]).toEqual([{ ok: true, value: 'hello' }, { ok: true, value: null }]);
});

test('a token that expired ends the requests Convex had not answered with its words, and says they may have run', async () => {
  const sync = connected('alice');
  const acknowledged = sync.request('Mutation', 'me:note', { body: 'before' });
  sync.frame(JSON.stringify({ type: 'MutationResponse', requestId: 0, success: true, result: 'alice', ts: T2, logLines: [] }));
  // An action sent while the token was good, still running when a later request meets the expiry: it may have
  // reached a payment provider, so nothing may say it did not run.
  const running = sync.request('Action', 'billing:charge', {});
  const later = sync.request('Mutation', 'me:note', { body: 'after expiry' });
  sync.frame(JSON.stringify({ type: 'AuthError', error: 'Convex token identity expired', baseVersion: 1, authUpdateAttempted: false })); // recorded
  sync.closed('the socket closed (1005)'); // recorded
  expect(await running.outcome).toEqual({ ok: false, message: 'Convex refused the token before it answered (Convex token identity expired); the action may have run.' });
  expect(await later.outcome).toEqual({ ok: false, message: 'Convex refused the token before it answered (Convex token identity expired); the mutation may have run.', again: 2 });
  expect(await acknowledged.outcome).toEqual({ ok: true, value: 'alice' });
  expect([sync.status, sync.authError]).toEqual(['closed', 'Convex token identity expired']);
  // The person's answers are not given as signed out, which the next connection is.
  expect(whoami(sync)).toBeUndefined();
  sync.connecting('session-2');
  expect(sync.opened().map(f => f.type)).toEqual(['Connect', 'ModifyQuerySet']);
});

test('a token Convex refused ends what was sent behind it with its words: it did not run', async () => {
  const sync = connected();
  const before = sync.request('Action', 'billing:charge', {});
  sync.setAuth('not-a-jwt');
  const behind = sync.request('Mutation', 'me:note', { body: 'behind a refused token' });
  // Recorded: Convex refused the token and closed the socket, and the mutation behind it wrote nothing.
  sync.frame(JSON.stringify({ type: 'AuthError', error: 'Could not parse JWT payload.', baseVersion: 0, authUpdateAttempted: true }));
  sync.closed('the socket closed (1005)');
  expect(await behind.outcome).toEqual({ ok: false, message: 'Could not parse JWT payload.', again: 1 });
  expect(await before.outcome).toEqual({ ok: false, message: 'Convex refused the token before it answered (Could not parse JWT payload.); the action may have run.' });
});

test('a token changed and changed back before Convex answered gives the answers held for it again', () => {
  const sync = connected();
  sync.setAuth('alice');
  expect(whoami(sync)).toBeUndefined();
  sync.setAuth(undefined);
  expect(whoami(sync)).toEqual({ ok: true, value: null });
});
