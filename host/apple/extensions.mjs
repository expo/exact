import { capturePrivacyManifest, writePrivacyManifest } from './privacy.mjs';
// @ref LLP 1133.024 D4, D22 item 5 — app extensions of more than one kind:
// what an appex compile links with, and the share extension, which links
// no Exact runtime and writes only its inbox in the App Group.
import { relative, resolve } from 'node:path';
import { mkdirSync, mkdtempSync, writeFileSync, copyFileSync, existsSync, readFileSync, rmSync } from 'node:fs';
import { plistFile } from './assets.mjs';
import { stripProductionExecutable } from './link.mjs';
import { appleActivationRule, applePresentation, appleShareMembers, appleSharePage } from '../../scripts/share-target.mjs';
import { embeddedProfile } from './devices.mjs';
import { swiftOptimization } from './toolchain.mjs';
import { notificationServiceDirectoryExists, notificationServiceSourceRoot, snapshotNotificationService } from '../../scripts/notification-service.mjs';
import { fileURLToPath } from 'node:url';
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
/** The Clang module cache every extension compile shares (widget, Live Activity, share, intents): a cache per build
 * directory rebuilt the system frameworks' modules for each, about two minutes for one alarm extension. Clang keys
 * its entries by compiler, SDK and flags, and writes them safely across concurrent builds. */
export const extensionModuleCache = resolve(root, 'target', 'apple-extension-module-cache');
/** The module cache an invocation named (`-module-cache-path`), when its output says that cache was compiled at another
 * path, else null. A copied app folder or a moved checkout carries caches whose precompiled headers record where they
 * were made, and every compile that reads one fails (clang: "PCH was compiled with module cache path '…', but the
 * path is currently '…'"). The path is the invocation's own argument, never one read from the message
 * (LLP 1156.006.000 D14). */
export const staleModuleCache = (args, output) => {
  const at = args.indexOf('-module-cache-path');
  return at >= 0 && args[at + 1] && /was compiled with module cache path .*but the path is currently/s.test(output) ? args[at + 1] : null;
};

/** An extension executable's link: application-extension API only, entered at `NSExtensionMain`. A bundle with an
 * `@main` (WidgetKit's) keeps its `main` with `keepMain`. */
export const appexArgs = ['-application-extension', '-Xlinker', '-e', '-Xlinker', '_NSExtensionMain'];
export const keepMain = ['-Xlinker', '-u', '-Xlinker', '_main'];

/** The kinds of app extension `--extensions` chooses among: what an iOS build packages from app.json beside the app,
 * the watch app included (a device build's, and `--watchos`'s). Widgets, Live Activities and controls share one appex;
 * an alarm's countdown is drawn by it too. The bake reads the choice (`EXACT_APPLE_EXTENSIONS`): a build without
 * `controls` omits every control (`bake/src/controls.rs`). */
export const EXTENSION_KINDS = ['widgets', 'activities', 'controls', 'intents', 'alarms', 'notifications', 'watch', 'share'];
/** The kinds `--extensions <all|none|kind,…>` keeps; every kind without the flag. An unknown kind is refused, so a
 * typo never builds more (or less) than was asked. */
export function extensionKinds(args) {
  const at = args.indexOf('--extensions');
  if (at < 0) return new Set(EXTENSION_KINDS);
  const value = args[at + 1];
  if (value === undefined || value.startsWith('--')) throw new Error(`--extensions takes all, none or a comma-separated list of ${EXTENSION_KINDS.join(', ')}`);
  if (value === 'all') return new Set(EXTENSION_KINDS);
  if (value === 'none') return new Set();
  const kinds = value.split(',').map(kind => kind.trim()).filter(Boolean), unknown = kinds.filter(kind => !EXTENSION_KINDS.includes(kind));
  if (unknown.length || !kinds.length) throw new Error(`--extensions: unknown kind ${unknown.join(', ') || '(empty)'}; it takes all, none or a comma-separated list of ${EXTENSION_KINDS.join(', ')}`);
  // A control's press runs the app's copy of its intent, which the app's intents module holds.
  if (kinds.includes('controls') && !kinds.includes('intents')) throw new Error('--extensions: controls need intents, which compiles the app\'s copy of each control\'s intent');
  return new Set(kinds);
}
/** The manifest an `--extensions` build packages from: the entries of a kind it leaves out are dropped, so nothing gated
 * on them (the widget extension, the shared framework, the App Group, `ExactWidgetKinds`, the ActivityKit arm,
 * `NSSupportsLiveActivities`) is built. The app's own code is unchanged; at runtime its calls find no extension and refuse. */
