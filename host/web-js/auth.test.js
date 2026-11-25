import { test, expect, jest, mock } from "bun:test";

const clock = { now: 0, agent: true };
const journal = [];
let ticket = 0;
let currentOrigin = "app";
const personRequired = [];
const Origin = { now: () => currentOrigin, personRequired: id => personRequired.push(id) };
const Invocation = { id: 0 };
mock.module("./rt.js", () => ({ clock, journal, nextTicket: () => ++ticket, Origin, Invocation }));
const realAuthGlue = await import("../web/auth-glue.js?web-js-auth-test");
let glueImported = false;
mock.module("./auth-glue.js", () => {
  glueImported = true;
  (globalThis.exact ??= {}).authHost = realAuthGlue;
  return realAuthGlue;
});
const { install, holds, answer } = await import("./auth.js");

test("authorization URL userinfo is refused without disclosure", async () => {
  globalThis.location = new URL("https://app.example/");
  clock.agent = true;
  const grants = "auth.session https://*.example.com\nauth.callback https://app.example/.exact/auth/callback";
  install(grants, () => "login");
  const before = journal.length;
  const url = "https://user:review-secret@login.example.com/authorize?code=hidden";
  const pending = openAuthSession(url, { callback: authCallback(), state: "s" });
  for (const hold of holds()) answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
  const error = await pending.catch(e => e);
  expect(error).toBeInstanceOf(Error);
  expect(error.message).toContain("userinfo");
  expect(error.message).not.toContain("review-secret");
  expect(JSON.stringify(holds())).not.toContain("review-secret");
  expect(journal.slice(before).join("\n")).not.toContain("review-secret");
});

test("callback userinfo is refused without disclosure", async () => {
  globalThis.location = new URL("https://app.example/");
  clock.agent = true;
  install("auth.session https://login.example.com", () => "login");
  const pending = openAuthSession("https://login.example.com/authorize", { callback: "https://user:review-secret@app.example/cb", state: "s" });
  for (const hold of holds()) answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
  const error = await pending.catch(e => e);
  expect(error.message).toContain("userinfo");
  expect(error.message).not.toContain("review-secret");
});

test("single-slash https callback cannot disclose userinfo", async () => {
  globalThis.location = new URL("https://app.example/");
  clock.agent = true;
  install("auth.session https://login.example.com", () => "login");
  const pending = openAuthSession("https://login.example.com/authorize", { callback: "https:/user:review-secret@app.example/cb", state: "s" });
  for (const hold of holds()) answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
  const error = await pending.catch(e => e);
  expect(error.status).toBe(502);
  expect(error.message).not.toContain("review-secret");
});

test("backslashed or tabbed https callbacks cannot disclose userinfo", async () => {
  for (const callback of ["https:\\/user:review-secret@app.example/cb", "https://\\/user:review-secret@app.example/cb", "https://app.example/c\tb@user:review-secret"]) {
    globalThis.location = new URL("https://app.example/");
    clock.agent = true;
    install("auth.session https://login.example.com", () => "login");
    const pending = openAuthSession("https://login.example.com/authorize", { callback, state: "s" });
    for (const hold of holds()) answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
    const error = await pending.catch(e => e);
    expect(error.status).toBe(502);
    expect(error.message).not.toContain("review-secret");
  }
});

// The drive's auth fixture is its launch's (LLP 1146 D4), not a query the app's route may hold: a held sign-in at the
// fixture the page launched with shows its URL whole wherever the page is
test("a held sign-in at the drive's launched auth fixture shows its URL whole at any route", async () => {
  const entries = performance.getEntriesByType;
  performance.getEntriesByType = type => type === "navigation" ? [{ name: "https://app.example/?agent=1&authFixture=http%3A%2F%2F127.0.0.1%3A4400" }] : entries.call(performance, type);
  globalThis.location = new URL("https://app.example/next?authFixture=http%3A%2F%2F127.0.0.1%3A5");
  clock.agent = true;
  install("auth.session http://127.0.0.1:4400\nauth.callback https://app.example/.exact/auth/callback", () => "login");
  try {
    const url = "http://127.0.0.1:4400/authorize?code=shown";
    const pending = openAuthSession(url, { callback: authCallback(), state: "s" });
    const [hold] = holds();
    expect(hold.device.args.url).toBe(url);
    answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
    expect((await pending.catch(e => e)).status).toBe(499);
  } finally { performance.getEntriesByType = entries; }
});

test("ordinary exact-origin sessions still reach a held callback", async () => {
  globalThis.location = new URL("https://app.example/");
  clock.agent = true;
  install("auth.session https://login.example.com\nauth.callback https://app.example/.exact/auth/callback", () => "login");
  const pending = openAuthSession("https://login.example.com/authorize?code=hidden", { callback: authCallback(), state: "s" });
  const [hold] = holds();
  expect(hold.device.args.origin).toBe("https://login.example.com");
  const callback = "https://app.example/.exact/auth/callback?state=s&code=returned";
  answer({ ticket: hold.ticket, op: "type", text: callback });
  expect(await pending).toBe(callback);
});

