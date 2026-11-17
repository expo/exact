// auth-glue.js across same-origin tabs. Browser messages and timers run only
// when the test advances a later-task scheduler.
import { afterAll, afterEach, expect, test } from 'bun:test';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { engineTests } from '../../scripts/coverage.mjs';

const engineTest = await engineTests(['chromium', 'webkit']);

const saved = [
  'BroadcastChannel', 'window', 'location', 'navigator', 'localStorage',
  'performance', 'addEventListener', 'removeEventListener', 'setTimeout',
  'clearTimeout', 'setInterval', 'clearInterval',
].map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]);
const savedNow = Date.now;
afterAll(() => {
  Date.now = savedNow;
  for (const [key, descriptor] of saved) {
    if (descriptor) Object.defineProperty(globalThis, key, descriptor);
    else delete globalThis[key];
  }
});

let registeringTab = null;
class Scheduler {
  tasks = [];
  gated = new Set();
  later(owner, kind, label, run) {
    const task = { owner, kind, label, run, cancelled: false };
    this.tasks.push(task);
    return task;
  }
  async next({ owner, kind, label } = {}) {
    const index = this.tasks.findIndex(task => !task.cancelled && !this.gated.has(task.owner)
      && (owner === undefined || task.owner === owner)
      && (kind === undefined || task.kind === kind)
      && (label === undefined || task.label === label));
    if (index < 0) return false;
    const [task] = this.tasks.splice(index, 1);
    const prior = registeringTab;
    registeringTab = task.owner;
    try { task.run(); }
    finally { registeringTab = prior; }
    await Promise.resolve();
    await Promise.resolve();
    return true;
  }
  async drain(filter = {}) {
    let count = 0;
    while (await this.next(filter)) {
      if (++count > 1000) throw new Error('later-task scheduler did not settle');
    }
  }
  reset() {
    this.tasks = [];
    this.gated.clear();
  }
}
const scheduler = new Scheduler();

let timerNumber = 0;
const timers = new Map();
function queueTimer(timer) {
  timer.task = scheduler.later(timer.owner, 'timer', `${timer.repeat ? 'interval' : 'timeout'}:${timer.delay}`, () => {
    if (!timer.active) return;
    if (!timer.repeat) {
      timer.active = false;
      timers.delete(timer.id);
    }
    timer.callback();
    if (timer.repeat && timer.active) queueTimer(timer);
  });
}
function startTimer(callback, delay, repeat) {
  const timer = { id: ++timerNumber, owner: registeringTab, callback, delay, repeat, active: true, task: null };
  timers.set(timer.id, timer);
  queueTimer(timer);
  return timer.id;
}
function clearTimer(id) {
  const timer = timers.get(id);
  if (!timer) return;
  timer.active = false;
  timer.task.cancelled = true;
  timers.delete(id);
}
globalThis.setTimeout = (callback, delay = 0) => startTimer(callback, Number(delay), false);
globalThis.clearTimeout = clearTimer;
globalThis.setInterval = (callback, delay = 0) => startTimer(callback, Number(delay), true);
globalThis.clearInterval = clearTimer;

// One origin's channels. A posted message is queued for every other channel,
// but a recipient closed before its task runs receives nothing.
const channels = new Set();
globalThis.BroadcastChannel = class {
  constructor(name) { this.name = name; this.owner = registeringTab; this.open = true; channels.add(this); }
  postMessage(data) {
    for (const channel of channels) if (channel !== this && channel.name === this.name) {
      scheduler.later(channel.owner, 'channel', String(data.identity ?? ''), () => {
        if (channel.open) channel.onmessage?.({ data: structuredClone(data) });
      });
    }
  }
  close() { this.open = false; channels.delete(this); }
};