export function withoutExtensions(manifest, kinds) {
  const dropped = { ...manifest };
  if (!kinds.has('widgets') && manifest.widgets) dropped.widgets = [];
  if (!kinds.has('activities') && manifest.live_activities) dropped.live_activities = [];
  if (!kinds.has('share')) delete dropped.share_target;
  return dropped;
}

export const shareId = app => `${app.id}.share`;
// @ref LLP 1133.004 §9 P6/P8/P9, §11 — one app-authored service extension for visible alerts, with no Exact runtime.
export const notificationServiceId = app => `${app.id}.notifications`;

/** The reserved iOS-only folder remains below the shared Apple module root, so its Swift never enters that dylib. */
export const notificationServiceDirectory = app => resolve(app.dir, notificationServiceSourceRoot);
export const notificationServicePlatformRefusal = (service, platform) => service && platform !== 'ios'
  ? `host/apple: ${service.directory} is an iOS and iPadOS notification service extension; ${platform} does not package it` : null;

/** The one convention-defined target. The bake records its sources; discovery
 * here establishes only whether the fixed folder exists. */
export function notificationService(app) {
  const directory = notificationServiceDirectory(app);
  if (!notificationServiceDirectoryExists(app.dir)) return null;
  return { directory, id: notificationServiceId(app), name: 'ExactNotificationService', module: 'ExactNotificationService',
    principalClass: 'ExactNotificationService.NotificationService' };
}

