// `signOut`'s `endSession` ends the person's sign-in at the provider (OpenID Connect RP-Initiated Logout 1.0 §2), so
// the next sign-in shows the provider's page again.
import { afterEach, beforeEach, expect, test } from 'bun:test';
import { oauth, type OAuthConfig, type OAuthStore } from '../index.ts';

const callback = 'com.example.app:/oauth';
const issuer = 'https://login.example.com';
const globals = globalThis as Record<string, unknown>;

let saved: PropertyDescriptor | undefined;
beforeEach(() => { saved = Object.getOwnPropertyDescriptor(globalThis, 'openAuthSession'); });
afterEach(() => {
  if (saved) Object.defineProperty(globalThis, 'openAuthSession', saved);
  else delete globals.openAuthSession;
});

const memory = (): OAuthStore => {
  const values = new Map<string, string>();
  return { get: key => values.get(key) ?? null, set: (key, value) => { values.set(key, value); }, forget: key => { values.delete(key); } };
};

// A provider whose discovery document names `endSession` as its end-session endpoint; its token replies carry no ID
// token, and it counts revocations.
function provider(endSession: string | null, extra: Partial<OAuthConfig> = {}) {
  const revoked: string[] = [];
  const transport = async (url: string, init: RequestInit): Promise<Response> => {
    if (url.endsWith('/.well-known/openid-configuration')) {
      return Response.json({
        issuer, authorization_endpoint: `${issuer}/authorize`, token_endpoint: `${issuer}/token`, revocation_endpoint: `${issuer}/revoke`,
        ...(endSession ? { end_session_endpoint: endSession } : {}),
      });
    }
    if (url.endsWith('/token')) return Response.json({ access_token: 'access', refresh_token: 'refresh', token_type: 'Bearer' });
    if (url.endsWith('/revoke')) { revoked.push(new URLSearchParams(String(init.body)).get('token')!); return new Response(null, { status: 200 }); }
    return new Response(null, { status: 404 });
  };
  const p = oauth({ issuer, clientId: 'client', key: 'end.session', scopes: [], apis: [], callback, fetch: transport, ...extra });
  return { p, revoked };
}

async function signedIn(p: ReturnType<typeof provider>['p']): Promise<OAuthStore> {
  const store = memory();
  expect((await p.signIn(store, { authorize: async (r) => `${callback}?code=c&state=${r.params.state}` })).ok).toBe(true);
  return store;
}

test('endSession opens the end-session endpoint with client_id, the callback and state, then revokes', async () => {
  const { p, revoked } = provider(`${issuer}/logout?tenant=t`);
  const store = await signedIn(p);
  const opened: { url: string; callback: string; state: string }[] = [];
  globals.openAuthSession = (url: string, o: { callback: string; state: string }) => {
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
  expect(revoked).toEqual(['refresh']);
  expect((await p.session(store)).signedIn).toBe(false);
});

test('the browser opens before sign-out awaits anything, as a web popup must', async () => {
  const { p } = provider(`${issuer}/logout`);
  const store = await signedIn(p);
  let opened = false;
  globals.openAuthSession = (_url: string, o: { callback: string; state: string }) => {
    opened = true;
    return Promise.resolve(`${o.callback}?state=${o.state}`);
  };
  const leaving = p.signOut(store, { endSession: true });
  expect(opened).toBe(true);
  expect((await leaving).ok).toBe(true);
});

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
