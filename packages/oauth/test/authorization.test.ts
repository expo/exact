import { afterEach, beforeEach, expect, test } from 'bun:test';
import { oauth, type OAuthAuthorization, type OAuthConfig, type OAuthStore } from '../index.ts';

const callback = 'com.example.app:/oauth';
const config: OAuthConfig = {
  issuer: 'https://login.example.com', clientId: 'client', key: 'authorization.session', scopes: [], apis: [], callback,
  endpoints: { authorization: 'https://login.example.com/authorize', token: 'https://login.example.com/token' },
  fetch: () => { throw new Error('No token request should be sent'); },
};
const memory = (): OAuthStore => {
  const values = new Map<string, string>();
  return { get: key => values.get(key) ?? null, set: (key, value) => { values.set(key, value); }, forget: key => { values.delete(key); } };
};
const globals = globalThis as Record<string, unknown>;
let saved: PropertyDescriptor | undefined;
beforeEach(() => { saved = Object.getOwnPropertyDescriptor(globalThis, 'openAuthSession'); });
afterEach(() => {
  if (saved) Object.defineProperty(globalThis, 'openAuthSession', saved);
  else delete globals.openAuthSession;
});

test('a failed system auth session keeps its status and reason', async () => {
  for (const message of ['the session could not start (no window)', 'The browser ended the session.']) {
    globals.openAuthSession = () => Promise.reject(Object.assign(new Error(message), { status: 502 }));
    const provider = oauth(config), store = memory();
    await provider.session(store);
    expect(await provider.signIn(store)).toEqual({
      ok: false, code: 'auth_session', error: `Sign-in could not finish: ${message}`, retryable: false, status: 502,
    });
    expect(store.get(config.key)).toBeNull();
  }
});

test('per-sign-in params override defaults, reach the browser synchronously and do not leak to the next press', async () => {
  const opened: URL[] = [];
  globals.openAuthSession = (url: string) => {
    opened.push(new URL(url));
    return Promise.reject(Object.assign(new Error('cancelled'), { status: 499 }));
  };
  const provider = oauth({ ...config, authorizationParams: { prompt: 'consent', audience: 'api' } }), store = memory();
  await provider.session(store);
  const signing = provider.signIn(store, { params: { prompt: 'select_account', login_hint: 'ada@example.com', connection_id: 'company' } });
  expect(opened).toHaveLength(1);
  expect(Object.fromEntries(opened[0].searchParams)).toMatchObject({ prompt: 'select_account', audience: 'api', login_hint: 'ada@example.com', connection_id: 'company' });
  expect((await signing).code).toBe('cancelled');
  await provider.session(store);
  await provider.signIn(store);
  expect(opened[1].searchParams.get('prompt')).toBe('consent');
  expect(opened[1].searchParams.has('login_hint')).toBe(false);
  expect(opened[1].searchParams.has('connection_id')).toBe(false);
  expect(opened[1].searchParams.get('state')).not.toBe(opened[0].searchParams.get('state'));
});

test('an unprepared sign-in and an app authorize hook receive per-sign-in params', async () => {
  const opened: URL[] = [];
  globals.openAuthSession = (url: string) => {
    opened.push(new URL(url));
    return Promise.reject(Object.assign(new Error('cancelled'), { status: 499 }));
  };
  const provider = oauth(config), store = memory();
  await provider.signIn(store, { params: { login_hint: 'ada@example.com' } });
  expect(opened[0].searchParams.get('login_hint')).toBe('ada@example.com');
  let request: OAuthAuthorization | undefined;
  const result = await provider.signIn(store, { params: { connection_id: 'company' }, authorize: async asked => {
    request = asked;
    return `${callback}?state=${asked.params.state}&error=access_denied`;
  } });
  expect(result).toMatchObject({ ok: false, code: 'access_denied', retryable: false });
  expect(request).toBeDefined();
  expect(request!.params.connection_id).toBe('company');
  expect(new URL(request!.url).searchParams.get('connection_id')).toBe('company');
  expect(request!.params.login_hint).toBeUndefined();
  expect(opened).toHaveLength(1);
});

test.each(['browser', 'authorize'])('%s sign-in snapshots params before asynchronous preparation', async path => {
  let request: URL | undefined;
  globals.openAuthSession = (url: string) => {
    request = new URL(url);
    return Promise.reject(Object.assign(new Error('cancelled'), { status: 499 }));
  };
  const params: Record<string, string> = { login_hint: 'ada@example.com' };
  const provider = oauth(config), store = memory();
  const signing = provider.signIn(store, { params, ...(path === 'authorize' ? { authorize: async (asked: OAuthAuthorization) => {
    request = new URL(asked.url);
    return `${callback}?state=${asked.params.state}&error=access_denied`;
  } } : {}) });
  params.login_hint = 'changed@example.com';
  params.max_age = '0';
  const result = await signing;
  expect(result.code).toBe(path === 'authorize' ? 'access_denied' : 'cancelled');
  expect(request).toBeDefined();
  expect(request!.searchParams.get('login_hint')).toBe('ada@example.com');
  expect(request!.searchParams.has('max_age')).toBe(false);
});

