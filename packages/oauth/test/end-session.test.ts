// `signOut`'s `endSession` ends the person's sign-in at the provider (OpenID Connect RP-Initiated Logout 1.0 §2), so
// the next sign-in shows the provider's page again.
import { afterEach, beforeEach, expect, test } from 'bun:test';
import { oauth, type OAuthConfig, type OAuthStore } from '../index.ts';

const callback = 'com.example.app:/oauth';
const issuer = 'https://login.example.com';
const globals = globalThis as Record<string, unknown>;

let saved: PropertyDescriptor | undefined;
let savedPrepare: PropertyDescriptor | undefined;
let savedCancel: PropertyDescriptor | undefined;
beforeEach(() => {
  saved = Object.getOwnPropertyDescriptor(globalThis, 'openAuthSession');
  savedPrepare = Object.getOwnPropertyDescriptor(globalThis, 'prepareAuthSession');
  savedCancel = Object.getOwnPropertyDescriptor(globalThis, 'cancelAuthSession');
});
afterEach(() => {
  if (saved) Object.defineProperty(globalThis, 'openAuthSession', saved);
  else delete globals.openAuthSession;
  if (savedPrepare) Object.defineProperty(globalThis, 'prepareAuthSession', savedPrepare);
  else delete globals.prepareAuthSession;
  if (savedCancel) Object.defineProperty(globalThis, 'cancelAuthSession', savedCancel);
  else delete globals.cancelAuthSession;
});

const memory = (): OAuthStore => {
  const values = new Map<string, string>();
  return { get: key => values.get(key) ?? null, set: (key, value) => { values.set(key, value); }, forget: key => { values.delete(key); } };
};

// A provider whose discovery document names `endSession` as its end-session endpoint; its token replies carry no ID
// token, and it counts revocations.
function provider(endSession: string | null, extra: Partial<OAuthConfig> = {}) {
  const revoked: string[] = [], events: string[] = [];
  const transport = async (url: string, init: RequestInit): Promise<Response> => {
    if (url.endsWith('/.well-known/openid-configuration')) {
      return Response.json({
        issuer, authorization_endpoint: `${issuer}/authorize`, token_endpoint: `${issuer}/token`, revocation_endpoint: `${issuer}/revoke`,
        ...(endSession ? { end_session_endpoint: endSession } : {}),
      });
    }
    if (url.endsWith('/token')) return Response.json({ access_token: 'access', refresh_token: 'refresh', token_type: 'Bearer' });
    if (url.endsWith('/revoke')) { events.push('revoke'); revoked.push(new URLSearchParams(String(init.body)).get('token')!); return new Response(null, { status: 200 }); }
    return new Response(null, { status: 404 });
  };
  const p = oauth({ issuer, clientId: 'client', key: 'end.session', scopes: [], apis: [], callback, fetch: transport, ...extra });
  return { p, revoked, events };
}

async function signedIn(p: ReturnType<typeof provider>['p']): Promise<OAuthStore> {
  const store = memory();
  expect((await p.signIn(store, { authorize: async (r) => `${callback}?code=c&state=${r.params.state}` })).ok).toBe(true);
  return store;
}

test('endSession revokes before opening the end-session endpoint with client_id, the callback and state', async () => {
  const { p, revoked, events } = provider(`${issuer}/logout?tenant=t`);
  const store = await signedIn(p);
  const opened: { url: string; callback: string; state: string; logout: boolean }[] = [];
  globals.openAuthSession = (url: string, o: { callback: string; state: string; logout: boolean }) => {
    events.push('open');
    opened.push({ url, ...o });
    return Promise.resolve(`${o.callback}?state=${o.state}`);
  };
  expect(await p.signOut(store, { endSession: true })).toEqual({ ok: true, code: '', error: '', retryable: false, status: 0 });
  expect(opened.length).toBe(1);
  const url = new URL(opened[0].url);
  expect(url.origin + url.pathname).toBe(`${issuer}/logout`);
  expect(url.searchParams.get('tenant')).toBe('t');
  expect(url.searchParams.get('client_id')).toBe('client');
  expect(url.searchParams.get('post_logout_redirect_uri')).toBe(callback);
  expect(url.searchParams.get('state')).toBe(opened[0].state);
  expect(opened[0].callback).toBe(callback);
  expect(opened[0].logout).toBe(true);
  expect(revoked).toEqual(['refresh']);
  expect(events).toEqual(['revoke', 'open']);
  expect((await p.session(store)).signedIn).toBe(false);
});

