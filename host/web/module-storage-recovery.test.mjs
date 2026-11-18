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

// Recovery covers the callback: what the answer does after it settles is not the lease's.
for (const placement of ['main', 'worker']) for (const [name, script] of [
  ['an answer that throws after a completed lease keeps its write', `return store.exclusive('token', async () => store.set('token', 'new'))
    .then(() => { throw new Error('unrelated'); });`],
  ['a POST after a failed read-only lease keeps the credential', `return store.exclusive('token', async () => { throw new Error('read failed'); })
    .catch(() => fetch('https://api.castle.test/report', { method: 'POST', body: 'failed' }));`],
]) test(`${placement}: ${name}`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'following') return store.exclusive('token', async () => store.get('token'));
  ${script}`, { placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    const result = await w.askWithDeliver('first', deliverSuccessful(w));
    if (result.tag === 1) expect(result.request.method).toBe('POST');
    else expect(result.tag).toBe(2);
    expect(result.writes).not.toContainEqual(['token', null, true]);
    const kept = environment.disk.get('exact.secret.token');
    expect(kept).toBe(result.tag === 1 ? 'old' : 'new');
    expect(environment.disk.has('exact.exclusive.exact.secret.token')).toBe(false);
    expect((await w.ask('following')).value).toBe(kept);
  } finally { w.done(); environment.restore(); }
}, 20000);

// A name leased again after another holder deleted it is delivered deleted.
for (const placement of ['main', 'worker']) test(`${placement}: a lease taken again after another holder's deletion delivers the deletion`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'twice') return store.exclusive('token', async () => {
    await fetch('https://api.castle.test/first');
    store.set('token', 'login');
  }).then(() => store.exclusive('token', async () => store.get('token')));
  return store.exclusive('token', async () => {
    await fetch('https://api.castle.test/hold');
    store.forget('token');
  });`, { placement, secrets: true, taskDelayedStorage: true });
  try {
    environment.disk.delete('exact.secret.token');
    expect((await w.askWithDeliver('twice', w.commit)).request.url).toBe('https://api.castle.test/first');
    const holder = w.askWithDeliver('holder', w.commit);
    await environment.requested(2);
    const twice = w.resumeWithDeliver('twice', undefined, w.commit);
    expect((await holder).request.url).toBe('https://api.castle.test/hold');
    await environment.requested(3);
    expect(environment.disk.get('exact.secret.token')).toBe('login');
    expect((await w.resumeWithDeliver('holder', undefined, w.commit)).tag).toBe(0);
    const result = await twice;
    expect(result.value).toBeNull();
    expect(environment.disk.has('exact.secret.token')).toBe(false);
    expect(w.runnerValue('token')).toBeNull();
  } finally { w.done(); environment.restore(); }
}, 20000);

// A failed lease that wrote nothing arms nothing, so a write its answer makes after the failure stands.
for (const placement of ['main', 'worker']) test(`${placement}: a write after a failed unarmed lease stands`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'following') return store.exclusive('token', async () => store.get('token'));
  return store.exclusive('token', async () => { throw new Error('unarmed'); })
    .catch(() => { store.set('token', 'replacement'); return 'caught'; });`, { placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    const result = await w.askWithDeliver('first', w.commit);
    expect(result.value).toBe('caught');
    expect(result.writes).not.toContainEqual(['token', null, true]);
    expect(environment.disk.get('exact.secret.token')).toBe('replacement');
    expect(w.runnerValue('token')).toBe('replacement');
    expect((await w.ask('following')).value).toBe('replacement');
  } finally { w.done(); environment.restore(); }
}, 20000);

// A released name the answer wrote since keeps that write when its next lease reads the names it released.
for (const placement of ['main', 'worker']) test(`${placement}: an answer's write to a name it released stands through its next lease`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`return store.exclusive('foo', async () => store.set('foo', 'first'))
    .then(() => store.exclusive('bar', async () => store.set('foo', 'second')))
    .then(() => store.exclusive('baz', async () => store.get('foo')));`, { placement, secrets: ['foo', 'bar', 'baz'], taskDelayedStorage: true });
  try {
    const result = await w.askWithDeliver('chain', w.commit);
    expect(result.value).toBe('second');
    expect(w.runnerValue('foo')).toBe('second');
  } finally { w.done(); environment.restore(); }
}, 20000);

