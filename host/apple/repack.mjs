// Experimental (EXACT_EXPERIMENTAL=1; host/apple/toolchain.mjs): a development device archive from a previous one's native
// build, which a development archive made with the same switch can be:
//
//   EXACT_EXPERIMENTAL=1 bun host/apple/repack.mjs --base <base.ipa> --archive <out.ipa> [--extensions <kinds>] <app>
//
// The caller has established that nothing native changed since the base build: the same checkout, toolchain and flags.
// This bakes the app (the Contract and TypeScript into the plan, compat.json and module bytecode, with no Cargo and no
// Swift), fits the bake into a copy of the base executable's linked sections (host/apple/sections.mjs), and assembles
// the bundle as build.mjs's assembleIOS does for an unsigned development archive: Info.plist, usage strings, the app's
// assets, icons, the receipt, then an ad-hoc signature. The executable's code and every framework are the base's.
//
// Whatever it cannot vouch for it refuses, before writing the archive, with a line starting `repack refused:`; the
// caller then builds in full. It refuses an App Clip, a share or alarm extension, a GPU crate, a package framework, Rust
// of its own, a base executable without reserved sections, a bake whose compat.json inputs or reach differ from the
// base's (the Contract changed what the binary must provide: grants, executors, App Intents faces), and changed Swift
// module inputs (native-extras.mjs).
//
// An app with widgets or Live Activities links its plan into its own copy of ExactCore, not its executable
// (widgets.mjs placeCoreFramework). It repacks only on its own previous build, and only while what its extension's
// binary is generated from is the base's (receipt `build.widgets`, widgets.mjs widgetsDigest): ExactCore's sections are
// refitted; the bake's entry trees (plans/, typescript/, activities/) are written into the bundle and the extension;
// the app's widget keys (ExactWidgetGeneration, ExactKeychainAccessGroup, ExactWidgetTypeScript) and the extension's
// Info.plist are written as a full build writes them; and ExactCore and the extension are signed again.
//
// The base may be another app's (for example a template built once from `exact new`): the app's identity
// (its id and name) reaches no compiled code, only the bundle, executable and framework names and Info.plists,
// entitlements, the receipt, the signatures and compat.json's `inputs.app`, all of which this writes again. Nor do its
// grants: what they link is `inputs.linked` (a development archive links every capability), they are enforced at run
// time from the fitted compat.json, and what they reach (usage keys, entitlements) is written again here. Nor do its
// icons: `inputs.icons` names the manifest's icon files only so that a new icon is a new binary cohort (an update
// cannot redraw one); no compiled code reads it, and the icons themselves (Assets.car, AppIcon*.png, CFBundleIcons)
// are drawn again here, as a full build draws them. compat.json's other inputs must still be the base's.
import { experimental, spawnSync } from './toolchain.mjs';
import { appleCargo } from '../../scripts/cargo-check.mjs';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { appleArtifacts, appClip, COMPATIBLE_SDK, compiledTargets, designCompatible, distributionKeys, entitlements, infoPlist, receipt, shippedReceipt, writeUsageStrings, copyAppleStaticTrees } from './build.mjs';
import { modulePlist } from './plist.mjs';
import { awaitBuildOutput, bakedBackendOrigin, bakeOutput, bakeWatchEnv, developmentBuildEnv, appleExecutableName, gpuModules, HOST_DEV, resolveApp, verifyBakeFiles } from '../../scripts/app.mjs';
import { HOST_MACOS, withHostDeployment } from '../../scripts/filesystem.mjs';
import { prepareRustBundle, nativeSource } from '../../scripts/rust.mjs';
import { bakeOutside, linkedInputs } from '../../scripts/linked-plan.mjs';
import { bakedSettings } from '../../js/bake/src/__exact_env.mjs';
import { hasEntries, platformWidgetEntries, widgetExtensionInfo, widgetId, widgetsDigest } from './widgets.mjs';
import { EXTENSION_KINDS, extensionKinds, notificationService, withoutExtensions } from './extensions.mjs';
import { appleAssetsKey, bakedAssetInputs, frameworkInfo, iosAssets, plistFile } from './assets.mjs';
import { buildInfo } from './buildinfo.mjs';
import { fitLinkedSections, occurrencesOutsideLinked, readLinkedSections } from './sections.mjs';
import { nativeExtras } from './native-extras.mjs';
import { renameZipPrefix } from './zip-rename.mjs';
import { listAssets } from '../web/serve.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const TARGET = 'aarch64-apple-ios', SDK_NAME = 'iphoneos';
export class RepackRefused extends Error { constructor(why) { super(`repack refused: ${why}`); } }
const refuse = (why) => { throw new RepackRefused(why); };
const tool = (cmd, args, opts = {}) => {
  const r = spawnSync(...appleCargo(cmd, args, { cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, ...opts }));
  if (r.status !== 0) throw new Error(`${cmd} ${args[0] ?? ''} failed (${r.status ?? r.error?.message}): ${(r.stderr ?? '').trim().slice(-2000)}`);
  return r;
};
/** The entries of a base bundle that are its native build's, kept as they are. Anything else there that repack does
 * not write is refused by name, so a bundle layout it does not know is never shipped half-rebuilt. */
