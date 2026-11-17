import { expect, test } from 'bun:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { spawnSync } from './toolchain.mjs';
import { tmpdir } from 'node:os';
import { resolve, sep } from 'node:path';
import { plistFile } from './assets.mjs';
import { assertNotificationServiceGrants, extensionKinds, notificationService, notificationServiceEntitlements, notificationServicePlatformRefusal, notificationServiceProfile, packageNotificationService } from './extensions.mjs';
import { notificationServiceInputChanges, notificationServiceMembers, snapshotNotificationService } from '../../scripts/notification-service.mjs';

const source = title => `
import UserNotifications
final class NotificationService: UNNotificationServiceExtension {
  private var finish: ((UNNotificationContent) -> Void)?
  private var content: UNMutableNotificationContent?
  override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
    let changed = request.content.mutableCopy() as! UNMutableNotificationContent
    changed.title = ${JSON.stringify(title)} + changed.title
    content = changed; finish = contentHandler; complete()
  }
  override func serviceExtensionTimeWillExpire() { complete() }
  private func complete() { guard let finish, let content else { return }; self.finish = nil; finish(content) }
}
`;

const repo = resolve(import.meta.dir, '../..');
const exactBake = resolve(repo, process.env.CARGO_TARGET_DIR ?? 'target', 'debug', 'exact-bake');
const rustCompat = dir => existsSync(exactBake)
  ? spawnSync(exactBake, ['compat', dir, '--platform', 'ios', '--target', 'arm64-apple-ios17.0-simulator', '--json'], { encoding: 'utf8' })
  : spawnSync('cargo', ['run', '--quiet', '-p', 'exact-bake', '--', 'compat', dir, '--platform', 'ios', '--target', 'arm64-apple-ios17.0-simulator', '--json'], { cwd: repo, encoding: 'utf8' });
const rustIdentity = dir => {
  const result = rustCompat(dir);
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout).inputs.notificationService;
};

