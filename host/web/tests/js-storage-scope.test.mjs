// A development page's storage is its app's own (LLP 1116.004 D8): every app
// the dev server serves shares its origin, and bake-off contacts read an
// earlier round's contacts.json, written under the same default app id. A
// development build writes the app directory's digest into the host files
// that name a store (scripts/agent-launch.mjs `developmentStore`): its files,
// databases and kept keys (storage-environment.js `storageKey`), its persisted
// state (persist.js) and its durable store (rt.js `Store`); a release ships
// them as written, keyed by the id alone. rt.js runs beside stand-ins for the
// modules it imports, as js-link-press.test.mjs runs it.
import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { appDigest, developmentStore } from '../../../scripts/agent-launch.mjs';
import { standIns } from './stand-ins.mjs';
import { fileURLToPath } from 'node:url';

const webJs = fileURLToPath(new URL('../../web-js/', import.meta.url)), web = fileURLToPath(new URL('../', import.meta.url));
const REAL = ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'held.js', 'font-family.js', 'observers.js', 'grid.js', 'overlay.js', 'roster.js', 'router.js', 'route-pattern.js', 'schedule.js', 'budget.js', 'shape.js', 'persist.js'];
const root = mkdtempSync(resolve(tmpdir(), 'exact-js-storage-'));
/** The runtime as a build lays it out for the app at `appDir`, or as written (a release) when it is null. */
function runtime(name, appDir) {
  const dir = resolve(root, name), ship = code => appDir ? developmentStore(code, appDir) : code;
  mkdirSync(dir);
  for (const [file, source] of standIns(REAL)) writeFileSync(resolve(dir, file), source);
  for (const f of REAL) writeFileSync(resolve(dir, f), ship(readFileSync(resolve(webJs, f), 'utf8')));
  writeFileSync(resolve(dir, 'storage-environment.js'), ship(readFileSync(resolve(web, 'storage-environment.js'), 'utf8')));
  return dir;
}
// One origin's `localStorage`, which every app the dev server serves shares.
const kept = new Map();
globalThis.localStorage = {
  get length() { return kept.size; }, key: i => [...kept.keys()][i] ?? null,
  getItem: k => kept.has(k) ? kept.get(k) : null, setItem: (k, v) => { kept.set(k, String(v)); }, removeItem: k => { kept.delete(k); },
};
const one = '/rounds/r5/apps/contacts', two = '/rounds/r6/apps/contacts';

test('a development build keys its files, databases and kept keys by its app directory; a release by the id alone', async () => {
  const dev = await import(resolve(runtime('key-one', one), 'storage-environment.js'));
  const other = await import(resolve(runtime('key-two', two), 'storage-environment.js'));
  const release = await import(resolve(runtime('key-release', null), 'storage-environment.js'));
  const id = 'com.example.contacts-exact2', d = appDigest(one);
  expect(dev.storageKey(id, 'http://127.0.0.1:8765/')).toBe(`${id}@${d}`);
  expect(dev.storageKey(id, 'http://127.0.0.1:1/?agent=1&storage=s1')).toBe(`${id}@${d}/agent/s1`);
  expect(dev.storageKey(id, 'http://127.0.0.1:1/?agent=1')).toBeNull();
  // Two apps of one id, in two directories, open two stores; a release opens the id's.
  expect(other.storageKey(id, 'http://127.0.0.1:8765/')).not.toBe(dev.storageKey(id, 'http://127.0.0.1:8765/'));
  expect(release.storageKey(id, 'http://127.0.0.1:8765/')).toBe(id);
  expect(release.storageKey(id, 'http://127.0.0.1:1/?agent=1&storage=s1')).toBe(`${id}/agent/s1`);
});

test('persisted state and the durable store are the app\'s own on a shared origin', async () => {
  kept.clear();
  const apps = [];
  for (const [name, dir] of [['one', one], ['two', two], ['release', null]]) {
    const at = runtime(name, dir);
    const rt = await import(resolve(at, 'rt.js')), persist = await import(resolve(at, 'persist.js'));
    apps.push({ rt, persist, prefix: dir ? `@${appDigest(dir)}/` : '' });
  }
  const [a, b, release] = apps;
  expect(a.rt.Kept('state')).toBe(`exact.state.@${appDigest(one)}/`);
  expect(release.rt.Kept('state')).toBe('exact.state.');
  // The first app keeps a goal of 2500 and a token (bake-off water-tracker: an earlier app's goal read as its own).
  const signal = v => ({ n: { v } });
  expect(a.persist.stored('goal', 2000, 'n')).toBe(2000);
  a.persist.keep(signal(2500), 'goal', 'n');
  for (const f of a.rt.After) f();
  a.rt.Store.set('token', 'abc'); a.rt.Store.persist();
  expect(kept.get(`exact.state.${a.prefix}goal`)).toBe('2500');
  expect(kept.get(`exact.secret.${a.prefix}token`)).toBe('abc');
  // The second app, another directory on the same origin, reads neither; the first reads its own again.
  expect(b.persist.stored('goal', 2000, 'n')).toBe(2000);
  b.rt.Store.load();
  expect(b.rt.Store.get('token')).toBeUndefined();
  expect(a.persist.stored('goal', 2000, 'n')).toBe(2500);
  a.rt.Store.map.clear(); a.rt.Store.load();
  expect(a.rt.Store.get('token')).toBe('abc');
  // A release's keys are the runner's: "exact.state.<name>".
  kept.set('exact.state.goal', '1500');
  expect(release.persist.stored('goal', 2000, 'n')).toBe(1500);
});

