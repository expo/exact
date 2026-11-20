import { test } from 'bun:test';
import assert from 'node:assert/strict';

test('watch sync is an explicit binary transport selection and each companion carries its own allowlist', async () => {
  const { watchSyncSelected, watchSyncKeys, watchInfo } = await import('./watch.mjs');
  const { widgetExtensionKeys } = await import('./widgets.mjs');
  const app = { id: 'dev.exact.sync', manifest: {} };
  assert.equal(watchSyncSelected(app), false);
  assert.deepEqual(watchSyncKeys(app), {});
  assert.deepEqual(widgetExtensionKeys({}, 'watchos'), {});
  app.manifest.watch = { source: 'watch.contract', component: 'Watch', sync: { state: [], secrets: [] } };
  assert.equal(watchSyncSelected(app), false);
  app.manifest.watch.sync = { state: ['liked'], secrets: ['seed'] };
  const keys = watchSyncKeys(app, 'secret.keep seed');
  assert.equal(watchSyncSelected(app), true);
  assert.deepEqual(widgetExtensionKeys(keys, 'watchos'), keys);
  assert.deepEqual(keys.ExactWatchSync.policy, { state: ['liked'], secrets: ['seed'] });
  assert.equal(watchInfo(app, { appInfo: keys, entries: [], live: true }).ExactWatchSync.grants, 'secret.keep seed');
});

test('watch and complication carry the same Keychain group', async () => {
  const { watchInfo, watchEntitlements } = await import('./watch.mjs');
  const { widgetExtensionKeys } = await import('./widgets.mjs');
  const app = { id: 'dev.exact.sync', manifest: { watch: { sync: { secrets: ['seed'] } } } };
  const info = watchInfo(app, { appInfo: {}, group: 'group.dev.exact.sync.watchkitapp', entries: [], live: true, keychain: 'TEAM.dev.exact.sync.watchkitapp' });
  assert.equal(info.ExactKeychainAccessGroup, 'TEAM.dev.exact.sync.watchkitapp');
  assert.equal(widgetExtensionKeys(info, 'watchos').ExactKeychainAccessGroup, info.ExactKeychainAccessGroup);
  for (const id of ['dev.exact.sync.watchkitapp', 'dev.exact.sync.watchkitapp.widgets']) {
    const signed = watchEntitlements({ id, group: info.ExactAppGroup, keychain: info.ExactKeychainAccessGroup, device: true,
      profile: { prefix: 'TEAM', team: 'TEAM', dev: true } });
    assert.deepEqual(signed['keychain-access-groups'], [info.ExactKeychainAccessGroup]);
    assert.equal(signed['application-identifier'], `TEAM.${id}`);
  }
});

test('a secret replica requires a profile granting the shared Keychain group', async () => {
  const { watchProfiles } = await import('./watch.mjs');
  const app = { id: 'dev.exact.sync', manifest: { watch: { sync: { secrets: ['seed'] } } } };
  const profile = { name: 'watch', appId: 'PREFIX.dev.exact.sync.watchkitapp', prefix: 'PREFIX', team: 'TEAM',
    method: 'development', dev: true, devices: ['phone'], platforms: ['iOS'], expires: new Date(Date.now() + 86400_000),
    groups: ['group.dev.exact.sync.watchkitapp'], keychain: [] };
  const options = { app, udid: 'phone', like: profile, installed: [profile] };
  assert.throws(() => watchProfiles(options), /Keychain group PREFIX.dev.exact.sync.watchkitapp/);
  profile.keychain = ['PREFIX.*'];
  assert.deepEqual(watchProfiles(options), [profile]);
  profile.keychain = ['PREFIX.dev.exact.sync.watchkitapp'];
  assert.deepEqual(watchProfiles(options), [profile]);
  app.manifest.watch.sync = { state: ['liked'] };
  profile.keychain = [];
  assert.deepEqual(watchProfiles(options), [profile]);
});

test("every app's watch companion builds in one shared Swift scratch per destination and trust", async () => {
  const { watchSwiftScratch } = await import('./watch.mjs');
  const { appleArtifacts } = await import('./build.mjs');
  const { resolveApp } = await import('../../scripts/app.mjs');
  const [hello, caltrain] = ['hello', 'caltrain'].map(resolveApp);
  assert.notEqual(hello.dir, caltrain.dir);
  for (const device of [false, true]) for (const production of [false, true]) {
    const scratch = watchSwiftScratch(hello, { device, production });
    assert.equal(watchSwiftScratch(caltrain, { device, production }), scratch);
    assert.ok(!scratch.startsWith(appleArtifacts(hello, { destination: 'ios' }).owner + '/'), scratch);
  }
  const all = [false, true].flatMap(device => [false, true].map(production => watchSwiftScratch(hello, { device, production })));
  assert.equal(new Set(all).size, 4);
  assert.equal(new Set([...all, appleArtifacts(hello, { destination: 'ios' }).swift, appleArtifacts(hello, { destination: 'ios-simulator' }).swift]).size, 6);
});