test("a loopback callback answers as the runner's arm does on the web (LLP 1133.005 D9)", async () => {
  globalThis.location = new URL("http://localhost:8080/");
  clock.agent = true;
  install("auth.session https://login.example.com\nauth.callback http://127.0.0.1/oauth/loopback", () => "login");
  const failed = (callback) => openAuthSession("https://login.example.com/authorize", { callback, state: "s" }).catch(e => e);
  // Granted at any port, then 501: a page cannot listen; the scheme in any case, as the runner reads it.
  for (const callback of ["http://127.0.0.1:4332/oauth/loopback", "HTTP://127.0.0.1:4332/oauth/loopback"]) {
    const loopback = await failed(callback);
    expect([loopback.status, loopback.message]).toEqual([501, "a page cannot listen"]);
  }
  // Not granted, or not D9's form (a port `Host` would not carry as written): the grants refuse it first.
  for (const callback of ["http://127.0.0.1:4332/other", "http://127.0.0.1:80/oauth/loopback", "http://127.0.0.1:04332/oauth/loopback", "http://localhost:4332/oauth/loopback"])
    expect((await failed(callback)).status).toBe(403);
  // The page's own callback on a loopback development origin is held as ever.
  const page = openAuthSession("https://login.example.com/authorize", { callback: authCallback(), state: "s" });
  expect(holds().length).toBe(1);
  for (const hold of holds()) answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
  expect((await page.catch(e => e)).status).toBe(499);
});

test("a state outside the BMP is matched whole, as auth.rs decodes it", async () => {
  globalThis.location = new URL("https://app.example/");
  clock.agent = true;
  install("auth.session https://login.example.com\nauth.callback https://app.example/cb", () => "login");
  const pending = openAuthSession("https://login.example.com/authorize", { callback: "https://app.example/cb", state: "s😀" });
  const [hold] = holds();
  answer({ ticket: hold.ticket, op: "type", text: "https://app.example/cb?code=c&state=s😀" });
  expect(await pending).toBe("https://app.example/cb?code=c&state=s😀");
});

test('auth grants trim Rust whitespace and keep FEFF as part of a declaration', async () => {
  globalThis.location = new URL('https://app.example/');
  clock.agent = true;
  install('\u0085auth.session https://login.example\u0085\nauth.callback https://app.example/cb', () => 'login');
  const pending = openAuthSession('https://login.example/authorize', { callback: 'https://app.example/cb', state: 's' });
  expect(holds()).toHaveLength(1);
  answer({ ticket: holds()[0].ticket, op: 'tap', choice: 'cancel' });
  await expect(pending).rejects.toMatchObject({ status: 499 });
  install('\ufeffauth.session https://login.example\nauth.callback https://app.example/cb', () => 'login');
  await expect(openAuthSession('https://login.example/authorize', { callback: 'https://app.example/cb', state: 's' })).rejects.toMatchObject({ status: 403 });
});

test('a blank auth window is reserved at the press before deferred glue loads', () => {
  const priorWindow = globalThis.window;
  const priorNavigator = globalThis.navigator;
  const priorFrame = globalThis.requestAnimationFrame;
  const priorExact = globalThis.exact;
  const opened = [];
  clock.agent = false;
  globalThis.window = { open: url => {
    const popup = { closed: false, url, close() { this.closed = true; } };
    opened.push(popup);
    return popup;
  } };
  globalThis.navigator = { userActivation: { isActive: true } };
  globalThis.requestAnimationFrame = undefined;
  globalThis.exact = {};
  try {
    install('auth.session https://login.example.com', () => 'logout');
    const prepared = prepareAuthSession();
    expect(typeof prepared).toBe('string');
    expect(opened.map(popup => popup.url)).toEqual(['about:blank']);
    cancelAuthSession(prepared);
    expect(opened[0].closed).toBe(true);
  } finally {
    clock.agent = true;
    globalThis.window = priorWindow;
    globalThis.navigator = priorNavigator;
    globalThis.requestAnimationFrame = priorFrame;
    globalThis.exact = priorExact;
  }
});

