// The one TypeScript configuration for an app's data module (`app.ts`), and
// the check every producer runs with it: the native bake (one-shot and
// resident, js/bake/src), which copies this file into its stage as
// `__exact_config.mjs`, and the web build (host/web-js/build.mjs), which
// imports it from here. One app.ts builds on every host or on none, refused
// with the same diagnostics at the same point (calc F2, calendar F9/F11).
// It reads only captured configuration; the producer owns the stage.
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, readSync, readdirSync, realpathSync, rmdirSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { builtinModules, createRequire } from 'node:module';
import { resolve, relative, dirname, basename, join, sep, isAbsolute, toNamespacedPath } from 'node:path';
// The bake writes the binding's generator beside this file in its stage (js/bake/src/resident.rs `BINDING`).
import { moduleBinding, languageModelToolCheck, languageModelToolNote, languageModelTools, languageModelToolSources, TOOL_CHECK, unownedTools } from './__exact_binding.mjs';
// And the settings' reader beside it (resident.rs `ENV`).
import { envReads, inlineSettings } from './__exact_env.mjs';
// And the check of what package code may use (resident.rs `PORTABLE`).
import { bare, lineColumn, packageRefusals, propertyName, walkRuntime } from './__exact_portable.mjs';

export { moduleBinding };

function httpOrigin(value, label) {
  let url; try { url=new URL(value); } catch { throw new Error(`${label}: expected an http(s) origin`); }
  if (!['http:','https:'].includes(url.protocol) || url.username || url.password || url.pathname!=='/' || url.search || url.hash)
    throw new Error(`${label}: expected an http(s) origin without credentials, path, query or fragment`);
  return url.origin;
}

/** Carry a drive flag's selection into its child builds. */
export function selectBackendOrigin(value, env=process.env) {
  if (!value || value.startsWith('--')) throw new Error('--backend <url>: name the backend origin');
  env.EXACT_BACKEND_ORIGIN=value;
  env.EXACT_BACKEND_PROVENANCE=JSON.stringify({origin:value,source:'--backend'});
}

/** Explicit backend, saved development selection, then the checkout's declared fixture default. */
export function backendOriginSelection(app, env=process.env, parse) {
  const file=resolve(app,'.exact/backend-origin');
  const value=env.EXACT_BACKEND_ORIGIN ?? (existsSync(file) ? readFileSync(file,'utf8').trim() : null);
  if (value===null) return fixtureBackendSelection(app,env,parse);
  // A child build keeps the flag's provenance only while it names that exact selection.
  let flag; try { flag=JSON.parse(env.EXACT_BACKEND_PROVENANCE ?? 'null'); } catch { /* no flag provenance */ }
  const source=env.EXACT_BACKEND_ORIGIN != null ? (flag?.origin===value && flag?.source==='--backend' ? '--backend' : 'EXACT_BACKEND_ORIGIN') : '.exact/backend-origin';
  try { return { origin: httpOrigin(value,source), source }; }
  catch (error) { throw Object.assign(error,{code:'backend-selection'}); }
}
export const backendOrigin = (app, env=process.env) => backendOriginSelection(app, env)?.origin ?? null;

/** A declared fixture gets a stable development port from its app directory, shared by builds and drives. */
export function fixtureBackendSelection(app, env=process.env, parse) {
  if (!developmentTrust(env)) return null;
  let manifest, code;
  try { manifest=JSON.parse(readFileSync(resolve(app,'app.json'),'utf8')); code=readFileSync(resolve(app,'app.ts'),'utf8'); } catch { return null; }
  const argv=manifest.commands?.server;
  if (!Array.isArray(argv) || !/(^|\/)bun$/.test(argv[0])) return null;
  const entry=argv.findIndex((a,i)=>i>0 && !a.startsWith('-'));
  if (entry<0 || !/(^|[/\\])fixtures?[/\\]/.test(argv[entry])) return null;
  const literal=literalBackendOrigin(code,parse ?? createRequire(import.meta.url)('rolldown/utils').parseSync);
  if (!literal || !loopback(literal)) return null;
  const args=argv.slice(entry+1), at=args.indexOf('--port'), given=at<0 ? env.PORT : args[at+1];
  // Rust canonicalizes Windows paths with a namespace prefix; server drives must hash the same spelling.
  const port=given===undefined ? 4790+parseInt(createHash('sha256').update(toNamespacedPath(realpathSync(app))).digest('hex').slice(0,8),16)%1000 : Number(given);
  if (!Number.isInteger(port) || port<1 || port>65535) throw new Error(`${at<0 ? 'PORT' : 'commands.server --port'}: a fixture port must be an integer from 1 to 65535`);
  const origin=new URL(literal); origin.port=String(port);
  return { origin:httpOrigin(origin.href,'fixture origin'), source:given===undefined ? 'checkout fixture port' : at<0 ? 'PORT' : 'commands.server --port' };
}

// The bound on a type check that never ends (the design tool's `build.mjs --check` spun 4.7 days, found 2026-10-26):
// 10 minutes, a bound for "never", not a target (the clone's check takes seconds). A test may shorten it
// (EXACT_CHECK_BOUND_MS), read only under EXACT_TEST_TIMING=1, as the design server's clocks are
// (scripts/tamber-timing.mjs); a value not shorter, not a whole number of ms from 50, or set without the switch is
// ignored. Here, not in a module of its own: the native bake copies this file alone into its stage.
export const CHECK_BOUND_MS = 10 * 60_000;

export function checkBound(env = process.env) {
  if (env.EXACT_TEST_TIMING !== '1') return CHECK_BOUND_MS;
  const said = env.EXACT_CHECK_BOUND_MS, ms = /^\d+$/.test(said ?? '') ? Number(said) : NaN;
  return ms >= 50 && ms < CHECK_BOUND_MS ? ms : CHECK_BOUND_MS;
}

/** The bound in words: `10 min`, `1.5 s`. */
export const boundWords = ms => (ms >= 60_000 ? `${ms / 60_000} min` : `${ms / 1000} s`);

/** Explain when an app-local selection takes precedence over the authored origin. */
export function backendOriginWarning(code, selection, parse) {
  if (selection?.source !== '.exact/backend-origin') return null;
  const literal=literalBackendOrigin(code,parse);
  if (!literal || literal===selection.origin) return null;
  return `.exact/backend-origin selects ${selection.origin}, overriding the app.ts origin literal ${JSON.stringify(literal)}. Edit or remove .exact/backend-origin to use the changed literal; EXACT_BACKEND_ORIGIN overrides both.`;
}

/** The top-level const origin the backend selection can replace, canonicalized as an http(s) origin. */
export function literalBackendOrigin(code, parse) {
  const parsed=parse('app.ts',code);
  if (parsed.errors.length) return null;
  const declarations=parsed.program.body.map(s=>s.type==='ExportNamedDeclaration' ? s.declaration : s)
    .filter(s=>s?.kind==='const').flatMap(s=>s.declarations);
  const init=bare(declarations.find(d=>d.id.type==='Identifier' && d.id.name==='origin')?.init);
  const literal=init?.type==='Literal' && typeof init.value==='string' ? init.value
    : init?.type==='TemplateLiteral' && !init.expressions.length ? init.quasis[0].value.cooked : null;
  if (!literal) return null;
  try {
    const url=new URL(literal);
    return ['http:','https:'].includes(url.protocol) && !url.username && !url.password ? url.origin : null;
  } catch { return null; }
}
/** The socket origin at an http(s) origin's host: ws for http, wss for https. */
const socketOrigin = (origin) => origin.replace(/^http/, 'ws');
// Validate every entry, including those this bake does not select. No value
// in this form goes through the constant evaluator or the plain form's grant
// URL rewrite. The entry is the selected origin's own; in a development build
// only, an origin at a private-network address with no entry takes the entry
// of the server at loopback, its fetch and socket
// lines moved to that address: a phone reaching the fixture on this Mac's LAN.
// A production build refuses it as any missing entry.
function keyedGrants(node, origin, at, development) {
  const entries=new Map();
  const refuse=(node, why)=>{ throw new Error(`app.ts:${at(node.start)}: grants: ${why}`); };
  const line=(node, place)=>{
    if (node?.type!=='Literal' || typeof node.value!=='string')
      refuse(node ?? place, `expected a string literal grant line, got ${node?.type ?? 'array hole'}`);
    if (/[\r\n\u2028\u2029]/.test(node.value)) refuse(node,'each string literal must contain one grant line');
    return node.value;
  };
  for (const property of node.properties) {
    if (property.type==='SpreadElement') refuse(property,'spread entries are not allowed');
    if (property.computed) refuse(property.key,'computed keys are not allowed');
    if (property.method || property.kind!=='init') refuse(property,'methods and accessors are not allowed');
    const key=property.key;
    if (key.type!=='Literal' || typeof key.value!=='string') refuse(key,`expected a string literal origin key, got ${key.type}`);
    // A key is written as the origin it means, so two spellings of one origin cannot both appear.
    const canonical=httpOrigin(key.value,`app.ts:${at(key.start)}: grants key ${JSON.stringify(key.value)}`);
    if (canonical!==key.value) refuse(key,`write the origin key ${JSON.stringify(key.value)} as ${JSON.stringify(canonical)}`);
    if (entries.has(key.value)) refuse(key,`duplicate key ${JSON.stringify(key.value)}`);
    const value=property.value;
    entries.set(key.value,value.type==='ArrayExpression' ? value.elements.map(n=>line(n,value)).join('\n') : line(value,value));
  }
  const wanted=new URL(origin).origin;
  if (entries.has(wanted)) return entries.get(wanted);
  const candidates=development && (privateNetwork(wanted) || loopback(wanted)) ? [...entries.keys()].filter(key=>sameServer(key,wanted)) : [];
  const local=candidates.filter(key=>new URL(key).port===new URL(wanted).port);
  if (!local.length) local.push(...candidates);
  if (local.length>1) refuse(node,`origin ${JSON.stringify(wanted)} is the server of more than one loopback key: ${local.map(k=>JSON.stringify(k)).join(', ')}`);
  if (!local.length) refuse(node,`no entry for origin ${JSON.stringify(wanted)}; keys: ${[...entries.keys()].map(k=>JSON.stringify(k)).join(', ') || '(none)'}`);
  return movedLines(entries.get(local[0]),local[0],wanted);
}

/** A build that is not production (`EXACT_UPDATE_TRUST` unset or `development`, as `bake/src/compat.rs` reads it). */
const developmentTrust = (env) => env.EXACT_UPDATE_TRUST === undefined || env.EXACT_UPDATE_TRUST === 'development';

/** A `net.fetch local-network` or `net.websocket local-network` line, as the native bake's entry matches it too. */
export const LOCAL_NETWORK_LINE = /^([ \t]*net\.(?:fetch|websocket)[ \t]+local-network)[ \t]*$/gm;
/** In a development build each such line gains the word that admits loopback as well (LLP 1133.036 D5), so a
 * drive's fixture on this machine stands in for a device on the LAN. A production build's lines stay as written,
 * and its bake refuses the word. */
export const developmentLocalNetwork = (spec, env=process.env) => developmentTrust(env) ? spec.replace(LOCAL_NETWORK_LINE, '$1 development') : spec;

/** Whether an origin's host is a private-network address: RFC 1918 IPv4, link-local IPv4 (RFC 3927),
 * unique-local IPv6 (fc00::/7, RFC 4193) or link-local IPv6 (fe80::/10, RFC 4291). */
export function privateNetwork(origin) {
  const host=new URL(origin).hostname;
  const v4=host.match(/^(\d+)\.(\d+)\.\d+\.\d+$/);
  if (v4) {
    const [a,b]=[Number(v4[1]),Number(v4[2])];
    return a===10 || (a===172 && b>=16 && b<=31) || (a===192 && b===168) || (a===169 && b===254);
  }
  if (!host.startsWith('[')) return false;
  const first=host.slice(1).split(':')[0];
  if (!first) return false;
  const bits=parseInt(first,16);
  return (bits & 0xfe00)===0xfc00 || (bits & 0xffc0)===0xfe80;
}

const loopback = origin => ['127.0.0.1','localhost','[::1]'].includes(new URL(origin).hostname);
/** A loopback grant key with the selected origin's scheme; a development fixture may move ports. */
function sameServer(key, origin) {
  const [k,o]=[new URL(key),new URL(origin)];
  return loopback(key) && k.protocol===o.protocol;
}