// PrivacyInfo.xcprivacy is native too: the build writes it from the executable's imports, the loose dylibs and the
// privacy manifests the native build captured (bundle.mjs), all of which a repack keeps from the base, whose native
// fingerprint it matches. Since the privacy check (40377f0086) every bundle has one, and refusing it ended every repack.
/** An app with widgets' shared framework, which holds its plan's sections, and its one extension. */
const CORE = 'Frameworks/ExactCore.framework/ExactCore', WIDGET_EXTENSION = 'PlugIns/ExactWidgets.appex';
/** The bake's entry trees a full build copies into the bundle root (build.mjs, from `<bake>.entries`). */
const ENTRY_TREES = ['plans', 'typescript', 'activities'];
const NATIVE_ENTRIES = new Set(['Frameworks', 'ExactSvgFilter.metallib', 'Metadata.appintents', 'PrivacyInfo.xcprivacy']);
/** What this repack writes again (assembleIOS's bundle-level resources) and the signature it replaces. */
const REBUILT = (name) => ['Info.plist', 'receipt.json', '_CodeSignature', 'assets', 'deck', 'Assets.car'].includes(name) || /^AppIcon.*\.png$/.test(name) || name.endsWith('.lproj');
/** compat.json's parts the binary acts on: what it was built to provide (`inputs`, but for the app's identity and
 * grants), its target and trust. A plan-only change leaves all of them as they were. */
const BINDING = ['inputs', 'target', 'embedded'];
/** `embedded`'s digests of the plan, each entry's plan and the bundle's assets are the bundle's own: they move with every
 * plan edit, and repack writes the plans again. */
const bindingOf = (compat, key) => key === 'embedded' && compat.embedded && typeof compat.embedded === 'object' ? (({ plan: _p, plans: _ps, assets: _a, ...rest }) => rest)(compat.embedded) : key === 'inputs' && compat.inputs && typeof compat.inputs === 'object' ? withoutApp(compat.inputs) : compat[key] ?? null;
/** compat.json's inputs but for the app's identity, its icons and its grants, which reach no compiled code
 * (`inputs.linked` says what the grants link; the icons are bundle resources repack draws again): the base may be
 * another app's, such as a template built with the placeholder icon. */
export const withoutApp = ({ app: _app, icons: _icons, grantCeiling: _c, rustGrants: _r, javascriptGrants: _j, ...inputs }) => inputs;
/** The compat.json fields a bake changed that the base's binary was built to provide: none, or what refuses a repack. */
export const bindingChanges = (baseCompat, compat) => {
  const differs = BINDING.filter(key => canonical(bindingOf(baseCompat, key)) !== canonical(bindingOf(compat, key)));
  const fields = differs.includes('inputs') ? Object.keys(withoutApp({ ...baseCompat.inputs, ...compat.inputs })).filter(key => canonical(baseCompat.inputs?.[key] ?? null) !== canonical(compat.inputs?.[key] ?? null)).map(key => `inputs.${key}`) : [];
  return [...fields, ...differs.filter(key => key !== 'inputs')];
};

/** Whether a base's full build stamped it as an archive (Info.plist's ExactBuildKind, XML as plistFile writes it):
 * a repack's kind is its base's, so it is the kind a full build of the same upload gets. */
export const archivedBase = (plist) => /<key>ExactBuildKind<\/key>\s*<string>archive<\/string>/.test(plist);

