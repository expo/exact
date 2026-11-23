// A setting a launch may give (LLP 1153.002): the bake's rewrite of a marked read and its refusals
// (`__exact_env.mjs` `inlineSettings`, `__exact_launch_setting.mjs`), what a launch's environment is to the check of a
// build's freshness, and how js/src/launch.js names the setting in a grant refusal.
import { expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve, sep } from 'node:path';
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
      // Only configuration reads the shared environment; restore it before checking.
      const saved=Object.fromEntries(['EXACT_BACKEND_ORIGIN','EXACT_BACKEND_PROVENANCE',...names,'ISSUE_502_ORIGIN'].map(name=>[name,process.env[name]]));
      try {
        for (const name of Object.keys(saved)) delete process.env[name];
        await configure(stage,false,parseSync,web);
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
    expect(web).toEqual({code:'const API = ("http://127.0.0.1:9999" as string);\nconst PICTURES = ("https://pictures.example" as string);\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n',record:set.record,reads:set.reads});
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
      expect(inline("const API = import.meta.env.API_BASE_URL ?? 'http://x';\n")).toEqual({code:'const API = ("http://x" as string);\n',record:{API_BASE_URL:{value:'http://x',source:'default'}},reads:[{file:'app.ts',name:'API_BASE_URL'}]});
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
      `import.meta.env.UNREAD: app.json's "env" declares it, and app.ts and its app-local imports read it nowhere: read it as const X = import.meta.env.UNREAD, or remove it from "env"`,
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
    await expect(configure(stage, false, parseSync)).rejects.toThrow('preload.feed.url: setting API');
    await expect(configure(stage, false, parseSync)).rejects.toThrow('EXACT_BACKEND_ORIGIN selects https://selected.example');
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


test('captured app modules inline settings, share defaults and read launch values before clients load', async () => {
  const { configure, dataBundle } = await import('../src/typescript.mjs');
  const { settingSources } = await import('../src/__exact_env.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { rolldown } = await import('rolldown');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-'));
  const app = resolve(root, 'app'), stage = resolve(root, 'stage');
  const put = (dir, file, text) => { const path = resolve(dir, file); mkdirSync(dirname(path), {recursive:true}); writeFileSync(path, text); };
  const api = "export const API = import.meta.env.ISSUE_859_API ?? 'https://built.example';\n";
  try {
    for (const launch of [false, true]) for (const web of [false, true]) {
      rmSync(stage, {recursive:true, force:true});
      put(app, 'app.json', JSON.stringify({env:[launch ? {name:'ISSUE_859_API',launch:true} : 'ISSUE_859_API']}));
      put(stage, '__exact_paths.json', JSON.stringify({app,mounts:[]}));
      put(stage, '__exact_entry.ts', "export * from './app';\n");
      put(stage, 'app.ts', "import { API } from './src/api';\nimport { STREAM } from './src/stream.mjs';\nexport const twin = STREAM;\nexport const value = API;\nexport const grants = 'net.fetch https:';\n");
      put(stage, 'src/api.ts', api);
      // A package.json does not make a captured app-local file a package.
      put(stage, 'src/package.json', '{"name":"app-client","type":"module"}');
      put(stage, 'src/stream.mjs', api.replace('API =', 'STREAM ='));
      put(stage, '__exact_backend_source.json', JSON.stringify(Object.fromEntries(settingSources(stage))));
      await configure(stage, false, parseSync, web);
      expect(JSON.parse(readFileSync(resolve(stage, '__exact_env.json'), 'utf8'))).toEqual({ISSUE_859_API:{value:'https://built.example',source:'default',...(launch ? {launch:true} : {})}});
      for (const file of ['src/api.ts', 'src/stream.mjs']) expect(readFileSync(resolve(stage, file), 'utf8')).not.toContain('import.meta.env');
      const bundle = await rolldown({input:resolve(stage,'__exact_entry.ts'),...dataBundle(stage),logLevel:'silent'});
      let code;
      try { code = (await bundle.generate({format:'iife',name:'exact'})).output[0].code; } finally { await bundle.close(); }
      const realm = vm.createContext({__exact_launch_settings:() => ({ISSUE_859_API:'https://launch.example'})});
      vm.runInContext(code, realm);
      expect(vm.runInContext('exact.value', realm)).toBe(launch && !web ? 'https://launch.example' : 'https://built.example');
      expect(vm.runInContext('exact.twin', realm)).toBe(vm.runInContext('exact.value', realm));
      // A second configuration must start from authored reads, including unchanged clients.
      await configure(stage, true, parseSync, web);
    }
  } finally { rmSync(root, {recursive:true, force:true}); }
});

test('captured settings refuse conflicting defaults, unsupported reads and grants derived through imports', async () => {
  const { configure } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-refusals-'));
  const app = resolve(root, 'app'), stage = resolve(root, 'stage');
  const put = (dir, file, text) => { const path = resolve(dir,file); mkdirSync(dirname(path),{recursive:true}); writeFileSync(path,text); };
  const api = "export const API = import.meta.env.ISSUE_859_API ?? 'https://built.example';\n";
  try {
    const setup = (entry, files, launch = false) => {
      rmSync(stage,{recursive:true,force:true});
      put(app,'app.json',JSON.stringify({env:[launch ? {name:'ISSUE_859_API',launch:true} : 'ISSUE_859_API']}));
      put(stage,'__exact_paths.json',JSON.stringify({app,mounts:[]}));
      put(stage,'app.ts',entry);
      for (const [file,code] of Object.entries(files)) put(stage,file,code);
    };
    setup("import './src/api';\nimport './src/stream';\nexport const grants = '';\n", {'src/api.ts':api,'src/stream.ts':api.replace('built.example','other.example')});
    await expect(configure(stage,false,parseSync)).rejects.toThrow('give every read the same default');
    setup("import './src/api';\nexport const grants = '';\n", {'src/api.ts':api.replace('const API', 'let API')});
    await expect(configure(stage,false,parseSync)).rejects.toThrow('top-level const of an app-local module');
    for (const [entry, files] of [
      ["import { ENDPOINT as API } from './src/barrel';\nexport const grants = `net.fetch ${API}`;\n", {'src/api.ts':api,'src/barrel.ts':"export { API as ENDPOINT } from './api';\n"}],
      ["import * as client from './src/api';\nexport const grants = `net.fetch ${client.API}`;\n", {'src/api.ts':api}],
      ["import { API } from 'client';\nexport const grants = `net.fetch ${API}`;\n", {'src/api.ts':api,'tsconfig.json':JSON.stringify({compilerOptions:{paths:{client:['./src/api.ts']}}})}],
    ]) {
      setup(entry,files,true);
      await expect(configure(stage,false,parseSync)).rejects.toThrow('while grants are fixed at build');
    }
  } finally { rmSync(root,{recursive:true,force:true}); }
});