/** A loopback entry's lines for the same server at `to`: each `net.fetch` and `net.websocket` line at the
 * loopback origin moves to `to`, keeping its path; every other line stays as written. */
function movedLines(lines, from, to) {
  const move=(target, a, b)=>target===a || target.startsWith(a+'/') ? b+target.slice(a.length) : target;
  return lines.split('\n').map(line=>{
    // A development build's only line (`developmentGrants`) moves too.
    const m=line.match(/^((?:[ \t]*development[ \t]+)?)(net\.fetch|net\.websocket) (\S+)(.*)$/);
    if (!m) return line;
    const [a,b]=m[2]==='net.fetch' ? [from,to] : [socketOrigin(from),socketOrigin(to)];
    return `${m[1]}${m[2]} ${move(m[3],a,b)}${m[4]}`;
  }).join('\n');
}

/** Select literal keyed grants, or move the plain form's matching fetch and
 * socket grant lines with the origin. Only the captured copy is rewritten. */
export function injectBackendOrigin(code, origin, parse, env=process.env) {
  const parsed=parse('app.ts',code);
  if (parsed.errors.length) {
    if (origin===null) return code;
    throw new Error('EXACT_BACKEND_ORIGIN: app.ts must parse before selecting a backend');
  }
  const statements=parsed.program.body.map(s=>s.type==='ExportNamedDeclaration' ? s.declaration : s);
  const declarations=statements.flatMap(s=>s?.declarations ?? []);
  const grants=declarations.filter(d=>d.id.type==='Identifier' && d.id.name==='grants');
  const keyed=grants.filter(d=>bare(d.init)?.type==='ObjectExpression');
  if (origin===null && !keyed.length) return code;
  const targets=declarations.filter(d=>d.id.type==='Identifier' && d.id.name==='origin');
  const init=bare(targets[0]?.init);
  const literal=init?.type==='Literal' && typeof init.value==='string' ? init.value
    : init?.type==='TemplateLiteral' && !init.expressions.length ? init.quasis[0].value.cooked : null;
  if (targets.length!==1 || !literal || !/^https?:\/\//.test(literal))
    throw new Error(keyed.length ? `app.ts:${lineColumn(code)(init?.start ?? keyed[0].start)}: grants keyed by origin need one top-level literal const origin`
      : "EXACT_BACKEND_ORIGIN: app.ts needs one top-level const origin, set to a literal http(s) string or import.meta.env.NAME ?? '<literal>'; use it in net.fetch ${origin}");
  const edits=origin===null ? [] : [[init.start,init.end,JSON.stringify(origin)]];
  const at=lineColumn(code);
  if (keyed.length) {
    for (const d of [...targets,...keyed]) if (!statements.some(s=>s?.kind==='const' && s.declarations.includes(d)))
      throw new Error(`app.ts:${at(d.start)}: grants keyed by origin require const ${d.id.name}`);
    httpOrigin(origin ?? literal,`app.ts:${at(init.start)}: grants origin`);
    for (const d of keyed) edits.push([d.init.start,d.init.end,JSON.stringify(keyedGrants(bare(d.init),origin ?? literal,at,developmentTrust(env)))]);
  }
  const previous=literal.replace(/\/+$/,'');
  // A development build's only line (`developmentGrants`) keeps its marker and moves too.
  const moved=line=>line===`net.fetch ${previous}` || line===`net.fetch ${previous}/` ? `net.fetch ${origin}`
    : line===`net.websocket ${socketOrigin(previous)}` || line===`net.websocket ${socketOrigin(previous)}/` ? `net.websocket ${socketOrigin(origin)}` : line;
  const replace=value=>value.split('\n').map(line=>{ const [,mark,rest]=line.match(/^([ \t]*development[ \t]+|)(.*)$/s); return mark+moved(rest); }).join('\n');
  const walk=node=>{
    if (!node || typeof node!=='object') return;
    if (node.type==='Literal' && typeof node.value==='string') { const next=replace(node.value); if(next!==node.value) edits.push([node.start,node.end,JSON.stringify(next)]); return; }
    if (node.type==='TemplateLiteral' && !node.expressions.length) { const next=replace(node.quasis[0].value.cooked); if(next!==node.quasis[0].value.cooked) edits.push([node.start,node.end,JSON.stringify(next)]); return; }
    if (node.type==='TemplateElement') { if(replace(node.value.cooked)!==node.value.cooked) throw new Error('EXACT_BACKEND_ORIGIN: use net.fetch ${origin} in an interpolated grants template, and write a net.websocket line at its host as a string of its own'); return; }
    for(const child of Object.values(node)) if(Array.isArray(child)) child.forEach(walk); else if(child && typeof child==='object') walk(child);
  };
  if (origin!==null) grants.filter(d=>!keyed.includes(d)).forEach(d=>walk(d.init));
  for(const [start,end,value] of edits.sort((a,b)=>b[0]-a[0])) code=code.slice(0,start)+value+code.slice(end);
  return code;
}

/** A grant line that begins with the word `development` is a development
 * build's only: a production bake leaves it out, any other build keeps it
 * without the word (Harmony's test call, `development device.calls
 * purpose.calls`, LLP 1133.033 §13 question 4). No grant family is a bare
 * word, so the marker is never a grant's own; the bake's trailing
 * `local-network development` is another thing. Only the captured copy is
 * rewritten, so the app's own `grants` is the build's at run time too. Run
 * after `injectBackendOrigin`, which has moved a marked line with the
 * backend's origin and made keyed grants the selected entry's string. */
export function developmentGrants(code, parse, env=process.env) {
  const parsed=parse('app.ts',code);
  if (parsed.errors.length) return code;
  const marker=/^([ \t]*)development[ \t]+(?=\S)/;
  const grants=staticFacts(code,parse).grants;
  if (grants===undefined || !grants.split('\n').some(line=>marker.test(line))) return code;
  // The binding exported as `grants`, as `staticFacts` reads it.
  const body=parsed.program.body;
  const local=body.flatMap(s=>s.type==='ExportNamedDeclaration' && !s.source ? (s.specifiers ?? []) : [])
    .find(x=>(x.exported.name ?? x.exported.value)==='grants')?.local;
  const name=local ? local.name ?? local.value : 'grants';
  const declaration=body.map(s=>s.type==='ExportNamedDeclaration' ? s.declaration : s)
    .flatMap(s=>s?.type==='VariableDeclaration' ? s.declarations : []).find(d=>d.id.type==='Identifier' && d.id.name===name);
  const development=developmentTrust(env);
  const lines=grants.split('\n').flatMap(line=>!marker.test(line) ? [line] : development ? [line.replace(marker,'$1')] : []);
  return code.slice(0,declaration.init.start)+JSON.stringify(lines.join('\n'))+code.slice(declaration.init.end);
}

// Rolldown IDs are file paths, not URL or slash-normalized strings. Both
// producers enforce the same captured-file boundary (LLP 1027 D5). A
// package's declarations are captured for the checker alone (`stagePackages`)
// and never load; a package's code loads only from where `capturedSources`
// captured it, which admits it before this check.
export function assertCapturedModule(stage, id) {
  if (!isAbsolute(id)) throw new Error('module outside captured app: '+id);
  const root=realpathSync.native(stage), file=realpathSync.native(id), path=relative(root,file);
  if (!path || isAbsolute(path) || path==='..' || path.startsWith('..'+sep) || resolve(root,path)!==file || !statSync(file).isFile()) {
    throw new Error('module outside captured app: '+id);
  }
  if (path.split(sep).includes('node_modules')) throw new Error('module outside captured app: '+packageOrigin(root,file));
}

// Package declarations (LLP 1027 D5, "Type-only package imports"). A bare
// specifier names a package. Its declaration files (`.d.ts`, which emit
// nothing) and `package.json` files are captured into the stage's
// `node_modules`, at the place TypeScript looks for them, so the checker reads
// a package's types from the capture as it reads app.ts, and the resolved
// graph still stays inside the stage. Its JavaScript is captured apart, file
// by file as the bundle reaches it (`capturedSources`).
const DECLARATION=/\.d\.[cm]?ts$/;
// A string after `from`, `import`, `import(`, `require(`, `declare module` or
// `<reference types=`, comments between included. Matching more than an
// import only stages unread declarations; peer requirements use syntax below.
const SPECIFIER=/(?:\bfrom|\bimport|\brequire|\bmodule|\btypes\s*=)(?:\s|\(|\/\*[\s\S]*?\*\/|\/\/[^\n]*)*(['"])([^'"\r\n]+)\1/g;
// A peer requirement must come from an import, including import types and
// leading triple-slash references, rather than an example in prose or a string.
function declarationImports(text, parse) {
  parse ??= createRequire(import.meta.url)('rolldown/utils').parseSync;
  const {program,comments}=parse('package.d.ts',text), imports=new Set();
  const visit=node=>{
    if (Array.isArray(node)) { for (const child of node) visit(child); return; }
    if (!node || typeof node!=='object') return;
    if (/^(Import|ExportNamed|ExportAll)Declaration$|^(ImportExpression|TSImportType)$/.test(node.type) && typeof node.source?.value==='string') imports.add(node.source.value);
    if (node.type==='TSExternalModuleReference' && typeof node.expression?.value==='string') imports.add(node.expression.value);
    for (const key in node) visit(node[key]);
  };
  visit(program);
  for (const comment of comments) {
    if (comment.start>=(program.body[0]?.start ?? text.length)) break;
    const types=/^\/\/\/\s*<reference\s(?:[^>]*?\s)?types\s*=\s*(['"])([^'"]*)\1/.exec(text.slice(comment.start,comment.end));
    if (types) imports.add(types[2]);
  }
  return imports;
}
const PACKAGES='__exact_declarations.json';
// A package's `/// <reference lib="esnext.disposable" />` to an ES library the
// configuration leaves out is staged as a plain comment, so it loads nothing:
// what that library declares (`Symbol.dispose`) no executor is known to have,
// so the app's code still cannot name it, and the package's declarations that
// do resolve it to nothing (`skipLibCheck`). Any other library a package
// names, the DOM's, is still loaded and refused (`check`). TypeScript reads a
// directive from a `///` comment in the comments before the file's first token,
// its attributes in any order.
const unreferenced=(bytes)=>{
  const text=bytes.toString('utf8');
  let out=text, at=text.charCodeAt(0)===0xFEFF ? 1 : 0;
  for (;;) {
    at+=/^\s*/.exec(text.slice(at))[0].length;
    if (text.startsWith('/*',at)) { const end=text.indexOf('*/',at+2); if (end<0) break; at=end+2; continue; }
    if (!text.startsWith('//',at)) break;
    // A `//` comment ends at ECMAScript's first line terminator: LF, CR, U+2028 or U+2029.
    const ends=text.slice(at).search(/[\n\r\u2028\u2029]/), end=ends<0 ? -1 : at+ends, line=text.slice(at,end<0 ? undefined : end);
    const lib=/^\/\/\/\s*<reference\s(?:[^>]*?\s)?lib\s*=\s*(["'])([^"']*)\1/i.exec(line)?.[2];
    if (lib && /^es(?:next|\d{4})(?:\.|$)/i.test(lib) && !configuredLibrary(`lib.${lib.toLowerCase()}.d.ts`)) out=out.slice(0,at)+'// '+out.slice(at+3);
    if (end<0) break;
    at=end+1;
  }
  return out===text ? bytes : Buffer.from(out);
};
// The package a specifier names, or null: relative, absolute, `#` subpath and
// scheme (`node:`, `exact:`) specifiers name none.
export function packageName(spec) {
  if (/^[./\\#]|^[A-Za-z][\w+.-]*:/.test(spec)) return null;
  const parts=spec.split('/'), name=spec.startsWith('@') ? parts.slice(0,2).join('/') : parts[0];
  return /^(?:@[\w.-]+\/)?[\w.-]+$/.test(name) && !/^\.+$/.test(name.split('/').at(-1)) ? name : null;
}
// Where a staged package file came from (`__exact_declarations.json`).
function packageOrigin(stage, file) {
  try {
    const { packages }=JSON.parse(readFileSync(resolve(stage,PACKAGES),'utf8'));
    const match=packages.map(([at,from])=>[resolve(stage,at),from]).sort((a,b)=>b[0].length-a[0].length)
      .find(([at])=>file===at || file.startsWith(at+sep));
    if (match) return resolve(match[1],relative(match[0],file));
  } catch { /* no packages staged */ }
  return file;
}
// The captured app's places: the app at the stage's root and each mount
// under its name, each with the directory it was captured from.
function capturedOrigins(stage) {
  const mapping=resolve(stage,'__exact_paths.json');
  const roots=existsSync(mapping) ? JSON.parse(readFileSync(mapping,'utf8')) : {app:stage,mounts:[]};
  return [...roots.mounts.map(([name,path])=>[resolve(stage,name),path]),[stage,roots.app]];
}
const within=(path,root)=>path===root || path.startsWith(root+sep);
// A path as the disk names it (a symlinked stage's real place); one that isn't there, an importer from outside the stage
// a bundler names, stays as written, which `within` then leaves alone.
const physicalId=id=>{ if (!isAbsolute(id)) return id; try { return realpathSync.native(id); } catch { return id; } };
/** Capture into the stage the declarations of every package the captured
 * TypeScript names, and of the packages those declarations name: each found
 * from where its file was captured, as Node and TypeScript find it (the
 * nearest `node_modules` upward; a package's `@types` companion too), and
 * placed where the checker, walking up from the staged file, finds it.
 * A linked package's declared peers come from the app, beside their consumer. A
 * `node_modules` inside a captured root is mirrored at its place; one above
 * it is placed at the root (the app's at the stage's top, a mount's under its
 * name); a package's dependency sits beside it unless nested inside it.
 * Writes `__exact_declarations.json`: each staged package, where it is
 * installed and the directory that is, and every file read where it is
 * installed, so a producer can watch them. */
export function stagePackages(stage, parse) {
  const roots=capturedOrigins(stage), app=roots.at(-1);
  const staged=new Map(), files=new Map(), queue=[], peers=[], peerRefusals=[];
  let total=0;
  // Over the cap, the error names the largest packages, each with the captured file whose imports reached it.
  const tooLarge=()=>{
    const mib=bytes=>`${(bytes/(1<<20)).toFixed(1)} MiB`;
    const largest=[...staged.values()].sort((a,b)=>b.bytes-a.bytes).slice(0,3)
      .map(p=>`${p.name} ${mib(p.bytes)} (${p.via ? `through ${p.via}, ` : ''}from ${p.source})`);
    return new Error(`package declarations exceed 64 MiB: ${largest.join(', ')}. The type check stages the declarations of every package a .ts or .js file in the app folder imports, whether or not app.ts reaches the file; a folder that is not the app's (a web page's source) belongs outside the app folder, or in a folder at its root whose name starts with a dot`);
  };
  // Where `name`, needed by a file in `dir` on disk, is found and placed.
  const find=(name,dir,root,owner)=>{
    for (let at=dir;;) {
      const candidate=join(at,'node_modules',name);
      if (!within(candidate,stage) && !within(stage,candidate) && existsSync(join(candidate,'package.json'))) {
        const mirror=(from,to)=>within(at,from) ? resolve(to,relative(from,at),'node_modules') : null;
        const modules=owner ? mirror(owner.from,owner.at) ?? owner.modules : mirror(root[1],root[0]) ?? resolve(root[0],'node_modules');
        return {real:realpathSync(candidate),installed:candidate,place:join(modules,name),modules};
      }
      const up=dirname(at);
      if (up===at) return null;
      at=up;
    }
  };
  // `source`: the captured file whose imports led here, as the app names it.
  const need=(text,dir,root,owner,source,place)=>{
    const declared=owner?.pkg?.json.peerDependencies ?? {};
    const imports=Object.keys(declared).length ? declarationImports(text,parse) : null;
    for (const [, , spec] of text.matchAll(SPECIFIER)) {
      const name=packageName(spec);
      if (!name) continue;
      if (Object.hasOwn(declared,name) && !imports.has(spec)) continue;
      let peer;
      try { peer=appPeer(app[1],owner?.pkg,name); }
      catch (error) {
        // Copying every declaration does not make it reachable. The checker
        // enforces this requirement only if it reads the importing file.
        peerRefusals.push([relative(stage,place),error.message]);
        continue;
      }
      for (const wanted of [name,'@types/'+name.replace(/^@/,'').replace('/','__')]) {
        const found=peer ? find(wanted,app[1],app,null) : find(wanted,dir,root,owner);
        if (!found) continue;
        if (peer) {
          found.modules=owner.modules;
          found.place=join(found.modules,wanted);
          peers.push([dirname(place),wanted,found.real]);
        }
        const known=staged.get(found.place)?.real;
        if (known && known!==found.real) throw new Error(`package declarations: two copies of ${wanted} (${known}, ${found.real}); install one`);
        if (known) continue;
        staged.set(found.place,Object.assign(found,{name:wanted,via:owner?.name,source,bytes:0}));
        copy(found.real,found.place,{at:found.place,from:found.real,installed:found.installed,modules:found.modules,
          name:wanted,source,staged:found,pkg:packageOf(join(found.real,'package.json'),found.installed)});
      }
    }
  };
  const copy=(dir,place,owner)=>{
    for (const entry of readdirSync(dir,{withFileTypes:true})) {
      const name=entry.name, path=join(dir,name);
      if (name==='node_modules' || name.startsWith('.')) continue;
      const kind=entry.isSymbolicLink() ? statSync(path,{throwIfNoEntry:false}) : entry;
      if (kind?.isDirectory()) copy(path,join(place,name),owner);
      else if (kind?.isFile() && (name==='package.json' || DECLARATION.test(name))) {
        const bytes=DECLARATION.test(name) ? unreferenced(readFileSync(path)) : readFileSync(path);
        total+=bytes.length;
        owner.staged.bytes+=bytes.length;
        if (total>64<<20) throw tooLarge();
        // Recorded where it is installed: a link retargeted is a change there.
        files.set(join(place,name),[join(owner.installed,relative(owner.from,path)),bytes]);
        if (DECLARATION.test(name)) queue.push([bytes.toString('utf8'),dirname(realpathSync(path)),null,owner,owner.source,join(place,name)]);
      }
    }
  };
  // The captured sources, from where each was captured, and every staged
  // `node_modules` (the app's capture holds none).
  const mirrors=[];
  const sources=(dir)=>{
    for (const entry of readdirSync(dir,{withFileTypes:true})) {
      const path=join(dir,entry.name);
      if (entry.isDirectory() && entry.name==='node_modules') mirrors.push(path);
      else if (entry.isDirectory()) sources(path);
      else if (entry.isFile() && /\.[cm]?[jt]sx?$/.test(entry.name) && !entry.name.startsWith('__exact_') && path!==resolve(stage,'app.js')) {
        const root=roots.find(([at])=>within(path,at));
        need(readFileSync(path,'utf8'),dirname(resolve(root[1],relative(root[0],path))),root,null,slash(relative(stage,path)));
      }
    }
  };
  sources(stage);
  while (queue.length) need(...queue.shift());
  // A later ordinary dependency may have occupied a nearer slot. The checker
  // must not silently read that copy for an app-owned peer.
  for (const [from,name,real] of peers) {
    for (let at=from; within(at,stage); at=dirname(at)) {
      const found=staged.get(join(at,'node_modules',name));
      if (!found) continue;
      if (found.real!==real) throw new Error(`package declarations: two copies of ${name} (${real}, ${found.real}); install one`);
      break;
    }
  }
  // The staged trees are exactly this capture: a package no longer named,
  // or no longer installed, goes.
  const prune=(dir)=>{
    for (const entry of readdirSync(dir,{withFileTypes:true})) {
      const path=join(dir,entry.name);
      if (entry.isDirectory()) { prune(path); try { rmdirSync(path); } catch { /* not empty */ } }
      else if (!files.has(path)) unlinkSync(path);
    }
  };
  for (const dir of mirrors) { prune(dir); try { rmdirSync(dir); } catch { /* still staged */ } }
  for (const [path,[,bytes]] of files) {
    if (existsSync(path) && readFileSync(path).equals(bytes)) continue;
    mkdirSync(dirname(path),{recursive:true});
    writeFileSync(path,bytes);
  }
  const record=JSON.stringify({
    packages:[...staged].map(([at,{installed,real}])=>[relative(stage,at),installed,real]).sort(),
    files:[...files.values()].map(([path])=>path).sort(),
    peerRefusals:peerRefusals.sort(),
  });
  const recordPath=resolve(stage,PACKAGES);
  if (!existsSync(recordPath) || readFileSync(recordPath,'utf8')!==record) writeFileSync(recordPath,record);
}

// Package code (LLP 1027 D5, "Package code"). A package's JavaScript is
// captured file by file as the bundle reaches it: each import is resolved
// from where its importer really is, or from the app for a linked package's peer
// (Node's lookup, a package's `exports`, `main` and `browser` fields, as a
// browser bundle resolves them), and the
// file it leads to is copied into the stage under `node_modules/.packages/
// <name>@<version>/`, with every `package.json` between it and the package's
// root (their `type` and `sideEffects`). The bundle loads only those copies.
// `__exact_packages.json` records each file by where it was read and its
// SHA-256, each import's resolution, and each package's installed paths
// (the links an install retargets), so a bake is a function of what it
// captured and every later bundle replays it (`replayCaptured`). A Node
// built-in, a native addon, an import that is not installed, and an
// unguarded use of the DOM or of Node's globals are refused by package and
// file: no executor has them.
const RECORD='__exact_packages.json';
/** The bundle's sites that run code from a string, a JSON list of warnings each producer says (the web build,
 * js/bake/src/lib.rs `bake_in`). */
export const CODE_WARNINGS='__exact_code_warnings.json';
const BUILTIN=new Set(builtinModules);
const builtin=(spec)=>spec.startsWith('node:') || BUILTIN.has(spec) || BUILTIN.has(spec.split('/')[0]);
// npm's package names (with the old capitals) and versions: a place in the
// stage is made of them, so nothing else is taken.
const NAME=/^(?:@[a-z0-9~][\w.~-]*\/)?[a-z0-9~][\w.~-]*$/i, VERSION=/^(?:[0-9a-z][\w.+-]*)?$/i;
// The package a file belongs to: the nearest `package.json` that names one
// (a nested `{"type":"module"}` names none).
function packageOf(file, installed) {
  for (let dir=dirname(file);;) {
    let json=null;
    try { json=JSON.parse(readFileSync(join(dir,'package.json'),'utf8')); } catch { /* none here, or not JSON */ }
    if (typeof json?.name==='string' && json.name) {
      const version=typeof json.version==='string' ? json.version : '';
      if (!NAME.test(json.name) || !VERSION.test(version)) throw new Error(`${join(dir,'package.json')}: not an npm package name and version (${JSON.stringify(json.name)}, ${JSON.stringify(version)})`);
      return {root:dir,key:json.name+'@'+version,name:json.name,json,installed:new Set(installed ? [installed] : [])};
    }
    const up=dirname(dir);
    if (up===dir) return null;
    dir=up;
  }
}
// Where Node finds package `name` from `dir`: the nearest `node_modules/<name>`
// upward, as installed (a link stays a link).
function installedAt(name, dir) {
  for (let at=dir;;) {
    const candidate=join(at,'node_modules',name);
    if (statSync(candidate,{throwIfNoEntry:false})) return candidate;
    const up=dirname(at);
    if (up===at) return null;
    at=up;
  }
}
// Linked consumers (a workspace or `link:` package: a link to a directory that
// is not an entry of a `node_modules`) use the app's peers despite their
// development install. Installed packages keep Node's ancestor lookup, including
// those an isolated install (pnpm, Bun) links from its store, which places each
// peer beside its consumer.
const storeEntry=(root)=>{
  const parent=dirname(root);
  return basename(parent)==='node_modules' || basename(parent).startsWith('@') && basename(dirname(parent))==='node_modules';
};
function appPeer(app, pkg, name) {
  if (!Object.hasOwn(pkg?.json.peerDependencies ?? {},name)) return null;
  if (storeEntry(pkg.root) || ![...pkg.installed].some(path=>resolve(path)!==pkg.root)) return null;
  const range=pkg.json.peerDependencies[name], installed=installedAt(name,app);
  const who=`${pkg.key} requires peer ${name}@${range}`;
  if (!installed || !existsSync(join(installed,'package.json'))) throw new Error(`${who}, which is not installed in the app; install a compatible version`);
  const json=JSON.parse(readFileSync(join(installed,'package.json'),'utf8'));
  if (typeof range!=='string' || typeof json.version!=='string' || !Bun.semver.satisfies(json.version,range))
    throw new Error(`${who}, but the app has ${name}@${json.version ?? '(no version)'}; install a compatible version`);
  return {installed};
}
const slash=(path)=>path.split(sep).join('/');
const kindOf=(options)=>options?.kind==='require-call' ? 'require' : 'import';
// Every bundle of the module resolves packages as a browser bundle does (the
// `browser` condition and field first), as the page's own bundle does, so
// the page and Hermes run the same file of a package; `process.env.NODE_ENV`
// is the constant packages test.
export const bundleOptions={platform:'browser',transform:{define:{'process.env.NODE_ENV':'"production"'}}};
// An async function resumes under the answer it began in (LLP 1133.002 D14),
// so a call it makes after an `await` is that answer's, even when it awaited
// a promise another answer made. No engine carries context across a native
// `await`, so a data module's bundles lower `async` to generators
// (`lowered`) and drive each step through `asyncContextHelper`, which puts
// back the answer current when the function was called and restores the one
// before after the step. The runtime that runs the module owns
// `__exact_async_context` (js/src/prelude.js, host/web-js/ts-data.js); a
// realm without one runs the steps as the stock helper would. The web hook also supplies the source's Promise,
// so reactions on an async helper's result retain their entropy owner.
export const lowered={target:'es2016'};
const ASYNC_HELPER=/@oxc-project\+runtime@[^/]*\/helpers\/esm\/asyncToGenerator\.js$/;
export const asyncContextHelper=`export default function _asyncToGenerator(fn) {
  return function () {
    var self = this, args = arguments, context = globalThis.__exact_async_context, at = context ? context.current() : undefined;
    var P = context && context.Promise || Promise;
    return new P(function (resolve, reject) {
      var gen = fn.apply(self, args);
      function step(key, arg) {
        // A live answer's step that a module turn settled runs in that answer's own turn (LLP 1146 D2)
        if (context && context.route && context.route(at, step, key, arg)) return;
        var before = context ? context.swap(at) : undefined, info;
        try { info = gen[key](arg); } catch (error) { if (context) context.swap(before); reject(error); return; }
        if (context) context.swap(before);
        if (info.done) resolve(info.value);
        else Promise.resolve(info.value).then(function (v) { step("next", v); }, function (e) { step("throw", e); });
      }
      step("next", undefined);
    });
  };
}
`;
/** Rolldown resolves the helpers its lowering names itself, never through a
 * plugin's `resolveId`, but passes them through `transform`: this replaces
 * `asyncToGenerator` there. */
export function asyncContext() {
  return { name:'exact-async-context', transform(code,id) { return ASYNC_HELPER.test(id) ? { code:asyncContextHelper, map:null } : null; } };
}
/** The native bundle of a data module, the same for the one-shot producer
 * and the resident compiler. */
export const dataBundle=(stage)=>({...bundleOptions, transform:{...bundleOptions.transform, ...lowered},
  plugins:[asyncContext(), capturedSources(stage)]});
/** The captured graph's Rolldown plugin, the same in every producer: app
 * files load only from the stage, package code only from its capture, and
 * the clock (`ambientRefusals`) or what no executor has (`packageRefusals`)
 * is refused by file and line, into `refusals` when given, else thrown. */
export function capturedSources(stage, refusals=null) {
  stage=realpathSync.native(stage);
  const files=new Map(), roots=new Map(), edges=new Map(), stray=new Map(), area=join(stage,'node_modules','.packages');
  // Each site that runs code from a string, once (`packageRefusals`): written beside the stage at the build's end.
  const warned=new Set();
  let total=0;
  // Staged declarations and captured package code sit under a `node_modules` in the stage.
  const staged=(id)=>within(id,stage) && relative(stage,id).split(sep).includes('node_modules');
  const label=(id,at='')=>{
    const file=files.get(id);
    if (!file) return slash(relative(stage,id))+at;
    return `${file.package.key} (${slash(relative(file.package.root,file.real))}${at})`;
  };
  // A diagnostic's file: the app's path, or the package's name, version and path in it (`zod@4.6.7/v4/core/util.js`).
  const place=(id,at='')=>{
    const file=files.get(id);
    return (file ? `${file.package.key}/${slash(relative(file.package.root,file.real))}` : slash(relative(stage,id)))+at;
  };
  const copy=(from,to,pkg)=>{
    if (files.has(to)) return;
    if (!within(to,area) || to===area) throw new Error('module outside captured app: '+from);
    const bytes=readFileSync(from);
    total+=bytes.length;
    if (total>64<<20) throw new Error('package code exceeds 64 MiB');
    if (!existsSync(to) || !readFileSync(to).equals(bytes)) { mkdirSync(dirname(to),{recursive:true}); writeFileSync(to,bytes); }
    files.set(to,{real:from,package:pkg,sha256:createHash('sha256').update(bytes).digest('hex')});
  };
  // The files a package's exact.json declares in `web.assets` (LLP 1154 D2), with that declaration: captured with its
  // code, so its digest covers their bytes and the web build serves them from the capture (host/web-js/build.mjs).
  const assets=(pkg)=>{
    const at=join(pkg.root,'exact.json');
    if (!existsSync(at)) return;
    let declared;
    try { declared=JSON.parse(readFileSync(at,'utf8')).web?.assets; } catch (e) { throw new Error(`${pkg.key}: exact.json is not JSON: ${e.message}`); }
    if (declared===undefined) return;
    if (!Array.isArray(declared) || declared.some(p=>typeof p!=='string')) throw new Error(`${pkg.key}: exact.json web.assets is a list of paths in the package`);
    const base=join(area,...pkg.key.split('/'));
    copy(join(pkg.root,'exact.json'),join(base,'exact.json'),pkg);
    for (const path of declared) {
      const from=resolve(pkg.root,path), real=existsSync(from) ? realpathSync.native(from) : null;
      if (isAbsolute(path) || real!==from || real===pkg.root || !within(real,pkg.root) || !statSync(real).isFile()) throw new Error(`${pkg.key}: exact.json web.assets names ${JSON.stringify(path)}, which is not a file in the package (a link is not one)`);
      copy(real,join(base,relative(pkg.root,real)),pkg);
    }
  };
  const capture=(real,who,installed)=>{
    const found=packageOf(real,installed);
    if (!found) throw new Error('module outside captured app: '+real);
    const pkg=roots.get(found.key) ?? found;
    if (pkg.root!==found.root) throw new Error(`two copies of ${found.key} (${pkg.root}, ${found.root}); install one`);
    if (!roots.has(pkg.key)) assets(pkg);
    roots.set(pkg.key,pkg);
    if (installed) pkg.installed.add(installed);
    if (/\.node$/.test(real) || pkg.json.gypfile || existsSync(join(pkg.root,'binding.gyp')))
      throw new Error(`${who} imports ${pkg.key}, a native addon: a data module runs JavaScript alone`);
    const base=join(area,...pkg.key.split('/')), place=join(base,relative(pkg.root,real));
    copy(real,place,pkg);
    for (let dir=dirname(real); within(dir,pkg.root); dir=dirname(dir)) {
      if (existsSync(join(dir,'package.json'))) copy(join(dir,'package.json'),join(base,relative(pkg.root,dir),'package.json'),pkg);
      if (dir===pkg.root) break;
    }
    return place;
  };
  const report=(why)=>{ if (!why.length) return; if (refusals) refusals.push(...why); else throw new Error(why.join('\n')); };
  return {
    name:'captured-sources',
    async resolveId(source, importer, options) {
      if (!importer || importer.startsWith('\0')) return null;
      importer=physicalId(importer);
      const from=files.get(importer), name=packageName(source), kind=kindOf(options);
      const record=(id)=>edges.set([slash(relative(stage,importer)),source,kind].join('\0'),[slash(relative(stage,importer)),source,kind,id ? slash(relative(stage,id)) : '']);
      // The app's own imports (a relative or absolute path) resolve as they
      // would, and are recorded, so a later bundle replays every one.
      if (!from && !name && !builtin(source)) {
        if (!within(importer,stage)) return null;
        const result=await this.resolve(source, importer, { ...options, skipSelf: true });
        const resolved=result && !result.external ? {...result,id:physicalId(result.id)} : result;
        if (resolved && !resolved.external && !resolved.id.startsWith('\0')) record(resolved.id);
        return resolved;
      }
      if (!from && name) {
        // `rows/../../helper.ts` names no file of a package: it leaves it, to a
        // place that differs between the stage and the app's own directory.
        if (source.split(/[\\/]/).some(part => part === '.' || part === '..')) throw new Error('module outside captured app: '+source);
        // A path mapping into the capture.
        const result=await this.resolve(source, importer, { ...options, skipSelf: true });
        const resolved=result && !result.external ? {...result,id:physicalId(result.id)} : result;
        if (resolved && !resolved.external && within(resolved.id,stage) && !staged(resolved.id)) { record(resolved.id); return resolved; }
      }
      // Linked consumers' peers belong to the app; other imports keep their own place.
      const origins=capturedOrigins(stage), [at,origin]=origins.find(([at])=>within(importer,at)) ?? [stage,stage];
      const app=origins.at(-1)[1], peer=name && appPeer(app,from?.package,name);
      const place=peer ? join(app,'app.ts') : from ? from.real : resolve(origin,relative(at,importer));
      const real=await this.resolve(source, place, { ...options, skipSelf: true }).catch(() => null);
      const who=label(importer);
      if (!real || real.external) {
        if (builtin(source)) throw new Error(`${who} imports Node's built-in '${source}': a data module runs on Hermes and in the browser, which have no Node modules; import a part of the package that needs none, or another package`);
        throw new Error(`${who} imports '${source}', which is not installed`);
      }
      // A `browser` field's `false`: Rolldown's empty module.
      if (real.id.startsWith('\0')) { record(null); return real; }
      const id=capture(physicalId(real.id), who, peer?.installed ?? (name && installedAt(name, dirname(place))));
      record(id);
      return { ...real, id };
    },
    load(id) {
      id=physicalId(id);
      if (!files.has(id) && !id.startsWith('\0rolldown/empty.js')) assertCapturedModule(stage, id);
      return null;
    },
    transform(code, id) {
      id=physicalId(id);
      const parse=(c,o)=>this.parse(c,o), js=/\.[cm]?js$/.test(id) && within(id,stage), inPackage=files.has(id);
      const reads=envReads(id,code,parse,(at)=>label(id,at),inPackage);
      // The app's own JavaScript is checked as package code is, and for the clock as the app's TypeScript is. Every other
      // module that runs, the app's TypeScript and a package's TypeScript or JSX, is checked only for code from a string.
      const fromStrings=[], other=!js && within(id,stage) && /\.[cm]?[jt]sx?$/.test(id) && !/\.d\.[cm]?ts$/.test(id);
      if (other) packageRefusals(code,(at)=>label(id,at),parse,fromStrings,/\.[cm]?tsx$/.test(id) ? 'tsx' : /\.jsx$/.test(id) ? 'jsx' : 'ts',(at)=>place(id,at));
      report([...(inPackage ? [] : ambientRefusals(stage,id,code,parse)), ...(inPackage ? [] : reads.map(r=>r.why)), ...(js ? packageRefusals(code,(at)=>label(id,at),parse,fromStrings,'js',(at)=>place(id,at)) : [])]);
      for (const why of fromStrings) warned.add(why);
      if (!inPackage || !reads.length) return null;
      // A package's read of a setting is a read only if the bundle keeps it: each becomes a global of the same length,
      // which tree-shaking drops with the code around it, and `renderChunk` refuses any the output still holds. A
      // global's read may throw, so the bundle keeps at least the code the read would have kept.
      for (const {start,end,why} of reads) {
        const marker=`__exact_env${stray.size.toString(36)}`;
        stray.set(marker,why);
        code=code.slice(0,start)+marker.padEnd(end-start)+code.slice(end);
      }
      return {code,map:null};
    },
    renderChunk(code) {
      report([...stray].filter(([marker])=>new RegExp(`\\b${marker}\\b`).test(code)).map(([,why])=>why));
      return null;
    },
    buildEnd() {
      const listed=[...files.values()].map(({real,package:{key,root},sha256})=>[key,slash(relative(root,real)),real,sha256])
        .sort((a,b)=>a[0]+'\0'+a[1]<b[0]+'\0'+b[1] ? -1 : 1);
      const resolved=[...edges.values()].sort((a,b)=>a.join('\0')<b.join('\0') ? -1 : 1);
      // A package's digest: its files' paths and bytes, and every import that reached it.
      const digests={};
      for (const key of [...roots.keys()].sort()) {
        const prefix='node_modules/.packages/'+key+'/';
        const lines=[...listed.filter(f=>f[0]===key).map(f=>f[1]+'\0'+f[3]), ...resolved.filter(e=>e[3].startsWith(prefix)).map(e=>e.join('\0'))];
        digests[key]=createHash('sha256').update(lines.join('\n')).digest('hex');
      }
      const record=JSON.stringify({
        packages:[...roots].map(([key,{root,installed}])=>[key,root,[...installed].sort()]).sort((a,b)=>a[0]<b[0] ? -1 : 1),
        files:listed, edges:resolved, digests,
      });
      const path=resolve(stage,RECORD);
      if (!existsSync(path) || readFileSync(path,'utf8')!==record) writeFileSync(path,record);
      writeFileSync(resolve(stage,CODE_WARNINGS),JSON.stringify([...warned].sort()));
    },
  };
}
/** The web assets a capture holds (`capturedSources`): each package's name, the asset's path in it, and the file. */
export function capturedAssets(stage) {
  const out=[];
  for (const [key] of JSON.parse(readFileSync(resolve(stage,RECORD),'utf8')).packages) {
    const base=join(stage,'node_modules','.packages',...key.split('/'));
    if (!existsSync(join(base,'exact.json'))) continue;
    let declared;
    try { declared=JSON.parse(readFileSync(join(base,'exact.json'),'utf8')).web?.assets; } catch (e) { throw new Error(`${key}: the captured exact.json is not JSON: ${e.message}`); }
    for (const path of declared ?? []) out.push({ name:key.slice(0,key.lastIndexOf('@')), path:slash(relative(base,resolve(base,path))), file:resolve(base,path) });
  }
  return out;
}
/** A later bundle of the checked capture (the web page's and its server
 * render's): every import from a captured file resolves to what
 * `capturedSources` resolved it to, and any other is refused, so nothing is
 * read again from the app's directory or where a package is installed. The
 * one exception is `allowed`: the host runtime's own module the web build
 * injects into the app's code. `rolldown` is a Rolldown plugin; `bun` a
 * Bun.build plugin's setup. Imports from outside the stage are left alone. */
export function replayCaptured(stage, allowed=[]) {
  stage=realpathSync.native(stage);
  const record=JSON.parse(readFileSync(resolve(stage,RECORD),'utf8'));
  const edges=new Map(record.edges.map(([importer,source,kind,target])=>[[resolve(stage,importer),source,kind].join('\0'),target && resolve(stage,target)]));
  const host=new Set(allowed);
  const lookup=(source,importer,kind)=>{
    if (importer) importer=physicalId(importer);
    if (!importer || !within(importer,stage)) return undefined;
    const key=[importer,source,kind].join('\0');
    if (edges.has(key)) return edges.get(key);
    if (host.has(source)) return undefined;
    throw new Error(`${slash(relative(stage,importer))} imports '${source}', which the checked capture did not resolve; build again`);
  };
  const EMPTY='module.exports = {};';
  return {
    rolldown: { name:'captured-replay',
      resolveId(source,importer,options) { const id=lookup(source,importer,kindOf(options)); return id===undefined ? null : id || '\0exact-empty'; },
      load(id) { return id==='\0exact-empty' ? EMPTY : null; } },
    bun(build) {
      build.onResolve({ filter: /.*/ }, (args) => {
        const id=lookup(args.path,args.importer,args.kind==='require-call' ? 'require' : 'import');
        return id===undefined ? undefined : id ? { path: id } : { path: args.path, namespace: 'exact-empty' };
      });
      build.onLoad({ filter: /.*/, namespace: 'exact-empty' }, () => ({ contents: EMPTY, loader: 'js' }));
    },
  };
}

// Time and seeds are source arguments (LLP 1027.000): every executor refuses the clock, randomness and every
// timer but an answer's own `setTimeout` (D1) when they are used, with these words (js/src/time.js,
// js/src/crypto.js, host/web-js/ts-fetch.js). A direct use in the app's own code is refused here too, at
// build, by file and line, so a test that runs the module under Bun, which has no such guard, cannot hide it.
const ambient=(api)=>api+' is unavailable in data sources; pass time or a random seed as an argument';
const timers=(api)=>api+' is unavailable in data sources: an answer waits with setTimeout; pass time as an argument';
const TIMERS=['setInterval','requestAnimationFrame','requestIdleCallback'];
const GLOBALS=['globalThis','self','window','global'];
// The global `name`: the bare identifier, or a global object's property.
const ambientGlobal=(node,name,local)=>{
  node=bare(node);
  if (node?.type==='Identifier') return node.name===name && !local.has(name);
  return propertyName(node)===name
    && bare(node.object)?.type==='Identifier' && GLOBALS.includes(bare(node.object).name) && !local.has(bare(node.object).name);
};
const member=(node,object,property,local)=>{ node=bare(node); return propertyName(node)===property && ambientGlobal(node.object,object,local); };
function refusal(node,local) {
  if (node.type==='CallExpression') {
    const callee=node.callee;
    if (member(callee,'Date','now',local)) return ambient('Date.now()');
    if (member(callee,'performance','now',local)) return ambient('performance.now()');
    if (member(callee,'Math','random',local)) return 'Math.random() is unavailable in data sources; seed from crypto.getRandomValues in a source (docs/recipes/random-and-shuffle.md), or pass a seed as an argument; a roll in the view needs no source (docs/recipes/random-rolls.md)';
    if (ambientGlobal(callee,'Date',local)) return ambient('Date()');
    const timer=TIMERS.find(name=>ambientGlobal(callee,name,local));
    if (timer) return timers(timer+'()');
  }
  if (node.type==='NewExpression' && ambientGlobal(node.callee,'Date',local) && !node.arguments.length) return ambient('new Date()');
  return null;
}
/** The direct uses of the global clock, randomness and timers where no enclosing
 * scope binds them at run time, as `path:line:col: why` with `file`'s path in the app: `parse`
 * is Rolldown's (a plugin's `this.parse`, or `parseAst`), given TypeScript.
 * An early warning a Bun test cannot give, not the guard: an alias still
 * reaches the runtime's refusal. */
export function ambientRefusals(stage, file, code, parse) {
  if (!isAbsolute(file) || /\.d\.[cm]?ts$/.test(file) || !/\.[cm]?[jt]sx?$/.test(file)) return [];
  const lang=/\.[cm]?tsx?$/.test(file) ? (file.endsWith('x') ? 'tsx' : 'ts') : 'js';
  const label=relative(stage,file).split(sep).join('/'), found=[], at=lineColumn(code);
  walkRuntime(parse(code,{lang}),(node,local)=>{
    if (typeof node.type==='string') { const why=refusal(node,local); if (why) found.push(label+':'+at(node.start)+': '+why); }
  });
  return found;
}

// The standard library is what every executor runs: the browser, and Hermes
// natively (js/src/standard.js fills what it lacks of ES2023). ES2024's
// ArrayBuffer resizing, shared memory and the RegExp `v` flag are not in
// Hermes, so they are not in the library. `WebWorker` is the web APIs a data
// module may name, not the DOM's UI types; docs/reference.md lists which of
// them each executor has. `.ts` import paths are the web's own (Bun, Deno,
// TypeScript 5 all take them), and Rolldown resolves them.
export const compilerOptions = {
  noEmit: true,
  strict: true,
  // Installed declaration internals belong to their package; app sources stay strict.
  skipLibCheck: true,
  target: 'ES2023',
  module: 'ESNext',
  moduleResolution: 'bundler',
  allowImportingTsExtensions: true,
  // The app's own `.js` files are typed from their code, not checked; a
  // package's JavaScript is never read for types (`maxNodeModuleJsDepth` 0).
  allowJs: true,
  lib: ['ES2023', 'ES2024.Object', 'ES2024.Collection', 'ES2024.Promise', 'ES2024.String', 'WebWorker'],
  // No package's types are global unless a file names it: an installed
  // `@types/node` or `@types/bun` would declare Node's timers and `process`, which
  // no executor has.
  types: [],
};
// The library files that configuration loads (ES2023 is ES5 through ES2023,
// with their parts and the decorators). A package's `/// <reference lib>`
// would add others, such as the DOM's, which native hosts cannot support.
const year=Math.max(...compilerOptions.lib.map(l=>/^ES(\d{4})$/i.exec(l)?.[1] ?? 0));
const configuredLibrary=(file)=>{
  const name=/^lib\.(.+)\.d\.ts$/.exec(file)?.[1]?.toLowerCase();
  if (!name) return false;
  const es=/^es(\d{4})(?:\.|$)/.exec(name);
  return ['es5','decorators','decorators.legacy','webworker'].includes(name)
    || compilerOptions.lib.some(l=>l.toLowerCase()===name)
    || Boolean(es && Number(es[1])<=year);
};

/** On Snapback, a successful mutation record says ok:true; refusals throw.
 * Narrow the captured seam, so TypeScript checks aliases and helpers without
 * interpreting application code. Ordinary resource records retain boolean ok. */
export function snapbackMutationTypes(declarations) {
  const original="export type Result<S extends Source> = SourceMap[S]['result'];";
  if(!declarations.includes(original)) return declarations;
  if(!declarations.includes('export type MutationSource =')) throw new Error('Snapback mutation types need regenerated Contract declarations; rebuild the Contract compiler');
  return declarations.replace(original,
    "type SnapbackMutationSuccess<R> = R extends {ok: boolean} ? R & {ok: true} : R;\n"+
    "export type Result<S extends Source> = S extends MutationSource ? SnapbackMutationSuccess<SourceMap[S]['result']> : SourceMap[S]['result'];");
}

/** `appId`, `grants` and the export names of the module `code` (app.ts),
 * read from its syntax: no build runs app or package code to learn them. Each
 * of `appId` and `grants` is a constant: a string, a template, `+` or an
 * array's `.join(…)` of constants, or a top-level `const` holding one; any
 * other is refused by name and place. Keyed grants have already become a
 * string in `injectBackendOrigin`. An absent one is `undefined`. */
export function staticFacts(code, parse) {
  const parsed=parse('app.ts',code);
  if (parsed.errors?.length) return { appId: undefined, grants: undefined, exported: [] };
  const body=parsed.program.body, at=lineColumn(code), consts=new Map(), exported=new Map();
  for (const statement of body) {
    const s=['ExportNamedDeclaration','ExportDefaultDeclaration'].includes(statement.type) ? statement.declaration ?? statement : statement;
    if (s.type==='VariableDeclaration') for (const d of s.declarations) {
      if (d.id.type!=='Identifier') continue;
      if (s.kind==='const') consts.set(d.id.name,d.init);
      if (statement.type==='ExportNamedDeclaration') exported.set(d.id.name,d.id.name);
    }
    if (statement.type==='ExportNamedDeclaration' && s.id?.name) exported.set(s.id.name,s.id.name);
    if (statement.type==='ExportNamedDeclaration' && !statement.source) for (const x of statement.specifiers ?? []) exported.set(x.exported.name ?? x.exported.value,x.local.name ?? x.local.value);
    if (statement.type==='ExportDefaultDeclaration') exported.set('default','default');
  }
  const value=(name)=>{
    if (!exported.has(name)) return undefined;
    const seen=new Set();
    const constant=(node)=>{
      node=bare(node);
      const refuse=()=>{ throw new Error(`app.ts:${at(node?.start ?? 0)}: ${name} must be a constant the build reads without running the module: a string, a template or + of constants, an array of them .join('\\n'), or a top-level const holding one`); };
      if (!node) refuse();
      if (node.type==='Literal' && ['string','number'].includes(typeof node.value)) return node.value;
      if (node.type==='TemplateLiteral') return node.quasis.map((q,i)=>q.value.cooked+(i<node.expressions.length ? constant(node.expressions[i]) : '')).join('');
      if (node.type==='BinaryExpression' && node.operator==='+') return constant(node.left)+constant(node.right);
      if (node.type==='Identifier' && consts.has(node.name) && !seen.has(node.name)) { seen.add(node.name); const v=constant(consts.get(node.name)); seen.delete(node.name); return v; }
      if (node.type==='CallExpression' && node.arguments.length<=1 && propertyName(bare(node.callee))==='join' && bare(bare(node.callee).object)?.type==='ArrayExpression')
        return bare(bare(node.callee).object).elements.map(constant).join(node.arguments.length ? constant(node.arguments[0]) : ',');
      return refuse();
    };
    // A number is refused: native evaluation reads `grants: string`, and an app id is a string too.
    const v=constant(consts.get(exported.get(name)) ?? null);
    if (typeof v!=="string") throw new Error(`app.ts: ${name} must be a string; it is the number ${v}`);
    return v;
  };
  return { appId: value('appId'), grants: value('grants'), exported: [...exported.keys()] };
}

export function configure(stage, incremental=false, parse) {
  const app=capturedOrigins(stage).at(-1)[1], modulePath=resolve(stage,'app.ts');
  if (existsSync(modulePath)) {
    const before=readFileSync(modulePath,'utf8'), original=resolve(stage,'__exact_backend_source.json');
    const authored=existsSync(original) ? JSON.parse(readFileSync(original,'utf8')) : before;
    const selection=backendOriginSelection(app,process.env,parse), warning=backendOriginWarning(authored,selection,parse);
    // `import.meta.env` first: a `const origin` it sets is a literal the backend selection may then override.
    const settings=inlineSettings(authored,app,parse);
    writeFileSync(resolve(stage,'__exact_env.json'), JSON.stringify(settings.record));
    let injected;
    try { injected=injectBackendOrigin(settings.code,selection?.origin ?? null,parse); }
    catch (error) {
      if (!selection) throw error;
      throw Object.assign(new Error(`${selection.source}: ${error.message.replace(/^EXACT_BACKEND_ORIGIN: /,'')}`),{code:'backend-selection'});
    }
    const after=developmentGrants(injected,parse);
    writeFileSync(resolve(stage,'__exact_backend.json'), JSON.stringify({origin:literalBackendOrigin(after,parse),warning}));
    if (before!==after) writeFileSync(modulePath,after);
    // Every producer refuses alike what the web build cannot read without running it.
    if (parse) staticFacts(after,parse);
  }
  const inside = (path) => {
    if (path !== stage && !path.startsWith(stage+sep)) throw new Error('tsconfig: path outside captured app: '+path);
    return path;
  };
  const origins=capturedOrigins(stage);
  const source = path => {
    const [at,from]=origins.find(([at])=>within(path,at));
    return resolve(from,relative(at,path));
  };
  const captured = path => {
    const match=[...origins].sort((a,b)=>b[1].length-a[1].length).find(([,from])=>within(path,from));
    if (!match) throw new Error('tsconfig: path outside captured app: '+path);
    return inside(resolve(match[0],relative(match[1],path)));
  };
  const local = path => './'+relative(stage,captured(path)).split(sep).join('/');
  const object = (value, label) => {
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('tsconfig: '+label+' must be an object');
    return value;
  };
  const chain = new Set();
  function read(path) {
    inside(path);
    if (chain.has(path)) throw new Error('tsconfig: cyclic extends: '+path);
    chain.add(path);
    const json=object(Bun.JSONC.parse(readFileSync(path,'utf8')),path), dir=source(dirname(path));
    let inherited={};
    for (const parent of json.extends === undefined ? [] : Array.isArray(json.extends) ? json.extends : [json.extends]) {
      if (typeof parent !== 'string' || !parent.startsWith('.')) throw new Error('tsconfig: extends must name a captured relative config');
      let target=captured(resolve(dir,parent));
      if (!existsSync(target) && !target.endsWith('.json')) target+='.json';
      inherited={...inherited,...read(target)};
    }
    const options=object(json.compilerOptions ?? {},'compilerOptions');
    if (options.baseUrl !== undefined) {
      if (typeof options.baseUrl !== 'string') throw new Error('tsconfig: baseUrl must be a string');
      inherited.base=resolve(dir,options.baseUrl);
      captured(inherited.base);
    }
    if (options.paths !== undefined) inherited.paths={map:object(options.paths,'paths'),dir};
    chain.delete(path);
    return inherited;
  }
  const entry=resolve(stage,'tsconfig.json');
  const {base,paths:declared}=existsSync(entry) ? read(entry) : {};
  const paths={};
  for (const [key,values] of Object.entries(declared?.map ?? {})) {
    if (key.split('*').length>2 || !Array.isArray(values) || !values.length || values.some(v=>typeof v!=='string' || v.split('*').length>2 || isAbsolute(v))) {
      throw new Error('tsconfig: paths.'+key+' must name relative paths with at most one wildcard');
    }
    paths[key]=values.map(value=>local(resolve(base ?? declared.dir,value)));
    // TypeScript 7 removed baseUrl. Lower its fallback to paths, shared with
    // Rolldown, so existing editor configuration retains the same resolution.
    if (base) paths[key].push(local(resolve(base,key)));
  }
  if (base && !paths['*']) paths['*']=[local(resolve(base,'*'))];
  for (const [name,file] of moduleBindings(stage,app)) paths[name] ??= [file];
  const config={compilerOptions:{...compilerOptions,paths,
    ...(incremental ? {incremental:true,tsBuildInfoFile:resolve(stage,'__exact_build.tsbuildinfo')} : {})},files:['__exact_entry.ts'],
    // The app's own declarations, as an editor's project has them (an
    // untyped package's `declare module`); a package's are read when named.
    include:['**/*.d.ts'],exclude:['**/node_modules']};
  const path=resolve(stage,'__exact_tsconfig.json'), bytes=JSON.stringify(config);
  if (!existsSync(path) || readFileSync(path,'utf8')!==bytes) writeFileSync(path,bytes);
  if(capturedOrigins(stage).slice(0,-1).some(([,from])=>/(?:^|[/\\])snapback4[/\\](?:ts|online)$/.test(from))) {
    const declaration=resolve(stage,'app.contract.d.ts');
    const before=readFileSync(declaration,'utf8'), after=snapbackMutationTypes(before);
    if(before!==after) writeFileSync(declaration,after);
  }
  stagePackages(stage,parse);
}

// A native module's names in data code (LLP 1133.002 D13): an `import … from
// "@exact/geolocation"` reaches a binding generated here from the package's
// `exact.json`, never committed, so a published package and a `file:` one
// read alike. Each interface installed above the app, the nearest first, is
// and its implementations are written as `__exact_modules/<name>.ts`, mapped by
// `paths`, which the checker and Rolldown share. A call goes through the
// runner's admission: the web runtime's `__exact_module`, else a native
// request (`exact-native:`), whose answer the runner gives as the module's
// declaration says, an error as `{code, message}` (D1).
// Members other interfaces declare on `name`'s names, by name, with their interface's shapes (LLP 1133.014 D10 item 1).
const membersOf=(name, interfaces)=>interfaces.flatMap(i=>Object.entries(i.declaration.members ?? {}).filter(([o])=>o.split('#')[0]===name).flatMap(([o,m])=>Object.entries(m ?? {}).map(([option,spec])=>[o.split('#')[1],{ option, type: spec.type, answers: spec.answers, shapes: i.declaration.shapes ?? {} }]))).reduce((out,[fn,m])=>((out[fn] ??= []).push(m), out),{});
function moduleBindings(stage, app) {
  // The target the build compiles, as an implementation's `targets` names it, or none (a loop serving several).
  const mapping=resolve(stage,'__exact_paths.json'), named=existsSync(mapping) ? JSON.parse(readFileSync(mapping,'utf8')).target : null;
  const target=['web','ios','macos','tvos','android','windows','linux'].includes(named) ? named : null, out=new Map();
  const packages=modulePackages(app), interfaces=packages.filter(i=>i.declaration.kind==='interface');
  const declarations=[...packages, ...interfaces.flatMap(i=>i.implementations.map(impl=>({name:impl.name,root:impl.dir,declaration:impl.json,implementations:[impl]})))];
  for (const {name,root,declaration,implementations} of declarations) {
    const file='__exact_modules/'+name.replace(/^@/,'').replace(/\//g,'__')+'.ts';
    mkdirSync(resolve(stage,'__exact_modules'),{recursive:true});
    // An interface the executor implements carries its own binding (LLP 1133.007 D12).
    const own=Array.isArray(declaration.executor) && typeof declaration.binding==='string' ? readFileSync(resolve(root,declaration.binding),'utf8') : null;
    const iface=declaration.kind==='implementation' ? interfaces.find(i=>i.name===String(declaration.implements).replace(/@[^@]*$/,'')) : null;
    const spec=iface ? {...declaration,shapes:Object.assign({},iface.declaration.shapes,...iface.implementations.map(i=>i.json.shapes),declaration.shapes)} : declaration;
    const text=own ?? moduleBinding(name,spec,declaration.kind==='implementation' ? [declaration] : implementations.map(i=>i.json),names=>domDeclarations(app,names),membersOf(name,interfaces),target), at=resolve(stage,file);
    if (!existsSync(at) || readFileSync(at,'utf8')!==text) writeFileSync(at,text);
    out.set(name,'./'+file);
  }
  return out;
}

/** Each module interface installed above `app`, the nearest first: its
 * package's name and directory, its declaration, and the implementations
 * its package carries (`exports` subpaths whose target is an
 * implementation's `exact.json`). */
export function moduleInterfaces(app) {
  return modulePackages(app).filter(i=>i.declaration.kind==='interface');
}

function modulePackages(app) {
  const out=new Map(), read=(path)=>{ try { return JSON.parse(readFileSync(path,'utf8')); } catch { return null; } };
  const entry=(exports, key)=>{
    const t=exports && typeof exports==='object' ? (Object.keys(exports).some(k=>k.startsWith('.')) ? exports[key] : key==='.' ? exports : null) : key==='.' ? exports : null;
    return typeof t==='string' ? t : t && typeof t==='object' ? (t.contract ?? t.default ?? null) : null;
  };
  for (let dir=app; ; dir=dirname(dir)) {
    const modules=join(dir,'node_modules');
    if (existsSync(modules)) {
      const names=readdirSync(modules).flatMap(n=>n.startsWith('@') ? (statSync(join(modules,n)).isDirectory() ? readdirSync(join(modules,n)).map(m=>n+'/'+m) : []) : [n]);
      for (const name of names) {
        if (out.has(name)) continue;
        const root=join(modules,name), pkg=read(join(root,'package.json')), main=entry(pkg?.exports,'.');
        if (!pkg || !main || !/(^|\/)exact\.json$/.test(main)) continue;
        const declaration=read(resolve(root,main));
        if (!['interface','implementation'].includes(declaration?.kind)) continue;
        const implementations=Object.keys(pkg.exports).filter(k=>k.startsWith('./')).map(k=>({name:name+k.slice(1),file:entry(pkg.exports,k)}))
          .filter(i=>i.file && /\/exact\.json$/.test(i.file)).map(i=>({name:i.name,dir:dirname(resolve(root,i.file)),json:read(resolve(root,i.file))})).filter(d=>d.json?.kind==='implementation');
        out.set(name,{name,root,declaration,implementations});
      }
    }
    if (dirname(dir)===dir) break;
  }
  return [...out.values()];
}

/** The implementation bound to interface `iface` for `target` (LLP 1133.001
 * D4): the one `app.json` names for the target, else the app's one direct
 * dependency that implements it there, else the one the interface carries;
 * `null` for none. Each choice is checked against the interface's version. */
export function implementationFor(app, iface, target) {
  const read=(path)=>{ try { return JSON.parse(readFileSync(path,'utf8')); } catch { return null; } };
  const located=(name)=>{
    const parts=name.split('/'), take=name.startsWith('@') ? 2 : 1;
    const packageName=parts.slice(0,take).join('/'), subpath=parts.slice(take).join('/');
    const key=subpath ? `./${subpath}` : '.';
    for (let dir=app; ; dir=dirname(dir)) {
      const root=join(dir,'node_modules',packageName), pkg=read(join(root,'package.json'));
      const main=typeof pkg?.exports==='string' && key==='.' ? pkg.exports : pkg?.exports?.[key];
      const at=typeof main==='string' ? main : main?.contract ?? main?.default;
      if (pkg && typeof at==='string' && /(^|\/)exact\.json$/.test(at)) { const json=read(resolve(root,at)); return json && {name,dir:dirname(resolve(root,at)),json}; }
      if (dirname(dir)===dir) return null;
    }
  };
  const covers=(i)=>i?.json?.kind==='implementation' && String(i.json.implements ?? '').replace(/@[^@]*$/,'')===iface.name && (i.json.targets ?? []).includes(target);
  const checked=(i)=>{
    const range=String(i.json.implements).slice(iface.name.length+1), version=read(join(iface.root,'package.json'))?.version;
    if (range && version && !Bun.semver.satisfies(version,range)) throw new Error(`bake-implementation-mismatch: ${i.name ?? iface.name} implements ${i.json.implements}, and the installed ${iface.name} is ${version}`);
    return i;
  };
  const manifest=read(join(app,'app.json')), named=manifest?.implementations?.[iface.name]?.[target];
  if (named) {
    const i=located(named);
    if (!covers(i)) throw new Error(`bake-implementation-missing: app.json names ${named} for ${iface.name} on ${target}, and ${i ? `it does not implement it for ${target}` : 'it is not installed'}`);
    return checked(i);
  }
  const deps=Object.keys(read(join(app,'package.json'))?.dependencies ?? {}).filter(n=>n!==iface.name).map(located).filter(covers);
  if (deps.length>1) throw new Error(`bake-implementation-ambiguous: ${deps.map(d=>d.name).join(' and ')} each implement ${iface.name} for ${target}; name one in app.json: "implementations": { "${iface.name}": { "${target}": "${deps[0].name}" } }`);
  if (deps.length) return checked(deps[0]);
  const carried=iface.implementations.find(i=>(i.json.targets ?? []).includes(target));
  return carried ? checked({name:iface.name,...carried}) : null;
}

/** The app's Rust data sources' text (`data/src`), where a generated binding's call writes its literal options. */
function rustSources(app) {
  const walk=(d)=>{ try { return readdirSync(d,{withFileTypes:true}).flatMap(e=>e.isDirectory() ? walk(join(d,e.name)) : e.name.endsWith('.rs') ? [readFileSync(join(d,e.name),'utf8')] : []); } catch { return []; } };
  return walk(join(app,'data','src')).join('\n');
}

/** The app's own source text that can name an interface: `app.ts`, and every
 * `.contract` file in the app's tree, since a root `app.contract` may import
 * the components that use one from a subdirectory. */
function appText(app) {
  const contracts=(d)=>{ try { return readdirSync(d,{withFileTypes:true}).flatMap(e=>e.isDirectory() ? (/^(\.|node_modules$|target$)/.test(e.name) ? [] : contracts(join(d,e.name))) : e.name.endsWith('.contract') ? [join(d,e.name)] : []); } catch { return []; } };
  return [join(app,'app.ts'),...contracts(app)].map(f=>{ try { return readFileSync(f,'utf8'); } catch { return ''; } }).join('\n');
}

/** The specifiers a TypeScript or JavaScript file imports and re-exports from, read from its syntax tree, so a string or
 * comment that spells an import is none: `import … from "x"` and `export … from "x"` (not `import type` or `export
 * type`), `import "x"` and `import("x")`. The compiler reads the same set by tokens (`contract/cli/src/sources/data.rs`). */
function moduleSpecifiers(file, text) {
  const { parseSync } = createRequire(import.meta.url)('rolldown/utils');
  const { program } = parseSync(file, text, { sourceType: 'module' });
  const out = [];
  const visit = (node) => {
    if (Array.isArray(node)) { for (const n of node) visit(n); return; }
    if (!node || typeof node !== 'object') return;
    if (/^(Import|ExportNamed|ExportAll)Declaration$/.test(node.type) && node.source && (node.importKind ?? node.exportKind) !== 'type') out.push(node.source.value);
    if (node.type === 'ImportExpression' && typeof node.source?.value === 'string') out.push(node.source.value);
    for (const key in node) visit(node[key]);
  };
  visit(program);
  return out;
}

/** The packages the app imports, wherever it imports them (LLP 1133.004
 * stage 0, LLP 1133 D6 rule 6): every Contract file `app.contract` reaches
 * through relative `use … from "./…"`, and every TypeScript file `app.ts`
 * reaches through relative imports, each bare specifier taken to its
 * package's name. `text` is those files' text, where a literal option a
 * build reads (an Info.plist key) is found. */
export function usedPackages(app) {
  const names=new Set(), seen=new Set(), texts=[];
  const packageName=(spec)=>{ const parts=spec.split('/'); return spec.startsWith('@') ? parts.slice(0,2).join('/') : parts[0]; };
  // A TypeScript import may name the emitted file (`./x.js` for `./x.ts`).
  const tsFile=(base)=>[...['', '.ts', '.tsx', '.mts', '.js', '.mjs', '/index.ts', '/index.js'].map(e=>base+e), base.replace(/\.m?js$/,'.ts'), base.replace(/\.m?js$/,'.tsx')].find(f=>{ try { return statSync(f).isFile(); } catch { return false; } });
  const visit=(file, contract)=>{
    if (!file || seen.has(file) || file.split(sep).includes('node_modules')) return;
    seen.add(file);
    let text='';
    try { text=readFileSync(file,'utf8'); } catch { return; }
    texts.push(text);
    // A Contract file's `use` lines, its comments left out.
    const specs=contract
      ? [...text.replace(/\/\*[\s\S]*?\*\//g,'').replace(/(^|[^:'"\\])\/\/.*$/gm,'$1').matchAll(/^\s*use\b[^\n]*?\bfrom\s+"([^"]+)"/gm)].map(m=>m[1])
      : moduleSpecifiers(file, text);
    for (const spec of specs) {
      if (spec.startsWith('.')) visit(contract ? resolve(dirname(file),spec) : tsFile(resolve(dirname(file),spec)), contract);
      else if (!spec.startsWith('exact:') && !spec.startsWith('node:')) names.add(packageName(spec));
    }
  };
  visit(join(app,'app.contract'), true);
  visit(tsFile(join(app,'app')), false);
  return { names, text: texts.join('\n') };
}

/** The Swift implementations the app's interfaces carry for `target` (`ios`,
 * `macos`, `tvos`), of the `extensions` in the compiled plan (LLP 1133.001
 * D4): each extension, its own Swift module's name,
 * its class, its source files, what it adds to Info.plist (`plist`,
 * `plistKeys`), and the system frameworks it needs from an OS version
 * (`frameworks`, LLP 1133.016 D15). */
export function appleImplementations(app, target, extensions, contractSources = []) {
  const { text } = usedPackages(app), named = `${text}\n${appText(app)}\n${contractSources.map(path => readFileSync(path, 'utf8')).join('\n')}`;
  return moduleInterfaces(app).filter(i=>extensions[i.name.split('/').at(-1)] === i.name).flatMap(i=>{
    const impl=implementationFor(app,i,target);
    const files=impl?.json.platform==='apple' && impl.json.sources?.swift;
    if (!impl || !files?.length || !impl.json.class) return [];
    const extension=i.name.split('/').at(-1);
    // An Info.plist dictionary keyed by each literal strings key the app passes at `name.option` (LLP 1133.002 D10).
    // A literal value is `plistKeys`', which host/apple checks against its allow-list.
    const declared=impl.json.plist ?? {};
    if (typeof declared!=='object' || Array.isArray(declared)) throw new Error(`${extension}'s Apple implementation: its plist is not an object`);
    const plist=Object.entries(declared).map(([key,at])=>{
      if (typeof at!=='string' || !/^\w+\.\w+$/.test(at)) throw new Error(`${extension}'s Apple implementation: its plist gives ${key} ${JSON.stringify(at)}, not "<call>.<option>" (a literal value is plistKeys')`);
      const [name,option]=at.split('.'), keys=new Set();
      for (const [,inside] of named.matchAll(new RegExp(`\\b${name}\\s*\\(([^)]*)\\)`,'g')))
        for (const [,,value] of inside.matchAll(new RegExp(`\\b${option}\\s*[=:]\\s*(["'])([^"']+)\\1`,'g'))) keys.add(value);
      // A Rust data source's generated binding takes the option as a snake_case field (`purpose_key: "…"`).
      const snake=option.replace(/[A-Z]/g,c=>'_'+c.toLowerCase());
      for (const [,value] of rustSources(app).matchAll(new RegExp(`\\b${snake}\\s*:\\s*(?:Some\\()?"([^"]+)"`,'g'))) keys.add(value);
      return {key,keys:[...keys].sort()};
    }).filter(p=>p.keys.length);
    // An interface that declares an authenticator entry (LLP 1133.010 D13): its class serves it in process.
    const authenticator=!!i.declaration?.authenticator?.apple;
    const events=(i.declaration.names ?? []).some(n=>n.event);
    return [{privacyManifest:resolve(impl.dir,'PrivacyInfo.xcprivacy'),extension,module:'ExactPkg_'+extension.replace(/\W/g,'_'),className:impl.json.class,sources:files.map(f=>resolve(impl.dir,f)),plist,plistKeys:impl.json.plistKeys ?? null,views:packageViewTags(i,impl),authenticator,start:i.declaration?.start==='load',frameworks:impl.json.frameworks??{},activity:impl.json.apple?.activity??null}];
  });
}

/** The view tags an implementation serves (LLP 1133.001 D1): every view its interface declares, and its own. */
function packageViewTags(iface, impl) {
  const tags=v=>Array.isArray(v) ? v.filter(t=>typeof t==='string') : v && typeof v==='object' ? Object.keys(v) : [];
  return [...new Set([...tags(iface.declaration.views), ...tags(impl?.json.views)])].sort();
}

/** The Kotlin implementations bound for Android (LLP 1133.001 D4) of the compiled plan's `extensions`: each extension,
 * its class, its source directories, which join the APK's Kotlin, and the Maven coordinates it names, which join the
 * one locked Gradle graph (LLP 1133.008 D8 item 5). */
export function androidImplementations(app, extensions) {
  return moduleInterfaces(app).filter(i=>extensions[i.name.split('/').at(-1)] === i.name).flatMap(i=>{
    const impl=implementationFor(app,i,'android'), dirs=impl?.json.platform==='android' && impl.json.sources?.kotlin;
    if (!impl || !dirs?.length || !impl.json.class) return [];
    return [{extension:i.name.split('/').at(-1),className:impl.json.class,dirs:dirs.map(d=>resolve(impl.dir,d)),views:packageViewTags(i,impl),mavenSource:resolve(impl.dir,'exact.json'),maven:impl.json.maven === undefined ? [] : impl.json.maven,manifest:impl.json.manifest ?? null,start:i.declaration?.start==='load'}];
  });
}

/** Each used interface's package configuration for `target` (LLP 1133.021 §9 item 7): extension name to the
 * values `app.json` gives under `packages.<interface>.<target>`, for the keys the interface's `config` declares. A
 * key typed `env` names a variable (`{"env": "NAME"}`) whose value is read here and never written to the repo. A
 * required key that is absent throws its declared code; one required only at `release` throws only for a release. */
export function packageConfig(app, target, { extensions, release = false }) {
  let manifest = {};
  try { manifest = JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8')); } catch { /* a plain Contract directory */ }
  const out = new Map();
  for (const i of moduleInterfaces(app).filter(i => extensions[i.name.split('/').at(-1)] === i.name)) {
    const declared = i.declaration?.config?.[target];
    if (!declared) continue;
    const given = manifest.packages?.[i.name]?.[target] ?? {}, config = {};
    for (const [key, d] of Object.entries(declared)) {
      let value = given[key];
      if (d.type === 'env' && value !== undefined) value = typeof value?.env === 'string' ? process.env[value.env] : undefined;
      const required = d.required === true || (d.required === 'release' && release);
      if ((value === undefined || value === '') && required) {
        const how = d.type === 'env' ? `{"env": "<VARIABLE>"} naming a variable that is set` : `a ${d.type}`;
        throw new Error(`${d.missing ?? 'bake-config-missing'}: app.json "packages": {"${i.name}": {"${target}": {"${key}": …}}} needs ${how}`);
      }
      if (value !== undefined && value !== '') config[key] = value;
    }
    out.set(i.name.split('/').at(-1), config);
  }
  return out;
}

/** Each extension's web implementation, as bound (LLP 1133.001 D4):
 * extension name to the implementation's JavaScript entry. */
export function webImplementations(app, extensions) {
  const out=new Map();
  for (const iface of moduleInterfaces(app).filter(i => extensions[i.name.split('/').at(-1)] === i.name)) {
    const web=implementationFor(app,iface,'web'), file=web?.json.sources?.javascript?.[0];
    if (file) out.set(iface.name.split('/').at(-1),resolve(web.dir,file));
  }
  return out;
}

/** The declarations of the DOM's `names` (WebAuthn's JSON dictionaries, LLP
 * 1133.011 D10), and of each type they name, copied from the TypeScript
 * compiler's own `lib.dom.d.ts`, which the data module's libraries leave
 * out: the binding is in the stage, so the check reads them there. A name
 * the library lacks is an opaque JSON value, narrowed by the caller. */
function domDeclarations(app, names) {
  let text='';
  // The compiler above the app, else exact2's own (an app outside the repo installs none).
  for (const from of [resolve(app,'package.json'), import.meta.url]) {
    try {
      const pkg=createRequire(from).resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`);
      text=readFileSync(resolve(dirname(pkg),'lib','lib.dom.d.ts'),'utf8');
      break;
    } catch { /* neither has one: each name is opaque */ }
  }
  const found=new Map(), queue=[...names];
  const declared=n=>new RegExp(`^(interface ${n}\\b[^{]*\\{\\n[\\s\\S]*?\\n\\}|type ${n} = [^;]*;)`,'m').exec(text)?.[1];
  while (queue.length) {
    const n=queue.shift();
    if (found.has(n)) continue;
    const decl=declared(n);
    found.set(n, decl ? 'export '+decl : `export type ${n} = unknown;`);
    if (decl) for (const [ref] of decl.slice(decl.indexOf(n)+n.length).matchAll(/\b[A-Z][A-Za-z0-9]*\b/g)) if (!found.has(ref) && declared(ref)) queue.push(ref);
  }
  return [...found.values()];
}

/** A name that is not a source (`Result<'posted'>`, a mutation's slot) fails
 * as "does not satisfy the constraint 'keyof SourceMap'", which lists none:
 * say which there are (sb4 farm wave 7, E3: 061 and 066, 15 minutes). */
function sourceNames(stage,diagnostics) {
  if(!/constraint 'keyof SourceMap'/.test(diagnostics))return '';
  let text='';
  try { text=readFileSync(resolve(stage,'app.contract.d.ts'),'utf8'); } catch { return ''; }
  const map=text.match(/export interface SourceMap \{\n([\s\S]*?)\n\}/)?.[1] ?? '';
  const names=[...map.matchAll(/^\s+("[^"]+"|[A-Za-z_$][\w$]*):/gm)].map(m=>m[1].replace(/^"|"$/g,''));
  return names.length ? `\nnote: Result<…> and Args<…> take a source's name, what the contract calls (\`resource x = name(…)\`, \`send slot = name(…)\`), never a resource's or mutation's own: ${names.map(n=>`'${n}'`).join(', ')}\n` : '';
}

// Short-lived native checks trade fewer GC cycles for a soft Go memory limit
// (not an RSS cap); explicit user settings always win.
const checkEnv={GOGC:'300',GOMEMLIMIT:'128MiB',GOMAXPROCS:'4',...process.env};

/** Value exports of the configured module namespace, including CommonJS re-exports; or what `inspect`, given the
 * checker, the namespace's type and the checker API, makes of them. */
export async function valueExports(stage, libraries, inspect) {
  const module = resolve(stage, 'app.ts');
  const compiler = createRequire(resolve(libraries, 'tsc')).resolve('typescript/unstable/async');
  const checking = await import(pathToFileURL(compiler).href), api = new checking.API({ cwd: stage });
  try {
    const config = resolve(stage, '__exact_tsconfig.json');
    const snapshot = await api.updateSnapshot({ openProjects: [config] });
    try {
      const { program, checker } = snapshot.getProject(config);
      checkFiles(stage, libraries, await program.getSourceFileNames());
      const file = await program.getSourceFile(module);
      const type = await checker.getTypeAtLocation(file);
      if (inspect) return await inspect(checker, type, checking);
      return (await checker.getPropertiesOfType(type)).map(symbol => symbol.name);
    } finally { await snapshot.dispose(); }
  } finally { await api.close(); }
}

/** Select the native canvas seam before checking or bundling, so CommonJS namespaces cannot retain an unused recorder. */
export async function prepareCanvas(stage, libraries) {
  const path = resolve(stage, '__exact_entry.ts'), before = readFileSync(path, 'utf8');
  const marker = '// __exact_canvas_entry\n';
  if (!before.includes(marker)) return;
  const exported = await valueExports(stage, libraries);
  const draws = exported.includes('draw');
  if (draws !== exported.includes('surfaces'))
    throw new Error('app.ts exports `draw` and `surfaces` together, or neither (LLP 1056 D1)');
  const entry = draws ? `import { canvasSeam } from './__exact_canvas.js';
export const abi = 2;
export const surfacesJson = JSON.stringify(app.surfaces);
const seam = canvasSeam(app.draw);
export const drawCanvas = (request: string, host?: unknown): string => seam.draw(JSON.parse(request), host);
export const retireCanvases = (retired: string): void => seam.retire(JSON.parse(retired));
` : 'export const abi = 1;\n';
  writeFileSync(path, before.replace(marker, entry));
}

function checkFiles(stage, libraries, files) {
  stage = realpathSync.native(stage);
  libraries = realpathSync.native(libraries);
  const allowed = path => within(path, stage) || (within(path, libraries) && configuredLibrary(relative(libraries, path)));
  for (const path of files) {
    if (path && !allowed(realpathSync.native(path))) throw new Error('module outside captured app: ' + path);
  }
  checkPeers(stage,files);
}

// Use the compiler's graph, including relative declaration imports and exports,
// rather than requiring peers from every subpath the package happens to ship.
function checkPeers(stage, files) {
  const record=resolve(stage,PACKAGES);
  if (!existsSync(record)) return;
  const {peerRefusals=[]}=JSON.parse(readFileSync(record,'utf8'));
  if (!peerRefusals.length) return;
  const used=new Set(files.filter(isAbsolute).map(physicalId));
  for (const [file,why] of peerRefusals) if (used.has(physicalId(resolve(stage,file)))) throw new Error(why);
}

/** Type-check a configured stage with the native compiler `tsc` (whose
 * standard libraries are in `libraries`): its diagnostics, or the compiler's
 * actual resolved graph, type-only imports included, must stay inside the
 * stage (a package's declarations are in it: `stagePackages`) and the
 * configured libraries. An external declaration must not influence an
 * accepted app artifact. */
/** The checker runs before the bundle refuses a Node built-in, and says only "Cannot find module 'node:fs'… Try
 * `npm i --save-dev @types/node`", which changes nothing (expo/exact-build#183). Said here as the bundle says it. */
export function nodeNote(diagnostics) {
  // TS2591 names an import's specifier as a module ("Cannot find module 'fs'") or, as the bake's checker words a
  // `node:` import, a name ("Cannot find name 'node:fs'"); a bare name there (`process`) is a global, said below.
  const modules=[...new Set([...diagnostics.matchAll(/Cannot find module '([^']+)'|Cannot find name '(node:[^']+)'/g)].map(match=>match[1] ?? match[2]).filter(builtin))];
  const names=/Cannot find name '(?:require|process|Buffer|__dirname|__filename)'/.test(diagnostics);
  if (!modules.length && !names) return '';
  return `\nnote: ${modules.length ? `${modules.map(m=>`'${m}'`).join(', ')} ${modules.length>1 ? "are Node's built-ins" : "is Node's built-in"}` : "Node's globals are"}: a data module runs on Hermes and in the browser, which have no Node modules, so @types/node does not help; import a part of the package that needs none, or another package\n`;
}
export async function check(stage, tsc, libraries) {
  // TypeScript expands Windows short paths; compare the same physical names
  // for the capture, libraries and files, including directory junctions.
  stage=realpathSync.native(stage);
  const entry=resolve(stage,'__exact_entry.ts'), declaration=resolve(stage,'app.contract.d.ts');
  if (existsSync(declaration) && readFileSync(declaration,'utf8').includes('export type LanguageModelToolSchemas =')) {
    const before=readFileSync(entry,'utf8').split(TOOL_CHECK)[0];
    const tools=languageModelTools(readFileSync(declaration,'utf8'),await languageModelToolSources(stage,bundleOptions),(...a)=>createRequire(import.meta.url)('rolldown/utils').parseSync(...a));
    const unowned=tools.length ? await valueExports(stage,libraries,(checker,type,api)=>unownedTools(checker,type,[...new Set(tools.map(t=>t.name))],api)) : [];
    const after=before+languageModelToolCheck(tools,unowned);
    if (readFileSync(entry,'utf8')!==after) writeFileSync(entry,after);
  }
  // A package's JavaScript entry (`typescript/bin/tsc`, an `EXACT_TSC`
  // override) keeps its Node shebang: run it through Bun, as the one-shot
  // producer runs its tools (js/bake/src/lib.rs `run`).
  let head='';
  try { const fd=openSync(tsc,'r'), bytes=Buffer.alloc(128); head=bytes.toString('utf8',0,readSync(fd,bytes,0,128,0)).split('\n')[0]; closeSync(fd); } catch { /* not readable: execFile says so */ }
  const script=head.startsWith('#!') && head.split(/\s+/).some(word=>['node','bun'].includes(word.split('/').at(-1)));
  const args=['--project',resolve(stage,'__exact_tsconfig.json'),'--pretty','false','--listFiles'];
  return new Promise((done, fail) => execFile(script ? process.execPath : tsc,script ? [tsc,...args] : args,
    {cwd:stage,env:checkEnv,encoding:'utf8',maxBuffer:4*1024*1024,timeout:checkBound(),killSignal:'SIGKILL'}, (error, stdout, stderr) => {
      // (A tsc that never returns is stopped at the bound: checkBound above.)
      if (error?.killed && error.signal === 'SIGKILL') return fail(new Error(`the TypeScript check did not finish in ${boundWords(checkBound())} (tsc stopped)`));
      if (error) {
        try { checkPeers(stage,String(stdout??'').split(/\r?\n/)); }
        catch (refusal) { return fail(refusal); }
        const diagnostics=String(stdout??'').split(/\r?\n/).filter(line=>! /^(?:\/|[A-Za-z]:[\\/]).*\.[cm]?[jt]sx?$/.test(line)).join('\n');
        const mutationFailure=/Type '(?:false|boolean)' is not assignable to type 'true'|SnapbackMutationSuccess/.test(diagnostics)
          && readFileSync(resolve(stage,'app.contract.d.ts'),'utf8').includes('type SnapbackMutationSuccess<');
        const refusal=mutationFailure ? "\nnote: a Snapback mutation returning {ok:false} leaves failure(slot) empty; throw refused({code: 'YOUR_CODE', message: 'Explain the refusal'}) instead. Annotate successful helpers with Result<'source'>, or keep ok:true literal with `as const`.\n" : '';
        return fail(new Error(diagnostics+languageModelToolNote(readFileSync(entry,'utf8'),diagnostics)+sourceNames(stage,diagnostics)+refusal+nodeNote(diagnostics)+String(stderr??'') || String(error.message)));
      }
      try { checkFiles(stage, libraries, stdout.trim().split(/\r?\n/)); }
      catch (error) { return fail(error); }
      done();
    }));
}

/** Contract build's early ambient check: walk reachable modules without executing application code. */
export async function ambientDiagnostics(app) {
  const {rolldown}=await import('rolldown');
  const found=[];
  let mounts={};
  try { mounts=JSON.parse(readFileSync(resolve(app,'app.json'),'utf8')).typescript?.sources ?? {}; } catch { /* plain Contract directory */ }
  const bundle=await rolldown({input:resolve(app,'app.ts'), logLevel:'silent', external:source=>!!packageName(source),
    plugins:[{name:'contract-ambient', async resolveId(source,importer) {
      if (!importer || !source.startsWith('.')) return null;
      const path=resolve(dirname(importer),source), child=relative(app,path).split(sep);
      return mounts[child[0]] ? this.resolve(resolve(app,mounts[child[0]],...child.slice(1)), importer, {skipSelf:true}) : null;
    }, transform(code,id) { found.push(...ambientRefusals(app,id,code,(c,o)=>this.parse(c,o))); return null; }}]});
  try { await bundle.generate({format:'esm'}); } finally { await bundle.close(); }
  return found.map(line=>{
    const [,file,row,col,message]=/^(.*?):(\d+):(\d+): ([\s\S]*)$/.exec(line);
    return {file:resolve(app,file),line:Number(row),col:Number(col),message};
  });
}
if (process.argv[2]==='--ambient' && basename(process.argv[1] ?? '')==='typescript.mjs') {
  try { console.log(JSON.stringify(await ambientDiagnostics(resolve(process.argv[3])))); }
  catch(error) { console.error(error.message); process.exitCode=1; }
}
