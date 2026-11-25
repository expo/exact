// The publisher (LLP 1030.007 D1): what publishing to a stream needs, for every app kind, shared by its two callers.
// `push.mjs` publishes the working tree's edit now; `deploy.mjs` publishes what its classification of an immutable
// snapshot allows (LLP 1030.000 D3). This module owns
//
// - discovery and stream start (`streams`): each admissible signed head with its cohort receipt, and each production
//   build receipt for a new stream, whose cohort is exactly the binary that build made;
// - the candidate per stream (`streamCandidates`): the plan baked by the app's kind, its graph, its assets and its
//   module pair, checked by the caller against the stream's cohort (`classifyArtifacts`);
// - the signed publish (`publishStream`): blobs read back, `seq` under the stream's lock, the head signed, an
//   immutable release record (LLP 1026 D11).
import { addedSwiftPackageInputs, swiftPackageInputChanged } from './package-libraries.mjs';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, readlinkSync, rmSync, writeFileSync } from 'node:fs';
import { hostname, userInfo } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Refusal, refuse, assetIndex, canonicalBytes, canonicalJson, envelopeBytes, portableAssetNames } from './deploy-signing.mjs';
import { admitHead, authenticatedReleaseFloor, binaryInputFile, cohortFiles, cohortReceipt, keepRecord, nextSeq, recoverCohort, sameCohortPayload } from './deploy-heads.mjs';
import { bakedEntryAssets, buildBake, classifyArtifacts, nativeManifestMetadata } from './app.mjs';
import { directoryInputNames, watchOutput } from './cargo-dep-info.mjs';
import { notificationServiceInputChanges, notificationServiceSourceRoot } from './notification-service.mjs';
import { appleAssetCatalogInventory, launchImageInventory, nativeResourceInventory } from '../host/apple/assets.mjs';
import { buildRust, publishedSignature, rustBundle, rustPackage, tieredNative } from './rust.mjs';
import { blobPath, sha256, streamPath } from './origin.mjs';
import { filesystem, filesystemRead, filesystemTree } from './filesystem.mjs';
import { streamBundle, unbundledTextures } from './texture-families.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

/** A human correlation id with millisecond UTC time, snapshot prefix, and a
 * random run nonce. Two publishers of the same commit in one clock tick do
 * not share the receipt namespace. */
export function defaultRelease(commit, now = new Date(), nonce = randomBytes(8).toString('hex')) {
  return `r-${now.toISOString().replace(/[-:]/g, '')}-${commit.slice(0, 7)}-${nonce}`;
}

/** The streams of `channel` with their frozen cohorts (LLP 1030.007 D1).
 * Push can start any stream from a production build receipt. Deploy uses
 * production receipts for repair and to start Android streams (D4). */
export async function streams(origin, app, channel, { deploying = false, platforms = [], builds = [] } = {}) {
  const out = [];
  const productionBuilds = builds.filter(({ trust, compat: { inputs } }) => trust === 'production'
    && inputs.platform !== 'web' && inputs.store?.L !== '0' && (!platforms.length || platforms.includes(inputs.platform)));
  // A declared list names the base app's streams; a variant finds its own (LLP 1030.008 D7).
  const declared = app.manifest.variant ? undefined : app.manifest.deploy?.streams?.filter(s => s.channel === channel);
  const names = deploying && declared ? declared.map(s => s.compatibilityId) : await origin.list(`.exact/${channel}`);
  for (const compatibilityId of names ?? []) {
    const stream = { channel, compatibilityId };
    const found = await origin.head(stream);
    if (!found && !deploying) {
      const history = await authenticatedReleaseFloor(origin, app, stream);
      if (history.floor !== null) refuse(`${streamPath(stream)}: the head is missing with authenticated release history; run deploy to recover it`);
      continue;
    }
    // Apps share a channel's directory on an origin; another app's stream is not ours.
    try { if (found && JSON.parse(found.bytes).app?.id !== app.id) continue; } catch { /* admitHead says why */ }
    const admission = found ? await admitHead(origin, found, app, stream) : null;
    const build = productionBuilds.find(build => build.compat.id === compatibilityId);
    const record = deploying && !(admission?.usable && admission.authenticated)
      ? await recoverCohort(origin, app, stream, admission, build) : null;
    if (deploying && !found && !record) {
      const history = await origin.list(`${streamPath(stream)}/releases`, { includeHidden: true });
      if (history?.length === 0 || history === null && origin.kind === 'directory') continue;
    }
    const head = admission?.usable && admission.authenticated ? admission.head : null;
    const cohort = head ? admission.cohort : record?.cohort;
    const platform = cohort?.compat?.inputs?.platform ?? record?.platform ?? null;
    if (platforms.length && platform && !platforms.includes(platform)) continue;
    if (!cohort) {
      const problem = head ? 'the head carries no cohort receipt' : `the head is not admissible (${admission?.problem ?? record?.cohortProblem ?? 'unauthenticated'})`;
      if (!deploying) refuse(`${streamPath(stream)}: ${problem}; run deploy`);
      out.push({ stream, platform, problem });
      continue;
    }
    const needsEvidenceBake = deploying && cohort.binaryInputs
      && await origin.get(blobPath(cohort.binaryInputs.sha256)) === null
      && !matchingBinaryInputFile(cohort.binaryInputs, build);
    out.push({ stream, head, cohort, platform, ...(record ? { recovered: true } : {}), ...(needsEvidenceBake ? { needsEvidenceBake } : {}) });
  }
  for (const build of productionBuilds) {
    const inputs = build.compat.inputs;
    // A stream that has a head keeps its frozen cohort; the receipt of its id is the binary just built, whose
    // evidence its candidate is compared with (LLP 1030.007 D3).
    const known = out.find((s) => s.stream.compatibilityId === build.compat.id);
    if (known) { if (known.cohort) known.build = build; continue; }
    if (deploying && inputs.platform !== 'android') continue;
    out.push({ stream: { channel, compatibilityId: build.compat.id }, head: null, cohort: cohortReceipt(build), platform: inputs.platform, build });
  }
  if (!out.length && !deploying) refuse(`no stream on the channel ${channel} at ${origin.describe()} has a head; a first deploy publishes them (bun scripts/deploy.mjs ${app.name} --channel ${channel} --origin ${origin.describe()} --yes)`);
  return out;
}

