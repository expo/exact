import { validateChannel, channelOrigin, sha256, blobPath } from './origin.mjs';
import { EXE } from './paths.mjs';
import {expect, mock, test} from 'bun:test';
import * as fs from 'node:fs';
import {tmpdir, homedir} from 'node:os';
import {resolve, relative} from 'node:path';
import {fileURLToPath} from 'node:url';
import {bakedEntryAssets, classifyArtifacts} from './app.mjs';
import {spawnSync as realSpawnSync} from 'node:child_process';
import {channelOf} from './deploy.mjs';
import {filesystem, filesystemRead} from './filesystem.mjs';
import {unbundledTextures} from './texture-families.mjs';

// The publisher's functions with boundary doubles, for an app at a fresh
// root whose cohort's binary has prelude runtime 4 and whose baked module
// needs `runtime` (its receipt's). Bun's native subprocess import bypasses
// module mocks; explicit bindings also isolate deployment tests.
function harness(classify=()=>({bundle:true}), runtime=4, {bake, cohortRuntime=4, grants='', watch=fs.watch}={}) {
  const root=fs.mkdtempSync(resolve(tmpdir(),'exact-push-profile-'));
  const app={id:'fixture',name:'fixture',dir:root,target:root,manifest:{app:{origin:root},deploy:{signing:{keys:{test:'public'}}}}};
  const compat={id:'a'.repeat(32),target:'fixture',inputs:{app:'fixture',platform:'linux',store:{L:'A'},rustMode:'native',grantCeiling:grants,keys:{test:'public'},typescriptRuntime:cohortRuntime}};
  const graph={artifacts:[{name:'app.plan',requires:{sources:{}},sha256:'plan'}]};
  const cohort={version:1,compat,binary:'binary',sources:{}};
  const build={trust:'production',compat,graph};
  const calls=[], published=[], bundles=[], destinations=[], rows=[];
  const sealed=(name,env)=>{
    calls.push(name);
    expect(env?.EXACT_GAME_PROOF_PROFILE,`${name} shipping profile`).toBe('release');
    expect(env?.EXACT_UPDATE_TRUST,`${name} production trust`).toBe('production');
  };
  const bindings={
    ...Object.fromEntries(['existsSync','readFileSync','writeFileSync','mkdtempSync','rmSync'].map(name=>[name,fs[name]])), watch,
    filesystem, filesystemRead, unbundledTextures,
    channelOf,validateChannel,resolve,relative,homedir,ROOT:fileURLToPath(new URL('..',import.meta.url)),
    process:{env:{EXACT_GAME_PROOF_PROFILE:'gpu-dev',EXACT_UPDATE_TRUST:'development'}},
    console:{log(){}},Refusal:Error,refuse(message){throw Error(message)},
    loadSigner:()=>({keyId:'test'}),resolveApp:()=>app,readBuilds:()=>[build],
    bakedEntryAssets,bakeOutput:()=>resolve(root,'bake'),
    cohortReceipt:()=>cohort,classifyArtifacts:classify,
    openOrigin:mock(spec=>({writable:true,list:async path=>{destinations.push({origin:spec,path});return []},describe:()=>spec})),
    channelOrigin,sha256:()=> 'digest',blobPath:x=>x,streamPath:()=> 'stream',
    defaultRelease:()=> 'r-fixture',admitHead(){throw Error('unexpected head')},
    requireReleasePins(){}, // the release pins' gate (game/inputs.mjs), not what this seals
    async publishStream({build,bundle,row}){
      expect(build.trust).toBe('production');calls.push('publish');published.push(build);bundles.push(bundle);rows.push(row);
      return {action:'current',platform:'linux',compatibilityId:compat.id};
    },
    RustBaker:class {constructor(app,env){sealed('resident',env)}close(){calls.push('close')}},
    async buildRust(app,opts){sealed('rust',opts.env);expect(opts.profile).toBe('release');return {plan:Buffer.from('plan')}},
    rustBundle:(app,bundle,build)=>({bundle,build}),
    spawnSync(tool,args,opts){
      if(tool==='git')return {status:0,stdout:'b'.repeat(40)};
      const name=tool==='cargo'?'tools':args[0]==='graph'?'graph':'typescript';
      sealed(name,opts.env);
      // `bake` runs the real one (its receipt's runtime is the bake's own).
      if(name==='typescript'&&bake)return realSpawnSync(bake,args,{...opts,env:{...process.env,...opts.env}});
      if(name==='typescript'){
        const out=args[args.indexOf('--out')+1];fs.mkdirSync(out,{recursive:true});
        fs.writeFileSync(resolve(out,'app.plan'),'plan');
        fs.writeFileSync(resolve(out,'app.hbc'),'bytecode');
        // The runtime the bake found the module uses (`typescript_runtime_of`).
        fs.writeFileSync(resolve(out,'app.module.json'),JSON.stringify({appId:'fixture',grants:'',typescriptRuntime:runtime}));
      }
      return {status:0,stdout:JSON.stringify(graph)};
    },
  };
  const source=fs.readFileSync(new URL('./push.mjs',import.meta.url),'utf8');
  const functions=source.slice(source.indexOf('function parseArgs('),source.indexOf('\nif (process.argv[1]'))
    .replace('export async function pusher','async function pusher');
  const pusher=new Function(...Object.keys(bindings),functions+'\nreturn pusher;')(...Object.values(bindings));
  return {root,app,build,calls,published,bundles,destinations,rows,bindings,pusher};
}

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
      expect(calls).toEqual(['tools','resident',typescript?'typescript':'rust','graph','publish','close']);
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

