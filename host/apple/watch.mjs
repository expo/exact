import { writePrivacyManifest } from './privacy.mjs';
// @ref LLP 1126.001 WP17 — watch complications use the snapshot driver and their own store.
import { resolve, relative } from 'node:path';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, writeFileSync, copyFileSync, cpSync, rmSync } from 'node:fs';
import { buildBake, bakeOutput, developmentBuildEnv, HOST_DEV, appleExecutableName } from '../../scripts/app.mjs';
import { appleAssets, plistFile } from './assets.mjs';
import { unclaimedNote } from '../../scripts/links.mjs';
import { WATCH_MINIMUM_OS } from './toolchain.mjs';
import { appleProduction, stripProductionExecutable } from './link.mjs';
import { reportBuilt } from '../../scripts/help.mjs';
import { copyStaticTreeIfPresent } from '../web/serve.mjs';
import { ensureWatchSDK, distributionToolchain } from './sdk.mjs';
import { platformWidgetEntries, prepareWidgetCore, sharedWidgetCore, placeCoreFramework, packageWidgets, widgetId } from './widgets.mjs';
import { installedProfiles, profilesFor, iosProfile, byExpiry, profileMisfit, METHODS } from './devices.mjs';
import { extensionKinds } from './extensions.mjs';
import { fileURLToPath } from 'node:url';
const root = resolve(fileURLToPath(new URL('../..', import.meta.url))), pkg = resolve(root, 'host/apple');

export function watchEntries(app, say = console.log) {
  return platformWidgetEntries(app, 'watchos').filter(entry => {
    if (!existsSync(resolve(app.dir, 'app.ts'))) return true;
    say(`host/apple: watchOS omits entry ${entry.kind}, including any Rust refresh in this TypeScript app, until LLP 1126.001 §5 Q3 passes`);
    return false;
  });
}
/** Whether the app has a watch app to build: its live `watch` entry (LLP 1126.002 D4), or complications. */
export const watchSyncSelected = app => ['state', 'secrets'].some(k => app.manifest.watch?.sync?.[k]?.length > 0);
export function watchSyncKeys(app, grants = '') {
  return watchSyncSelected(app) ? { ExactWatchSync: { app: app.id, grants,
    policy: { state: app.manifest.watch.sync.state ?? [], secrets: app.manifest.watch.sync.secrets ?? [] } } } : {};
}
export const watchCompanion = (app, say = () => {}) => !!app.manifest.watch || watchEntries(app, say).length > 0;
/** The watch app's Rust target, Swift triple, SDK and bundle platform: arm64 alone, from watchOS 26 (LLP 1126.002 D7). */
export const watchDestination = device => device
  ? { target: 'aarch64-apple-watchos', triple: `arm64-apple-watchos${WATCH_MINIMUM_OS}`, sdkName: 'watchos', platform: 'WatchOS' }
  : { target: 'aarch64-apple-watchos-sim', triple: `arm64-apple-watchos${WATCH_MINIMUM_OS}-simulator`, sdkName: 'watchsimulator', platform: 'WatchSimulator' };
/** The companion's own Info.plist, never the iOS app's, but for the iPhone app's other names, which Siri requires to
 * be the same on the watch and the watch does not inherit. A device bundle declares arm64, which leaves out the
 * arm64_32 watches watchOS 26 still supports (D7); `toolchain` is a distributed build's DT keys. */