test('only transitive app imports supply settings and syntax errors reach the type checker', async () => {
  const { configure, check } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-graph-'));
  const app = resolve(root, 'app'), stage = resolve(root, 'stage');
  mkdirSync(app); mkdirSync(stage); mkdirSync(resolve(stage, 'src'));
  const put = (file, code) => writeFileSync(resolve(stage, file), code);
  try {
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({env:['ISSUE_859_API'],preload:{feed:{url:'{env:ISSUE_859_API}/feed'}}}));
    put('__exact_paths.json', JSON.stringify({app,mounts:[]}));
    put('__exact_entry.ts', "export * from './app';\n");
    put('app.ts', "export { API } from './src/api';\n");
    put('src/api.ts', "export { API } from './settings';\n");
    const source = "export const API = import.meta.env.ISSUE_859_API ?? 'https://built.example';\n";
    put('src/settings.ts', source);
    put('notes.ts', "export const debug = () => import.meta.env.DEBUG;\n");
    put('unimported.ts', source.replace('built.example', 'unused.example'));
    await configure(stage, false, parseSync);
    expect(JSON.parse(readFileSync(resolve(stage, '__exact_env.json'), 'utf8')).ISSUE_859_API.value).toBe('https://built.example');
    expect(readFileSync(resolve(stage, 'unimported.ts'), 'utf8')).toContain('import.meta.env');
    put('src/settings.ts', source + 'export const broken = ;\n');
    await configure(stage, false, parseSync);
    await expect(check(stage, resolve(libraries, 'tsc'), libraries)).rejects.toThrow('Expression expected');
    put('app.ts', 'export {};\n');
    put('notes.ts', 'export const broken = ;\n');
    await expect(configure(stage, false, parseSync)).rejects.toThrow('read it nowhere');
  } finally { rmSync(root, {recursive:true,force:true}); }
});

test('uncaptured files and package reads do not satisfy a declared setting', async () => {
  const { configure, dataBundle } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { rolldown } = await import('rolldown');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-packages-'));
  const app = resolve(root,'app'), stage = resolve(root,'stage');
  const put = (dir,file,text) => { const path=resolve(dir,file); mkdirSync(dirname(path),{recursive:true}); writeFileSync(path,text); };
  try {
    put(app,'app.json',JSON.stringify({env:['ISSUE_859_API']}));
    put(app,'dist/dropped.ts',"export const API = import.meta.env.ISSUE_859_API ?? 'https://dropped.example';\n");
    put(stage,'__exact_paths.json',JSON.stringify({app,mounts:[]}));
    put(stage,'app.ts',"export const grants = '';\n");
    await expect(configure(stage,false,parseSync)).rejects.toThrow('app.ts and its app-local imports read it nowhere');
    put(app,'app.json',JSON.stringify({env:[]}));
    put(app,'node_modules/client/package.json','{"name":"client","version":"1.0.0","type":"module","main":"index.js"}');
    put(app,'node_modules/client/index.js',"export const API = import.meta.env.ISSUE_859_API ?? 'https://package.example';\n");
    put(stage,'app.ts',"export { API } from 'client';\n");
    rmSync(resolve(stage,'__exact_backend_source.json'),{force:true});
    await configure(stage,false,parseSync);
    const bundle = await rolldown({input:resolve(stage,'app.ts'),...dataBundle(stage),logLevel:'silent'});
    try { await expect(bundle.generate({format:'esm'})).rejects.toThrow('client@1.0.0 (index.js:1:20) reads import.meta.env'); } finally { await bundle.close(); }
    put(app,'app.json',JSON.stringify({env:['ISSUE_859_API']}));
    await expect(configure(stage,false,parseSync)).rejects.toThrow('app.ts and its app-local imports read it nowhere');
  } finally { rmSync(root,{recursive:true,force:true}); }
});

test('settings resolve the same browser entry and conditions as the bundle', async () => {
  const { configure, dataBundle } = await import('../src/typescript.mjs');
  const { appSettings } = await import('../../../scripts/app-imports.mjs');
  const { serverUrl } = await import('../../../scripts/app-server.mjs');
  const { webInputDigests } = await import('../../../scripts/agent-launch.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { rolldown } = await import('rolldown');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-resolution-'));
  const app = resolve(root, 'app'), stage = resolve(root, 'stage');
  const put = (dir, file, code) => { mkdirSync(resolve(dir, file, '..'), {recursive:true}); writeFileSync(resolve(dir, file), code); };
  const source = "export const API = import.meta.env.ISSUE_859_API ?? 'https://browser.example';\n";
  try {
    for (const [entry, files] of [
      ["export { API } from './client';", {'client/package.json':JSON.stringify({main:'legacy.js',module:'api.js'}), 'client/legacy.js':'export const API = 0;', 'client/api.js':source}],
      ["export { API } from './client';", {'client/package.json':JSON.stringify({main:'legacy.js',module:'other.js',browser:'api.js'}), 'client/legacy.js':'export const API = 0;', 'client/other.js':'export const API = 1;', 'client/api.js':source}],
      ["export { API } from './legacy.js';", {'package.json':JSON.stringify({browser:{'./legacy.js':'./api.js'}}), 'legacy.js':'export const API = 0;', 'api.js':source}],
      ["export { API } from './api.js';", {'api.js':source, 'api.ts':source.replace('browser.example','unused.example')}],
      ["export { API } from '#api';", {'package.json':JSON.stringify({imports:{'#api':{browser:'./api.js',default:'./legacy.js'}}}), 'legacy.js':'export const API = 0;', 'api.js':source}],
      ["export { API } from '#api';", {'package.json':JSON.stringify({imports:{'#api':{require:'./legacy.js',import:'./api.js'}}}), 'legacy.js':'export const API = 0;', 'api.js':source}],
      ["export const API = require('#api').API;", {'package.json':JSON.stringify({imports:{'#api':{import:'./legacy.js',require:'./api.js'}}}), 'legacy.js':'export const API = 0;', 'api.js':source}],
    ]) {
      for (const dir of [app, stage]) {
        rmSync(dir, {recursive:true,force:true});
        put(dir, 'app.json', JSON.stringify({env:['ISSUE_859_API']}));
        put(dir, 'app.ts', entry);
        for (const [file, code] of Object.entries(files)) put(dir, file, code);
      }
      put(stage, '__exact_paths.json', JSON.stringify({app,mounts:[]}));
      await configure(stage, false, parseSync);
      const record = JSON.parse(readFileSync(resolve(stage, '__exact_env.json'), 'utf8'));
      expect(record.ISSUE_859_API.value).toBe('https://browser.example');
      const bundle = await rolldown({input:resolve(stage, 'app.ts'), ...dataBundle(stage), logLevel:'silent'});
      try {
        const code = (await bundle.generate({format:'iife',name:'exact'})).output[0].code;
        const realm = vm.createContext({});
        vm.runInContext(code, realm);
        expect(vm.runInContext('exact.API', realm)).toBe('https://browser.example');
      } finally { await bundle.close(); }
      const sourceInputs = JSON.parse(readFileSync(resolve(stage,'__exact_inputs.json'),'utf8'));
      put(app,'.exact/typescript-inputs.json',JSON.stringify(sourceInputs));
      expect(appSettings(app, {}).record).toEqual(record);
      expect(webInputDigests({dir:app,manifest:{env:['ISSUE_859_API']}}, true, [])['import.meta.env.ISSUE_859_API']).toBe(JSON.stringify(record.ISSUE_859_API));
      expect(serverUrl(app, {ISSUE_859_API:'http://127.0.0.1:8080'})).toBe('http://127.0.0.1:8080');
    }
  } finally { rmSync(root, {recursive:true,force:true}); }
}, 30000);