test('a host revocation timeout completes local sign-out and releases the lease', async () => {
  const values = new Map([['timeout', JSON.stringify({ accessToken: 'access', refreshToken: 'refresh', revocationEndpoint: `${issuer}/revoke` })]]);
  let lane: Promise<unknown> = Promise.resolve(), nextLease = false, timeout = true;
  const sent: string[] = [];
  const store: OAuthStore = {
    get: key => values.get(key) ?? null,
    set: (key, value) => { values.set(key, value); },
    forget: key => { values.delete(key); },
    exclusive: <T>(_key: string, work: () => Promise<T>): Promise<T> => {
      const run = lane.then(work); lane = run.then(() => undefined, () => undefined); return run;
    },
  };
  const p = oauth({ issuer, clientId: 'client', key: 'timeout', scopes: [], apis: [],
    endpoints: { authorization: `${issuer}/authorize`, token: `${issuer}/token`, revocation: `${issuer}/revoke` },
    fetch: async (_url, init) => {
      expect((init as { exactTimeout?: number }).exactTimeout).toBe(60_000);
      sent.push(new URLSearchParams(String(init.body)).get('token')!);
      if (timeout) {
        timeout = false;
        throw new DOMException('The request timed out', 'TimeoutError');
      }
      return new Response(null, { status: 200 });
    } });
  expect((await p.signOut(store)).ok).toBe(true);
  await store.exclusive!('timeout', async () => { nextLease = true; });
  expect(nextLease).toBe(true);
  expect(values.has('timeout')).toBe(false);
  expect(sent).toEqual(['refresh']);
  sent.length = 0;
  expect((await p.signOut(store)).ok).toBe(true);
  expect(sent).toEqual(['refresh']);
});

test('a 400 revocation reply drops the token instead of retrying it', async () => {
  const key = 'revoke.400', sent: string[] = [], store = memory();
  store.set(key, JSON.stringify({ accessToken: 'access', refreshToken: 'refresh', revocationEndpoint: `${issuer}/revoke` }));
  const p = oauth({ issuer, clientId: 'client', key, scopes: [], apis: [],
    endpoints: { authorization: `${issuer}/authorize`, token: `${issuer}/token`, revocation: `${issuer}/revoke` },
    fetch: async (url, init) => {
      if (url.endsWith('/revoke')) sent.push(new URLSearchParams(String(init.body)).get('token')!);
      return new Response(null, { status: 400 });
    } });
  expect((await p.signOut(store)).ok).toBe(true);
  expect((await p.signOut(store)).ok).toBe(true);
  expect(sent).toEqual(['refresh']);
});

test('sign-out binds logout and revocation to the session read under its lease', async () => {
  const { p, revoked } = provider(`${issuer}/logout`);
  const sessions = new Map<string, string>();
  const kept = (who: string) => JSON.stringify({
    accessToken: `access-${who}`, refreshToken: `refresh-${who}`, idToken: `id-${who}`,
    endSessionEndpoint: `${issuer}/logout`, revocationEndpoint: `${issuer}/revoke`,
  });
  sessions.set('end.session', kept('alice'));
  let enter!: () => void;
  const queued = new Promise<void>(resolve => { enter = resolve; });
  let release!: () => void;
  const lease = new Promise<void>(resolve => { release = resolve; });
  const store: OAuthStore = {
    get: key => sessions.get(key) ?? null,
    set: (key, value) => { sessions.set(key, value); },
    forget: key => { sessions.delete(key); },
    exclusive: (_key, next) => { enter(); return lease.then(next); },
  };
  const opened: string[] = [];
  globals.openAuthSession = (url: string, o: { callback: string; state: string }) => {
    opened.push(url);
    return Promise.resolve(`${o.callback}?state=${o.state}`);
  };
  const leaving = p.signOut(store, { endSession: true });
  await queued;
  sessions.set('end.session', kept('bob'));
  release();
  expect((await leaving).ok).toBe(true);
  expect(new URL(opened[0]).searchParams.get('id_token_hint')).toBe('id-bob');
  expect(revoked).toEqual(['refresh-bob']);
});

