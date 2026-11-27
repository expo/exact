// @ref LLP 1031.000 — Complete iOS `--embed` packaging bundle writer.
// Emits App Store-ready wrapped `.framework` bundles, deterministic zero-env-var
// `ExactEmbedComposition.swift`, `PrivacyInfo.xcprivacy`, consumer-owned widget and
// App Intents sources, resources, and backwards-compatible `capabilities.json` metadata.
import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyBakeFiles } from '../../scripts/app.mjs';
import { embedLibrary, placeLibrary } from '../../scripts/package-libraries.mjs';
import { listAssets } from '../web/serve.mjs';
import { packageActivityArm } from './activities.mjs';
import { assertArchiveFrameworks, bakedAssetInputs, copyNativeResources, iosAssets, wrapFramework } from './assets.mjs';
import { controlWidgetsSource, offeredControls } from './controls.mjs';
import { appleFaces, intentsSource, pressesTools } from './intents.mjs';
import { alarmKeys, backgroundKeys, declaredOrigin, nfcKeys, startLocation, storeKeys, updateActivation } from './plist.mjs';
import { associatedDomains, entitlementValue, grantEntitlements, grantEntitlementValues, merchantEntitlement, signInEntitlement } from './presence.mjs';
import { writeEmbedPrivacyManifest, writePrivacyManifest } from './privacy.mjs';
import { activityEntries, copyWidgetFonts, hasEntries, platformWidgetEntries, widgetAppKeys, widgetBundleSource, widgetGroup, widgetReload, widgetSwiftSources } from './widgets.mjs';

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const pkg = resolve(root, 'host/apple');

const LINK_TO_ARM = { web_views: 'web', video: 'video', sound: 'sound', midi: 'midi', svg: 'svg', surfaces: 'canvas' };
const KNOWN_ARMS = new Set(['web', 'video', 'sound', 'midi', 'svg', 'canvas']);
const ALARM_SOURCES = [resolve(pkg, 'widget/Alarm.swift'), resolve(pkg, 'widget/AlarmAttributes.swift')];

/** Host arms required for an iOS `--embed` build, driven strictly by `buildReceipt.graph.loads`
 * plus any explicitly configured `app.manifest.link` arms when updating (`L=A`) declares them
 * (LLP 1031.000 D1.2). Never falls back to compiling all arms for a bare app. */
export function embedArmReach({ reaches, app, level }) {
  const needed = new Set();
  if (Array.isArray(reaches)) {
    for (const mod of reaches) if (KNOWN_ARMS.has(mod)) needed.add(mod);
  }
  if (level === 'A' && Array.isArray(app?.manifest?.link)) {
    for (const name of app.manifest.link) {
      const arm = LINK_TO_ARM[name] ?? (KNOWN_ARMS.has(name) ? name : null);
      if (arm) needed.add(arm);
    }
  }
  return needed;
}

/** Deterministic, zero-env-var `ExactEmbedComposition.swift` compiled by the brownfield consumer
 * against only the SwiftPM products listed in `capabilities.json` (LLP 1031.000 D2). */