test('discovery and freshness use the bake across mounted imports and checker-only dependencies', async () => {
  const { configure, check, dataBundle } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { rolldown } = await import('rolldown');
  const { serverUrl } = await import('../../../scripts/app-server.mjs');
  const { shown, webChanges, webInputDigests } = await import('../../../scripts/agent-launch.mjs');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-record-'));
  const dir = resolve(root, 'app'), shared = resolve(root, 'shared'), dist = resolve(dir, 'dist'), stage = resolve(dist, '.gen/typescript');
  const manifest = {env:['ISSUE_859_API'],typescript:{sources:{core:'../shared'}}}, app = {dir,manifest};
  const put = (dir, file, code) => { mkdirSync(dirname(resolve(dir,file)), {recursive:true}); writeFileSync(resolve(dir,file), code); };
  try {
    const files = {
      'app.ts': "export { API } from './core/api';\nexport const value: import('./model').Model = 'hello';\n",
      'settings.ts': "export const API = import.meta.env.ISSUE_859_API ?? 'https://built.example';\n",
      'model.d.ts': '/// <reference path="./reference.d.ts" />\nexport type Model = string;\n',
      'reference.d.ts': 'interface Referenced { value: string }\n',
    };
    for (const [file, code] of Object.entries(files)) { put(dir,file,code); put(stage,file,code); }
    put(shared, 'api.ts', "export { API } from '../settings';\n");
    put(stage, 'core/api.ts', readFileSync(resolve(shared,'api.ts'),'utf8'));
    put(dir, 'app.json', JSON.stringify(manifest));
    put(stage, 'app.json', JSON.stringify(manifest));
    put(dir, '.env', 'ISSUE_859_API=http://127.0.0.1:8787\n');
    put(stage, '__exact_paths.json', JSON.stringify({app:dir,mounts:[['core',shared]]}));
    put(stage, '__exact_entry.ts', "export * from './app';\n");
    put(dist,'.exact-build.json',JSON.stringify({target:'js',inputs:{},inputRoots:[]}));
    expect(webChanges(dist,app).all).toContain(shown(resolve(dir,'app.ts')));
    await configure(stage, false, parseSync, true);
    await check(stage, resolve(libraries,'tsc'), libraries);
    const resolved = JSON.parse(readFileSync(resolve(stage,'__exact_inputs.json'),'utf8'));
    expect(resolved.reads).toContainEqual({kind:'absent',path:resolve(dir,'model.ts')});
    const bundle = await rolldown({input:resolve(stage,'__exact_entry.ts'), ...dataBundle(stage),logLevel:'silent'});
    try { await bundle.generate({format:'esm'}); } finally { await bundle.close(); }
    expect(serverUrl(dir, {})).toBe('http://127.0.0.1:8787');
    const inputs = webInputDigests(app,true,[]);
    expect(inputs['import.meta.env.ISSUE_859_API']).toBe(JSON.stringify({value:'http://127.0.0.1:8787',source:'.env'}));
    for (const path of [resolve(shared,'api.ts'),resolve(dir,'model.d.ts'),resolve(dir,'reference.d.ts')]) expect(inputs[shown(path)]).toBeString();
    put(dist, '.exact-build.json', JSON.stringify({target:'js',inputs,inputRoots:[]}));
    expect(webChanges(dist,app).all).toEqual([]);
    // The checker tried this file before choosing model.d.ts.
    put(dir, 'model.ts', 'export type Model = string;\n');
    const old = new Date('2020-01-01');
    utimesSync(resolve(dir,'model.ts'),old,old);
    expect(webChanges(dist,app).all).toContain(shown(resolve(dir,'model.ts')));
    rmSync(resolve(dir,'model.ts'));
    put(dir, '.env', 'ISSUE_859_API=http://127.0.0.1:8899\n');
    expect(webChanges(dist,app).app).toContain('import.meta.env.ISSUE_859_API');
    put(dir, 'model.d.ts', 'export type Model = number;\n');
    put(dir, 'reference.d.ts', 'interface Referenced { value: number }\n');
    for (const file of ['model.d.ts','reference.d.ts']) expect(webChanges(dist,app).app).toContain(shown(resolve(dir,file)));
    put(stage,'model.d.ts',readFileSync(resolve(dir,'model.d.ts'),'utf8'));
    await expect(check(stage,resolve(libraries,'tsc'),libraries)).rejects.toThrow("Type 'string' is not assignable to type 'number'");
  } finally { rmSync(root,{recursive:true,force:true}); }
}, 30000);