function notificationServiceSources(app, service, files) {
  const sources = files.filter(file => file.path.endsWith('.swift'));
  const declarations = sources.flatMap(file => {
    const source = readFileSync(file.absolute, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
    return [...source.matchAll(/\bclass\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(?:UserNotifications\.)?UNNotificationServiceExtension\b/g)]
      .map(match => ({ name: match[1], file: file.path }));
  });
  if (declarations.length !== 1 || declarations[0].name !== 'NotificationService') {
    const found = declarations.length ? declarations.map(({ name, file }) => `${name} in ${file}`).join(', ') : 'none';
    throw new Error(`${relative(app.dir, service.directory)} must declare exactly one UNNotificationServiceExtension subclass named NotificationService; found ${found}`);
  }
  return sources.map(file => file.absolute);
}

/** A service extension changes a visible push; it is not a second way to acquire push or notification permission. */
export function assertNotificationServiceGrants(service, grantCeiling) {
  if (!service) return;
  const grants = String(grantCeiling ?? '').split('\n').map(line => line.trim());
  if (!grants.includes('device.push')) throw new Error(`${service.directory}: a notification service extension requires the device.push grant and its companion device.notifications <purpose-key> grant`);
  if (!grants.some(line => line.startsWith('device.notifications '))) throw new Error('device.push and a notification service extension require the companion device.notifications <purpose-key> grant');
}

/** Only the containing app's groups cross into the extension. Push, browser, HealthKit and every other app entitlement
 * stay on their owning executable. */
export function notificationServiceEntitlements(service, appEntitlements) {
  const application = String(appEntitlements['application-identifier'] ?? appEntitlements['com.apple.application-identifier'] ?? '');
  const prefix = application.endsWith(service.id.slice(0, -'.notifications'.length))
    ? application.slice(0, -service.id.slice(0, -'.notifications'.length).length) : '';
  return {
    'application-identifier': `${prefix}${service.id}`,
    ...(appEntitlements['com.apple.developer.team-identifier'] ? { 'com.apple.developer.team-identifier': appEntitlements['com.apple.developer.team-identifier'] } : {}),
    'get-task-allow': !!appEntitlements['get-task-allow'],
    ...(appEntitlements['com.apple.security.application-groups'] ? { 'com.apple.security.application-groups': appEntitlements['com.apple.security.application-groups'] } : {}),
    ...(appEntitlements['keychain-access-groups'] ? { 'keychain-access-groups': appEntitlements['keychain-access-groups'] } : {}),
  };
}

/** The embedded executable takes an explicit App ID profile of the app's team, prefix and method. When it shares a
 * group, the profile must authorize that exact value; a wildcard profile is never accepted for this target. */
export function notificationServiceProfile({ service, appProfile, udid, entitlements, installed }) {
  const groups = entitlements['com.apple.security.application-groups'] ?? [], keychain = entitlements['keychain-access-groups'] ?? [];
  const candidates = installed.filter(profile => profile.appId === `${profile.prefix}.${service.id}`
    && groups.every(group => profile.groups.includes(group))
    && keychain.every(group => profile.keychain.includes(group) || profile.keychain.includes(`${profile.prefix}.*`)));
  try {
    return embeddedProfile(udid, service.id, appProfile,
      [...(groups.length ? ['com.apple.security.application-groups'] : []), ...(keychain.length ? ['keychain-access-groups'] : [])], candidates);
  } catch (error) {
    throw new Error(`host/apple: the notification service extension needs a profile for the App ID ${service.id}${groups.length ? ` with the App Group ${groups.join(', ')}` : ''}${keychain.length ? ` and shared Keychain group ${keychain.join(', ')}` : ''}. Register that App ID and install a matching profile. ${error.message}`);
  }
}

/** The app's Info.plist keys for the claim (`ShareReceive.swift`): its `share_target`, and the group its inbox
 * is in when the extension was built (no group, no inbox). */
export const shareInfo = (app, group = null) => app.manifest.share_target
  ? { ExactShareTarget: JSON.stringify(app.manifest.share_target), ExactAppIdentifier: app.id, ...(group ? { ExactShareGroup: group } : {}) }
  : {};

/** The share extension (D4 and D6, amended 2026-11-15) in `bundle`'s `PlugIns/ExactShare.appex`: the host's one Swift
 * extension, parameterized by its Info.plist (the group, the app's id, the mapped members, each files member's UTIs,
 * the activation rule and the post button's word). It holds the one presentation the app declares: that
 * presentation's source is compiled and its controller is the principal class. For an app that maps `url` it also
 * holds Exact's page script, which Safari runs in a shared page, and the code that reads its answer. `entitlements`
 * is the extension's plist text; `sign(extension, path)` signs it. */
export function packageShareExtension({ app, bundle, directory, sdk, triple, device = false, platform = 'ios', group, entitlements, sign, runApple, production, symbols = directory, profile = null }) {
  const mac = platform === 'macos', contents = mac ? resolve(bundle, 'Contents') : bundle;
  const target = app.manifest.share_target;
  const extension = resolve(contents, 'PlugIns/ExactShare.appex');
  rmSync(extension, { recursive: true, force: true });
  const extensionContents = mac ? resolve(extension, 'Contents') : extension;
  const binary = mac ? resolve(extensionContents, 'MacOS/ExactShare') : resolve(extension, 'ExactShare');
  mkdirSync(mac ? resolve(extensionContents, 'MacOS') : extension, { recursive: true });
  if (profile) copyFileSync(profile.path, resolve(extensionContents, mac ? 'embedded.provisionprofile' : 'embedded.mobileprovision'));
  const appInfo = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(contents, 'Info.plist')], { read: true }).stdout);
  const versions = Object.fromEntries(['CFBundleVersion', 'CFBundleShortVersionString', mac ? 'LSMinimumSystemVersion' : 'MinimumOSVersion'].map(key => [key, appInfo[key]]).filter(([, v]) => v !== undefined));
  const params = target.params ?? {}, page = appleSharePage(target);
  const [principalClass, presentationSource] = applePresentation(target) === 'confirmation'
    ? ['ExactShareConfirmationViewController', 'ShareConfirmation.swift'] : ['ExactShareViewController', 'ShareViewController.swift'];
  writeFileSync(resolve(extensionContents, 'Info.plist'), plistFile({
    CFBundleIdentifier: shareId(app), CFBundleName: 'ExactShare', CFBundleDisplayName: appInfo.CFBundleDisplayName ?? appInfo.CFBundleName ?? app.displayName,
    CFBundleExecutable: 'ExactShare', CFBundlePackageType: 'XPC!', CFBundleInfoDictionaryVersion: '6.0', ...versions,
    ...(!mac ? { UIDeviceFamily: appInfo.UIDeviceFamily } : {}), CFBundleSupportedPlatforms: mac ? ['MacOSX'] : appInfo.CFBundleSupportedPlatforms,
    ExactAppGroup: group, ExactAppIdentifier: app.id, ExactShareParams: ['title', 'text', 'url'].filter(k => params[k]), ExactShareMembers: appleShareMembers(target),
    ...(target.apple?.postTitle ? { ExactSharePostTitle: target.apple.postTitle } : {}),
    NSExtension: { NSExtensionPointIdentifier: 'com.apple.share-services', NSExtensionPrincipalClass: principalClass,
      // The script's name, without `.js`, as Safari looks it up among the extension's resources.
      NSExtensionAttributes: { NSExtensionActivationRule: appleActivationRule(target), ...(page ? { NSExtensionJavaScriptPreprocessingFile: 'SharePage' } : {}) } },
  }));
  if (page) {
    const resources = mac ? resolve(extensionContents, 'Resources') : extension;
    mkdirSync(resources, { recursive: true });
    copyFileSync(resolve(root, 'host/apple/share/SharePage.js'), resolve(resources, 'SharePage.js'));
  }
  mkdirSync(directory, { recursive: true });
  const entitlementPath = resolve(directory, 'share-entitlements.plist');
  writeFileSync(entitlementPath, entitlements);
  runApple('xcrun', ['--sdk', mac ? 'macosx' : device ? 'iphoneos' : 'iphonesimulator', 'swiftc', '-parse-as-library', swiftOptimization(), ...(production ? ['-g'] : []), ...appexArgs,
    ...(page ? ['-D', 'EXACT_SHARE_PAGE'] : []), '-module-name', 'ExactShare', '-target', triple, '-sdk', sdk, '-module-cache-path', extensionModuleCache,
    ...(!device && !mac ? ['-Xlinker', '-sectcreate', '-Xlinker', '__TEXT', '-Xlinker', '__entitlements', '-Xlinker', entitlementPath] : []),
    resolve(root, 'host/apple/share/ShareInbox.swift'), resolve(root, 'host/apple/share', presentationSource), '-o', binary]);
  stripProductionExecutable(binary, { production, dsym: resolve(symbols, 'ExactShare.appex.dSYM'), sourceDsym: `${binary}.dSYM` });
  writePrivacyManifest(mac ? resolve(extensionContents, 'Resources') : extension, [binary], { mac });
  sign(extension, entitlementPath);
  return extension;
}