export function watchInfo(app, { appInfo, group, entries, live, generation, icon = {}, device = false, toolchain = {}, keychain }) {
  const { platform, sdkName } = watchDestination(device);
  return { CFBundleIdentifier: `${app.id}.watchkitapp`, CFBundleName: app.displayName, CFBundleDisplayName: app.displayName, CFBundleExecutable: 'ExactWatch', CFBundlePackageType: 'APPL',
    CFBundleVersion: appInfo.CFBundleVersion, CFBundleShortVersionString: appInfo.CFBundleShortVersionString, CFBundleSupportedPlatforms: [platform], DTPlatformName: sdkName, MinimumOSVersion: WATCH_MINIMUM_OS, UIDeviceFamily: [4],
    ...(device ? { UIRequiredDeviceCapabilities: ['arm64'] } : {}), ...toolchain,
    WKApplication: true, WKCompanionAppBundleIdentifier: app.id, WKRunsIndependentlyOfCompanionApp: true,
    ExactAppGroup: group, ExactAppIdentifier: app.id, ExactWidgetTypeScript: false, ExactWidgetGeneration: generation,
    ExactWatchEntries: entries.map(({ kind, families, title }) => ({ kind, families, title: title ?? kind })),
    ...watchSyncKeys(app, appInfo.ExactWatchSync?.grants),
    ...(keychain ? { ExactKeychainAccessGroup: keychain } : {}),
    ...(live ? { ExactWatchPlan: 'plans/watch.plan' } : {}), ...icon,
    ...(appInfo.INAlternativeAppNames ? { INAlternativeAppNames: appInfo.INAlternativeAppNames } : {}) };
}
/** The profiles a signed device build embeds in the watch app and, with complications, in its widget extension:
 * each for its own App ID, granting the watch's App Group, of the iPhone app's profile's (`like`) team, prefix and
 * method, naming the phone or, for an archive, the app's devices; the longest-lived. Or a refusal naming what to
 * register. Empty for an app with no watch app. `installed` is the decoded profiles, which a test replaces. */
export function watchProfiles({ app, udid, like, installed = installedProfiles() }) {
  if (!watchCompanion(app)) return [];
  const watch = `${app.id}.watchkitapp`, group = `group.${watch}`;
  const sharesSecrets = app.manifest.watch?.sync?.secrets?.length > 0;
  const ids = [watch, ...(watchEntries(app, () => {}).length ? [widgetId({ id: watch })] : [])];
  return ids.map(id => {
    const candidates = profilesFor(id, installed).filter(iosProfile).sort(byExpiry);
    const why = candidates.map(p => [p, profileMisfit(p, udid, like) ?? (!p.groups.includes(group) ? `does not grant the App Group ${group}` : (!sharesSecrets || (p.keychain ?? []).some(k => k === `${p.prefix}.*` || k === `${p.prefix}.${watch}`)) ? null : `does not grant the Keychain group ${p.prefix}.${watch}`)]);
    const found = why.find(([, reason]) => !reason)?.[0];
    if (found) return found;
    throw new Error(`host/apple: the watch app needs a ${METHODS[like.method]} profile for ${id} as the iPhone app's (${like.name}) is, granting the App Group ${group}: ${why.length ? why.map(([p, reason]) => `"${p.name}" ${reason}`).join('; ') : 'none on this Mac is for it'}. In the Apple Developer portal, register the App ID ${id} with App Groups (${group}) and Keychain Sharing (${like.prefix}.${watch}), make a ${METHODS[like.method]} profile for it of team ${like.team}${udid ? ' that includes this phone' : ''}, and install it; or leave the watch app out with --extensions.`);
  });
}
export function watchEntitlements({ id, group, keychain, profile, device = false }) {
  return {
    ...(device ? { 'application-identifier': profile ? `${profile.prefix}.${id}` : id, ...(profile ? { 'com.apple.developer.team-identifier': profile.team } : {}), 'get-task-allow': profile?.dev ?? false } : {}),
    'com.apple.security.application-groups': [group], ...(keychain ? { 'keychain-access-groups': [keychain] } : {}) };
}
export function watchSimulator(args) {
  const at = args.indexOf('--watch-sim');
  if (at < 0) return undefined;
  const id = args[at + 1];
  if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(id ?? '')) throw new Error('--watch-sim requires a UDID (names are not accepted)');
  return id;
}
export function packageWatch({ bin, bundle, production, symbols, sign, entitled, privacyManifests = [], ...widgets }) {
  const executable = resolve(bundle, 'ExactWatch');
  copyFileSync(resolve(bin, 'ExactWatch'), executable);
  stripProductionExecutable(executable, { production, dsym: resolve(symbols, 'ExactWatch.app.dSYM') });
  if (widgets.entries.length) packageWidgets({ ...widgets, bundle, production, symbols, sign, platform: 'watchos' });
  const core = resolve(bundle, 'Frameworks/ExactCore.framework');
  stripProductionExecutable(resolve(core, 'ExactCore'), { production, dsym: resolve(symbols, 'ExactCore.framework.dSYM') });
  writePrivacyManifest(core, [resolve(core, 'ExactCore')], { apiManifests: privacyManifests });
  writePrivacyManifest(bundle, [executable], { apiManifests: privacyManifests });
  sign(core);
  sign(bundle, entitled);
}
/** The watch app, compiled, signed and placed in the iPhone app at `parent`'s `Watch/`, which the caller signs
 * after it. On a device, `signing` is the identity that signs the iPhone app and the profiles `watchProfiles` chose;
 * without it the watch app is ad-hoc signed, for a re-signer, as an `--unsigned` iPhone app is. The watch app
 * boots the plan it carries and never opens an update store, so it runs the generation it was built with (D10).
 * Returns its path, or null when the app has no watch app. */