test('checker paths track the sources of a published web build', async () => {
  const { checkerOutput, packageOrigin, recordSourceInputs } = await import('../src/__exact_env.mjs');
  const { shown, webChanges, webInputDigests } = await import('../../../scripts/agent-launch.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-checker-paths-'));
  const app = { dir, manifest: {} };
  const pending = resolve(dir, 'dist.stage-abcdef'), dist = resolve(dir, 'dist');
  const stage = resolve(pending, '.gen/typescript');
  const checkerPath = path => sep === '\\' ? path.replaceAll(sep, '/') : '/' + path;
  try {
    mkdirSync(stage, { recursive: true });
    writeFileSync(resolve(dir, 'app.ts'), 'export const value = 1;\n');
    writeFileSync(resolve(stage, '__exact_paths.json'), JSON.stringify({ app: dir }));
    const loaded = ['__exact_entry.ts', '__exact_env.d.ts', 'app.contract.d.ts', 'app.ts'];
    for (const file of loaded) writeFileSync(resolve(stage, file), file === 'app.ts' ? readFileSync(resolve(dir, file)) : '');
    const { files, resolution } = checkerOutput([
      ...loaded.map(file => checkerPath(resolve(stage, file))),
      `File '${checkerPath(resolve(stage, 'app.contract.ts'))}' does not exist.`,
    ].join('\r\n'));
    recordSourceInputs(stage, 'modules', [resolve(stage, 'app.ts')]);
    recordSourceInputs(stage, 'types', files.map(path => [path, packageOrigin(stage, path)]));
    recordSourceInputs(stage, 'resolution', resolution.map(path => [path, packageOrigin(stage, path)]));
    const sourceInputs = JSON.parse(readFileSync(resolve(stage, '__exact_inputs.json'), 'utf8'));
    writeFileSync(resolve(pending, '.exact-build.json'), JSON.stringify({
      target: 'js', inputs: webInputDigests(app, true, [], sourceInputs), inputRoots: [], sourceInputs,
    }));
    renameSync(pending, dist);
    expect(webChanges(dist, app).all).toEqual([]);
    expect(sourceInputs.reads).toContainEqual({ kind: 'absent', path: resolve(dir, 'app.contract.ts') });
    const old = new Date('2020-01-01');
    for (const file of ['app.contract.ts', 'app.ts']) {
      writeFileSync(resolve(dir, file), 'export const value = 2;\n');
      utimesSync(resolve(dir, file), old, old);
      expect(webChanges(dist, app).all).toContain(shown(resolve(dir, file)));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('resolution metadata and new candidates invalidate recorded inputs, including mounted sources', async () => {
  const { configure } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { receiptChanges, shown, webChanges, webInputDigests } = await import('../../../scripts/agent-launch.mjs');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-candidates-'));
  const dir = resolve(root, 'app'), shared = resolve(root, 'shared'), dist = resolve(dir, 'dist'), stage = resolve(dist, '.gen/typescript');
  const put = (base, file, code) => { mkdirSync(dirname(resolve(base, file)), { recursive: true }); writeFileSync(resolve(base, file), code); };
  const app = { dir, manifest: { env: ['ISSUE_859_API'], typescript: { sources: { core: '../shared' } } } };
  try {
    for (const mount of ['', 'core/']) {
      rmSync(stage, { recursive: true, force: true });
      const owner = mount ? shared : dir;
      const entry = `export { API } from './${mount}api';\n`;
      const source = "export const API = import.meta.env.ISSUE_859_API ?? 'https://old.example';\n";
      put(dir, 'app.json', JSON.stringify(app.manifest));
      put(stage, 'app.json', JSON.stringify(app.manifest));
      put(dir, 'app.ts', entry); put(stage, 'app.ts', entry);
      put(owner, 'api.js', source); put(stage, `${mount}api.js`, source);
      put(shared, 'unused.ts', 'export const unused = 1;\n');
      put(stage, 'core/unused.ts', 'export const unused = 1;\n');
      put(stage, '__exact_paths.json', JSON.stringify({ app: dir, mounts: [['core', shared]] }));
      const config = JSON.stringify({compilerOptions:{paths:{client:[`./${mount}client`]}}});
      for (const base of [dir, stage]) put(base, 'tsconfig.json', config);
      const withClient = entry + "export { value } from 'client';\n";
      put(dir, 'app.ts', withClient); put(stage, 'app.ts', withClient);
      for (const [file, code] of Object.entries({'client/package.json':'{"main":"one.js"}', 'client/one.js':'export const value = 1;', 'client/two.js':'export const value = 2;'})) {
        put(owner, file, code); put(stage, `${mount}${file}`, code);
      }
      await configure(stage, false, parseSync, true);
      const sourceInputs = JSON.parse(readFileSync(resolve(stage, '__exact_inputs.json'), 'utf8'));
      expect(sourceInputs.modules).toContainEqual([`${mount}api.js`, resolve(owner, 'api.js')]);
      put(dist, '.exact-build.json', JSON.stringify({ target: 'js', inputs: webInputDigests(app, true, [], sourceInputs), inputRoots: [], sourceInputs }));
      const receipt = resolve(root, 'receipt.json'), native = { ...app, target: resolve(root, 'target'), workspace: dir, crate: () => 'candidate-apple' };
      writeFileSync(receipt, JSON.stringify({ target: 'aarch64-apple-darwin', started: new Date(Date.now() + 60_000).toISOString(), build: { typescriptSources: sourceInputs } }));
      expect(webChanges(dist, app).all).toEqual([]);
      expect(receiptChanges(receipt, native)).toEqual([]);
      // A name no resolution would choose is no input, beside the importer or not.
      put(owner, 'out/shots/web.png', 'png'); put(owner, 'unrelated.ts', 'export const unrelated = 1;\n');
      expect(webChanges(dist, app).all).toEqual([]);
      expect(receiptChanges(receipt, native)).toEqual([]);
      rmSync(resolve(owner, 'out'), {recursive:true}); rmSync(resolve(owner, 'unrelated.ts'));
      put(owner, 'client/package.json', '{"main":"two.js"}');
      const metadata = resolve(owner, 'client/package.json'), old = new Date('2020-01-01');
      utimesSync(metadata, old, old);
      expect(webChanges(dist, app).all).toContain(shown(metadata));
      expect(receiptChanges(receipt, native)).toContain(shown(metadata));
      put(owner, 'client/package.json', '{"main":"one.js"}');
      // Older timestamps still change the resolver's preferred module.
      put(owner, 'api.ts', source.replace('old.example', 'new.example'));
      utimesSync(resolve(owner, 'api.ts'), old, old);
      expect(webChanges(dist, app).all).toContain(shown(resolve(owner, 'api.ts')));
      expect(receiptChanges(receipt, native)).toContain(shown(resolve(owner, 'api.ts')));
      put(stage, `${mount}api.ts`, readFileSync(resolve(owner, 'api.ts'), 'utf8'));
      await configure(stage, false, parseSync, true);
      expect(JSON.parse(readFileSync(resolve(stage, '__exact_inputs.json'), 'utf8')).modules).toContainEqual([`${mount}api.ts`, resolve(owner, 'api.ts')]);
      rmSync(resolve(owner, 'api.ts'));
      rmSync(resolve(owner, 'api.js'));
      rmSync(resolve(owner, 'client'), {recursive:true,force:true});
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 30_000);

test.each(['shadowed source', 'new runtime candidate'])('package resolution freshness: %s', async scenario => {
  const { configure, check, dataBundle } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { rolldown } = await import('rolldown');
  const { shown, webChanges, webInputDigests } = await import('../../../scripts/agent-launch.mjs');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-package-resolution-'));
  const dir = resolve(root, 'app'), dist = resolve(dir, 'dist'), stage = resolve(dist, '.gen/typescript');
  const app = {dir,manifest:{}}, pkg = resolve(dir, 'node_modules/client');
  const put = (base, file, code) => { mkdirSync(dirname(resolve(base, file)), {recursive:true}); writeFileSync(resolve(base, file), code); };
  const build = async () => {
    await configure(stage, false, parseSync, true);
    await check(stage, resolve(libraries, 'tsc'), libraries);
    const bundle = await rolldown({input:resolve(stage, '__exact_entry.ts'), ...dataBundle(stage), logLevel:'silent'});
    try { return (await bundle.generate({format:'esm'})).output[0].code; } finally { await bundle.close(); }
  };
  try {
    for (const base of [dir, stage]) {
      put(base, 'app.json', '{}');
      put(base, 'app.ts', "export { value } from 'client';\n");
    }
    put(stage, '__exact_paths.json', JSON.stringify({app:dir,mounts:[]}));
    put(stage, '__exact_entry.ts', "export * from './app';\n");
    put(pkg, 'package.json', JSON.stringify({name:'client',version:'1.0.0',type:'module',exports:'./index.js',types:'./index.d.ts'}));
    put(pkg, 'index.js', "export { value } from './value';\n");
    put(pkg, 'index.d.ts', 'export declare const value: string;\n');
    put(pkg, 'value.js', "export const value = 'from-js';\n");
    if (scenario === 'shadowed source') put(pkg, 'index.ts', "export const value = 'excluded';\n");
    expect(await build()).toContain('from-js');
    const sourceInputs = JSON.parse(readFileSync(resolve(stage, '__exact_inputs.json'), 'utf8'));
    put(dist, '.exact-build.json', JSON.stringify({target:'js',inputs:webInputDigests(app,true,[],sourceInputs),inputRoots:[],sourceInputs}));
    expect(webChanges(dist, app).all).toEqual([]);
    if (scenario === 'shadowed source') {
      put(pkg, 'index.ts', "export const value = 'changed';\n");
      expect(webChanges(dist, app).all).toContain(shown(resolve(pkg, 'index.ts')));
    } else {
      put(pkg, 'value.ts', "export const value = 'from-ts';\n");
      const old = new Date('2020-01-01');
      utimesSync(resolve(pkg, 'value.ts'), old, old);
      expect(webChanges(dist, app).all.length).toBeGreaterThan(0);
      expect(await build()).toContain('from-ts');
    }
  } finally { rmSync(root, {recursive:true,force:true}); }
}, 30_000);

test.each(['app', 'mount', 'package'])('package entry fallback freshness: %s', async owner => {
  const { configure, dataBundle } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { rolldown } = await import('rolldown');
  const { appSourceFiles } = await import('../../../scripts/app-imports.mjs');
  const { shown, webChanges, webInputDigests } = await import('../../../scripts/agent-launch.mjs');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-entry-fallback-'));
  const dir = resolve(root, 'app'), shared = resolve(root, 'shared'), dist = resolve(dir, 'dist'), stage = resolve(dist, '.gen/typescript');
  const mounted = owner === 'mount', packaged = owner === 'package';
  const mounts = mounted ? [['core', shared]] : [];
  const client = resolve(packaged ? resolve(dir, 'node_modules') : mounted ? shared : dir, 'client');
  const app = {dir, manifest:{env:packaged ? [] : ['ISSUE_859_API'], ...(mounted ? {typescript:{sources:{core:'../shared'}}} : {})}};
  const put = (base, file, code) => { mkdirSync(dirname(resolve(base, file)), {recursive:true}); writeFileSync(resolve(base, file), code); };
  const source = value => `export const API = ${packaged ? '' : 'import.meta.env.ISSUE_859_API ?? '}'https://${value}.example';\n`;
  const build = async () => {
    rmSync(stage, {recursive:true,force:true});
    for (const [file, from] of appSourceFiles(dir, {manifest:app.manifest, mounts})) put(stage, file, readFileSync(from));
    put(stage, '__exact_paths.json', JSON.stringify({app:dir,mounts}));
    put(stage, '__exact_entry.ts', "export * from './app';\n");
    await configure(stage, false, parseSync, true);
    const bundle = await rolldown({input:resolve(stage, '__exact_entry.ts'), ...dataBundle(stage), logLevel:'silent'});
    try { return (await bundle.generate({format:'esm'})).output[0].code; } finally { await bundle.close(); }
  };
  try {
    put(dir, 'app.json', JSON.stringify(app.manifest));
    if (!packaged) put(dir, 'tsconfig.json', JSON.stringify({compilerOptions:{paths:{client:[mounted ? './core/client' : './client']}}}));
    put(dir, 'app.ts', "export { API } from 'client';\n");
    put(client, 'package.json', JSON.stringify({name:'client',version:'1.0.0',type:'module',browser:'./browser/index.js',main:'./legacy.js'}));
    put(client, 'legacy.js', source('legacy'));
    mkdirSync(resolve(client, 'browser'));
    expect(await build()).toContain('https://legacy.example');
    const sourceInputs = JSON.parse(readFileSync(resolve(stage, '__exact_inputs.json'), 'utf8'));
    put(dist, '.exact-build.json', JSON.stringify({target:'js',inputs:webInputDigests(app,true,[],sourceInputs),inputRoots:[],sourceInputs}));
    expect(webChanges(dist, app).all).toEqual([]);
    put(client, 'browser/index.js', source('browser'));
    const old = new Date('2020-01-01');
    utimesSync(resolve(client, 'browser/index.js'), old, old);
    const changed = webChanges(dist, app).all;
    expect(await build()).toContain('https://browser.example');
    expect(changed).toContain(shown(resolve(client, 'browser/index.js')));
  } finally { rmSync(root, {recursive:true,force:true}); }
}, 30_000);

test.each(['app', 'mount'])('path mapping fallback freshness: %s', async owner => {
  const { configure, check, dataBundle } = await import('../src/typescript.mjs');
  const { sourceInputChanges } = await import('../src/__exact_env.mjs');
  const { parseSync } = await import('rolldown/utils');
  const { rolldown } = await import('rolldown');
  const { appSourceFiles } = await import('../../../scripts/app-imports.mjs');
  const { shown, webChanges, webInputDigests } = await import('../../../scripts/agent-launch.mjs');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-alias-fallback-'));
  const dir = resolve(root, 'app'), shared = resolve(root, 'shared'), dist = resolve(dir, 'dist'), stage = resolve(dist, '.gen/typescript');
  const mounted = owner === 'mount', mounts = mounted ? [['core', shared]] : [], local = mounted ? shared : dir;
  const app = {dir,manifest:{...(mounted ? {typescript:{sources:{core:'../shared'}}} : {})}};
  const put = (base, file, code) => { mkdirSync(dirname(resolve(base, file)), {recursive:true}); writeFileSync(resolve(base, file), code); };
  const build = async () => {
    rmSync(stage, {recursive:true,force:true});
    for (const [file, from] of appSourceFiles(dir, {manifest:app.manifest, mounts})) put(stage, file, readFileSync(from));
    put(stage, '__exact_paths.json', JSON.stringify({app:dir,mounts}));
    put(stage, '__exact_entry.ts', "export * from './app';\n");
    await configure(stage, false, parseSync, true);
    await check(stage, resolve(libraries, 'tsc'), libraries);
    const bundle = await rolldown({input:resolve(stage, '__exact_entry.ts'), ...dataBundle(stage), logLevel:'silent'});
    try { return (await bundle.generate({format:'esm'})).output[0].code; }
    finally { await bundle.close(); }
  };
  try {
    put(dir, 'app.json', JSON.stringify(app.manifest));
    put(local, 'preferred/value.d.ts', 'export declare const value: string;\n');
    put(local, 'fallback/value.js', "export const value = 'fallback';\n");
    for (const alias of ['client', 'client/*']) {
      const target = alias.includes('*') ? '*' : 'value', prefix = mounted ? 'core/' : '';
      put(dir, 'tsconfig.json', JSON.stringify({compilerOptions:{paths:{[alias]:[`./${prefix}preferred/${target}`, `./${prefix}fallback/${target}`]}}}));
      put(dir, 'app.ts', `export { value } from '${alias.replace('*','value')}';\n`);
      expect(await build()).toContain('"fallback"');
      const sourceInputs = JSON.parse(readFileSync(resolve(stage, '__exact_inputs.json'), 'utf8'));
      put(dist, '.exact-build.json', JSON.stringify({target:'js',inputs:webInputDigests(app,true,[],sourceInputs),inputRoots:[],sourceInputs}));
      expect(sourceInputChanges(sourceInputs)).toEqual([]);
      expect(webChanges(dist, app).all).toEqual([]);
      put(local, 'preferred/value.js', "export const value = 'preferred';\n");
      const changed = sourceInputChanges(sourceInputs), web = webChanges(dist, app).all;
      expect(await build()).toContain('"preferred"');
      expect(changed).toEqual([resolve(local, 'preferred/value.js')]);
      expect(web).toContain(shown(resolve(local, 'preferred/value.js')));
      rmSync(resolve(local, 'preferred/value.js'));
    }
  } finally { rmSync(root, {recursive:true,force:true}); }
}, 30_000);

test('type-only edges neither supply settings nor conflict with runtime defaults', async () => {
  const { configure, check } = await import('../src/typescript.mjs');
  const { parseSync } = await import('rolldown/utils');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const root = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-type-edges-'));
  const app = resolve(root, 'app'), stage = resolve(root, 'stage');
  const put = (dir, file, code) => { mkdirSync(resolve(dir, file, '..'), {recursive:true}); writeFileSync(resolve(dir, file), code); };
  const source = "export type Client = string;\nexport const API = import.meta.env.ISSUE_859_API ?? 'https://types.example';\n";
  try {
    put(app, 'app.json', JSON.stringify({env:['ISSUE_859_API']}));
    put(stage, '__exact_paths.json', JSON.stringify({app,mounts:[]}));
    put(stage, '__exact_entry.ts', "export * from './app';\n");
    put(stage, 'types.ts', source);
    for (const edge of ["import type { Client } from './types';", "import { Client } from './types'; export type Selected = Client;", "import { type Client } from './types';", "export type { Client } from './types';", "export { type Client } from './types';", "export type * from './types';"]) {
      const entry = edge + "\nexport const API = import.meta.env.ISSUE_859_API ?? 'https://runtime.example';\n";
      put(stage, 'app.ts', entry);
      await configure(stage, false, parseSync);
      await check(stage, resolve(libraries, 'tsc'), libraries);
      expect(readFileSync(resolve(stage, 'types.ts'), 'utf8')).toBe(source);
      put(stage, 'app.ts', edge);
      await expect(configure(stage, false, parseSync)).rejects.toThrow('read it nowhere');
    }
    for (const edge of ["import './types';", "import { API, type Client } from './types'; export { API };", "export { API, type Client } from './types';"]) {
      put(stage, 'app.ts', edge);
      await configure(stage, false, parseSync);
      expect(JSON.parse(readFileSync(resolve(stage, '__exact_env.json'), 'utf8')).ISSUE_859_API.value).toBe('https://types.example');
      put(stage, 'types.ts', source);
    }
  } finally { rmSync(root, {recursive:true,force:true}); }
}, 30_000);

test('reachable declarations and type-only modules stay fresh without reading settings', async () => {
  const { shown, webChanges, webInputDigests, webInputs } = await import('../../../scripts/agent-launch.mjs');
  const { appSettings } = await import('../../../scripts/app-imports.mjs');
  const dir = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-declarations-'));
  const app = {dir,manifest:{env:[]}}, dist = resolve(dir, 'dist');
  const put = (file, code) => writeFileSync(resolve(dir, file), code);
  try {
    mkdirSync(dist);
    put('app.json', JSON.stringify(app.manifest));
    put('app.ts', "import { Model } from './model.d.ts';\nimport type { Client } from './types';\nimport type { Details } from './details';\nexport const value: Model = 'hello';\n");
    put('model.d.ts', 'export type Model = string;\n');
    put('details.d.ts', 'export type Details = string;\n');
    put('types.ts', "export type Client = string;\nexport const API = import.meta.env.UNDECLARED;\n");
    mkdirSync(resolve(dir,'.exact'),{recursive:true});
    put('.exact/typescript-inputs.json',JSON.stringify({modules:[['app.ts',resolve(dir,'app.ts')]],types:['model.d.ts','types.ts','details.d.ts'].map(file=>[file,resolve(dir,file)])}));
    const inputs = webInputs(app, true, -Infinity, []).app;
    for (const file of ['model.d.ts', 'types.ts', 'details.d.ts']) expect(inputs).toContain(shown(resolve(dir, file)));
    expect(appSettings(dir, {}).record).toEqual({});
    put('dist/.exact-build.json', JSON.stringify({target:'js',inputs:webInputDigests(app,true,[]),inputRoots:[]}));
    expect(webChanges(dist, app).all).toEqual([]);
    put('model.d.ts', 'export type Model = number;\n');
    expect(webChanges(dist, app).app).toContain(shown(resolve(dir, 'model.d.ts')));
    put('types.ts', 'export type Client = number;\n');
    expect(webChanges(dist, app).app).toContain(shown(resolve(dir, 'types.ts')));
    put('model.d.ts', "export type Model = string;\nexport declare const API: typeof import.meta.env.UNDECLARED;\n");
    expect(appSettings(dir, {}).record).toEqual({});
  } finally { rmSync(dir, {recursive:true,force:true}); }
});

test('web settings inline app-local consts, record input changes and refuse retained package reads', async () => {
  const {webCompiler} = await import('../../../host/web-js/module.mjs');
  const {builtFile} = await import('../../../host/web-js/content-names.mjs');
  const {webChanges} = await import('../../../scripts/agent-launch.mjs');
  const {spawnSync} = await import('node:child_process');
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../../..'), dir = mkdtempSync(resolve(root, 'target/web-unused-exports-'));
  const pkg = resolve(dir, 'node_modules/debug-helper'), out = resolve(dir, 'dist');
  try {
    mkdirSync(pkg, { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Unused', app: { id: 'test.unused', name: 'Unused' }, env:['ISSUE_859_API'] }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  resource message = message() as shape string\n  view\n    column\n      text message\n      canvas surface=spark() width=10 height=10\n');
    writeFileSync(resolve(pkg, 'package.json'), JSON.stringify({ name: 'debug-helper', version: '1.0.0', type: 'module', exports: './index.js', types: './index.d.ts' }));
    writeFileSync(resolve(pkg, 'index.js'), 'export const debug = () => import.meta.env.DEBUG;\n');
    writeFileSync(resolve(pkg, 'index.d.ts'), 'export declare const debug: () => string;\n');
    const build = () => spawnSync(process.execPath, ['host/web/build.mjs', dir, '--js', '--render', 'none'],
      { cwd: root, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir, EXACT_WEB_DIST:out } });
    mkdirSync(resolve(dir,'src'));
    writeFileSync(resolve(dir,'src/api.ts'), "export const API = import.meta.env.ISSUE_859_API ?? 'https://built.example';\n");
    const base = "import './src/api';\nimport { API } from './src/api';\nexport const grants = '';\nexport const surfaces = { spark: 0 };\nexport const draw = () => false;\n";
    writeFileSync(resolve(dir, 'app.ts'), base + "export const answer = () => API;\nexport { debug } from 'debug-helper';\n");
    const built = build();
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const code = readFileSync(resolve(out, builtFile(out, 'app.js')), 'utf8');
    expect(code).not.toContain('import.meta.env');
    expect(code).toContain('https://built.example');
    const app = {dir,manifest:{env:['ISSUE_859_API']}};
    expect(webChanges(out,app).all).toEqual([]);
    writeFileSync(resolve(dir,'src/extra.ts'),"export const marker = 'extra';\n");
    writeFileSync(resolve(dir,'app.ts'),base+"export const answer = () => API;\nexport { marker } from './src/extra';\n");
    expect(webChanges(out,app).all).toContain(resolve(dir,'app.ts').slice(root.length+1));
    let rebuilt=build();
    expect(rebuilt.status,rebuilt.stdout+rebuilt.stderr).toBe(0);
    expect(webChanges(out,app).all).toEqual([]);
    writeFileSync(resolve(dir,'app.ts'),base+"export const answer = () => API;\n");
    rebuilt=build();
    expect(rebuilt.status,rebuilt.stdout+rebuilt.stderr).toBe(0);
    // No import names it any more, so its removal changes no resolution.
    rmSync(resolve(dir,'src/extra.ts'));
    expect(webChanges(out,app).all).toEqual([]);
    for (const source of [
      base + 'export const answer = () => debug();\n',
      base.replace('() => false', '() => !!debug()') + "export const answer = () => 'ready';\n",
    ]) {
      writeFileSync(resolve(dir, 'app.ts'), "import { debug } from 'debug-helper';\n" + source);
      const refused = build();
      expect(refused.status).toBe(1);
      expect(refused.stderr).toContain('debug-helper@1.0.0 (index.js:1:28) reads import.meta.env');
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60000);

test('a setting is a web build input by value and source, and serve finds one changed since the build', async () => {
  const {webChanges,webInputDigests}=await import('../../../scripts/agent-launch.mjs');
  const {changedSettings}=await import('../src/__exact_env.mjs');
  const dir=mkdtempSync(resolve(tmpdir(),'exact-settings-')), dist=resolve(dir,'dist'), name='EXACT_TEST_SETTING', key=`import.meta.env.${name}`;
  const app={dir,manifest:{env:[name]}};
  const bake=env=>writeFileSync(resolve(dist,'.exact-build.json'),JSON.stringify({target:'js',inputs:webInputDigests(app,true,[]),inputRoots:[],env}));
  try {
    mkdirSync(dist);
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify(app.manifest));
    writeFileSync(resolve(dir, 'app.ts'), `const API = import.meta.env.${name};\n`);
    mkdirSync(resolve(dir,'.exact'),{recursive:true});
    writeFileSync(resolve(dir,'.exact/typescript-inputs.json'),JSON.stringify({modules:[['app.ts',resolve(dir,'app.ts')]],types:[]}));
    writeFileSync(resolve(dir,'.env'),`${name}=http://a.example\n`);
    bake({[name]:{value:'http://a.example',source:'.env'}});
    expect(webChanges(dist,app).all).toEqual([]);
    writeFileSync(resolve(dir,'.env'),`${name}=http://b.example\n`);
    expect(webChanges(dist,app).app).toEqual([key]);
    expect(changedSettings(dist,{},dir)).toEqual([{name,built:{value:'http://a.example',source:'.env'},now:{value:'http://b.example',source:'.env'}}]);
    // The same value from another source is another build input, and the same build for serve.
    writeFileSync(resolve(dir,'.env'),`${name}=http://a.example\n`);
    expect(webChanges(dist,app).all).toEqual([]);
    writeFileSync(resolve(dir,'.env.local'),`${name}=http://a.example\n`);
    expect(webChanges(dist,app).app).toEqual([key]);
    expect(changedSettings(dist,{},dir)).toEqual([]);
    rmSync(resolve(dir,'.env.local'));
    process.env[name]='http://e.example';
    try { expect(webChanges(dist,app).app).toEqual([key]); } finally { delete process.env[name]; }
    // A build copied away from its sources: only the environment changes a value, and an unset name keeps the build's.
    expect(changedSettings(dist,{})).toEqual([]);
    expect(changedSettings(dist,{[name]:'http://c.example'})).toEqual([{name,built:{value:'http://a.example',source:'.env'},now:{value:'http://c.example',source:'environment'}}]);
    // A default stands until a source sets a value, and the digest records it as the bake inlines it.
    rmSync(resolve(dir,'.env'));
    writeFileSync(resolve(dir,'app.json'),JSON.stringify({env:[name]}));
    mkdirSync(resolve(dir,'src'));
    writeFileSync(resolve(dir,'app.ts'),"export { API } from './src/api';\n");
    writeFileSync(resolve(dir,'src/api.ts'),`export const API = import.meta.env.${name} ?? 'http://default.example';\n`);
    writeFileSync(resolve(dir,'.exact/typescript-inputs.json'),JSON.stringify({modules:['app.ts','src/api.ts'].map(file=>[file,resolve(dir,file)]),types:[]}));
    expect(webInputDigests(app,true)[key]).toBe(JSON.stringify({value:'http://default.example',source:'default'}));
    bake({[name]:{value:'http://default.example',source:'default'}});
    expect(webChanges(dist,app).all).toEqual([]);
    expect(changedSettings(dist,{},dir)).toEqual([]);
    expect(changedSettings(dist,{[name]:'http://d.example'},dir).map(c=>c.now)).toEqual([{value:'http://d.example',source:'environment'}]);
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('setting freshness follows current imports, including added and removed modules', async () => {
  const { shown, webChanges, webInputDigests, webInputs } = await import('../../../scripts/agent-launch.mjs');
  const { utimesSync } = await import('node:fs');
  const dir = mkdtempSync(resolve(import.meta.dir, '../../../target/issue-859-freshness-'));
  const app = {dir,manifest:{env:['ISSUE_859_API']}}, dist = resolve(dir, 'dist'), key = 'import.meta.env.ISSUE_859_API';
  const put = (file, code) => writeFileSync(resolve(dir, file), code);
  const source = value => `export const API = import.meta.env.ISSUE_859_API ?? '${value}';\n`;
  const digest = () => webInputDigests(app, true, []);
  const bake = () => put('dist/.exact-build.json', JSON.stringify({target:'js',inputs:digest(),inputRoots:[]}));
  try {
    mkdirSync(dist);
    put('app.json', JSON.stringify(app.manifest));
    put('app.ts', "export { API } from './settings';\nexport { default as seed } from './seed.json';\n");
    put('seed.json', '{"count":1}');
    put('settings.ts', source('https://old.example'));
    put('unused.ts', source('https://unused.example'));
    mkdirSync(resolve(dir,'.exact'),{recursive:true});
    put('.exact/typescript-inputs.json',JSON.stringify({modules:['app.ts','settings.ts','seed.json'].map(file=>[file,resolve(dir,file)]),types:[]}));
    expect(digest()[key]).toBe(JSON.stringify({value:'https://old.example',source:'default'}));
    expect(webInputs(app, true, -Infinity, []).app).not.toContain(shown(resolve(dir, 'unused.ts')));
    expect(digest()[shown(resolve(dir, 'seed.json'))]).not.toBeNull();
    bake();
    put('unused.ts', source('https://other.example'));
    expect(webChanges(dist, app).all).toEqual([]);
    put('settings.ts', source('https://new.example'));
    expect(webChanges(dist, app).app).toContain(key);
    bake();
    rmSync(resolve(dir, 'settings.ts'));
    expect(webChanges(dist, app).app).toContain(key);
    bake();
    // The missing import's first creation counts even with a time older than the build.
    put('settings.ts', source('https://added.example'));
    utimesSync(resolve(dir, 'settings.ts'), new Date(0), new Date(0));
    expect(webChanges(dist, app).app).toContain(key);
    expect(webChanges(dist, app).app).toContain(shown(resolve(dir, 'settings.ts')));
    bake();
    put('app.ts', 'export {};\n');
    expect(webChanges(dist, app).app).toContain(shown(resolve(dir,'app.ts')));
    put('.exact/typescript-inputs.json',JSON.stringify({modules:[['app.ts',resolve(dir,'app.ts')]],types:[]}));
    expect(digest()[key]).toBeNull();
  } finally { rmSync(dir, {recursive:true,force:true}); }
});