test("sign-out reserves the press's browser window while its leased session is pending", async () => {
  const { p, revoked } = provider(`${issuer}/logout`);
  const sessions = new Map<string, string>();
  const kept = (who: string) => JSON.stringify({
    accessToken: `access-${who}`, refreshToken: `refresh-${who}`, idToken: `id-${who}`,
    endSessionEndpoint: `${issuer}/logout`, revocationEndpoint: `${issuer}/revoke`, login: who,
  });
  sessions.set('end.session', kept('alice'));
  let queued!: () => void;
  const waiting = new Promise<void>(resolve => { queued = resolve; });
  let release!: () => void;
  const lease = new Promise<void>(resolve => { release = resolve; });
  const store: OAuthStore = {
    get: key => sessions.get(key) ?? null,
    set: (key, value) => { sessions.set(key, value); },
    forget: key => { sessions.delete(key); },
    exclusive: (_key, next) => { queued(); return lease.then(next); },
  };
  let active = true;
  const prepared = new Set<string>();
  globals.prepareAuthSession = () => {
    if (!active) return null;
    prepared.add('pressed-window');
    return 'pressed-window';
  };
  globals.cancelAuthSession = (id: string) => { prepared.delete(id); };
  const opened: string[] = [];
  globals.openAuthSession = (url: string, o: { callback: string; state: string; prepared?: string }) => {
    if (!o.prepared || !prepared.delete(o.prepared)) return Promise.reject(Object.assign(new Error('popup blocked'), { status: 428 }));
    opened.push(url);
    return Promise.resolve(`${o.callback}?state=${o.state}`);
  };
  const leaving = p.signOut(store, { endSession: true });
  await waiting;
  sessions.set('end.session', kept('bob'));
  active = false;
  release();
  expect((await leaving).code).toBe('');
  expect(new URL(opened[0]).searchParams.get('id_token_hint')).toBe('id-bob');
  expect(revoked).toEqual(['refresh-bob']);
});

test('a provider logout that could not open is retained for a later sign-out press', async () => {
  const { p } = provider(`${issuer}/logout`);
  const store = memory();
  store.set('end.session', JSON.stringify({
    accessToken: 'access-alice', refreshToken: 'refresh-alice', idToken: 'id-alice', login: 'alice',
    endSessionEndpoint: `${issuer}/logout`, revocationEndpoint: `${issuer}/revoke`,
  }));
  let prepared = 0, opened = 0;
  globals.prepareAuthSession = () => `window-${++prepared}`;
  globals.cancelAuthSession = () => {};
  globals.openAuthSession = (url: string, o: { callback: string; state: string }) => {
    opened++;
    expect(new URL(url).searchParams.get('id_token_hint')).toBe('id-alice');
    if (opened === 1) return Promise.reject(Object.assign(new Error('popup blocked'), { status: 428 }));
    return Promise.resolve(`${o.callback}?state=${o.state}`);
  };
  expect(await p.signOut(store, { endSession: true })).toMatchObject({ ok: true, code: 'end_session' });
  expect(JSON.parse(store.get('end.session')!)).toEqual({
    pendingLogout: { id: 'alice', endpoint: `${issuer}/logout`, idToken: 'id-alice' },
  });
  expect(await p.signOut(store, { endSession: true })).toMatchObject({ ok: true, code: '' });
  expect(opened).toBe(2);
  expect(store.get('end.session')).toBeNull();
});

test("only the addressed provider return completes its sign-out", async () => {
  const store = memory();
  const pending = (id: string) => JSON.stringify({
    pendingLogout: { id, endpoint: `${issuer}/logout`, idToken: `id-${id}` },
  });
  store.set('addressed.alice', pending('alice'));
  store.set('addressed.bob', pending('bob'));
  const configured = (key: string) => oauth({
    issuer, clientId: 'client', key, scopes: [], apis: [], callback,
    endpoints: { authorization: `${issuer}/authorize`, token: `${issuer}/token`, endSession: `${issuer}/logout` },
  });
  globals.prepareAuthSession = (() => { let next = 0; return () => `window-${++next}`; })();
  globals.cancelAuthSession = () => {};
  const returns = new Map<string, { resolve: (url: string) => void; reject: (error: Error) => void }>();
  let bothOpened!: () => void;
  const opened = new Promise<void>(resolve => { bothOpened = resolve; });
  globals.openAuthSession = (url: string) => new Promise<string>((resolve, reject) => {
    returns.set(new URL(url).searchParams.get('id_token_hint')!, { resolve, reject });
    if (returns.size === 2) bothOpened();
  });

  const alice = configured('addressed.alice').signOut(store, { endSession: true });
  const bob = configured('addressed.bob').signOut(store, { endSession: true });
  await opened;
  returns.get('id-bob')!.resolve(callback);
  expect(await bob).toEqual({ ok: true, code: '', error: '', retryable: false, status: 0 });
  expect(store.get('addressed.bob')).toBeNull();
  expect(JSON.parse(store.get('addressed.alice')!)).toEqual({
    pendingLogout: { id: 'alice', endpoint: `${issuer}/logout`, idToken: 'id-alice' },
  });

  let aliceSettled = false;
  alice.then(() => { aliceSettled = true; });
  await Promise.resolve();
  expect(aliceSettled).toBe(false);
  returns.get('id-alice')!.reject(Object.assign(new Error('cancelled'), { status: 499 }));
  expect(await alice).toMatchObject({ ok: true, code: 'end_session', retryable: false });
  expect(JSON.parse(store.get('addressed.alice')!)).toEqual({
    pendingLogout: { id: 'alice', endpoint: `${issuer}/logout`, idToken: 'id-alice' },
  });
});

