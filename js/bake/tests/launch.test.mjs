// A setting a launch may give (LLP 1153.002): the bake's rewrite of a marked read and its refusals
// (`__exact_env.mjs` `inlineSettings`, `__exact_launch_setting.mjs`), what a launch's environment is to the check of a
// build's freshness, and how js/src/launch.js names the setting in a grant refusal.
import { expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

test('a setting checks as a string across build values, native launches and web bundles', async () => {
  const {check,configure,injectBackendOrigin,staticFacts} = await import('../src/typescript.mjs');
  const {parseSync} = await import('rolldown/utils');
  const {inlineSettings} = await import('../src/__exact_env.mjs');
  const libraries=resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))),'lib');
  const root=mkdtempSync(resolve(tmpdir(),'exact-setting-types-')), app=resolve(root,'app'), stage=resolve(root,'stage');
  mkdirSync(app);
  mkdirSync(stage);
  const code=`type Mode = 'demo' | 'live';
const MODE = import.meta.env.ISSUE_502_MODE ?? 'demo';
const EXPLICIT = (import.meta.env.ISSUE_502_MODE ?? 'demo') as string;
const UNION = (import.meta.env.ISSUE_502_MODE ?? 'demo') as 'demo' | 'live';
const SATISFIES = (import.meta.env.ISSUE_502_MODE ?? 'demo') satisfies 'demo' | 'live';
const CONSTANT = (import.meta.env.ISSUE_502_MODE ?? 'demo') as const;
const ANGLE = <'demo' | 'live'>(import.meta.env.ISSUE_502_MODE ?? 'demo');
const ANGLE_CONST = <const>(import.meta.env.ISSUE_502_MODE ?? 'demo');
const SATISFIES_UNION = ((import.meta.env.ISSUE_502_MODE ?? 'demo') satisfies Mode) as Mode;
const CONSTANT_UNION = ((import.meta.env.ISSUE_502_MODE ?? 'demo') as const) as Mode;
const ANGLE_SATISFIES = <Mode>((import.meta.env.ISSUE_502_MODE ?? 'demo') satisfies Mode)!;
const ANGLE_CONSTANT = <Mode>(<const>(import.meta.env.ISSUE_502_MODE ?? 'demo'))!;
const UNPAREN = import.meta.env.ISSUE_502_DIRECT as 'demo' | 'live';
const NONNULL = ((import.meta.env.ISSUE_502_MODE ?? 'demo')!);
const labels = {demo:'Demo',live:'Live'};
export const label = labels[UNION] + labels[ANGLE] + labels[UNPAREN];
export const stacked = labels[SATISFIES_UNION] + labels[CONSTANT_UNION] + labels[ANGLE_SATISFIES] + labels[ANGLE_CONSTANT];
export const checked: string[] = [SATISFIES, CONSTANT, ANGLE_CONST];
export const nonnull = NONNULL === 'live';
const origin = import.meta.env.ISSUE_502_ORIGIN ?? 'https://built.example';
const API: \`https://\${string}\` = import.meta.env.ISSUE_502_ORIGIN ?? 'https://built.example';
export const url: \`https://\${string}\` = API;
export const greeting = MODE === 'live' ? 'Live' : 'Demo';
export const explicit = EXPLICIT === 'live';
export const moved = origin === 'https://other.example';
// @ts-expect-error A setting is a string, not a number.
const number: number = MODE;
// @ts-expect-error A setting is not restricted to this build's literal value.
const literal: 'demo' = MODE;
`;
  try {
    writeFileSync(resolve(stage,'__exact_paths.json'),JSON.stringify({app,mounts:[]}));
    for (const launch of [false,true]) for (const web of [false,true]) for (const value of [null,'demo','live']) {
      const names=['ISSUE_502_MODE','ISSUE_502_DIRECT','ISSUE_502_API'];
      writeFileSync(resolve(app,'app.json'),JSON.stringify({env:[...names.map(name=>launch ? {name,launch:true} : name),'ISSUE_502_ORIGIN']}));
      const file=resolve(app,'.env');
      writeFileSync(file,`ISSUE_502_DIRECT=${value ?? 'demo'}\n${value===null ? '' : `ISSUE_502_MODE=${value}\nISSUE_502_API=https://${value}.example\n`}`);
      // Narrow annotations reject a replaceable native string, even when its build value fits.
      const rejection=launch && !web ? '// @ts-expect-error A launch may give any string.\n' : '';
      const annotated=`${rejection}const ANNOTATED: 'demo' | 'live' = import.meta.env.ISSUE_502_MODE ?? 'demo';
export const annotated = labels[ANNOTATED];
${rejection}const TEMPLATE: \`https://\${string}\` = import.meta.env.ISSUE_502_API ?? 'https://built.example';
export const template: \`https://\${string}\` = TEMPLATE;
const STRING: string = import.meta.env.ISSUE_502_MODE ?? 'demo';
export const string = STRING === 'live';
`;
      const source=code+annotated+'export const grants = `net.fetch ${origin}`;\n';
      if (value===null) {
        rmSync(file);
        expect(() => inlineSettings(source,app,parseSync,{},{launches:!web})).toThrow('import.meta.env.ISSUE_502_DIRECT has no value');
        writeFileSync(file,'ISSUE_502_DIRECT=demo\n');
      }
      writeFileSync(resolve(app,'app.ts'),source);
      writeFileSync(resolve(stage,'app.ts'),source);
      writeFileSync(resolve(stage,'__exact_entry.ts'),"export * from './app';\n");
      // configure is synchronous: restore the shared environment before yielding to the checker.
      const saved=Object.fromEntries(['EXACT_BACKEND_ORIGIN','EXACT_BACKEND_PROVENANCE',...names,'ISSUE_502_ORIGIN'].map(name=>[name,process.env[name]]));
      try {
        for (const name of Object.keys(saved)) delete process.env[name];
        configure(stage,false,parseSync,web);
      } finally {
        for (const [name,value] of Object.entries(saved)) { if (value===undefined) delete process.env[name]; else process.env[name]=value; }
      }
      await check(stage,resolve(libraries,'tsc'),libraries);
      const configured=readFileSync(resolve(stage,'app.ts'),'utf8'), suffix=launch ? names.map(name=>'\nenv.launch '+name).join('') : '';
      expect(staticFacts(configured,parseSync).grants).toBe('net.fetch https://built.example'+suffix);
      const moved=injectBackendOrigin(configured,'https://selected.example',parseSync);
      expect(staticFacts(moved,parseSync).grants).toBe('net.fetch https://selected.example'+suffix);
      writeFileSync(resolve(stage,'app.ts'),moved);
      await check(stage,resolve(libraries,'tsc'),libraries);
    }
  } finally {
    rmSync(root,{recursive:true,force:true});
  }
});

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
    expect(set.code).toBe('const API = (__exact_launch["API_BASE_URL"] ?? "http://127.0.0.1:9999");\nconst PICTURES = ("https://pictures.example" as string);\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n\nimport { launched as __exact_launch } from \'./__exact_launch\';\n');
    // A name app.ts already spells is not taken.
    expect(inline("const __exact_launch = 1;\nconst API = import.meta.env.API_BASE_URL ?? 'http://x';\nconst PICTURES = import.meta.env.PICTURES;\n",{PICTURES:'p'}).code).toBe('const __exact_launch = 1;\nconst API = (__exact_launch_["API_BASE_URL"] ?? "http://127.0.0.1:9999");\nconst PICTURES = ("p" as string);\n\nimport { launched as __exact_launch_ } from \'./__exact_launch\';\n');
    expect(set.record).toEqual({API_BASE_URL:{value:'http://127.0.0.1:9999',source:'.env',launch:true},PICTURES:{value:'https://pictures.example',source:'.env'}});
    // The web's bundles, which no launch gives a value: the build's, inlined, and the same record.
    const web=inlineSettings(code,dir,parseSync,{},{launches:false});
    expect(web).toEqual({code:'const API = ("http://127.0.0.1:9999" as string);\nconst PICTURES = ("https://pictures.example" as string);\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n',record:set.record});
    expect(staticFacts(set.code,parseSync).grants).toBe('net.fetch https:\nnet.fetch https://pictures.example');
    // The grants carry each marked name, which a host answers a launch's value for and no other; the bake writes them.
    expect(staticFacts(launchGrants(set.code,parseSync,['API_BASE_URL']),parseSync).grants).toBe('net.fetch https:\nnet.fetch https://pictures.example\nenv.launch API_BASE_URL');
    expect(launchGrants(set.code,parseSync,[])).toBe(set.code);
    expect(launchGrants("export const appId = 'a';\n",parseSync,['API_BASE_URL'])).toBe("export const appId = 'a';\n\nexport const grants = \"env.launch API_BASE_URL\";\n");
    // A private top-level `grants` is the app's own and stays: the bake's lines take a name of their own, exported as
    // grants, which the module's grants are.
    for (const own of ["const grants = 'label';\n", "function grants() {}\n"]) {
      const out=launchGrants(`${own}export const appId = 'a';\n`,parseSync,['API_BASE_URL']);
      expect(out).toBe(`${own}export const appId = 'a';\n\nconst __exact_launch_grants = "env.launch API_BASE_URL";\nexport { __exact_launch_grants as grants };\n`);
      expect(staticFacts(out,parseSync).grants).toBe('env.launch API_BASE_URL');
    }
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
      expect(inline("const API = import.meta.env.API_BASE_URL ?? 'http://x';\n")).toEqual({code:'const API = ("http://x" as string);\n',record:{API_BASE_URL:{value:'http://x',source:'default'}}});
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