export function generateEmbedComposition({ products = [], composition = 'embedded' } = {}) {
  const capabilityProducts = products.filter(p => p !== 'ExactKit');
  const has = name => capabilityProducts.includes(name);
  const imports = ['import ExactKit', 'import Foundation'];
  if (composition === 'updating') imports.push('import ExactUpdates');
  for (const product of capabilityProducts) {
    if (product === 'ExactDragIn') imports.push('#if os(iOS)\nimport ExactDragIn\n#endif');
    else imports.push(`import ${product}`);
  }
  if (has('ExactNotifications')) imports.push('#if !os(tvOS)\nimport UserNotifications\n#endif');

  const installCalls = ['        AgentLink.install()'];
  for (const product of capabilityProducts) {
    if (product === 'ExactDragIn') {
      installCalls.push('        #if os(iOS)', '        ExactDragIn.install()', '        #endif');
    } else {
      installCalls.push(`        ${product}.install()`);
    }
  }

  const hooks = [];
  if (has('ExactHostEventMailbox')) {
    hooks.push(
      `    public static func installHostEvents(on app: ExactApp, applicationDelegate: AnyClass?, sceneDelegate: AnyClass?, session: @escaping () -> ExactSession? = { nil }) {\n        app.installHostEvents(applicationDelegate: applicationDelegate, sceneDelegate: sceneDelegate, session: session)\n    }`,
      `    public static func hostEvents(on app: ExactApp, connectionOptions: NSObject) {\n        app.hostEvents(connectionOptions: connectionOptions)\n    }`,
    );
  }
  if (has('ExactNotifications')) {
    hooks.push(
      `    #if !os(tvOS)\n    public static func installNotificationClicks(on app: ExactApp, boot: ((ExactSession) -> Void)? = nil, activate: ((ExactSession) -> Void)? = nil) {\n        app.installNotificationClicks(boot: boot, activate: activate)\n    }\n    public static func openNotification(on app: ExactApp, _ response: UNNotificationResponse) {\n        app.openNotification(response)\n    }\n    #endif`,
      `    public static func registeredForRemoteNotifications(on app: ExactApp, deviceToken: Data) {\n        app.registeredForRemoteNotifications(deviceToken: deviceToken)\n    }`,
      `    public static func failedToRegisterForRemoteNotifications(on app: ExactApp, error: Error) {\n        app.failedToRegisterForRemoteNotifications(error: error)\n    }`,
    );
  }
  if (has('ExactWakeCarriers')) {
    hooks.push(
      `    public static func installWakes(on app: ExactApp, boot: (() -> Void)? = nil) {\n        app.installWakes(boot: boot)\n    }`,
      `    public static func handleTransferEvents(on app: ExactApp, _ identifier: String, completion: @escaping () -> Void) {\n        app.handleTransferEvents(identifier, completion: completion)\n    }`,
    );
  }
  if (has('ExactIndexMirror') || has('ExactToolDoor')) {
    const body = ['        app.installReach()'];
    if (has('ExactIndexMirror')) body.push('        app.installContentIndex()');
    if (has('ExactToolDoor')) body.push('        app.installToolDoor(boot: boot)');
    hooks.push(`    public static func installReach(on app: ExactApp, boot: (() -> Void)? = nil) {\n${body.join('\n')}\n    }`);
  }
  if (has('ExactIndexMirror')) {
    hooks.push(`    public static func location(of activity: NSUserActivity) -> String? {\n        ExactContentIndex.location(of: activity)\n    }`);
  }

  return `// Generated by host/apple/build.mjs --embed (LLP 1031.000 D2). Do not edit by hand.
// Add this file to your Xcode target alongside the SwiftPM products in capabilities.json.
${imports.join('\n')}

public enum ExactEmbedComposition {
    public static let app: ExactApp = {
        install()
        let app = ExactApp.shared
${composition === 'updating' ? '        ExactUpdates.install(on: app)\n' : ''}        return app
    }()

    /// Install the baked capabilities' Swift halves in capabilities.json order before creating any session.
    public static func install() {
${installCalls.join('\n')}
    }
${hooks.length ? '\n' + hooks.join('\n\n') + '\n' : ''}}
`;
}

/** Declarative app and widget entitlement dictionaries for a brownfield host's Xcode targets.
 * Uses `$(AppIdentifierPrefix)` for Keychain access groups and explicitly excludes `applinks:`
 * from widget extension entitlements (LLP 1031.000 D1.6, LLP 1156.008.000 D6.2). */
