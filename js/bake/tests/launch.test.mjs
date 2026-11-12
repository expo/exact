// A setting a launch may give (LLP 1153.002): the bake's rewrite of a marked read and its refusals
// (`__exact_env.mjs` `inlineSettings`, `typescript.mjs` `launchGrants`), what Bun loaded beside what a launch gave,
// and how js/src/launch.js names the setting in a grant refusal.
import { expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import vm from 'node:vm';

test('a marked setting is read at launch with the build\'s value behind it, an unmarked one is inlined as before, and its misuses are refused', async () => {
  const {inlineSettings,settingLines} = await import('../src/__exact_env.mjs');
  const {launchGrants,launchIntro,staticFacts} = await import('../src/typescript.mjs');
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
    expect(set.code).toBe('const API = (globalThis as unknown as { readonly __exact_launch?: { readonly [name: string]: string } }).__exact_launch?.["API_BASE_URL"] ?? "http://127.0.0.1:9999";\nconst PICTURES = "https://pictures.example";\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n');
    expect(set.record).toEqual({API_BASE_URL:{value:'http://127.0.0.1:9999',source:'.env',launch:true},PICTURES:{value:'https://pictures.example',source:'.env'}});
    // The web's bundles, which no launch gives a value: the build's, inlined, and the same record.
    const web=inlineSettings(code,dir,parseSync,{},{launches:false});
    expect(web).toEqual({code:'const API = "http://127.0.0.1:9999";\nconst PICTURES = "https://pictures.example";\nexport const grants = `net.fetch https:\\nnet.fetch ${PICTURES}`;\n',record:set.record});
    expect(staticFacts(set.code,parseSync).grants).toBe('net.fetch https:\nnet.fetch https://pictures.example');
    // The grants carry each marked name, which a host answers a launch's value for and no other; the bake writes them.
    expect(staticFacts(launchGrants(set.code,parseSync,['API_BASE_URL']),parseSync).grants).toBe('net.fetch https:\nnet.fetch https://pictures.example\nenv.launch API_BASE_URL');
    expect(launchGrants(set.code,parseSync,[])).toBe(set.code);
    expect(launchGrants("export const appId = 'a';\n",parseSync,['API_BASE_URL'])).toBe("export const appId = 'a';\n\nexport const grants = \"env.launch API_BASE_URL\";\n");
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
    // The bundle's first line asks for the marked names, and for none where no name is marked.
    const intro=record=>{ writeFileSync(resolve(dir,'__exact_env.json'),JSON.stringify(record)); return launchIntro(dir).intro(); };
    expect(intro({API_BASE_URL:{value:'http://x',source:'default',launch:true},PICTURES:{value:'p',source:'.env'}})).toBe('globalThis.__exact_launch_settings?.(["API_BASE_URL"]);');
    expect(intro({PICTURES:{value:'p',source:'.env'}})).toBe('');
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

test('a value the process holds is the launch\'s unless it is the one Bun loaded', async () => {
  const {settingChanges,settingValues} = await import('../src/__exact_env.mjs');
  const dir=mkdtempSync(resolve(tmpdir(),'exact-launch-bun-')), name='EXACT_TEST_SETTING';
  const loaded=value=>`OTHER=${'0'.repeat(64)},${name}=${createHash('sha256').update(value).digest('hex')}`;
  const built={[name]:{value:'http://a.example',source:'.env',launch:true}};
  try {
    writeFileSync(resolve(dir,'.env'),`${name}=http://a.example\n`);
    // Bun loaded A and the process holds A: the build's, from its file, and no change.
    const bun={[name]:'http://a.example',EXACT_BUN_LOADED:loaded('http://a.example')};
    expect(settingValues(dir,[name],bun)).toEqual({[name]:{value:'http://a.example',source:'.env'}});
    expect(settingChanges(built,bun,dir,{launch:true})).toEqual([]);
    // Bun loaded A and a command or a launcher below set B: the environment's, which a launch gives (no change where a
    // launch gives the value) and a build takes.
    const set={[name]:'http://b.example',EXACT_BUN_LOADED:loaded('http://a.example')};
    expect(settingValues(dir,[name],set)).toEqual({[name]:{value:'http://b.example',source:'environment'}});
    expect(settingChanges(built,set,dir,{launch:true})).toEqual([]);
    expect(settingChanges(built,set,dir).map(c=>c.now)).toEqual([{value:'http://b.example',source:'environment'}]);
    // Nothing loaded: what the process holds is the environment's.
    expect(settingValues(dir,[name],{[name]:'http://a.example'})).toEqual({[name]:{value:'http://a.example',source:'environment'}});
  } finally { rmSync(dir,{recursive:true,force:true}); }
});

test('a grant refusal at a launch value\'s origin names the setting, origins compared as the URL standard has them', () => {
  // js/src/launch.js in a realm of its own, its door answering the launch's values.
  const values={DEFAULT_PORT:'https://api.example:443/v1',UPPER:'https://UPPER.Example/v1',CREDENTIALS:'https://user:secret@credentials.example/v1',OTHER_PORT:'https://port.example:8443'};
  const realm={__exact_host:(op,name)=>op===27 ? values[name] : undefined,URL};
  new Function('globalThis',readFileSync(resolve(import.meta.dir,'../../src/launch.js'),'utf8'))(realm);
  realm.__exact_launch_settings(Object.keys(values));
  expect(realm.__exact_launch).toEqual(values);
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

test('a page realm, which no launch gives a value, reads the build\'s whatever a package sets first', () => {
  // The wasm target's module realm runs the native bundle over prelude.js alone, without js/src/launch.js.
  const realm=vm.createContext({URL,TextEncoder,TextDecoder,AbortController,AbortSignal,crypto,__exact_host:()=>'',console:{error() {}}});
  for (const file of ['binary.js','prelude.js']) vm.runInContext(readFileSync(resolve(import.meta.dir,'../../src',file),'utf8'),realm);
  vm.runInContext("globalThis.__exact_launch_settings?.(['API']); try { globalThis.__exact_launch = { API: 'https://package.example' }; } catch {} globalThis.read = globalThis.__exact_launch?.API ?? 'http://built.test';",realm);
  expect(realm.read).toBe('http://built.test');
});
