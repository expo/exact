// auth-glue.js across two same-origin tabs, each its own import of the module: the
// callback page's broadcast reaches every tab, and only the tab whose session's
// state it carries takes it.
import { afterAll, expect, test } from 'bun:test';

const saved = ['BroadcastChannel', 'window', 'location', 'navigator', 'addEventListener', 'removeEventListener']
  .map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]);
afterAll(() => {
  for (const [k, d] of saved) if (d) Object.defineProperty(globalThis, k, d); else delete globalThis[k];
});

// One origin's channels: a post reaches every other open channel of its name.
const open = new Set();
globalThis.BroadcastChannel = class {
  constructor(name) { this.name = name; open.add(this); }
  postMessage(data) { for (const c of open) if (c !== this && c.name === this.name) c.onmessage?.({ data }); }
  close() { open.delete(this); }
};
globalThis.window = { open: () => ({ closed: false, close() {} }) };
globalThis.location = { origin: 'https://app.example' };
globalThis.navigator = {};
globalThis.addEventListener = () => {};
globalThis.removeEventListener = () => {};

const CALLBACK = 'https://app.example/.exact/auth/callback';
/** A tab with one sign-in pending under `state`; `done` collects what it reports. */
async function tab(name, ticket, state) {
  const { arm } = await import(`./auth-glue.js?tab=${name}`);
  const done = [];
  arm({ ticket }, {
    agent: false, log() {}, active: () => true, deliver() {},
    call: req => {
      if (req.op === 'done') done.push(req);
      return req.op === 'arm' ? { present: { url: `https://login.example/authorize?state=${state}`, callback: CALLBACK, state } } : {};
    },
  });
  return done;
}
// The callback page's post (auth-callback.js).
const broadcast = url => {
  const channel = new BroadcastChannel('exact-auth');
  channel.postMessage({ type: 'exact-auth', url });
  channel.close();
};

test("a broadcast callback ends only the sign-in whose state it carries, in either order", async () => {
  const a = await tab('a', 1, 'state-a'), b = await tab('b', 2, 'state-b');
  for (const url of [`${CALLBACK}?code=c`, `${CALLBACK}?code=c&state=state-a&state=state-b`, `${CALLBACK}?state=other`, 'not a url']) broadcast(url);
  expect([a, b]).toEqual([[], []]);
  broadcast(`${CALLBACK}?code=b&state=state-b`);
  expect([a, b]).toEqual([[], [{ op: 'done', ticket: 2, url: `${CALLBACK}?code=b&state=state-b` }]]);
  broadcast(`${CALLBACK}?code=a&state=state-a`);
  expect(a).toEqual([{ op: 'done', ticket: 1, url: `${CALLBACK}?code=a&state=state-a` }]);
  expect(b.length).toBe(1);
  expect(open.size).toBe(0);
});