const popups = new Map();
const popupFor = (owner, url) => {
  let href = String(url);
  const values = new Map();
  const popup = {
    owner, closed: false, events: [['open', href]], storageThrows: false, stored: values,
    location: {
      get href() { return href; },
      set href(value) { href = String(value); popup.events.push(['navigate', href]); },
    },
    sessionStorage: {
      getItem(key) { if (popup.storageThrows) throw new Error('storage unavailable'); return values.get(key) ?? null; },
      setItem(key, value) {
        if (popup.storageThrows) throw new Error('storage unavailable');
        values.set(key, String(value)); popup.events.push(['store', key, String(value)]);
      },
      removeItem(key) { if (popup.storageThrows) throw new Error('storage unavailable'); values.delete(key); },
    },
    close() { this.closed = true; },
  };
  return popup;
};
globalThis.window = {
  open(url) {
    const popup = popupFor(registeringTab, url);
    popups.set(registeringTab, popup);
    return popup;
  },
};
globalThis.location = { origin: 'https://app.example', search: '' };
let forbiddenCoordinationReads = 0;
const navigatorDouble = {};
Object.defineProperty(navigatorDouble, 'locks', { get() { forbiddenCoordinationReads++; throw new Error('locks forbidden'); } });
globalThis.navigator = navigatorDouble;
Object.defineProperty(globalThis, 'localStorage', {
  configurable: true,
  get() { forbiddenCoordinationReads++; throw new Error('shared storage forbidden'); },
});
let now = 10_000, mono = 0;
Date.now = () => now;
globalThis.performance = { timeOrigin: 1000, now: () => mono };
const messageListeners = new Map();
globalThis.addEventListener = (type, listener) => {
  if (type === 'message') messageListeners.set(registeringTab, listener);
};
globalThis.removeEventListener = (type, listener) => {
  if (type !== 'message') return;
  for (const [name, registered] of messageListeners) if (registered === listener) messageListeners.delete(name);
};

const CALLBACK = 'https://app.example/.exact/auth/callback';
const BOUND_MS = 10 * 60 * 1000;
const tabs = new Map();
let moduleNumber = 0;
async function makeTab(name) {
  const auth = await import(`./auth-glue.js?tab=${encodeURIComponent(name)}-${++moduleNumber}`);
  const done = [], delivered = [], active = new Set();
  const result = {
    name, done, delivered, active, current: null,
    forget(ticket) { active.delete(ticket); },
    prepare() {
      const prior = registeringTab;
      registeringTab = name;
      try { return auth.prepare(); }
      finally { registeringTab = prior; }
    },
    async arm(ticket, state, logout = false, callback = CALLBACK, prepared = undefined) {
      active.add(ticket);
      this.current = { ticket, state, logout, callback };
      registeringTab = name;
      auth.arm({ ticket }, {
        agent: false,
        log() {},
        active: candidate => active.has(candidate),
        deliver(candidate) { delivered.push(candidate); },
        call: request => {
          if (request.op === 'done') done.push(request);
          return request.op === 'arm' ? { present: {
            url: `https://login.example/authorize?state=${state}`,
            callback, state, logout, ...(prepared === undefined ? {} : { prepared }),
          } } : {};
        },
      });
      registeringTab = null;
    },
  };
  tabs.set(name, result);
  return result;
}
async function tab(name, ticket, state, logout = false, callback = CALLBACK) {
  const result = await makeTab(name);
  await result.arm(ticket, state, logout, callback);
  return result;
}

const popupIdentity = name => popups.get(name)?.stored.get('exact.auth.return') ?? null;

function post(url, extra = {}) {
  const channel = new BroadcastChannel('exact-auth');
  channel.postMessage({ type: 'exact-auth', url, ...extra });
  channel.close();
}
function opener(name, url, extra = {}) {
  const source = popups.get(name);
  scheduler.later(name, 'opener', String(extra.identity ?? ''), () => {
    messageListeners.get(name)?.({
      origin: location.origin,
      source,
      data: { type: 'exact-auth', url, ...extra },
    });
  });
}
function discard(name) {
  messageListeners.delete(name);
  popups.get(name)?.close();
  popups.delete(name);
  for (const channel of [...channels]) if (channel.owner === name) channel.close();
}

afterEach(async () => {
  scheduler.gated.clear();
  // End a still-live session through its ordinary opener path so no timer or
  // listener crosses into the next test.
  for (const [name, value] of tabs) if (messageListeners.has(name) && value.current) {
    const { state, callback } = value.current;
    const endpoint = new URL(callback);
    opener(name, `${endpoint.origin}${endpoint.pathname}?state=${encodeURIComponent(state)}`);
  }
  await scheduler.drain({ kind: 'opener' });
  for (const name of tabs.keys()) discard(name);
  tabs.clear();
  scheduler.reset();
  for (const timer of timers.values()) timer.active = false;
  timers.clear();
  for (const channel of [...channels]) channel.close();
  messageListeners.clear();
  popups.clear();
  globalThis.exact = {};
  now = 10_000;
  mono = 0;
  forbiddenCoordinationReads = 0;
});

