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

test('a Mac HTTPS auth callback selects a profile and signs its webcredentials domain', () => {
  const callback = 'https://auth.example/return', domains = 'com.apple.developer.associated-domains';
  const compat = { ...bake(`auth.session https://accounts.google.com\nauth.callback ${callback}\nsecret.keep session`),
    reach: { auth: { callbacks: [callback], associatedDomains: ['webcredentials:auth.example'] } } };
  const profile = { name: 'auth', team: 'TEAM', prefix: 'TEAM', dev: true, method: 'development',
    appId: `TEAM.${app.id}`, platforms: ['OSX'], devices: ['THIS-MAC'], expires: new Date('2099-01-01'),
    entitlements: [domains] };
  const signed = macSigning(app, compat, { current: 'identity', teamOf: () => 'TEAM', identityFor: () => 'identity', log: () => {},
    find: (a, required, options) => macProfile(a, required, { ...options, installed: [profile], named: null, udid: 'THIS-MAC' }) });
  expect(signed.provisioning?.profile).toBe(profile);
  expect(signed.provisioning?.entitlements[domains]).toEqual(['webcredentials:auth.example?mode=developer']);
  const said = [];
  expect(macProvisioning(app, compat, { find: none, log: line => said.push(line) })).toBeNull();
  expect(said.join('\n')).toContain('HTTPS auth sessions fail association validation');
  expect(() => macProvisioning(app, compat, { release: true, find: none })).toThrow(/^grant-needs-profile: auth.callback https:\/\/auth.example\/return:/);
  for (const callback of ['app.test:/return', 'http://127.0.0.1/return', 'https://auth.example/.exact/auth/callback']) {
    expect(macProfileNeeds({ reach: { auth: { callbacks: [callback], associatedDomains: [] } } })).toEqual([]);
  }
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
test('a development build asks for every webcredentials domain in developer mode, a release build never', async () => {
  const { associatedDomains } = await import('./presence.mjs');
  const auth = { passkeys: ['app.example'], associatedDomains: ['webcredentials:app.example', 'webcredentials:auth.example', 'applinks:app.example'] };
  expect(associatedDomains(auth, true)).toEqual(['webcredentials:app.example?mode=developer', 'webcredentials:auth.example?mode=developer', 'applinks:app.example']);
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

test('SharePlay: a Mac build without a profile signs no Group Activities, and a release ships without it (LLP 1133.001.000 D3)', async () => {
  const { entitlementValue, macProfileEntitlements } = await import('./presence.mjs');
  const key = 'com.apple.developer.group-session', compat = bake('device.shareplay', [key]), said = [];
  expect(macProvisioning(app, compat, { release: true, find: none, log: (l) => said.push(l) })).toBeNull();
  expect(said).toEqual([expect.stringMatching(/^device\.shareplay: this build signs no com\.apple\.developer\.group-session, since .*: SharePlay is unavailable to the app\.$/)]);
  expect(macProfileNeeds(compat)).toEqual([{ entitlement: key, lines: 'device.shareplay', effect: 'SharePlay is unavailable to the app', mandatory: false }]);
  expect(entitlementValue(key, true)).toBe(true);
  const profile = { prefix: 'TEAM', team: 'TEAM', dev: true };
  expect(macProfileEntitlements(app, profile, macProfileNeeds(compat), compat)[key]).toBe(true);
});

test('CloudKit signs its arrays and capitalised environment, and an unprovisioned Mac release claims no shares', async () => {
  const { entitlements, infoPlist, macInfoPlist, tvReach } = await import('./build.mjs');
  const { macProfileEntitlements, signsGrant } = await import('./presence.mjs');
  const { hostEventKeys } = await import('./plist.mjs');
  const { plistFile, PROFILE_KEYS } = await import('./assets.mjs');
  const owner = { id: 'com.example.boards', displayName: 'Boards', manifest: { host: {} } };
  const services = 'com.apple.developer.icloud-services', containers = 'com.apple.developer.icloud-container-identifiers';
  const environment = 'com.apple.developer.icloud-container-environment', keys = [services, containers, environment];
  for (const container of [null, 'iCloud.com.example.shared']) {
    const values = { [services]: ['CloudKit'], ...(container ? { [containers]: [container] } : {}) };
    const reach = { entitlements: keys, profileEntitlements: keys, entitlementValues: values, tvOmits: keys };
    const compat = { inputs: { grantCeiling: `device.cloudkit${container ? ` ${container}` : ''}` }, reach };
    for (const dev of [true, false]) {
      const expected = { [services]: ['CloudKit'], [containers]: [container ?? `iCloud.${owner.id}`], [environment]: dev ? 'Development' : 'Production' };
      const signed = entitlements(owner, 'TEAM', dev, reach, { distributed: !dev });
      for (const [key, value] of Object.entries(expected)) {
        const entry = plistFile({ [key]: value }).match(/<dict>([\s\S]*)<\/dict>/)[1].trim();
        expect(signed).toContain(entry);
        expect(PROFILE_KEYS).toContain(key);
      }
      expect(macProfileEntitlements(owner, { prefix: 'PREFIX', team: 'TEAM', dev }, macProfileNeeds(compat), compat)).toMatchObject(expected);
      for (const omitted of ['aps-environment', 'ubiquity-', 'remote-notification']) expect(signed).not.toContain(omitted);
    }
    for (const bundle of [{ ...owner, clipParent: owner.id }, { ...owner, widgetAppId: owner.id }]) {
      expect(entitlements(bundle, null, true, reach)).not.toContain('com.apple.developer.icloud-');
    }
    expect(entitlements(owner, null, true, tvReach(reach))).not.toContain('com.apple.developer.icloud-');
    const said = [];
    expect(macProvisioning(owner, compat, { release: true, find: none, log: l => said.push(l) })).toBeNull();
    expect(said.join('\n')).toContain('CloudKit is unavailable');
    expect(signsGrant('device.cloudkit', compat, null)).toBe(false);
    expect(signsGrant('device.cloudkit', compat, { entitlements: { [services]: ['CloudKit'] } })).toBe(false);
    const taken = provisioning => ({ packages: hostEventKeys([{ extension: 'invites', event: 'cloudkit-share' }], line => signsGrant(line, compat, provisioning)) });
    expect(macInfoPlist(owner, { reach: taken(null) })).not.toContain('CKSharingSupported');
    const clip = infoPlist(owner, false, { reach: taken(undefined), clip: true });
    expect(clip).not.toContain('CKSharingSupported');
    expect(clip).not.toContain('ExactHostEvents');
    for (const plist of [infoPlist(owner, false, { reach: taken(undefined) }), macInfoPlist(owner, { reach: taken({ entitlements: Object.fromEntries(keys.map(k => [k, true])) }) })]) {
      expect(plist).toContain('CKSharingSupported');
      expect(plist).toContain('ExactHostEvents');
    }
  }
});

test('CloudKit profiles must allow the service, selected container and signed environment by value on iOS and macOS', async () => {
  const { decodeProfile, profile, allows } = await import('./devices.mjs');
  const { grantEntitlementValues } = await import('./presence.mjs');
  const { plistFile } = await import('./assets.mjs');
  const services = 'com.apple.developer.icloud-services', containers = 'com.apple.developer.icloud-container-identifiers';
  const environment = 'com.apple.developer.icloud-container-environment', required = [services, containers, environment];
  const owner = { id: 'com.example.boards' }, values = { [services]: ['CloudKit'], [containers]: ['iCloud.com.example.shared'] };
  for (const mac of [false, true]) {
    const decoded = (edits = {}) => decodeProfile('/synthetic/cloudkit', 'darwin', { read: () => ({ stdout: plistFile({
      Name: 'CloudKit', TeamIdentifier: ['TEAM'], Platform: [mac ? 'OSX' : 'iOS'], ProvisionedDevices: ['DEVICE'],
      ExpirationDate: new Date(Date.now() + 864e7).toISOString(),
      Entitlements: { [mac ? 'com.apple.application-identifier' : 'application-identifier']: `TEAM.${owner.id}`, 'get-task-allow': true,
        ...values, [environment]: ['Development', 'Production'], ...edits },
    }) }) });
    const chosen = decoded();
    chosen.expires = new Date(Date.now() + 864e7);
    const select = installed => mac
      ? macProfile(owner, required, { installed, named: null, team: null, udid: 'DEVICE', values })
      : profile('DEVICE', owner.id, required, { installed, named: null, team: null, values });
    expect(select([chosen])).toBe(chosen);
    expect(allows({ ...chosen, dev: false }, required, values)).toBe(true);
    expect(allows({ ...chosen, dev: false, entitlementValues: { ...chosen.entitlementValues, [environment]: ['Development'] } }, required, values)).toBe(false);
    expect(allows({ ...chosen, entitlementValues: { ...chosen.entitlementValues, [containers]: ['iCloud.com.example.*'] } }, required, values)).toBe(true);
    const defaults = grantEntitlementValues(owner, { entitlementValues: { [services]: ['CloudKit'] } });
    expect(defaults[containers]).toEqual([`iCloud.${owner.id}`]);
    const defaultProfile = { ...chosen, entitlementValues: { ...chosen.entitlementValues, [containers]: defaults[containers] } };
    expect(allows(defaultProfile, required, defaults)).toBe(true);
    if (mac) expect(macProfile(owner, required, { installed: [defaultProfile], named: null, team: null, udid: 'DEVICE', values: defaults })).toBe(defaultProfile);
    for (const edits of [{ [services]: ['CloudDocuments'] }, { [containers]: ['iCloud.com.example.other'] }, { [environment]: ['Production'] }]) {
      const wrong = { ...decoded(edits), expires: chosen.expires };
      expect(allows(wrong, required, values)).toBe(false);
      if (mac) expect(select([wrong])).toBeNull();
      else expect(() => select([wrong])).toThrow(/grant-device-profile/);
      expect(select([wrong, chosen])).toBe(chosen);
    }
  }
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
      'com.apple.developer.aps-environment': 'production', 'keychain-access-groups': ['T.com.example'],
      'com.apple.developer.applesignin': ['Default'], 'com.apple.developer.in-app-payments': ['merchant.com.example'] };
    writeFileSync(ent, plistFile({ ...kept, 'com.apple.security.app-sandbox': true }));
    expect(Bun.spawnSync(['codesign', '--force', '--sign', '-', '--entitlements', ent, bin]).exitCode).toBe(0);
    expect(profileEntitlements(bin)).toEqual(kept);
    expect(profileEntitlements(bin, '')).toEqual({});
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the Mac signs Sign in with Apple and Apple Pay from its profile, Developer ID and store builds alike (LLP 1030.005 §6 piece 4)', async () => {
  const { macProfileEntitlements } = await import('./presence.mjs');
  const signIn = 'com.apple.developer.applesignin', pay = 'com.apple.developer.in-app-payments';
  const compat = { inputs: { grantCeiling: 'auth.apple com.example.web\npayment.apple-pay merchant.com.example.lock' },
    reach: { profileEntitlements: [], auth: { signIn: { apple: ['com.example.web'] } }, payments: { merchantIds: ['merchant.com.example.lock'] } } };
  const needs = macProfileNeeds(compat);
  expect(needs).toEqual([
    { entitlement: signIn, lines: 'auth.apple com.example.web', mandatory: false, effect: expect.stringContaining('NotSupportedError') },
    { entitlement: pay, lines: 'payment.apple-pay merchant.com.example.lock', mandatory: false, effect: expect.stringContaining('canMakePayment() is false') },
  ]);
  // A synthetic profile of each distribution kind: one allowing both is chosen, one lacking Apple Pay is not.
  const profile = (method, entitlements, name = method) => ({ name, team: 'TEAM', prefix: 'TEAM', appId: `TEAM.${app.id}`, path: `/p/${name}`,
    platforms: ['OSX'], devices: [], dev: false, method, expires: new Date(Date.now() + 864e7), entitlements, keychain: [], groups: [] });
  for (const method of ['developerid', 'appstore']) {
    const both = profile(method, [signIn, pay]), lacking = profile(method, [signIn], `${method}-no-pay`);
    const find = (a, required, options) => macProfile(a, required, { ...options, installed: [lacking, both], named: null, team: undefined });
    const signed = macProvisioning(app, compat, { release: true, method, find, log: () => {} });
    expect(signed.profile).toBe(both);
    expect(signed.entitlements[signIn]).toEqual(['Default']);
    expect(signed.entitlements[pay]).toEqual(['merchant.com.example.lock']);
    expect(macProfile(app, [signIn, pay], { method, installed: [lacking], named: null, team: undefined })).toBeNull();
  }
  // Without a profile neither is signed, and a Developer ID release still ships, saying what the app reports; a store
  // release is refused, since TestFlight requires the profile.
  const said = [];
  expect(macProvisioning(app, compat, { release: true, method: 'developerid', find: none, log: (l) => said.push(l) })).toBeNull();
  expect(() => macProvisioning(app, compat, { release: true, method: 'appstore', find: none, log: () => {} })).toThrow(/^appstore-needs-profile: no App Store macOS/);
  expect(said).toEqual([expect.stringMatching(/^auth\.apple com\.example\.web: this build signs no com\.apple\.developer\.applesignin, since no Developer ID macOS/),
    expect.stringMatching(/^payment\.apple-pay merchant\.com\.example\.lock: this build signs no com\.apple\.developer\.in-app-payments/)]);
  expect(macProfileEntitlements(app, { prefix: 'P', team: 'T', dev: false }, needs, compat)).toEqual({ 'com.apple.application-identifier': 'P.com.example.lock',
    'com.apple.developer.team-identifier': 'T', [signIn]: ['Default'], [pay]: ['merchant.com.example.lock'] });
});

test('the Mac sample host takes the app\'s id under its profile, and a second identity without one (#396)', async () => {
  const { macHostIdentity } = await import('./presence.mjs');
  const app = { id: 'com.example.notes' };
  expect(macHostIdentity(app, { name: 'Notes Mac', appId: 'TEAM.com.example.notes' })).toEqual({ id: 'com.example.notes', name: 'Host (not Exact)' });
  expect(macHostIdentity(app, null).id).toBe('com.example.notes.host');
});

test('the iCloud key-value store signs the application identifier as its value, where a profile allows it (expo/exact#663)', async () => {
  const { entitlements, SIMULATOR_PREFIX } = await import('./build.mjs');
  const { KEY_VALUE_STORE, macProfileEntitlements } = await import('./presence.mjs');
  const { profile: iosProfile, allows } = await import('./devices.mjs');
  const { mkdtempSync, copyFileSync, writeFileSync, rmSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const { join } = await import('node:path');
  const store = { id: 'com.example.notes', manifest: { host: { ios: {} } } };
  const reach = { entitlements: [KEY_VALUE_STORE] };
  const value = (plist) => new RegExp(`<key>${KEY_VALUE_STORE.replaceAll('.', '\\.')}</key>\\s*<string>([^<]*)</string>`).exec(plist)?.[1];
  // iOS: the signing profile's prefix, or a simulator's; an App Clip and an extension sign none.
  const device = entitlements(store, 'TEAMID0000', true, reach, { prefix: 'PREFIX0000' });
  expect(value(device)).toBe('PREFIX0000.com.example.notes');
  expect(value(entitlements(store, null, true, reach, { simulator: true }))).toBe(`${SIMULATOR_PREFIX}.com.example.notes`);
  expect(entitlements({ ...store, id: 'com.example.notes.Clip', clipParent: store.id }, null, true, reach, { simulator: true })).not.toContain(KEY_VALUE_STORE);
  expect(entitlements({ ...store, widgetAppId: store.id }, null, true, reach, { simulator: true })).not.toContain(KEY_VALUE_STORE);
  // The signature carries it: codesign reads it back from a signed executable.
  const dir = mkdtempSync(join(tmpdir(), 'exact-kvstore-'));
  try {
    const bin = join(dir, 'true'), ent = join(dir, 'e.plist');
    copyFileSync('/usr/bin/true', bin);
    writeFileSync(ent, device);
    expect(Bun.spawnSync(['codesign', '--force', '--sign', '-', '--entitlements', ent, bin]).exitCode).toBe(0);
    expect(value(Bun.spawnSync(['codesign', '-d', '--entitlements', '-', '--xml', bin]).stdout.toString())).toBe('PREFIX0000.com.example.notes');
  } finally { rmSync(dir, { recursive: true, force: true }); }
  // An iOS device build needs a profile that allows it: one without iCloud is refused, naming the entitlement.
  const ios = (entitlements, name) => ({ name, team: 'TEAMID0000', prefix: 'PREFIX0000', appId: 'PREFIX0000.*', path: `/p/${name}`, platforms: ['iOS'],
    devices: ['PHONE'], dev: true, method: 'development', expires: new Date(Date.now() + 864e7), entitlements, keychain: [], groups: [] });
  const without = ios(['application-identifier'], 'no iCloud'), withStore = ios(['application-identifier', KEY_VALUE_STORE], 'iCloud');
  expect(allows(without, reach.entitlements)).toBe(false);
  expect(() => iosProfile('PHONE', store.id, reach.entitlements, { installed: [without], named: null, team: null }))
    .toThrow(/^grant-device-profile: com\.apple\.developer\.ubiquity-kvstore-identifier is needed by this app's grants/);
  expect(iosProfile('PHONE', store.id, reach.entitlements, { installed: [without, withStore], named: null, team: null })).toBe(withStore);
  // The Mac: only under a profile that allows it, valued from that profile; a release without one is refused.
  const compat = bake('device.icloud-kv', [KEY_VALUE_STORE]), said = [];
  expect(macProvisioning(store, compat, { find: none, log: (l) => said.push(l) })).toBeNull();
  expect(said).toEqual([expect.stringMatching(/^device\.icloud-kv: this build signs no com\.apple\.developer\.ubiquity-kvstore-identifier, since .*: NSUbiquitousKeyValueStore keeps its values on this Mac and syncs none\.$/)]);
  expect(() => macProvisioning(store, compat, { release: true, find: none })).toThrow(/^grant-needs-profile: device\.icloud-kv: /);
  const mac = (entitlements, name) => ({ ...ios(entitlements, name), platforms: ['OSX'], devices: ['THIS-MAC'] });
  expect(macProfile(store, [KEY_VALUE_STORE], { installed: [mac([], 'no iCloud')], named: null, udid: 'THIS-MAC', team: null })).toBeNull();
  const chosen = mac([KEY_VALUE_STORE], 'iCloud');
  expect(macProfile(store, [KEY_VALUE_STORE], { installed: [chosen], named: null, udid: 'THIS-MAC', team: null })).toBe(chosen);
  expect(macProfileEntitlements(store, chosen, macProfileNeeds(compat), compat)[KEY_VALUE_STORE]).toBe('PREFIX0000.com.example.notes');
});

test('Sensitive Content Analysis signs its array on iOS and under a Mac profile, and a Mac release without one is refused (expo/exact#708)', async () => {
  const { entitlements } = await import('./build.mjs');
  const { macProfileEntitlements } = await import('./presence.mjs');
  const { PROFILE_KEYS } = await import('./assets.mjs');
  const key = 'com.apple.developer.sensitivecontentanalysis.client', value = { [key]: ['analysis'] };
  const analyzer = { id: 'com.example.safe', manifest: { host: { ios: {} } } };
  const signed = entitlements(analyzer, null, true, { entitlements: [key], entitlementValues: value }, { simulator: true });
  expect(signed.replace(/\s+/g, '')).toContain(`<key>${key}</key><array><string>analysis</string></array>`);
  const compat = { ...bake('device.sensitive-content-analysis', [key]), reach: { profileEntitlements: [key], entitlementValues: value } }, said = [];
  expect(macProvisioning(analyzer, compat, { find: none, log: (l) => said.push(l) })).toBeNull();
  expect(said).toEqual([expect.stringMatching(/^device\.sensitive-content-analysis: this build signs no com\.apple\.developer\.sensitivecontentanalysis\.client, since .*: SCSensitivityAnalyzer reports nothing as sensitive\.$/)]);
  expect(() => macProvisioning(analyzer, compat, { release: true, find: none })).toThrow(/^grant-needs-profile: device\.sensitive-content-analysis: /);
  expect(macProfileEntitlements(analyzer, { prefix: 'P', team: 'T', dev: true }, macProfileNeeds(compat), compat)[key]).toEqual(['analysis']);
  expect(PROFILE_KEYS).toContain(key);
});
