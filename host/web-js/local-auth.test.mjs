// The runner's local-auth cases (contract/cli/tests/it/local_auth.rs), through modules.js's public call and ticket
// paths. The fixture removes only local-auth's web restrictions to exercise the declared mechanisms (LLP 1133.010 §7).
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

const root = resolve(import.meta.dir, '../..');
const declared = file => JSON.parse(readFileSync(resolve(root, `packages/local-auth/${file}`), 'utf8'));
const iface = declared('exact.json');
const shapes = { ...iface.shapes, ...declared('apple/exact.json').shapes, ...declared('android/exact.json').shapes };
function type(t) {
  if (t.startsWith('option<')) return ['?', type(t.slice(7, -1))];
  if (t.startsWith('enum<')) return ['e', t.slice(5, -1).split('|')];
  return { number: 'n', bool: 'b', string: 's', 'strings-key': 's' }[t]
    ?? Object.fromEntries(Object.entries(shapes[t]).map(([k, v]) => [k, type(v)]));
}
function rows(web) {
  return Object.fromEntries(iface.names.map(n => {
    const sub = structuredClone({ ...n.substitute, permission: iface.permission, origin: n.origin });
    if (!web) { delete sub.web; delete sub.unsupported; }
    return [n.source ?? n.send, [`local-auth.${n.source ?? n.send}`, !!n.send, sub, type(n.answers), n.request ? 'always' : '',
      n.options && Object.fromEntries(Object.entries(n.options).map(([k, v]) => [k, type(v)])), !!n.live]];
  }));
}

let server, browser;
beforeAll(async () => {
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    const local = resolve(root, '.' + path);
    const file = existsSync(local) ? local : resolve(root, 'host/web', path.split('/').at(-1));
    return existsSync(file) ? new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('Not found', { status: 404 });
  } });
  browser = await suiteBrowser();
});
afterAll(async () => { await browser?.close(); server?.stop(true); });

async function world(run, { web = false, camera = false, bound = false, configure = () => {} } = {}) {
  const fixture = rows(web);
  configure(fixture);
  if (camera) {
    const permission = { name: 'camera', status: 'cameraStatus', field: 'state', denied: 'NotAllowedError', asks: { available: true } };
    fixture.authenticate[2].permissionBy = { video: permission };
    fixture.authenticate[5].video = ['?', 'b'];
    fixture.cameraStatus = ['local-auth.cameraStatus', false, { permission, default: { available: true } },
      { available: 'b', state: ['e', ['granted', 'prompt', 'denied', 'unknown']] }];
  }
  if (bound) fixture.authenticate[2].implementation = true;
  const page = await browser.newPage();
  try {
    await page.goto(server.url.href + '?agent');
    await page.evaluate(async ([rows, bound]) => {
      const rt = await import('/host/web-js/rt.js');
      const { modules } = await import('/host/web-js/modules.js');
      const { reportTime } = await import('/host/web/navigation.js');
      rt.clock.agent = true;
      rt.data.grants = 'device.local-auth purpose.lock';
      modules(rows, bound ? { 'local-auth': async () => ({ implementation: () => ({ authenticate: () => ({ timestamp: 1 }) }) }) } : {});
      globalThis.auth = {
        start(args = {}, origin = 'app', contract = false) {
          this.pending = contract
            ? Promise.resolve(rt.data.reserved.authenticate(null, [[args.reason ?? 'reason.unlock', args.biometricOnly ?? null]], 'checked')).then(
              value => ({ timestamp: value[0], android: value[1] }), error => ({ error: error.reason }))
            : exact.extensions.call({ extension: 'local-auth', send: 'authenticate', args: { reason: 'reason.unlock', ...args }, origin });
          return exact.extensions.holds();
        },
        read: () => exact.extensions.call({ extension: 'local-auth', source: 'localAuthStatus' }),
        now: () => reportTime(rt.clock.now)[0] + rt.clock.now,
      };
    }, [fixture, bound]);
    await run({
      page,
      start: (args = {}, origin = 'app', contract = false) => page.evaluate(([a, o, c]) => auth.start(a, o, c), [args, origin, contract]),
      holds: () => page.evaluate(() => exact.extensions.holds()),
      result: () => page.evaluate(() => auth.pending),
      read: () => page.evaluate(() => auth.read()),
      prefer: async req => { const result = await page.evaluate(req => exact.extensions.prefer(req), req); expect(result.error).toBeUndefined(); },
      tap: async (capability, choice, permission = 'local-auth') => {
        const holds = await page.evaluate(() => exact.extensions.holds());
        expect(holds).toHaveLength(1);
        expect(holds[0].device).toEqual({ capability, args: capability === 'permission'
          ? { capability: permission, request: 'authenticate', origin: 'app' } : { request: 'authenticate', origin: 'app' } });
        const result = await page.evaluate(([ticket, choice]) => exact.extensions.answer({ op: 'tap', ticket, choice }), [holds[0].ticket, choice]);
        expect(result).toEqual({ ticket: holds[0].ticket, capability, answered: choice, delivery: 'substituted' });
        return holds[0].ticket;
      },
    });
  } finally { await page.close(); }
}