test("the test's browser delivers every message and timer in a later task", async () => {
  const seen = [];
  registeringTab = 'double-a';
  const sender = new BroadcastChannel('proof');
  registeringTab = 'double-b';
  const receiver = new BroadcastChannel('proof');
  receiver.onmessage = () => seen.push('channel');
  messageListeners.set('double-b', () => seen.push('opener'));
  setTimeout(() => seen.push('timer'), 1);
  registeringTab = 'double-a';
  sender.postMessage({});
  scheduler.later('double-b', 'opener', 'proof', () => messageListeners.get('double-b')?.({}));
  registeringTab = null;
  await Promise.resolve();
  expect(seen).toEqual([]);
  await scheduler.next({ kind: 'channel' });
  await scheduler.next({ kind: 'opener' });
  await scheduler.next({ kind: 'timer' });
  expect(seen).toEqual(['channel', 'opener', 'timer']);
  sender.close();
  receiver.close();
  discard('double-a');
  discard('double-b');
});

test("the wasm auth arm forwards the agent drive's fixture", async () => {
  location.search = '?agent=1&authFixture=http%3A%2F%2F127.0.0.1%3A4400';
  try {
    const { arm } = await import('./auth-glue.js?agent-fixture');
    let request;
    arm({ ticket: 21 }, {
      agent: true, log() {}, active: () => true, deliver() {},
      call: value => { request = value; return { held: true }; },
    });
    expect(request).toEqual({ op: 'arm', ticket: 21, agent: true, origin: location.origin,
      popup: true, fixture: 'http://127.0.0.1:4400' });
  } finally { location.search = ''; }
});

test('a broadcast callback ends only the sign-in whose state it carries, in either order', async () => {
  const a = await tab('state-a', 1, 'a'), b = await tab('state-b', 2, 'b');
  for (const url of [`${CALLBACK}?code=c`, `${CALLBACK}?code=c&state=a&state=b`, `${CALLBACK}?state=other`, 'not a url']) {
    post(url);
    await scheduler.drain({ kind: 'channel' });
  }
  expect([a.done, b.done]).toEqual([[], []]);
  post(`${CALLBACK}?code=b&state=b`);
  await scheduler.drain({ kind: 'channel' });
  expect(b.done).toEqual([{ op: 'done', ticket: 2, url: `${CALLBACK}?code=b&state=b` }]);
  post(`${CALLBACK}?code=a&state=a`);
  await scheduler.drain({ kind: 'channel' });
  expect(a.done).toEqual([{ op: 'done', ticket: 1, url: `${CALLBACK}?code=a&state=a` }]);
});

test('a severed state-less return completes no logout and uses no cross-tab coordination', async () => {
  const a = await tab('severed-a', 3, 'a', true), b = await tab('severed-b', 4, 'b', true);
  post(CALLBACK);
  await scheduler.drain({ kind: 'channel' });
  expect([a.done, b.done]).toEqual([[], []]);
  expect(forbiddenCoordinationReads).toBe(0);
  await scheduler.next({ owner: 'severed-a', kind: 'timer', label: `timeout:${BOUND_MS}` });
  expect(a.done).toEqual([{ op: 'done', ticket: 3, status: 499, message: 'cancelled' }]);
});

