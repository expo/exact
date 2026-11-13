// The protocol state (LLP 1157 D3 to D7), fed frames: the ones marked
// "recorded" were sent by a Convex backend (the open-source one,
// `npx convex dev` without an account, 2026-11-10) to a client speaking
// 1.49.0's protocol; the rest follow `convex@1.49.0`'s
// `src/browser/sync/protocol.ts`.
import { expect, test } from 'bun:test';
import { convex } from '../index.ts';
import { canonicalPath, ConvexFailure, ProtocolError, Sync, tsAtOrBefore, type Outcome } from '../sync.ts';

// The frames here carry JSON alone, which Convex's encoding leaves as it is.
const JSON_ONLY = { convexToJson: (value: unknown) => value, jsonToConvex: (json: unknown) => json };
const T0 = 'AAAAAAAAAAA=', T1 = '0CrNh8fK5hg=', T2 = 'aOyvV8rK5hg=', T3 = 'mFTHV8rK5hg='; // recorded, in order
const version = (querySet: number, identity: number, ts: string) => ({ querySet, identity, ts });
const transition = (start: object, end: object, modifications: object[] = []) => JSON.stringify({ type: 'Transition', startVersion: start, endVersion: end, modifications });
const updated = (queryId: number, value: unknown, journal: string | null = null) => ({ type: 'QueryUpdated', queryId, value, logLines: [], journal });
const HELLO = { at: 1794344471951.407, author: 'probe', body: 'hello', id: 'j5794fzrrdhxs541k0e4sdc3858hza3p' };

/** A client whose socket is open, with `messages:list` asked for before it opened. */
function opened() {
  const sync = new Sync(JSON_ONLY);
  sync.connecting('session-1');
  const first = sync.ask(0, 'messages:list', { channel: 'general' });
  const frames = sync.opened();
  return { sync, first, frames };
}
const settled = async (p: Promise<Outcome>) => Promise.race([p, new Promise<'pending'>(r => setTimeout(() => r('pending'), 5))]);

test('timestamps compare as little-endian u64s', () => {
  expect(tsAtOrBefore(T0, T1)).toBe(true);
  expect(tsAtOrBefore(T1, T2)).toBe(true);
  expect(tsAtOrBefore(T2, T3)).toBe(true);
  expect(tsAtOrBefore(T3, T2)).toBe(false);
  expect(tsAtOrBefore(T2, T2)).toBe(true);
});

test('a function path is canonical', () => {
  expect(canonicalPath('messages:list')).toBe('messages:list');
  expect(canonicalPath('messages.js:list')).toBe('messages:list');
  expect(canonicalPath('messages')).toBe('messages:default');
  expect(canonicalPath('dir/messages:list')).toBe('dir/messages:list');
});

test('a query asked before the socket opens goes in the first query set, and its result answers it', async () => {
  const { sync, first, frames } = opened();
  expect(first.frames).toEqual([]);
  expect(first.read()).toBeUndefined();
  expect(frames).toEqual([
    { type: 'Connect', sessionId: 'session-1', connectionCount: 0, lastCloseReason: 'InitialConnect', clientTs: 0 },
    { type: 'ModifyQuerySet', baseVersion: 0, newVersion: 1, modifications: [{ type: 'Add', queryId: 0, udfPath: 'messages:list', args: [{ channel: 'general' }] }] },
  ]);
  expect(sync.status).toBe('open');
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [])])); // recorded
  await first.wait;
  expect(first.read()).toEqual({ ok: true, value: [] });
  expect([sync.status, sync.rev]).toEqual(['ready', 1]);
  // Asked again at the new rev, it answers now.
  expect(sync.ask(1, 'messages:list', { channel: 'general' }).read()).toEqual({ ok: true, value: [] });
});

test('a query asked while the socket is open is added at the next query-set version', () => {
  const { sync } = opened();
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [])]));
  const second = sync.ask(1, 'messages:count', {});
  expect(second.frames).toEqual([{ type: 'ModifyQuerySet', baseVersion: 1, newVersion: 2, modifications: [{ type: 'Add', queryId: 1, udfPath: 'messages:count', args: [{}] }] }]);
  // The same query, asked again, sends nothing.
  expect(sync.ask(1, 'messages:count', {}).frames).toEqual([]);
});

test('a failed query is its error, with a ConvexError\'s data', () => {
  const { sync, first } = opened();
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [{ type: 'QueryFailed', queryId: 0, errorMessage: 'Server Error', errorData: { code: 7 }, logLines: [], journal: null }]));
  expect(first.read()).toEqual({ ok: false, message: 'Server Error', data: { code: 7 } });
});