test('a check asks permission, then the person; each check has its own answer', () => world(async w => {
  expect((await w.start())[0]?.device.capability).toBe('permission');
  const permission = await w.tap('permission', 'allow');
  const presence = (await w.holds())[0];
  expect(presence?.device.capability).toBe('presence');
  expect(presence.ticket).not.toBe(permission);
  expect((await w.read()).state).toBe('granted');
  await w.prefer({ extension: { 'local-auth.localAuthStatus': { available: true, biometry: 'faceID', deviceCredential: true, presenceSecrets: true } } });
  expect(await w.holds()).toEqual([presence]);
  await w.page.evaluate(async () => { (await import('/host/web-js/rt.js')).clock.now += 60000; });
  await w.tap('presence', 'allow');
  expect(await w.result()).toEqual({ timestamp: await w.page.evaluate(() => auth.now()), android: null });
  expect(await w.holds()).toEqual([]);
  expect(await w.page.evaluate(ticket => exact.extensions.answer({ op: 'tap', ticket, choice: 'allow' }), presence.ticket)).toBeUndefined();
  for (const [choice, error] of [['fail', 'AUTHENTICATION_FAILED'], ['lockout', 'LOCKOUT'], ['cancel', 'USER_CANCEL']]) {
    for (const contract of [false, true]) {
      expect((await w.start({}, 'app', contract))[0]?.device.capability).toBe('presence');
      await w.tap('presence', choice);
      expect(await w.result()).toEqual({ error });
    }
  }
}));

test('denial leaves the passcode, fails a biometric-only call and continues a held check', () => world(async w => {
  await w.start();
  await w.tap('permission', 'deny');
  expect((await w.read()).state).toBe('denied');
  await w.tap('presence', 'allow');
  expect((await w.result()).timestamp).toBeNumber();
  expect(await w.start({ biometricOnly: true })).toEqual([]);
  expect(await w.result()).toEqual({ error: 'PERMISSION_DENIED' });
  expect((await w.start())[0]?.device.capability).toBe('presence');
  await w.prefer({ permission: { 'local-auth': 'denied' } });
  expect((await w.holds())[0]?.device.capability).toBe('presence');
  await w.tap('presence', 'cancel');
  expect(await w.result()).toEqual({ error: 'USER_CANCEL' });
  await w.prefer({ permission: { 'local-auth': 'prompt' } });
  await w.start({ biometricOnly: true });
  await w.tap('permission', 'deny');
  expect(await w.holds()).toEqual([]);
  expect(await w.result()).toEqual({ error: 'PERMISSION_DENIED' });
}));

test('cancelling permission keeps prompt, and a device with no authentication has no check ticket', () => world(async w => {
  await w.start();
  await w.tap('permission', 'cancel');
  expect(await w.result()).toEqual({ error: 'PERMISSION_DENIED' });
  expect((await w.read()).state).toBe('prompt');
  await w.prefer({ extension: { 'local-auth.localAuthStatus': { available: false, biometry: 'none', deviceCredential: false, presenceSecrets: false } } });
  expect(await w.start()).toEqual([]);
  expect(await w.result()).toEqual({ error: 'NOT_AVAILABLE' });
}));