test('a logout seeds its blank popup with a fresh identity before navigating it', async () => {
  const directA = await tab('seed-direct-a', 22, 'a', true);
  const directB = await tab('seed-direct-b', 23, 'b', true);
  const prepared = await makeTab('seed-prepared');
  const preparedId = prepared.prepare();
  await prepared.arm(24, 'prepared', true, CALLBACK, preparedId);
  const signIn = await tab('seed-sign-in', 25, 'sign-in');
  const identities = ['seed-direct-a', 'seed-direct-b', 'seed-prepared'].map(popupIdentity);
  for (const identity of identities) expect(identity).toMatch(/^[0-9a-f]{32}$/);
  expect(new Set(identities).size).toBe(3);
  for (const name of ['seed-direct-a', 'seed-direct-b', 'seed-prepared']) {
    const popup = popups.get(name);
    expect(popup.events[0]).toEqual(['open', 'about:blank']);
    expect(popup.events.slice(-2)).toEqual([
      ['store', 'exact.auth.return', popupIdentity(name)],
      ['navigate', expect.stringContaining('https://login.example/authorize')],
    ]);
  }
  expect(popups.get('seed-sign-in').events).toEqual([
    ['open', 'https://login.example/authorize?state=sign-in'],
  ]);
  expect(popupIdentity('seed-sign-in')).toBe(null);
  expect([directA.done, directB.done, prepared.done, signIn.done]).toEqual([[], [], [], []]);
});

test("an addressed state-less return completes its own logout and no other tab's", async () => {
  for (const reverse of [false, true]) {
    const suffix = reverse ? 'reverse' : 'forward';
    const a = await tab(`addressed-a-${suffix}`, reverse ? 27 : 26, `a-${suffix}`, true);
    const b = await tab(`addressed-b-${suffix}`, reverse ? 29 : 28, `b-${suffix}`, true);
    for (const value of reverse ? [a, b] : [b, a]) {
      post(CALLBACK, { identity: popupIdentity(value.name) });
      await scheduler.drain({ kind: 'channel' });
      expect(value.done).toEqual([{ op: 'done', ticket: value.current.ticket, url: CALLBACK }]);
    }
    expect(a.done).toHaveLength(1);
    expect(b.done).toHaveLength(1);
    discard(a.name);
    discard(b.name);
  }
});

test('a stalled tab neither delays nor takes another tab\'s addressed return', async () => {
  const stalled = await tab('addressed-stalled', 30, 'stalled', true);
  const running = await tab('addressed-running', 31, 'running', true);
  scheduler.gated.add('addressed-stalled');
  post(CALLBACK, { identity: popupIdentity('addressed-running') });
  await scheduler.next({ owner: 'addressed-running', kind: 'channel' });
  expect(running.done).toEqual([{ op: 'done', ticket: 31, url: CALLBACK }]);
  expect(stalled.done).toEqual([]);
  scheduler.gated.delete('addressed-stalled');
  await scheduler.next({ owner: 'addressed-stalled', kind: 'channel' });
  expect(stalled.done).toEqual([]);
  post(CALLBACK, { identity: popupIdentity('addressed-stalled') });
  await scheduler.next({ owner: 'addressed-stalled', kind: 'channel' });
  expect(stalled.done).toEqual([{ op: 'done', ticket: 30, url: CALLBACK }]);
});

test('a state-less return with no identity, or one no live logout holds, completes nothing', async () => {
  const waiting = await tab('unaddressed', 32, 'waiting', true);
  for (const identity of [undefined, '', '0'.repeat(32)]) {
    post(CALLBACK, identity === undefined ? {} : { identity });
    await scheduler.drain({ kind: 'channel' });
  }
  expect(waiting.done).toEqual([]);

  const replaced = await tab('replaced-address', 33, 'old', true);
  const retired = popupIdentity('replaced-address');
  await replaced.arm(34, 'new', true);
  post(CALLBACK, { identity: retired });
  await scheduler.drain({ owner: 'replaced-address', kind: 'channel' });
  expect(replaced.done).toEqual([]);

  const forgotten = await tab('forgotten-address', 35, 'forgotten', true);
  const forgottenIdentity = popupIdentity('forgotten-address');
  forgotten.forget(35);
  post(CALLBACK, { identity: forgottenIdentity });
  await scheduler.drain({ owner: 'forgotten-address', kind: 'channel' });
  expect(forgotten.done).toEqual([]);
  expect(forgotten.delivered).toEqual([]);
});

