// A development device archive from a previous one's native build (a build service's `?base=`):
//
//   bun host/apple/repack.mjs --base <base.ipa> --archive <out.ipa> [--extensions <kinds>] <app>
//
// The caller has established that nothing native changed since the base build (the service's native fingerprint).
// This bakes the app (the Contract and TypeScript into the plan, compat.json and module bytecode, with no Cargo and no
// Swift), fits the bake into a copy of the base executable's linked sections (host/apple/sections.mjs), and assembles
// the bundle as build.mjs's assembleIOS does for an unsigned development archive: Info.plist, usage strings, the app's
// assets, icons, the receipt, then an ad-hoc signature. The executable's code and every framework are the base's.
//
// Whatever it cannot vouch for it refuses, before writing the archive, with a line starting `repack refused:`; the
// caller then builds in full. It refuses an app with widgets or Live Activities (their plans link into ExactCore), an
// App Clip, a share or alarm extension, a GPU crate, a package framework, Rust of its own, a base executable without
// reserved sections, and a bake whose compat.json inputs or reach differ from the base's (the Contract changed what
// the binary must provide: grants, executors, App Intents faces).
import { spawnSync } from './toolchain.mjs';
import { appleCargo } from '../../scripts/cargo-check.mjs';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { appleArtifacts, appClip, COMPATIBLE_SDK, deploymentTargets, designCompatible, distributionKeys, entitlements, infoPlist, modulePlist, receipt, shippedReceipt, writeUsageStrings, copyAppleStaticTrees, assertAppleIdentity } from './build.mjs';
import { bakeOutput, bakeWatchEnv, developmentBuildEnv, executableName, gpuModules, HOST_DEV, resolveApp } from '../../scripts/app.mjs';
import { HOST_MACOS, withHostDeployment } from '../../scripts/filesystem.mjs';
import { prepareRustBundle, nativeSource } from '../../scripts/rust.mjs';
import { bakeOutside } from '../../scripts/linked-plan.mjs';
import { hasEntries } from './widgets.mjs';
import { iosAssets } from './assets.mjs';
import { buildInfo } from './buildinfo.mjs';
import { fitLinkedSections, readLinkedSections } from './sections.mjs';

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
const NATIVE_ENTRIES = new Set(['Frameworks', 'ExactSvgFilter.metallib', 'Metadata.appintents']);
/** What this repack writes again (assembleIOS's bundle-level resources) and the signature it replaces. */
const REBUILT = (name) => ['Info.plist', 'receipt.json', '_CodeSignature', 'assets', 'deck', 'Assets.car'].includes(name) || /^AppIcon.*\.png$/.test(name) || name.endsWith('.lproj');
/** compat.json's parts the binary acts on: what it was built to provide (`inputs`), what its grants reach (`reach`:
 * entitlements, usage keys), its target and trust. A plan-only change leaves all of them as they were. */
const BINDING = ['inputs', 'reach', 'target', 'embedded'];
/** `embedded`'s digests of the plan and the bundle's assets are the bundle's own: they move with every plan edit. */
const bindingOf = (compat, key) => key === 'embedded' && compat.embedded && typeof compat.embedded === 'object' ? (({ plan: _p, assets: _a, ...rest }) => rest)(compat.embedded) : compat[key] ?? null;