test('an unclaimed auth reservation closes at its bound', () => {
  const priorWindow = globalThis.window;
  const priorNavigator = globalThis.navigator;
  const priorExact = globalThis.exact;
  const popup = { closed: false, close() { this.closed = true; } };
  clock.agent = false;
  globalThis.window = { open: () => popup };
  globalThis.navigator = { userActivation: { isActive: true } };
  globalThis.exact = {};
  jest.useFakeTimers();
  try {
    install('auth.session https://login.example.com', () => 'logout');
    expect(typeof prepareAuthSession()).toBe('string');
    jest.advanceTimersByTime(10 * 60 * 1000);
    expect(popup.closed).toBe(true);
    expect(globalThis.exact.authReservations.size).toBe(0);
  } finally {
    jest.useRealTimers();
    clock.agent = true;
    globalThis.window = priorWindow;
    globalThis.navigator = priorNavigator;
    globalThis.exact = priorExact;
  }
});

test('an agent-fulfilled auth request closes its unadopted prepared window', async () => {
  const priorWindow = globalThis.window;
  const priorNavigator = globalThis.navigator;
  const priorExact = globalThis.exact;
  const popup = { closed: false, close() { this.closed = true; } };
  clock.agent = true;
  globalThis.window = { open: () => popup };
  globalThis.navigator = { userActivation: { isActive: true } };
  globalThis.location = new URL('https://app.example/');
  globalThis.exact = {};
  try {
    install('auth.session https://login.example.com\nauth.callback https://app.example/.exact/auth/callback', () => 'logout');
    const prepared = prepareAuthSession();
    const pending = openAuthSession('https://login.example.com/logout', { callback: authCallback(), state: 's', logout: true, prepared });
    const [hold] = holds();
    answer({ ticket: hold.ticket, op: 'type', text: 'https://app.example/.exact/auth/callback?state=s' });
    await pending;
    expect(popup.closed).toBe(true);
  } finally {
    globalThis.window = priorWindow;
    globalThis.navigator = priorNavigator;
    globalThis.exact = priorExact;
  }
});

test.each(['a clock-adjusted auth timeout', 'a logout opener response', 'a logout identity response'])('%s releases the JS target', async mode => {
  const names = ['window', 'navigator', 'location', 'performance', 'BroadcastChannel',
    'requestAnimationFrame', 'addEventListener', 'removeEventListener', 'setTimeout',
    'clearTimeout', 'setInterval', 'clearInterval', 'exact'];
  const saved = names.map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]);
  const savedNow = Date.now;
  const timers = [], popups = [];
  let timerNumber = 0, listener = null, channel = null, mono = 0, wall = 10_000, target = 'first';
  const timer = (callback, delay, repeat) => {
    const value = { id: ++timerNumber, callback, delay: Number(delay), repeat, active: true };
    timers.push(value);
    return value;
  };
  const clear = value => { if (value) value.active = false; };
  const run = delay => {
    const value = timers.find(candidate => candidate.active && candidate.delay === delay);
    if (!value) throw new Error(`no live ${delay} ms timer`);
    if (!value.repeat) value.active = false;
    value.callback();
  };
  const define = (name, value) => Object.defineProperty(globalThis, name, { configurable: true, writable: true, value });
  define('setTimeout', (callback, delay = 0) => timer(callback, delay, false));
  define('clearTimeout', clear);
  define('setInterval', (callback, delay = 0) => timer(callback, delay, true));
  define('clearInterval', clear);
  define('performance', { timeOrigin: 1000, now: () => mono });
  define('location', { origin: 'https://app.example', search: '' });
  define('navigator', { userActivation: { isActive: true } });
  define('BroadcastChannel', class { constructor() { channel = this; } close() {} });
  define('addEventListener', (type, value) => { if (type === 'message') listener = value; });
  define('removeEventListener', (type, value) => { if (type === 'message' && listener === value) listener = null; });
  define('requestAnimationFrame', callback => { callback(); return 1; });
  define('exact', { authHost: realAuthGlue });
  define('window', { open(url) {
    const values = new Map();
    const popup = {
      closed: false,
      location: { href: String(url) },
      sessionStorage: {
        getItem: key => values.get(key) ?? null,
        setItem: (key, value) => values.set(key, String(value)),
        removeItem: key => values.delete(key),
      },
      close() { this.closed = true; },
    };
    popups.push(popup);
    return popup;
  } });
  Date.now = () => wall;
  clock.agent = false;
  try {
    install('auth.session https://login.example.com\nauth.callback https://app.example/.exact/auth/callback', () => target);
    run(0);
    await import('./auth-glue.js');
    await Promise.resolve();
    expect(glueImported).toBe(true);

    if (mode !== 'a clock-adjusted auth timeout') {
      for (const part of ['?', '#']) for (const name of ['code', 'error', 'id_token', 'access_token']) {
        for (const state of ['', '&state=first', '&state=wrong']) {
          const pending = openAuthSession('https://login.example.com/logout', {
            callback: authCallback(), state: 'first', logout: true,
          }).catch(error => error);
          const popup = popups.at(-1);
          const data = { type: 'exact-auth', url: `${authCallback()}${part}${name}=answer${state}`,
            identity: popup.sessionStorage.getItem('exact.auth.return') };
          if (mode === 'a logout opener response') listener({ origin: location.origin, source: popup, data });
          else channel.onmessage({ data });
          await expect(pending).resolves.toMatchObject({ status: 502, message: 'the logout return carries a sign-in response' });
          expect(timers.filter(timer => timer.active)).toEqual([]);
        }
      }
      return;
    }

    const first = openAuthSession('https://login.example.com/authorize', {
      callback: authCallback(), state: 'first',
    }).catch(error => error);
    expect(popups).toHaveLength(1);
    wall -= 60 * 60 * 1000;
    run(10 * 60 * 1000);
    await expect(first).resolves.toMatchObject({ status: 499 });

    target = 'second';
    const callback = `${authCallback()}?code=done&state=second`;
    const second = openAuthSession('https://login.example.com/authorize', {
      callback: authCallback(), state: 'second',
    });
    listener({ origin: location.origin, source: popups.at(-1), data: { type: 'exact-auth', url: callback } });
    expect(await second).toBe(callback);
  } finally {
    clock.agent = true;
    Date.now = savedNow;
    for (const [name, descriptor] of saved) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  }
});