test('a state-less return never completes a sign-in or a logout when it carries code or error', async () => {
  const signIn = await tab('identity-sign-in', 36, 'sign-in');
  const logout = await tab('identity-logout', 37, 'logout', true);
  const identity = popupIdentity('identity-logout');
  post(CALLBACK, { identity });
  await scheduler.next({ owner: 'identity-sign-in', kind: 'channel' });
  expect(signIn.done).toEqual([]);
  await scheduler.next({ owner: 'identity-logout', kind: 'channel' });
  expect(logout.done).toHaveLength(1);

  for (const [name, parameter, ticket] of [['identity-code', 'code=c', 38], ['identity-error', 'error=denied', 39]]) {
    const value = await tab(name, ticket, name, true);
    post(`${CALLBACK}?${parameter}`, { identity: popupIdentity(name) });
    await scheduler.drain({ owner: name, kind: 'channel' });
    expect(value.done).toEqual([]);
  }
});

test('a logout whose popup storage is unavailable still opens and still takes its opener or matching state', async () => {
  for (const [via, ticket] of [['opener', 40], ['state', 41]]) {
    const name = `storage-unavailable-${via}`;
    const value = await makeTab(name);
    const prepared = value.prepare();
    popups.get(name).storageThrows = true;
    await value.arm(ticket, via, true, CALLBACK, prepared);
    expect(popups.get(name).location.href).toContain('https://login.example/authorize');
    expect(popupIdentity(name)).toBe(null);
    if (via === 'opener') opener(name, CALLBACK);
    else post(`${CALLBACK}?state=${via}`);
    await scheduler.drain({ owner: name, kind: via === 'opener' ? 'opener' : 'channel' });
    expect(value.done).toHaveLength(1);
  }
});

test("a return its opener delivered completes that popup's logout and no other", async () => {
  const first = await tab('opener-first', 5, 'first', true);
  const second = await tab('opener-second', 6, 'second', true);
  opener('opener-second', CALLBACK);
  post(CALLBACK);
  await scheduler.next({ owner: 'opener-second', kind: 'opener' });
  await scheduler.drain({ kind: 'channel' });
  expect(first.done).toEqual([]);
  expect(second.done).toEqual([{ op: 'done', ticket: 6, url: CALLBACK }]);
});

test('a return handled past the bound, before the bound timer, answers 499 once', async () => {
  const cases = [
    ['late-opener', true, 'logout', () => opener('late-opener', CALLBACK)],
    ['late-logout-state', true, 'logout-state', () => post(`${CALLBACK}?state=logout-state`)],
    ['late-sign-in', false, 'sign-in-state', () => post(`${CALLBACK}?code=late&state=sign-in-state`)],
    ['late-addressed', true, 'addressed', () => post(CALLBACK, { identity: popupIdentity('late-addressed') })],
  ];
  for (let index = 0; index < cases.length; index++) {
    const [name, logout, state, send] = cases[index];
    const value = await tab(name, 7 + index, state, logout);
    mono += BOUND_MS;
    send();
    await scheduler.drain({ owner: name, kind: name === 'late-opener' ? 'opener' : 'channel' });
    expect(value.done).toEqual([{ op: 'done', ticket: 7 + index, status: 499, message: 'cancelled' }]);
    expect(value.delivered).toEqual([7 + index]);
    expect(await scheduler.next({ owner: name, kind: 'timer', label: `timeout:${BOUND_MS}` })).toBe(false);
    mono = 0;
    discard(name);
  }
});

test('a wall clock set back during a wait does not stretch its bound', async () => {
  for (const [name, logout, ticket] of [['clock-back-logout', true, 42], ['clock-back-sign-in', false, 43]]) {
    const value = await tab(name, ticket, name, logout);
    now -= 60 * 60 * 1000;
    await scheduler.next({ owner: name, kind: 'timer', label: 'interval:250' });
    expect(value.done).toEqual([]);
    await scheduler.next({ owner: name, kind: 'timer', label: `timeout:${BOUND_MS}` });
    expect(value.done).toEqual([{ op: 'done', ticket, status: 499, message: 'cancelled' }]);
    expect(value.delivered).toEqual([ticket]);
    expect(await scheduler.next({ owner: name, kind: 'timer', label: 'interval:250' })).toBe(false);
    if (logout) post(CALLBACK, { identity: popupIdentity(name) });
    else post(`${CALLBACK}?code=late&state=${name}`);
    await scheduler.drain({ owner: name, kind: 'channel' });
    expect(value.done).toHaveLength(1);
    expect(value.delivered).toEqual([ticket]);
    now = 10_000;
    discard(name);
  }
});