export async function repack(args) {
  const at = (flag) => args.includes(flag) ? args[args.indexOf(flag) + 1] : null;
  const baseIpa = at('--base'), ipa = at('--archive') && resolve(process.cwd(), at('--archive'));
  const appArg = args.find((a, i) => !a.startsWith('--') && !['--base', '--archive', '--extensions'].includes(args[i - 1]));
  if (!baseIpa || !ipa || !appArg) throw new Error('usage: repack.mjs --base <base.ipa> --archive <out.ipa> [--extensions kinds] <app>');
  const started = Date.now(), times = {}, mark = (name) => { times[name] = (Date.now() - started) / 1000; };
  const app = resolveApp(appArg);
  // What only a full build makes.
  if (app.manifest.game) refuse('a game builds in full');
  if (hasEntries(app)) refuse('an app with widgets or Live Activities links its plan into ExactCore');
  if (appClip(app)) refuse('an App Clip is a second bundle');
  if (app.manifest.share_target) refuse('a share extension is a second bundle');
  if (app.hasGpu || gpuModules(app.manifest).length) refuse('a GPU crate is part of the bundle\'s native build');
  if (existsSync(resolve(app.dir, 'data/Cargo.toml'))) refuse('an app with a Rust data crate bakes with its own compiled code');
  if (existsSync(resolve(app.dir, 'library')) || app.manifest.library) refuse('a package framework builds in full');

  // The bake, as build.mjs's buildBake prepares it for a development device archive, outside Cargo.
  const targets = deploymentTargets(app), sdk = tool('xcrun', ['--sdk', SDK_NAME, '--show-sdk-path']).stdout.trim();
  let env = withHostDeployment({ ...process.env, ...developmentBuildEnv(), EXACT_WIDGET_GALLERY: '0', SDKROOT: sdk, MACOSX_DEPLOYMENT_TARGET: process.env.MACOSX_DEPLOYMENT_TARGET || HOST_MACOS, IPHONEOS_DEPLOYMENT_TARGET: targets.ios, EXACT_APPLE_LINK: 'all' });
  if (env.EXACT_UPDATE_TRUST && env.EXACT_UPDATE_TRUST !== 'development') refuse('only a development archive repacks');
  env.CARGO_TARGET_DIR = app.target; env.EXACT_BAKE_OUTPUT = bakeOutput(app, env);
  mkdirSync(env.EXACT_BAKE_OUTPUT, { recursive: true });
  if (prepareRustBundle(app, 'ios', TARGET, env)) refuse('an app with interpreted Rust bundles builds in full');
  const native = nativeSource(app, env); if (native) env.EXACT_NATIVE_SOURCE = native; else delete env.EXACT_NATIVE_SOURCE;
  Object.assign(env, bakeWatchEnv(app));
  const baked = bakeOutside(app, 'ios', TARGET, env);
  const plan = readFileSync(resolve(baked, 'app.plan')), compatText = readFileSync(resolve(baked, 'compat.json')), compat = JSON.parse(compatText);
  const bytecode = existsSync(resolve(baked, 'app.hbc')) ? readFileSync(resolve(baked, 'app.hbc')) : refuse('the bake made no module bytecode');
  if (compat.target !== TARGET || compat.inputs?.platform !== 'ios' || compat.inputs?.app !== app.id) throw new Error('the bake names another target, platform or app');
  const graph = JSON.parse(readFileSync(resolve(baked, 'artifacts.json'), 'utf8'));
  mark('bake');

  // The base bundle.
  const work = mkdtempSync(resolve(tmpdir(), 'exact-repack-'));
  try {
    tool('unzip', ['-q', resolve(baseIpa), '-d', work]);
    const apps = readdirSync(resolve(work, 'Payload')).filter(name => name.endsWith('.app'));
    if (apps.length !== 1) refuse('the base IPA does not hold exactly one app');
    const paths = appleArtifacts(app, { destination: 'ios', composition: compat.inputs.store?.L === '0' ? 'embedded' : 'updating', trust: 'development' });
    const name = basename(paths.bundle), executable = executableName(app);
    if (apps[0] !== name) refuse(`the app's bundle is now ${name}, the base's ${apps[0]}`);
    const base = resolve(work, 'Payload', name), baseExecutable = resolve(base, executable);
    if (!existsSync(baseExecutable)) refuse(`the base bundle has no executable ${executable}`);
    const was = readLinkedSections(baseExecutable), baseCompat = JSON.parse(was.compat.toString('utf8'));
    const differs = BINDING.filter(key => canonical(bindingOf(baseCompat, key)) !== canonical(bindingOf(compat, key)));
    if (differs.length) {
      const fields = differs.includes('inputs') ? Object.keys({ ...baseCompat.inputs, ...compat.inputs }).filter(key => canonical(baseCompat.inputs?.[key] ?? null) !== canonical(compat.inputs?.[key] ?? null)).map(key => `inputs.${key}`) : [];
      refuse(`the Contract changed what the binary provides (compat.json ${[...fields, ...differs.filter(key => key !== 'inputs')].join(', ')})`);
    }
    for (const entry of readdirSync(base)) if (entry !== executable && !NATIVE_ENTRIES.has(entry) && !REBUILT(entry)) refuse(`the base bundle holds ${entry}, which repack does not rebuild`);

    // A new bundle: the base's native entries, then what assembleIOS writes for an unsigned development archive.
    const bundle = resolve(work, 'new', name);
    mkdirSync(bundle, { recursive: true });
    for (const entry of [executable, ...NATIVE_ENTRIES]) if (existsSync(resolve(base, entry))) cpSync(resolve(base, entry), resolve(bundle, entry), { recursive: true, verbatimSymlinks: true });
    try { fitLinkedSections(resolve(bundle, executable), { plan, compat: compatText, bytecode }, { renewUuid: true }); }
    catch (error) { if (error.code === 'EXACT_SECTION_ROOM') refuse(`${error.message} (a base built before sections were reserved, or a plan that outgrew them)`); throw error; }
    assertAppleIdentity(app, resolve(bundle, executable), compat.id);
    mark('sections');
    const appleReach = modulePlist(app, compat.reach, 'ios');
    // Keys the native build computed and repack cannot: the module roster's first-frame modules (none on Linux).
    const baseInfo = JSON.parse(tool('plutil', ['-convert', 'json', '-o', '-', resolve(base, 'Info.plist')]).stdout);
    const rosterKeys = baseInfo.ExactFirstFrameModules ? { ExactFirstFrameModules: baseInfo.ExactFirstFrameModules } : {};
    writeFileSync(resolve(bundle, 'Info.plist'), infoPlist(app, true, { development: null, distributed: false, reach: appleReach, tv: false }));
    copyAppleStaticTrees(app.dir, bundle, [['assets', 'assets'], ['deck', 'deck']]);
    const linkRoot = paths.scratch; mkdirSync(linkRoot, { recursive: true });
    const swiftc = tool('xcrun', ['--sdk', SDK_NAME, 'swiftc', '--version']).stdout ?? '';
    const assetKeys = iosAssets(app, bundle, true, { tv: false, catalog: false, kept: { dir: resolve(linkRoot, 'assets'), stamp: swiftc } });
    mark('assets');
    const stampSdk = { sdk: SDK_NAME, sdkVersion: designCompatible(app, 'ios') ? COMPATIBLE_SDK.ios : null };
    const stamp = { ...rosterKeys, ...distributionKeys(SDK_NAME), ...buildInfo(app, { root, archive: true, production: false, ...stampSdk }) };
    writeFileSync(resolve(bundle, 'Info.plist'), infoPlist(app, true, { development: null, distributed: false, reach: appleReach, icon: assetKeys, distribution: stamp, tv: false }));
    writeUsageStrings(appleReach, bundle);
    const ent = resolve(work, 'entitlements.plist');
    writeFileSync(ent, entitlements({ ...app, id: app.id, widgetTV: false, widgetHost: false }, undefined, false, appleReach, { simulator: false, prefix: undefined, allows: [], distributed: false }));
    // The base receipt's native build (its Cargo products and binary inputs), with this bake's compat and graph.
    const baseReceipt = JSON.parse(readFileSync(resolve(base, 'receipt.json'), 'utf8'));
    const build = { ...baseReceipt.build, compat, graph };
    writeFileSync(resolve(bundle, 'receipt.json'), shippedReceipt(receipt(app, { compatibilityId: compat.id, build, composition: baseReceipt.composition, platform: 'ios', target: TARGET, sdk, identity: '-', profile: null, entitlements: readFileSync(ent, 'utf8'), gpu: null, development: null, repackedFrom: baseCompat.id })));
    tool('codesign', ['--force', '--sign', '-', '--timestamp=none', '--entitlements', ent, bundle], { stdio: 'ignore' });
    mark('sign');
    const payload = resolve(work, 'out', 'Payload');
    mkdirSync(payload, { recursive: true }); renameSync(bundle, resolve(payload, name));
    mkdirSync(dirname(ipa), { recursive: true }); rmSync(ipa, { force: true });
    tool('ditto', ['-c', '-k', '--sequesterRsrc', '--keepParent', payload, ipa]);
    mark('archive');
    console.log(`host/apple: ${ipa} (repacked from the base's native build: ${Object.entries(times).map(([step, s]) => `${step} ${s.toFixed(1)} s`).join(', ')})`);
    return { times, compat: compat.id };
  } finally { rmSync(work, { recursive: true, force: true }); }
}

const canonical = (value) => Array.isArray(value) ? `[${value.map(canonical).join(',')}]` : value && typeof value === 'object' ? `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}` : JSON.stringify(value ?? null);

if (import.meta.main) {
  try { await repack(process.argv.slice(2)); }
  catch (error) { console.error(error instanceof RepackRefused ? error.message : (error.stack ?? String(error))); process.exit(error instanceof RepackRefused ? 3 : 1); }
}