export function embedEntitlements({ app, reach, hasWidgets, ios, claimedDomains = [] }) {
  const appDict = {};
  const widgetDict = {};
  if (!ios) return { app: appDict, widget: widgetDict };
  const group = widgetGroup(app);
  const keychain = `$(AppIdentifierPrefix)${app.id}`;
  if (hasWidgets) {
    appDict['com.apple.security.application-groups'] = [group];
    appDict['keychain-access-groups'] = [keychain];
    widgetDict['com.apple.security.application-groups'] = [group];
    widgetDict['keychain-access-groups'] = [keychain];
    if (grantEntitlements(app, ['aps-environment']).length && activityEntries(app).some(e => e.push === true)) {
      appDict['aps-environment'] = 'production';
    }
  }
  if (app.manifest.share_target) appDict['com.apple.security.application-groups'] = [group];
  const linked = [...new Set([...claimedDomains, ...associatedDomains(reach?.auth, false)])];
  if (linked.length) appDict['com.apple.developer.associated-domains'] = linked;
  const values = grantEntitlementValues(app, reach);
  for (const name of grantEntitlements(app, reach?.entitlements)) {
    appDict[name] = values[name] ?? entitlementValue(name, false, keychain);
  }
  Object.assign(appDict, merchantEntitlement(reach, { simulator: false, team: null, allows: [] }), signInEntitlement(reach?.auth, { simulator: false, team: null, allows: [] }));
  return { app: appDict, widget: widgetDict };
}

/** Exact-owned Info.plist keys for the brownfield host to merge into its app target. */
export function embedInfoPlist({ app, reach, usageKeys, rosterKeys = {}, launchParts = [], alarm = null, hasWidgets = false, generation = null, ios = true }) {
  const dict = { ...usageKeys(reach), ...rosterKeys };
  if (app.manifest.moduleConfig) dict.ExactModuleConfig = app.manifest.moduleConfig;
  if (app.manifest.audio_session) dict.ExactAudioSession = app.manifest.audio_session;
  if (launchParts.length) dict.ExactLaunchParts = launchParts;
  if (ios) {
    Object.assign(dict, backgroundKeys(app.manifest.host?.ios ?? {}, reach, app.id, false), nfcKeys(app.manifest.host?.ios ?? {}, false), startLocation(app), declaredOrigin(app), updateActivation(app), storeKeys(app, 'ios'), alarmKeys(alarm, false));
    if (hasWidgets) {
      Object.assign(dict, widgetAppKeys(app, 'ios', { group: widgetGroup(app), keychain: `$(AppIdentifierPrefix)${app.id}`, generation }));
      if (activityEntries(app).length) {
        dict.NSSupportsLiveActivities = true;
        if (activityEntries(app).some(e => e.frequent === true)) dict.NSSupportsLiveActivitiesFrequentUpdates = true;
      }
    }
  }
  return dict;
}

/** Emit consumer-compiled App Intents Swift sources and linker requirements (LLP 1031.000 D1.4). */
export function stageEmbedIntents({ embed, app, compat, launchPartsDir = null, nativeIntents = [], ios = true }) {
  nativeIntents = nativeIntents.filter(Boolean);
  const platform = ios ? 'ios' : 'macos';
  const faces = appleFaces(compat), controls = offeredControls(compat, platform);
  const launchFiles = launchPartsDir && existsSync(launchPartsDir)
    ? readdirSync(launchPartsDir).filter(f => f.endsWith('.swift')).sort() : [];
  if (!faces.length && !controls.length && !launchFiles.length && !nativeIntents.length) return null;

  const intentsDir = resolve(embed, 'intents');
  mkdirSync(intentsDir, { recursive: true });
  const sources = [];
  if (faces.length || controls.length) {
    const out = resolve(intentsDir, 'ExactIntents.swift');
    writeFileSync(out, intentsSource(faces, app.manifest.intents?.entities ?? {}, controls));
    sources.push('intents/ExactIntents.swift');
  }
  if (launchFiles.length) {
    const launchOut = resolve(intentsDir, 'LaunchParts');
    mkdirSync(launchOut, { recursive: true });
    for (const file of launchFiles) {
      copyFileSync(resolve(launchPartsDir, file), resolve(launchOut, file));
      sources.push(`intents/LaunchParts/${file}`);
    }
  }
  const launchLink = nativeIntents.flatMap(m => m.moduleName === 'ExactAppModules'
    ? ['-Xlinker', '-needed_library', '-Xlinker', ios ? 'Frameworks/ExactModules.framework/ExactModules' : 'libexact_modules.dylib',
      '-Xlinker', '-rpath', '-Xlinker', ios ? '@executable_path/Frameworks' : '@executable_path']
    : JSON.parse(readFileSync(m.values, 'utf8')).flatMap(t => ['-Xlinker', '-u', '-Xlinker', `_exact_launch_intent_type_${t.mangledTypeName}`]));
  return {
    sources,
    tools: faces.map(f => f.signature.name),
    controls: controls.map(c => c.kind ?? c.control?.kind).filter(Boolean),
    requiresExactToolDoor: pressesTools(compat, platform),
    launchLink,
  };
}