test('a wall clock set forward during a wait ends nothing early', async () => {
  const returningLogout = await tab('clock-forward-logout-return', 44, 'logout-return', true);
  const returningSignIn = await tab('clock-forward-sign-in-return', 45, 'sign-in-return');
  const boundedLogout = await tab('clock-forward-logout-bound', 46, 'logout-bound', true);
  const boundedSignIn = await tab('clock-forward-sign-in-bound', 47, 'sign-in-bound');
  const values = [returningLogout, returningSignIn, boundedLogout, boundedSignIn];
  now += 60 * 60 * 1000;
  for (let count = 0; count < 3; count++) {
    for (const value of values) {
      await scheduler.next({ owner: value.name, kind: 'timer', label: 'interval:250' });
      expect(value.done).toEqual([]);
    }
  }
  post(CALLBACK, { identity: popupIdentity(returningLogout.name) });
  post(`${CALLBACK}?code=done&state=sign-in-return`);
  await scheduler.drain({ owner: returningLogout.name, kind: 'channel' });
  await scheduler.drain({ owner: returningSignIn.name, kind: 'channel' });
  expect(returningLogout.done).toEqual([{ op: 'done', ticket: 44, url: CALLBACK }]);
  expect(returningSignIn.done).toEqual([{ op: 'done', ticket: 45, url: `${CALLBACK}?code=done&state=sign-in-return` }]);
  expect([boundedLogout.done, boundedSignIn.done]).toEqual([[], []]);
  mono += BOUND_MS;
  for (const [value, ticket] of [[boundedLogout, 46], [boundedSignIn, 47]]) {
    await scheduler.next({ owner: value.name, kind: 'timer', label: `timeout:${BOUND_MS}` });
    expect(value.done).toEqual([{ op: 'done', ticket, status: 499, message: 'cancelled' }]);
    expect(value.delivered).toEqual([ticket]);
  }
});

test('a return for a request the runner let go reports nothing and closes its popup', async () => {
  const value = await tab('forgotten', 10, 'forgotten', true);
  value.forget(10);
  opener('forgotten', CALLBACK);
  await scheduler.next({ owner: 'forgotten', kind: 'opener' });
  expect(value.done).toEqual([]);
  expect(value.delivered).toEqual([]);
  expect(popups.get('forgotten').closed).toBe(true);
});

test('a tab at another callback endpoint takes no return', async () => {
  const a = await tab('endpoint-a', 13, 'a', true, 'https://app.example/callback-a');
  const b = await tab('endpoint-b', 14, 'b', true, 'https://app.example/callback-b');
  post('https://app.example/callback-b', { identity: popupIdentity('endpoint-b') });
  await scheduler.drain({ kind: 'channel' });
  expect(a.done).toEqual([]);
  expect(b.done).toEqual([{ op: 'done', ticket: 14, url: 'https://app.example/callback-b' }]);
});

test("a sign-in response completes no logout with the same state", async () => {
  const signIn = await tab('same-state-sign-in', 15, 'shared');
  const logout = await tab('same-state-logout', 16, 'shared', true);
  post(`${CALLBACK}?error=cancelled`);
  await scheduler.drain({ kind: 'channel' });
  expect([signIn.done, logout.done]).toEqual([[], []]);
  post(`${CALLBACK}?code=sign-in-code&state=shared`);
  await scheduler.drain({ kind: 'channel' });
  expect(signIn.done).toEqual([{ op: 'done', ticket: 15, url: `${CALLBACK}?code=sign-in-code&state=shared` }]);
  expect(logout.done).toEqual([]);
  post(`${CALLBACK}?state=shared`);
  await scheduler.drain({ kind: 'channel' });
  expect(logout.done).toEqual([{ op: 'done', ticket: 16, url: `${CALLBACK}?state=shared` }]);
});

