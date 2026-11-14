import { test, expect } from 'bun:test';
import { presenceLines, macProvisioning, macProfileNeeds, macProfile, macSigning, macProfileIdentity } from './presence.mjs';
import { macWidgetSigning, macWidgetProfiles, widgetSigningTeam, macWidgetGroup } from './widgets.mjs';
import { shareInfo, macShareEntitlements } from './extensions.mjs';

// LLP 1133.010 D7: what a macOS build without a profile says of the lines only a profile can sign, and the release
// build it refuses.
const app = { id: 'com.example.lock' };
const ceiling = ['net.fetch https://x', 'secret.keep token user-presence optional', 'secret.keep key biometry-any apple.orWatch', 'secret.keep plain'].join('\n');
const none = () => null;
const bake = (grantCeiling, profileEntitlements = []) => ({ inputs: { grantCeiling }, reach: { profileEntitlements } });

test('Mac widgets, share and push resolve ambiguous identities from profiles before choosing extensions', () => {
  const owner = { id: 'com.example.profile', manifest: { widgets: [{ families: ['systemSmall'] }], share_target: {} } };
  const a = 'A'.repeat(40), b = 'B'.repeat(40), teams = { [a]: 'FIRSTTEAM0', [b]: 'CHOSENTEAM' };
  const certificates = Object.entries(teams).map(([fingerprint, team]) => ({ fingerprint, subject: `CN=Apple Development: ${team}\nOU=${team}` }));
  const teamOf = sha => widgetSigningTeam(sha, { certificates });
  const profile = (team, id = owner.id, days = 100) => ({ team, prefix: team, appId: `${team}.${id}`, name: `${team} ${id}`, path: `/profiles/${team}/${id}`,
    platforms: ['OSX'], devices: ['THIS-MAC'], dev: true, method: 'development', expires: new Date(Date.now() + days * 864e5),
    groups: [`${team}.${owner.id}.widgets`], keychain: [`${team}.*`], entitlements: ['com.apple.developer.aps-environment'], aps: 'development' });
  const chosen = profile('CHOSENTEAM'), extension = profile('CHOSENTEAM', `${owner.id}.widgets`);
  let installed = [chosen, extension];
  const profiles = { named: null, udid: 'THIS-MAC' };
  const widgetProfiles = options => macWidgetProfiles({ ...options, ...profiles, installed });
  const identityFor = (p, current, options) => macProfileIdentity(p, current, { ...options, find: team => Object.keys(teams).find(sha => teams[sha] === team), teamOf });
  const find = (app, required, options) => macProfile(app, required, { ...options, ...profiles, installed });
  const said = [];
  const options = { current: null, identities: () => [], find, widgetProfiles, identityFor, teamOf, log: line => said.push(line),
    signWidgets: (app, sha, options) => macWidgetSigning(app, sha, { ...options, certificates, profiles: { ...profiles, installed } }) };
  const push = bake('device.push', ['com.apple.developer.aps-environment']);
  const signed = macSigning(owner, push, options);
  expect(signed.sha1).toBe(b);
  expect(signed.widgets.team).toBe('CHOSENTEAM');
  expect(signed.widgets.profiles.extension).toBe(extension);
  expect(signed.provisioning.profile).toBe(chosen);
  expect(signed.shareTeam).toBe('CHOSENTEAM');
  const group = macWidgetGroup(owner, signed.shareTeam);
  expect(group).toBe('CHOSENTEAM.com.example.profile.widgets');
  expect(shareInfo(owner, group).ExactShareGroup).toBe(group);
  expect(macShareEntitlements(group)).toContain(group);
  expect(said.join('\n')).not.toContain('ad hoc');
  // Share-only and grant-only apps also resolve the team's identity, without a widget to choose it.
  expect(macSigning({ ...owner, manifest: { share_target: {} } }, bake(''), options).shareTeam).toBe('CHOSENTEAM');
  expect(macSigning({ ...owner, manifest: {} }, push, options).sha1).toBe(b);
  installed = [...installed, profile('FIRSTTEAM0', owner.id, 300), profile('FIRSTTEAM0', `${owner.id}.widgets`, 300)];
  for (const manifest of [owner.manifest, { share_target: {} }, {}]) {
    let message;
    try { macSigning({ ...owner, manifest }, push, options); } catch (error) { message = error.message; }
    expect(message).toMatch(/several teams .*DEVELOPMENT_TEAM=<team-id> or EXACT_IDENTITY=<sha1>.*EXACT_IDENTITY=-/);
    for (const capability of ['push', 'App Attest', 'age range', 'keychain-access groups', 'passkeys', 'extensions', 'login-keychain']) expect(message).toContain(capability);
  }
  // An explicit team wins over the other team's later expiry; a named profile overrides it as on devices.
  const saved = process.env.DEVELOPMENT_TEAM;
  try {
    process.env.DEVELOPMENT_TEAM = 'CHOSENTEAM';
    const selected = macSigning({ ...owner, manifest: {} }, push, options);
    expect(selected.sha1).toBe(b);
    expect(selected.provisioning.profile).toBe(chosen);
    const named = installed.find(p => p.team === 'FIRSTTEAM0');
    expect(macProfile(owner, [], { installed, named, udid: 'THIS-MAC' })).toBe(named);
    const share = macSigning({ ...owner, manifest: { share_target: {} } }, push, { ...options, current: b,
      find: (app, required, options) => macProfile(app, required, { ...options, installed, named, udid: 'THIS-MAC' }) });
    expect(share.sha1).toBe(a);
    expect(share.shareTeam).toBe('FIRSTTEAM0');
    installed = installed.filter(p => p.team === 'FIRSTTEAM0');
    expect(() => macSigning({ ...owner, manifest: {} }, push, options)).toThrow(/no suitable .*CHOSENTEAM \(DEVELOPMENT_TEAM\)/);
  } finally { if (saved === undefined) delete process.env.DEVELOPMENT_TEAM; else process.env.DEVELOPMENT_TEAM = saved; }
  installed = [];
  expect(() => macSigning(owner, push, options)).toThrow(/Mac extensions .*DEVELOPMENT_TEAM=<team-id> or EXACT_IDENTITY=<sha1>/);
  expect(macSigning({ ...owner, manifest: {} }, bake(''), options).sha1).toBeNull();
  expect(macSigning(owner, push, { ...options, current: '-' }).widgets).toBeNull();
  expect(said.join('\n')).toContain('this build is signed ad hoc (EXACT_IDENTITY=-)');
});

