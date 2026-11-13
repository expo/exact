import { test, expect } from 'bun:test';
import { presenceLines, macProvisioning, macProfileNeeds } from './presence.mjs';

// LLP 1133.010 D7: what a macOS build without a profile says of the lines only a profile can sign, and the release
// build it refuses.
const app = { id: 'com.example.lock' };
const ceiling = ['net.fetch https://x', 'secret.keep token user-presence optional', 'secret.keep key biometry-any apple.orWatch', 'secret.keep plain'].join('\n');
const none = () => null;
const bake = (grantCeiling, profileEntitlements = []) => ({ inputs: { grantCeiling }, reach: { profileEntitlements } });

test('reads the lines that keep a name locked, and which are optional', () => {
  expect(presenceLines(ceiling)).toEqual([{ name: 'token', optional: true }, { name: 'key', optional: false }]);
  expect(presenceLines('secret.keep plain\nsecret.keep other optional')).toEqual([]);
});

test('a development build without a profile says it keeps no locked secret', () => {
  const said = [];
  macProvisioning(app, bake(ceiling), { find: none, log: (l) => said.push(l) });
  expect(said).toHaveLength(1);
  expect(said[0]).toStartWith('secret.keep token, secret.keep key: this build signs no keychain-access-groups');
  expect(said[0]).toContain('presenceSecrets is false, a set of secret.keep key throws NotSupportedError, an optional name stays plain');
});

test('a release build with a name that must be locked is refused without a profile', () => {
  expect(() => macProvisioning(app, bake(ceiling), { release: true, find: none, log: () => {} })).toThrow(/^grant-needs-profile: secret.keep token, secret.keep key: .*Or write `optional` on each secret.keep line\.$/);
  // Only optional names: it ships, keeping them plain.
  expect(() => macProvisioning(app, bake('secret.keep token user-presence optional'), { release: true, find: none, log: () => {} })).not.toThrow();
});

test('an app with no line a profile signs says nothing', () => {
  const said = [];
  expect(macProvisioning(app, bake('secret.keep plain'), { find: none, log: (l) => said.push(l) })).toBeNull();
  expect(said).toEqual([]);
  expect(macProfileNeeds(bake('secret.keep plain'))).toEqual([]);
});

test('a Mac build with a passkey line and no profile has none, and a release build is refused', async () => {
  const { passkeyLines } = await import('./presence.mjs');
  const grants = 'auth.passkey app.example\nauth.passkey localhost';
  expect(passkeyLines(grants)).toEqual(['app.example']);
  const said = [];
  macProvisioning(app, bake(grants), { find: none, log: (l) => said.push(l) });
  expect(said[0]).toContain('getClientCapabilities().available is false');
  expect(() => macProvisioning(app, bake(grants), { release: true, find: none })).toThrow(/^grant-needs-profile: auth.passkey app.example: /);
  expect(macProvisioning(app, bake('auth.passkey localhost'), { release: true, find: none })).toBeNull();
});

test('one profile signs every need: the rows, the keychain group and the domains, each as its build signs it', () => {
  const profile = { name: 'p', team: 'T', prefix: 'P', dev: true, method: 'development' };
  const compat = { ...bake('secret.keep key biometry-any\nauth.passkey app.example', ['com.apple.developer.aps-environment', 'com.apple.developer.declared-age-range']),
    reach: { profileEntitlements: ['com.apple.developer.aps-environment', 'com.apple.developer.declared-age-range'], auth: { passkeys: ['app.example'], associatedDomains: ['webcredentials:app.example'] } } };
  let asked;
  const signed = macProvisioning(app, compat, { find: (a, required) => { asked = required; return profile; }, log: () => {} });
  expect(asked).toEqual(['com.apple.developer.aps-environment', 'com.apple.developer.declared-age-range', 'keychain-access-groups', 'com.apple.developer.associated-domains']);
  expect(signed.entitlements).toEqual({ 'com.apple.application-identifier': 'P.com.example.lock', 'com.apple.developer.team-identifier': 'T',
    'com.apple.developer.aps-environment': 'development', 'com.apple.developer.declared-age-range': true,
    'keychain-access-groups': ['P.com.example.lock'], 'com.apple.developer.associated-domains': ['webcredentials:app.example?mode=developer'] });
});
test('a development build asks for a passkey domain in developer mode, a release build never', async () => {
  const { associatedDomains } = await import('./presence.mjs');
  const auth = { passkeys: ['app.example'], associatedDomains: ['webcredentials:app.example', 'webcredentials:auth.example'] };
  expect(associatedDomains(auth, true)).toEqual(['webcredentials:app.example?mode=developer', 'webcredentials:auth.example']);
  expect(associatedDomains(auth, false)).toEqual(auth.associatedDomains);
  expect(associatedDomains(undefined, true)).toEqual([]);
});

