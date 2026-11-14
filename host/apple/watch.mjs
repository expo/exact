import { writePrivacyManifest } from './privacy.mjs';
// @ref LLP 1126.001 WP17 — watch complications use the snapshot driver and their own store.
import { resolve, relative } from 'node:path';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, writeFileSync, copyFileSync, cpSync, rmSync } from 'node:fs';
import { buildBake, bakeOutput, developmentBuildEnv, HOST_DEV, appleExecutableName } from '../../scripts/app.mjs';
import { appleAssets, plistFile } from './assets.mjs';
import { appleProduction, stripProductionExecutable } from './link.mjs';
import { reportBuilt } from '../../scripts/help.mjs';
import { copyStaticTreeIfPresent } from '../web/serve.mjs';
import { ensureWatchSimulatorSDK } from './sdk.mjs';
import { platformWidgetEntries, prepareWidgetCore, sharedWidgetCore, placeCoreFramework, packageWidgets } from './widgets.mjs';
import { fileURLToPath } from 'node:url';
const root = resolve(fileURLToPath(new URL('../..', import.meta.url))), pkg = resolve(root, 'host/apple');

export function watchEntries(app) {
  return platformWidgetEntries(app, 'watchos').filter(entry => {
    if (!existsSync(resolve(app.dir, 'app.ts'))) return true;
    console.log(`host/apple: watchOS omits entry ${entry.kind}, including any Rust refresh in this TypeScript app, until LLP 1126.001 §5 Q3 passes`);
    return false;
  });
}
/** The companion's own Info.plist, never the iOS app's. */
export function watchInfo(app, { appInfo, group, entries, live, generation, icon = {} }) {
  return { CFBundleIdentifier: `${app.id}.watchkitapp`, CFBundleName: app.displayName, CFBundleDisplayName: app.displayName, CFBundleExecutable: 'ExactWatch', CFBundlePackageType: 'APPL',
    CFBundleVersion: appInfo.CFBundleVersion, CFBundleShortVersionString: appInfo.CFBundleShortVersionString, CFBundleSupportedPlatforms: ['WatchSimulator'], MinimumOSVersion: '10.0', UIDeviceFamily: [4],
    WKApplication: true, WKCompanionAppBundleIdentifier: app.id, WKRunsIndependentlyOfCompanionApp: true,
    ExactAppGroup: group, ExactAppIdentifier: app.id, ExactWidgetTypeScript: false, ExactWidgetGeneration: generation,
    ExactWatchEntries: entries.map(({ kind, families, title }) => ({ kind, families, title: title ?? kind })),
    ...(live ? { ExactWatchPlan: 'plans/watch.plan' } : {}), ...icon };
}
export function watchSimulator(args) {
  const at = args.indexOf('--watch-sim');
  if (at < 0) return undefined;
  const id = args[at + 1];
  if (!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(id ?? '')) throw new Error('--watch-sim requires a UDID (names are not accepted)');
  return id;
}
export function packageWatch({ bin, bundle, parent, production, symbols, sign, privacyManifests = [], ...widgets }) {
  const executable = resolve(bundle, 'ExactWatch');
  copyFileSync(resolve(bin, 'ExactWatch'), executable);
  stripProductionExecutable(executable, { production, dsym: resolve(symbols, 'ExactWatch.app.dSYM') });
  if (widgets.entries.length) packageWidgets({ ...widgets, bundle, production, symbols, sign, platform: 'watchos', device: false });
  const core = resolve(bundle, 'Frameworks/ExactCore.framework');
  stripProductionExecutable(resolve(core, 'ExactCore'), { production, dsym: resolve(symbols, 'ExactCore.framework.dSYM') });
  writePrivacyManifest(core, [resolve(core, 'ExactCore')], { apiManifests: privacyManifests });
  writePrivacyManifest(bundle, [executable], { apiManifests: privacyManifests });
  sign(core);
  sign(bundle);
  sign(parent);
}
export async function buildWatch(app, args, { runApple, appleArtifacts, appleBuildLock, receipt, buildParent }) {
  if (args.some(a => ['--device', '--archive', '--tvos', '--host', '--embed', '--test', '--url'].includes(a))) throw new Error('--watchos currently builds the watch simulator companion; use --sim for its iPhone and --watch-sim for its paired watch');
  if (args.includes('--run') && args.includes('--no-install')) throw new Error('--no-install takes no --run');
  const watchSim = watchSimulator(args);
  const release = appleBuildLock(app);
  try {
    const parentArgs = args.filter((a, i) => !['--watchos', '--run', '--no-install', '--watch-sim'].includes(a) && args[i - 1] !== '--watch-sim');
    await buildParent([...parentArgs, '--ios', '--no-install']);
    if (process.exitCode) throw new Error(`iOS companion build refused (exit ${process.exitCode}); watch build stopped`);
    const production = args.includes('--release') || process.env.EXACT_UPDATE_TRUST === 'production';
    const { optimize } = appleProduction(app, 'ios', args);
    const parent = appleArtifacts(app, { destination: 'ios-simulator', trust: production ? 'production' : 'development', optimize: production ? optimize : 'speed' });
    const entries = watchEntries(app);
    const watch = { ...app, id: `${app.id}.watchkitapp` };
    const bundle = resolve(parent.bundle, 'Watch', `${appleExecutableName(app)}.app`);
    rmSync(resolve(parent.bundle, 'Watch'), { recursive: true, force: true });
    // LLP 1126.002 D4: the bake refuses a live entry that reaches TypeScript.
    const live = !!app.manifest.watch;
    if (!entries.length && !live) {
      console.log(`host/apple: no watch entry and no Rust accessory entries; watch bundle omitted${args.includes('--run') ? '; --run skipped' : ''}`);
      return;
    }
    if (process.platform === 'linux') ensureWatchSimulatorSDK();
    const profile = production ? 'release' : HOST_DEV;
    const directory = resolve(parent.namespace, 'watchos'), target = 'aarch64-apple-watchos-sim', triple = 'arm64-apple-watchos10.0-simulator';
    mkdirSync(directory, { recursive: true });
    const sdk = runApple('xcrun', ['--sdk', 'watchsimulator', '--show-sdk-path'], { read: true }).stdout.trim();
    const env = { ...developmentBuildEnv(), EXACT_UPDATE_TRUST: production ? 'production' : 'development', WATCHOS_DEPLOYMENT_TARGET: '10.0', EXACT_APPLE_LINK: 'plan' };
    const baked = buildBake({ ...app, hasGpu: false }, 'ios', target, { env, profile });
    const output = resolve(bakeOutput(app, env), `ios-${target}.entries`);
    const libDir = resolve(app.target, target, profile), crate = app.crate('ios');
    const swiftRoot = resolve(directory, 'swift');
    const swiftArgs = ['build', '-c', production ? 'release' : 'debug', '--scratch-path', swiftRoot, ...(!production ? ['-Xswiftc', '-O'] : []),
      '--triple', triple, '--sdk', sdk, '-Xcc', '-isysroot', '-Xcc', sdk, '-Xlinker', '-syslibroot', '-Xlinker', sdk,
      '-Xswiftc', '-Xclang-linker', '-Xswiftc', '-isysroot', '-Xswiftc', '-Xclang-linker', '-Xswiftc', sdk];
    const swiftEnv = { ...process.env, WATCHOS_DEPLOYMENT_TARGET: '10.0', EXACT_LIB_DIR: libDir, EXACT_LIB: crate.replaceAll('-', '_'), EXACT_WIDGET_GALLERY: '0', EXACT_UPDATE_TRUST: production ? 'production' : 'development' };
    delete swiftEnv.SDKROOT;
    const swiftc = runApple('xcrun', ['swiftc', '--version'], { read: true }).stdout;
    const prepared = prepareWidgetCore({ directory: resolve(directory, 'core'), swiftBuildRoot: swiftRoot, swiftArgs, triple, platform: 'watchos', device: false, swiftc });
    const shared = sharedWidgetCore({ app: watch, prepared, libDir, crate, env: swiftEnv, runApple });
    const group = `group.${watch.id}`, entitlements = plistFile({ 'com.apple.security.application-groups': [group] });
    const entitlementPath = resolve(directory, 'watch-entitlements.plist');
    writeFileSync(entitlementPath, entitlements);
    // Simulator entitlements are embedded in __TEXT by each link.
    const sign = (path, _embeddedEntitlements) => runApple('codesign', ['--force', '--sign', '-', path]);
    mkdirSync(resolve(bundle, 'Frameworks'), { recursive: true });
    placeCoreFramework(shared.framework, resolve(bundle, 'Frameworks/ExactCore.framework'), watch, 'watchos');
    mkdirSync(resolve(bundle, 'plans'), { recursive: true });
    for (const entry of entries) copyFileSync(resolve(output, `plans/${entry.kind}.plan`), resolve(bundle, `plans/${entry.kind}.plan`));
    if (live) copyFileSync(resolve(output, 'plans/watch.plan'), resolve(bundle, 'plans/watch.plan'));
    for (const name of ['assets', 'deck']) copyStaticTreeIfPresent(resolve(app.dir, name), resolve(bundle, name));
    const appInfo = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(parent.bundle, 'Info.plist')], { read: true }).stdout);
    const icon = appleAssets(app, bundle, { platform: 'watchos', kept: { dir: resolve(directory, 'assets'), stamp: swiftc } });
    const info = watchInfo(app, { appInfo, group, entries, live, generation: createHash('sha256').update(JSON.stringify([baked.compat.id, baked.graph.artifacts])).digest('hex'), icon });
    writeFileSync(resolve(bundle, 'Info.plist'), plistFile(info));
    const executableArgs = [...swiftArgs, '-Xlinker', '-rpath', '-Xlinker', '@executable_path/Frameworks',
      '-Xlinker', '-sectcreate', '-Xlinker', '__TEXT', '-Xlinker', '__entitlements', '-Xlinker', entitlementPath];
    const options = { cwd: pkg, env: { ...swiftEnv, EXACT_CORE_FRAMEWORK: relative(pkg, shared.xcframework) } };
    runApple('swift', [...executableArgs, '--product', 'ExactWatch'], options);
    const bin = runApple('swift', [...executableArgs, '--show-bin-path'], { ...options, read: true }).stdout.trim();
    packageWatch({ bin, bundle, parent: parent.bundle, app: watch, shared, directory, sdk, triple, entitlements, sign, runApple, production, entries,
      privacyManifests: (baked.binary.privacyManifests ?? []).slice(0, 1), symbols: resolve(`${parent.binary}.dSYMs`, 'Watch') });
    writeFileSync(resolve(directory, 'receipt.json'), receipt(app, { platform: 'watchos-simulator', target, sdk, bundle, compatibilityId: baked.compat.id, build: baked }));
    console.log(`host/apple: ${bundle}; watch group ${group}; ${entries.map(e => `${e.kind} [${e.families.join(', ')}]`).join('; ')}`);
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
    if (args.includes('--run')) runApple('xcrun', ['simctl', 'launch', pair.watch.udid, watch.id]);
  } finally { release(); }
}
