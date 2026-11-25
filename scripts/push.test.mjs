import { validateChannel, channelOf, channelOrigin, sha256, blobPath } from './origin.mjs';
import { EXE } from './paths.mjs';
import {expect, mock, test} from 'bun:test';
import * as fs from 'node:fs';
import {tmpdir, homedir} from 'node:os';
import {resolve, relative} from 'node:path';
import {fileURLToPath} from 'node:url';
import {bakedEntryAssets, classifyArtifacts, nativeManifestMetadata} from './app.mjs';
import {spawnSync as realSpawnSync} from 'node:child_process';
import {canonicalBytes, classify} from './deploy.mjs';
import {publishedModule, publishStream, streamHead} from './publisher.mjs';
import {admitHead, authenticatedReleaseFloor, cohortFiles, latestRecord, recoverCohort} from './deploy-heads.mjs';
import {DirectoryOrigin, streamPath} from './origin.mjs';
import {createHash, generateKeyPairSync, sign} from 'node:crypto';
import {filesystem, filesystemRead, filesystemTree} from './filesystem.mjs';
import {streamBundle, unbundledTextures} from './texture-families.mjs';
import {rustAbi, rustBundle, tieredBytes, tieredNative} from './rust.mjs';
import {canonicalJson} from './deploy-signing.mjs';
import {binaryInputFile, cohortReceipt} from './deploy-heads.mjs';
import {binarySourceInputs, buildInputCollector, directoryInputNames, swiftTargetInputs, watchOutput} from './cargo-dep-info.mjs';
import {appleAssetCatalogInventory, launchImageInventory, nativeResourceInventory} from '../host/apple/assets.mjs';
import {notificationServiceInputChanges, notificationServiceSourceRoot} from './notification-service.mjs';
import {addedSwiftPackageInputs, swiftPackageInputChanged} from './package-libraries.mjs';
import {need} from './coverage.mjs';
// The tools these tests run as built (docs/testing.md: a test runs a built binary and builds none), in Cargo's target
// directory, the lane's own if it set one: the reason a test that needs one is skipped here, or null.
const built = (...tools) => {
  const debug = resolve(import.meta.dir, '..', process.env.CARGO_TARGET_DIR || 'target', 'debug');
  const absent = tools.filter(tool => !fs.existsSync(resolve(debug, `${tool}${EXE}`)));
  return absent.length ? `${absent.join(' and ')} not built: cargo build ${absent.map(tool => `-p ${tool}`).join(' ')}` : null;
};

// The publisher's functions with boundary doubles, for an app at a fresh
// root whose cohort's binary has prelude runtime 4 and whose baked module
// needs `runtime` (its receipt's). Bun's native subprocess import bypasses
// module mocks; explicit bindings also isolate deployment tests.
// `entries`: the named entries (name to text) the Rust or TypeScript bake makes beside the plan.
function harness(classify=()=>({bundle:true}), runtime=4, {appBake, bake, graphTool, compatForEnv, cohortRuntime=4, grants='', watch=fs.watch, targets, entries={}, planRequires={}, moduleGrants='', rustModule=true, produced, reuseSignature, graphs, computed=c=>c, bytecodeVersion, openOrigin}={}) {
  const root=fs.mkdtempSync(resolve(tmpdir(),'exact-push-profile-'));
  const app={id:'fixture',name:'fixture',displayName:'Fixture',dir:root,target:root,manifest:{app:{origin:root},deploy:{signing:{keys:{test:'public'}}}}};
  const compat={id:'a'.repeat(32),target:'fixture',inputs:{app:'fixture',platform:'linux',store:{L:'A'},rustMode:'native',grantCeiling:grants,keys:{test:'public'},typescriptRuntime:cohortRuntime}};
  const graph={version:1,sources:{},artifacts:[{name:'app.plan',requires:{sources:{},...planRequires},sha256:sha256(Buffer.from('plan')),bytes:4,kind:'bundle'}]};
  const cohort={version:1,compat,binary:'binary',sources:{}};
  const build={trust:'production',compat,graph};
  // `targets`: a production build for each [target, rustMode], each its own cohort.
  const builds=targets?targets.map(([target,rustMode],i)=>({trust:'production',compat:{...compat,id:String(i).repeat(32),target,inputs:{...compat.inputs,rustMode}},graph})):[build];
  const bundled=[];
  const calls=[], published=[], bundles=[], destinations=[], rows=[];
  const sealed=(name,env)=>{
    calls.push(name);
    expect(env?.EXACT_GAME_PROOF_PROFILE,`${name} shipping profile`).toBe('release');
    expect(env?.EXACT_UPDATE_TRUST,`${name} production trust`).toBe('production');
  };
  const bindings={
    ...Object.fromEntries(['closeSync','existsSync','mkdirSync','openSync','readFileSync','readlinkSync','writeFileSync','mkdtempSync','rmSync'].map(name=>[name,fs[name]])), watch,
    filesystem, filesystemRead, filesystemTree, unbundledTextures, streamBundle,
    nativeManifestMetadata, directoryInputNames, watchOutput, addedSwiftPackageInputs, swiftPackageInputChanged, notificationServiceInputChanges, notificationServiceSourceRoot, appleAssetCatalogInventory, launchImageInventory, nativeResourceInventory,
    channelOf,validateChannel,resolve,relative,homedir,ROOT:fileURLToPath(new URL('..',import.meta.url)),
    process:{env:{EXACT_GAME_PROOF_PROFILE:'gpu-dev',EXACT_UPDATE_TRUST:'development'}},
    console:{log(){}},Refusal:Error,refuse(message){throw Error(message)},
    loadSigner:()=>({keyId:'test'}),resolveApp:()=>app,readBuilds:()=>builds,
    bakedEntryAssets,bakeOutput:()=>resolve(root,'bake'),binaryInputFile,
    // The app's own bake (web, without its wasm): its plan, as the bake output names it.
    buildBake:appBake ?? ((app,platform,target,opts)=>{sealed('app',opts.env);fs.writeFileSync(resolve(opts.output,`${platform}-${target}.plan`),'plan');return {compat:{target,inputs:{platform}},graph:{artifacts:[]}}}),
    cohortReceipt:b=>b===build?cohort:{version:1,compat:b.compat,binary:'binary',sources:{}},classifyArtifacts:classify,
    openOrigin:openOrigin ?? mock(spec=>({writable:true,list:async path=>{destinations.push({origin:spec,path});return []},describe:()=>spec})),
    channelOrigin,sha256,blobPath,streamPath,authenticatedReleaseFloor,recoverCohort,canonicalJson,
    defaultRelease:()=> 'r-fixture',admitHead,publishedModule,
    publishedSignature: reuseSignature ?? (() => null),
    requireReleasePins(){}, // the release pins' gate (game/inputs.mjs), not what this seals
    async publishStream({build,bundle,row}){
      expect(build.trust).toBe('production');calls.push('publish');published.push(build);bundles.push(bundle);rows.push(row);
      return {action:'current',platform:'linux',compatibilityId:compat.id};
    },
    RustBaker:class {constructor(app,env){sealed('resident',env)}close(){calls.push('close')}},
    async buildRust(app,opts){sealed('rust',opts.env);expect(opts.profile).toBe('release');return {plan:Buffer.from('plan'),target:opts.nativeTarget,entries:new Map(Object.entries(entries).map(([name,text])=>[name,Buffer.from(text)])),...produced}},
    rustPackage: () => rustModule,
    rustBundle:(app,bundle,build,built)=>{bundled.push([build.compat.target,built.target]);return produced ? rustBundle(app,bundle,build,built) : {bundle,build}},
    spawnSync(tool,args,opts){
      if(tool==='git')return {status:0,stdout:'b'.repeat(40)};
      const name=tool==='cargo'?'tools':['graph','compat'].includes(args[0])?args[0]:'typescript';
      sealed(name,opts.env);
      // `bake` runs the real one (its receipt's runtime is the bake's own).
      if(['graph','compat'].includes(name)&&graphTool)return realSpawnSync(graphTool,args,{...opts,env:{...process.env,...opts.env}});
      if(name==='graph'){
        fs.writeFileSync(opts.stdio[1],JSON.stringify({...graph,...graphs?.[fs.readFileSync(args[1],'utf8')]}));
        return {status:0};
      }
      // Unchanged sources compute each cohort's own inputs for its target.
      if(name==='compat'){
        const target=args[args.indexOf('--target')+1];
        const base=computed(builds.find(b=>b.compat.target===target)?.compat??compat);
        return {status:0,stdout:JSON.stringify(compatForEnv?.(opts.env,base)??base)};
      }
      if(name==='typescript'&&bake)return realSpawnSync(bake,args,{...opts,env:{...process.env,...opts.env}});
      if(name==='typescript'){
        const out=args[args.indexOf('--out')+1];fs.mkdirSync(out,{recursive:true});
        fs.writeFileSync(resolve(out,'app.plan'),'plan');
        fs.writeFileSync(resolve(out,'app.hbc'),'bytecode');
        // The runtime the bake found the module uses (`typescript_runtime_of`).
        fs.writeFileSync(resolve(out,'app.module.json'),JSON.stringify({appId:'fixture',grants:moduleGrants,typescriptRuntime:runtime,...(bytecodeVersion?{bytecodeVersion}:{})}));
        for(const [name,text] of Object.entries(entries)){fs.mkdirSync(resolve(out,name,'..'),{recursive:true});fs.writeFileSync(resolve(out,name),text);}
      }
      return {status:0,stdout:JSON.stringify(graph)};
    },
  };
  // The publisher's candidates (publisher.mjs, before its publishing section, which the bindings stand in for) and push.
  const source=fs.readFileSync(new URL('./push.mjs',import.meta.url),'utf8');
  const publisher=fs.readFileSync(new URL('./publisher.mjs',import.meta.url),'utf8');
  const functions=(publisher.slice(publisher.indexOf('/** The streams of `channel`'),publisher.indexOf('// ---------------------------------------------------------------- publishing'))
    +source.slice(source.indexOf('function parseArgs('),source.indexOf('\nif (process.argv[1]')))
    .replaceAll('export async function ', 'async function ').replaceAll('export function ', 'function ').replaceAll('export const ', 'const ');
  const pusher=new Function(...Object.keys(bindings),functions+'\nreturn { pusher, streams, streamCandidates, binaryInputChanges };')(...Object.values(bindings));
  return {root,app,build,cohort,calls,published,bundles,bundled,destinations,rows,bindings,...pusher};
}