export function embedWatch(app, { parent, namespace, device = false, production, symbols, signing = null, runApple, receipt }) {
  const entries = watchEntries(app);
  const watch = { ...app, id: `${app.id}.watchkitapp` };
  const bundle = resolve(parent, 'Watch', `${appleExecutableName(app)}.app`);
  rmSync(resolve(parent, 'Watch'), { recursive: true, force: true });
  // LLP 1126.002 D4: the bake refuses a live entry that reaches TypeScript.
  const live = !!app.manifest.watch;
  if (!entries.length && !live) {
    console.log('host/apple: no watch entry and no Rust accessory entries; watch bundle omitted');
    return null;
  }
  const unclaimed = unclaimedNote(app.manifest, 'watchos');
  if (unclaimed) console.log(`host/apple: ${unclaimed}`);
  const { target, triple, sdkName } = watchDestination(device);
  if (process.platform === 'linux') ensureWatchSDK(sdkName);
  const profile = production ? 'release' : HOST_DEV;
  const directory = resolve(namespace, 'watchos');
  mkdirSync(directory, { recursive: true });
  const sdk = runApple('xcrun', ['--sdk', sdkName, '--show-sdk-path'], { read: true }).stdout.trim();
  const env = { ...developmentBuildEnv(), EXACT_UPDATE_TRUST: production ? 'production' : 'development', WATCHOS_DEPLOYMENT_TARGET: WATCH_MINIMUM_OS, EXACT_APPLE_LINK: 'plan' };
  const baked = buildBake({ ...app, hasGpu: false }, 'ios', target, { env, profile });
  const output = resolve(bakeOutput(app, env), `ios-${target}.entries`);
  const libDir = resolve(app.target, target, profile), crate = app.crate('ios');
  const swiftRoot = resolve(directory, 'swift');
  const swiftArgs = ['build', '-c', production ? 'release' : 'debug', '--scratch-path', swiftRoot, ...(!production ? ['-Xswiftc', '-O'] : []),
    '--triple', triple, '--sdk', sdk, '-Xcc', '-isysroot', '-Xcc', sdk, '-Xlinker', '-syslibroot', '-Xlinker', sdk,
    '-Xswiftc', '-Xclang-linker', '-Xswiftc', '-isysroot', '-Xswiftc', '-Xclang-linker', '-Xswiftc', sdk];
  const swiftEnv = { ...process.env, EXACT_WATCH_SYNC: watchSyncSelected(app) ? '1' : '0', WATCHOS_DEPLOYMENT_TARGET: WATCH_MINIMUM_OS, EXACT_LIB_DIR: libDir, EXACT_LIB: crate.replaceAll('-', '_'), EXACT_WIDGET_GALLERY: '0', EXACT_UPDATE_TRUST: production ? 'production' : 'development' };
  delete swiftEnv.SDKROOT;
  const swiftc = runApple('xcrun', ['swiftc', '--version'], { read: true }).stdout;
  const prepared = prepareWidgetCore({ directory: resolve(directory, 'core'), swiftBuildRoot: swiftRoot, swiftArgs, triple, platform: 'watchos', device, swiftc });
  const shared = sharedWidgetCore({ app: watch, prepared, libDir, crate, env: swiftEnv, runApple });
  // A simulator reads entitlements from each link's __TEXT; a device from the signature, under the profile's App ID.
  const group = `group.${watch.id}`, [appProfile, extensionProfile] = signing?.profiles ?? [];
  const keychain = app.manifest.watch?.sync?.secrets?.length > 0
    ? (appProfile ? `${appProfile.prefix}.${watch.id}` : watch.id) : undefined;
  const entitlements = (id, profile) => plistFile(watchEntitlements({ id, group, keychain, profile, device }));
  const entitlementPath = resolve(directory, 'watch-entitlements.plist');
  writeFileSync(entitlementPath, entitlements(watch.id, appProfile));
  const sign = (path, entitled) => runApple('codesign', ['--force', '--sign', signing?.identity ?? '-', '--timestamp=none', ...(device && entitled ? ['--entitlements', entitled] : []), path]);
  mkdirSync(resolve(bundle, 'Frameworks'), { recursive: true });
  placeCoreFramework(shared.framework, resolve(bundle, 'Frameworks/ExactCore.framework'), watch, 'watchos');
  mkdirSync(resolve(bundle, 'plans'), { recursive: true });
  for (const entry of entries) copyFileSync(resolve(output, `plans/${entry.kind}.plan`), resolve(bundle, `plans/${entry.kind}.plan`));
  if (live) copyFileSync(resolve(output, 'plans/watch.plan'), resolve(bundle, 'plans/watch.plan'));
  for (const name of ['assets', 'deck']) copyStaticTreeIfPresent(resolve(app.dir, name), resolve(bundle, name));
  const appInfo = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(parent, 'Info.plist')], { read: true }).stdout);
  const icon = appleAssets(app, bundle, { platform: 'watchos', device, kept: { dir: resolve(directory, 'assets'), stamp: swiftc } });
  const info = watchInfo(app, { appInfo, group, entries, live, generation: createHash('sha256').update(JSON.stringify([baked.compat.id, baked.graph.artifacts])).digest('hex'), icon,
    device, keychain, toolchain: production && device ? distributionToolchain(sdkName) : {} });
  writeFileSync(resolve(bundle, 'Info.plist'), plistFile(info));
  if (appProfile) copyFileSync(appProfile.path, resolve(bundle, 'embedded.mobileprovision'));
  const executableArgs = [...swiftArgs, '-Xlinker', '-rpath', '-Xlinker', '@executable_path/Frameworks',
    ...(device ? [] : ['-Xlinker', '-sectcreate', '-Xlinker', '__TEXT', '-Xlinker', '__entitlements', '-Xlinker', entitlementPath])];
  const options = { cwd: pkg, env: { ...swiftEnv, EXACT_CORE_FRAMEWORK: relative(pkg, shared.xcframework) } };
  runApple('swift', [...executableArgs, '--product', 'ExactWatch'], options);
  const bin = runApple('swift', [...executableArgs, '--show-bin-path'], { ...options, read: true }).stdout.trim();
  packageWatch({ bin, bundle, app: watch, shared, directory, sdk, triple, device, entitlements: entitlements(widgetId(watch), extensionProfile), profile: extensionProfile ?? null, entitled: entitlementPath,
    sign, runApple, production, entries, privacyManifests: (baked.binary.privacyManifests ?? []).slice(0, 1), symbols });
  writeFileSync(resolve(directory, 'receipt.json'), receipt(app, { platform: device ? 'watchos' : 'watchos-simulator', target, sdk, bundle, compatibilityId: baked.compat.id, build: baked,
    identity: signing?.identity ?? 'ad-hoc', profile: appProfile ? { name: appProfile.name, team: appProfile.team, expires: appProfile.expires } : null }));
  console.log(`host/apple: ${bundle}; watch group ${group}; ${entries.map(e => `${e.kind} [${e.families.join(', ')}]`).join('; ')}`);
  return bundle;
}
/** `--watchos`: the iPhone app for the simulator with its watch app inside, installed on a simulator pair. A device
 * build of the iPhone app (`--device`, `--archive`) embeds the watch app itself (`embedWatch`). */
