import { test, expect } from 'bun:test';
import { mkdtempSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, chmodSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { listIdentities, preflight, ascValidation, debuggable, entitlementLines, appstore } from './appstore.mjs';
import { signRelease, signedEntitlements, plistFile } from './assets.mjs';
import { macSigning, macProfile, macProfileIdentity, shareStoreProfile, sandboxEntitlements } from './presence.mjs';
import { macWidgetProfiles, macWidgetSigning, widgetSigningTeam, widgetId } from './widgets.mjs';
import { shareId } from './extensions.mjs';

// LLP 1030.005: the Mac App Store lane's refusals, its signing roles and its profiles, without Apple's credentials:
// identities from a stubbed `security`, synthetic App Store profiles, and real ad hoc signatures on a synthetic bundle.
const TEAM = 'STORETEAM0', SHA = 'C'.repeat(40), INSTALLER = 'D'.repeat(40), DEVID = 'E'.repeat(40);
const id = (sha1, name) => ({ sha1, name, team: /\(([A-Z0-9]{10})\)$/.exec(name)?.[1] ?? null });
const store = id(SHA, `Apple Distribution: Example (${TEAM})`), installer = id(INSTALLER, `3rd Party Mac Developer Installer: Example (${TEAM})`);
const devid = id(DEVID, `Developer ID Application: Example (${TEAM})`);
const app = { id: 'com.example.store', displayName: 'Store', manifest: { categories: ['productivity'] } };
const lists = (codesigning, basic) => (policy) => policy === 'codesigning' ? codesigning : basic;

test('the identities are read from security, the installer\'s under the basic policy', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-appstore-security-')), saved = process.env.PATH;
  try {
    writeFileSync(join(dir, 'codesigning'), `  1) ${SHA} "${store.name}"\n  2) ${DEVID} "${devid.name}"\n     2 valid identities found\n`);
    writeFileSync(join(dir, 'basic'), `  1) ${INSTALLER} "${installer.name}"\n     1 valid identities found\n`);
    writeFileSync(join(dir, 'security'), `#!/bin/sh\n[ "$1 $2 $3" = "find-identity -v -p" ] && /bin/cat "${dir}/$4"\n`);
    chmodSync(join(dir, 'security'), 0o755);
    process.env.PATH = `${dir}:${saved}`;
    expect(listIdentities('codesigning')).toEqual([store, devid]);
    expect(listIdentities('basic')).toEqual([installer]);
  } finally { process.env.PATH = saved; rmSync(dir, { recursive: true, force: true }); }
});

test('a store build with nothing to sign with names every lack at once', () => {
  let message;
  try { preflight({ ...app, manifest: {} }, { env: {}, list: lists([devid], []) }); } catch (error) { message = error.message; }
  const lines = message.split('\n');
  expect(lines).toHaveLength(3);
  expect(lines[0]).toStartWith('app.json names no category');
  expect(lines[0]).toContain('LSApplicationCategoryType');
  expect(lines[1]).toContain('no "Apple Distribution" certificate is on this Mac (the older form is "3rd Party Mac Developer Application")');
  expect(lines[1]).toContain('EXACT_APPSTORE_ID=<sha1>');
  expect(lines[2]).toContain('no "Mac Installer Distribution" certificate is on this Mac');
  expect(lines[2]).toContain('security find-identity -p basic lists it, and -p codesigning does not');
  // Ad hoc needs only the category.
  expect(() => preflight({ ...app, manifest: {} }, { adHoc: true, env: {} })).toThrow(/^app.json names no category[^\n]*$/);
  expect(preflight(app, { adHoc: true, env: {} }).identity).toBeNull();
});

test('the store identities are chosen by kind and team, or by name', () => {
  const env = {}, found = preflight(app, { env, list: lists([devid, store], [installer]), directories: [] });
  expect([found.identity, found.installer]).toEqual([store, installer]);
  expect(found.asc.skip).toContain('EXACT_ASC_KEY_ID and EXACT_ASC_ISSUER_ID');
  expect(() => preflight(app, { env: { EXACT_APPSTORE_ID: DEVID }, list: lists([devid, store], [installer]) }))
    .toThrow(`EXACT_APPSTORE_ID names "${devid.name}", which is not an Apple Distribution`);
  expect(() => preflight(app, { env: { EXACT_APPSTORE_ID: 'F'.repeat(40) }, list: lists([store], [installer]) })).toThrow('names no valid identity');
  const other = id('A'.repeat(40), 'Apple Distribution: Other (OTHERTEAM0)');
  expect(() => preflight(app, { env: {}, list: lists([store, other], [installer]) })).toThrow(/several Apple Distribution identities .*EXACT_APPSTORE_ID=<sha1>/);
  expect(preflight(app, { env: { DEVELOPMENT_TEAM: TEAM }, list: lists([store, other], [installer]) }).identity).toEqual(store);
  // The installer is of the signing identity's team.
  const foreign = id('B'.repeat(40), 'Mac Installer Distribution: Other (OTHERTEAM0)');
  expect(() => preflight(app, { env: {}, list: lists([store], [foreign]) })).toThrow(`no "Mac Installer Distribution" certificate of team ${TEAM}`);
});