test.each([
  ['the default selection', ['widgets', 'activities', 'intents', 'alarms', 'notifications', 'watch', 'share']],
  ['--extensions none', []],
])('an unchanged Apple cohort keeps %s current', async (_, appleExtensions) => {
  const { root, app, binaryInputChanges } = harness();
  try {
    app.manifest = { app: { id: 'fixture', name: 'Fixture' } };
    app.target = resolve(root, 'target');
    const compat = { target: 'aarch64-apple-ios', delivery: {}, inputs: { platform: 'ios', store: { L: 'A' }, keys: {} } };
    const evidence = { version: 1, metadata: { ...nativeManifestMetadata(app.manifest, 'ios'), appleExtensions,
      delivery: compat.delivery, store: compat.inputs.store, keys: compat.inputs.keys }, files: [], directories: [], absent: [], links: [] };
    const bytes = Buffer.from(JSON.stringify(evidence));
    expect(binaryInputChanges(bytes, app, compat)).toEqual([]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test("a package's native configuration is a binary input, recorded by digest (LLP 1133.021 §9 item 7)", () => {
  const { root, app, binaryInputChanges } = harness();
  const key = 'push-test-config-sentinel', manifest = { app: { id: 'fixture', name: 'Fixture' }, packages: { '@exact/maps': { android: { apiKey: { env: 'PUSH_TEST_MAPS_KEY' } } } } };
  try {
    app.manifest = manifest;
    app.target = resolve(root, 'target');
    const compat = { target: 'aarch64-linux-android', delivery: {}, inputs: { platform: 'android', store: { L: 'A' }, keys: {} } };
    const evidence = { version: 1, metadata: { ...nativeManifestMetadata(manifest, 'android', compat.target, { PUSH_TEST_MAPS_KEY: key }),
      delivery: compat.delivery, store: compat.inputs.store, keys: compat.inputs.keys }, files: [], directories: [], absent: [], links: [] };
    const bytes = Buffer.from(JSON.stringify(evidence));
    expect(bytes.includes(key)).toBe(false);
    expect(binaryInputChanges(bytes, app, compat, [], evidence, { PUSH_TEST_MAPS_KEY: key })).toEqual([]);
    expect(binaryInputChanges(bytes, app, compat, [], evidence, { PUSH_TEST_MAPS_KEY: `${key}-rotated` })).toEqual(['packages.@exact/maps']);
    expect(binaryInputChanges(bytes, app, compat, [], evidence, {})).toEqual(['packages.@exact/maps']);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test("an app's vendor Info.plist key is a binary input (LLP 1069.008 D4)", () => {
  const { root, app, binaryInputChanges } = harness();
  const manifest = (url) => ({ app: { id: 'fixture', name: 'Fixture' }, host: { macos: { infoPlist: { SUFeedURL: url } } } });
  try {
    app.manifest = manifest('https://example.com/appcast.xml');
    app.target = resolve(root, 'target');
    const compat = { target: 'aarch64-apple-darwin', delivery: {}, inputs: { platform: 'macos', store: { L: 'A' }, keys: {} } };
    const evidence = { version: 1, metadata: { ...nativeManifestMetadata(app.manifest, 'macos'), delivery: compat.delivery, store: compat.inputs.store, keys: compat.inputs.keys },
      files: [], directories: [], absent: [], links: [] };
    const bytes = Buffer.from(JSON.stringify(evidence));
    expect(binaryInputChanges(bytes, app, compat, [], evidence)).toEqual([]);
    app.manifest = manifest('https://example.com/beta.xml');
    expect(binaryInputChanges(bytes, app, compat, [], evidence)).toEqual(['host']);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('notification service cohort evidence ignores hidden entries', async () => {
  const { root, app, binaryInputChanges } = harness();
  const service = resolve(root, 'modules/apple/notification-service');
  try {
    fs.mkdirSync(resolve(service, '.hidden'), { recursive: true });
    fs.writeFileSync(resolve(service, 'NotificationService.swift'), 'class NotificationService {}\n');
    fs.writeFileSync(resolve(service, '.DS_Store'), 'Finder metadata');
    fs.writeFileSync(resolve(service, '.hidden/Ignored.swift'), 'ignored\n');
    app.manifest = { app: { id: 'fixture', name: 'Fixture' } };
    app.target = resolve(root, 'target');
    const compat = { target: 'aarch64-apple-ios', delivery: {}, inputs: { platform: 'ios', store: { L: 'A' }, keys: {} } };
    const source = Buffer.from('class NotificationService {}\n'), size = Buffer.alloc(8); size.writeBigUInt64LE(BigInt(source.length));
    const tree = createHash('sha256').update('NotificationService.swift').update(Buffer.from([0])).update(size).update(source).digest('hex');
    const identity = { tree, files: [{ path: 'NotificationService.swift', sha256: sha256(source) }] };
    const evidence = { version: 1, metadata: { ...nativeManifestMetadata(app.manifest, 'ios'), appleExtensions: ['notifications'],
      delivery: compat.delivery, store: compat.inputs.store, keys: compat.inputs.keys }, files: [], directories: [], absent: [], links: [],
      inventories: [{ kind: 'notification-service', root: 'app', path: 'modules/apple/notification-service', identity }] };
    const bytes = Buffer.from(JSON.stringify(evidence));
    expect(binaryInputChanges(bytes, app, compat)).toEqual([]);
    fs.writeFileSync(resolve(service, '.DS_Store'), 'changed');
    fs.writeFileSync(resolve(service, '.new'), 'new ignored file'); fs.mkdirSync(resolve(service, 'target')); fs.writeFileSync(resolve(service, 'target/Generated.swift'), 'ignored\n');
    expect(binaryInputChanges(bytes, app, compat)).toEqual([]);
    fs.writeFileSync(resolve(service, 'NotificationService.swift'), 'class NotificationService { let changed = true }\n');
    expect(binaryInputChanges(bytes, app, compat)).toEqual(['app/modules/apple/notification-service/NotificationService.swift']);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('stream candidates preserve an excluded cohort notification service selection', async () => {
  const compatForEnv = (env, compat) => ({ ...compat, inputs: { ...compat.inputs,
    notificationService: env.EXACT_APPLE_EXTENSIONS === '' ? null : { tree: 'present', files: ['NotificationService.swift'] } } });
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false, compatForEnv });
  const service = resolve(root, 'modules/apple/notification-service');
  try {
    fs.mkdirSync(service, { recursive: true }); fs.writeFileSync(resolve(service, 'NotificationService.swift'), 'class NotificationService {}\n');
    app.manifest = { app: { id: 'fixture', name: 'Fixture' }, deploy: { signing: { keys: { test: 'public' } } } };
    Object.assign(cohort.compat.inputs, { platform: 'ios', rustMode: 'off', notificationService: null, formatVersion: 4, kernelSchema: '0'.repeat(16), executors: [] });
    const evidence = { version: 1, metadata: { ...nativeManifestMetadata(app.manifest, 'ios'), appleExtensions: [],
      delivery: cohort.compat.delivery, store: cohort.compat.inputs.store, keys: cohort.compat.inputs.keys }, files: [], directories: [], absent: [], links: [] };
    const bytes = Buffer.from(JSON.stringify(evidence)); cohort.binaryInputs = { sha256: sha256(bytes), bytes: bytes.length };
    const origin = { get: async () => bytes };
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const snapshot = { bundle: { plan: { bytes: Buffer.from('plan') }, assets: [] }, compat: cohort.compat, graph: { artifacts: [] } };
    const [candidate] = await streamCandidates(app, origin, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }],
      { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' }, { snapshot });
    expect(candidate.build.pendingInputs).toEqual([]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each([
  { name: 'the only channel', deploy: { channels: { dogfood: 'dogfood-origin' } }, expected: 'dogfood', origin: 'dogfood-origin' },
  { name: 'the only channel before app.origin', deploy: { channels: { dogfood: 'dogfood-origin' } }, fallback: true, expected: 'dogfood', origin: 'dogfood-origin' },
  { name: 'deploy.channel before the only channel', deploy: { channel: 'beta', channels: { dogfood: 'dogfood-origin' } }, fallback: true, expected: 'beta' },
  { name: '--channel before deploy.channel', deploy: { channel: 'beta', channels: { dogfood: 'dogfood-origin' } }, channel: 'dogfood', expected: 'dogfood', origin: 'dogfood-origin' },
  { name: 'prod with no channels', deploy: {}, fallback: true, expected: 'prod' },
  { name: 'prod with several channels', deploy: { channels: { dogfood: 'dogfood-origin', beta: 'beta-origin' } }, fallback: true, expected: 'prod' },
])('push uses $name for stream discovery and publication', async ({ deploy, channel, fallback, expected, origin }) => {
  const { root, app, destinations, rows, pusher } = harness();
  try {
    if (!fallback) delete app.manifest.app.origin;
    Object.assign(app.manifest.deploy, deploy);
    expect(await pusher({ _: [root], channel, keys: root })).toBe(0);
    expect(destinations).toEqual([{ origin: origin ?? root, path: `.exact/${expected}` }]);
    expect(rows.map(row => row.channel)).toEqual([expected]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each([
  { name: 'prod', channels: {} },
  { name: 'the only channel', channels: { dogfood: 'dogfood-origin' } },
])('push rejects an empty deploy.channel before falling back to $name', async ({ channels }) => {
  const { root, app, bindings, pusher } = harness();
  try {
    Object.assign(app.manifest.deploy, { channel: '', channels });
    await expect(pusher({ _: [root], keys: root })).rejects.toThrow('the channel "" cannot name a directory under .exact/');
    expect(bindings.openOrigin).not.toHaveBeenCalled();
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('push seals every Rust, TypeScript and graph bake against an inherited proof profile', async () => {
  const {root,calls,published,bindings,pusher}=harness();
  try {
    for(const typescript of [false,true]){
      if(typescript)fs.writeFileSync(resolve(root,'app.ts'),'');
      calls.length=0;published.length=0;
      expect(await pusher({_:[root],origin:root,keys:root})).toBe(0);
      expect(calls).toEqual(['tools','resident',typescript?'typescript':'rust','compat','graph','publish','close']);
      expect(bindings.process.env).toEqual({EXACT_GAME_PROOF_PROFILE:'gpu-dev',EXACT_UPDATE_TRUST:'development'});
      // A module needs the runtime its receipt names, not the cohort's (LLP 1016.000 D10).
      if(typescript)for(const artifact of published[0].graph.artifacts)expect(artifact.requires.typescriptRuntime).toBe(4);
    }
  } finally {fs.rmSync(root,{recursive:true,force:true})}
});

test('push sends a TypeScript module that uses nothing newer to an older binary', async () => {
  const {root,calls,published,pusher}=harness(classifyArtifacts,4);
  try {
    fs.writeFileSync(resolve(root,'app.ts'),'');
    expect(await pusher({_:[root],origin:root,keys:root})).toBe(0);
    expect(calls).toContain('publish');
    expect(published[0].graph.artifacts.every(a=>a.requires.typescriptRuntime===4)).toBe(true);
  } finally {fs.rmSync(root,{recursive:true,force:true})}
});

// A Rust module is built for each native target a stream's binary runs, whichever comes first, and each stream gets its
// own; one stream that cannot take the update publishes none.
test('push builds a Rust module for each native target and checks every stream before publishing', async () => {
  const x86='x86_64-unknown-linux-gnu', arm='aarch64-unknown-linux-gnu';
  for (const targets of [[[x86,'native'],[arm,'tiered'],['wasm32-unknown-unknown','wasm']],[[arm,'native'],['wasm32-unknown-unknown','wasm'],[x86,'native']]]) {
    const {root,bundled,published,pusher}=harness(undefined,4,{targets});
    try {
      expect(await pusher({_:[root],origin:root,keys:root})).toBe(0);
      const native=targets.filter(([,mode])=>mode!=='wasm').map(([t])=>t);
      expect(bundled.filter(([cohort])=>cohort!=='wasm32-unknown-unknown')).toEqual(native.map(t=>[t,t]));
      expect(new Set(bundled.map(([,built])=>built))).toEqual(new Set(native));
      expect(published.map(b=>b.compat.target)).toEqual(targets.map(([t])=>t));
    } finally {fs.rmSync(root,{recursive:true,force:true})}
  }
  const {root,published,pusher}=harness(build=>build.compat.target===arm?{bundle:false,missing:['rustTarget']}:{bundle:true},4,{targets:[[x86,'native'],[arm,'native']]});
  try {
    await expect(pusher({_:[root],origin:root,keys:root})).rejects.toThrow('cannot take this as an update — rustTarget');
    expect(published).toEqual([]);
  } finally {fs.rmSync(root,{recursive:true,force:true})}
  // LLP 1156.006.000 D19: two streams that cannot take it are both named in the one refusal, each with its build.
  const both=harness(()=>({bundle:false,missing:['rustTarget']}),4,{targets:[[x86,'native'],[arm,'native']]});
  try {
    const said=await both.pusher({_:[both.root],origin:both.root,keys:both.root}).then(()=>'',e=>e.message);
    expect(said).toMatch(/^2 streams on the channel \S+ cannot take this as an update, and nothing is published until each can:\n/);
    expect(said.match(/cannot take this as an update — rustTarget\. It needs /g)).toHaveLength(2);
    expect(both.published).toEqual([]);
  } finally {fs.rmSync(both.root,{recursive:true,force:true})}
  // LLP 1156.006.000 D25: a stream whose binary provably cannot run the update is left and the rest are published;
  // with no stream left nothing is, and a miss that is no proof beside one that is still refuses the push.
  const proof=['app.plan: appleUses.picker (not linked into the installed binary; that needs a new binary)'];
  const unfitArm=(verdict)=>harness(build=>build.compat.target===arm?verdict:{bundle:true},4,{targets:[[x86,'native'],[arm,'native']]});
  const left=unfitArm({bundle:false,missing:proof,needsBinary:proof});
  try {
    expect(await left.pusher({_:[left.root],origin:left.root,keys:left.root})).toBe(0);
    expect(left.published.map(b=>b.compat.target)).toEqual([x86]);
  } finally {fs.rmSync(left.root,{recursive:true,force:true})}
  const unproven=unfitArm({bundle:false,missing:[...proof,'envelope: signing key test is not carried by this cohort'],needsBinary:proof});
  try {
    await expect(unproven.pusher({_:[unproven.root],origin:unproven.root,keys:unproven.root})).rejects.toThrow('is not carried by this cohort. It needs');
    expect(unproven.published).toEqual([]);
  } finally {fs.rmSync(unproven.root,{recursive:true,force:true})}
  const none=harness(()=>({bundle:false,missing:proof,needsBinary:proof}),4,{targets:[[x86,'native'],[arm,'native']]});
  try {
    await expect(none.pusher({_:[none.root],origin:none.root,keys:none.root})).rejects.toThrow(/^2 streams on the channel \S+ cannot take this as an update/);
    expect(none.published).toEqual([]);
  } finally {fs.rmSync(none.root,{recursive:true,force:true})}
});

// An app with widgets or Live Activities: the binary's production bake listed their plans, schemas and, for a TypeScript
// app, the module pair (bake/src/widgets.rs). An update carries the ones push baked, in place of those whole: a changed
// entry's new bytes, an added one, and none removed since.
test('push starts a stream with the named entries and module pair it baked, in place of the binary\'s', async () => {
  for (const typescript of [true, false]) {
    const baked={'plans/departure.plan':'activity plan','plans/arrival.plan':'added plan','activities/departure.schema.json':'{}'};
    const {root,build,published,bundles,pusher}=harness(classifyArtifacts,4,{entries:baked});
    try {
      if(typescript)fs.writeFileSync(resolve(root,'app.ts'),'');
      const installed={'plans/departure.plan':'installed plan','plans/gone.plan':'removed plan','typescript/app.hbc':'installed bytecode'};
      for(const [name,text] of Object.entries(installed))build.graph.artifacts.push({name,sha256:sha256(Buffer.from(text)),bytes:text.length,kind:'bundle',requires:{}});
      expect(await pusher({_:[root],origin:root,keys:root})).toBe(0);
      const names=['activities/departure.schema.json','app.plan','plans/arrival.plan','plans/departure.plan',...(typescript?['typescript/app.hbc','typescript/app.module.json']:[])];
      expect(published[0].graph.artifacts.map(a=>a.name).sort()).toEqual(names);
      expect(bundles[0].assets.map(a=>a.name).sort()).toEqual(names.filter(n=>n!=='app.plan'));
      const bytes=Object.fromEntries(bundles[0].assets.map(a=>[a.name,a.bytes.toString()]));
      expect([bytes['plans/departure.plan'],bytes['plans/arrival.plan'],bytes['typescript/app.hbc']]).toEqual(['activity plan','added plan',typescript?'bytecode':undefined]);
    } finally {fs.rmSync(root,{recursive:true,force:true})}
  }
});

test('push refuses a TypeScript module that needs a newer prelude than the cohort has', async () => {
  const {root,calls,pusher}=harness(classifyArtifacts,5);
  try {
    fs.writeFileSync(resolve(root,'app.ts'),'');
    await expect(pusher({_:[root],origin:root,keys:root})).rejects.toThrow(/typescriptRuntime \(requires a prelude of runtime 5 or later; this cohort's is 4\).*It needs a new binary/);
    expect(calls).not.toContain('publish');
  } finally {fs.rmSync(root,{recursive:true,force:true})}
});

test('refused stream candidates preserve the evidence diagnostic', () => {
  const source = fs.readFileSync(new URL('./push.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('    // Every current stream is checked before any is published.');
  const end = source.indexOf('    const results = [];', start);
  const update = { reason: 'the binary input receipt is missing or does not match', row: { platform: 'ios', compatibilityId: 'a'.repeat(32) } };
  const check = new Function('current', 'classifyArtifacts', 'signingKey', 'refuse', 'app', 'binaryCommand', 'name', source.slice(start, end));
  expect(() => check([update], () => ({ bundle: false, binary: true, missing: ['unrelated'] }), {}, message => { throw Error(message); }, { name: 'fixture' }, () => 'a new binary', row => `${row.platform} ${row.compatibilityId.slice(0, 8)}`))
    .toThrow(/binary input receipt is missing or does not match.*needs a new binary/);
});

// The real bake and classifier: a module that may open a database needs
// runtime 10, as its transaction's options may ask to check foreign keys and an
// older prelude would drop them (bake/src/compat.rs). Here they are data, a
// migration parsed from JSON, and a cohort on runtime 9 is refused.
need('fixture', built('exact-js-bake'))('push refuses a SQLite module whose transaction options are data to a cohort on runtime 9', async () => {
  const ROOT=fileURLToPath(new URL('..',import.meta.url));
  // The bake the lane built (docs/testing.md: a test runs a built binary, it
  // builds none), in Cargo's target directory, the lane's own if it set one.
  const bake=resolve(ROOT,process.env.CARGO_TARGET_DIR||'target',`debug/exact-js-bake${EXE}`);
  const grants='sqlite.open app:/data/a.db';
  const {root,calls,pusher}=harness(classifyArtifacts,undefined,{bake,cohortRuntime:9,grants});
  try {
    fs.writeFileSync(resolve(root,'app.json'),JSON.stringify({name:'fixture',app:{id:'fixture',name:'fixture'}}));
    fs.writeFileSync(resolve(root,'app.contract'),'component App\n  state count = 0\n  resource message = message(count) as shape string\n  view\n    text message\n');
    fs.writeFileSync(resolve(root,'app.ts'),`import type { Sources, Answer } from './app.contract.d.ts';
export const appId = 'fixture';
export const grants = '${grants}';
const migration = JSON.parse('{"commands": [], "options": {"foreign\\\\u004beys": "check"}}');
const sources: Sources = { message: ([count], _store, storage) => storage.sqlite.open('app:/data/a.db').then(db => db.transaction(migration.commands, migration.options)).then(() => String(count), () => String(count)) };
export const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);
`);
    await expect(pusher({_:[root],origin:root,keys:root})).rejects.toThrow(/typescriptRuntime \(requires a prelude of runtime 10 or later; this cohort's is 9\).*It needs a new binary/);
    expect(calls).not.toContain('publish');
  } finally {fs.rmSync(root,{recursive:true,force:true})}
}, 120000);

test('push --watch publishes after a save to what a push reads, and not to what a tool writes', async () => {
  let listener;
  const { root, rows, pusher } = harness(undefined, 4, { watch: (dir, options, saved) => { listener = saved; } });
  const settle = () => new Promise(ok => setTimeout(ok, 200));
  try {
    pusher({ _: [root], keys: root, watch: true });
    while (!listener) await settle();
    const pushes = async (file) => { const before = rows.length; listener('change', file); await settle(); return rows.length - before; };
    // The design, and an asset or a deck's file of any type (`carriedAssets`)
    for (const file of ['app.contract', 'app.forma', 'assets/logo.png', 'assets/Inter.ttf', 'assets/chime.wav', 'assets/clip.mov', 'deck/deck.js']) {
      expect([file, await pushes(file)]).toEqual([file, 1]);
    }
    // What the app's own state, a build or the Finder writes
    for (const file of ['.exact/sent.json', '.exact/shells/Cargo.toml', 'target/debug/x.rs', 'assets/.DS_Store', 'notes.txt']) {
      expect([file, await pushes(file)]).toEqual([file, 0]);
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('push refuses incomplete values and reserved channels before publishing', () => {
  const source = fs.readFileSync(new URL('./push.mjs', import.meta.url), 'utf8');
  const parse = new Function('refuse', 'USAGE', source.slice(source.indexOf('function parseArgs('), source.indexOf('\nconst seconds')) + ';return parseArgs;')((message) => { throw Error(message); }, 'usage');
  for (const flag of ['--origin', '--channel', '--keys', '--seq-above']) {
    expect(() => parse(['app', flag])).toThrow();
    expect(() => parse(['app', flag, '--watch'])).toThrow();
  }
  for (const value of ['-1', 'NaN', '1.5', '9007199254740991']) expect(() => parse(['app', '--seq-above', value])).toThrow();
  expect(() => parse(['app', 'extra'])).toThrow();
  expect(parse(['app', '--origin', '/tmp/staging', '--seq-above', '7'])).toEqual({ _: ['app'], origin: '/tmp/staging', seqAbove: 7 });
  for (const name of ['blobs', 'BLOBS', 'root', '.', '..', 'a/b']) expect(() => validateChannel(name)).toThrow();
  expect(validateChannel('prod')).toBe('prod');
});

test('push inventories current app files and carries only head-owned artifacts', async () => {
  const root = fs.mkdtempSync(resolve(tmpdir(), 'exact-push-assets-'));
  try {
    fs.mkdirSync(resolve(root, 'assets'));
    fs.writeFileSync(resolve(root, 'assets/new.png'), 'new');
    fs.mkdirSync(resolve(root, 'deck/nested'), { recursive: true });
    fs.writeFileSync(resolve(root, 'deck/nested/.config'), 'config');
    for (const folder of ['assets', 'deck', 'deck/nested']) fs.writeFileSync(resolve(root, folder, '.DS_Store'), 'Bud1');
    const shader = Buffer.from('shader');
    const source = fs.readFileSync(new URL('./publisher.mjs', import.meta.url), 'utf8');
    const named = source.indexOf('const namedEntry');
    const fn = `${source.slice(named, source.indexOf('\n', named))}\n${source.slice(source.indexOf('async function carriedAssets('), source.indexOf('\n/** The command that builds an APK'))}`;
    const carried = new Function('filesystem', 'filesystemRead', 'unbundledTextures', 'resolve', 'sha256', 'blobPath', 'refuse', fn + ';return carriedAssets;')(filesystem, filesystemRead, unbundledTextures, resolve, sha256, blobPath, (message) => { throw Error(message); });
    const asked = [];
    const assets = await carried({ get: async (path) => { asked.push(path); return shader; } }, { dir: root }, { assets: [
      { name: 'assets/deleted.png', sha256: 'deleted' }, { name: 'shaders/x.wgsl', sha256: sha256(shader) },
      // The named entries are push's own bake's.
      { name: 'plans/widget.plan', sha256: 'old widget' }, { name: 'activities/trip.schema.json', sha256: 'old schema' },
    ] });
    expect(assets.map(a => a.name)).toEqual(['assets/new.png', 'deck/nested/.config', 'shaders/x.wgsl']);
    expect(assets.map(a => a.bytes.toString())).toEqual(['new', 'config', 'shader']);
    expect(assets.every(a => a.sha256 === sha256(a.bytes))).toBe(true);
    // A baked texture's three files: a cohort baked with ASTC alone is sent its ASTC payload. An iOS cohort
    // baked before thinning names no family, and its renderer falls back to the RGBA8 file: it is sent all three.
    for (const name of ['fox.tex', 'fox.bc.tex', 'fox.astc.tex']) fs.writeFileSync(resolve(root, 'assets', name), name);
    const textures = async (inputs) => (await carried({ get: async () => shader }, { dir: root }, {}, inputs)).map(a => a.name).filter(n => n.endsWith('.tex')).sort();
    expect(await textures({ platform: 'ios', textureFamily: 'astc' })).toEqual(['assets/fox.astc.tex']);
    expect(await textures({ platform: 'ios' })).toEqual(['assets/fox.astc.tex', 'assets/fox.bc.tex', 'assets/fox.tex']);
    expect(await textures({ platform: 'linux' })).toEqual(['assets/fox.astc.tex', 'assets/fox.bc.tex', 'assets/fox.tex']);
    expect(asked).toEqual([blobPath(sha256(shader))]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each([
  { name: 'an asset tree', count: 4, mib: 100 },
  { name: 'one asset', count: 1, mib: 400 },
])('push reads $name larger than one filesystem reply', async ({ count, mib }) => {
  const root = fs.mkdtempSync(resolve(import.meta.dir, '../target/push-large-assets-'));
  try {
    fs.mkdirSync(resolve(root, 'assets'));
    // The base64 payload exceeds the synchronous helper's 512 MiB reply limit.
    const bytes = Buffer.alloc(mib * 1024 * 1024, 97), digest = sha256(bytes);
    const names = Array.from({ length: count }, (_, i) => `assets/${i}.bin`);
    for (const name of names) fs.writeFileSync(resolve(root, name), bytes);
    const source = fs.readFileSync(new URL('./publisher.mjs', import.meta.url), 'utf8');
    const named = source.indexOf('const namedEntry');
    const fn = `${source.slice(named, source.indexOf('\n', named))}\n${source.slice(source.indexOf('async function carriedAssets('), source.indexOf('\n/** The command that builds an APK'))}`;
    const carried = new Function('filesystem', 'filesystemRead', 'unbundledTextures', 'resolve', 'sha256', 'blobPath', 'refuse', fn + ';return carriedAssets;')(filesystem, filesystemRead, unbundledTextures, resolve, sha256, blobPath, (message) => { throw Error(message); });
    const assets = await carried({}, { dir: root }, {});
    expect(assets.map(asset => asset.name)).toEqual(names);
    for (const asset of assets) {
      expect(asset.bytes.length).toBe(bytes.length);
      expect(asset.sha256).toBe(digest);
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}, 60000);

const bundleCandidates = async real => {
  const target = resolve(import.meta.dir, '..', process.env.CARGO_TARGET_DIR || 'target', 'debug');
  const bake = real ? resolve(target, `exact-js-bake${EXE}`) : null;
  const graphTool = real ? resolve(target, `exact-bake${EXE}`) : null;
  const { root, app, cohort, streams, streamCandidates } = harness(classifyArtifacts, 4, { bake, graphTool });
  try {
    fs.writeFileSync(resolve(root, 'app.ts'), '');
    if (real) {
      fs.writeFileSync(resolve(root, 'app.json'), JSON.stringify({ name: 'fixture', app: { id: 'fixture', name: 'Fixture' } }));
      fs.writeFileSync(resolve(root, 'app.contract'), 'component App\n  state count = 0\n  resource message = message(count) as shape string\n  view\n    text message\n');
      fs.writeFileSync(resolve(root, 'app.ts'), "export const appId = 'fixture';\nexport const grants = '';\nexport const answer = () => 'updated';\n");
      const baked = resolve(root, 'initial-bake');
      const production = { ...process.env, EXACT_UPDATE_TRUST: 'production' };
      const result = realSpawnSync(bake, [root, '--out', baked], { env: production, encoding: 'utf8' });
      expect(result.status, result.stderr).toBe(0);
      const compatFile = resolve(root, 'compat.json'); fs.writeFileSync(compatFile, JSON.stringify(cohort.compat));
      const graph = realSpawnSync(graphTool, ['graph', resolve(baked, 'app.plan'), compatFile, cohort.compat.target, root], { env: production, encoding: 'utf8' });
      expect(graph.status, graph.stderr).toBe(0);
      const requires = JSON.parse(graph.stdout).artifacts[0].requires;
      for (const key of ['formatVersion', 'formatDigest', 'kernelSchema']) cohort.compat.inputs[key] = requires[key];
    }
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.displayName = 'Fixture';
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    Object.assign(cohort.compat.inputs, { platform: 'ios', rustMode: 'off', keys: { test: raw }, formatVersion: cohort.compat.inputs.formatVersion ?? 4, kernelSchema: cohort.compat.inputs.kernelSchema ?? '0'.repeat(16), executors: [] });
    if (real) {
      // The installed binary was built from these sources: its cohort's inputs are the ones they compute.
      fs.writeFileSync(resolve(root, 'app.json'), JSON.stringify({ name: 'fixture', app: { id: 'fixture', name: 'Fixture' }, deploy: { signing: { key: 'test', keys: { test: raw } } } }));
      const computed = realSpawnSync(graphTool, ['compat', root, '--platform', 'ios', '--target', cohort.compat.target, '--json', '--grants', ''], { env: { ...process.env, EXACT_UPDATE_TRUST: 'production' }, encoding: 'utf8' });
      expect(computed.status, computed.stderr).toBe(0);
      cohort.compat.inputs = JSON.parse(computed.stdout).inputs;
    }
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const initialBundle = { plan: { bytes: Buffer.from('old plan'), sha256: sha256(Buffer.from('old plan')), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] };
    const initial = streamHead({ app, bundle: initialBundle, stream, seq: 7, release: 'initial' });
    initial.cohort = cohort; initial.signature = signer.sign(initial);
    await origin.putHead(stream, Buffer.from(JSON.stringify(initial)), { previousDigest: null });
    await origin.put(`${streamPath(stream)}/releases/initial.json`, Buffer.from(JSON.stringify({ envelope: initial })), { immutable: true });
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const found = await streams(origin, app, 'prod', { deploying: true, platforms: ['ios'] });
    expect(found.map(s => s.platform)).toEqual(['ios']);
    const updates = await streamCandidates(app, origin, found, env);
    const candidates = new Map(updates.map(update => [update.row.compatibilityId, update]));
    const args = { app, origin, opts: { platform: ['ios'] }, channel: 'prod', snapshot: { commit: 'a'.repeat(40), dirty: false, changes: [] },
      release: 'update', bundle: initialBundle, compat: {}, builds: {}, platforms: ['ios'], wantOrigin: false, candidates, knownStreams: found };
    const table = await classify(args);
    expect(table.rows.map(row => row.action), JSON.stringify(table.rows)).toEqual(['bundle']);
    const update = updates[0], row = table.rows[0];
    const result = await publishStream({ ...update, origin, app, signer, row, release: 'update', snapshot: table.snapshot, opts: {}, log() {} });
    expect(result.seq).toBe(8);
    const current = await origin.head(stream);
    expect(current.json.cohort).toBeUndefined();
    expect(await admitHead(origin, current, app, stream)).toMatchObject({ authenticated: true, cohort });
    expect((await classify({ ...args, release: 'unchanged' })).rows[0].action).toBe('current');
    const before = (await origin.head(stream)).bytes;
    expect((await publishStream({ ...update, origin, app, signer, row, release: 'unchanged', snapshot: table.snapshot, opts: {}, log() {} })).action).toBe('current');
    expect((await origin.head(stream)).bytes).toEqual(before);
    expect(await origin.get(`${streamPath(stream)}/releases/unchanged.json`)).toBeNull();

    const receiptPath = blobPath(current.json.cohortReceipt.sha256);
    const receiptBytes = await origin.get(receiptPath);
    fs.unlinkSync(origin.path(receiptPath));
    const unrelated = { ...initial, stream: { ...initial.stream, seq: 6 }, cohort: { ...cohort, binary: 'different' } };
    unrelated.signature = signer.sign(unrelated);
    const oldPath = `${streamPath(stream)}/releases/initial.json`;
    const otherPath = `${streamPath(stream)}/releases/unrelated.json`;
    await origin.put(otherPath, Buffer.from(JSON.stringify({ envelope: unrelated })), { immutable: true });
    const oldRecord = await origin.get(oldPath);
    fs.unlinkSync(origin.path(oldPath));
    expect(await latestRecord(origin, app, stream)).toMatchObject({ cohort: null, envelope: { stream: { seq: 8 } } });
    await origin.put(oldPath, oldRecord, { immutable: true });
    expect(await latestRecord(origin, app, stream)).toMatchObject({ cohort, envelope: { stream: { seq: 8 } } });
    const recovery = await streams(origin, app, 'prod', { deploying: true, platforms: ['ios'] });
    expect(recovery[0]?.cohort).toEqual(cohort);
    const [repaired] = await streamCandidates(app, origin, recovery, env, { snapshot: { bundle: update.bundle } });
    const repair = (await classify({ ...args, knownStreams: recovery, release: 'repair',
      candidates: new Map([[stream.compatibilityId, repaired]]) })).rows[0];
    expect(repair).toMatchObject({ action: 'bundle', seq: 9 });
    expect(await publishStream({ ...repaired, origin, app, signer, row: repair, release: 'repair', snapshot: table.snapshot, opts: {}, log() {} }))
      .toMatchObject({ action: 'published', seq: 9 });
    expect(await origin.get(receiptPath)).toEqual(receiptBytes);
    expect(await admitHead(origin, await origin.head(stream), app, stream)).toMatchObject({ usable: true, authenticated: true, cohort });
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
};
// Real signatures, origin, classification and locked publication; only the compilers are boundary doubles.
test.each([false])('bundle candidates publish to signed iOS cohorts and leave unchanged heads alone (real compilers: %s)', bundleCandidates);
need('fixture', built('exact-js-bake', 'exact-bake')).each([true])('bundle candidates publish to signed iOS cohorts and leave unchanged heads alone (real compilers: %s)', bundleCandidates);

need('fixture', built('contract', 'exact-bake'))('a first push builds and publishes a plan with 137 native sources', async () => {
  const target = resolve(import.meta.dir, '..', process.env.CARGO_TARGET_DIR || 'target', 'debug');
  const compiler = resolve(target, `contract${EXE}`), graphTool = resolve(target, `exact-bake${EXE}`);
  const compile = (app, file, env) => {
    const baked = realSpawnSync(compiler, ['build', resolve(app.dir, 'app.contract'), '-o', file], { env, encoding: 'utf8' });
    expect(baked.status, baked.stderr).toBe(0);
  };
  const { root, app, build, cohort, streams, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false, graphTool,
    appBake(app, platform, target, opts) {
      compile(app, resolve(opts.output, `${platform}-${target}.plan`), opts.env);
      return { compat: { target, inputs: { platform } }, graph: { artifacts: [] } };
    },
  });
  try {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest = { name: 'fixture', app: { id: 'fixture', name: 'Fixture' }, deploy: { signing: { key: 'test', keys: { test: raw } } } };
    fs.writeFileSync(resolve(root, 'app.json'), JSON.stringify(app.manifest));
    const shapes = ['shape S0\n  leaf: string\n'];
    for (let level = 1; level <= 6; level++) shapes.push(`shape S${level}\n  left: S${level - 1}\n  right: S${level - 1}\n`);
    const sources = Array.from({ length: 137 }, (_, i) => `  resource value${i} = source${i}() as shape S6\n`);
    const contract = shapes.join('\n') + 'component App\n' + sources.join('') + '  view\n    text "Before"\n';
    const entry = resolve(root, 'app.contract');
    fs.writeFileSync(entry, contract);
    const env = { ...process.env, EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const initial = resolve(root, 'initial.plan');
    compile(app, initial, env);
    const computed = realSpawnSync(graphTool, ['compat', root, '--platform', 'ios', '--target', 'aarch64-apple-ios', '--json', '--grants', ''], { env, encoding: 'utf8' });
    expect(computed.status, computed.stderr).toBe(0);
    build.compat = JSON.parse(computed.stdout);
    const compatFile = resolve(root, 'compat.json');
    fs.writeFileSync(compatFile, computed.stdout);
    const installed = realSpawnSync(graphTool, ['graph', initial, compatFile, build.compat.target, root],
      { env, encoding: 'utf8', maxBuffer: 4 * 1024 * 1024 });
    expect(installed.status, installed.error?.message ?? installed.stderr).toBe(0);
    expect(Buffer.byteLength(installed.stdout)).toBeGreaterThan(1024 * 1024);
    build.graph = JSON.parse(installed.stdout);
    expect(Object.keys(build.graph.sources)).toHaveLength(137);
    Object.assign(build, { version: 1, binary: { sha256: '0'.repeat(64) } });
    Object.assign(cohort, cohortReceipt(build));
    fs.writeFileSync(entry, contract.replace('Before', 'After'));
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const [candidate] = await streamCandidates(app, origin, await streams(origin, app, 'prod', { builds: [build] }), env);
    expect(candidate.reason).toBeUndefined();
    expect(await publishStream({ ...candidate, origin, app, signer, release: 'first', snapshot: {}, opts: {}, log() {} }))
      .toMatchObject({ action: 'published', seq: 1 });
    const stream = { channel: 'prod', compatibilityId: build.compat.id };
    const head = await origin.head(stream);
    expect(head.bytes.length).toBeLessThan(2048);
    expect(await admitHead(origin, head, app, stream)).toMatchObject({ usable: true, authenticated: true, cohort });

    const receiptPath = blobPath(head.json.cohortReceipt.sha256);
    const receiptBytes = await origin.get(receiptPath);
    fs.unlinkSync(origin.path(receiptPath));
    const newBinary = { ...build, binary: { ...build.binary, sha256: '1'.repeat(64) } };
    const discovery = { deploying: true, platforms: ['ios'] };
    expect((await streams(origin, app, 'prod', { ...discovery, builds: [newBinary] }))[0].cohort).toBeUndefined();
    const knownStreams = await streams(origin, app, 'prod', { ...discovery, builds: [build] });
    const [recovered] = await streamCandidates(app, origin, knownStreams.filter(s => s.cohort), env, { snapshot: { bundle: candidate.bundle } });
    const table = await classify({ app, origin, opts: { platform: ['ios'] }, channel: 'prod', snapshot: {}, release: 'repair',
      bundle: candidate.bundle, compat: {}, builds: {}, platforms: ['ios'], wantOrigin: false, knownStreams,
      candidates: new Map(recovered ? [[stream.compatibilityId, recovered]] : []) });
    expect(table.rows[0]).toMatchObject({ action: 'bundle', seq: 2 });
    const repair = { ...recovered, origin, app, signer, row: table.rows[0], release: 'repair',
      snapshot: {}, opts: {}, log() {}, recoveryBuild: build };
    const otherHead = { ...head.json, stream: { ...head.json.stream, seq: 2 },
      cohortReceipt: { ...head.json.cohortReceipt, sha256: 'f'.repeat(64) } };
    otherHead.signature = signer.sign(otherHead);
    const raced = Object.create(origin);
    raced.head = async () => ({ bytes: Buffer.from(JSON.stringify(otherHead)), json: otherHead });
    await expect(publishStream({ ...repair, origin: raced })).rejects.toThrow("the locked head's cohort receipt could not be verified");
    expect(await origin.get(`${streamPath(stream)}/releases/repair.json`)).toBeNull();
    expect(await publishStream(repair)).toMatchObject({ action: 'published', seq: 2 });
    expect(await origin.get(receiptPath)).toEqual(receiptBytes);
    expect(await admitHead(origin, await origin.head(stream), app, stream)).toMatchObject({ usable: true, cohort });

    fs.unlinkSync(origin.path(receiptPath));
    fs.unlinkSync(origin.path(`${streamPath(stream)}/exact.json`));
    expect((await streams(origin, app, 'prod', { ...discovery, builds: [newBinary] }))[0].cohort).toBeUndefined();
    const fromHistory = await streams(origin, app, 'prod', { ...discovery, builds: [build] });
    const [historyCandidate] = await streamCandidates(app, origin, fromHistory.filter(s => s.cohort), env, { snapshot: { bundle: candidate.bundle } });
    const historyTable = await classify({ app, origin, opts: { platform: ['ios'] }, channel: 'prod', snapshot: {}, release: 'restore',
      bundle: candidate.bundle, compat: {}, builds: {}, platforms: ['ios'], wantOrigin: false, knownStreams: fromHistory,
      candidates: new Map(historyCandidate ? [[stream.compatibilityId, historyCandidate]] : []) });
    expect(historyTable.rows[0]).toMatchObject({ action: 'bundle', seq: 3 });
    expect(await publishStream({ ...repair, ...historyCandidate, row: historyTable.rows[0], release: 'restore' }))
      .toMatchObject({ action: 'published', seq: 3 });
    expect(await origin.get(receiptPath)).toEqual(receiptBytes);
    expect(await admitHead(origin, await origin.head(stream), app, stream)).toMatchObject({ usable: true, cohort });
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}, 30_000);

test.each([
  { name: 'a newer runtime', runtime: 5, reason: 'typescriptRuntime' },
  { name: 'different grants', moduleGrants: 'fetch https://example.test', reason: 'different grants' },
  { name: 'an unlinked executor', planRequires: { executors: ['new-executor'] }, reason: 'executors.new-executor' },
  { name: 'a new shader interface', shader: true, reason: 'gpuSurfaces.changed' },
  { name: 'an uncarried signing key', key: true, reason: 'signing key test' },
])('deploy reports $name as needing a binary without publishing', async ({ runtime = 4, moduleGrants, planRequires, shader, key, reason }) => {
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, runtime, { moduleGrants, planRequires });
  try {
    fs.writeFileSync(resolve(root, 'app.ts'), '');
    cohort.compat.inputs.platform = 'ios';
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    cohort.compat.inputs.keys = key ? {} : { test: raw };
    const head = streamHead({ app, bundle: { plan: { sha256: '0'.repeat(64), bytes: Buffer.from('old'), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] }, stream, seq: 1, release: 'old' });
    head.cohort = cohort;
    head.signature = { keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') };
    const bytes = Buffer.from(JSON.stringify(head));
    const origin = { kind: 'directory', describe: () => root, list: async () => [], get: async () => null, head: async () => ({ json: head, bytes, sha256: sha256(bytes) }) };
    const asset = { name: 'shaders/changed.wgsl', bytes: Buffer.from('changed'), sha256: sha256(Buffer.from('changed')) };
    const snapshot = { bundle: { plan: { bytes: Buffer.from('plan') }, assets: shader ? [asset] : [] }, graph: { artifacts: shader ? [{ name: asset.name, requires: { gpuSurfaces: [{ name: 'changed', interface: 'new' }] } }] : [] } };
    const updates = await streamCandidates(app, origin, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }], { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' }, { snapshot });
    const table = await classify({ app, origin, opts: {}, channel: 'prod', snapshot: { dirty: false, changes: [] }, release: 'refused', bundle: snapshot.bundle,
      compat: { ios: cohort.compat }, builds: {}, platforms: ['ios'], wantOrigin: false, candidates: new Map(updates.map(u => [u.row.compatibilityId, u])) });
    expect(table.rows[0].action).toBe('binary');
    expect(table.rows[0].reason).toContain(reason);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each(['missing', 'tampered'])('deploy and push reject a %s cohort receipt', async kind => {
  const { root, app, cohort, streams } = harness(classifyArtifacts);
  try {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.displayName = 'Fixture'; app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    cohort.compat.inputs.platform = 'ios';
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const bundle = { plan: { bytes: Buffer.from('old'), sha256: sha256(Buffer.from('old')), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] };
    const head = streamHead({ app, bundle, stream, seq: 7, release: 'old' });
    if (kind !== 'missing') head.cohort = cohort;
    head.signature = { keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') };
    if (kind === 'tampered') head.cohort.compat.inputs.typescriptRuntime = 100;
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    await origin.putHead(stream, Buffer.from(JSON.stringify(head)), { previousDigest: null });
    const found = await streams(origin, app, 'prod', { deploying: true, platforms: ['ios'] });
    expect(found).toHaveLength(1);
    expect(found[0].cohort).toBeUndefined();
    const table = await classify({ app, origin, opts: {}, channel: 'prod', snapshot: { dirty: false, changes: [] }, release: 'refused', bundle,
      compat: {}, builds: {}, platforms: ['ios'], wantOrigin: false, knownStreams: found });
    expect(table.rows.map(row => row.action)).toEqual(['binary']);
    await expect(streams(origin, app, 'prod')).rejects.toThrow(kind === 'missing' ? 'no cohort receipt' : 'does not verify');
    expect((await origin.head(stream)).bytes).toEqual(Buffer.from(JSON.stringify(head)));
    expect((await origin.list(`${streamPath(stream)}/releases`)) ?? []).toEqual([]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('a declared stream with no head or history remains new, while invalid history is retained', async () => {
  const { root, app, streams } = harness(classifyArtifacts);
  try {
    const stream = { channel: 'prod', compatibilityId: 'b'.repeat(32) };
    app.manifest.deploy.streams = [stream];
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    expect(await streams(origin, app, 'prod', { deploying: true, platforms: ['ios'] })).toEqual([]);
    await origin.put(`${streamPath(stream)}/releases/invalid.json`, Buffer.from('{}'));
    const found = await streams(origin, app, 'prod', { deploying: true, platforms: ['ios'] });
    expect(found).toHaveLength(1);
    expect(found[0].cohort).toBeUndefined();
    expect(found[0].problem).toContain('not admissible');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});


test('a Contract-only deploy checks new source shapes against the installed native sources', async () => {
  const shape = { params: [], result: 'String' };
  const { root, app, cohort, streamCandidates, calls } = harness(classifyArtifacts, 4, { rustModule: false, planRequires: { sources: { added: shape } } });
  try {
    cohort.compat.inputs.rustMode = 'off';
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const snapshot = { bundle: { plan: { bytes: Buffer.from('plan') }, assets: [] }, graph: { artifacts: [] } };
    const [update] = await streamCandidates(app, {}, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }], { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' }, { snapshot });
    const key = { id: 'test', public: 'public' };
    expect(classifyArtifacts(update.build, cohort, key).missing).toContain('app.plan: sources.added (missing source or different parameter/result shape)');
    cohort.sources.added = shape;
    expect(classifyArtifacts(update.build, cohort, key).bundle).toBe(true);
    expect(calls).toEqual(['tools', 'compat', 'graph']);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('a TypeScript pair currently removes the plan source-shape requirement', async () => {
  const shape = { params: [], result: '{ frozen: Bool }' };
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, {
    planRequires: { sources: { exactPage: shape } },
  });
  try {
    fs.writeFileSync(resolve(root, 'app.ts'), '');
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const [update] = await streamCandidates(
      app,
      {},
      [{ stream, head: { assets: [] }, cohort, platform: 'ios' }],
      { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' },
    );
    const plan = update.build.graph.artifacts.find(artifact => artifact.name === 'app.plan');
    expect(plan.requires.sources).toBeUndefined();
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test.each([
  { scope: null, moduleGrants: '', allowed: false },
  { scope: null, moduleGrants: 'net.fetch https://api.example', allowed: true },
  { scope: null, moduleGrants: 'net.fetch https://other.example', allowed: false },
  { scope: '', moduleGrants: '', allowed: true },
  { scope: '', moduleGrants: 'net.fetch https://api.example', allowed: false },
  { scope: null, moduleGrants: 'net.fetch https://api.example', ceiling: 'net.fetch https://api.example\nnet.fetch https://other.example', allowed: true },
  { scope: null, sourceGrants: 'net.fetch https://api.example\nnet.fetch https://other.example', moduleGrants: 'net.fetch https://other.example\nnet.fetch https://api.example', ceiling: 'net.fetch https://api.example\nnet.fetch https://other.example', allowed: false },
  { scope: null, sourceGrants: 'net.fetch https://api.example', moduleGrants: 'net.fetch https://api.example', ceiling: '', allowed: false },
  { scope: null, sourceGrants: null, moduleGrants: 'net.fetch https://api.example', allowed: true },
])('a TypeScript candidate checks its source grants within a declared-fetch ceiling (%j)', async ({ scope, sourceGrants = 'net.fetch https://api.example', ceiling = 'net.fetch https://api.example', moduleGrants, allowed }) => {
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, { grants: ceiling, moduleGrants });
  try {
    fs.writeFileSync(resolve(root, 'app.ts'), '');
    cohort.compat.inputs.javascriptGrants = scope;
    cohort.compat.inputs.rustGrants = sourceGrants;
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const [update] = await streamCandidates(app, {}, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }], { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' });
    const checked = update.reason ? { bundle: false } : classifyArtifacts(update.build, cohort, { id: 'test', public: 'public' });
    expect(checked.bundle).toBe(allowed);
    if (allowed) {
      const pair = update.build.graph.artifacts.filter(a => a.name.startsWith('typescript/'));
      expect(pair.map(a => a.requires.grantCeiling)).toEqual([moduleGrants, moduleGrants]);
      const key = scope !== null ? 'javascriptGrants' : sourceGrants !== null ? 'rustGrants' : 'grantCeiling';
      expect(pair.map(a => a.requires[key])).toEqual([moduleGrants, moduleGrants]);
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each([null, 'astc', 'bc'])('a snapshot candidate keeps the installed texture family %s without a native bake', async family => {
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false });
  try {
    Object.assign(cohort.compat.inputs, { platform: 'ios', rustMode: 'off', textureFamily: family });
    const names = ['assets/fox.model', 'assets/fox/0.tex', 'assets/fox/0.astc.tex', 'assets/fox/0.bc.tex'];
    const assets = names.map(name => ({ name, bytes: Buffer.from(name), sha256: sha256(Buffer.from(name)) }));
    const snapshot = { bundle: { plan: { bytes: Buffer.from('plan') }, assets }, graph: { artifacts: [] } };
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const [update] = await streamCandidates(app, {}, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }], { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' }, { snapshot });
    const expected = family ? ['assets/fox.model', `assets/fox/0.${family}.tex`] : names;
    expect(update.bundle.assets.map(a => a.name)).toEqual(expected);
    expect(update.build.graph.artifacts.map(a => a.name)).toEqual(['app.plan', ...expected]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each(['native', 'tiered'])('an unchanged macOS %s candidate reuses its authenticated signature before bundling', async mode => {
  const wasm = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]);
  const previous = Buffer.from('code signed yesterday'), fresh = Buffer.from('code signed today');
  const produced = { variants: { wasm: { bytes: wasm, receipt: { target: 'wasm32-unknown-unknown' } }, native: { bytes: fresh, receipt: { target: 'aarch64-apple-darwin' } } } };
  const reuseSignature = mock((now, before) => before?.equals(previous) && now.equals(fresh) ? previous : null);
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, { produced, reuseSignature });
  try {
    Object.assign(cohort.compat, { target: 'aarch64-apple-darwin' });
    Object.assign(cohort.compat.inputs, { platform: 'macos', rustMode: mode, rustGrants: '', rustAbi: rustAbi(), rustTarget: cohort.compat.target, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    cohort.compat.inputs.keys = { test: raw };
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const found = [{ stream, head: { assets: [] }, cohort, platform: 'macos' }];
    const snapshot = { compat: cohort.compat, bundle: { plan: { bytes: Buffer.from('plan') }, assets: [] }, graph: { artifacts: [] } };
    produced.variants.native.bytes = previous;
    const [initial] = await streamCandidates(app, origin, found, env, { snapshot });
    await publishStream({ ...initial, origin, app, signer, release: 'initial', snapshot: {}, opts: {}, log() {} });
    produced.variants.native.bytes = fresh;
    const [update] = await streamCandidates(app, origin, found, env, { snapshot });
    const image = update.bundle.assets.find(a => /\.(bin|dylib)$/.test(a.name));
    expect(mode === 'tiered' ? tieredNative(image.bytes) : image.bytes).toEqual(previous);
    expect(image.bytes).toEqual(mode === 'tiered' ? tieredBytes(wasm, previous) : previous);
    const before = (await origin.head(stream)).bytes;
    expect((await publishStream({ ...update, origin, app, signer, release: 'unchanged', snapshot: {}, opts: {}, log() {} })).action).toBe('current');
    expect((await origin.head(stream)).bytes).toEqual(before);
    expect(await origin.get(`${streamPath(stream)}/releases/unchanged.json`)).toBeNull();
    expect(reuseSignature).toHaveBeenCalledTimes(2);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

need('fixture', built('exact-js-bake', 'exact-bake')).each(['minimumOS', 'grantPurposes', 'localizations', 'orientation', 'shortcuts', null])('a snapshot candidate derives current inputs without an Apple build (changed: %s)', async changed => {
  const target = resolve(import.meta.dir, '..', process.env.CARGO_TARGET_DIR || 'target', 'debug');
  const bake = resolve(target, `exact-js-bake${EXE}`), graphTool = resolve(target, `exact-bake${EXE}`);
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, { bake, graphTool });
  try {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest = { app: { id: 'fixture', name: 'Fixture' }, host: { ios: { minimumOS: '17.0' } }, deploy: { signing: { key: 'test', keys: { test: raw } } } };
    app.manifest.orientation = 'portrait';
    app.manifest.shortcuts = [{ name: 'Open', url: '/open' }];
    fs.writeFileSync(resolve(root, 'app.json'), JSON.stringify(app.manifest));
    fs.writeFileSync(resolve(root, 'app.contract'), 'component App\n  state path = ""\n  action follow(url: string)\n    path = url\n  view\n    main navigationKey="root" navigate=follow\n      text "Hello"\n');
    fs.writeFileSync(resolve(root, 'app.ts'), "export const appId = 'fixture';\nexport const grants = 'device.camera purpose.eye';\nexport const answer = () => { throw Error('no source'); };\n");
    fs.mkdirSync(resolve(root, 'strings'));
    fs.writeFileSync(resolve(root, 'strings/en.json'), JSON.stringify({ 'purpose.eye': 'Take a photo.' }));
    const env = { ...process.env, EXACT_UPDATE_TRUST: 'production', EXACT_BAKE_ANALYSIS: '1', EXACT_GAME_PROOF_PROFILE: 'release' };
    const initial = resolve(root, 'initial');
    const result = realSpawnSync(bake, [root, '--cargo', 'ios', 'aarch64-apple-ios', initial], { env, encoding: 'utf8' });
    expect(result.status, result.stderr).toBe(0);
    cohort.compat = JSON.parse(fs.readFileSync(resolve(initial, 'compat.json')));
    const evidence = { version: 1, metadata: { ...nativeManifestMetadata(app.manifest, 'ios'), delivery: cohort.compat.delivery, store: cohort.compat.inputs.store, keys: cohort.compat.inputs.keys }, files: [], directories: [], absent: [], links: [] };
    const binary = { sha256: cohort.binary, sourceInputs: evidence };
    Object.assign(cohort, cohortReceipt({ version: 1, compat: cohort.compat, graph: { version: 1, sources: {} }, binary }));
    const plan = fs.readFileSync(resolve(initial, 'app.plan'));
    const bundle = { plan: { bytes: plan, sha256: sha256(plan), formatVersion: cohort.compat.inputs.formatVersion, kernelSchema: cohort.compat.inputs.kernelSchema }, assets: [] };
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const head = streamHead({ app, bundle, stream, seq: 7, release: 'initial' });
    head.cohort = cohort;
    head.signature = { keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') };
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const file = binaryInputFile({ binary });
    await origin.put(blobPath(file.sha256), file.bytes);
    await origin.putHead(stream, Buffer.from(JSON.stringify(head)), { previousDigest: null });
    if (changed === 'minimumOS') {
      app.manifest.host.ios.minimumOS = '18.0';
      fs.writeFileSync(resolve(root, 'app.json'), JSON.stringify(app.manifest));
    } else if (changed === 'orientation' || changed === 'shortcuts') {
      if (changed === 'orientation') app.manifest.orientation = 'landscape';
      else delete app.manifest.shortcuts;
      fs.writeFileSync(resolve(root, 'app.json'), JSON.stringify(app.manifest));
    } else if (changed === 'localizations') fs.writeFileSync(resolve(root, 'strings/fr.json'), JSON.stringify({ 'purpose.eye': 'Prendre une photo.' }));
    else if (changed) fs.writeFileSync(resolve(root, 'strings/en.json'), JSON.stringify({ 'purpose.eye': 'Photograph your receipt.' }));
    const snapshot = { bundle, compat: cohort.compat, graph: { artifacts: [] } };
    const [update] = await streamCandidates(app, origin, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }], env, { snapshot });
    expect(update.reason).toBeUndefined();
    const check = classifyArtifacts(update.build, cohort, { id: 'test', public: raw });
    expect(check.binary).toBe(changed !== null);
    if (changed) expect(update.build.pendingInputs).toContain(changed);
    else expect(update.build.pendingInputs).toEqual([]);
    if (changed === 'grantPurposes' || changed === 'localizations') expect(check.bundle).toBe(false);
    const table = await classify({ app, origin, opts: {}, channel: 'prod', snapshot: { dirty: false, changes: [] }, release: 'changed', bundle,
      compat: {}, builds: {}, platforms: ['ios'], wantOrigin: false, candidates: new Map([[stream.compatibilityId, update]]) });
    expect(table.rows.some(row => row.action === 'binary' && row.reason.includes(changed))).toBe(changed !== null);
    expect((await origin.head(stream)).bytes).toEqual(Buffer.from(JSON.stringify(head)));
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('signed binary source evidence detects changed inputs and refuses damaged receipts', async () => {
  const { root, app, build, cohort, streams, streamCandidates, calls } = harness(classifyArtifacts, 4, { rustModule: false });
  try {
    app.manifest.app = { id: 'fixture', name: 'Fixture' };
    app.target = resolve(root, 'target');
    fs.mkdirSync(app.target);
    const native = resolve(root, 'native'), code = resolve(native, 'code.rs'), link = resolve(native, 'link.rs'), added = resolve(native, 'added.rs');
    fs.mkdirSync(native);
    fs.writeFileSync(code, 'old code');
    fs.symlinkSync('code.rs', link);
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    Object.assign(cohort.compat.inputs, { platform: 'ios', rustMode: 'off', keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    const metadata = { ...nativeManifestMetadata(app.manifest, 'ios'), store: cohort.compat.inputs.store, keys: cohort.compat.inputs.keys };
    build.version = 1;
    build.binary = { sha256: cohort.binary, sourceInputs: binarySourceInputs({
      files: [{ path: code, sha256: sha256(fs.readFileSync(code)) }],
      directories: new Map([[native, { path: native, names: directoryInputNames(native) }]]),
      absent: new Map([[added, added]]), links: new Map([[link, { path: link, target: 'code.rs' }]]), metadata,
    }, [{ name: 'app', path: app.dir }], [app.target]) };
    Object.assign(cohort, cohortReceipt(build));
    const file = binaryInputFile(build);
    expect(file.bytes.toString()).not.toContain(root);
    const originRoot = resolve(root, 'origin'), origin = new DirectoryOrigin(originRoot);
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const bundle = { plan: { bytes: Buffer.from('plan'), sha256: sha256(Buffer.from('plan')), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] };
    await publishStream({ origin, app, signer, row: { ...stream, platform: 'ios' }, bundle, compat: cohort.compat, build, release: 'initial', snapshot: {}, opts: {}, log() {} });
    expect(await origin.get(blobPath(cohort.binaryInputs.sha256))).toEqual(file.bytes);
    const head = (await origin.head(stream)).bytes;
    const found = await streams(origin, app, 'prod', { deploying: true, platforms: ['ios'] });
    expect(found[0].cohort.binaryInputs).toEqual(cohort.binaryInputs);
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const snapshot = { bundle, compat: cohort.compat, graph: { artifacts: [] } };
    const candidate = async () => (await streamCandidates(app, origin, found, env, { snapshot }))[0];
    const unchanged = await candidate();
    expect(unchanged.build.pendingInputs).toEqual([]);
    expect((await publishStream({ ...unchanged, origin, app, signer, release: 'unchanged', snapshot: {}, opts: {}, log() {} })).action).toBe('current');
    const live = app.dir;
    app.dir = resolve(root, 'captured-app');
    fs.cpSync(native, resolve(app.dir, 'native'), { recursive: true, verbatimSymlinks: true });
    expect((await candidate()).build.pendingInputs).toEqual([]);
    app.dir = live;
    for (const [change, name] of [['file', 'native/code.rs'], ['directory', 'native'], ['link', 'native/link.rs']]) {
      if (change === 'file') fs.writeFileSync(code, 'new code');
      if (change === 'directory') fs.writeFileSync(added, 'new input');
      if (change === 'link') { fs.unlinkSync(link); fs.symlinkSync('other.rs', link); }
      const update = await candidate();
      expect(classifyArtifacts(update.build, cohort, { id: 'test', public: raw }).binary).toBe(true);
      expect(update.build.pendingInputs).toContain(`app/${name}`);
      if (change === 'directory') expect(update.build.pendingInputs).toContain('app/native/added.rs');
      fs.writeFileSync(code, 'old code');
      fs.rmSync(added, { force: true });
      fs.unlinkSync(link); fs.symlinkSync('code.rs', link);
    }
    const tampered = Buffer.from(file.bytes);
    tampered[0] ^= 1;
    for (const damaged of [null, tampered]) {
      const path = resolve(originRoot, blobPath(file.sha256));
      if (damaged) fs.writeFileSync(path, damaged); else fs.unlinkSync(path);
      const update = await candidate();
      expect(update.reason).toContain('binary input receipt is missing or does not match');
      const table = await classify({ app, origin, opts: {}, channel: 'prod', snapshot: {}, release: 'damaged', bundle,
        compat: {}, builds: {}, platforms: ['ios'], wantOrigin: false, candidates: new Map([[stream.compatibilityId, update]]) });
      expect(table.rows.map(row => row.action)).toEqual(['binary']);
      const withBuild = opts => streams(origin, app, 'prod', { builds: [build], ...opts });
      const [push] = await streamCandidates(app, origin, await withBuild({}), env);
      expect(push.reason).toContain('binary input receipt is missing or does not match');
      const [deploy] = await streamCandidates(app, origin, await withBuild({ deploying: true }), env, { snapshot });
      if (damaged) expect(deploy.reason).toContain('binary input receipt is missing or does not match');
      else expect(deploy.build.pendingInputs).toEqual([]);
      expect(await origin.get(blobPath(file.sha256))).toEqual(damaged);
      fs.writeFileSync(path, file.bytes);
    }
    expect((await origin.head(stream)).bytes).toEqual(head);
    expect((await origin.list(`${streamPath(stream)}/releases`)).length).toBe(1);
    expect(calls.every(call => ['tools', 'graph', 'compat', 'app'].includes(call))).toBe(true);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('a first push from a production build publishes the evidence its cohort names, and a deploy then reads it', async () => {
  const { root, app, build, cohort, bindings, streams, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false });
  try {
    app.manifest.app = { id: 'fixture', name: 'Fixture' };
    const code = resolve(root, 'native/code.rs');
    fs.mkdirSync(resolve(root, 'native'));
    fs.writeFileSync(code, 'code');
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    Object.assign(cohort.compat.inputs, { platform: 'ios', rustMode: 'off', keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    const metadata = { ...nativeManifestMetadata(app.manifest, 'ios'), store: cohort.compat.inputs.store, keys: cohort.compat.inputs.keys };
    build.version = 1;
    build.binary = { sha256: cohort.binary, sourceInputs: binarySourceInputs({ files: [{ path: code, sha256: sha256(fs.readFileSync(code)) }],
      directories: new Map(), absent: new Map(), links: new Map(), metadata }, [{ name: 'app', path: app.dir }], []) };
    Object.assign(cohort, cohortReceipt(build));
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const [first] = await streamCandidates(app, origin, await streams(origin, app, 'prod', { builds: bindings.readBuilds() }), env);
    expect((await publishStream({ ...first, origin, app, signer, release: 'first', snapshot: {}, opts: {}, log() {} })).action).toBe('published');
    expect(await origin.get(blobPath(cohort.binaryInputs.sha256))).toEqual(binaryInputFile(build).bytes);
    const found = await streams(origin, app, 'prod', { deploying: true, platforms: ['ios'] });
    const snapshot = { bundle: first.bundle, compat: cohort.compat, graph: { artifacts: [] } };
    const [update] = await streamCandidates(app, origin, found, env, { snapshot });
    expect(update.reason).toBeUndefined();
    expect(update.build.pendingInputs).toEqual([]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('push refuses all streams when a signed stream has lost its head', async () => {
  const { root, app, bindings, pusher, published } = harness(classifyArtifacts, 4, {
    rustModule: false, targets: [['healthy', 'off'], ['headless', 'off']], openOrigin: path => new DirectoryOrigin(path),
  });
  try {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const plan = Buffer.from('older plan');
    const bundle = { plan: { bytes: plan, sha256: sha256(plan), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] };
    const [healthy, headless] = bindings.readBuilds();
    for (const build of [healthy, headless]) {
      Object.assign(build.compat.inputs, { keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
      Object.assign(build, { version: 1, binary: { sha256: '0'.repeat(64) } });
      const stream = { channel: 'prod', compatibilityId: build.compat.id };
      const head = streamHead({ app, bundle, stream, seq: 3, release: 'initial' });
      if (build === healthy) head.cohort = cohortReceipt(build);
      else head.cohortReceipt = cohortFiles(cohortReceipt(build))[0].card;
      head.signature = { keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') };
      if (build === healthy) await origin.putHead(stream, Buffer.from(JSON.stringify(head)), { previousDigest: null });
      else await origin.put(`${streamPath(stream)}/releases/initial.json`, Buffer.from(JSON.stringify({ envelope: head })), { immutable: true });
    }

    await expect(pusher({ _: [root], origin: origin.describe(), keys: root }))
      .rejects.toThrow(`${headless.compat.id}: the head is missing with authenticated release history`);
    expect(published).toEqual([]);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each(['head', 'history', 'late history'])('a first push rechecks the frozen cohort when signed evidence appears (%s)', async evidence => {
  const { root, app, build, streams, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false });
  try {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    Object.assign(build.compat.inputs, { rustMode: 'off', keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    build.version = 1;
    build.binary = { sha256: '0'.repeat(64) };
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const [first] = await streamCandidates(app, origin, await streams(origin, app, 'prod', { builds: [build] }), env);
    expect(first.reason).toBeUndefined();
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const stream = { channel: 'prod', compatibilityId: build.compat.id };
    const bytes = Buffer.from('older plan');
    const bundle = { ...first.bundle, plan: { ...first.bundle.plan, bytes, sha256: sha256(bytes) } };
    const head = streamHead({ app, bundle, stream, seq: 7, release: 'other' });
    head.signature = signer.sign(head);
    const headBytes = Buffer.from(JSON.stringify(head));
    const recordPath = `${streamPath(stream)}/releases/other.json`;
    const record = Buffer.from(JSON.stringify({ envelope: head }));
    if (evidence === 'head') await origin.putHead(stream, headBytes, { previousDigest: null });
    else if (evidence === 'history') await origin.put(recordPath, record, { immutable: true });
    else {
      const list = origin.list.bind(origin);
      // AppDrop has a conditional head write. Another publisher can write its
      // release record between recovery's listing and sequence allocation.
      origin.list = async (...args) => {
        origin.list = list;
        const names = await list(...args);
        await origin.put(recordPath, record, { immutable: true });
        return names;
      };
    }

    await expect(publishStream({ ...first, origin, app, signer, release: 'first', snapshot: {}, opts: {}, log() {} }))
      .rejects.toThrow('no authenticated capability receipt');
    expect((await origin.head(stream))?.bytes ?? null).toEqual(evidence === 'head' ? headBytes : null);
    expect(await origin.get(`${streamPath(stream)}/releases/first.json`)).toBeNull();
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test.each(['publish', 'repair'])('a recovered inline cohort requires its signed binary input receipt (%s)', async action => {
  const { root, app, build, streams, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false });
  try {
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    app.manifest.deploy.store = { android: 'A' };
    Object.assign(build.compat.inputs, { platform: 'android', rustMode: 'off', keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    const metadata = { ...nativeManifestMetadata(app.manifest, 'android'), store: build.compat.inputs.store, keys: build.compat.inputs.keys };
    const sourceInputs = { version: 1, metadata, files: [], directories: [], absent: [], links: [] };
    Object.assign(build, { version: 1, binary: { sha256: '0'.repeat(64), sourceInputs } });
    const newer = { ...build, binary: { sha256: '1'.repeat(64), sourceInputs: { ...sourceInputs, metadata: { ...metadata, changed: true } } } };
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const [first] = await streamCandidates(app, origin, await streams(origin, app, 'prod', { builds: [newer] }), env);
    expect(first.reason).toBeUndefined();

    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const stream = { channel: 'prod', compatibilityId: build.compat.id };
    const head = streamHead({ app, bundle: first.bundle, stream, seq: 7, release: 'older' });
    head.cohort = cohortReceipt(build);
    head.signature = signer.sign(head);
    const base = streamPath(stream);
    await origin.put(`${base}/releases/older.json`, Buffer.from(JSON.stringify({ envelope: head })), { immutable: true });
    const file = binaryInputFile(build);
    const publish = { origin, app, signer, release: 'next', snapshot: {}, opts: {}, log() {} };
    const options = { deploying: true, platforms: ['android'] };
    const discover = binary => streams(origin, app, 'prod', { ...options, builds: [binary] });
    const candidates = found => streamCandidates(app, origin, found, env, { snapshot: { bundle: first.bundle } });
    if (action === 'publish') {
      await expect(publishStream({ ...first, ...publish })).rejects.toThrow('binary input receipt is missing or does not match');
      expect(await origin.get(`${base}/releases/next.json`)).toBeNull();
      expect(await origin.head(stream)).toBeNull();
    } else {
      const [unmatched] = await candidates(await discover(newer));
      expect(unmatched.reason).toContain('binary input receipt is missing or does not match');
      const knownStreams = await discover(build);
      const [restored] = await candidates(knownStreams);
      expect(restored.reason).toBeUndefined();
      expect(restored.build.pendingInputs).toEqual([]);
      const { rows } = await classify({ app, origin, opts: {}, channel: 'prod', snapshot: {}, release: 'next',
        bundle: first.bundle, compat: {}, builds: {}, platforms: ['android'], wantOrigin: false, knownStreams,
        candidates: new Map([[stream.compatibilityId, restored]]) });
      expect(rows).toEqual([expect.objectContaining({ platform: 'android', action: 'bundle', seq: 8 })]);
      expect(await publishStream({ ...restored, ...publish, row: rows[0], recoveryBuild: build })).toMatchObject({ action: 'published', seq: 8 });
      expect(await origin.get(blobPath(file.sha256))).toEqual(file.bytes);
      expect(await admitHead(origin, await origin.head(stream), app, stream)).toMatchObject({ usable: true, cohort: head.cohort });
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

// An app with neither app.ts nor a Rust module: its own bake makes the plan (LLP 1030.007 D2), and no module pair.
test('push bakes an app with neither a TypeScript nor a Rust module through its own bake', async () => {
  const {root,calls,bundles,pusher}=harness(classifyArtifacts,4,{rustModule:false});
  try {
    expect(await pusher({_:[root],origin:root,keys:root})).toBe(0);
    expect(calls).toEqual(['tools','resident','app','compat','graph','publish','close']);
    expect(bundles[0].plan.bytes.toString()).toBe('plan');
    expect(bundles[0].assets.filter(a=>/^(rust|typescript)\//.test(a.name))).toEqual([]);
  } finally {fs.rmSync(root,{recursive:true,force:true})}
});

// A stream whose compatibility inputs this source no longer computes is frozen: push publishes to the current streams,
// and refuses when none is current, naming the verb that starts one (LLP 1030.007 D3).
test('push leaves a stream whose binary predates the sources, and refuses when no stream is current', async () => {
  const x86='x86_64-unknown-linux-gnu', arm='aarch64-unknown-linux-gnu';
  const moved=(targets)=>c=>targets.includes(c.target)?{...c,inputs:{...c.inputs,dataCrate:{tree:'edited'}}}:c;
  const one=harness(classifyArtifacts,4,{targets:[[x86,'native'],[arm,'native']],computed:moved([arm])});
  try {
    expect(await one.pusher({_:[one.root],origin:one.root,keys:one.root})).toBe(0);
    expect(one.published.map(b=>b.compat.target)).toEqual([x86]);
  } finally {fs.rmSync(one.root,{recursive:true,force:true})}
  const none=harness(classifyArtifacts,4,{targets:[[x86,'native'],[arm,'native']],computed:moved([x86,arm])});
  try {
    await expect(none.pusher({_:[none.root],origin:none.root,keys:none.root})).rejects.toThrow(/no stream on the channel prod can take this as an update: linux 00000000: its binary predates these sources \(dataCrate\); a new binary: .*; linux 11111111/);
    expect(none.published).toEqual([]);
  } finally {fs.rmSync(none.root,{recursive:true,force:true})}
});

// A module of another Hermes bytecode version is for a newer APK: the older cohort is frozen, not a refusal that would
// keep the newer stream from its update (LLP 1030.006 D4).
test('push freezes a cohort whose Hermes bytecode version the module does not match', async () => {
  const x86='x86_64-unknown-linux-gnu', arm='aarch64-unknown-linux-gnu';
  const {root,bindings,published,pusher}=harness(classifyArtifacts,4,{targets:[[x86,'native'],[arm,'native']],bytecodeVersion:98});
  try {
    fs.writeFileSync(resolve(root,'app.ts'),'');
    const [older,newer]=bindings.readBuilds();
    older.compat.inputs.hermesBytecode=96;
    newer.compat.inputs.hermesBytecode=98;
    expect(await pusher({_:[root],origin:root,keys:root})).toBe(0);
    expect(published.map(b=>b.compat.target)).toEqual([arm]);
    expect(published[0].graph.artifacts.find(a=>a.name==='typescript/app.hbc').requires.hermesBytecode).toBe(98);
  } finally {fs.rmSync(root,{recursive:true,force:true})}
});

// Without a Rust module, the rust.data crate runs in the binary and baked the plan's first frame: a change to it moves
// `dataCrate`, which supersedes the cohort in push and withholds its bundle in deploy. Another native source is a note,
// and the bundle still goes (LLP 1030.007 D3, D5).
test('a change to the data crate supersedes its cohort, and another native change is a note', async () => {
  let crate = 'old records';
  const { root, app, build, cohort, bindings, streams, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false,
    computed: c => ({ ...c, inputs: { ...c.inputs, dataCrate: { tree: crate } } }) });
  try {
    // The real path: an evidence path read through a link to a tool directory's ancestor is no source.
    const dir = app.dir = fs.realpathSync(root);
    app.manifest.app = { id: 'fixture', name: 'Fixture' };
    const code = resolve(dir, 'native/code.kt');
    fs.mkdirSync(resolve(dir, 'native'));
    fs.writeFileSync(code, 'old activity');
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    app.manifest.deploy.store = { android: 'A' };
    Object.assign(cohort.compat.inputs, { platform: 'android', rustMode: 'off', dataCrate: { tree: crate }, keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    const metadata = { ...nativeManifestMetadata(app.manifest, 'android'), store: cohort.compat.inputs.store, keys: cohort.compat.inputs.keys };
    build.version = 1;
    build.binary = { sha256: cohort.binary, sourceInputs: binarySourceInputs({ files: [{ path: code, sha256: sha256(fs.readFileSync(code)) }],
      directories: new Map(), absent: new Map(), links: new Map(), metadata }, [{ name: 'app', path: dir }], []) };
    Object.assign(cohort, cohortReceipt(build));
    const origin = new DirectoryOrigin(resolve(dir, 'origin'));
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const [first] = await streamCandidates(app, origin, await streams(origin, app, 'prod', { builds: bindings.readBuilds() }), env);
    expect(first.superseded).toEqual([]);
    expect((await publishStream({ ...first, origin, app, signer, release: 'first', snapshot: {}, opts: {}, log() {} })).action).toBe('published');
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id }, head = (await origin.head(stream)).bytes;
    const candidate = async () => (await streamCandidates(app, origin, await streams(origin, app, 'prod', { builds: bindings.readBuilds() }), env))[0];
    const table = async (update) => classify({ app, origin, opts: {}, channel: 'prod', snapshot: { dirty: false, changes: [] }, release: 'edited', bundle: update.bundle,
      compat: {}, builds: {}, platforms: ['android'], wantOrigin: false, candidates: new Map([[stream.compatibilityId, update]]) });
    fs.writeFileSync(code, 'new activity');
    const noted = await candidate();
    expect([noted.superseded, noted.nativeData, noted.build.pendingInputs]).toEqual([[], [], ['app/native/code.kt']]);
    const bundled = (await table(noted)).rows;
    expect(bundled.find(r => r.kind === 'stream').action).toBe('current'); // the head has this plan; nothing withheld
    expect(bundled.find(r => r.kind === 'binary').reason).toStartWith('native inputs differ from the APK\'s: app/native/code.kt; a new APK: ');
    fs.writeFileSync(code, 'old activity');
    crate = 'edited records';
    const update = await candidate();
    expect([update.superseded, update.nativeData]).toEqual([['dataCrate'], ['dataCrate']]);
    const row = (await table(update)).rows.find(r => r.kind === 'stream');
    expect([row.action, row.reason]).toEqual(['binary', 'its binary predates these native sources: dataCrate; a new APK: EXACT_UPDATE_TRUST=production bun exact.mjs android --release (--bundle for the store)']);
    expect((await origin.head(stream)).bytes).toEqual(head);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('a cohort signed before binary input evidence takes a bundle and says its native sources are unchecked', async () => {
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false });
  try {
    app.manifest.app = { id: 'fixture', name: 'Fixture' };
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    Object.assign(cohort.compat.inputs, { platform: 'ios', rustMode: 'off', keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    expect(cohort.binaryInputs).toBeUndefined();
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const plan = Buffer.from('plan');
    const bundle = { plan: { bytes: plan, sha256: sha256(plan), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] };
    const head = streamHead({ app, bundle, stream, seq: 3, release: 'old' });
    head.cohort = cohort;
    head.signature = { keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') };
    await origin.put(blobPath(bundle.plan.sha256), plan);
    await origin.putHead(stream, Buffer.from(JSON.stringify(head)), { previousDigest: null });
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const snapshot = { bundle: { ...bundle, plan: { ...bundle.plan, bytes: Buffer.from('new plan'), sha256: sha256(Buffer.from('new plan')) } }, compat: cohort.compat, graph: { artifacts: [] } };
    const [update] = await streamCandidates(app, origin, [{ stream, head, cohort, platform: 'ios' }], env, { snapshot });
    expect(update.reason).toBeUndefined();
    expect(update.build.pendingInputs).toEqual([]);
    expect(update.note).toContain('records no binary input evidence');
    const table = await classify({ app, origin, opts: {}, channel: 'prod', snapshot: { dirty: false, changes: [] }, release: 'next', bundle,
      compat: {}, builds: {}, platforms: ['ios'], wantOrigin: false, candidates: new Map([[stream.compatibilityId, update]]) });
    expect(table.rows.map(row => row.action), JSON.stringify(table.rows)).toEqual(['bundle']);
    expect(table.notes.some(note => note.includes('records no binary input evidence'))).toBe(true);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('signed binary source evidence holds the consumed SwiftPM targets\' membership, so an added native source needs a binary', async () => {
  const { root, app, build, cohort, streamCandidates } = harness(classifyArtifacts, 4, { rustModule: false });
  try {
    app.manifest.app = { id: 'fixture', name: 'Fixture' };
    app.target = resolve(root, 'target');
    fs.mkdirSync(app.target);
    const packageRoot = resolve(root, 'host/apple');
    for (const [path, text] of [['Sources/ExactIOS/App.swift', 'app'], ['Sources/ExactKit/Kit.swift', 'kit'], ['Sources/ExactKit/Views/Row.swift', 'row'], ['Sources/ExactMac/App.swift', 'mac']]) {
      fs.mkdirSync(resolve(packageRoot, path, '..'), { recursive: true });
      fs.writeFileSync(resolve(packageRoot, path), text);
    }
    const description = { targets: [
      { name: 'ExactIOS', path: 'Sources/ExactIOS', sources: ['App.swift'], target_dependencies: ['ExactKit'] },
      { name: 'ExactKit', path: 'Sources/ExactKit', sources: ['Kit.swift', 'Views/Row.swift'] },
      { name: 'ExactMac', path: 'Sources/ExactMac', sources: ['App.swift'], target_dependencies: ['ExactKit'] },
    ] };
    const collected = buildInputCollector({ outputs: [], nameOf: path => path, hashes: { of: path => sha256(fs.readFileSync(path)) }, replaced: new Set() });
    swiftTargetInputs(description, packageRoot, 'ExactIOS', collected.add);
    const { privateKey, publicKey } = generateKeyPairSync('ed25519');
    const raw = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64');
    app.manifest.deploy.signing = { key: 'test', keys: { test: raw } };
    Object.assign(cohort.compat.inputs, { platform: 'ios', rustMode: 'off', keys: { test: raw }, formatVersion: 4, kernelSchema: '0'.repeat(16) });
    const metadata = { ...nativeManifestMetadata(app.manifest, 'ios'), store: cohort.compat.inputs.store, keys: cohort.compat.inputs.keys };
    build.version = 1;
    build.binary = { sha256: cohort.binary, sourceInputs: binarySourceInputs({ files: [...collected.inputs.values()], directories: collected.directories,
      absent: collected.absent, links: collected.links, metadata }, [{ name: 'app', path: app.dir }], []) };
    Object.assign(cohort, cohortReceipt(build));
    const origin = new DirectoryOrigin(resolve(root, 'origin'));
    const file = binaryInputFile(build);
    await origin.put(blobPath(file.sha256), file.bytes);
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const plan = Buffer.from('plan');
    const snapshot = { bundle: { plan: { bytes: plan, sha256: sha256(plan), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] }, compat: cohort.compat, graph: { artifacts: [] } };
    const env = { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' };
    const candidate = async () => (await streamCandidates(app, origin, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }], env, { snapshot }))[0];
    expect((await candidate()).build.pendingInputs).toEqual([]);
    // A target the product does not consume is not its input.
    fs.writeFileSync(resolve(packageRoot, 'Sources/ExactMac/Menu.swift'), 'menu');
    expect((await candidate()).build.pendingInputs).toEqual([]);
    for (const added of ['Sources/ExactIOS/Delegate.swift', 'Sources/ExactKit/Views/Cell.swift']) {
      fs.writeFileSync(resolve(packageRoot, added), 'added');
      const update = await candidate();
      expect(update.build.pendingInputs).toEqual([`app/host/apple/${added.slice(0, added.lastIndexOf('/'))}`]);
      expect(classifyArtifacts(update.build, cohort, { id: 'test', public: raw }).binary).toBe(true);
      fs.rmSync(resolve(packageRoot, added));
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('a text update retains the main and widget raster catalog inventory', async () => {
  const main = { svg: 'two.svg', viewBox: '10 20 40 20' };
  const widget = { svg: 'widget.svg', viewBox: '0 0 20 40' };
  const rendition = { sha256: 'installed', viewBox: main.viewBox };
  const graphs = Object.fromEntries([['plan', main], ['widget', widget]].map(([name, icon]) => [name,
    { appleIcons: [icon], artifacts: [{ name: 'app.plan', requires: { appleRasterIcons: { [icon.svg]: rendition } } }] }]));
  const { root, app, cohort, streamCandidates } = harness(classifyArtifacts, 4, { graphs, entries: { 'plans/widget.plan': 'widget' } });
  try {
    cohort.compat.inputs.platform = 'ios';
    cohort.compat.inputs.appleRasterIcons = { [main.svg]: rendition, [widget.svg]: rendition };
    cohort.compat.target = 'aarch64-apple-ios';
    fs.mkdirSync(resolve(root, 'assets'));
    for (const icon of [main, widget]) fs.writeFileSync(resolve(root, 'assets', icon.svg), '<svg/>');
    const metadata = { appleAssetCatalogs: appleAssetCatalogInventory({ ...app, appleIcons: [main, widget] }, 'ios'),
      ...nativeManifestMetadata(app.manifest, 'ios'), store: cohort.compat.inputs.store, keys: cohort.compat.inputs.keys };
    const evidence = Buffer.from(JSON.stringify({ version: 1, metadata, files: [], directories: [], absent: [], links: [] }));
    cohort.binaryInputs = { sha256: sha256(evidence), bytes: evidence.length };
    const snapshot = { compat: cohort.compat, bundle: { plan: { bytes: Buffer.from('plan') }, assets: [] }, graph: { artifacts: [] } };
    const stream = { channel: 'prod', compatibilityId: cohort.compat.id };
    const [candidate] = await streamCandidates(app, { get: async () => evidence }, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }],
      { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' }, { snapshot });
    expect(candidate.build.pendingInputs).toEqual([]);
    expect(candidate.build.graph.appleIcons).toEqual([main, widget]);
    expect(candidate.build.graph.artifacts.find(card => card.name === 'plans/widget.plan').requires).toEqual(graphs.widget.artifacts[0].requires);
    const key = { id: 'test', public: 'public' };
    expect(classifyArtifacts(candidate.build, cohort, key).bundle).toBe(true);
    graphs.widget.artifacts[0].requires.appleRasterIcons[widget.svg] = { ...rendition, sha256: 'changed' };
    const [changed] = await streamCandidates(app, { get: async () => evidence }, [{ stream, head: { assets: [] }, cohort, platform: 'ios' }],
      { EXACT_UPDATE_TRUST: 'production', EXACT_GAME_PROOF_PROFILE: 'release' }, { snapshot });
    expect(classifyArtifacts(changed.build, cohort, key).missing).toContain('plans/widget.plan: appleRasterIcons.widget.svg (requires the installed SVG rendition)');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test('updates require the raster renditions installed by their Apple cohort', () => {
  const red = { sha256: 'red-blue', viewBox: '10 20 40 20' };
  const green = { ...red, sha256: 'green-blue' };
  const cohort = { sources: {}, binary: 'same', compat: { inputs: { store: { L: 'A' }, appleRasterIcons: { 'two.svg': red, 'spare.svg': red } } } };
  const candidate = raster => ({ binary: { sha256: 'same' }, graph: { sources: {}, artifacts: [{ name: 'app.plan', requires: { appleRasterIcons: raster } }] } });
  expect(classifyArtifacts(candidate({ 'two.svg': red }), cohort).bundle).toBe(true);
  expect(classifyArtifacts(candidate({}), cohort).bundle).toBe(true);
  for (const roster of [{ 'two.svg': green }, { 'new.svg': red }]) {
    const result = classifyArtifacts(candidate(roster), cohort);
    expect(result.bundle).toBe(false);
    expect(result.missing[0]).toContain('appleRasterIcons');
  }
  expect(classifyArtifacts(candidate({ 'two.svg': red }), { ...cohort, compat: { inputs: { store: { L: 'A' } } } }).bundle).toBe(false);
});