test("a prepared window preserves the press's activation until the auth request knows its URL", async () => {
  const priorNavigator = navigator, priorOpen = window.open;
  const activation = { isActive: true }, opened = [];
  globalThis.navigator = { userActivation: activation };
  window.open = url => {
    const popup = popupFor(registeringTab, url);
    opened.push(popup);
    popups.set(registeringTab, popup);
    return popup;
  };
  try {
    const { arm, prepare } = await import('./auth-glue.js?prepared-window');
    registeringTab = 'prepared-window';
    const prepared = prepare();
    registeringTab = null;
    expect(typeof prepared).toBe('string');
    expect(opened.map(popup => popup.location.href)).toEqual(['about:blank']);
    activation.isActive = false;
    const unrelated = [];
    arm({ ticket: 18 }, {
      agent: false, log() {}, active: () => true, deliver() {},
      call: request => {
        if (request.op === 'done') unrelated.push(request);
        return request.op === 'arm' ? { present: {
          url: 'https://login.example/unrelated', callback: CALLBACK, state: 'other', logout: false,
        } } : {};
      },
    });
    expect(unrelated).toEqual([{ op: 'done', ticket: 18, status: 428, message: 'popup blocked' }]);
    const done = [];
    registeringTab = 'prepared-window';
    arm({ ticket: 17 }, {
      agent: false, log() {}, active: () => true, deliver() {},
      call: request => {
        if (request.op === 'done') done.push(request);
        return request.op === 'arm' ? { present: {
          url: 'https://login.example/logout?id_token_hint=leased', callback: CALLBACK,
          state: 'logout', logout: true, prepared,
        } } : {};
      },
    });
    registeringTab = null;
    popups.set('prepared-window', opened[0]);
    expect(opened).toHaveLength(1);
    expect(opened[0].location.href).toBe('https://login.example/logout?id_token_hint=leased');
    opener('prepared-window', `${CALLBACK}?state=logout`);
    await scheduler.next({ owner: 'prepared-window', kind: 'opener' });
    expect(done).toEqual([{ op: 'done', ticket: 17, url: `${CALLBACK}?state=logout` }]);
  } finally {
    registeringTab = null;
    globalThis.navigator = priorNavigator;
    window.open = priorOpen;
    discard('prepared-window');
  }
});

test("a logout without a prepared id never adopts the sign-in button's window", async () => {
  const priorNavigator = navigator, priorOpen = window.open;
  const signIn = popupFor('waiting-sign-in', 'about:blank');
  globalThis.navigator = { userActivation: { isActive: false } };
  globalThis.exact = { authWaiting: { window: signIn, claimed: false } };
  window.open = () => null;
  const done = [];
  try {
    const { arm } = await import('./auth-glue.js?idless-logout');
    arm({ ticket: 19 }, {
      agent: false, log() {}, active: () => true, deliver() {},
      call: request => {
        if (request.op === 'done') done.push(request);
        return request.op === 'arm' ? { present: {
          url: 'https://login.example/logout?id_token_hint=alice', callback: CALLBACK,
          state: 'logout', logout: true,
        } } : {};
      },
    });
    expect(signIn.location.href).toBe('about:blank');
    expect(signIn.closed).toBe(false);
    expect(done).toEqual([{ op: 'done', ticket: 19, status: 428, message: 'popup blocked' }]);
  } finally {
    globalThis.navigator = priorNavigator;
    window.open = priorOpen;
    globalThis.exact.authWaiting = null;
  }
});