test('an ordinary build discovers the app notification service extension', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-service-'));
  try {
    const folder = resolve(dir, 'modules/apple/notification-service');
    mkdirSync(folder, { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    writeFileSync(resolve(folder, 'NotificationService.swift'), source('[Exact] '));
    writeFileSync(resolve(folder, 'Helper.swift'), 'func helper() {}\n');
    writeFileSync(resolve(folder, '.DS_Store'), 'Finder metadata');
    mkdirSync(resolve(folder, '.helpers'), { recursive: true });
    writeFileSync(resolve(folder, '.helpers/Ignored.swift'), 'class Hidden: UNNotificationServiceExtension {}\n');
    mkdirSync(resolve(folder, 'target'), { recursive: true });
    writeFileSync(resolve(folder, 'target/Generated.swift'), 'class Generated: UNNotificationServiceExtension {}\n');
    const service = notificationService({ dir, id: 'com.example.fixture' }, 'ios');
    assert.equal(service.id, 'com.example.fixture.notifications');
    assert.deepEqual(rustIdentity(dir).files.map(file => file.path), ['Helper.swift', 'NotificationService.swift']);
    assert.ok(extensionKinds([]).has('notifications'));
    assert.deepEqual([...extensionKinds(['--extensions', 'notifications'])], ['notifications']);
    assert.ok(!extensionKinds(['--extensions', 'none']).has('notifications'));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the convention refuses a missing, renamed, or second service subclass', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-classes-'));
  const folder = resolve(dir, 'modules/apple/notification-service');
  try {
    mkdirSync(folder, { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    const app = { dir, id: 'com.example.fixture' }, service = notificationService(app), packageSources = () => packageNotificationService({ service, identity: rustIdentity(dir), app, directory: resolve(dir, 'scratch') });
    expect(packageSources).toThrow(/found none/);
    writeFileSync(resolve(folder, 'A.swift'), 'class Other: UNNotificationServiceExtension {}\n');
    expect(packageSources).toThrow(/named NotificationService; found Other/);
    writeFileSync(resolve(folder, 'B.swift'), 'class NotificationService: UserNotifications.UNNotificationServiceExtension {}\n');
    expect(packageSources).toThrow(/found Other.*NotificationService/);
    rmSync(resolve(folder, 'A.swift')); writeFileSync(resolve(folder, 'image.png'), 'not packaged');
    const unsupported = rustCompat(dir); assert.notEqual(unsupported.status, 0); assert.match(unsupported.stderr, /image\.png.*unsupported notification service resource/);
    expect(() => notificationServiceMembers(folder)).toThrow(/image\.png.*unsupported notification service resource/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the Rust bake inventory is the notification packager inventory', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-inventory-'));
  const folder = resolve(dir, 'modules/apple/notification-service');
  try {
    mkdirSync(resolve(folder, 'Helpers'), { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    writeFileSync(resolve(folder, 'NotificationService.swift'), source('[Exact] '));
    writeFileSync(resolve(folder, 'Helpers.swift'), 'func helper() {}\n');
    writeFileSync(resolve(folder, 'Helpers/Decode.swift'), 'func decode() {}\n');
    writeFileSync(resolve(folder, '.DS_Store'), 'Finder metadata');
    mkdirSync(resolve(folder, '.hidden'));
    writeFileSync(resolve(folder, '.hidden/Ignored.swift'), 'ignored\n');
    const identity = rustIdentity(dir);
    assert.deepEqual(notificationServiceMembers(folder).map(file => file.path), identity.files.map(file => file.path));
    assert.deepEqual(notificationServiceInputChanges(folder, identity), []);
    writeFileSync(resolve(folder, '.DS_Store'), 'changed Finder metadata');
    writeFileSync(resolve(folder, '.hidden/Ignored.swift'), 'changed ignored source\n');
    assert.deepEqual(notificationServiceInputChanges(folder, identity), []);
    writeFileSync(resolve(folder, 'Helpers/Decode.swift'), 'func changed() {}\n');
    assert.deepEqual(notificationServiceInputChanges(folder, identity), ['Helpers/Decode.swift']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('failed notification service verification removes its private snapshot', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-snapshot-'));
  const folder = resolve(dir, 'modules/apple/notification-service'), scratch = resolve(dir, 'scratch');
  try {
    mkdirSync(folder, { recursive: true }); mkdirSync(scratch);
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    const file = resolve(folder, 'NotificationService.swift'), original = source('[Exact] ');
    writeFileSync(file, original);
    const digestIdentity = rustIdentity(dir);
    writeFileSync(file, source('[Changed] '));
    const digestSnapshot = mkdtempSync(resolve(scratch, '.notification-service-'));
    expect(() => snapshotNotificationService(folder, digestIdentity, digestSnapshot)).toThrow(/recorded its digest/);
    assert.ok(!existsSync(digestSnapshot));

    writeFileSync(file, original);
    const membershipIdentity = rustIdentity(dir);
    writeFileSync(resolve(folder, 'Added.swift'), 'func added() {}\n');
    const membershipSnapshot = mkdtempSync(resolve(scratch, '.notification-service-'));
    expect(() => snapshotNotificationService(folder, membershipIdentity, membershipSnapshot)).toThrow(/sources changed/);
    assert.ok(!existsSync(membershipSnapshot));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the bake and packager refuse symbolic-link notification sources by name', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-links-'));
  const folder = resolve(dir, 'modules/apple/notification-service'), shared = resolve(dir, 'shared');
  try {
    mkdirSync(folder, { recursive: true }); mkdirSync(shared);
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    writeFileSync(resolve(folder, 'NotificationService.swift'), source('[Exact] '));
    writeFileSync(resolve(shared, 'Linked.swift'), 'func linked() {}\n');
    mkdirSync(resolve(shared, 'Directory')); writeFileSync(resolve(shared, 'Directory/Nested.swift'), 'func nested() {}\n');
    for (const [name, target] of [['Linked.swift', resolve(shared, 'Linked.swift')], ['LinkedDirectory', resolve(shared, 'Directory')]]) {
      const link = resolve(folder, name); symlinkSync(target, link);
      const baked = rustCompat(dir);
      assert.notEqual(baked.status, 0); assert.match(baked.stderr, new RegExp(`${name}.*symbolic link`));
      expect(() => notificationServiceMembers(folder)).toThrow(new RegExp(`${name}.*symbolic link`));
      rmSync(link);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('the bake and packager refuse a linked notification service directory or parent by path', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-root-links-'));
  try {
    for (const [name, link, target, service] of [
      ['modules/apple/notification-service', 'modules/apple/notification-service', 'outside/service', '.'],
      ['modules/apple', 'modules/apple', 'outside/apple', 'notification-service'],
      ['modules', 'modules', 'outside/modules', 'apple/notification-service'],
    ]) {
      const app = resolve(dir, name.replaceAll('/', '-')), linked = resolve(app, link), outside = resolve(app, target), folder = resolve(outside, service);
      mkdirSync(resolve(linked, '..'), { recursive: true }); mkdirSync(folder, { recursive: true });
      writeFileSync(resolve(app, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
      writeFileSync(resolve(folder, 'NotificationService.swift'), source('[Exact] '));
      symlinkSync(outside, linked, 'dir');
      const baked = rustCompat(app);
      assert.notEqual(baked.status, 0); assert.match(baked.stderr, new RegExp(`${name}.*symbolic link`));
      expect(() => notificationServiceMembers(resolve(app, 'modules/apple/notification-service'))).toThrow(new RegExp(`${name}.*symbolic link`));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('a linked or cyclic module root with no notification service is not refused', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-no-service-link-'));
  try {
    const outside = resolve(dir, 'outside-modules');
    mkdirSync(outside); writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    symlinkSync(outside, resolve(dir, 'modules'), 'dir');
    assert.equal(notificationService({ dir, id: 'com.example.fixture' }), null);
    assert.equal(rustIdentity(dir), null);
    rmSync(resolve(dir, 'modules'));
    mkdirSync(resolve(dir, 'modules'));
    symlinkSync('../alias', resolve(dir, 'modules/apple'), 'dir');
    symlinkSync('modules/apple', resolve(dir, 'alias'), 'dir');
    assert.equal(notificationService({ dir, id: 'com.example.fixture' }), null);
    assert.equal(rustIdentity(dir), null);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a source name beginning with U+FEFF keeps its name', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-bom-name-'));
  const folder = resolve(dir, 'modules/apple/notification-service'), snapshot = resolve(dir, 'snapshot');
  try {
    mkdirSync(resolve(folder, '\uFEFFHelpers'), { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    writeFileSync(resolve(folder, 'NotificationService.swift'), source('[Exact] '));
    writeFileSync(resolve(folder, 'Helper.swift'), 'func plain() {}\n');
    writeFileSync(resolve(folder, '\uFEFFHelpers/\uFEFFHelper.swift'), 'func bom() {}\n');
    const identity = rustIdentity(dir), names = identity.files.map(file => file.path);
    assert.deepEqual(notificationServiceMembers(folder).map(file => file.path), names);
    assert.ok(names.includes('\uFEFFHelpers/\uFEFFHelper.swift'));
    assert.ok(names.includes('Helper.swift'));
    assert.deepEqual(notificationServiceInputChanges(folder, identity), []);
    const copied = snapshotNotificationService(folder, identity, snapshot);
    assert.ok(existsSync(resolve(copied.directory, '\uFEFFHelpers/\uFEFFHelper.swift')));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('the bake and packager validate raw source names before exclusions', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-name-bytes-'));
  const folder = resolve(dir, 'modules/apple/notification-service');
  try {
    mkdirSync(folder, { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    writeFileSync(resolve(folder, 'NotificationService.swift'), source('[Exact] '));
    for (const bytes of [Buffer.from([0x2e, 0xff]), Buffer.from([0xff, 0x2e, 0x73, 0x77, 0x69, 0x66, 0x74])]) {
      const raw = Buffer.concat([Buffer.from(folder + sep), bytes]);
      try { writeFileSync(raw, 'invalid name'); }
      catch (error) {
        assert.equal(error.code, 'EILSEQ');
        return;
      }
      const baked = rustCompat(dir);
      assert.notEqual(baked.status, 0); assert.match(baked.stderr, /non-UTF-8 source name/);
      expect(() => notificationServiceMembers(folder)).toThrow(/non-UTF-8 source name/);
      rmSync(raw);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the service requires push and notification grants and inherits only shared containers', () => {
  const service = { id: 'com.example.fixture.notifications', directory: '/tmp/service', sources: ['/tmp/service/NotificationService.swift'] };
  expect(() => assertNotificationServiceGrants(service, 'device.notifications purpose.notifications')).toThrow(/device.push/);
  expect(() => assertNotificationServiceGrants(service, 'device.push')).toThrow(/companion device.notifications/);
  expect(() => assertNotificationServiceGrants(service, 'device.push\ndevice.notifications purpose.notifications')).not.toThrow();
  assert.deepEqual(notificationServiceEntitlements(service, {
    'application-identifier': 'TEAM123456.com.example.fixture', 'com.apple.developer.team-identifier': 'TEAM123456',
    'get-task-allow': true, 'aps-environment': 'development', 'com.apple.developer.associated-domains': ['applinks:example.com'],
    'com.apple.security.application-groups': ['group.com.example.fixture'], 'keychain-access-groups': ['TEAM123456.com.example.fixture'],
  }), {
    'application-identifier': 'TEAM123456.com.example.fixture.notifications', 'com.apple.developer.team-identifier': 'TEAM123456',
    'get-task-allow': true, 'com.apple.security.application-groups': ['group.com.example.fixture'],
    'keychain-access-groups': ['TEAM123456.com.example.fixture'],
  });
});

test('only iOS and iPadOS package the service folder', () => {
  const service = { directory: 'modules/apple/notification-service' };
  assert.equal(notificationServicePlatformRefusal(service, 'ios'), null);
  for (const platform of ['macOS', 'tvOS', 'watchOS']) assert.match(notificationServicePlatformRefusal(service, platform), new RegExp(`${platform} does not package it`));
});

test('a device service needs its exact App ID profile with the app containers', () => {
  const service = { id: 'com.example.fixture.notifications' }, now = Date.now(), appProfile = {
    name: 'App', team: 'TEAM123456', prefix: 'TEAM123456', method: 'development', devices: ['phone'], expires: new Date(now + 60_000),
  };
  const candidate = { ...appProfile, name: 'Service', path: '/profiles/service.mobileprovision', appId: 'TEAM123456.com.example.fixture.notifications',
    groups: ['group.com.example.fixture'], keychain: ['TEAM123456.*'], platforms: ['iOS'], entitlements: ['com.apple.security.application-groups', 'keychain-access-groups'] };
  const entitlements = { 'com.apple.security.application-groups': ['group.com.example.fixture'], 'keychain-access-groups': ['TEAM123456.com.example.fixture'] };
  assert.equal(notificationServiceProfile({ service, appProfile, udid: 'phone', entitlements, installed: [candidate] }), candidate);
  expect(() => notificationServiceProfile({ service, appProfile, udid: 'phone', entitlements, installed: [{ ...candidate, appId: 'TEAM123456.*' }] })).toThrow(/App ID com\.example\.fixture\.notifications/);
  expect(() => notificationServiceProfile({ service, appProfile, udid: 'phone', entitlements, installed: [{ ...candidate, groups: [] }] })).toThrow(/App Group group\.com\.example\.fixture/);
});

test.skipIf(process.platform !== 'darwin')('the iOS assembly path packages the convention service by default and omits it with --extensions none', async () => {
  const { assembleIOS } = await import('./bundle.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-assembly-')), appDir = resolve(dir, 'app');
  const folder = resolve(appDir, 'modules/apple/notification-service');
  const runApple = (command, args, options = {}) => {
    const result = spawnSync(command, args, { encoding: 'utf8', ...options });
    assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.stderr}`);
    return result;
  };
  try {
    mkdirSync(folder, { recursive: true });
    writeFileSync(resolve(appDir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    writeFileSync(resolve(folder, 'NotificationService.swift'), source('[Exact] '));
    const app = { dir: appDir, id: 'com.example.fixture', name: 'Fixture', displayName: 'Fixture', manifest: {} };
    const service = notificationService(app), identity = rustIdentity(appDir);
    const sdk = runApple('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path'], { read: true }).stdout.trim();
    const triple = `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`;
    const mainSource = resolve(dir, 'main.c'), executable = resolve(dir, 'Fixture');
    writeFileSync(mainSource, 'int main(void) { return 0; }\n');
    runApple('clang', [mainSource, '-o', executable]);
    const plan = Buffer.from('plan'), digest = createHash('sha256').update(plan).digest('hex');
    const assemble = args => {
      const binDir = resolve(dir, args.includes('none') ? 'none' : 'default'), capture = resolve(binDir, 'capture');
      mkdirSync(capture, { recursive: true });
      const selected = extensionKinds(args), notification = selected.has('notifications') ? service : null;
      const bakedCompat = { id: 'compat', embedded: { plan: { sha256: digest, bytes: plan.length }, assets: [] },
        inputs: { grantCeiling: 'device.push\ndevice.notifications purpose.notifications', notificationService: notification ? identity : null } };
      const buildReceipt = { binary: { metadata: {} }, graph: { artifacts: [] } };
      const bundle = resolve(binDir, 'Fixture.app');
      assembleIOS({ app, paths: { bundle, executable: 'Fixture', capture }, binDir, bin: executable, device: false,
        production: false, distribution: false, tv: false, bakedCompat, bakedPlan: plan, buildReceipt,
        serviceDylibs: [], moduleLibraries: [], gpuArtifacts: [], looseArms: [], modulesBuilt: null, hasSvg: false,
        svgFilterBuilt: null, canvasGpuBuilt: null, args, unsigned: true, sha1: '-', linkRoot: binDir,
        sdkName: 'iphonesimulator', sdk, triple, rosterKeys: {}, stampSdk: {}, notification,
        composition: 'embedded', destination: 'ios', target: triple, libDir: capture, appleReach: { entitlements: [] } }, {
        runApple, copyAppleStaticTrees() {}, writeUsageStrings() {}, assertAppleIdentity() {}, copyAppleGpuProducts() {},
        receipt: () => '{}', shippedReceipt: value => value, distributionKeys: () => ({}), appClip: () => null,
        entitlements: () => plistFile({ 'application-identifier': 'SIMULATORX.com.example.fixture', 'get-task-allow': true }),
        infoPlist: () => plistFile({ CFBundleIdentifier: app.id, CFBundleName: app.name, CFBundleDisplayName: app.displayName,
          CFBundleExecutable: 'Fixture', CFBundlePackageType: 'APPL', CFBundleVersion: '1', CFBundleShortVersionString: '1.0',
          MinimumOSVersion: '17.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }),
      });
      return bundle;
    };
    const bundle = assemble([]), extension = resolve(bundle, 'PlugIns/ExactNotificationService.appex');
    assert.ok(existsSync(extension));
    const info = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(extension, 'Info.plist')], { read: true }).stdout);
    assert.equal(info.CFBundleIdentifier, 'com.example.fixture.notifications');
    assert.equal(info.NSExtension.NSExtensionPrincipalClass, 'ExactNotificationService.NotificationService');
    runApple('codesign', ['--verify', '--deep', '--strict', bundle]);
    assert.ok(!existsSync(resolve(assemble(['--extensions', 'none']), 'PlugIns/ExactNotificationService.appex')));
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

test.skipIf(process.platform !== 'darwin')('the service packager makes a signed, closed simulator appex and keeps it across rebuilds and IPA export', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-package-')), appDir = resolve(dir, 'app');
  const bundle = resolve(dir, 'Fixture.app'), scratch = resolve(dir, 'scratch'), folder = resolve(appDir, 'modules/apple/notification-service');
  const runApple = (command, args, options = {}) => {
    const result = spawnSync(command, args, { encoding: 'utf8', ...options });
    assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.stderr}`);
    return result;
  };
  try {
    mkdirSync(resolve(folder, 'Helpers'), { recursive: true }); mkdirSync(bundle, { recursive: true });
    writeFileSync(resolve(appDir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    const file = resolve(folder, 'NotificationService.swift'); writeFileSync(file, source('[Exact] '));
    writeFileSync(resolve(folder, 'Helpers.swift'), 'func helper() {}\n'); writeFileSync(resolve(folder, 'Helpers/Decode.swift'), 'func decode() {}\n');
    writeFileSync(resolve(folder, '.DS_Store'), 'Finder metadata'); mkdirSync(resolve(folder, '.hidden')); writeFileSync(resolve(folder, '.hidden/Ignored.swift'), 'ignored\n');
    mkdirSync(resolve(folder, 'target')); writeFileSync(resolve(folder, 'target/Generated.swift'), 'ignored\n');
    writeFileSync(resolve(folder, 'PrivacyInfo.xcprivacy'), '{"NSPrivacyTracking":false}');
    writeFileSync(resolve(bundle, 'Info.plist'), plistFile({ CFBundleIdentifier: 'com.example.fixture', CFBundleName: 'Fixture', CFBundleDisplayName: 'Fixture',
      CFBundleVersion: '7', CFBundleShortVersionString: '2.1', MinimumOSVersion: '17.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }));
    const sdk = runApple('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path'], { read: true }).stdout.trim();
    const app = { dir: appDir, id: 'com.example.fixture', displayName: 'Fixture' }, service = notificationService(app);
    const entitlements = { 'application-identifier': `SIMULATORX.${service.id}`, 'get-task-allow': true,
      'com.apple.security.application-groups': ['group.com.example.fixture'], 'keychain-access-groups': ['SIMULATORX.com.example.fixture'] };
    const build = () => packageNotificationService({ service, identity: rustIdentity(appDir), app, bundle, directory: scratch, sdk,
      triple: `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`, entitlements, runApple,
      sign: extension => runApple('codesign', ['--force', '--sign', '-', '--timestamp=none', extension]) });
    const bakedIdentity = rustIdentity(appDir);
    writeFileSync(file, source('[Edited after bake] '));
    expect(() => packageNotificationService({ service, app, bundle, directory: scratch, sdk,
      identity: bakedIdentity,
      triple: `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`, entitlements, runApple,
      sign: extension => runApple('codesign', ['--force', '--sign', '-', '--timestamp=none', extension]) })).toThrow(/changed after the bake/);
    assert.deepEqual(readdirSync(scratch).filter(name => name.startsWith('.notification-service-')), []);
    writeFileSync(file, source('[Exact] '));
    const membershipIdentity = rustIdentity(appDir);
    writeFileSync(resolve(folder, 'Added.swift'), 'func added() {}\n');
    expect(() => packageNotificationService({ service, app, bundle, directory: scratch, sdk,
      identity: membershipIdentity,
      triple: `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`, entitlements, runApple,
      sign: extension => runApple('codesign', ['--force', '--sign', '-', '--timestamp=none', extension]) })).toThrow(/sources changed/);
    assert.deepEqual(readdirSync(scratch).filter(name => name.startsWith('.notification-service-')), []);
    rmSync(resolve(folder, 'Added.swift'));
    writeFileSync(file, source('[Exact] '));
    const extension = build(), binary = resolve(extension, 'ExactNotificationService');
    assert.ok(existsSync(binary));
    const info = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(extension, 'Info.plist')], { read: true }).stdout);
    assert.equal(info.CFBundleIdentifier, 'com.example.fixture.notifications');
    assert.equal(info.MinimumOSVersion, '17.0'); assert.equal(info.CFBundleVersion, '7');
    assert.equal(info.NSExtension.NSExtensionPointIdentifier, 'com.apple.usernotifications.service');
    assert.equal(info.NSExtension.NSExtensionPrincipalClass, 'ExactNotificationService.NotificationService');
    const privacy = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(extension, 'PrivacyInfo.xcprivacy')], { read: true }).stdout);
    assert.equal(privacy.NSPrivacyTracking, false);
    assert.match(runApple('otool', ['-l', binary], { read: true }).stdout, /sectname __entitlements/);
    assert.match(runApple('strings', [binary], { read: true }).stdout, /group\.com\.example\.fixture/);
    assert.doesNotMatch(runApple('otool', ['-L', binary], { read: true }).stdout, /Exact(?:Kit|Core|Modules|Runner)/);
    runApple('codesign', ['--verify', '--deep', '--strict', extension]);
    const first = createHash('sha256').update(readFileSync(binary)).digest('hex');
    assert.equal(build(), extension, 'an ordinary rebuild replaces the same appex');
    writeFileSync(file, source('[Changed] '));
    assert.equal(build(), extension, 'a source edit keeps the appex in the product');
    assert.notEqual(createHash('sha256').update(readFileSync(binary)).digest('hex'), first);
    packageNotificationService({ service, identity: rustIdentity(appDir), app, bundle, directory: scratch, sdk, production: true, symbols: resolve(dir, 'symbols'),
      triple: `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`, entitlements, runApple,
      sign: built => runApple('codesign', ['--force', '--sign', '-', '--timestamp=none', built]) });
    assert.ok(existsSync(resolve(dir, 'symbols/ExactNotificationService.appex.dSYM/Contents/Resources/DWARF/ExactNotificationService')));
    const payload = resolve(dir, 'Payload'); mkdirSync(payload); cpSync(bundle, resolve(payload, 'Fixture.app'), { recursive: true });
    const ipa = resolve(dir, 'Fixture.ipa'); runApple('ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', payload, ipa]);
    assert.match(runApple('unzip', ['-l', ipa], { read: true }).stdout, /Payload\/Fixture\.app\/PlugIns\/ExactNotificationService\.appex\/ExactNotificationService/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

test.skipIf(process.platform !== 'darwin')('the service packager compiles the bytes recorded before a packaging callback edits the source', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-race-')), appDir = resolve(dir, 'app');
  const bundle = resolve(dir, 'Fixture.app'), scratch = resolve(dir, 'scratch'), folder = resolve(appDir, 'modules/apple/notification-service');
  const runApple = (command, args, options = {}) => {
    const result = spawnSync(command, args, { encoding: 'utf8', ...options });
    assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.stderr}`);
    return result;
  };
  try {
    mkdirSync(folder, { recursive: true }); mkdirSync(bundle);
    writeFileSync(resolve(appDir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    const file = resolve(folder, 'NotificationService.swift'); writeFileSync(file, source('[Recorded before packaging callback] '));
    writeFileSync(resolve(bundle, 'Info.plist'), plistFile({ CFBundleIdentifier: 'com.example.fixture', CFBundleName: 'Fixture',
      CFBundleExecutable: 'Fixture', CFBundlePackageType: 'APPL', CFBundleVersion: '1', CFBundleShortVersionString: '1.0',
      MinimumOSVersion: '17.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }));
    const sdk = runApple('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path'], { read: true }).stdout.trim();
    const app = { dir: appDir, id: 'com.example.fixture', displayName: 'Fixture' }, service = notificationService(app);
    const identity = rustIdentity(appDir);
    let edited = false;
    const injected = (command, args, options) => {
      if (!edited && command === 'plutil') { edited = true; writeFileSync(file, source('[Edited during packaging] ')); }
      return runApple(command, args, options);
    };
    const extension = packageNotificationService({ service, identity, app, bundle, directory: scratch, sdk,
      triple: `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`, entitlements: {}, runApple: injected, sign() {} });
    const strings = runApple('strings', [resolve(extension, 'ExactNotificationService')], { read: true }).stdout;
    assert.match(strings, /Recorded before packaging callback/);
    assert.doesNotMatch(strings, /Edited during packaging/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

test.skipIf(process.platform !== 'darwin')('a notification service declares its own file-timestamp reasons', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-privacy-')), appDir = resolve(dir, 'app');
  const bundle = resolve(dir, 'Fixture.app'), scratch = resolve(dir, 'scratch'), folder = resolve(appDir, 'modules/apple/notification-service');
  const runApple = (command, args, options = {}) => {
    const result = spawnSync(command, args, { encoding: 'utf8', ...options });
    assert.equal(result.status, 0, `${command} ${args.join(' ')}\n${result.stderr}`);
    return result;
  };
  try {
    mkdirSync(folder, { recursive: true }); mkdirSync(bundle);
    writeFileSync(resolve(appDir, 'app.json'), JSON.stringify({ app: { id: 'com.example.fixture', name: 'Fixture' }, rust: false }));
    writeFileSync(resolve(bundle, 'Info.plist'), plistFile({ CFBundleIdentifier: 'com.example.fixture', CFBundleName: 'Fixture',
      CFBundleExecutable: 'Fixture', CFBundlePackageType: 'APPL', CFBundleVersion: '1', CFBundleShortVersionString: '1.0',
      MinimumOSVersion: '17.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }));
    const sdk = runApple('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path'], { read: true }).stdout.trim();
    const app = { dir: appDir, id: 'com.example.fixture', displayName: 'Fixture' }, service = notificationService(app);
    const build = () => packageNotificationService({ service, identity: rustIdentity(appDir), app, bundle, directory: scratch, sdk,
      triple: `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`, entitlements: {}, runApple, sign() {} });
    const privacy = resolve(folder, 'PrivacyInfo.xcprivacy');
    const declaration = { NSPrivacyAccessedAPITypes: [{ NSPrivacyAccessedAPIType: 'NSPrivacyAccessedAPICategoryFileTimestamp', NSPrivacyAccessedAPITypeReasons: ['DDA9.1'] }] };
    const cases = [
      source('[Exact] ').replace('import UserNotifications', 'import UserNotifications\nimport Foundation')
        .replace('changed.title = "[Exact] " + changed.title', 'changed.title = String(describing: URLResourceValues().contentModificationDate) + changed.title'),
      source('[Exact] ').replace('import UserNotifications', 'import UserNotifications\nimport Darwin')
        .replace('changed.title = "[Exact] " + changed.title', 'var value = Darwin.stat(); _ = stat("/", &value); changed.title = String(value.st_mtimespec.tv_sec) + changed.title'),
    ];
    for (const implementation of cases) {
      rmSync(privacy, { force: true });
      writeFileSync(resolve(folder, 'NotificationService.swift'), implementation);
      expect(build).toThrow(/FileTimestamp/);
      writeFileSync(privacy, JSON.stringify(declaration));
      build();
      const output = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(bundle, 'PlugIns/ExactNotificationService.appex/PrivacyInfo.xcprivacy')], { read: true }).stdout);
      assert.deepEqual(output.NSPrivacyAccessedAPITypes, declaration.NSPrivacyAccessedAPITypes);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);