test('Apple\'s validation runs with a whole App Store Connect key, is skipped with none, and refuses half of one', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-appstore-keys-'));
  try {
    expect(ascValidation({}, [dir]).skip).toStartWith('Apple\'s validation was not run');
    expect(ascValidation({ EXACT_ASC_KEY_ID: 'KEY123' }, [dir]).problem).toBe(`Apple's validation needs EXACT_ASC_ISSUER_ID, AuthKey_KEY123.p8 (in one of ${dir}) beside what is set`);
    writeFileSync(join(dir, 'AuthKey_KEY123.p8'), 'key');
    expect(ascValidation({ EXACT_ASC_KEY_ID: 'KEY123', EXACT_ASC_ISSUER_ID: 'issuer' }, [dir])).toEqual({ keyId: 'KEY123', issuer: 'issuer', key: join(dir, 'AuthKey_KEY123.p8') });
    expect(() => preflight(app, { env: { EXACT_ASC_ISSUER_ID: 'issuer' }, list: lists([store], [installer]), directories: [dir] })).toThrow('needs EXACT_ASC_KEY_ID');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a line the sandbox cannot take refuses the store build, every one named', () => {
  const compat = { reach: { sandboxEntitlements: ['com.apple.security.app-sandbox'], sandboxRefusals: ['`process.spawn path:t3`: the Mac App Store sandbox runs no program outside the app\'s bundle', '`env.read login:PATH`: …'] } };
  expect(() => sandboxEntitlements(compat)).toThrow(/^grant-sandbox: the Mac App Store sandbox cannot take these lines \(LLP 1030\.005 D2\); a Developer ID build keeps them:\n  `process\.spawn path:t3`: .*\n  `env\.read login:PATH`/);
  expect(() => macSigning({ ...app, manifest: {} }, compat, { current: SHA, distribution: true, store: 'appstore' })).toThrow(/^grant-sandbox/);
  expect(sandboxEntitlements({ reach: { sandboxEntitlements: ['com.apple.security.app-sandbox'], sandboxRefusals: [] } })).toEqual({ 'com.apple.security.app-sandbox': true });
  // `mac --sandbox` refuses the same lines, and signs the dev server's connection besides, ad hoc and with no profile.
  expect(() => macSigning({ ...app, manifest: {} }, compat, { current: '-', sandboxed: true })).toThrow(/^grant-sandbox/);
  const development = macSigning({ ...app, manifest: {} }, { inputs: { grantCeiling: '' }, reach: { profileEntitlements: [], sandboxEntitlements: ['com.apple.security.app-sandbox'], sandboxRefusals: [] } },
    { current: '-', sandboxed: true, log: () => {} });
  expect(development.sandbox).toEqual({ 'com.apple.security.app-sandbox': true, 'com.apple.security.network.client': true });
  expect(development.provisioning).toBeNull();
});

// Synthetic App Store and Developer ID profiles of one team; the Developer ID ones live longer, so only the method
// keeps a store build off them.
const certificates = [{ fingerprint: SHA, subject: `CN=${store.name}\nOU=${TEAM}` }, { fingerprint: DEVID, subject: `CN=${devid.name}\nOU=${TEAM}` }];
const teamOf = (sha1, distribution, kind) => { try { return widgetSigningTeam(sha1, { distribution, store: kind, certificates }); } catch { return null; } };
const owner = { id: 'com.example.store', manifest: { widgets: [{ families: ['systemSmall'] }], share_target: {} } };
const profile = (forId, method, days) => ({ team: TEAM, prefix: TEAM, appId: `${TEAM}.${forId}`, name: `${method} ${forId}`, path: `/profiles/${method}/${forId}`,
  platforms: ['OSX'], devices: [], dev: false, method, expires: new Date(Date.now() + days * 864e5), certificates: [SHA, DEVID],
  groups: [`${TEAM}.${owner.id}.widgets`], keychain: [`${TEAM}.*`], entitlements: [] });