/** Emit consumer-owned WidgetKit extension sources, shared `ExactCore.framework`,
 * `ExactWidgetReload.framework`, plans, TypeScript, fonts, and metadata (LLP 1031.000 D1.6). */
export function stageEmbedWidgets({ embed, app, captureDir, compat, shared, triple, sdk, device, production, runApple, alarm = null, frameworkPlatform, generation, apiManifests = [], widgetEntitlements = {} }) {
  const entries = platformWidgetEntries(app, 'ios');
  const activities = activityEntries(app).length > 0;
  const controls = offeredControls(compat, 'ios');
  const hasCore = Boolean(shared);
  if (!hasCore && !controls.length && !alarm) return null;

  const widgetsDir = resolve(embed, 'widgets');
  const sourcesDir = resolve(widgetsDir, 'Sources');
  mkdirSync(sourcesDir, { recursive: true });
  const frameworksDir = resolve(embed, 'Frameworks');
  mkdirSync(frameworksDir, { recursive: true });

  let activityFramework = null;
  let fonts = [];
  if (hasCore) {
    const coreDest = resolve(frameworksDir, 'ExactCore.framework');
    cpSync(shared.framework, coreDest, { recursive: true, verbatimSymlinks: true });
    writePrivacyManifest(coreDest, [resolve(coreDest, 'ExactCore')], { apiManifests });

    const reloadBuilt = widgetReload({ directory: shared.base ?? dirname(shared.framework), triple, sdk, device, platform: 'ios', production, runApple });
    copyFileSync(reloadBuilt, resolve(frameworksDir, 'libexact_widget_reload.dylib'));
    wrapFramework(frameworksDir, 'libexact_widget_reload.dylib', 'ExactWidgetReload', app, frameworkPlatform);

    if (activities) {
      packageActivityArm({ app, bundle: embed, shared, sdk, triple, device, runApple });
      activityFramework = 'Frameworks/ExactActivity.framework';
    }
    for (const tree of ['plans', 'typescript']) {
      if (existsSync(resolve(captureDir, tree))) cpSync(resolve(captureDir, tree), resolve(widgetsDir, tree), { recursive: true });
    }
    fonts = copyWidgetFonts(captureDir, widgetsDir);
  }

  const alarmMember = alarm ? '; if #available(iOS 26.0, *) { ExactAlarmCountdown() }' : '';
  const bundleSource = !hasCore && !controls.length && alarm
    ? 'import SwiftUI\nimport WidgetKit\n@main struct ExactWidgets: WidgetBundle { var body: some Widget { if #available(iOS 26.0, *) { ExactAlarmCountdown() } } }\n'
    : widgetBundleSource(entries, { platform: 'ios', activities, alarmMember, entities: app.manifest.intents?.entities, controls: controls.length ? controlWidgetsSource(app.id, controls) : null });
  writeFileSync(resolve(sourcesDir, 'WidgetBundle.swift'), bundleSource);

  const sdkSources = [...(hasCore ? widgetSwiftSources(activities) : []), ...(alarm ? ALARM_SOURCES : [])];
  for (const file of sdkSources) copyFileSync(file, resolve(sourcesDir, basename(file)));
  const sources = ['widgets/Sources/WidgetBundle.swift', ...sdkSources.map(f => `widgets/Sources/${basename(f)}`)];

  const appKeys = hasCore ? widgetAppKeys(app, 'ios', { group: widgetGroup(app), keychain: `$(AppIdentifierPrefix)${app.id}`, generation }) : {};
  const infoPlist = {
    ...appKeys,
    ...(fonts.length ? { UIAppFonts: fonts } : {}),
    NSExtension: { NSExtensionPointIdentifier: 'com.apple.widgetkit-extension' },
  };
  return {
    sharedCore: hasCore ? 'Frameworks/ExactCore.framework' : null,
    reloadFramework: hasCore ? 'Frameworks/ExactWidgetReload.framework' : null,
    activityFramework,
    products: hasCore ? ['ExactSnapshot'] : [],
    moduleName: alarm ?? 'ExactWidgets',
    sources,
    plans: existsSync(resolve(widgetsDir, 'plans')),
    typescript: existsSync(resolve(widgetsDir, 'typescript')),
    fonts,
    weakFrameworks: alarm ? ['AlarmKit'] : [],
    infoPlist,
    entitlements: widgetEntitlements,
  };
}