// A lease's value is what its name holds when it is granted: the answer's earlier write to that name is superseded,
// as a native lease's read replaces it.
for (const placement of ['main', 'worker']) test(`${placement}: a write made before its name's lease does not replace what another holder kept`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`store.set('token', 'older');
  return store.exclusive('token', async () => store.get('token'));`, { placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    environment.disk.set('exact.secret.token', 'newer');
    const result = await w.askWithDeliver('first', w.commit);
    expect(result.value).toBe('newer');
    expect(environment.disk.get('exact.secret.token')).toBe('newer');
    expect(w.runnerValue('token')).toBe('newer');
  } finally { w.done(); environment.restore(); }
}, 20000);

// Recovery arms as the callback makes a request with effects, though the callback fails before its answer settles,
// and a write of the value the lease holds arms nothing (LLP 1146 D5).
for (const placement of ['main', 'worker']) for (const [name, script, kept] of [
  ['a POST its failed callback did not await forgets the credential', `fetch('https://api.castle.test/report', { method: 'POST' }); throw new Error('after sending');`, null],
  ['a failed callback that wrote back what it held keeps the credential', `store.set('token', store.get('token')); throw new Error('unchanged');`, 'old'],
]) test(`${placement}: ${name}`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'following') return store.exclusive('token', async () => store.get('token'));
  return store.exclusive('token', () => { ${script} }).catch(e => 'caught: ' + e.message);`, { placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    await w.askWithDeliver('first', deliverSuccessful(w));
    expect(environment.disk.get('exact.secret.token') ?? null).toBe(kept);
    expect(environment.disk.has('exact.exclusive.exact.secret.token')).toBe(false);
    expect((await w.ask('following')).value).toBe(kept);
  } finally { w.done(); environment.restore(); }
}, 20000);

// A lease's write back of what it holds is no write, so a write after its failure is not the lease's either.
for (const placement of ['main', 'worker']) test(`${placement}: a write after a failed lease that wrote back what it held stands`, async () => {
  const environment = secretEnvironment('old');
  const w = await moduleRealm(`if (source === 'following') return store.exclusive('token', async () => store.get('token'));
  return store.exclusive('token', () => { store.set('token', store.get('token')); throw new Error('unchanged'); })
    .catch(() => { store.set('token', 'replacement'); return 'caught'; });`, { placement, secrets: true, stored: [['token', 'old']], taskDelayedStorage: true });
  try {
    expect((await w.askWithDeliver('first', w.commit)).value).toBe('caught');
    expect(environment.disk.get('exact.secret.token')).toBe('replacement');
    expect((await w.ask('following')).value).toBe('replacement');
  } finally { w.done(); environment.restore(); }
}, 20000);

// A request with effects arms the lease that made it, not the one the answer holds when the request is delivered.
for (const placement of ['main', 'worker']) test(`${placement}: an earlier lease's request with effects does not arm the next lease`, async () => {
  const environment = secretEnvironment('old');
  environment.disk.set('exact.secret.bar', 'kept');
  const w = await moduleRealm(`return store.exclusive('foo', () => { fetch('https://api.castle.test/report', { method: 'POST' }); return 'foo'; })
    .then(() => store.exclusive('bar', async () => { await fetch('https://api.castle.test/safe'); throw new Error('bar failed'); }))
    .catch(e => e.message);`, { placement, secrets: ['foo', 'bar'], stored: [['bar', 'kept']], taskDelayedStorage: true });
  try {
    let result = await w.askWithDeliver('chain', deliverSuccessful(w));
    for (let i = 0; result.tag === 1 && i < 4; i++) result = await w.resumeWithDeliver('chain', undefined, deliverSuccessful(w));
    expect(result.value).toBe('bar failed');
    expect(environment.disk.get('exact.secret.bar')).toBe('kept');
  } finally { w.done(); environment.restore(); }
}, 20000);