test('a query no generation of asks wants is removed, at the first ask of the generation after', () => {
  const { sync } = opened();
  expect(sync.ask(0, 'board:cells', {}).frames[0].modifications).toEqual([{ type: 'Add', queryId: 1, udfPath: 'board:cells', args: [{}] }]);
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [])]));
  // rev 1: only `messages:list` is asked again. `board:cells` was asked at rev 0, the generation before: kept.
  expect(sync.ask(1, 'messages:list', { channel: 'general' }).frames).toEqual([]);
  sync.frame(transition(version(1, 0, T1), version(1, 0, T2)));
  // rev 2: the first ask finds `board:cells` unasked at rev 1 and removes it.
  expect(sync.ask(2, 'messages:list', { channel: 'general' }).frames).toEqual([{ type: 'ModifyQuerySet', baseVersion: 2, newVersion: 3, modifications: [{ type: 'Remove', queryId: 1 }] }]);
  expect(sync.inspect().queries.map(q => q.udfPath)).toEqual(['messages:list']);
});

test('a skipped rev removes nothing that the last generation of asks wanted', () => {
  const { sync } = opened();
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [])]));
  sync.frame(transition(version(1, 0, T1), version(1, 0, T2)));
  sync.frame(transition(version(1, 0, T2), version(1, 0, T3)));
  // Three frames mapped before any ask: rev is 3, and the query was last asked at rev 0.
  expect(sync.ask(3, 'messages:list', { channel: 'general' }).frames).toEqual([]);
  expect(sync.inspect().queries.length).toBe(1);
});

test('a mutation settles when a Transition reaches its timestamp', async () => {
  const { sync } = opened();
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [])]));
  const { frames, outcome } = sync.request('Mutation', 'messages:send', { channel: 'general', body: 'hello', author: 'probe' });
  expect(frames).toEqual([{ type: 'Mutation', requestId: 0, udfPath: 'messages:send', args: [{ channel: 'general', body: 'hello', author: 'probe' }] }]);
  sync.frame(JSON.stringify({ type: 'MutationResponse', requestId: 0, success: true, result: null, ts: T2, logLines: [] })); // recorded
  expect(await settled(outcome)).toBe('pending');
  sync.frame(transition(version(1, 0, T1), version(1, 0, T2), [updated(0, [HELLO])])); // recorded
  expect(await outcome).toEqual({ ok: true, value: null });
  expect(sync.ask(2, 'messages:list', { channel: 'general' }).read()).toEqual({ ok: true, value: [HELLO] });
});

test('a mutation Convex refuses settles at once with its words', async () => {
  const { sync } = opened();
  const { outcome } = sync.request('Mutation', 'messages:send', { channel: 5 });
  sync.frame(JSON.stringify({ type: 'MutationResponse', requestId: 0, success: false, result: 'ArgumentValidationError: Object is missing the required field `author`.', ts: null, logLines: [] })); // recorded, shortened
  expect(await outcome).toEqual({ ok: false, message: 'ArgumentValidationError: Object is missing the required field `author`.' });
});

test('an action settles with its response', async () => {
  const { sync } = opened();
  const { outcome } = sync.request('Action', 'ai:reply', {});
  sync.frame(JSON.stringify({ type: 'ActionResponse', requestId: 0, success: true, result: 'hi', logLines: [] }));
  expect(await outcome).toEqual({ ok: true, value: 'hi' });
});

test('the socket closing ends what waits, and the next connection starts from version 0', async () => {
  const { sync } = opened();
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [HELLO])]));
  const pending = sync.ask(1, 'messages:count', {});
  const unanswered = sync.request('Mutation', 'messages:send', {});
  const acknowledged = sync.request('Mutation', 'messages:send', {});
  sync.frame(JSON.stringify({ type: 'MutationResponse', requestId: 1, success: true, result: 3, ts: T2, logLines: [] }));
  sync.closed('the socket closed (1006)');
  await pending.wait;
  expect(pending.read()).toBeUndefined();
  expect(await unanswered.outcome).toEqual({ ok: false, message: 'The connection to Convex closed before it answered (the socket closed (1006)); the mutation may have run.', again: 0 });
  expect(await acknowledged.outcome).toEqual({ ok: true, value: 3 });
  expect([sync.status, sync.error]).toEqual(['closed', 'the socket closed (1006)']);
  // What the socket had delivered is still answered.
  expect(sync.ask(1, 'messages:list', { channel: 'general' }).read()).toEqual({ ok: true, value: [HELLO] });

  sync.connecting('session-2');
  expect(sync.opened()).toEqual([
    { type: 'Connect', sessionId: 'session-1', connectionCount: 1, lastCloseReason: 'the socket closed (1006)', maxObservedTimestamp: T2, clientTs: 0 },
    { type: 'ModifyQuerySet', baseVersion: 0, newVersion: 1, modifications: [
      { type: 'Add', queryId: 0, udfPath: 'messages:list', args: [{ channel: 'general' }], journal: null },
      { type: 'Add', queryId: 1, udfPath: 'messages:count', args: [{}] } ] },
  ]);
  sync.frame(transition(version(0, 0, T0), version(1, 0, T3), [updated(0, [HELLO]), updated(1, 1)]));
  expect([sync.status, sync.rev]).toEqual(['ready', 2]);
});