// Real browsing-context separation, with the app's clock controlled independently of the provider.
for (const engine of ['chromium', 'webkit']) engineTest[engine](`${engine}: auth waits through COOP, closure and focus return until a callback, supersession or the bound`, async () => {
  const html = body => new Response(body, { headers: { 'content-type': 'text/html' } });
  const provider = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const url = new URL(request.url), callback = url.searchParams.get('callback');
    if (url.searchParams.has('quick')) return Response.redirect(callback);
    const response = html(`<button id="approve">Approve</button><script>
      document.querySelector('button').onclick = () => location.href = ${JSON.stringify(callback)};
    </script>`);
    if (url.searchParams.has('coop')) response.headers.set('Cross-Origin-Opener-Policy', 'same-origin');
    return response;
  } });
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/auth-glue.js' || path === '/auth-callback.js')
      return new Response(Bun.file(new URL(`.${path}`, import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    if (path === '/.exact/auth/callback') return html('<script src="/auth-callback.js"></script>');
    return html(`<button id="sign-in">Sign in</button><script type="module">
      import { arm } from '/auth-glue.js';
      window.reports = []; window.deliveries = []; window.logs = [];
      window.ticket = 0; window.mode = 'coop';
      const open = window.open.bind(window);
      window.open = (...args) => window.popup = open(...args);
      document.querySelector('button').onclick = () => {
        const current = ++window.ticket;
        const callback = location.origin + '/.exact/auth/callback';
        const state = 'state-' + current;
        arm({ ticket: current }, {
          agent: false, active: t => t === window.ticket,
          deliver: t => deliveries.push(t), log: line => logs.push(line),
          call: req => {
            if (req.op === 'done') reports.push(req);
            return req.op === 'arm' ? { present: { callback, state,
              url: ${JSON.stringify(provider.url.href)} + '?' + mode + '&callback=' +
                encodeURIComponent(callback + '?state=' + state + '&code=fixture') } } : {};
          },
        });
      };
    </script>`);
  } });
  let browser;
  try {
    browser = await suiteBrowser(engine);
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.clock.install();
    await page.clock.pauseAt(new Date());
    const start = async mode => {
      await page.evaluate(mode => { window.mode = mode; }, mode);
      const opened = page.waitForEvent('popup');
      await page.locator('#sign-in').click();
      return opened;
    };
    const reports = () => page.evaluate(() => reports);
    const popup = await start('coop');
    await popup.locator('#approve').waitFor();
    expect(await popup.evaluate(() => window.opener === null)).toBe(true);
    expect(await page.evaluate(() => popup.closed)).toBe(true);
    expect(popup.isClosed()).toBe(false);
    await page.bringToFront();
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await page.clock.runFor(6000);
    expect(await reports()).toEqual([]);
    await popup.locator('#approve').click();
    await page.waitForFunction(() => reports.length === 1);
    expect(await reports()).toEqual([{ op: 'done', ticket: 1, url: `${server.url.origin}/.exact/auth/callback?state=state-1&code=fixture` }]);
    expect(await page.evaluate(() => logs)).toContain('auth 1: a callback arrived (channel)');

    // A redirect that immediately closes its window still delivers exactly once.
    await page.evaluate(() => { window.savedChannel = window.BroadcastChannel; window.BroadcastChannel = undefined; });
    await start('quick');
    await page.waitForFunction(() => reports.length === 2);
    expect((await reports())[1]).toEqual({ op: 'done', ticket: 2, url: `${server.url.origin}/.exact/auth/callback?state=state-2&code=fixture` });
    expect(await page.evaluate(() => logs)).toContain('auth 2: a callback arrived (opener)');
    await page.evaluate(() => { window.BroadcastChannel = window.savedChannel; });

    // Closing a real window is indistinguishable from COOP to its opener.
    const closed = await start('plain');
    await closed.locator('#approve').waitFor();
    expect(await page.evaluate(() => popup.closed)).toBe(false);
    await closed.close();
    expect(await page.evaluate(() => popup.closed)).toBe(true);
    await page.clock.runFor(6000);
    expect((await reports()).length).toBe(2);
    await page.clock.fastForward(600_000 - 6000 - 1);
    expect((await reports()).length).toBe(2);
    await page.clock.runFor(1);
    expect((await reports())[2]).toEqual({ op: 'done', ticket: 3, status: 499, message: 'cancelled' });

    const stale = await start('coop');
    await stale.locator('#approve').waitFor();
    const current = await start('plain');
    await current.locator('#approve').waitFor();
    await Promise.all([stale.waitForEvent('close'), stale.locator('#approve').click()]);
    await current.locator('#approve').click();
    await page.waitForFunction(() => reports.length === 4);
    expect((await reports())[3].ticket).toBe(5);

    // A forgotten request closes its reachable popup and never delivers a cancellation.
    const forgotten = await start('plain');
    await forgotten.locator('#approve').waitFor();
    await page.evaluate(() => { window.ticket = 0; });
    await Promise.all([forgotten.waitForEvent('close'), page.clock.runFor(250)]);
    await page.clock.fastForward(600_000);
    expect((await reports()).length).toBe(4);
    expect(await page.evaluate(() => deliveries)).toEqual([1, 2, 3, 5]);
  } finally {
    await browser?.close(); server.stop(true); provider.stop(true);
  }
}, 30_000);