/** Why a base's layout cannot take this upload: a base with an extension is a widget app's (its executable has no
 * plan sections), and a widget app's base must have its extension and its own ExactCore. Null when it can. */
export const widgetLayoutRefusal = (widgets, base) =>
  !widgets && base.plugIns ? 'the base bundle holds PlugIns, which repack does not rebuild (a base with widgets or Live Activities this upload does not have)'
  : widgets && !(base.extension && base.core) ? 'the base was built without widgets or Live Activities (it has no widget extension or ExactCore)' : null;
/** Why an app with widgets cannot take a base (null when it can): ExactCore and the extension are its own build's, the
 * extension's binary is kept only while what generates it is unchanged (widgets.mjs widgetsDigest), and an alarm
 * countdown in the extension builds in full. */
export const widgetBaseRefusal = (upload, base) =>
  base.app !== upload.app ? `an app with widgets or Live Activities repacks only on its own previous build (the base is ${base.app ?? 'an unnamed app'}'s)`
  : !base.digest ? 'the base build recorded no widget digest (built before widget repacks)'
  : base.digest !== upload.digest ? 'the widget or Live Activity declarations changed (a kind, family, configuration or title), or the extensions packaged'
  : base.alarm ? 'an alarm countdown in the widget extension builds in full' : null;

/** The extension kinds this build packages, as build.mjs takes them: `--extensions`, less a notification service the
 * app does not have (so a widget app's widgetsDigest is its full build's). */
export const packagedKinds = (args, app) => {
  const kinds = extensionKinds(args);
  if (!notificationService(app)) kinds.delete('notifications');
  return kinds;
};

/** A repack's receipt `build`: the base's native build (its Cargo products and binary inputs), with `own` (what the
 * repack writes again) and what the bake in `baked` read and selected (its TypeScript sources, settings and backend
 * origin, its linked inputs), as app.mjs records them for a full build. A whole receipt names this bake's linked plan,
 * not the base's. */
export const repackedBuild = (base, baked, app, own) => {
  const { settings: _s, typescriptSources: _t, linkedInputs: _l, ...native } = base ?? {};
  return { ...native, ...own, backendOrigin: bakedBackendOrigin(baked), ...bakedSettings(baked, app.dir),
    ...('linkedPlan' in (base ?? {}) ? { linkedPlan: baked, linkedInputs: linkedInputs(baked) } : {}) };
};