// A write that failed after reading the value it replaces retries under the secret's lease
// (storage-environment.js `retrySecretWrite`), and only while that value is still there; one that could not read it is
// said and dropped, since a retry could overwrite another realm's later write.
test('committed store writes retry without new writes, coalesce by name, and exclude rollback', async () => {
  const rt = await import(resolve(runtime('retry', null), 'rt.js'));
  const storage = globalThis.localStorage, document = globalThis.document, navigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  globalThis.document = { querySelector: () => null, getElementById: () => ({}) };
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { locks: { request: async (_name, work) => work() } } });
  kept.clear();
  let blocked = true, unreadable = false;
  const refuse = () => { if (blocked) throw new Error('quota exceeded'); };
  globalThis.localStorage = {
    get length() { return storage.length; }, key: i => storage.key(i),
    getItem: k => { if (unreadable) throw new Error('disabled storage'); return storage.getItem(k); },
    setItem: (k, v) => { refuse(); storage.setItem(k, v); }, removeItem: k => { refuse(); storage.removeItem(k); },
  };
  const warnings = () => rt.journal.filter(l => l.includes('store token failed:'));
  // A commit persists; a retry it starts lands later, which awaiting `persist` again waits for.
  const commit = async f => { expect(rt.commit(f ?? (() => {}))).toBe(true); await rt.Store.persist(); };
  try {
    await commit(() => rt.Store.set('token', 'first')); await commit(); await commit();
    const firstWarnings = warnings();
    blocked = false; await commit();
    expect(kept.get('exact.secret.token')).toBe('first');
    expect(firstWarnings).toHaveLength(1);
    blocked = true; await commit(() => rt.Store.set('token', 'second')); await commit();
    expect(warnings()).toHaveLength(2);
    expect(rt.commit(() => { rt.Store.set('token', 'rollback'); rt.Store.set('uncommitted', 'never'); throw new rt.Refusal('test'); })).toBe(false);
    blocked = false; await commit();
    expect(kept.get('exact.secret.token')).toBe('second');
    expect(kept.has('exact.secret.uncommitted')).toBe(false);
    blocked = true; await commit(() => rt.Store.set('token', 'obsolete'));
    await commit(() => rt.Store.set('token', 'latest')); await commit();
    expect(warnings()).toHaveLength(4);
    blocked = false; await commit();
    expect(kept.get('exact.secret.token')).toBe('latest');
    blocked = true; await commit(() => rt.Store.set('token', 'obsolete'));
    await commit(() => rt.Store.set('token', null)); await commit();
    expect(warnings()).toHaveLength(6);
    blocked = false; await commit();
    expect(kept.has('exact.secret.token')).toBe(false);
    blocked = true; await commit(() => rt.Store.set('token', null));
    await commit(() => rt.Store.set('token', 'latest'));
    blocked = false; await commit();
    expect(kept.get('exact.secret.token')).toBe('latest');
    // Another realm's write while this one's retry waits keeps that realm's value.
    blocked = true; await commit(() => rt.Store.set('token', 'stale'));
    kept.set('exact.secret.token', 'another realm');
    blocked = false; await commit();
    expect(kept.get('exact.secret.token')).toBe('another realm');
    unreadable = true; await commit(() => rt.Store.set('token', 'unread'));
    unreadable = false; await commit();
    expect([kept.get('exact.secret.token'), warnings().length]).toEqual(['another realm', 10]);
  } finally {
    globalThis.localStorage = storage;
    if (navigator) Object.defineProperty(globalThis, 'navigator', navigator); else delete globalThis.navigator;
    if (document === undefined) delete globalThis.document; else globalThis.document = document;
  }
});

test('persisted state says each failed value once and retries on a later commit', async () => {
  const at = runtime('state-retry', null), rt = await import(resolve(at, 'rt.js')), persist = await import(resolve(at, 'persist.js'));
  const storage = globalThis.localStorage, s = { n: { v: 1 } };
  kept.clear(); persist.keep(s, 'count', 'n');
  let blocked = true;
  globalThis.localStorage = { setItem(...args) { if (blocked) throw new Error('unavailable'); storage.setItem(...args); } };
  const flush = () => { for (const f of rt.After) f(); };
  const warnings = () => rt.journal.filter(l => l.includes('persisted state count:') && l.includes('failed:'));
  try {
    flush(); flush(); expect(warnings()).toHaveLength(1);
    blocked = false; flush(); expect(kept.get('exact.state.count')).toBe('1');
    blocked = true; s.n.v = 2; flush(); flush(); expect(warnings()).toHaveLength(2);
    s.n.v = 3; flush(); flush(); expect(warnings()).toHaveLength(3);
    blocked = false; flush(); expect(kept.get('exact.state.count')).toBe('3');
  } finally { globalThis.localStorage = storage; }
});