export async function buildWatch(app, args, { runApple, appleArtifacts, appleBuildLock, receipt, buildParent }) {
  if (args.some(a => ['--device', '--archive'].includes(a))) throw new Error('--watchos builds the simulator companion; a --device or --archive build of the iPhone app embeds its watch app, so leave out --watchos');
  if (args.some(a => ['--tvos', '--host', '--embed', '--test', '--url'].includes(a))) throw new Error('--watchos builds the watch simulator companion; use --sim for its iPhone and --watch-sim for its paired watch');
  if (args.includes('--run') && args.includes('--no-install')) throw new Error('--no-install takes no --run');
  if (!extensionKinds(args).has('watch')) throw new Error('--watchos builds the watch app, which --extensions leaves out');
  const watchSim = watchSimulator(args);
  const release = appleBuildLock(app);
  try {
    const parentArgs = args.filter((a, i) => !['--watchos', '--run', '--no-install', '--watch-sim'].includes(a) && args[i - 1] !== '--watch-sim');
    await buildParent([...parentArgs, '--ios', '--no-install']);
    if (process.exitCode) throw new Error(`iOS companion build refused (exit ${process.exitCode}); watch build stopped`);
    const production = args.includes('--release') || process.env.EXACT_UPDATE_TRUST === 'production';
    const { optimize } = appleProduction(app, 'ios', args);
    const parent = appleArtifacts(app, { destination: 'ios-simulator', trust: production ? 'production' : 'development', optimize: production ? optimize : 'speed' });
    const bundle = embedWatch(app, { parent: parent.bundle, namespace: parent.namespace, production, symbols: resolve(`${parent.binary}.dSYMs`, 'Watch'), runApple, receipt });
    if (!bundle) { if (args.includes('--run')) console.log('host/apple: --run skipped'); return; }
    // Simulator entitlements are embedded in __TEXT by each link.
    runApple('codesign', ['--force', '--sign', '-', parent.bundle]);
    if (args.includes('--no-install')) return;
    const pick = flag => args.includes(flag) ? args[args.indexOf(flag) + 1] : undefined;
    const pairs = JSON.parse(runApple('xcrun', ['simctl', 'list', 'pairs', '-j'], { read: true }).stdout).pairs;
    const devices = Object.values(pairs).filter(pair => pair.watch && pair.phone);
    const matches = devices.filter(pair => (!watchSim || pair.watch.udid.toLowerCase() === watchSim.toLowerCase()) && (!pick('--sim') || [pair.phone.udid, pair.phone.name].includes(pick('--sim'))));
    if (matches.length !== 1) throw new Error('name one existing simulator pair with --sim <iPhone udid> --watch-sim <watch udid>; this build creates and shuts down no simulators');
    const pair = matches[0];
    for (const device of [pair.phone, pair.watch]) {
      if (device.state !== 'Booted') throw new Error(`boot your simulator first: xcrun simctl boot ${device.udid}`);
    }
    runApple('xcrun', ['simctl', 'install', pair.phone.udid, parent.bundle]);
    runApple('xcrun', ['simctl', 'install', pair.watch.udid, bundle]);
    // The launcher prints the last object: this one, naming the phone both bundles are installed for.
    if (args.includes('--json')) reportBuilt({ path: parent.bundle, target: 'ios-simulator', trust: process.env.EXACT_UPDATE_TRUST ?? 'development', signing: 'ad-hoc', installed: { name: pair.phone.name, udid: pair.phone.udid } });
    if (args.includes('--run')) runApple('xcrun', ['simctl', 'launch', pair.watch.udid, `${app.id}.watchkitapp`]);
  } finally { release(); }
}