/** Apple's service extension changes a visible alert whose `aps` has `mutable-content: 1`; when it does not complete
 * in time, the system shows the original. It is a separate executable with no Exact runtime.
 * https://developer.apple.com/documentation/usernotifications/modifying-content-in-newly-delivered-notifications */
export function packageNotificationService({ service, identity, app, bundle, directory, sdk, triple, device = false, entitlements, sign, runApple, production, symbols = directory, profile = null }) {
  if (!notificationService(app) || !identity) throw Object.assign(new Error('the notification service sources changed after the bake recorded their identity; build again'), { rebuild: true });
  let snapshotDirectory;
  try {
    mkdirSync(directory, { recursive: true });
    snapshotDirectory = mkdtempSync(resolve(directory, '.notification-service-'));
    const snapshot = snapshotNotificationService(service.directory, identity, snapshotDirectory);
    const sources = notificationServiceSources(app, service, snapshot.files);
    const capturedPrivacy = snapshot.files.find(file => file.path === 'PrivacyInfo.xcprivacy');
    const extension = resolve(bundle, 'PlugIns/ExactNotificationService.appex');
    rmSync(extension, { recursive: true, force: true });
    mkdirSync(extension, { recursive: true });
    if (profile) copyFileSync(profile.path, resolve(extension, 'embedded.mobileprovision'));
    const appInfo = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(bundle, 'Info.plist')], { read: true }).stdout);
    const versions = Object.fromEntries(['CFBundleVersion', 'CFBundleShortVersionString', 'MinimumOSVersion'].map(key => [key, appInfo[key]]).filter(([, value]) => value !== undefined));
    writeFileSync(resolve(extension, 'Info.plist'), plistFile({
      CFBundleIdentifier: service.id, CFBundleName: service.name, CFBundleDisplayName: appInfo.CFBundleDisplayName ?? appInfo.CFBundleName ?? app.displayName,
      CFBundleExecutable: service.name, CFBundlePackageType: 'XPC!', CFBundleInfoDictionaryVersion: '6.0', ...versions,
      UIDeviceFamily: appInfo.UIDeviceFamily, CFBundleSupportedPlatforms: appInfo.CFBundleSupportedPlatforms,
      ...(appInfo.DTPlatformName ? { DTPlatformName: appInfo.DTPlatformName } : {}),
      NSExtension: { NSExtensionPointIdentifier: 'com.apple.usernotifications.service', NSExtensionPrincipalClass: service.principalClass },
    }));
    const entitlementPath = resolve(directory, 'notification-service-entitlements.plist'), binary = resolve(extension, service.name);
    writeFileSync(entitlementPath, plistFile(entitlements));
    runApple('xcrun', ['--sdk', device ? 'iphoneos' : 'iphonesimulator', 'swiftc', '-parse-as-library', swiftOptimization(), ...(production ? ['-g'] : []), ...appexArgs,
      '-module-name', service.module, '-target', triple, '-sdk', sdk, '-module-cache-path', extensionModuleCache,
      ...(!device ? ['-Xlinker', '-sectcreate', '-Xlinker', '__TEXT', '-Xlinker', '__entitlements', '-Xlinker', entitlementPath] : []),
      ...sources, '-o', binary]);
    stripProductionExecutable(binary, { production, dsym: resolve(symbols, 'ExactNotificationService.appex.dSYM'), sourceDsym: `${binary}.dSYM` });
    writePrivacyManifest(extension, [binary], { manifests: capturedPrivacy ? [capturePrivacyManifest(capturedPrivacy.absolute)] : [], mode: 'app-owned' });
    sign(extension, entitlementPath);
    return extension;
  } finally { if (snapshotDirectory) rmSync(snapshotDirectory, { recursive: true, force: true }); }
}

/** The Mac extension's entitlements: sandboxed, as every Mac app extension is, with the app's group, and its App ID
 * and team under the App Store profile a store build embeds (`profile`, for the extension's `id`; LLP 1030.005 D1). */
export const macShareEntitlements = (group, profile = null, id = null) => plistFile({ 'com.apple.security.app-sandbox': true, 'com.apple.security.application-groups': [group],
  ...(profile ? { 'com.apple.application-identifier': `${profile.prefix}.${id}`, 'com.apple.developer.team-identifier': profile.team } : {}) });