test("fragment callbacks retain their parameters and require one exact state across query and fragment", async () => {
  globalThis.location = new URL("https://app.example/");
  clock.agent = true;
  install("auth.session https://login.example.com\nauth.callback https://app.example/cb", () => "login");
  const pending = openAuthSession("https://login.example.com/authorize", { callback: "https://app.example/cb", state: "s +😀" });
  const [hold] = holds();
  try {
    for (const suffix of ["#id_token=token", "#state=wrong", "?state=wrong#state=s+%2B%F0%9F%98%80", "?state=s+%2B%F0%9F%98%80#state=wrong", "#state=s+%2B%F0%9F%98%80&state=wrong"]) {
      expect(answer({ ticket: hold.ticket, op: "type", text: "https://app.example/cb" + suffix }).error).toBeDefined();
    }
    const callback = "https://app.example/cb?tracking=1#state=s+%2B%F0%9F%98%80&id_token=token&code=hybrid";
    expect(answer({ ticket: hold.ticket, op: "type", text: callback }).error).toBeUndefined();
    expect(await pending).toBe(callback);
  } finally {
    answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
    await pending.catch(() => {});
  }
});

test("authFixture shows the full URL for both http and https loopback origins, never non-loopback", async () => {
  clock.agent = true;
  for (const [origin, shown] of [
    ["http://127.0.0.1:4100", true],
    ["https://127.0.0.1:4100", true],
    ["https://localhost:4100", true],
    ["https://[::1]:4100", true],
    ["https://evil.example:4100", false],
  ]) {
    globalThis.location = new URL(`https://app.example/?authFixture=${encodeURIComponent(origin)}`);
    install(`auth.session ${origin}\nauth.callback https://app.example/cb`, () => "login");
    const url = `${origin}/authorize?code_challenge=secret&state=s`;
    const pending = openAuthSession(url, { callback: "https://app.example/cb", state: "s" });
    const [hold] = holds();
    try {
      expect(hold.device.args.url).toBe(shown ? url : undefined);
    } finally {
      answer({ ticket: hold.ticket, op: "tap", choice: "cancel" });
      await pending.catch(() => {});
    }
  }
});

test("non-app origins are refused 403 without holding or opening, and tool calls report person-required", async () => {
  globalThis.location = new URL("https://app.example/");
  clock.agent = true;
  let active = null;
  install(
    "auth.session https://login.example.com\nauth.callback https://app.example/.exact/auth/callback",
    () => "login",
    () => active,
  );
  try {
    for (const origin of ["device", "wake", "tool", "action"]) {
      currentOrigin = origin;
      Invocation.id = origin === "tool" ? 7 : 0;
      personRequired.length = 0;
      const err = await openAuthSession("https://login.example.com/authorize", {
        callback: authCallback(),
        state: "s",
      }).catch(e => e);
      expect(err.status).toBe(403);
      expect(holds()).toHaveLength(0);
      expect(personRequired).toEqual(origin === "tool" ? [7] : []);
    }
    // Async continuation after await: Origin.now() returned to "app", active call stays "tool".
    currentOrigin = "app";
    Invocation.id = 0;
    personRequired.length = 0;
    active = { origin: "tool", invocation: 11 };
    const asyncErr = await openAuthSession("https://login.example.com/authorize", {
      callback: authCallback(),
      state: "s",
    }).catch(e => e);
    expect(asyncErr.status).toBe(403);
    expect(holds()).toHaveLength(0);
    expect(personRequired).toEqual([11]);
  } finally {
    currentOrigin = "app";
    Invocation.id = 0;
    active = null;
  }
});