// The client's `live()` refreshed starts a connection whose mapper ignores every later event of the last one, its
// close included, so connecting again ends what the last one had not answered, as its close would have.
test('connecting again ends what the last connection had not answered, and sends none of it again', async () => {
  const mappers: ((event: object, socket?: object) => unknown)[] = [];
  const fetched = globalThis.fetch;
  globalThis.fetch = ((_: string, init: { exactStream: (typeof mappers)[number] }) => { mappers.push(init.exactStream); return new Promise(() => {}); }) as never;
  try {
    const client = convex('https://example.convex.cloud', { getFunctionName: (name: unknown) => name as string, ...JSON_ONLY });
    const closed: number[] = [];
    const open = (n: number) => {
      const sent: string[] = [];
      mappers[n]({ type: 'open', data: '' }, { send: (data: string) => sent.push(JSON.parse(data).type), close() { closed.push(n); } });
      return sent;
    };
    client.live();
    open(0);
    mappers[0]({ type: 'message', data: transition(version(0, 0, T0), version(1, 0, T1)) });
    const action = client.action('billing:charge' as never, {}), mutation = client.mutation('messages:send' as never, {});
    const acknowledged = client.mutation('messages:send' as never, {});
    mappers[0]({ type: 'message', data: JSON.stringify({ type: 'MutationResponse', requestId: 2, success: true, result: 3, ts: T2, logLines: [] }) });
    client.live();
    expect(closed).toEqual([0]);
    const ended = (p: Promise<unknown>) => settled(p.then(value => ({ ok: true, value }), (e: ConvexFailure) => ({ ok: false, message: e.message, again: e.again })));
    const replaced = 'The connection to Convex closed before it answered (a new connection replaced it)';
    expect(await ended(action)).toEqual({ ok: false, message: `${replaced}; the action may have run.`, again: undefined });
    expect(await ended(mutation)).toEqual({ ok: false, message: `${replaced}; the mutation may have run.`, again: 1 });
    expect(await ended(acknowledged)).toEqual({ ok: true, value: 3 });
    expect(client.inspect().requests).toBe(0);
    expect(open(1)).toEqual(['Connect', 'ModifyQuerySet']);
  } finally { globalThis.fetch = fetched; }
});

test('a new token withholds answers until Convex confirms it, and keeps the ones it does not name', async () => {
  const { sync } = opened();
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [HELLO])]));
  expect(sync.setAuth('jwt')).toEqual([{ type: 'Authenticate', tokenType: 'User', value: 'jwt', baseVersion: 0 }]);
  const asked = sync.ask(1, 'messages:list', { channel: 'general' });
  expect(asked.read()).toBeUndefined();
  // The Transition at the new identity names no query: its result is the same for this person (recorded: Convex
  // named only the query whose result the identity changed).
  sync.frame(transition(version(1, 0, T1), version(1, 1, T2)));
  await asked.wait;
  expect(sync.ask(2, 'messages:list', { channel: 'general' }).read()).toEqual({ ok: true, value: [HELLO] });
  // The token Convex has, asked for again, sends nothing.
  expect(sync.setAuth('jwt')).toEqual([]);
  // Signed out: the same, with no token.
  expect(sync.setAuth(undefined)).toEqual([{ type: 'Authenticate', tokenType: 'None', baseVersion: 1 }]);
  expect(sync.ask(2, 'messages:list', { channel: 'general' }).read()).toBeUndefined();
});

test('a token that could not be sent leaves the identity as Convex has it', () => {
  const { sync } = opened();
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [HELLO])]));
  expect(() => sync.setAuth('jwt', () => { throw new Error('this turn may not send'); })).toThrow('this turn may not send');
  expect(sync.ask(1, 'messages:list', { channel: 'general' }).read()).toEqual({ ok: true, value: [HELLO] });
  expect(sync.setAuth('jwt')).toEqual([{ type: 'Authenticate', tokenType: 'User', value: 'jwt', baseVersion: 0 }]);
});