test('named Mac widget profiles resolve before unrelated candidates, installed or external', () => {
  const owner = { id: 'com.example.widget', manifest: { widgets: [{ families: ['systemSmall'] }] } };
  const profile = (team, id = owner.id) => ({ team, prefix: team, appId: `${team}.${id}`, name: `${team} ${id}`, path: `/profiles/${team}/${id}`,
    platforms: ['OSX'], devices: ['THIS-MAC'], dev: true, method: 'development', expires: new Date('2099-01-01'),
    groups: [`${team}.${owner.id}.widgets`], keychain: [`${team}.*`], entitlements: [] });
  const named = profile('CHOSENTEAM'), extension = profile('CHOSENTEAM', `${owner.id}.widgets`);
  for (const external of [false, true]) {
    const installed = [profile('OTHERTEAM0'), profile('OTHERTEAM0', `${owner.id}.widgets`), extension, ...(external ? [] : [named])];
    const profiles = { installed, named, udid: 'THIS-MAC' };
    const signed = macSigning(owner, bake(''), { current: null, namedWidgetProfile: named,
      find: (a, required, options) => macProfile(a, required, { installed, named: null, udid: 'THIS-MAC', ...options }),
      widgetProfiles: options => macWidgetProfiles({ ...options, ...profiles }),
      identityFor: p => `identity of ${p.team}`, teamOf: () => 'CHOSENTEAM',
      signWidgets: () => ({ team: 'CHOSENTEAM', profiles: macWidgetProfiles({ app: owner, team: 'CHOSENTEAM', ...profiles }) }) });
    expect(signed.sha1).toBe('identity of CHOSENTEAM');
    expect(signed.widgets.profiles.app).toBe(named);
  }
});