test('a check without Face ID skips permission, validates taps and fills a Contract answer', () => world(async w => {
  await w.prefer({ extension: { 'local-auth.localAuthStatus': { available: true, biometry: 'touchID', deviceCredential: true, presenceSecrets: false } } });
  expect((await w.start({}, 'app', true))[0]?.device.capability).toBe('presence');
  const before = await w.holds();
  for (const req of [{ op: 'type', text: '{}' }, { op: 'tap', choice: 'deny' }]) {
    expect((await w.page.evaluate(req => exact.extensions.answer(req), { ticket: before[0].ticket, ...req })).error).toBeString();
    expect(await w.holds()).toEqual(before);
  }
  await w.tap('presence', 'allow');
  expect(await w.result()).toEqual({ timestamp: await w.page.evaluate(() => auth.now()), android: null });
}));

test('presence is never delegated to a tool, in any permission state', () => world(async w => {
  for (const state of ['prompt', 'granted', 'denied']) {
    await w.prefer({ permission: { 'local-auth': state } });
    expect(await w.start({}, 'tool')).toEqual([]);
    expect(await w.result()).toEqual({ error: 'PERMISSION_DENIED', message: 'person-required' });
  }
}));

test('host facts overlay defaults and preferences without changing the preferred value', () => world(async w => {
  expect((await w.read()).presenceSecrets).toBe(false);
  const preferred = { available: true, biometry: 'faceID', deviceCredential: true, presenceSecrets: true };
  await w.prefer({ extension: { 'local-auth.localAuthStatus': preferred } });
  expect((await w.read()).presenceSecrets).toBe(false);
  expect(await w.page.evaluate(() => exact.extensions.state()['local-auth'].preferred.localAuthStatus)).toEqual(preferred);
  await w.prefer({ permission: { 'local-auth': 'granted' } });
  expect((await w.read()).presenceSecrets).toBe(false);
  await w.prefer({ extension: { 'local-auth.localAuthStatus': 'clear' } });
  expect((await w.read()).presenceSecrets).toBe(false);
}));

test.each([false, true])('a selected permission denies independently of the default continuation (bound: %p)', bound => world(async w => {
  await w.prefer({ permission: { 'local-auth': 'granted', camera: 'denied' } });
  expect(await w.start({ video: true })).toEqual([]);
  expect(await w.result()).toEqual({ error: 'NotAllowedError' });
}, { camera: true, bound }));

test.each([false, true])('a selected permission asks according to its own status (bound: %p)', bound => world(async w => {
  await w.prefer({ extension: { 'local-auth.localAuthStatus': { available: false, biometry: 'touchID', deviceCredential: false, presenceSecrets: false } } });
  expect((await w.start({ video: true }))[0]?.device.capability).toBe('permission');
  await w.tap('permission', 'cancel', 'camera');
  expect(await w.result()).toEqual({ error: 'NotAllowedError' });
  expect(await w.page.evaluate(() => exact.extensions.state()['local-auth'].permissions.camera)).toBe('prompt');
  await w.prefer({ extension: { 'local-auth.cameraStatus': { available: false } } });
  expect(await w.start({ video: true })).toEqual([]);
  expect(await w.result()).toEqual(bound ? { timestamp: 1, android: null } : { error: 'NOT_AVAILABLE' });
}, { camera: true, bound }));