const sandboxed = { inputs: { grantCeiling: '' }, reach: { profileEntitlements: [], sandboxEntitlements: ['com.apple.security.app-sandbox'], sandboxRefusals: [] } };
function signing(subject, installed, current = SHA) {
  const profiles = { installed, named: null, udid: null };
  return macSigning(subject, sandboxed, { current, release: true, distribution: true, store: 'appstore', log: () => {}, identities: () => [], teamOf,
    find: (a, required, options) => macProfile(a, required, { installed, named: null, ...options }),
    widgetProfiles: (options) => macWidgetProfiles({ ...options, ...profiles }),
    signWidgets: (a, sha1, options) => macWidgetSigning(a, sha1, { ...options, certificates, profiles }),
    identityFor: (p, sha1, options) => macProfileIdentity(p, sha1, { ...options, teamOf }),
    shareProfile: (a, team) => shareStoreProfile(a, team, { installed }) });
}

test('a store build with widgets and share signs each bundle under its own App Store profile', () => {
  const all = [owner.id, widgetId(owner), shareId(owner)];
  const installed = [...all.map((p) => profile(p, 'appstore', 100)), ...all.map((p) => profile(p, 'developerid', 300))];
  const signed = signing(owner, installed);
  expect(signed.sha1).toBe(SHA);
  expect(signed.provisioning.profile.name).toBe(`appstore ${owner.id}`);
  expect(signed.widgets.profiles.extension.name).toBe(`appstore ${widgetId(owner)}`);
  expect(signed.shareProfile.name).toBe(`appstore ${shareId(owner)}`);
  expect(signed.sandbox).toEqual({ 'com.apple.security.app-sandbox': true });
  expect(signed.provisioning.entitlements).toEqual({ 'com.apple.application-identifier': `${TEAM}.${owner.id}`, 'com.apple.developer.team-identifier': TEAM });
  // A Developer ID identity is not the store's.
  expect(() => signing(owner, installed, DEVID)).toThrow(/an Apple Distribution or 3rd Party Mac Developer Application/);
});

test('a share-only store build needs the app\'s and the extension\'s App Store profiles, and says which is missing', () => {
  const shareOnly = { id: owner.id, manifest: { share_target: {} } };
  const both = [profile(owner.id, 'appstore', 100), profile(shareId(owner), 'appstore', 100)];
  const signed = signing(shareOnly, both);
  expect([signed.provisioning.profile.name, signed.shareProfile.name, signed.shareTeam]).toEqual([`appstore ${owner.id}`, `appstore ${shareId(owner)}`, TEAM]);
  expect(() => signing(shareOnly, [profile(owner.id, 'appstore', 100)])).toThrow(`appstore-needs-profile: no App Store macOS provisioning profile on this Mac for ${shareId(owner)} of team ${TEAM}`);
  // Developer ID profiles do not stand in for the store's, and the app's profile is required even with no grant needing one.
  let message;
  try { signing({ id: owner.id, manifest: {} }, [profile(owner.id, 'developerid', 300)]); } catch (error) { message = error.message; }
  expect(message).toStartWith(`appstore-needs-profile: no App Store macOS provisioning profile on this Mac for ${owner.id}; in the Apple Developer portal, make an App Store macOS profile`);
  expect(message).toEndWith('TestFlight requires the profile (LLP 1030.005 D1).');
});

function bundle(dir, info) {
  const at = join(dir, 'Store.app'), contents = join(at, 'Contents');
  for (const sub of ['MacOS', 'Helpers', 'PlugIns/ExactShare.appex/Contents/MacOS']) mkdirSync(join(contents, sub), { recursive: true });
  writeFileSync(join(contents, 'Info.plist'), plistFile({ CFBundleExecutable: 'Store', CFBundleIdentifier: app.id, CFBundlePackageType: 'APPL', ...info }));
  copyFileSync('/usr/bin/true', join(contents, 'MacOS/Store'));
  copyFileSync('/usr/bin/true', join(contents, 'Helpers/tool'));
  const extension = join(contents, 'PlugIns/ExactShare.appex');
  writeFileSync(join(extension, 'Contents/Info.plist'), plistFile({ CFBundleExecutable: 'ExactShare', CFBundleIdentifier: `${app.id}.share`, CFBundlePackageType: 'XPC!' }));
  copyFileSync('/usr/bin/true', join(extension, 'Contents/MacOS/ExactShare'));
  const ent = join(dir, 'share.plist'), group = `${TEAM}.${app.id}.widgets`;
  writeFileSync(ent, plistFile({ 'com.apple.security.app-sandbox': true, 'com.apple.security.application-groups': [group] }));
  expect(Bun.spawnSync(['codesign', '--force', '--sign', '-', '--entitlements', ent, extension]).exitCode).toBe(0);
  return { at, extension, group };
}