test('a later sign-in replaces a retained provider hint', async () => {
  const { p } = provider(`${issuer}/logout`);
  const store = memory();
  store.set('end.session', JSON.stringify({
    accessToken: 'access-alice', refreshToken: 'refresh-alice', idToken: 'id-alice', login: 'alice',
    endSessionEndpoint: `${issuer}/logout`, revocationEndpoint: `${issuer}/revoke`,
  }));
  globals.openAuthSession = () => Promise.reject(Object.assign(new Error('popup blocked'), { status: 428 }));
  expect(await p.signOut(store, { endSession: true })).toMatchObject({ ok: true, code: 'end_session' });
  expect(JSON.parse(store.get('end.session')!).pendingLogout.idToken).toBe('id-alice');
  expect((await p.signIn(store, { authorize: async (r) => `${callback}?code=c&state=${r.params.state}` })).ok).toBe(true);
  expect(store.get('end.session')).not.toContain('id-alice');
  let opened = '';
  globals.openAuthSession = (url: string, o: { callback: string; state: string }) => {
    opened = url;
    return Promise.resolve(`${o.callback}?state=${o.state}`);
  };
  expect((await p.signOut(store, { endSession: true })).ok).toBe(true);
  expect(new URL(opened).searchParams.get('id_token_hint')).toBeNull();
  expect(store.get('end.session') ?? '').not.toContain('id-alice');
});

test('a prepared window is cancelled when no leased logout can use it', async () => {
  const { p } = provider(`${issuer}/logout`);
  let next = 0;
  const cancelled: string[] = [];
  globals.prepareAuthSession = () => `window-${++next}`;
  globals.cancelAuthSession = (id: string) => { cancelled.push(id); };
  globals.openAuthSession = () => { throw new Error('must not open'); };
  expect((await p.signOut(memory(), { endSession: true })).code).toBe('');
  const failed: OAuthStore = {
    ...memory(),
    exclusive: () => Promise.reject(new Error('lease failed')),
  };
  await expect(p.signOut(failed, { endSession: true })).rejects.toThrow('lease failed');
  expect(cancelled).toEqual(['window-1', 'window-2']);
});

test('a native sign-out opens after its lease without a prepared browser window', async () => {
  const { p } = provider(`${issuer}/logout`);
  const store = await signedIn(p);
  let release!: () => void;
  const lease = new Promise<void>(resolve => { release = resolve; });
  const native: OAuthStore = { ...store, exclusive: (_key, next) => lease.then(next) };
  globals.prepareAuthSession = () => null;
  let opened = false;
  globals.openAuthSession = (_url: string, o: { callback: string; state: string }) => {
    opened = true;
    return Promise.resolve(`${o.callback}?state=${o.state}`);
  };
  const leaving = p.signOut(native, { endSession: true });
  expect(opened).toBe(false);
  release();
  expect((await leaving).code).toBe('');
  expect(opened).toBe(true);
});