test('a resolved Mac identity restricts grant profiles to its team without requiring a profile', () => {
  const profile = team => ({ team, prefix: team, name: team, appId: `${team}.${app.id}`, platforms: ['OSX'],
    devices: ['THIS-MAC'], method: 'development', dev: true, expires: new Date('2099-01-01'), keychain: [`${team}.*`],
    entitlements: ['com.apple.developer.aps-environment', 'com.apple.developer.devicecheck.appattest-environment',
      'com.apple.developer.declared-age-range', 'com.apple.developer.associated-domains'], aps: 'development' });
  const chosen = profile('CHOSENTEAM'), other = profile('OTHERTEAM0'), said = [];
  let installed = [other, chosen];
  const options = { current: 'sole-identity', teamOf: () => 'CHOSENTEAM', identityFor: () => 'sole-identity', log: line => said.push(line),
    find: (a, required, options) => macProfile(a, required, { ...options, installed, named: null, udid: 'THIS-MAC' }) };
  const compat = bake('device.push\nsecret.keep key user-presence\nauth.passkey example.com', chosen.entitlements.slice(0, 3));
  expect(macSigning(app, compat, options).provisioning.profile).toBe(chosen);
  installed = [other];
  const saved = { DEVELOPMENT_TEAM: process.env.DEVELOPMENT_TEAM, EXACT_IDENTITY: process.env.EXACT_IDENTITY };
  try {
    process.env.DEVELOPMENT_TEAM = 'OTHERTEAM0';
    process.env.EXACT_IDENTITY = 'sole-identity';
    const signed = macSigning(app, compat, options);
    expect(signed.sha1).toBe('sole-identity');
    expect(signed.provisioning).toBeNull();
    for (const effect of ['pushPermissionState().available is false', 'attestationStatus().available is false',
      'ageRangeStatus().available is false', 'presenceSecrets is false', 'getClientCapabilities().available is false']) expect(said.join('\n')).toContain(effect);
    said.length = 0;
    const owner = { ...app, manifest: { widgets: [{ families: ['systemSmall'] }] } };
    const widget = macSigning(owner, compat, { ...options, widgetProfiles: none,
      signWidgets: () => ({ team: 'CHOSENTEAM' }) });
    expect(widget.sha1).toBe('sole-identity');
    expect(widget.provisioning).toBeNull();
    expect(said).toHaveLength(5);
  } finally {
    for (const [key, value] of Object.entries(saved)) if (value === undefined) delete process.env[key]; else process.env[key] = value;
  }
});

test('Developer ID profile ambiguity names its own identity selector', () => {
  const installed = ['FIRSTTEAM0', 'CHOSENTEAM'].map(team => ({ team, prefix: team, name: team, appId: `${team}.${app.id}`,
    platforms: ['OSX'], method: 'developerid', dev: false, devices: [], keychain: [], expires: new Date('2099-01-01'),
    entitlements: ['com.apple.developer.aps-environment'], aps: 'production' }));
  for (const manifest of [{}, { share_target: {} }]) {
    let failure;
    try { macSigning({ ...app, manifest }, bake('device.push', ['com.apple.developer.aps-environment']), {
      current: null, distribution: true, find: (a, required, options) => macProfile(a, required, { ...options, installed, named: null, team: null }) }); }
    catch (error) { failure = error.message; }
    expect(failure).toContain('EXACT_DEVELOPER_ID=<sha1>');
    expect(failure).not.toContain('DEVELOPMENT_TEAM');
    expect(failure).not.toContain('EXACT_IDENTITY');
    expect(failure).not.toContain('Apple Development');
  }
});

test('Mac extension refusals name the ad hoc escape and the distribution identity', () => {
  for (const manifest of [{ widgets: [{ families: ['systemSmall'] }] }, { share_target: {} }]) {
    const owner = { id: app.id, manifest };
    expect(() => macSigning(owner, bake(''), { current: null, find: none })).toThrow(/EXACT_IDENTITY=-.*without.*extension/);
    expect(() => macSigning(owner, bake(''), { current: null, distribution: true, find: none })).toThrow(/EXACT_DEVELOPER_ID/);
  }
  const owner = { id: app.id, manifest: { widgets: [{ families: ['systemSmall'] }] } };
  expect(() => macWidgetSigning(owner, null)).toThrow(/EXACT_IDENTITY=-.*without.*extension/);
  expect(() => macWidgetSigning(owner, null, { distribution: true })).toThrow(/EXACT_DEVELOPER_ID/);
});

test('macSigning keeps DEVELOPMENT_TEAM despite another team having a later profile', () => {
  const saved = process.env.DEVELOPMENT_TEAM;
  try {
    process.env.DEVELOPMENT_TEAM = 'CHOSENTEAM';
    const profile = (team, days) => ({ name: team, team, prefix: team, appId: `${team}.${app.id}`, platforms: ['OSX'],
      devices: ['THIS-MAC'], method: 'development', dev: true, expires: new Date(Date.now() + days * 864e5),
      entitlements: ['com.apple.developer.aps-environment'], aps: 'development', keychain: [] });
    const chosen = profile('CHOSENTEAM', 100), other = profile('OTHERTEAM0', 300);
    const options = { current: 'chosen-identity', log: () => {}, identityFor: p => `identity of ${p.team}`,
      find: (a, required, options) => macProfile(a, required, { ...options, installed: [chosen, other], named: null, udid: 'THIS-MAC' }) };
    const signed = macSigning(app, bake('device.push', ['com.apple.developer.aps-environment']), options);
    expect(signed.provisioning.profile).toBe(chosen);
    expect(signed.sha1).toBe('identity of CHOSENTEAM');
  } finally { if (saved === undefined) delete process.env.DEVELOPMENT_TEAM; else process.env.DEVELOPMENT_TEAM = saved; }
});

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
