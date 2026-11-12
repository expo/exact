// A setting a launch may give (LLP 1153.002), as the producers build for it: the grant line a host answers a name for,
// and the stage's launch module that asks for the values first. The native bake copies it beside `__exact_config.mjs`
// (typescript.mjs), which imports it (js/bake/src/resident.rs `LAUNCH`); the web build and the scripts import it here.
import { existsSync, readFileSync, realpathSync, unlinkSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';

// A path as the disk names it, as typescript.mjs's `physicalId`.
const physicalId=id=>{ if (!isAbsolute(id)) return id; try { return realpathSync.native(id); } catch { return id; } };

/** The declaration of the binding app.ts exports as `grants`, as `staticFacts` reads it. */
export function grantsDeclaration(body) {
  const local=body.flatMap(s=>s.type==='ExportNamedDeclaration' && !s.source ? (s.specifiers ?? []) : [])
    .find(x=>(x.exported.name ?? x.exported.value)==='grants')?.local;
  const name=local ? local.name ?? local.value : 'grants';
  return body.map(s=>s.type==='ExportNamedDeclaration' ? s.declaration : s)
    .flatMap(s=>s?.type==='VariableDeclaration' ? s.declarations : []).find(d=>d.id.type==='Identifier' && d.id.name===name);
}

/** `code` (app.ts) with an `env.launch <NAME>` grant line for each of `names`, the settings app.json marks a launch
 * may give (LLP 1153.002 D2): a host answers a launch's value only for a name the module's grants carry, so the lines
 * are the bake's, and one app.ts writes itself is refused (the native bake checks the module's grants again,
 * js/bake/src/lib.rs). Only the captured copy is rewritten, before `developmentGrants`, so every producer and host
 * reads the same grants. `staticFacts` evaluates them (typescript.mjs). */
export function launchGrants(code, parse, names, staticFacts) {
  const parsed=parse('app.ts',code);
  if (parsed.errors.length) return code;
  // The grants as evaluated, however they are spelled (`'env.' + 'launch HOME'`), a development build's line included.
  const grants=staticFacts(code,parse).grants;
  const written=(grants ?? '').split('\n').map(line=>line.trim()).filter(line=>/^(?:development\s+)?env\.launch(?:\s|$)/.test(line));
  if (written.length) throw new Error(written.map(line=>`app.ts: grants \`${line}\`: an env.launch line is the bake's, written for each name app.json's "env" marks { "name": "NAME", "launch": true }; mark the setting there and remove the line`).join('\n'));
  if (!names.length) return code;
  const value=JSON.stringify([...grants ? [grants] : [], ...names.map(name=>`env.launch ${name}`)].join('\n'));
  // Only the binding app.ts exports as `grants` is rewritten: a private one of that name is the app's own.
  const declaration=grants===undefined ? null : grantsDeclaration(parsed.program.body);
  if (declaration) return code.slice(0,declaration.init.start)+value+code.slice(declaration.init.end);
  // Grants another module may give: a declaration added here would shadow (`export *`) or duplicate them. A type-only
  // re-export (`export type * from`, `export { type grants } from`) gives no value.
  const exported=x=>x && (x.name ?? x.value);
  const reexport=parsed.program.body.find(s=>s.exportKind!=='type' && (s.type==='ExportAllDeclaration' ? !s.exported || exported(s.exported)==='grants'
    : s.type==='ExportNamedDeclaration' && s.source && (s.specifiers ?? []).some(x=>x.exportKind!=='type' && exported(x.exported)==='grants')));
  if (reexport) throw new Error(`app.ts: \`${code.slice(reexport.start,reexport.end).replace(/;$/,'')}\` may give the module its grants, and ${names.join(', ')} is marked "launch": true in app.json, whose env.launch line the bake adds to grants declared in app.ts: declare export const grants in app.ts`);
  if (!topLevelNames(parsed.program.body).has('grants')) return `${code}\nexport const grants = ${value};\n`;
  // Beside the app's own `grants`, the bake's lines take a name of their own, exported as `grants`.
  let local='__exact_launch_grants';
  while (code.includes(local)) local+='_';
  return `${code}\nconst ${local} = ${value};\nexport { ${local} as grants };\n`;
}

// The names a pattern binds (`const { a, b: [c] } = …`).
const patternNames=p=>!p ? [] : p.type==='Identifier' ? [p.name] : p.type==='ObjectPattern' ? p.properties.flatMap(x=>patternNames(x.type==='RestElement' ? x.argument : x.value))
  : p.type==='ArrayPattern' ? p.elements.flatMap(patternNames) : p.type==='AssignmentPattern' ? patternNames(p.left) : p.type==='RestElement' ? patternNames(p.argument) : [];
// The names app.ts binds at its top level: its declarations, functions, classes, enums and namespaces, and its imports.
const topLevelNames=body=>new Set(body.flatMap(statement=>{
  const s=statement.type==='ExportNamedDeclaration' ? statement.declaration ?? statement : statement;
  if (s.type==='ImportDeclaration') return (s.specifiers ?? []).map(x=>x.local.name);
  if (s.type==='VariableDeclaration') return s.declarations.flatMap(d=>patternNames(d.id));
  return s.id?.name ? [s.id.name] : [];
}));

/** The stage's own `__exact_launch.ts` (`launchModule`) for an import of
 * `__exact_launch` beside it, whatever else would resolve there (an app's
 * `__exact_launch.tsx`, which resolution tries first; the bake refuses any). */
export function launchSource(stage) {
  stage=realpathSync.native(stage);
  const module=join(stage,'__exact_launch.ts');
  return { name:'exact-launch-module', resolveId(source, importer) {
    return importer && !importer.startsWith('\0') && source.startsWith('.') && resolve(dirname(physicalId(importer)),source)===join(stage,'__exact_launch') ? module : null;
  } };
}
const LAUNCH_IMPORT="import './__exact_launch';\n";
/** Where a native build's app.ts reads settings a launch may give (`names`,
 * LLP 1153.002 D2), the stage's `__exact_launch.ts` asks for their values:
 * the prelude's `__exact_launch_settings` (js/src/launch.js) asks the host
 * once and answers a frozen object with no prototype, which app.ts's reads
 * take before the build's value (`__exact_env.mjs` `launchRead`). The bundler
 * parses it as any module, so a module's binding that would shadow a global
 * it reads, or its export, is renamed. The entry imports it first, so ES
 * module order runs it before any module of the app or a package; only the
 * runtime's own scripts run before the bundle. A realm without the function
 * (the wasm target's page, an older binary) gets an empty object and the
 * build's values; the receipt asks for the runtime that has it
 * (`bake/src/compat.rs`). With no `names`, neither the file nor the import is
 * there. */
export function launchModule(stage, names) {
  const entry=resolve(stage,'__exact_entry.ts'), module=resolve(stage,'__exact_launch.ts');
  const before=existsSync(entry) ? readFileSync(entry,'utf8') : null;
  let after=before?.startsWith(LAUNCH_IMPORT) ? before.slice(LAUNCH_IMPORT.length) : before;
  if (names.length) {
    const code=`type Settings = { readonly [name: string]: string | undefined };\nexport const launched: Settings = (globalThis as unknown as { __exact_launch_settings?: (names: string[]) => Settings }).__exact_launch_settings?.(${JSON.stringify(names)}) ?? Object.freeze(Object.create(null));\n`;
    if (!existsSync(module) || readFileSync(module,'utf8')!==code) writeFileSync(module,code);
    if (after!==null) after=LAUNCH_IMPORT+after;
  } else if (existsSync(module)) unlinkSync(module);
  if (after!==before) writeFileSync(entry,after);
}
