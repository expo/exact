import { afterAll, expect, test } from 'bun:test';

const saved = ['BroadcastChannel', 'window', 'location', 'history', 'sessionStorage']
  .map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]);
afterAll(() => {
  for (const [key, descriptor] of saved) {
    if (descriptor) Object.defineProperty(globalThis, key, descriptor);
    else delete globalThis[key];
  }
});

async function callback({ hasOpener = false, stored = null, throws = false }, number) {
  const sent = [];
  const opener = hasOpener ? { postMessage: message => sent.push(['opener', message]) } : null;
  globalThis.window = { opener, close() { sent.push(['close']); } };
  globalThis.location = {
    href: 'https://app.example/.exact/auth/callback',
    origin: 'https://app.example',
    pathname: '/.exact/auth/callback',
  };
  globalThis.history = { replaceState() { sent.push(['replace']); } };
  globalThis.sessionStorage = {
    getItem(key) { sent.push(['get', key]); if (throws) throw new Error('storage unavailable'); return stored; },
    removeItem(key) { sent.push(['remove', key]); if (throws) throw new Error('storage unavailable'); },
  };
  globalThis.BroadcastChannel = class {
    postMessage(message) { sent.push(['channel', message]); }
    close() {}
  };
  await import(`./auth-callback.js?callback-test=${number}`);
  return sent;
}

test('the callback reads its popup identity, removes it, and sends it to its opener and then the channel', async () => {
  const sent = await callback({ hasOpener: true, stored: 'a'.repeat(32) }, 1);
  expect(sent).toEqual([
    ['get', 'exact.auth.return'],
    ['remove', 'exact.auth.return'],
    ['opener', { type: 'exact-auth', url: 'https://app.example/.exact/auth/callback', identity: 'a'.repeat(32) }],
    ['channel', { type: 'exact-auth', url: 'https://app.example/.exact/auth/callback', identity: 'a'.repeat(32) }],
    ['replace'],
    ['close'],
  ]);
});

test('a callback with no stored identity, or storage that throws, sends its URL without one', async () => {
  for (const [number, options, before] of [
    [2, {}, [['get', 'exact.auth.return'], ['remove', 'exact.auth.return']]],
    [3, { throws: true }, [['get', 'exact.auth.return']]],
  ]) {
    const sent = await callback(options, number);
    expect(sent).toEqual([
      ...before,
      ['channel', { type: 'exact-auth', url: 'https://app.example/.exact/auth/callback' }],
      ['replace'],
      ['close'],
    ]);
  }
});
