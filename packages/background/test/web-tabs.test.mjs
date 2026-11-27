// The web implementation's stored tags are the origin's: each tab writes only the tag it changed.
import { afterEach, beforeEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

const KEY = 'exact.background.sync';
let items, tabs, saved;

beforeEach(() => {
  items = new Map();
  tabs = [];
  saved = {
    localStorage: Object.getOwnPropertyDescriptor(globalThis, 'localStorage'),
    navigator: Object.getOwnPropertyDescriptor(globalThis, 'navigator'),
  };
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: k => items.get(k) ?? null, setItem: (k, v) => void items.set(k, String(v)) },
  });
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { onLine: false } });
});

afterEach(() => {
  for (const t of tabs) t.dispose();
  for (const [name, d] of Object.entries(saved)) {
    if (d) Object.defineProperty(globalThis, name, d);
    else delete globalThis[name];
  }
});

const tab = (outcome = 'completed') => {
  const t = implementation({ changed() {}, log() {}, wakes: { raise: async () => outcome } });
  tabs.push(t);
  return t;
};
const listed = async () => (await tab().syncTags()).map(s => s.tag).sort();
const stored = () => new Map(JSON.parse(items.get(KEY)));
const settle = () => new Promise(r => setTimeout(r, 0));

test('tags registered in two open tabs are both kept for the next open', async () => {
  const a = tab(), b = tab();
  await a.registerSync({ tag: 'upload' });
  await b.registerSync({ tag: 'outbox' });
  expect(await listed()).toEqual(['outbox', 'upload']);
});

test('a tab that completes its tag removes only that tag', async () => {
  const a = tab(), b = tab();
  await a.registerSync({ tag: 'upload' });
  await b.registerSync({ tag: 'outbox' });
  navigator.onLine = true;
  await a.registerSync({ tag: 'upload' });
  await settle();
  expect([...stored().keys()]).toEqual(['outbox']);
});

test('a tag that fails keeps its attempts and backoff beside the other tab\'s', async () => {
  const a = tab('failed'), b = tab();
  await a.registerSync({ tag: 'upload' });
  await b.registerSync({ tag: 'outbox' });
  navigator.onLine = true;
  await a.registerSync({ tag: 'upload' });
  await settle();
  const upload = stored().get('upload');
  expect(upload.attempts).toBe(1);
  expect(upload.notBefore).toBeGreaterThan(Date.now());
  expect(stored().has('outbox')).toBe(true);
});

test('a stored list that does not parse is replaced by the registration', async () => {
  items.set(KEY, 'not json');
  expect(await tab().registerSync({ tag: 'upload' })).toBeNull();
  expect(await listed()).toEqual(['upload']);
});