test('Sign in with Apple: a simulator signs it, a device build when its profile allows it, development or distribution (LLP 1133.014 D7)', async () => {
  const { entitlements } = await import('./build.mjs');
  const app = { id: 'com.example.app', manifest: { host: { ios: {} } } };
  const reach = { auth: { signIn: { apple: ['com.example.web'] } } };
  const signs = (team, debuggable, options) => entitlements(app, team, debuggable, reach, options).includes('com.apple.developer.applesignin');
  const allows = ['com.apple.developer.applesignin'];
  expect(signs(null, true, { simulator: true })).toBe(true);
  // A device: the signing profile's entitlements decide, whatever its kind.
  expect(signs('TEAM', true, { allows: [] })).toBe(false);
  expect(signs('TEAM', true, { allows })).toBe(true);
  expect(signs('TEAM', false, { allows, distributed: true })).toBe(true);
  // Unsigned (no profile), or a grant without `auth.apple`: none.
  expect(signs(null, false, {})).toBe(false);
  expect(entitlements(app, 'TEAM', false, { auth: {} }, { allows })).not.toContain('applesignin');
});

test('App Attest and the age range: a Mac build without a profile signs neither, a release is refused; the environment is by build (LLP 1133.028)', async () => {
  const { entitlementValue } = await import('./presence.mjs');
  const rows = ['com.apple.developer.declared-age-range', 'com.apple.developer.devicecheck.appattest-environment'];
  const said = [];
  macProvisioning(app, bake('device.attestation\ndevice.age-range\ndevice.tracking purpose.ads', rows), { find: none, log: (l) => said.push(l) });
  expect(said[0]).toMatch(/^device\.age-range: this build signs no com\.apple\.developer\.declared-age-range, since .*ageRangeStatus\(\)\.available is false/);
  expect(said[1]).toMatch(/^device\.attestation: this build signs no com\.apple\.developer\.devicecheck\.appattest-environment, since .*attestationStatus\(\)\.available is false/);
  expect(() => macProvisioning(app, bake('device.age-range', rows.slice(0, 1)), { release: true, find: none })).toThrow(/^grant-needs-profile: device.age-range: /);
  expect(macProvisioning(app, bake('device.tracking purpose.ads'), { release: true, find: none })).toBeNull();
  const env = 'com.apple.developer.devicecheck.appattest-environment';
  expect([entitlementValue(env, true), entitlementValue(env, false), entitlementValue('com.apple.developer.declared-age-range', true)]).toEqual(['development', 'production', true]);
  expect([entitlementValue('aps-environment', true), entitlementValue('aps-environment', false)]).toEqual(['development', 'production']);
});

test('a release keeps what the build signed under its profile, and nothing else of its signature (LLP 1133.010 D7)', async () => {
  const { profileEntitlements, plistFile } = await import('./assets.mjs');
  const { mkdtempSync, copyFileSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const dir = mkdtempSync(join(tmpdir(), 'exact-profile-entitlements-'));
  try {
    const bin = join(dir, 'true'), ent = join(dir, 'e.plist');
    copyFileSync('/usr/bin/true', bin);
    const kept = { 'com.apple.application-identifier': 'T.com.example', 'com.apple.developer.team-identifier': 'T',
      'com.apple.developer.aps-environment': 'production', 'keychain-access-groups': ['T.com.example'] };
    writeFileSync(ent, plistFile({ ...kept, 'com.apple.security.app-sandbox': true }));
    expect(Bun.spawnSync(['codesign', '--force', '--sign', '-', '--entitlements', ent, bin]).exitCode).toBe(0);
    expect(profileEntitlements(bin)).toEqual(kept);
    expect(profileEntitlements(bin, '')).toEqual({});
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the Mac sample host takes the app\'s id under its profile, and a second identity without one (#396)', async () => {
  const { macHostIdentity } = await import('./presence.mjs');
  const app = { id: 'com.example.notes' };
  expect(macHostIdentity(app, { name: 'Notes Mac', appId: 'TEAM.com.example.notes' })).toEqual({ id: 'com.example.notes', name: 'Host (not Exact)' });
  expect(macHostIdentity(app, null).id).toBe('com.example.notes.host');
});
