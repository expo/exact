// A setting a launch may give (LLP 1153.002): the bake's rewrite of a marked read and its refusals
// (`__exact_env.mjs` `inlineSettings`, `__exact_launch_setting.mjs`), what a launch's environment is to the check of a
// build's freshness, and how js/src/launch.js names the setting in a grant refusal.
import { expect, test } from 'bun:test';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import vm from 'node:vm';

test('a marked setting is read at launch with the build\'s value behind it, an unmarked one is inlined as before, and its misuses are refused', async () => {
  const {inlineSettings,settingLines} = await import('../src/__exact_env.mjs');
  const {launchGrants,launchModule,staticFacts} = await import('../src/typescript.mjs');
  const {validate} = await import('../../../scripts/app.mjs');
  const {parseSync} = await import('rolldown/utils');
  const schema = JSON.parse(readFileSync(resolve(import.meta.dir,'../../../scripts/app.schema.json'),'utf8'));
  const dir=mkdtempSync(resolve(tmpdir(),'exact-launch-settings-'));
  const declare=(...env)=>writeFileSync(resolve(dir,'app.json'),JSON.stringify({env}));
  const inline=(code,env={})=>inlineSettings(code,dir,parseSync,env);
  try {
    // The manifest takes a name or { name, launch }, and nothing else.
    const problems=env=>validate(env,schema.properties.env,'env',schema);
    expect(problems(['API_BASE_URL',{name:'PICTURES',launch:true},{name:'MODE'},{name:'LEVEL',launch:false}])).toEqual([]);
    for (const bad of [{launch:true},{name:'API',launch:'yes'},{name:'api',launch:true},{name:'API',launch:true,default:'x'},['API']]) expect(problems([bad]).length).toBe(1);
    declare({name:'API_BASE_URL',launch:true},'PICTURES');
    writeFileSync(resolve(dir,'.env'),'API_BASE_URL=http://127.0.0.1:9999\nPICTURES=https://pictures.example\n');
    const code="const API = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8080';\nconst PICTURES = import.meta.env.PICTURES;\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n";
    const set=inline(code);
    // The marked read takes the launch's value where the bundle asked for one, else the build's; the unmarked one is its value.
    // The read takes what the stage's launch module exported, which app.ts's copy imports last, so no line moves.
    expect(set.code).toBe('const API = __exact_launch["API_BASE_URL"] ?? "http://127.0.0.1:9999";\nconst PICTURES = "https://pictures.example";\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n\nimport { launched as __exact_launch } from \'./__exact_launch\';\n');
    // A name app.ts already spells is not taken.
    expect(inline("const __exact_launch = 1;\nconst API = import.meta.env.API_BASE_URL ?? 'http://x';\nconst PICTURES = import.meta.env.PICTURES;\n",{PICTURES:'p'}).code).toBe('const __exact_launch = 1;\nconst API = __exact_launch_["API_BASE_URL"] ?? "http://127.0.0.1:9999";\nconst PICTURES = "p";\n\nimport { launched as __exact_launch_ } from \'./__exact_launch\';\n');
    expect(set.record).toEqual({API_BASE_URL:{value:'http://127.0.0.1:9999',source:'.env',launch:true},PICTURES:{value:'https://pictures.example',source:'.env'}});
    // The web's bundles, which no launch gives a value: the build's, inlined, and the same record.
    const web=inlineSettings(code,dir,parseSync,{},{launches:false});
    expect(web).toEqual({code:'const API = "http://127.0.0.1:9999";\nconst PICTURES = "https://pictures.example";\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n',record:set.record});
    expect(staticFacts(set.code,parseSync).grants).toBe('net.fetch https:\nnet.fetch https://pictures.example');
    // The grants carry each marked name, which a host answers a launch's value for and no other; the bake writes them.
    expect(staticFacts(launchGrants(set.code,parseSync,['API_BASE_URL']),parseSync).grants).toBe('net.fetch https:\nnet.fetch https://pictures.example\nenv.launch API_BASE_URL');
    expect(launchGrants(set.code,parseSync,[])).toBe(set.code);
    expect(launchGrants("export const appId = 'a';\n",parseSync,['API_BASE_URL'])).toBe("export const appId = 'a';\n\nexport const grants = \"env.launch API_BASE_URL\";\n");
    // Grants another module may give, beside a marked name: a declaration added to app.ts would shadow or duplicate them,
    // so the bake refuses and names the re-export. Without a marked name, or with grants declared in app.ts, it does not.
    for (const reexport of ["export * from './permissions'", "export { grants } from './permissions'", "export { x as grants } from './permissions'"]) {
      expect(() => launchGrants(`${reexport};\n`,parseSync,['API_BASE_URL'])).toThrow(`app.ts: \`${reexport}\` may give the module its grants, and API_BASE_URL is marked "launch": true in app.json, whose env.launch line the bake adds to grants declared in app.ts: declare export const grants in app.ts`);
      expect(launchGrants(`${reexport};\n`,parseSync,[])).toBe(`${reexport};\n`);
    }
    expect(staticFacts(launchGrants("export * from './permissions';\nexport const grants = 'net.fetch https:';\n",parseSync,['API_BASE_URL']),parseSync).grants).toBe('net.fetch https:\nenv.launch API_BASE_URL');
    // A type-only re-export gives no value, so no grants: the bake declares them as for an app.ts with none.
    for (const reexport of ["export type * from './types'", "export type { grants } from './types'", "export { type grants } from './types'"]) {
      expect(launchGrants(`${reexport};\n`,parseSync,['API_BASE_URL'])).toBe(`${reexport};\n\nexport const grants = "env.launch API_BASE_URL";\n`);
    }
    // However app.ts spells it, and whether or not a name is marked: the grants as evaluated, a development line too.
    for (const names of [[],['API_BASE_URL']]) for (const grants of ["'env.' + 'launch HOME'","['net.fetch https:', 'development env.' + 'launch HOME'].join('\\n')"]) expect(() => launchGrants(`export const grants = ${grants};\n`,parseSync,names)).toThrow('an env.launch line is the bake\'s');
    expect(() => launchGrants("export const grants = 'net.fetch https:\\nenv.launch HOME';\n",parseSync,[])).toThrow('app.ts: grants `env.launch HOME`: an env.launch line is the bake\'s, written for each name app.json\'s "env" marks { "name": "NAME", "launch": true }; mark the setting there and remove the line');
    expect(settingLines(set.record)).toEqual(['import.meta.env.API_BASE_URL = "http://127.0.0.1:9999" (.env; a launch that sets it replaces it, except on Android and the web)','import.meta.env.PICTURES = "https://pictures.example" (.env)']);
    // Unset at build, the default is the build's value.
    rmSync(resolve(dir,'.env'));
    declare({name:'API_BASE_URL',launch:true});
    expect(inline("const API = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8080';\n").record).toEqual({API_BASE_URL:{value:'http://127.0.0.1:8080',source:'default',launch:true}});
    // `"launch": false`, or only a name, is the bare name: inlined, byte for byte.
    for (const entry of ['API_BASE_URL',{name:'API_BASE_URL'},{name:'API_BASE_URL',launch:false}]) {
      declare(entry);
      expect(inline("const API = import.meta.env.API_BASE_URL ?? 'http://x';\n")).toEqual({code:'const API = "http://x";\n',record:{API_BASE_URL:{value:'http://x',source:'default'}}});
    }
    // The stage's launch module asks for the marked names, and the entry imports it first; with none marked, neither is there.
    const entry='import * as app from \'./app\';\nexport const answer = app.answer;\n', at=name=>resolve(dir,name);
    writeFileSync(at('__exact_entry.ts'),entry);
    for (let i=0; i<2; i++) launchModule(dir,['API_BASE_URL']);
    expect(readFileSync(at('__exact_entry.ts'),'utf8')).toBe('import \'./__exact_launch\';\n'+entry);
    expect(readFileSync(at('__exact_launch.ts'),'utf8')).toContain('.__exact_launch_settings?.(["API_BASE_URL"]) ?? Object.freeze(Object.create(null));');
    launchModule(dir,[]);
    expect(readFileSync(at('__exact_entry.ts'),'utf8')).toBe(entry);
    expect(existsSync(at('__exact_launch.ts'))).toBe(false);
    // Refused, each at once: a marked name of the host's, grants over a marked name (a template, through another const, keyed by
    // its origin), a marked name app.ts never reads, a name marked and bare, and an entry of no shape the manifest takes.
    declare({name:'EXACT_API',launch:true},{name:'API_BASE_URL',launch:true},{name:'UNREAD',launch:true},'TWICE',{name:'TWICE',launch:true},{name:'BAD',launch:1});
    const misuse="const EXACT = import.meta.env.EXACT_API ?? 'http://e';\nconst API = import.meta.env.API_BASE_URL ?? 'http://a';\nconst TWICE = import.meta.env.TWICE ?? 'http://t';\nconst HOST = API + '/v1';\nexport const grants = `net.fetch ${HOST}`;\n";
    expect(() => inline(misuse)).toThrow([
      `app.json's "env": {"name":"BAD","launch":1} is not a setting: write its name, or { "name": "API_BASE_URL", "launch": true } for one a launch may give`,
      `app.json's "env": EXACT_API is marked "launch": true, and a name that begins with EXACT_ is the host's: rename the setting`,
      `app.json's "env" names TWICE twice, once marked "launch": true: name it once`,
      `app.ts:2:13: grants read API, and import.meta.env.API_BASE_URL is one a launch may give ("launch": true in app.json), while grants are fixed at build: grant what a launch may name (net.fetch https:, net.fetch http:, or each origin it may give) without API`,
      `import.meta.env.UNREAD: app.json's "env" declares it, and app.ts reads it nowhere: read it as const X = import.meta.env.UNREAD, or remove it from "env"`,
    ].join('\n'));
    declare({name:'API_BASE_URL',launch:true});
    expect(() => inline("const origin = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8080';\nexport const grants = { 'http://127.0.0.1:8080': 'net.fetch http://127.0.0.1:8080' };\n")).toThrow('app.ts:1:16: grants read origin, and import.meta.env.API_BASE_URL is one a launch may give');
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('a marked name the environment the app is started with sets is the launch\'s, however it came there', async () => {
  const {settingChanges,settingValues} = await import('../src/__exact_env.mjs');
  const dir=mkdtempSync(resolve(tmpdir(),'exact-launch-bun-')), name='EXACT_TEST_SETTING';
  try {
    writeFileSync(resolve(dir,'.env'),`${name}=http://a.example\n`);
    // Built with B from the environment. A launch environment that sets the name, empty or not and whatever set it (a
    // launcher, or Bun from `.env`, whose names it notes for a build's sources, LLP 1153 D4.2), gives the launch's
    // value: no change (LLP 1153.002 D2, "A drive").
    const built={[name]:{value:'http://b.example',source:'environment',launch:true}};
    for (const launch of [{[name]:'http://c.example'},{[name]:'http://a.example',EXACT_BUN_LOADED:name},{[name]:''}]) expect(settingChanges(built,{},dir,{launch})).toEqual([]);
    // One that does not: the build's sources decide, and the `.env` beside the app now gives A.
    expect(settingChanges(built,{},dir,{launch:{}}).map(c=>c.now)).toEqual([{value:'http://a.example',source:'.env'}]);
    // For a build, what Bun loaded is still the file it came from.
    expect(settingValues(dir,[name],{[name]:'http://a.example',EXACT_BUN_LOADED:name})).toEqual({[name]:{value:'http://a.example',source:'.env'}});
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('a grant refusal at a launch value\'s origin names the setting, origins compared as the URL standard has them', () => {
  // js/src/launch.js in a realm of its own, its door answering the launch's values.
  const values={DEFAULT_PORT:'https://api.example:443/v1',UPPER:'https://UPPER.Example/v1',CREDENTIALS:'https://user:secret@credentials.example/v1',OTHER_PORT:'https://port.example:8443'};
  const realm={__exact_host:(op,name)=>op===27 ? values[name] : undefined,URL};
  new Function('globalThis',readFileSync(resolve(import.meta.dir,'../../src/launch.js'),'utf8'))(realm);
  const given=realm.__exact_launch_settings(Object.keys(values));
  expect(given).toEqual(values);
  expect(Object.isFrozen(given) && Object.getPrototypeOf(given)===null).toBe(true);
  const refused=url=>realm.__exact_launch_refusal({kind:'Refused',message:"outside the app's grants (net.fetch)"},url).message;
  expect(refused(new URL('/x','https://api.example:443/v1').href)).toContain(': https://api.example is DEFAULT_PORT\'s value');
  expect(refused('https://api.example/x')).toContain('is DEFAULT_PORT\'s value');
  expect(refused('https://upper.example/x')).toContain(': https://upper.example is UPPER\'s value');
  expect(refused('https://api.example.com/x')).toBe("outside the app's grants (net.fetch)");
  expect(refused('https://other:pw@credentials.example/x')).toContain(': https://credentials.example is CREDENTIALS\'s value');
  expect(refused('https://port.example/x')).toBe("outside the app's grants (net.fetch)");
  expect(refused('https://port.example:8443/x')).toContain('is OTHER_PORT\'s value');
  // Asked once.
  expect(() => realm.__exact_launch_settings(['HOME'])).toThrow('read once');
});

// What a package app.ts imports first may do: rebind `globalThis`, put a getter on `Object.prototype`, replace the
// function the launch module calls, and bind the launch module's names itself.
const ATTACK = `const spoof = { API_BASE_URL: 'https://package.example' };
try { Object.defineProperty(globalThis, 'globalThis', { value: { __exact_launch: spoof, __exact_launch_settings: () => spoof }, configurable: true }); } catch {}
try { Object.defineProperty(Object.prototype, 'API_BASE_URL', { get: () => 'https://prototype.example', configurable: true }); } catch {}
try { globalThis.__exact_launch_settings = () => spoof; } catch {}
const __exact_launch = spoof, launched = spoof;
export const named = __exact_launch === launched ? __exact_launch : null;
`;

test('a marked read takes what the launch module asked for before any module ran, which no module\'s names or code change', async () => {
  const {inlineSettings} = await import('../src/__exact_env.mjs');
  const {launchModule,launchSource} = await import('../src/typescript.mjs');
  const prelude=(await import('../../../host/web-js/glue.mjs')).MODULE_PRELUDE.map(file=>file.slice('js/src/'.length));
  const {rolldown} = await import('rolldown');
  const {parseSync} = await import('rolldown/utils');
  // An app's files of the launch module's name, which the bake refuses: with the producer's resolution the import is
  // still its module; without it, `.tsx`, which resolution tries before `.ts`, would be read.
  const shadow="export const launched = { API_BASE_URL: 'https://shadow.example' };\n";
  // app.ts's own top-level names the launch module reads or binds: a function hoisted above every statement, a const,
  // and the name a read is rewritten to.
  for (const own of ['', 'function globalThis() {}\nexport const hoisted = typeof globalThis;\n', "const Object = { freeze: () => ({ API_BASE_URL: 'https://app.example' }), create: () => null };\nexport const own = () => Object;\n", "const __exact_launch = { API_BASE_URL: 'https://app.example' };\nexport const own = __exact_launch;\n"]) {
    const dir=mkdtempSync(resolve(tmpdir(),'exact-launch-binding-'));
    try {
      writeFileSync(resolve(dir,'app.json'),JSON.stringify({env:[{name:'API_BASE_URL',launch:true}]}));
      const {code}=inlineSettings(`import { named } from './attack';\n${own}export const read = import.meta.env.API_BASE_URL ?? 'http://built.example';\nexport const spoofed = named.API_BASE_URL;\n`,dir,parseSync,{});
      writeFileSync(resolve(dir,'app.ts'),code);
      writeFileSync(resolve(dir,'attack.js'),ATTACK);
      writeFileSync(resolve(dir,'__exact_entry.ts'),"import * as app from './app';\nexport const read = app.read, spoofed = app.spoofed;\n");
      launchModule(dir,['API_BASE_URL']);
      for (const name of ['__exact_launch.js','__exact_launch.tsx']) writeFileSync(resolve(dir,name),shadow);
      const bundled=async plugins=>{
        const bundle=await rolldown({input:resolve(dir,'__exact_entry.ts'),cwd:dir,transform:{target:'es2016'},plugins,logLevel:'silent'});
        try { return (await bundle.generate({format:'iife',name:'exact'})).output[0].code; } finally { await bundle.close(); }
      };
      const script=await bundled([launchSource(dir)]);
      expect(script).not.toContain('shadow.example');
      expect(await bundled([])).toContain('shadow.example');
      // The launch module's code is the bundle's first, before the package app.ts imports first.
      expect(script.match(/\/\/#region (\S+)/g)?.map(region=>region.split('/').at(-1))).toEqual(['__exact_launch.ts','attack.js','app.ts','__exact_entry.ts']);
      const run=(files,host)=>{
        const realm=vm.createContext({URL,TextEncoder,TextDecoder,AbortController,AbortSignal,crypto,__exact_host:host,console:{error() {}}});
        for (const file of files) vm.runInContext(readFileSync(resolve(import.meta.dir,'../../src',file),'utf8'),realm);
        vm.runInContext(script,realm);
        return vm.runInContext('[exact.read, exact.spoofed]',realm);
      };
      // The wasm target's page realm runs this bundle over binary.js and the prelude's scripts (host/web-js/glue.mjs
      // `MODULE_PRELUDE`): no launch, the build's value.
      expect(run(['binary.js',...prelude],()=>'')).toEqual(['http://built.example','https://package.example']);
      // Natively launch.js comes first (js/build.rs), and the door answers the launch's value.
      expect(run(['launch.js','binary.js',...prelude],(op,name)=>op===27 && name==='API_BASE_URL' ? 'https://launch.example' : '')).toEqual(['https://launch.example','https://package.example']);
    } finally { rmSync(dir,{recursive:true,force:true}); }
  }
});
