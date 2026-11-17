import { expect, test } from 'bun:test';
import { moduleRealm, secretEnvironment } from './module-storage-harness.mjs';

const deliverSuccessful = realm => result => {
  if (result.tag === 0) realm.commit(result);
};

const failures = [
  ['before any effect', `throw new Error('validation failed');`, 'none'],
  ['after only a GET', `await fetch('https://api.castle.test/safe'); throw new Error('after GET');`, 'request'],
  ['after only a timer', `await new Promise(resolve => setTimeout(resolve, 0)); throw new Error('after timer');`, 'timer'],
];

for (const placement of ['main', 'worker']) for (const caught of [false, true]) {
  for (const [name, body, wait] of failures) test(`${placement}: a browser lease rejected ${name}${caught ? ' and caught' : ''} preserves its credential`, async () => {
    const environment = secretEnvironment('old');
    const w = await moduleRealm(`if (source === 'failed') {
      const failed = store.exclusive('token', async () => { ${body} });
      return ${caught ? "failed.catch(error => 'caught: ' + error.message)" : 'failed'};
    }
    return store.exclusive('token', async () => store.get('token'));`, {
      placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true,
    });
    try {
      let result = await w.askWithDeliver('failed', deliverSuccessful(w));
      if (wait === 'request') {
        expect(result.request.method).toBe('GET');
        result = await w.resumeWithDeliver('failed', undefined, deliverSuccessful(w));
      } else if (wait === 'timer') {
        expect(result.timer).toBeDefined();
        result = await w.resumeWithDeliver('failed', undefined, deliverSuccessful(w));
      }
      if (caught) expect(result.value).toStartWith('caught: ');
      else expect(result.tag).toBe(2);
      expect(result.writes).not.toContainEqual(['token', null, true]);
      expect(environment.disk.get('exact.secret.token')).toBe('old');
      expect((await w.ask('following')).value).toBe('old');
    } finally { w.done(); environment.restore(); }
  }, 20000);
}

for (const placement of ['main', 'worker']) test(`${placement}: a refused browser lease acquisition emits no deletion`, async () => {
  const environment = secretEnvironment('old');
  environment.refuseNext();
  const w = await moduleRealm(`return store.exclusive('token', async () => {
    store.forget('token');
    return 'body ran';
  });`, { placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    const refused = await w.ask('refused');
    expect(refused.tag).toBe(2);
    expect(refused.writes).toEqual([]);
    expect(environment.disk.get('exact.secret.token')).toBe('old');
  } finally { w.done(); environment.restore(); }
}, 20000);

test(`main: forgetting one lease during another answer's storage turn stages no deletion`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'forgotten') return store.exclusive('token', async () => {
    try { await fetch('https://api.castle.test/safe'); }
    catch (error) {
      await new Promise(resolve => { globalThis.releaseForgotten = resolve; });
      throw error;
    }
  });
  return store.exclusive('token', async () => {
    await read('app:/data/held');
    return store.get('token');
  });`, { placement: 'main', secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    expect((await w.ask('forgotten')).request.method).toBe('GET');
    const active = w.askWithDeliver('active', w.commit);
    await environment.requested(2);
    w.forget(['active']);
    await w.storageEvent('held', 'app:/data/held');
    w.callGlobal('releaseForgotten');
    await environment.checkpoint();
    w.releaseHeldStorage();
    const result = await active;
    expect(result.writes).toEqual([]);
    expect(result.value).toBe('old');
    expect(environment.disk.get('exact.secret.token')).toBe('old');
    expect(w.runnerValue('token')).toBe('old');
  } finally { w.done(); environment.restore(); }
}, 20000);