test('a connection opened with a token authenticates before its query set, and a refused token is dropped', () => {
  const sync = new Sync(JSON_ONLY);
  sync.connecting('s');
  sync.setAuth('jwt');
  sync.ask(0, 'me:get', {});
  expect(sync.opened().map(f => f.type)).toEqual(['Connect', 'Authenticate', 'ModifyQuerySet']);
  sync.frame(JSON.stringify({ type: 'AuthError', error: 'Could not parse JWT payload.', baseVersion: 0, authUpdateAttempted: true })); // recorded, shortened
  expect(sync.authError).toBe('Could not parse JWT payload.');
  sync.closed('the socket closed (1005)');
  sync.connecting('s');
  expect(sync.opened().map(f => f.type)).toEqual(['Connect', 'ModifyQuerySet']);
});

test('a connection Convex has pinged twice and sent no results is ended, to connect again', () => {
  const sync = new Sync(JSON_ONLY);
  sync.connecting('session-1');
  sync.ask(0, 'messages:list', {});
  sync.opened();
  expect(sync.frame(JSON.stringify({ type: 'Ping' }))).toEqual([]);
  let thrown: unknown;
  try { sync.frame(JSON.stringify({ type: 'Ping' })); } catch (e) { thrown = e; }
  expect(thrown).toBeInstanceOf(ProtocolError);
  expect([(thrown as ProtocolError).message, (thrown as ProtocolError).fatal]).toEqual(['Convex sent no results on this connection; connecting again', false]);
  // A connection Convex has answered may stay quiet as long as it likes, and each connection counts its own.
  sync.closed((thrown as Error).message);
  sync.connecting('session-1');
  sync.opened();
  sync.frame(JSON.stringify({ type: 'Ping' }));
  sync.frame(transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [])]));
  for (let i = 0; i < 3; i++) expect(sync.frame(JSON.stringify({ type: 'Ping' }))).toEqual([]);
});

test('a chunked Transition is applied once whole', () => {
  const { sync, first } = opened();
  const whole = transition(version(0, 0, T0), version(1, 0, T1), [updated(0, [HELLO])]);
  const chunk = (partNumber: number, text: string) => JSON.stringify({ type: 'TransitionChunk', chunk: text, partNumber, totalParts: 2, transitionId: 't1' });
  sync.frame(chunk(0, whole.slice(0, 40)));
  expect(sync.rev).toBe(0);
  sync.frame(chunk(1, whole.slice(40)));
  expect(first.read()).toEqual({ ok: true, value: [HELLO] });
  expect(sync.rev).toBe(1);
});

test('a frame out of place is a protocol error; one the protocol lacks is fatal', () => {
  const { sync } = opened();
  expect(() => sync.frame(transition(version(4, 0, T1), version(5, 0, T2)))).toThrow(ProtocolError);
  expect(() => sync.frame(JSON.stringify({ type: 'TransitionChunk', chunk: '', partNumber: 1, totalParts: 2, transitionId: 't' }))).toThrow(ProtocolError);
  const fatal = (text: string) => { try { sync.frame(text); } catch (e) { return (e as ProtocolError).fatal; } };
  expect(fatal(JSON.stringify({ type: 'FatalError', error: 'bad client' }))).toBe(true);
  expect(fatal(JSON.stringify({ type: 'Rebase' }))).toBe(true);
  expect(fatal(transition(version(4, 0, T1), version(5, 0, T2)))).toBe(false);
});

// What `index.d.ts` declares is what `index.ts` implements, as the type checker sees the two (`types.ts`).
test('a socket that ended failed stays failed through the close that follows', () => {
  const { sync } = opened();
  sync.closed('Convex sent a frame this client cannot follow', true);
  sync.closed('the socket closed (1000)');
  expect([sync.status, sync.error]).toEqual(['failed', 'Convex sent a frame this client cannot follow']);
  // A new connection starts over.
  sync.connecting('s2');
  expect(sync.status).toBe('connecting');
});

test('the declared API is the one implemented', () => {
  const checked = Bun.spawnSync([`${import.meta.dir}/../../../node_modules/.bin/tsc`, '--noEmit', '--strict', '--allowImportingTsExtensions', '--module', 'preserve', 'test/types.ts'], { cwd: `${import.meta.dir}/..` });
  expect(checked.exitCode, `${checked.stdout}${checked.stderr}`).toBe(0);
}, 60_000);
