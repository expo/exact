// A request refused while it was out is sent again only as the sign-in it was sent as. The provider issues no ID
// token, so a sign-in has no subject to tell accounts apart by.
import { expect, test } from 'bun:test';
import { handle, signedInSession } from './fixture.ts';
import { oauth, OAuthError, type OAuthAuthorization, type OAuthStore } from '../index.ts';

const callback = 'com.example.app:/oauth';
const api = 'https://api.example.com';

const memory = (): OAuthStore => {
  const values = new Map<string, string>();
  return { get: key => values.get(key) ?? null, set: (key, value) => { values.set(key, value); }, forget: key => { values.delete(key); } };
};

// The authorization code names the account; its tokens carry that name. The API's first request is held until
// `release`, then refused with 401; any later one is answered 200.
function provider() {
  const sent: string[] = [], refreshes: string[] = [];
  let release = (): void => undefined, out = (): void => undefined;
  const held = new Promise<void>((done) => { release = done; });
  const going = new Promise<void>((done) => { out = done; });
  const transport = async (url: string, init: RequestInit): Promise<Response> => {
    if (url.endsWith('/token')) {
      const body = new URLSearchParams(String(init.body));
      const name = body.get('grant_type') === 'refresh_token' ? `${body.get('refresh_token')!.split('-')[0]}-renewed` : body.get('code')!;
      if (body.get('grant_type') === 'refresh_token') refreshes.push(body.get('refresh_token')!);
      return Response.json({ access_token: name, refresh_token: `${name.split('-')[0]}-refresh-${refreshes.length}`, token_type: 'Bearer' });
    }
    if (url.endsWith('/revoke')) return new Response(null, { status: 200 });
    sent.push(`${init.method ?? 'GET'} ${new Headers(init.headers).get('authorization')} ${init.body ?? ''}`.trim());
    if (sent.length > 1) return new Response('{}', { status: 200 });
    out();
    await held;
    return new Response(null, { status: 401 });
  };
  const p = oauth({
    issuer: 'https://login.example.com', clientId: 'client', key: 'account.session', scopes: [], apis: [api], callback,
    endpoints: { authorization: 'https://login.example.com/authorize', token: 'https://login.example.com/token', revocation: 'https://login.example.com/revoke' },
    fetch: transport,
  });
  const as = (account: string) => ({ authorize: async (r: OAuthAuthorization) => `${callback}?code=${account}&state=${r.params.state}` });
  return { p, as, sent, refreshes, release, going };
}

for (const [method, body] of [['GET', undefined], ['POST', '{}'], ['DELETE', undefined]] as const) {
  test(`a ${method} refused after a sign-out and another account's sign-in is not sent as that account`, async () => {
    const { p, as, sent, refreshes, release, going } = provider(), store = memory();
    expect((await p.signIn(store, as('alice'))).ok).toBe(true);
    const request = p.fetch(store, `${api}/files`, { method, body }).then(() => null, (e: unknown) => e);
    await going;
    expect((await p.signOut(store)).ok).toBe(true);
    expect((await p.signIn(store, as('bob'))).ok).toBe(true);
    release();
    const error = await request;
    expect(error).toBeInstanceOf(OAuthError);
    expect(error).toMatchObject({ code: 'session_changed', signedOut: false });
    expect(sent).toEqual([`${method} Bearer alice ${body ?? ''}`.trim()]);
    expect(refreshes).toEqual([]);
  });
}

test('a request refused after a sign-in over the session, as the same account too, is not sent again', async () => {
  const { p, as, sent, refreshes, release, going } = provider(), store = memory();
  await p.signIn(store, as('alice'));
  const request = p.fetch(store, `${api}/files`, { method: 'POST', body: '{}' }).then(() => null, (e: unknown) => e);
  await going;
  await p.signIn(store, as('alice'));
  release();
  expect(await request).toMatchObject({ code: 'session_changed' });
  expect([sent, refreshes]).toEqual([['POST Bearer alice {}'], []]);
});

test('a request refused after a sign-out alone is not sent again', async () => {
  const { p, as, sent, release, going } = provider(), store = memory();
  await p.signIn(store, as('alice'));
  const request = p.fetch(store, `${api}/files`).then(() => null, (e: unknown) => e);
  await going;
  await p.signOut(store);
  release();
  expect(await request).toMatchObject({ code: 'signed_out', signedOut: true });
  expect(sent).toEqual(['GET Bearer alice']);
});

test("a request refused after the same sign-in's refresh is sent again with the refreshed token", async () => {
  const { p, as, sent, refreshes, release, going } = provider(), store = memory();
  await p.signIn(store, as('alice'));
  const request = p.fetch(store, `${api}/files`, { method: 'POST', body: '{}' });
  await going;
  expect((await p.refresh(store)).ok).toBe(true);
  release();
  expect((await request).status).toBe(200);
  expect([sent, refreshes]).toEqual([['POST Bearer alice {}', 'POST Bearer alice-renewed {}'], ['alice-refresh-0']]);
});

test('the first read from a prepared store and its refresh use the selected account', async () => {
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch: handle });
  try {
    const origin = server.url.origin, key = 'spec.session', clientId = 'client-spec';
    const p = oauth({ issuer: origin + '/p/spec', key, clientId, apis: [origin], scopes: ['openid', 'offline_access'] });
    const person = { sub: 'reader-2', name: 'Grace Hopper', email: 'grace@example.test' };
    const grace = memory(), ada = memory();
    grace.set(key, await signedInSession(origin, key, { clientId, ...person, token: 'test-reader' }));
    ada.set(key, await signedInSession(origin, key, { clientId, sub: 'reader-1' }));
    expect(p.accessToken(grace)).toBe('test-reader');
    expect(p.claims(grace)).toMatchObject(person);
    expect((await p.session(grace)).signedIn).toBe(true);
    const me = (store: OAuthStore) => p.json(store, origin + '/p/spec/userinfo');
    expect(await me(grace)).toEqual(person);
    expect((await fetch(origin + '/p/okta/userinfo', { headers: { Authorization: 'Bearer ' + p.accessToken(grace) } })).status).toBe(401);
    await fetch(origin + '/control', { method: 'POST', body: JSON.stringify({ action: 'expire-access', profile: 'spec' }) });
    expect(await me(grace)).toEqual(person);
    expect(p.accessToken(grace)).not.toBe('test-reader');
    expect(p.claims(grace)).toMatchObject(person);
    expect(await me(ada)).toMatchObject({ sub: 'reader-1' });
  } finally { server.stop(true); }
});