test('overlapping sign-outs revoke each distinct token exactly once', async () => {
  const revoked: string[] = [];
  let aliceReached!: () => void;
  const aliceStarted = new Promise<void>(resolve => { aliceReached = resolve; });
  let releaseAlice!: () => void;
  const aliceHeld = new Promise<void>(resolve => { releaseAlice = resolve; });
  const requests: string[] = [];
  const transport = async (url: string, init: RequestInit): Promise<Response> => {
    if (url.endsWith('/token')) {
      requests.push('token-bob');
      return Response.json({ access_token: 'access-bob', refresh_token: 'refresh-bob', token_type: 'Bearer' });
    }
    if (!url.endsWith('/revoke')) return new Response(null, { status: 404 });
    const token = new URLSearchParams(String(init.body)).get('token')!;
    requests.push(token);
    revoked.push(token);
    if (token === 'refresh-alice') { aliceReached(); await aliceHeld; }
    return new Response(null, { status: 200 });
  };
  const p = oauth({
    issuer, clientId: 'client', key: 'overlap', scopes: [], apis: [], callback, fetch: transport,
    endpoints: { authorization: `${issuer}/authorize`, token: `${issuer}/token`, revocation: `${issuer}/revoke` },
  });
  const sessions = new Map([['overlap', JSON.stringify({
    accessToken: 'access-alice', refreshToken: 'refresh-alice', revocationEndpoint: `${issuer}/revoke`,
  })]]);
  let lane: Promise<unknown> = Promise.resolve(), entered = 0;
  const waiters: Array<() => void> = [];
  const store: OAuthStore = {
    get: key => sessions.get(key) ?? null,
    set: (key, value) => { sessions.set(key, value); },
    forget: key => { sessions.delete(key); },
    exclusive: <T>(_key: string, work: () => Promise<T>): Promise<T> => {
      entered++;
      for (const done of waiters.splice(0)) done();
      const run = lane.then(work);
      lane = run.then(() => undefined, () => undefined);
      return run;
    },
  };
  const queued = (count: number): Promise<void> => entered >= count ? Promise.resolve() : new Promise(resolve => waiters.push(resolve));
  const alice = p.signOut(store);
  await aliceStarted;
  const bobSignIn = p.signIn(store, { authorize: async request => `${callback}?code=bob&state=${request.params.state}` });
  await queued(2);
  const bob = p.signOut(store);
  await queued(3);
  expect(requests).toEqual(['refresh-alice']);
  releaseAlice();
  expect((await alice).ok).toBe(true);
  expect((await bobSignIn).ok).toBe(true);
  expect((await bob).ok).toBe(true);
  expect((await p.signOut(store)).ok).toBe(true);
  expect((await p.signOut(store)).ok).toBe(true);
  expect(revoked).toEqual(['refresh-alice', 'refresh-bob']);
  expect(requests).toEqual(['refresh-alice', 'token-bob', 'refresh-bob']);
}, 2000);

test("logoutRedirectParam names the callback's parameter for a provider that calls it otherwise", async () => {
  const { p } = provider(`${issuer}/logout`, { logoutRedirectParam: 'returnTo' });
  const store = await signedIn(p);
  let url = '';
  globals.openAuthSession = (u: string, o: { callback: string; state: string }) => { url = u; return Promise.resolve(`${o.callback}?state=${o.state}`); };
  await p.signOut(store, { endSession: true });
  const q = new URL(url).searchParams;
  expect([q.get('returnTo'), q.has('post_logout_redirect_uri')]).toEqual([callback, false]);
});

test('a browser step that does not finish still signs out, and says so', async () => {
  const { p, revoked } = provider(`${issuer}/logout`);
  const store = await signedIn(p);
  globals.openAuthSession = () => Promise.reject(Object.assign(new Error('cancelled'), { status: 499 }));
  const r = await p.signOut(store, { endSession: true });
  expect(r).toMatchObject({ ok: true, code: 'end_session', retryable: false });
  expect(r.error).toContain('cancelled');
  expect(revoked).toEqual(['refresh']);
  expect((await p.session(store)).signedIn).toBe(false);
});

test('a provider without an end-session endpoint signs out, and says the browser step was not taken', async () => {
  const { p } = provider(null);
  const store = await signedIn(p);
  let opened = false;
  globals.openAuthSession = () => { opened = true; return Promise.reject(new Error('not opened')); };
  expect(await p.signOut(store, { endSession: true })).toMatchObject({ ok: true, code: 'end_session' });
  expect(opened).toBe(false);
  expect((await p.session(store)).signedIn).toBe(false);
});

test('fixed endpoints take endSession, and a sign-out without the option opens no browser', async () => {
  const transport = async (url: string): Promise<Response> =>
    url.endsWith('/token') ? Response.json({ access_token: 'a', token_type: 'Bearer' }) : new Response(null, { status: 404 });
  const p = oauth({
    issuer, clientId: 'client', key: 'end.fixed', scopes: [], apis: [], callback, fetch: transport,
    endpoints: { authorization: `${issuer}/authorize`, token: `${issuer}/token`, endSession: `${issuer}/v2/logout` },
  });
  let urls: string[] = [];
  globals.openAuthSession = (u: string, o: { callback: string; state: string }) => { urls.push(u); return Promise.resolve(`${o.callback}?state=${o.state}`); };
  let store = await signedIn(p);
  expect((await p.signOut(store)).ok).toBe(true);
  expect(urls).toEqual([]);
  store = await signedIn(p);
  expect((await p.signOut(store, { endSession: true })).code).toBe('');
  expect(urls.map(u => u.split('?')[0])).toEqual([`${issuer}/v2/logout`]);
  urls = [];
  // Signed out, there is no session to end
  expect((await p.signOut(store, { endSession: true })).code).toBe('');
  expect(urls).toEqual([]);
});