test('a variant\'s settings come after the environment and before the files, over what Bun loaded from them', async () => {
  // From the #651 reporter's tested diff, with the selection scoped to the app (LLP 1030.008 D2, D6).
  const {settingValues} = await import('../src/__exact_env.mjs');
  const dir=mkdtempSync(resolve(tmpdir(),'exact-variant-env-'));
  try {
    writeFileSync(resolve(dir,'app.json'),JSON.stringify({app:{id:'com.example.v',name:'V'},env:['A','B'],variants:{dev:{idSuffix:'.dev',name:'V Dev',env:{A:'a-dev'}}}}));
    writeFileSync(resolve(dir,'.env'),'A=a-file\nB=b-file\n');
    const file={A:{value:'a-file',source:'.env'},B:{value:'b-file',source:'.env'}}, dev={EXACT_VARIANT:'dev',EXACT_VARIANT_APP:dir};
    expect(settingValues(dir,['A','B'],{})).toEqual(file);
    expect(settingValues(dir,['A','B'],dev)).toEqual({...file,A:{value:'a-dev',source:'variants.dev'}});
    // A value Bun loaded from `.env` is the file's, which the variant's replaces; the environment's replaces both.
    expect(settingValues(dir,['A'],{...dev,A:'a-file',EXACT_BUN_LOADED:'A'})).toEqual({A:{value:'a-dev',source:'variants.dev'}});
    expect(settingValues(dir,['A'],{...dev,A:'a-shell'})).toEqual({A:{value:'a-shell',source:'environment'}});
    // A variant selected for another app is not this one's.
    expect(settingValues(dir,['A'],{EXACT_VARIANT:'dev',EXACT_VARIANT_APP:tmpdir()})).toEqual({A:{value:'a-file',source:'.env'}});
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('a grant refusal at a launch value\'s origin names the setting, origins compared as the URL standard has them', () => {
  // js/src/launch.js in a realm of its own, its door answering the launch's values.
  const values={DEFAULT_PORT:'https://api.example:443/v1',UPPER:'https://UPPER.Example/v1',CREDENTIALS:'https://user:secret@credentials.example/v1',OTHER_PORT:'https://port.example:8443'};
  const realm={__exact_host:(op,name)=>op===28 ? values[name] : undefined,URL};
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
      expect([...script.matchAll(/\/\/#region (\S+)/g)].map(region=>region[1].split('/').at(-1))).toEqual(['__exact_launch.ts','attack.js','app.ts','__exact_entry.ts']);
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
      expect(run(['launch.js','binary.js',...prelude],(op,name)=>op===28 && name==='API_BASE_URL' ? 'https://launch.example' : '')).toEqual(['https://launch.example','https://package.example']);
    } finally { rmSync(dir,{recursive:true,force:true}); }
  }
});

test('a preload names settings that are undeclared or have no build value, and accepts a module default', async () => {
  const { inlineSettings } = await import('../src/__exact_env.mjs');
  const { parseSync } = await import('rolldown/utils');
  const app = mkdtempSync(resolve(tmpdir(), 'exact-preload-settings-'));
  const parse = parseSync;
  try {
    const write = env => writeFileSync(resolve(app, 'app.json'), JSON.stringify({ env, preload: { feed: { url: '{env:EONE_API}/feed' } } }));
    write([]);
    expect(() => inlineSettings('', app, parse, {})).toThrow('preload.feed.url: setting EONE_API is not declared');
    write([{ name: 'EONE_API', launch: true }]);
    expect(() => inlineSettings('const API = import.meta.env.EONE_API;', app, parse, {})).toThrow('preload.feed.url: setting EONE_API has no build value');
    const { record } = inlineSettings("const API = import.meta.env.EONE_API ?? 'https://built.test';", app, parse, {});
    expect(record).toEqual({ EONE_API: { value: 'https://built.test', source: 'default', launch: true } });
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('a backend selection refuses a preload setting whose origin literal it rewrites', async () => {
  const { configure } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-preload-backend-'));
  const app = resolve(root, 'app'), stage = resolve(root, 'stage');
  mkdirSync(app); mkdirSync(stage);
  const saved = Object.fromEntries(['EXACT_BACKEND_ORIGIN', 'EXACT_BACKEND_PROVENANCE', 'API'].map(name => [name, process.env[name]]));
  try {
    process.env.EXACT_BACKEND_ORIGIN = 'https://selected.example';
    delete process.env.API; delete process.env.EXACT_BACKEND_PROVENANCE;
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({env:['API'], preload:{feed:{url:'{env:API}/feed'}}}));
    writeFileSync(resolve(stage, '__exact_paths.json'), JSON.stringify({app, mounts:[]}));
    writeFileSync(resolve(stage, 'app.ts'), "const origin = import.meta.env.API ?? 'https://built.example';\nexport const grants = `net.fetch ${origin}`;\n");
    expect(() => configure(stage, false, parseSync)).toThrow('preload.feed.url: setting API');
    expect(() => configure(stage, false, parseSync)).toThrow('EXACT_BACKEND_ORIGIN selects https://selected.example');
  } finally {
    for (const [name, value] of Object.entries(saved)) { if (value === undefined) delete process.env[name]; else process.env[name] = value; }
    rmSync(root, {recursive:true, force:true});
  }
});

test('settings and preloads read the resolved shell manifest together', async () => {
  const { inlineSettings, declaredSettings } = await import('../src/__exact_env.mjs');
  const { parseSync } = await import('rolldown/utils');
  const app = mkdtempSync(resolve(tmpdir(), 'exact-preload-manifest-'));
  try {
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({env:['WRONG'], preload:{wrong:{url:'{env:MISSING}'}}}));
    mkdirSync(resolve(app, '.shells'));
    writeFileSync(resolve(app, '.shells/app.json'), JSON.stringify({env:['API'], preload:{feed:{url:'{env:API}/feed'}}}));
    expect(declaredSettings(app)).toEqual(['API']);
    expect(inlineSettings("const origin = import.meta.env.API ?? 'https://built.example';", app, parseSync, {}).record.API.value).toBe('https://built.example');
    writeFileSync(resolve(app, '.shells/app.json'), JSON.stringify({env:['API'], preload:{feed:{url:'{env:MISSING}/feed'}}}));
    expect(() => inlineSettings("const origin = import.meta.env.API ?? 'https://built.example';", app, parseSync, {})).toThrow('preload.feed.url: setting MISSING');
  } finally { rmSync(app, {recursive:true, force:true}); }
});