function libraryPrivacyManifests(library, captured) {
  const paths = new Set([library?.privacyManifest, ...(library?.privacyManifestPaths ?? [])]);
  return [...captured.filter(manifest => paths.has(manifest.path)), ...(library?.privacyManifests ?? [])];
}

function listRelativeFiles(dir, prefix = '') {
  if (!existsSync(dir)) return [];
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...listRelativeFiles(resolve(dir, entry.name), rel));
    else out.push(rel);
  }
  return out;
}

/** Complete `--embed` writer stage (LLP 1031.000 D1-D3). Writes atomically into `paths.embed`. */
export function writeEmbedBundle({
  app, ios, device, tv = false, target, sdk, sdkName, triple, crate, libDir, paths, cleanup,
  buildReceipt, bakedCompat, bakedPlan, composition, optimize, production, distribution,
  appleLink, swiftCapabilities, gpuArtifacts, hasGpu, loadName, frameworkBundles, frameworkPlatform,
  svgFilterBuilt, svgFilterLibraryName, library,
  looseArms = [], hasSvg = false, svgBuilt = null, svgLoadName = 'libexact_svg.dylib',
  canvasGpuBuilt = null, canvasGpuLoadName = 'libexact_canvas_gpu.dylib',
  modulesBuilt = null, modulesLoadName = 'libexact_modules.dylib',
  moduleLibraries = [], packageLibraries = [], packageResources = [], serviceDylibs = [],
  rosterKeys = {}, launchPartsDir = null, launchModuleNames = [], nativeIntents = [],
  shared = null, alarm = null, appleReach = null, swiftc = '', linkRoot = null,
  claimedDomains = [],
  copyAppleGpuProducts, copyAppleStaticTrees, placeAppleArtifact, receipt, writeUsageStrings, usageKeys, runApple,
}) {
  const platform = ios ? (device ? 'ios' : 'ios-simulator') : 'macos';
  const embed = mkdtempSync(resolve(paths.namespace, '.embed-'));
  cleanup.push(embed);

  const hasWidgets = Boolean(ios && shared);
  const archive = `lib${crate.replace(/-/g, '_')}.a`;
  mkdirSync(resolve(embed, 'include'), { recursive: true });
  // When shared-core is emitted for widgets, the consumer links ExactCore.framework instead of lib<crate>.a (LLP 1031.000 D1.6).
  if (!hasWidgets) copyFileSync(resolve(libDir, archive), resolve(embed, archive));

  const families = [...readFileSync(resolve(pkg, 'Sources/CExact/module.modulemap'), 'utf8').matchAll(/explicit module \w+ \{\s*header "[^"]*\/(exact_(\w+)\.h)"/g)];
  const unlinked = new Set(families.filter(([, , name]) => appleLink !== 'all' && !appleLink.split(',').includes(name)).map(([, header]) => header));
  for (const header of readdirSync(resolve(pkg, 'include')).filter(h => !unlinked.has(h))) {
    copyFileSync(resolve(pkg, 'include', header), resolve(embed, 'include', header));
  }

  const frameworksDir = resolve(embed, 'Frameworks');
  const embeddedLibrary = embedLibrary(library, embed, ios);
  const appManifests = (buildReceipt.binary?.privacyManifests ?? []).slice(0, 1);
  const packageManifests = (buildReceipt.binary?.privacyManifests ?? []).slice(1);

  if (ios) {
    const wrapIntoFrameworks = (built, loose, name, privacy = {}) => {
      mkdirSync(frameworksDir, { recursive: true });
      copyFileSync(built, resolve(frameworksDir, loose));
      wrapFramework(frameworksDir, loose, name, app, frameworkPlatform, privacy);
    };
    if (gpuArtifacts.length) {
      mkdirSync(frameworksDir, { recursive: true });
      if (frameworkBundles) copyAppleGpuProducts(libDir, frameworksDir, gpuArtifacts, true);
      else for (const m of gpuArtifacts) wrapIntoFrameworks(resolve(libDir, m.built), m.load, m.framework, { apiManifests: appManifests });
    }
    for (const [has, built, load, framework] of looseArms) if (has && framework) wrapIntoFrameworks(built, load, framework, { apiManifests: appManifests });
    if (hasSvg && svgBuilt) wrapIntoFrameworks(svgBuilt, svgLoadName, 'ExactSvg', { apiManifests: appManifests });
    if (canvasGpuBuilt) wrapIntoFrameworks(canvasGpuBuilt, canvasGpuLoadName, 'ExactCanvasGpu', { apiManifests: appManifests });
    if (modulesBuilt) wrapIntoFrameworks(modulesBuilt, modulesLoadName, 'ExactModules', { modules: true, apiManifests: appManifests, manifests: packageManifests });
    for (const svc of serviceDylibs) wrapIntoFrameworks(svc.built, svc.load, svc.framework, { apiManifests: appManifests });
    for (const lib of packageLibraries) wrapIntoFrameworks(lib.built, lib.load, lib.framework, { modules: true, apiManifests: appManifests, manifests: libraryPrivacyManifests(lib, packageManifests) });
    for (const lib of moduleLibraries) { mkdirSync(frameworksDir, { recursive: true }); placeLibrary(lib, frameworksDir); }
  } else {
    copyAppleGpuProducts(libDir, embed, gpuArtifacts, false);
  }

  if (svgFilterBuilt) copyFileSync(svgFilterBuilt, resolve(embed, svgFilterLibraryName));
  copyAppleStaticTrees(paths.capture, embed);
  verifyBakeFiles(bakedCompat, bakedPlan, listAssets(embed, true, ['typescript']));

  // Resources: host.ios.resources, SwiftPM resource bundles, localized InfoPlist.strings, and icons/catalogs when declared (LLP 1031.000 D1.7).
  const resourcesDir = resolve(embed, 'Resources');
  if (ios) {
    if (app.manifest.host?.ios?.resources?.length) {
      mkdirSync(resourcesDir, { recursive: true });
      copyNativeResources(app, resourcesDir, buildReceipt.binary?.metadata?.nativeResources ?? [], 'ios');
    }
    if (packageResources.length) {
      mkdirSync(resourcesDir, { recursive: true });
      for (const from of new Set(packageResources)) cpSync(from, resolve(resourcesDir, basename(from)), { recursive: true });
    }
    if (appleReach && Object.keys(appleReach.usage ?? {}).length) {
      mkdirSync(resourcesDir, { recursive: true });
      writeUsageStrings(appleReach, resourcesDir);
    }
    const wantsAssets = Boolean(buildReceipt.binary?.metadata?.appleAssetCatalogs?.length || app.manifest.host?.ios?.icon || app.manifest.icons?.length || app.manifest.alternate_icons?.length);
    if (wantsAssets && linkRoot) {
      mkdirSync(resourcesDir, { recursive: true });
      iosAssets(app, resourcesDir, device, { tv, catalog: production, requireIcon: false, kept: { dir: resolve(linkRoot, 'assets'), stamp: swiftc }, ...bakedAssetInputs(buildReceipt.graph), expected: buildReceipt.binary?.metadata?.appleAssetCatalogs ?? [], expectedLaunchImage: buildReceipt.binary?.metadata?.launchImage ?? null });
    }
  }

  const generation = createHash('sha256').update(JSON.stringify([bakedCompat.id, buildReceipt.graph?.artifacts ?? []])).digest('hex');
  const entitlementPair = embedEntitlements({ app, reach: appleReach, hasWidgets, ios, claimedDomains });
  const embedWidgets = ios ? stageEmbedWidgets({
    embed, app, captureDir: paths.capture, compat: bakedCompat, shared, triple, sdk, device, production,
    runApple, alarm, frameworkPlatform, generation, apiManifests: appManifests, widgetEntitlements: entitlementPair.widget,
  }) : null;
  const embedIntents = stageEmbedIntents({ embed, app, compat: bakedCompat, launchPartsDir, nativeIntents, ios });

  if (ios && existsSync(frameworksDir)) assertArchiveFrameworks(frameworksDir);

  const products = ['ExactKit', ...swiftCapabilities];
  const compositionSource = 'ExactEmbedComposition.swift';
  writeFileSync(resolve(embed, compositionSource), generateEmbedComposition({ products, composition, ios }));
  writeEmbedPrivacyManifest(embed, { manifests: appManifests });

  const emittedFrameworks = existsSync(frameworksDir)
    ? readdirSync(frameworksDir).filter(name => /\.(?:framework|xcframework)$/.test(name)).sort().map(name => `Frameworks/${name}`)
    : [];
  const emittedResources = [
    'PrivacyInfo.xcprivacy',
    ...(svgFilterBuilt ? [svgFilterLibraryName] : []),
    ...['assets', 'deck', 'plans', 'shaders', 'typescript', 'activities'].filter(dir => existsSync(resolve(embed, dir))),
    ...listRelativeFiles(resourcesDir, 'Resources'),
  ];

  const embedMeta = {
    link: hasWidgets
      ? { mode: 'shared-core', archive: null, sharedCore: 'Frameworks/ExactCore.framework', headers: 'include' }
      : { mode: 'static-archive', archive, sharedCore: null, headers: 'include' },
    compositionSource,
    frameworks: emittedFrameworks,
    resources: emittedResources,
    privacyManifest: 'PrivacyInfo.xcprivacy',
    intents: embedIntents,
    widgets: embedWidgets,
    infoPlist: embedInfoPlist({ app, reach: appleReach, usageKeys, rosterKeys, launchParts: launchModuleNames, alarm, hasWidgets, generation, ios }),
    entitlements: entitlementPair,
  };

  writeFileSync(resolve(embed, 'compat.json'), JSON.stringify(bakedCompat, null, 2) + '\n');
  writeFileSync(resolve(embed, 'capabilities.json'), JSON.stringify({
    products,
    install: swiftCapabilities.map(product => `${product}.install()`),
    optimize,
    swiftFlags: production && optimize === 'size' ? ['-Osize', '-Xfrontend', '-internalize-at-link'] : [],
    ...embeddedLibrary,
    embed: embedMeta,
  }, null, 2) + '\n');

  const gpu = hasGpu ? (ios ? 'Frameworks/ExactGpu.framework/ExactGpu' : loadName) : null;
  writeFileSync(resolve(embed, 'receipt.json'), receipt(app, {
    platform,
    target: ios ? target : (process.arch === 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin'),
    sdk,
    archive: hasWidgets ? null : archive,
    ...(hasWidgets ? { sharedCore: 'Frameworks/ExactCore.framework' } : {}),
    gpu,
    package: pkg,
    composition,
    compatibilityId: bakedCompat.id,
    build: buildReceipt,
  }));

  const linkNote = hasWidgets
    ? 'Frameworks/ExactCore.framework (shared-core)'
    : `${archive} ${(statSync(resolve(embed, archive)).size / 1048576).toFixed(2)} MB`;
  placeAppleArtifact(embed, paths.embed);
  console.log(`host/apple: ${paths.embed.replace(root + '/', '')} — ${linkNote}, include/*.h, ${compositionSource}, PrivacyInfo.xcprivacy${gpu ? `, ${gpu}` : ''}${emittedFrameworks.length ? `, ${emittedFrameworks.length} framework${emittedFrameworks.length === 1 ? '' : 's'}` : ''}${svgFilterBuilt ? `, ${svgFilterLibraryName} (the bundle's top level)` : ios ? ', no SVG filter kernels (no Metal toolchain)' : ''}, compat.json; link it with ExactKit${composition === 'updating' ? ' + ExactUpdates' : ''} from the package at ${pkg.replace(root + '/', '')}`);
}