test.each([false, true])('permission conditions compare JSON values structurally (bound: %p)', bound => world(async w => {
  const status = {
    available: true, biometry: 'faceID', deviceCredential: true, presenceSecrets: false,
    methods: ['faceID'], policy: { fallback: null, methods: ['faceID'], biometricOnly: false },
  };
  // Defaults and separately supplied preferences have the same values, with different object key order.
  for (const preferred of [false, true]) {
    if (preferred) await w.prefer({ extension: { 'local-auth.localAuthStatus': status } });
    expect((await w.start())[0]?.device.capability).toBe('permission');
    await w.tap('permission', 'cancel');
    expect(await w.result()).toEqual({ error: 'PERMISSION_DENIED' });
  }
  for (const difference of [
    { methods: ['faceID', 'touchID'] },
    { methods: ['touchID'] },
    { policy: { ...status.policy, methods: ['touchID'] } },
    { policy: { methods: ['faceID'], biometricOnly: false } },
  ]) {
    await w.prefer({ extension: { 'local-auth.localAuthStatus': { ...status, ...difference } } });
    const held = await w.start();
    if (bound) expect(held).toEqual([]);
    else {
      expect(held[0]?.device.capability).toBe('presence');
      await w.tap('presence', 'cancel');
    }
    expect(await w.result()).toEqual(bound ? { timestamp: 1, android: null } : { error: 'USER_CANCEL' });
  }
}, { bound, configure(fixture) {
  fixture.authenticate[2].permission.asks = {
    methods: ['faceID'], policy: { biometricOnly: false, methods: ['faceID'], fallback: null },
  };
  Object.assign(fixture.localAuthStatus[2].default, {
    methods: ['faceID'], policy: { fallback: null, methods: ['faceID'], biometricOnly: false },
  });
  Object.assign(fixture.localAuthStatus[3], {
    methods: ['[', 's'], policy: { fallback: ['?', 's'], methods: ['[', 's'], biometricOnly: 'b' },
  });
} }));

// The runner looks a status value up by serde_json's text, whose objects list their keys sorted.
test('a check refused by status matches an object value whatever its key order', () => world(async w => {
  await w.prefer({ permission: { 'local-auth': 'granted' } });
  for (const policy of [{ a: 1, b: 2 }, { b: 2, a: 1 }]) {
    await w.prefer({ extension: { 'local-auth.localAuthStatus': { available: true, biometry: 'touchID', deviceCredential: true, presenceSecrets: false, policy } } });
    expect(await w.start()).toEqual([]);
    expect(await w.result()).toEqual({ error: 'NOT_AVAILABLE' });
  }
  await w.prefer({ extension: { 'local-auth.localAuthStatus': { available: true, biometry: 'touchID', deviceCredential: true, presenceSecrets: false, policy: { a: 1, b: 3 } } } });
  expect((await w.start())[0]?.device.capability).toBe('presence');
}, { configure(fixture) {
  fixture.authenticate[2].check.unless = { policy: { '{"a":1,"b":2}': 'NOT_AVAILABLE' } };
  fixture.localAuthStatus[3].policy = ['?', { a: 'n', b: 'n' }];
} }));

test.each([false, true])('a held ticket uses its selected permission on denial (bound: %p)', bound => world(async w => {
  for (const answer of ['tap', 'prefer']) {
    await w.prefer({ permission: { camera: 'prompt' } });
    expect((await w.start({ video: true }))[0]?.device.capability).toBe('permission');
    if (answer === 'tap') await w.tap('permission', 'deny', 'camera');
    else await w.prefer({ permission: { camera: 'denied' } });
    expect(await w.holds()).toEqual([]);
    expect(await w.result()).toEqual({ error: 'NotAllowedError' });
  }
}, { camera: true, bound }));

test('the real local-auth web cell stays unavailable and unsupported', () => world(async w => {
  for (const state of ['prompt', 'granted', 'denied']) {
    await w.prefer({ permission: { 'local-auth': state } });
    await w.prefer({ extension: { 'local-auth.localAuthStatus': { available: true, biometry: 'faceID', deviceCredential: true, presenceSecrets: true } } });
    expect(await w.read()).toEqual({ available: false, biometry: 'none', deviceCredential: false, state: 'unknown', presenceSecrets: false, apple: null, android: null });
    expect(await w.start()).toEqual([]);
    expect(await w.result()).toEqual({ error: 'NotSupportedError' });
  }
}, { web: true }));