export async function repack(args) {
  if (!experimental()) refuse('repack is experimental: set EXACT_EXPERIMENTAL=1');
  const at = (flag) => args.includes(flag) ? args[args.indexOf(flag) + 1] : null;
  const baseIpa = at('--base'), ipa = at('--archive') && resolve(process.cwd(), at('--archive'));
  const appArg = args.find((a, i) => !a.startsWith('--') && !['--base', '--archive', '--extensions'].includes(args[i - 1]));
  if (!baseIpa || !ipa || !appArg) throw new Error('usage: repack.mjs --base <base.ipa> --archive <out.ipa> [--extensions kinds] <app>');
  const started = Date.now(), times = {}, mark = (name) => { times[name] = (Date.now() - started) / 1000; };
  const app = resolveApp(appArg);
  // The extensions this build packages, as build.mjs takes them (--extensions): a widget left out is no widget here.
  const packaged = packagedKinds(args, app);
  if (packaged.size < EXTENSION_KINDS.length) app.manifest = withoutExtensions(app.manifest, packaged);
  // An app with widgets or Live Activities: its plan is in its own ExactCore's sections (see above).
  const widgets = hasEntries(app);
  mark('app');
  // What only a full build makes.
  if (app.manifest.game) refuse('a game builds in full');
  if (appClip(app)) refuse('an App Clip is a second bundle');
  if (app.manifest.share_target) refuse('a share extension is a second bundle');
  if (app.hasGpu || gpuModules(app.manifest).length) refuse('a GPU crate is part of the bundle\'s native build');
  if (existsSync(resolve(app.dir, 'data/Cargo.toml'))) refuse('an app with a Rust data crate bakes with its own compiled code');
  if (existsSync(resolve(app.dir, 'library')) || app.manifest.library) refuse('a package framework builds in full');

  // The app's build claim, as build.mjs holds it: the one writer of its bake directory (bakeOutside) until this is done.
  const release = awaitBuildOutput(app, appleArtifacts(app).lock, () => console.error(`waiting for the Apple build of ${app.name} already running`));
  let work = null, unzipping = null;
  try {
  // The bake, as build.mjs's buildBake prepares it for a development device archive, outside Cargo.
  const sdk = tool('xcrun', ['--sdk', SDK_NAME, '--show-sdk-path']).stdout.trim();
  let env = withHostDeployment({ ...process.env, ...developmentBuildEnv(), EXACT_WIDGET_GALLERY: '0', SDKROOT: sdk, MACOSX_DEPLOYMENT_TARGET: process.env.MACOSX_DEPLOYMENT_TARGET || HOST_MACOS, IPHONEOS_DEPLOYMENT_TARGET: compiledTargets(app, 'development').ios, EXACT_APPLE_LINK: 'all' });
  if (env.EXACT_UPDATE_TRUST && env.EXACT_UPDATE_TRUST !== 'development') refuse('only a development archive repacks');
  env.CARGO_TARGET_DIR = app.target; env.EXACT_BAKE_OUTPUT = bakeOutput(app, env);
  mkdirSync(env.EXACT_BAKE_OUTPUT, { recursive: true });
  if (prepareRustBundle(app, 'ios', TARGET, env)) refuse('an app with interpreted Rust bundles builds in full');
  const native = nativeSource(app, env); if (native) env.EXACT_NATIVE_SOURCE = native; else delete env.EXACT_NATIVE_SOURCE;
  Object.assign(env, bakeWatchEnv(app));
  mark('setup');
  // The base IPA unzips while the bake runs (a process of its own); both are needed only after it.
  work = mkdtempSync(resolve(tmpdir(), 'exact-repack-'));
  unzipping = Bun.spawn(['unzip', '-q', resolve(baseIpa), '-d', work], { stdout: 'ignore', stderr: 'pipe' });
  // The SDK here is a compiled, read-only one the native fingerprint pinned: its bake tool is built.
  const baked = bakeOutside(app, 'ios', TARGET, env, undefined, { toolBuilt: true });
  const plan = readFileSync(resolve(baked, 'app.plan')), compatText = readFileSync(resolve(baked, 'compat.json')), compat = JSON.parse(compatText);
  const bytecode = existsSync(resolve(baked, 'app.hbc')) ? readFileSync(resolve(baked, 'app.hbc')) : refuse('the bake made no module bytecode');
  if (compat.target !== TARGET || compat.inputs?.platform !== 'ios' || compat.inputs?.app !== app.id) throw new Error('the bake names another target, platform or app');
  const graph = JSON.parse(readFileSync(resolve(baked, 'artifacts.json'), 'utf8'));
  mark('bake');

  // The base bundle.
    if (await unzipping.exited !== 0) throw new Error(`unzip of the base IPA failed: ${(await new Response(unzipping.stderr).text()).trim().slice(-500)}`);
    mark('unzip');
    const apps = readdirSync(resolve(work, 'Payload')).filter(name => name.endsWith('.app'));
    if (apps.length !== 1) refuse('the base IPA does not hold exactly one app');
    const paths = appleArtifacts(app, { destination: 'ios', composition: compat.inputs.store?.L === '0' ? 'embedded' : 'updating', trust: 'development' });
    // NFD and iOS's flat-bundle name, as the Apple build names the executable and CFBundleExecutable (scripts/app.mjs
    // appleExecutableName(app, true), expo/exact#467: " App" after a name the bundle root reserves): an
    // app named with an accent ("Caf\u00e9") otherwise looks up and writes an NFC name the base bundle never had.
    const name = basename(paths.bundle), executable = appleExecutableName(app, true);
    // The bundle and executable are named after the app: a base of another app's (or one renamed since) is renamed.
    const base = resolve(work, 'Payload', apps[0]);
    const baseName = apps[0] === name ? executable : (/<key>CFBundleExecutable<\/key>\s*<string>([^<]+)<\/string>/.exec(readFileSync(resolve(base, 'Info.plist'), 'utf8'))?.[1] ?? refuse('the base bundle names no executable')).replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&apos;/g, "'");
    const baseExecutable = resolve(base, baseName);
    if (baseName.includes('/') || !existsSync(baseExecutable)) refuse(`the base bundle has no executable ${baseName}`);
    // Where the plan's sections are: the executable's, or an app with widgets' ExactCore. A base with an extension is
    // a widget app's, whose executable has none; a widget app's base must have its extension and its own ExactCore.
    const layout = widgetLayoutRefusal(widgets, { plugIns: existsSync(resolve(base, 'PlugIns')), extension: existsSync(resolve(base, WIDGET_EXTENSION)), core: existsSync(resolve(base, CORE)) });
    if (layout) refuse(layout);
    const holder = widgets ? CORE : baseName;
    let was;
    try { was = readLinkedSections(resolve(base, holder)); }
    catch (error) { refuse(`the base's ${holder} has no linked plan sections (${error.message})`); }
    const baseCompat = JSON.parse(was.compat.toString('utf8'));
    const changes = bindingChanges(baseCompat, compat);
    if (changes.length) refuse(`the Contract changed what the binary provides (compat.json ${changes.join(', ')})`);
    // The Swift module's generated inputs, which compat.json does not carry (a package `use`, data words, the theme).
    const baseReceipt = JSON.parse(readFileSync(resolve(base, 'receipt.json'), 'utf8'));
    if (!baseReceipt.build?.nativeExtras) refuse('the base build recorded no Swift module digest (built before repacks)');
    const extras = nativeExtras(app, 'ios', graph.extensions, graph.contractSources); mark('extras');
    if (baseReceipt.build.nativeExtras !== extras) refuse('the app\'s Swift module inputs changed (a package use or import, data words, or the theme of an app with Swift)');
    const basePlist = readFileSync(resolve(base, 'Info.plist'), 'utf8');
    const differs = widgets && widgetBaseRefusal({ app: app.id, digest: widgetsDigest(app, [...packaged]) }, { app: baseCompat.inputs?.app, digest: baseReceipt.build.widgets, alarm: basePlist.includes('<key>ExactAlarmCountdown</key>') });
    if (differs) refuse(differs);
    const kept = widgets ? new Set([...NATIVE_ENTRIES, 'PlugIns']) : NATIVE_ENTRIES;
    for (const entry of readdirSync(base)) if (entry !== baseName && !kept.has(entry) && !REBUILT(entry) && !(widgets && ENTRY_TREES.includes(entry))) refuse(`the base bundle holds ${entry}, which repack does not rebuild`);
    if (widgets) for (const entry of readdirSync(resolve(base, 'PlugIns'))) if (entry !== 'ExactWidgets.appex') refuse(`the base bundle holds PlugIns/${entry}, which repack does not rebuild`);

    // A new bundle: the base's native entries, then what assembleIOS writes for an unsigned development archive.
    const bundle = resolve(work, 'new', name);
    mkdirSync(bundle, { recursive: true });
    cpSync(baseExecutable, resolve(bundle, executable));
    for (const entry of kept) if (existsSync(resolve(base, entry))) cpSync(resolve(base, entry), resolve(bundle, entry), { recursive: true, verbatimSymlinks: true });
    // Another app's base: each framework's Info.plist names the app (frameworkInfo), and its signature covers it.
    // Each is written for this app, as wrapFramework does, and signed again as assembleIOS signs it.
    const baseId = baseCompat.inputs?.app;
    // The base's code is kept as it is: if its own app id is compiled in (outside the sections this rewrites), this app
    // would run with another app's id (its storage keyed under it). A full build compiles its own.
    if (baseId && baseId !== app.id && occurrencesOutsideLinked(baseExecutable, baseId)) refuse(`the base's executable names its app id ${baseId} in compiled code`);
    if (baseId !== app.id && existsSync(resolve(bundle, 'Frameworks'))) for (const entry of readdirSync(resolve(bundle, 'Frameworks'))) {
      // A loose dylib (an arm an SDK ships unwrapped, such as libexact_web.dylib) has no Info.plist, and its signature
      // names the file: it is every app's, and is kept as it is.
      if (entry.endsWith('.dylib')) continue;
      const framework = resolve(bundle, 'Frameworks', entry), name = basename(entry, '.framework'), plist = resolve(framework, 'Info.plist');
      if (!entry.endsWith('.framework') || !existsSync(plist) || readFileSync(plist, 'utf8') !== plistFile(frameworkInfo(name, { ...app, id: baseId }, 'iPhoneOS'))) refuse(`the base's ${entry} is not one repack can rename`);
      writeFileSync(plist, plistFile(frameworkInfo(name, app, 'iPhoneOS')));
      tool('codesign', ['--force', '--sign', '-', '--timestamp=none', framework], { stdio: 'ignore' });
    }
    mark('copy');
    // The sections, as the full build fits them (placeCoreFramework for an app with widgets).
    const sections = resolve(bundle, widgets ? CORE : executable);
    try { fitLinkedSections(sections, { plan, compat: compatText, bytecode }, { renewUuid: true }); }
    catch (error) { if (error.code === 'EXACT_SECTION_ROOM') refuse(`${error.message} (a base built before sections were reserved, or a plan that outgrew them)`); throw error; }
    // The executable's identity is the compat.json just fitted (assertAppleIdentity scans the whole file for it).
    const fitted = JSON.parse(readLinkedSections(sections).compat.toString('utf8'));
    if (fitted.id !== compat.id || fitted.inputs?.app !== app.id) throw new Error('the fitted executable does not name this bake');
    mark('sections');
    const appleReach = modulePlist(app, compat.reach, 'ios', graph.extensions, graph.contractSources);
    mark('reach');
    // Keys the native build computed and repack cannot: the module roster's first-frame modules (none on Linux).
    // (Converting the plist is a process of its own: only when the key is there at all.)
    const baseInfo = readFileSync(resolve(base, 'Info.plist')).includes('ExactFirstFrameModules') ? JSON.parse(tool('plutil', ['-convert', 'json', '-o', '-', resolve(base, 'Info.plist')]).stdout) : {};
    const rosterKeys = baseInfo.ExactFirstFrameModules ? { ExactFirstFrameModules: baseInfo.ExactFirstFrameModules } : {};
    writeFileSync(resolve(bundle, 'Info.plist'), infoPlist(app, true, { development: null, distributed: false, reach: appleReach, tv: false }));
    mark('plist1');
    copyAppleStaticTrees(app.dir, bundle, [['assets', 'assets'], ['deck', 'deck']]);
    // The bake's entry trees, as build.mjs captures them into the bundle (B1), and the widgets' own copies of the plans
    // and TypeScript in the extension, as packageWidgets copies them (the base's are the old bake's).
    if (widgets) {
      copyAppleStaticTrees(resolve(env.EXACT_BAKE_OUTPUT, `ios-${TARGET}.entries`), bundle, ENTRY_TREES.map(tree => [tree, tree]));
      const extension = resolve(bundle, WIDGET_EXTENSION);
      for (const tree of ['plans', 'typescript']) {
        rmSync(resolve(extension, tree), { recursive: true, force: true });
        if (existsSync(resolve(bundle, tree))) cpSync(resolve(bundle, tree), resolve(extension, tree), { recursive: true });
      }
      if (!existsSync(resolve(extension, 'plans')) && platformWidgetEntries(app).length) throw new Error('the bake wrote no plans tree for the widget extension');
      // What the bundle now holds of the bake's files is what its compat.json embeds, as assembleIOS checks it.
      verifyBakeFiles(compat, plan, listAssets(bundle, true, ['typescript']));
    }
    mark('trees');
    const linkRoot = paths.scratch; mkdirSync(linkRoot, { recursive: true });
    // The compiler the base's icons were keyed by: the same one, since the native fingerprint pins the toolchain.
    const swiftc = baseReceipt.build.assets?.stamp ?? tool('xcrun', ['--sdk', SDK_NAME, 'swiftc', '--version']).stdout ?? '';
    // Icons drawn from the same inputs as the base's are the base's (development archives record what theirs were made
    // from, bundle.mjs): taken from the base bundle with the Info.plist keys they gave, else drawn again.
    mark('swiftc');
    const assetsKey = appleAssetsKey(app, { platform: 'ios', device: true, catalog: false, stamp: swiftc, ...bakedAssetInputs(graph) });
    let assetKeys;
    if (baseReceipt.build.assets?.key === assetsKey && baseReceipt.build.assets.keys) {
      for (const entry of readdirSync(base)) if (entry === 'Assets.car' || /^AppIcon.*\.png$/.test(entry)) cpSync(resolve(base, entry), resolve(bundle, entry));
      assetKeys = baseReceipt.build.assets.keys;
    } else assetKeys = iosAssets(app, bundle, true, { tv: false, catalog: false, kept: { dir: resolve(linkRoot, 'assets'), stamp: swiftc }, ...bakedAssetInputs(graph) });
    mark('assets');
    const stampSdk = { sdk: SDK_NAME, sdkVersion: designCompatible(app, 'ios') ? COMPATIBLE_SDK.ios : null };
    // The toolchain and store keys (distributionKeys: DT*, UIRequiredDeviceCapabilities, orientations, versions) are in
    // a development archive only where this SDK's full build writes them (bundle.mjs: every archive, or only a
    // production one, as SDKs differ): as the base, which that full build made, has them.
    // Its kind (ExactBuildKind, which the app reads at run time: `archive` is a release build to BuildInfo.swift) is
    // the base's too: a development archive is `debug` where this SDK's full build stamps an archive only when it is
    // distributed (bundle.mjs), `archive` where it stamps every one.
    const stamped = basePlist.includes('<key>DTSDKName</key>');
    const stamp = { ...rosterKeys, ...(stamped ? distributionKeys(SDK_NAME) : {}), ...buildInfo(app, { root, archive: archivedBase(basePlist), production: false, ...stampSdk }) };
    writeFileSync(resolve(bundle, 'Info.plist'), infoPlist(app, true, { development: null, distributed: false, reach: appleReach, icon: assetKeys, distribution: stamp, tv: false }));
    // What assembleIOS adds after it for an app with widgets, in its order (plutil -insert, as it writes them).
    const infoPath = resolve(bundle, 'Info.plist');
    if (widgets) tool('plutil', ['-insert', 'ExactWidgetGeneration', '-string', createHash('sha256').update(JSON.stringify([compat.id, graph.artifacts])).digest('hex'), infoPath]);
    writeUsageStrings(appleReach, bundle);
    mark('plist');
    const ent = resolve(work, 'entitlements.plist');
    writeFileSync(ent, entitlements({ ...app, id: app.id, widgetTV: false, widgetHost: false }, undefined, false, appleReach, { simulator: false, prefix: undefined, allows: [], distributed: false }));
    if (widgets) {
      const group = JSON.parse(tool('plutil', ['-convert', 'json', '-o', '-', ent]).stdout)['keychain-access-groups'][0];
      tool('plutil', ['-insert', 'ExactKeychainAccessGroup', '-string', group, infoPath]);
      tool('plutil', ['-insert', 'ExactWidgetTypeScript', '-bool', String(existsSync(resolve(app.dir, 'app.ts'))), infoPath]);
      // ExactCore's Info.plist takes the app's versions and device capabilities (packageWidgets): the base's stands
      // only while those are the base app's.
      const json = (path) => JSON.parse(tool('plutil', ['-convert', 'json', '-o', '-', path]).stdout);
      const appInfo = json(infoPath), baseApp = json(resolve(base, 'Info.plist'));
      const inherited = (info) => canonical(['CFBundleVersion', 'CFBundleShortVersionString', 'MinimumOSVersion', 'UIRequiredDeviceCapabilities'].map(key => info[key] ?? null));
      if (inherited(appInfo) !== inherited(baseApp)) refuse('the app\'s version, minimum OS or device capabilities changed, which ExactCore\'s Info.plist carries');
      // The extension's Info.plist, from this app's, then its signature with its own entitlements, as packageWidgets
      // signs it for an unsigned device archive.
      const extension = resolve(bundle, WIDGET_EXTENSION);
      writeFileSync(resolve(extension, 'Info.plist'), widgetExtensionInfo(app, appInfo, 'ios'));
      const extensionEnt = resolve(work, 'widget-entitlements.plist');
      writeFileSync(extensionEnt, entitlements({ ...app, id: widgetId(app), widgetAppId: app.id }, undefined, false, null, { simulator: false, prefix: undefined }));
      tool('codesign', ['--force', '--sign', '-', '--timestamp=none', '--entitlements', extensionEnt, extension], { stdio: 'ignore' });
      // ExactCore's signature covers the sections just refitted.
      tool('codesign', ['--force', '--sign', '-', '--timestamp=none', resolve(bundle, 'Frameworks/ExactCore.framework')], { stdio: 'ignore' });
    }
    // The receipt is whole or shipped (shippedReceipt: no linkedPlan, the binary by its digest alone) where this SDK's
    // full build makes it so (bundle.mjs: every archive shipped, or only a production one), as the base's is.
    const whole = 'linkedPlan' in (baseReceipt.build ?? {}) || Array.isArray(baseReceipt.build?.binary?.inputs);
    const build = repackedBuild(baseReceipt.build, baked, app, { compat, graph, assets: { key: assetsKey, keys: assetKeys, stamp: swiftc } });
    mark('entitlements');
    const written = receipt(app, { compatibilityId: compat.id, build, composition: baseReceipt.composition, platform: 'ios', target: TARGET, sdk, identity: '-', profile: null, entitlements: readFileSync(ent, 'utf8'), gpu: null, development: null, repackedFrom: baseCompat.id });
    writeFileSync(resolve(bundle, 'receipt.json'), whole ? written : shippedReceipt(written));
    mark('receipt');
    tool('codesign', ['--force', '--sign', '-', '--timestamp=none', '--entitlements', ent, bundle], { stdio: 'ignore' });
    mark('sign');
    const payload = resolve(work, 'out', 'Payload');
    mkdirSync(payload, { recursive: true }); renameSync(bundle, resolve(payload, name));
    mkdirSync(dirname(ipa), { recursive: true }); rmSync(ipa, { force: true });
    // The base IPA, patched: only the entries whose bytes changed are removed and compressed again (the frameworks,
    // most of the archive, keep the base's compressed bytes).
    // (A renamed base: its entries take this bundle's name, and its executable is compared as this app's.)
    const before = files(base), now = files(resolve(payload, name)), prefix = `Payload/${name}/`;
    if (baseName !== executable) { before.set(executable, before.get(baseName)); before.delete(baseName); }
    const renamedAway = baseName !== executable ? [prefix + baseName] : [];
    mark('compare');
    const changed = [...now.keys()].filter(rel => !before.has(rel) || !before.get(rel).equals(now.get(rel)));
    const removed = [...before.keys()].filter(rel => !now.has(rel));
    if (apps[0] === name) cpSync(resolve(baseIpa), ipa); else renameZipPrefix(resolve(baseIpa), ipa, `Payload/${apps[0]}/`, prefix);
    mark('copy-ipa');
    // (A renamed base's executable is in the archive under its old name: that entry goes, and this one is added.)
    const drop = [...[...changed.filter(rel => before.has(rel) && !(renamedAway.length && rel === executable)), ...removed].map(rel => prefix + rel), ...renamedAway];
    if (drop.length) tool('zip', ['-q', '-d', ipa, '-@'], { input: drop.join('\n') + '\n' });
    mark('zip-delete');
    if (process.env.EXACT_REPACK_DEBUG) console.log(`changed: ${changed.join(' ')}`);
    if (changed.length) tool('zip', ['-q', '-1', '-X', '-y', ipa, '-@'], { cwd: resolve(work, 'out'), input: changed.map(rel => prefix + rel).join('\n') + '\n' });
    mark('archive');
    console.log(`host/apple: ${ipa} (repacked from the base's native build: ${Object.entries(times).map(([step, s], i, all) => `${step} ${(s - (all[i - 1]?.[1] ?? 0)).toFixed(2)}`).join(', ')})`);
    return { times, compat: compat.id };
  } finally {
    // An unzip still running on an early failure stops before its destination goes.
    if (unzipping) { unzipping.kill(); await unzipping.exited; }
    if (work) rmSync(work, { recursive: true, force: true });
    release();
  }
}

const canonical = (value) => Array.isArray(value) ? `[${value.map(canonical).join(',')}]` : value && typeof value === 'object' ? `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}` : JSON.stringify(value ?? null);

if (import.meta.main) {
  try { await repack(process.argv.slice(2)); }
  catch (error) { console.error(error instanceof RepackRefused ? error.message : (error.stack ?? String(error))); process.exit(error instanceof RepackRefused ? 3 : 1); }
}

/** Every file under `dir` (relative path -> bytes; a link as its target's text), for the archive patch. */
function files(dir, at = dir, out = new Map()) {
  for (const entry of readdirSync(at, { withFileTypes: true })) {
    const path = resolve(at, entry.name), rel = path.slice(dir.length + 1);
    if (entry.isSymbolicLink()) out.set(rel, Buffer.from(`link:${readlinkSync(path)}`));
    else if (entry.isDirectory()) files(dir, path, out);
    else out.set(rel, readFileSync(path));
  }
  return out;
}