// A TypeScript app with a Live Activity: its production bake lists the activity's plan and schema and the module pair
// (bake/src/widgets.rs), which exist only in the bake's output, beside the receipt readBuilds reads.
test('push starts a stream for a TypeScript app with a Live Activity from what its bake made, listing one module pair', async () => {
  const {root,build,published,bundles,pusher}=harness(classifyArtifacts);
  try {
    fs.writeFileSync(resolve(root,'app.ts'),'');
    const made={'plans/departure.plan':'activity plan','activities/departure.schema.json':'{}','typescript/app.hbc':'baked bytecode','typescript/app.module.json':'{}'};
    const entries=resolve(root,'bake',`${build.compat.inputs.platform}-${build.compat.target}.entries`);
    for(const [name,text] of Object.entries(made)){
      fs.mkdirSync(resolve(entries,name,'..'),{recursive:true});fs.writeFileSync(resolve(entries,name),text);
      build.graph.artifacts.push({name,sha256:sha256(Buffer.from(text)),bytes:text.length,kind:'bundle',requires:{}});
    }
    expect(await pusher({_:[root],origin:root,keys:root})).toBe(0);
    const names=['activities/departure.schema.json','app.plan','plans/departure.plan','typescript/app.hbc','typescript/app.module.json'];
    expect(published[0].graph.artifacts.map(a=>a.name).sort()).toEqual(names);
    expect(bundles[0].assets.map(a=>a.name).sort()).toEqual(names.filter(n=>n!=='app.plan'));
    // The plans are the bake's; the pair is the one push baked.
    const bytes=Object.fromEntries(bundles[0].assets.map(a=>[a.name,a.bytes.toString()]));
    expect([bytes['plans/departure.plan'],bytes['typescript/app.hbc']]).toEqual(['activity plan','bytecode']);
    // A file the bake no longer holds as it listed it refuses.
    fs.writeFileSync(resolve(entries,'plans/departure.plan'),'edited');
    await expect(pusher({_:[root],origin:root,keys:root})).rejects.toThrow(/plans\/departure\.plan: .* is not the file the linux bake listed/);
  } finally {fs.rmSync(root,{recursive:true,force:true})}
});

test('push refuses a TypeScript module that needs a newer prelude than the cohort has', async () => {
  const {root,calls,pusher}=harness(classifyArtifacts,5);
  try {
    fs.writeFileSync(resolve(root,'app.ts'),'');
    await expect(pusher({_:[root],origin:root,keys:root})).rejects.toThrow(/typescriptRuntime \(requires a prelude of runtime 5 or later; this cohort's is 4\).*It needs a new binary/);
    expect(calls).not.toContain('publish');
  } finally {fs.rmSync(root,{recursive:true,force:true})}
});

// The real bake and classifier: a module that may open a database needs
// runtime 10, as its transaction's options may ask to check foreign keys and an
// older prelude would drop them (bake/src/compat.rs). Here they are data, a
// migration parsed from JSON, and a cohort on runtime 9 is refused.
test('push refuses a SQLite module whose transaction options are data to a cohort on runtime 9', async () => {
  const ROOT=fileURLToPath(new URL('..',import.meta.url));
  // The bake the lane built (docs/testing.md: a test runs a built binary, it
  // builds none), in Cargo's target directory, the lane's own if it set one.
  const bake=resolve(ROOT,process.env.CARGO_TARGET_DIR||'target',`debug/exact-js-bake${EXE}`);
  if(!fs.existsSync(bake))throw new Error('build the bake first: cargo build -p exact-js-bake');
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
    for (const file of ['app.contract', 'app.tamber', 'assets/logo.png', 'assets/Inter.ttf', 'assets/chime.wav', 'assets/clip.mov', 'deck/deck.js']) {
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
    const source = fs.readFileSync(new URL('./push.mjs', import.meta.url), 'utf8');
    const fn = source.slice(source.indexOf('async function carriedAssets('), source.indexOf('\nfunction gitHead('));
    const carried = new Function('filesystem', 'filesystemRead', 'unbundledTextures', 'resolve', 'sha256', 'blobPath', 'refuse', fn + ';return carriedAssets;')(filesystem, filesystemRead, unbundledTextures, resolve, sha256, blobPath, (message) => { throw Error(message); });
    const asked = [];
    const assets = await carried({ get: async (path) => { asked.push(path); return shader; } }, { dir: root }, { assets: [
      { name: 'assets/deleted.png', sha256: 'deleted' }, { name: 'shaders/x.wgsl', sha256: sha256(shader) },
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
    const source = fs.readFileSync(new URL('./push.mjs', import.meta.url), 'utf8');
    const fn = source.slice(source.indexOf('async function carriedAssets('), source.indexOf('\nfunction gitHead('));
    const carried = new Function('filesystem', 'filesystemRead', 'unbundledTextures', 'resolve', 'sha256', 'blobPath', 'refuse', fn + ';return carriedAssets;')(filesystem, filesystemRead, unbundledTextures, resolve, sha256, blobPath, (message) => { throw Error(message); });
    const assets = await carried({}, { dir: root }, {});
    expect(assets.map(asset => asset.name)).toEqual(names);
    for (const asset of assets) {
      expect(asset.bytes.length).toBe(bytes.length);
      expect(asset.sha256).toBe(digest);
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}, 60000);