/** `exact-bake graph` and, for a TypeScript app, `exact-js-bake`, built once
 * beside the Rust baker's tools. */
export function publishTools(app, env, typescript) {
  const target = resolve(app.target, 'push-tools');
  const packages = ['exact-bake', ...(typescript ? ['exact-js-bake'] : [])];
  const r = spawnSync('cargo', ['build', '-q', '--release', ...packages.flatMap((p) => ['-p', p])], { cwd: ROOT, env: { ...env, CARGO_TARGET_DIR: target }, stdio: 'inherit' });
  if (r.status !== 0) refuse(`could not build ${packages.join(' and ')}`);
  return { graph: resolve(target, 'release/exact-bake'), typescript: resolve(target, 'release/exact-js-bake') };
}

/** A TypeScript app's plan, module pair and named entries, baked as its build
 * script bakes them (`exact_js_bake::bake`, production mode). */
function bakeTypescript(tool, app, env) {
  const dir = mkdtempSync(resolve(app.target, 'push-ts-'));
  try {
    rmSync(dir, { recursive: true });
    const r = spawnSync(tool, [app.dir, '--out', dir], { cwd: ROOT, env, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    if (r.status !== 0) refuse(r.stderr.trim().split('\n').slice(-6).join('\n'));
    return { plan: readFileSync(resolve(dir, 'app.plan')), receipt: readFileSync(resolve(dir, 'app.module.json')), bytecode: readFileSync(resolve(dir, 'app.hbc')), entries: bakedEntries(dir) };
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

/** The plan and named entries of an app with neither module, from its own web crate's bake without the wasm, as
 * deploy's snapshot bake makes them (`host/web/build.mjs --bake`): the Contract compiled and, for a `rust.data` app,
 * the first frame baked through its data source (LLP 1153.003 D2), as each native host's bake runs it (LLP 1030.007
 * D2). */
function bakeApp(app, env) {
  mkdirSync(app.target, { recursive: true });
  const output = mkdtempSync(resolve(app.target, 'push-bake-'));
  try {
    const build = buildBake(app, 'web', 'wasm32-unknown-unknown', { env, check: true, output });
    const plan = readFileSync(resolve(output, `web-${build.compat.target}.plan`));
    return { plan, entries: new Map(bakedEntryAssets(output, build, namedEntry).map((a) => [a.name, a.bytes])) };
  } finally { rmSync(output, { recursive: true, force: true }); }
}

/** Whether an update's file is one of the named entries a bake makes beside the app's plan: its widgets' and Live
 * Activities' plans and their companion files (bake/src/widgets.rs). */
const namedEntry = (name) => /^(plans|activities)\//.test(name);

/** The named entries a bake wrote under `dir`, by name. */
const bakedEntries = (dir) => new Map(['plans', 'activities'].flatMap((tree) => [...(filesystemTree(resolve(dir, tree)) ?? [])].map(([name, bytes]) => [`${tree}/${name}`, bytes])));

/** The module pair as an update carries it (`typescript/`): the plan no
 * longer needs the cohort's source shapes, since the module answers them;
 * the pair needs the prelude runtime the bake found it uses (its receipt's
 * `typescriptRuntime`, `bake/src/compat.rs` `typescript_runtime_of`), which
 * the classifier compares with the cohort's binary, so a module that uses
 * nothing newer still goes to an older binary (LLP 1016.000 D10); and it
 * needs exactly its source's baked grants (`Paired::decode`), within the
 * cohort's ceiling. */
function typescriptBundle(bundle, build, baked, cohort) {
  const meta = JSON.parse(baked.receipt), runtime = meta.typescriptRuntime;
  const inputs = cohort.compat.inputs;
  if (meta.appId !== inputs.app) refuse(`the module names app ${meta.appId}, not ${inputs.app}`);
  const grantKey = ['javascriptGrants', 'rustGrants', 'grantCeiling'].find(key => typeof inputs[key] === 'string');
  // Grants the cohort records and the module's differ: a proof that this binary cannot run it (`needsBinary`, LLP
  // 1156.006.000 D25). A cohort that records none proves nothing.
  const differs = !!grantKey && (meta.grants ?? '').trim() !== inputs[grantKey].trim();
  if (!grantKey || differs) throw Object.assign(new Refusal('the TypeScript module declares different grants than the installed binary; that needs a new binary'), { needsBinary: differs });
  const assets = [
    { name: 'typescript/app.module.json', bytes: baked.receipt },
    { name: 'typescript/app.hbc', bytes: baked.bytecode },
  ].map((a) => ({ ...a, sha256: sha256(a.bytes) }));
  const runtimeRequired = runtime !== undefined ? { typescriptRuntime: runtime } : {};
  // An Android cohort names its Hermes bytecode version and what its APK was composed from (LLP 1030.006 D4, D4a):
  // a pair of another version, or one whose source derives a permission the APK does not declare, needs a new APK.
  const android = inputs.hermesBytecode !== undefined ? { hermesBytecode: meta.bytecodeVersion ?? null } : {};
  if (inputs.androidLinked) android.androidPermissions = meta.androidPermissions ?? [];
  // A module that can leave an answer for the executor needs an Apple binary that links it, and so does one whose
  // receipt omits `typescriptHostWork` (LLP 1047.001 D12, D13).
  const apple = ['ios', 'macos'].includes(inputs.platform) && meta.typescriptHostWork !== false ? { appleUses: ['io'] } : {};
  const requires = { grantCeiling: meta.grants ?? '', [grantKey]: inputs[grantKey], ...runtimeRequired, ...android, ...apple };
  // A new stream's bake already lists a pair when its widgets or activities need one (bake/src/widgets.rs); this one replaces it.
  const other = (a) => !a.name.startsWith('typescript/');
  const artifacts = build.graph.artifacts.filter(other).map((a) => {
    if (a.name !== 'app.plan') return a;
    const { sources, ...rest } = a.requires;
    return { ...a, requires: { ...rest, ...runtimeRequired } };
  });
  artifacts.push(...assets.map((a) => ({ name: a.name, sha256: a.sha256, bytes: a.bytes.length, kind: 'bundle', requires })));
  return { bundle: { ...bundle, assets: [...bundle.assets.filter(other), ...assets] }, build: { ...build, graph: { ...build.graph, artifacts } } };
}

function planGraph(tool, app, plan, compat, env, entries) {
  const dir = mkdtempSync(resolve(app.target, 'push-graph-'));
  try {
    writeFileSync(resolve(dir, 'app.plan'), plan);
    writeFileSync(resolve(dir, 'compat.json'), JSON.stringify(compat));
    const graphPath = resolve(dir, 'graph.json');
    const output = openSync(graphPath, 'w');
    try {
      const r = spawnSync(tool, ['graph', resolve(dir, 'app.plan'), resolve(dir, 'compat.json'), compat.target, app.dir],
        { env, encoding: 'utf8', stdio: ['ignore', output, 'pipe'] });
      if (r.status !== 0) refuse(r.stderr?.trim() || r.error?.message || `exact-bake graph exited with ${r.signal ?? r.status}`);
    } finally { closeSync(output); }
    const graph = JSON.parse(readFileSync(graphPath, 'utf8'));
    const icons = [...(graph.appleIcons ?? [])];
    for (const [name, bytes] of entries ?? []) {
      if (!name.startsWith('plans/') || !name.endsWith('.plan')) continue;
      const entry = planGraph(tool, app, bytes, compat, env);
      icons.push(...(entry.appleIcons ?? []));
      const raster = entry.artifacts[0].requires.appleRasterIcons;
      const uses = entry.artifacts[0].requires.appleUses;
      graph.artifacts.push({ ...entry.artifacts[0], name, requires: { ...(raster ? { appleRasterIcons: raster } : {}), ...(uses ? { appleUses: uses } : {}) } });
    }
    graph.appleIcons = [...new Map(icons.sort((a, b) => Buffer.compare(Buffer.from(a.svg), Buffer.from(b.svg))).map(icon => [icon.svg, icon])).values()];
    return graph;
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

/** The compatibility inputs this source computes for the cohort's platform and target, with `base.compat`'s grants
 * (a snapshot's fresh web bake, or the cohort's own). This computes no native artifact. */
function snapshotCompat(tool, app, cohort, base, built, env) {
  const inputs = base.compat.inputs;
  const args = ['compat', app.dir, '--platform', cohort.compat.inputs.platform, '--target', cohort.compat.target, '--json'];
  const grants = inputs.javascriptGrants ?? inputs.rustGrants ?? inputs.grantCeiling;
  if (typeof grants === 'string') args.push('--grants', grants);
  if (typeof inputs.javascriptGrants === 'string' && typeof inputs.rustGrants === 'string') args.push('--rust-grants', inputs.rustGrants);
  const settings = built.receipt && JSON.parse(built.receipt).env;
  if (settings) args.push('--settings', JSON.stringify(settings));
  const r = spawnSync(tool, args, { env, encoding: 'utf8' });
  if (r.status !== 0) refuse(r.stderr.trim());
  return JSON.parse(r.stdout);
}

function matchingBinaryInputFile(card, build) {
  const file = binaryInputFile(build);
  return file?.sha256 === card.sha256 && file.bytes.length === card.bytes ? file : null;
}

/** Read the binary input evidence named by the cohort. During stream start
 * or repair a local build can supply missing bytes that match the card. */
export async function readBinaryInputFile(origin, cohort, build) {
  const card = cohort.binaryInputs;
  if (!card) return null;
  const bytes = origin ? await origin.get(blobPath(card.sha256)) : null;
  if (bytes && bytes.length === card.bytes && sha256(bytes) === card.sha256) return { bytes };
  if (bytes === null) {
    const file = matchingBinaryInputFile(card, build);
    if (file) return { bytes: file.bytes, restore: file };
  }
  refuse('the cohort binary input receipt is missing or does not match its signed digest');
}

/** Compare a native build's source evidence with this source. Native SDKs and generated products are not needed to
 * read it. */
function binaryInputEvidence(bytes) {
  const evidence = JSON.parse(bytes);
  if (evidence.version !== 1) refuse('the cohort binary input receipt has an unsupported version');
  return evidence;
}

export function binaryInputChanges(bytes, app, compat, appleIcons, evidence = binaryInputEvidence(bytes), env = process.env, extensions) {
  const platform = compat.inputs.platform, apple = ['ios', 'macos'].includes(platform);
  const assetApp = { ...app, appleIcons, manifest: { host: app.manifest.host } };
  const appleAssetCatalogs = apple ? appleAssetCatalogInventory(assetApp, compat.target.includes('-tvos') ? 'tvos' : platform) : [];
  const launchImage = platform === 'ios' ? launchImageInventory(app) : null;
  const nativeResources = apple ? nativeResourceInventory(app, null, platform) : [];
  const metadata = { ...(appleAssetCatalogs.length ? { appleAssetCatalogs } : {}), ...(launchImage ? { launchImage } : {}), ...(nativeResources.length ? { nativeResources } : {}), ...(apple && evidence.metadata.appleExtensions !== undefined ? { appleExtensions: evidence.metadata.appleExtensions } : {}),
    ...nativeManifestMetadata(app.manifest, platform, compat.target, env, compat.embedded?.trust !== 'production'), delivery: compat.delivery, store: compat.inputs.store, keys: compat.inputs.keys };
  const keys = new Set([...Object.keys(metadata), ...Object.keys(evidence.metadata)]);
  const changed = [...keys].filter(key => canonicalJson(metadata[key] ?? null) !== canonicalJson(evidence.metadata[key] ?? null));
  const roots = { app: app.dir, exact: ROOT, workspace: app.workspace ?? app.dir };
  const skip = watchOutput([app.target, ...['dist', 'web-dist', 'dist-windows', 'artifacts'].map(name => resolve(app.dir, name))]);
  const check = (rows, matches) => {
    for (const row of rows) {
      if (!Object.hasOwn(roots, row.root)) refuse(`the binary input receipt names an unknown source root ${row.root}`);
      try { if (matches(resolve(roots[row.root], row.path), row)) continue; } catch { /* a missing input changed */ }
      changed.push(`${row.root}/${row.path}`);
    }
  };
  check(evidence.files, (path, row) => sha256(readFileSync(path)) === row.sha256);
  check(evidence.directories, (path, row) => canonicalJson(directoryInputNames(path, skip)) === canonicalJson(row.names));
  check(evidence.absent, path => !existsSync(path));
  check(evidence.links, (path, row) => sha256(Buffer.from(readlinkSync(path))) === row.sha256);
  if (apple) changed.push(...addedSwiftPackageInputs(app.dir, compat.target.includes('-tvos') ? 'tvos' : platform, extensions, evidence.inventories));
  for (const row of evidence.inventories ?? []) {
    if (!Object.hasOwn(roots, row.root)) refuse(`the binary input receipt names an unknown source root ${row.root}`);
    if (row.kind === 'swift-package') {
      if (swiftPackageInputChanged(resolve(roots[row.root], row.path), row.identity)) changed.push(`${row.identity.interface}/${row.identity.directory} Swift package`);
      continue;
    }
    if (row.kind !== 'notification-service') refuse(`the binary input receipt names an unknown inventory ${row.kind}`);
    changed.push(...notificationServiceInputChanges(resolve(roots[row.root], row.path), row.identity).map(name => name === notificationServiceSourceRoot ? `${row.root}/${row.path}` : `${row.root}/${row.path}/${name}`));
  }
  return changed;
}

/** A new stream's assets from the app's files: the bake's bundle rows, held to the digests the binary was built with.
 * What the bake made (the named entries and the module pair) comes from push's own bake instead. */
function bakedAssets(app, build) {
  const made = (name) => namedEntry(name) || name.startsWith('typescript/');
  return build.graph.artifacts.filter((a) => a.name !== 'app.plan' && !made(a.name)).map((a) => {
    const path = resolve(app.dir, a.name.startsWith('shaders/') ? `gpu/${a.name}` : a.name);
    const bytes = existsSync(path) ? readFileSync(path) : null;
    if (!bytes || sha256(bytes) !== a.sha256) refuse(`${a.name}: ${path} is not what the installed binary was built with; push to an existing stream, or rebuild the binary`);
    return { name: a.name, bytes, sha256: a.sha256 };
  });
}

/** The head's assets other than its module pair and named entries, which push
 * bakes: an `assets/` or `deck/` file as it is now in the app, but for the
 * texture families a cohort baked
 * with one (`inputs.textureFamily`) does not read, and anything else (shaders)
 * as the head has it. A cohort baked before thinning names no family: its
 * renderer falls back to files a thinned update would leave out. */
async function carriedAssets(origin, app, head, inputs) {
  const out = [];
  for (const folder of ['assets', 'deck']) {
    const root = resolve(app.dir, folder);
    const names = Object.keys(filesystem({ op: 'names', root, optionalRoot: true }) ?? {});
    const unbundled = unbundledTextures(names.map((name) => `${folder}/${name}`), inputs?.textureFamily);
    for (const name of names.filter((name) => !unbundled.has(`${folder}/${name}`))) {
      const encoded = await filesystemRead({ op: 'get', root, path: name });
      if (encoded === null) refuse(`${folder}/${name}: asset disappeared during push`);
      const bytes = Buffer.from(encoded, 'base64');
      out.push({ name: `${folder}/${name}`, bytes, sha256: sha256(bytes) });
    }
  }
  for (const card of head.assets ?? []) {
    if (/^(rust|typescript|assets|deck)\//.test(card.name) || namedEntry(card.name)) continue;
    const bytes = await origin.get(blobPath(card.sha256));
    if (!bytes || sha256(bytes) !== card.sha256) refuse(`${card.name}: the head's blob is missing from the origin`);
    out.push({ name: card.name, bytes, sha256: card.sha256 });
  }
  return out;
}

/** The command that builds an APK with a production receipt (LLP 1030.006 D6); `--bundle` is the store's form. */
export const APK_COMMAND = 'EXACT_UPDATE_TRUST=production bun exact.mjs android --release (--bundle for the store)';

/** What a stream needs when an update cannot carry a change: Android's APK is built by its own verb, every other
 * platform's binary by deploy. */
export function binaryCommand(app, platform) {
  return platform === 'android' ? `a new APK: ${APK_COMMAND}` : `a new binary: bun scripts/deploy.mjs ${app.name}`;
}

/** Candidates for signed cohorts, without building their hosts. Deploy calls
 * this from its immutable source snapshot; push supplies its resident baker. */
export async function streamCandidates(app, origin, found, env, { baker = null, snapshot = null, tools = null } = {}) {
  if (!found.length) return [];
  const typescript = existsSync(resolve(app.dir, 'app.ts'));
  tools ??= publishTools(app, env, typescript);
  // A Rust module is built for each native target a stream's binary runs, as deploy builds it; a wasm stream takes
  // any of them, whose wasm variant is the same.
  const nativeTargets = [...new Set(found.filter((s) => ['native', 'tiered'].includes(s.cohort.compat.inputs.rustMode)).map((s) => s.cohort.compat.target))];
  const variants = new Map();
  // The three kinds of bake (LLP 1030.007 D2): the TypeScript module, the Rust module, or the app's own bake, which a
  // snapshot has already made.
  if (typescript) variants.set(null, bakeTypescript(tools.typescript, app, env));
  else if (rustPackage(app)) for (const target of nativeTargets.length ? nativeTargets : [null]) variants.set(target, await buildRust(app, { compat: found[0].cohort.compat, env, nativeTarget: target, profile: 'release', baker }));
  else variants.set(null, snapshot ? { plan: snapshot.bundle.plan.bytes, entries: new Map(snapshot.bundle.assets.filter(a => namedEntry(a.name)).map(a => [a.name, a.bytes])) } : bakeApp(app, env));
  const updates = [];
  for (const { stream, head, cohort, platform, recovered, build: binary } of found) {
    // The compatibility inputs this source computes and the cohort's do not match: its binary predates the source.
    let superseded = [], nativeData = [];
    try {
      let built = variants.get(cohort.compat.target) ?? variants.values().next().value;
      // Without a snapshot's bake the grants are the cohort's: a module's are checked by its own receipt, and a data
      // crate's are its native code, which `dataCrate` covers.
      const evidenceFile = await readBinaryInputFile(head || recovered ? origin : null, cohort, !head || snapshot ? binary : null);
      const evidenceBytes = evidenceFile?.bytes ?? null;
      const evidence = evidenceBytes === null ? null : binaryInputEvidence(evidenceBytes);
      const compatEnv = evidence?.metadata.appleExtensions !== undefined ? { ...env, EXACT_APPLE_EXTENSIONS: evidence.metadata.appleExtensions.join(',') } : env;
      const compat = snapshotCompat(tools.graph, app, cohort, snapshot?.compat ? snapshot : { compat: cohort.compat }, built, compatEnv);
      const graph = planGraph(tools.graph, app, built.plan, compat, env, built.entries);
      const inputs = cohort.compat.inputs;
      // Plans check their required raster renditions. Native packaging owns
      // module pins, linked composition and texture thinning; an Android
      // cohort's composition and Hermes bytecode version are compared with
      // what the generation requires (LLP 1030.006 D4, D4a).
      const packaged = new Set(['typescriptModule', 'linked', 'linkRoster', 'textureFamily', 'appleRasterIcons', 'androidLinked', 'hermesBytecode']);
      const keys = new Set([...Object.keys(compat.inputs), ...Object.keys(inputs)]);
      superseded = [...keys].filter(key => !packaged.has(key) && canonicalJson(compat.inputs[key] ?? null) !== canonicalJson(inputs[key] ?? null));
      // A module of another Hermes bytecode version than the APK's engine runs is for a newer APK (LLP 1030.006 D4).
      const bytecode = typescript && inputs.hermesBytecode !== undefined ? JSON.parse(built.receipt).bytecodeVersion ?? null : undefined;
      if (bytecode !== undefined && canonicalJson(bytecode) !== canonicalJson(inputs.hermesBytecode ?? null)) superseded.push('hermesBytecode');
      // Without a module, the data crate runs in the binary and baked the plan's first frame (LLP 1030.007 D5).
      nativeData = superseded.filter(key => key === 'dataCrate');
      // The native files and manifest metadata the binary's evidence names, which the compatibility id does not all encode.
      const unchecked = evidence === null;
      const pendingInputs = [...superseded, ...(unchecked ? [] : binaryInputChanges(evidenceBytes, app, compat, graph.appleIcons, evidence, env, graph.extensions))];
      // The named entries are this bake's, in place of the last generation's whole.
      const entries = [...built.entries].map(([name, bytes]) => ({ name, bytes, sha256: sha256(bytes) }));
      const staticAssets = snapshot
        ? streamBundle(snapshot.bundle, platform, inputs).assets.filter(a => !/^(rust|typescript)\//.test(a.name) && !namedEntry(a.name))
        : head ? await carriedAssets(origin, app, head, inputs) : bakedAssets(app, binary);
      const assets = [...staticAssets, ...entries];
      let bundle = { plan: { bytes: built.plan, sha256: sha256(built.plan), formatVersion: inputs.formatVersion, kernelSchema: inputs.kernelSchema }, assets };
      // A new stream's head names the production build's evidence, which publication uploads.
      const sourceInputs = binary?.binary?.sourceInputs ? { sourceInputs: binary.binary.sourceInputs } : {};
      let build = { version: 1, trust: 'production', compat, binary: { sha256: cohort.binary, ...sourceInputs }, pendingInputs,
        graph: { ...graph, artifacts: [graph.artifacts[0], ...assets.map((a) => ({ name: a.name, sha256: a.sha256, bytes: a.bytes.length, kind: 'bundle', requires: graph.artifacts.find(card => card.name === a.name)?.requires ?? snapshot?.graph?.artifacts.find(card => card.name === a.name)?.requires ?? {} }))] } };
      if (built.variants?.native && ['native', 'tiered'].includes(inputs.rustMode) && cohort.compat.target.includes('apple')) {
        const reused = publishedSignature(built.variants.native.bytes, await publishedModule(origin, app, stream), env);
        if (reused) built = { ...built, variants: { ...built.variants, native: { ...built.variants.native, bytes: reused } } };
      }
      ({ bundle, build } = typescript ? typescriptBundle(bundle, build, built, cohort) : rustBundle(app, bundle, build, built));
      const row = { kind: 'stream', platform, channel: stream.channel, compatibilityId: stream.compatibilityId, action: 'bundle', receipt: cohort };
      const note = unchecked ? { note: `${platform} ${stream.compatibilityId.slice(0, 8)}: the signed cohort records no binary input evidence, so a change to native sources goes undetected until a new binary records it` } : {};
      updates.push({ row, bundle, compat: cohort.compat, build, superseded, nativeData, ...note });
    } catch (error) {
      if (!(error instanceof Refusal)) throw error;
      updates.push({ row: { kind: 'stream', platform, ...stream }, reason: error.message, needsBinary: error.needsBinary === true, superseded, nativeData });
    }
  }
  return updates;
}

// ---------------------------------------------------------------- publishing

export function sunsetFor(app, stream) {
  const cards = app.manifest.deploy?.sunset;
  const key = `${stream.channel}/${stream.compatibilityId}`;
  return cards && Object.hasOwn(cards, key) ? cards[key] : cards?.[stream.compatibilityId] ?? null;
}

/** Plan and asset changes by digest, plus the stream's advisory card. */
export function changesAgainst(bundle, head, sunset = null) {
  const changes = [];
  const card = (value) => value ? [value.message, value.store || null] : null;
  if (JSON.stringify(card(sunset)) !== JSON.stringify(card(head?.sunset))) changes.push({ name: 'sunset', change: !sunset ? 'removed' : !head?.sunset ? 'new' : 'changed' });
  if (!head) {
    changes.push({ name: 'app.plan', change: 'new' });
    for (const a of bundle.assets) changes.push({ name: a.name, change: 'new' });
    return changes;
  }
  const plan = head.plan;
  if (plan?.sha256 !== bundle.plan.sha256 || plan?.bytes !== bundle.plan.bytes.length
    || plan?.url !== `../../blobs/${bundle.plan.sha256}` || plan?.formatVersion !== bundle.plan.formatVersion
    || plan?.kernelSchema !== bundle.plan.kernelSchema) changes.push({ name: 'app.plan', change: 'changed' });
  const before = new Map((head.assets ?? []).map((a) => [a.name, a]));
  for (const a of bundle.assets) {
    const had = before.get(a.name);
    if (had === undefined) changes.push({ name: a.name, change: 'new' });
    else if (had.sha256 !== a.sha256 || had.bytes !== a.bytes.length || had.url !== `./${a.sha256}`) {
      changes.push({ name: a.name, change: 'changed', note: a.name.endsWith('.wgsl') ? 'interface unchanged: asset' : undefined });
    }
    before.delete(a.name);
  }
  for (const name of before.keys()) changes.push({ name, change: 'removed' });
  return changes;
}

/** A stream's asset index, a blob beside the ones it names: its cards
 * resolve against its own URL, so each is `./<sha256>`. Null for no assets. */
export function streamIndex(bundle) {
  portableAssetNames(bundle.assets.map((asset) => asset.name));
  return assetIndex(bundle.assets.map((asset) => ({ name: asset.name, url: `./${asset.sha256}`, sha256: asset.sha256, bytes: asset.bytes.length })),
    (sha) => `../../blobs/${sha}`);
}

/** The unsigned stream head. One producer keeps its identity/name and blob
 * pointers identical across classification fixtures and publication. */
export function streamHead({ app, bundle, stream, seq, release, sunset = null }) {
  const index = streamIndex(bundle);
  const head = {
    exact: 1,
    app: { id: app.id, name: app.displayName },
    plan: { url: `../../blobs/${bundle.plan.sha256}`, sha256: bundle.plan.sha256, bytes: bundle.plan.bytes.length,
      formatVersion: bundle.plan.formatVersion, kernelSchema: bundle.plan.kernelSchema },
    ...(index ? { assets: index.card } : {}),
    stream: { app: app.id, channel: stream.channel, compatibilityId: stream.compatibilityId, seq },
    release,
  };
  if (sunset) head.sunset = sunset.store ? { message: sunset.message, store: sunset.store } : { message: sunset.message };
  return head;
}

/** The signed native Rust module of the stream's last published release: its
 * head authenticated, its blob the one the head names. Else null. */
export async function publishedModule(origin, app, stream) {
  try {
    const admission = await admitHead(origin, await origin.head(stream), app, stream);
    const card = admission.usable && admission.authenticated ? admission.head.assets?.find((a) => /^rust\/app\.module\.(dylib|bin)$/.test(a.name)) : null;
    const bytes = card ? await origin.get(blobPath(card.sha256)) : null;
    if (!bytes || sha256(bytes) !== card.sha256) return null;
    return card.name.endsWith('.bin') ? tieredNative(bytes) : bytes;
  } catch { return null; }
}

/** Publish one stream (LLP 1030.000 D3 item 5): under its lock, refuse a
 * reused immutable receipt, read the head and allocate seq, put and verify
 * content-addressed blobs, prepare the immutable release record, then swap
 * the signed head conditionally. A failure before that last operation leaves
 * every URL in the prior head untouched and its bytes still retrievable. */
export async function publishStream({ origin, row, bundle, compat, app, signer, release, snapshot, opts, log, build, recoveryBuild }) {
  const stream = { channel: row.channel, compatibilityId: row.compatibilityId };
  const base = streamPath(stream);
  const recordPath = `${base}/releases/${release}.json`;
  const index = streamIndex(bundle);
  const files = [{ name: 'app.plan', sha256: bundle.plan.sha256, bytes: bundle.plan.bytes }, ...(index ? [{ name: 'assets.json', sha256: index.card.sha256, bytes: index.bytes }] : []), ...bundle.assets];
  if (await origin.get(recordPath)) refuse(`release ${release} already has an immutable record at ${origin.describe()}/${recordPath}; choose another --release`);
  return origin.withLock(stream, async () => {
    if (await origin.get(recordPath)) refuse(`release ${release} already has an immutable record at ${origin.describe()}/${recordPath}; choose another --release`);
    const current = await origin.head(stream);
    const previousDigest = current?.sha256 ?? null;
    const admission = current ? await admitHead(origin, current, app, stream) : null;
    let cohortCard = null;
    let startingCohort = false;
    let restoreEvidence;
    if (build) {
      const frozen = await recoverCohort(origin, app, stream, admission, recoveryBuild);
      // A concurrent publication can freeze the cohort after classification.
      startingCohort = !current && !frozen;
      const installed = frozen?.cohort ?? (startingCohort ? row.receipt ?? cohortReceipt(build) : null);
      const locked = admission?.authenticated ? admission.head.cohortReceipt : null;
      if (locked && !installed) refuse(`the locked head's cohort receipt could not be verified: ${admission.problem}`);
      const checked = classifyArtifacts(build, installed, {id:signer.keyId,public:app.manifest.deploy?.signing?.keys?.[signer.keyId]});
      if (!checked.bundle || installed.compat.id !== stream.compatibilityId) refuse(`the locked cohort rejects the candidate: ${checked.missing.join('; ')}`);
      const cohort = frozen?.cohortCard ? [] : cohortFiles(installed);
      cohortCard = frozen?.cohortCard ?? cohort[0].card;
      if (locked && !sameCohortPayload(locked, cohortCard)) refuse('the recovered cohort receipt does not match the locked head');
      files.push(...cohort);
      const restoreFrom = startingCohort ? build : recoveryBuild;
      restoreEvidence = (await readBinaryInputFile(origin, installed, restoreFrom))?.restore;
    }
    const changes = admission?.usable
      ? changesAgainst(bundle, admission.head, sunsetFor(app, stream))
      : current ? [{ name: 'exact.json', change: 'repair', note: admission.problem }] : changesAgainst(bundle, null, sunsetFor(app, stream));
    if (admission?.usable && !changes.length && admission.seq > (opts.seqAbove ?? 0)) {
      if (restoreEvidence) await putVerifiedBlob(origin, restoreEvidence);
      const note = restoreEvidence ? 'the head already names this bundle. Restored its signed binary input receipt' : 'the head is admissible and already names this bundle';
      return { ...row, action: 'current', seq: admission.seq, note };
    }
    // `seqAbove`: a stream moving to a new origin starts above every seq its clients accepted from the old one.
    const seq = await nextSeq(origin, app, stream, admission, `the locked head at ${origin.describe()}/${base}/exact.json`, opts.seqAbove ?? 0, { startingCohort });
    const head = streamHead({ app, bundle, stream, seq, release, sunset: sunsetFor(app, stream) });
    if (cohortCard) head.cohortReceipt = cohortCard;
    head.signature = signer.sign(head);
    const bytes = envelopeBytes(head);
    if (restoreEvidence) files.push(restoreEvidence);
    let written = 0;
    for (const file of files) {
      if (await putVerifiedBlob(origin, file) === 'written') written++;
    }
    log(`  ${row.platform} ${row.compatibilityId.slice(0, 8)}: ${files.length} blobs on the origin (${written} written, ${files.length - written} present), each read back and checked`);
    const originDigest = sha256(bytes);
    const entryDigest = sha256(canonicalBytes(head));
    const record = {
      release, at: new Date().toISOString(), by: userInfo().username, host: hostname(),
      platform: row.platform, stream: head.stream, seq, snapshot,
      head: { entryDigest, originDigest, bytes: bytes.length, keyId: signer.keyId }, previous: previousDigest,
      compat: { id: compat.id, inputs: compat.inputs }, row: { action: row.action, changes },
      // The asset index as JSON: its blob is this value's canonical bytes,
      // which a release bake checks against the head (bake/src/receipt.rs).
      envelope: head, ...(index ? { assets: JSON.parse(index.bytes.toString('utf8')) } : {}),
    };
    await origin.put(recordPath, Buffer.from(JSON.stringify(record, null, 2) + '\n', 'utf8'), { immutable: true });
    keepRecord(origin, app, stream, `${release}.json`, seq);
    try {
      await origin.putHead(stream, bytes, { previousDigest });
    } catch (error) {
      let observed;
      try {
        observed = await origin.head(stream);
      } catch (readError) {
        const unknown = new Error(`the head write failed (${error.message}) and its outcome could not be read back (${readError.message})`);
        unknown.headOutcomeUnknown = true;
        throw unknown;
      }
      if (!observed?.bytes.equals(bytes)) throw error;
      log(`  ${row.platform} ${row.compatibilityId.slice(0, 8)}: the head write response failed, but readback confirms seq ${seq}`);
    }
    return { ...row, action: 'published', seq, changes, head: { seq, sha256: originDigest, entryDigest, release }, previous: previousDigest };
  });
}

async function putVerifiedBlob(origin, file) {
  const path = blobPath(file.sha256);
  const status = await origin.put(path, file.bytes, { immutable: true });
  const back = await origin.get(path);
  if (!back || sha256(back) !== file.sha256) refuse(`the blob ${file.sha256} (${file.name}) read back from ${origin.describe()} is not what was written`);
  return status;
}