test('the store signs each part for its role: the app sandboxed, a helper inheriting, the share extension kept (LLP 1030.005 D1)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-appstore-sign-'));
  try {
    const { at, extension, group } = bundle(dir, { ExactShareGroup: `${TEAM}.${app.id}.widgets` });
    const compat = { inputs: { store: { L: '0' } }, reach: { entitlements: [], sandboxEntitlements: ['com.apple.security.app-sandbox', 'com.apple.security.network.client'],
      sandboxSources: { 'com.apple.security.app-sandbox': ['any app'], 'com.apple.security.network.client': ['net.fetch https://api.example'] } } };
    const scratch = join(dir, 'scratch');
    mkdirSync(scratch);
    const order = signRelease(at, { compat, manifest: {}, identity: '-', id: app.id, scratch, store: true, hardened: false, timestamp: ['--timestamp=none'] });
    const signed = Object.fromEntries(signedEntitlements(order).map(({ path, entitlements }) => [path.slice(at.length + 1) || 'app', entitlements]));
    expect(signed.app).toEqual({ 'com.apple.security.app-sandbox': true, 'com.apple.security.network.client': true, 'com.apple.security.application-groups': [group] });
    expect(signed['Contents/Helpers/tool']).toEqual({ 'com.apple.security.app-sandbox': true, 'com.apple.security.inherit': true });
    expect(signed['Contents/PlugIns/ExactShare.appex']).toEqual({ 'com.apple.security.app-sandbox': true, 'com.apple.security.application-groups': [group] });
    expect(Bun.spawnSync(['codesign', '--verify', '--deep', '--strict', at]).exitCode).toBe(0);
    expect(debuggable(signedEntitlements(order))).toEqual([]);
    expect(entitlementLines(signed.app, compat)).toEqual(['  com.apple.security.app-sandbox: any app', '  com.apple.security.application-groups: the extensions\' App Group',
      '  com.apple.security.network.client: net.fetch https://api.example']);
    // A signature that allows a debugger is found wherever it is.
    const ent = join(dir, 'debug.plist');
    writeFileSync(ent, plistFile({ 'com.apple.security.get-task-allow': true }));
    Bun.spawnSync(['codesign', '--force', '--sign', '-', '--entitlements', ent, join(at, 'Contents/Helpers/tool')]);
    expect(debuggable(signedEntitlements(order))).toEqual([join(at, 'Contents/Helpers/tool')]);
    // A Developer ID release keeps the share extension's entitlements and the app's group too, and signs no sandbox.
    signRelease(at, { compat, manifest: {}, identity: '-', id: app.id, scratch, timestamp: ['--timestamp=none'] });
    const release = signedEntitlements([extension, at]);
    expect(release[0].entitlements['com.apple.security.application-groups']).toEqual([group]);
    expect(release[1].entitlements).toEqual({ 'com.apple.security.application-groups': [group] });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('appstore writes its .pkg inside out using the sanitized executable name even when displayName has traversal', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-appstore-path-'));
  try {
    const out = join(dir, 'nested/out'), calls = [];
    const badApp = { id: app.id, displayName: '../../escape', manifest: { categories: ['productivity'], deploy: { store: { macos: '0' } } } };
    const built = join(dir, 'built/Store.app');
    mkdirSync(join(built, 'Contents/Resources'), { recursive: true });
    writeFileSync(join(built, 'Contents/Info.plist'), plistFile({ CFBundleExecutable: 'Store', CFBundleIdentifier: app.id, CFBundlePackageType: 'APPL' }));
    mkdirSync(join(built, 'Contents/MacOS'), { recursive: true });
    copyFileSync('/usr/bin/true', join(built, 'Contents/MacOS/Store'));
    writeFileSync(join(built, 'Contents/Resources/receipt.json'), JSON.stringify({ build: { compat: { inputs: { store: { L: '0' } }, reach: { entitlements: [], sandboxEntitlements: ['com.apple.security.app-sandbox'] } } } }));
    const sh = (cmd, args) => {
      calls.push([cmd, args]);
      if (cmd === '/usr/bin/ditto') { mkdirSync(join(args[1], 'Contents/Resources'), { recursive: true }); writeFileSync(join(args[1], 'Contents/Resources/receipt.json'), readFileSync(join(args[0], 'Contents/Resources/receipt.json'))); mkdirSync(join(args[1], 'Contents/MacOS'), { recursive: true }); copyFileSync('/usr/bin/true', join(args[1], 'Contents/MacOS/Store')); writeFileSync(join(args[1], 'Contents/Info.plist'), readFileSync(join(args[0], 'Contents/Info.plist'))); }
    };
    const pkg = appstore(badApp, {
      build: () => built, out, env: {}, log: () => {}, list: lists([store], [installer]), sh,
      sign: (staged) => [staged], signed: (order) => order.map((path) => ({ path, entitlements: { 'com.apple.security.app-sandbox': true } })),
    });
    expect(pkg.startsWith(`${out}/`)).toBe(true);
    expect(pkg.endsWith('/-..-escape.pkg')).toBe(true);
    const productbuild = calls.find(([cmd]) => cmd === 'productbuild');
    expect(productbuild[1].at(-1)).toBe(pkg);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