test('per-sign-in params refuse every package-owned field before consuming the prepared transaction', async () => {
  let opened = false;
  globals.openAuthSession = () => { opened = true; return Promise.reject(Object.assign(new Error('cancelled'), { status: 499 })); };
  const provider = oauth(config), store = memory();
  await provider.session(store);
  for (const name of ['response_type', 'client_id', 'redirect_uri', 'scope', 'state', 'code_challenge', 'code_challenge_method', 'nonce', 'max_age']) {
    await expect(provider.signIn(store, { params: { [name]: 'override' } })).rejects.toThrow(`params may not set ${name}`);
  }
  expect(opened).toBe(false);
  const signing = provider.signIn(store);
  expect(opened).toBe(true);
  expect((await signing).code).toBe('cancelled');
});

test('HTTPS refusals name the configuration option and allow loopback development servers', () => {
  const insecure = 'http://192.168.1.5:8080';
  const cases: [string, OAuthConfig][] = [
    ['issuer', { ...config, issuer: insecure }],
    ...(['authorization', 'token', 'revocation'] as const).map(name => [
      `endpoints.${name}`, { ...config, endpoints: { ...config.endpoints, [name]: insecure } },
    ] as [string, OAuthConfig]),
    ['apis[2]', { ...config, apis: ['https://one.example', 'https://two.example', insecure] }],
  ];
  for (const [option, value] of cases) {
    expect(() => oauth(value)).toThrow(`oauth: ${option}: ${insecure} is not HTTPS`);
  }
  for (const origin of ['http://127.0.0.1:8080', 'http://localhost:8080', 'http://[::1]:8080', 'https://dev.example']) {
    expect(() => oauth({ ...config, issuer: origin, endpoints: { authorization: `${origin}/authorize`, token: `${origin}/token`, revocation: `${origin}/revoke` }, apis: [origin] })).not.toThrow();
  }
});

test.each(['browser', 'authorize'])('%s sends and validates a nonce chosen for this sign-in', async path => {
  const nonce = 'key-bound + / nonce';
  for (const echoed of [nonce, 'wrong', undefined]) {
    let sent: string | null = null;
    const authorize = async (url: string) => {
      const q = new URL(url).searchParams;
      sent = q.get('nonce');
      return `${callback}?state=${q.get('state')}&code=one`;
    };
    globals.openAuthSession = authorize;
    const provider = oauth({ ...config, scopes: ['openid'], fetch: async () => {
      const claims = { iss: config.issuer, sub: 'ada', aud: config.clientId, exp: 4000000000, iat: 1, nonce: echoed };
      const token = `e30.${Buffer.from(JSON.stringify(claims)).toString('base64url')}.signature`;
      return Response.json({ access_token: 'access', token_type: 'Bearer', id_token: token });
    } }), store = memory();
    await provider.session(store);
    const result = await provider.signIn(store, { nonce, ...(path === 'authorize' ? { authorize: (asked: OAuthAuthorization) => authorize(asked.url) } : {}) });
    expect(sent).toBe(nonce);
    expect(result.ok).toBe(echoed === nonce);
    expect(provider.idToken(store) !== null).toBe(echoed === nonce);
    if (echoed !== nonce) expect(result.code).toBe('invalid_response');
  }
});

test('a supplied nonce is per press, validated before consumption, and opens a prepared browser synchronously', async () => {
  const opened: URL[] = [];
  globals.openAuthSession = (url: string) => {
    opened.push(new URL(url));
    return Promise.reject(Object.assign(new Error('cancelled'), { status: 499 }));
  };
  const provider = oauth({ ...config, scopes: ['openid'] }), store = memory();
  await provider.session(store);
  for (const nonce of ['', null, 7]) {
    await expect(provider.signIn(store, { nonce: nonce as string })).rejects.toThrow('nonce');
  }
  const options = { nonce: 'first' };
  const first = provider.signIn(store, options);
  options.nonce = 'changed';
  expect(opened).toHaveLength(1);
  expect(opened[0].searchParams.get('nonce')).toBe('first');
  await first;
  await provider.session(store);
  await provider.signIn(store);
  const generated = opened[1].searchParams.get('nonce');
  expect(generated).toBeTruthy();
  expect(generated).not.toBe('first');
  await expect(oauth(config).signIn(store, { nonce: 'no-openid' })).rejects.toThrow('openid');
});
