// The settings an app reads at build (LLP 1153 D4): `import.meta.env.NAME` in a top-level const of an app-local module, each name
// declared in app.json's `env`, its value from the environment, then the selected variant's `env` (LLP 1030.008 D6),
// then `.env.local`, then `.env` beside app.json, else the const's `??` default. The value is inlined into the
// captured copy, so every host runs the same module; a name
// marked `"launch": true` keeps the build's value as the one a launch's replaces (LLP 1153.002). The
// native bake writes this file into its stage beside `__exact_config.mjs`, which imports it
// (js/bake/src/resident.rs `ENV`); the web build and the scripts import it from here.
import { existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, statSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, extname, isAbsolute, normalize, relative, resolve, sep } from 'node:path';
import { parseEnv } from 'node:util';
import { dlopen, FFIType } from 'bun:ffi';
import { createRequire } from 'node:module';

/** The files a value is read from after the environment, in order. */
export const ENV_FILES = ['.env.local', '.env'];
const NAME = /^[A-Z][A-Z0-9_]*$/;

// Shared by the settings scan and the bundles. Rolldown adds the import kind and `default` conditions.
export const moduleResolution = {
  mainFields: ['browser', 'module', 'main'], aliasFields: [['browser']], conditionNames: ['browser'],
  extensions: ['.tsx', '.ts', '.jsx', '.js', '.json'],
  extensionAlias: {'.js':['.js','.ts','.tsx'], '.jsx':['.jsx','.tsx'], '.mjs':['.mjs','.mts'], '.cjs':['.cjs','.cts']},
};

// An entry of app.json's `env`: a name, or `{ "name": "API_BASE_URL", "launch": true }`, a name a launch may give
// (LLP 1153.002 D1). `"launch": false`, or an object with only its name, is the bare name.
const entryName = entry => entry !== null && typeof entry === 'object' && !Array.isArray(entry) && typeof entry.name === 'string'
  && Object.keys(entry).every(key => key === 'name' || key === 'launch') && ['boolean', 'undefined'].includes(typeof entry.launch) ? entry.name : entry;
const manifestOf = app => {
  let manifest = {};
  try { manifest = JSON.parse(readFileSync(resolve(app, existsSync(resolve(app, '.shells/app.json')) ? '.shells/app.json' : 'app.json'), 'utf8')); } catch { /* no manifest: none declared */ }
  return manifest;
};

// The `env` of the variant selected for the app in `app` (scripts/variants.mjs `selectedVariant`; this module stays
// self-contained, since the native bake copies it alone), with its name, or null.
const variantEnv = (app, env) => {
  const real = path => { try { return realpathSync(path); } catch { return path; } };
  if (!env.EXACT_VARIANT || !env.EXACT_VARIANT_APP || real(env.EXACT_VARIANT_APP) !== real(app)) return null;
  return { name: env.EXACT_VARIANT, values: manifestOf(app).variants?.[env.EXACT_VARIANT]?.env ?? {} };
};

const manifestEnv = app => { const manifest = manifestOf(app); return Array.isArray(manifest?.env) ? manifest.env : []; };

/** The names app.json's `env` declares in the app directory `app` (none without one); an entry of no shape it takes
 * is kept as written, for `inlineSettings` to refuse. */
export function declaredSettings(app) {
  return settingNames(manifestEnv(app));
}
/** The names of `env`, a manifest's list. */
export const settingNames = env => [...new Set((env ?? []).map(entryName))];
/** The names of `env`, a manifest's list, that a launch may give (`"launch": true`, LLP 1153.002 D1). */
export const launchNames = env => new Set((env ?? []).filter(entry => entry?.launch === true && typeof entryName(entry) === 'string').map(entryName));

// A file's assignments in Node's dotenv grammar (`util.parseEnv`: `NAME=value`, `#` comments, quoted values, taken as
// written, with no `$` expansion; https://nodejs.org/api/util.html#utilparseenvcontent). An absent file assigns none.
const assignments = (app, file) => { const path = resolve(app, file); return existsSync(path) ? parseEnv(readFileSync(path, 'utf8')) : {}; };

// Bun loads `.env` files from its working directory into `process.env` before a script's first line runs
// (https://bun.sh/docs/runtime/env): `.env.local` (except when NODE_ENV is `test`), then `.env.<NODE_ENV, else
// development>`, then `.env`, never over a variable already set. Every process a script starts inherits them as its
// own environment, and `--no-env-file` on a child cannot take them out. The environment a process was started with is
// still there to read, untouched by that loading: the C library's (getenv(3); on Linux /proc/self/environ, proc(5)),
// and on Windows the process's environment block (GetEnvironmentVariableW, which a script's own assignment to
// `process.env` does reach, so only names in Bun's files are asked of it). So the first exact2 script of a command
// (scripts/app.mjs and the app's launcher call `noteBunLoaded`) notes in EXACT_BUN_LOADED each name Bun loaded, one set
// in `process.env` but not in the environment it was given, and every process below inherits the note: those names'
// values are Bun's, never the environment's, whatever the files say later (a dev loop builds long after it started).
// A variable the command was given is the environment's whether or not a file holds the same value. On any other
// system nothing is noted, and a value Bun loaded counts as the environment's.
const bunLoaded = (dir, env) => {
  const mode = env.NODE_ENV || 'development';
  return Object.assign({}, ...['.env', `.env.${mode}`, ...mode === 'test' ? [] : ['.env.local']].map(file => assignments(dir, file)));
};
// Whether the environment this process was started with holds a name; null where it cannot be read.
let startedWith;
const given = () => {
  if (startedWith !== undefined) return startedWith;
  startedWith = null;
  try {
    if (process.platform === 'linux') {
      const names = new Set(readFileSync('/proc/self/environ', 'latin1').split('\0').map(pair => pair.slice(0, pair.indexOf('='))));
      startedWith = name => names.has(name);
    } else if (process.platform === 'darwin') {
      const { symbols } = dlopen('/usr/lib/libSystem.B.dylib', { getenv: { args: [FFIType.cstring], returns: FFIType.cstring } });
      startedWith = name => symbols.getenv(Buffer.from(`${name}\0`)) !== null;
    } else if (process.platform === 'win32') {
      // With no buffer, the length a value needs: 0 for a name the block lacks, and for one whose value is empty, which
      // only the last error tells apart (ERROR_ENVVAR_NOT_FOUND, 203); a success leaves the last error as it was.
      const { symbols } = dlopen('kernel32.dll', {
        GetEnvironmentVariableW: { args: [FFIType.ptr, FFIType.ptr, FFIType.u32], returns: FFIType.u32 },
        GetLastError: { args: [], returns: FFIType.u32 }, SetLastError: { args: [FFIType.u32], returns: FFIType.void },
      });
      startedWith = name => { symbols.SetLastError(0); return symbols.GetEnvironmentVariableW(Buffer.from(`${name}\0`, 'utf16le'), null, 0) > 0 || symbols.GetLastError() !== 203; };
    }
  } catch { /* unreadable: nothing is noted */ }
  return startedWith;
};
// The names Bun loaded into this process from its working directory's `.env` files.
const loadedHere = () => {
  const candidates = Object.keys(bunLoaded(process.cwd(), process.env)).filter(name => process.env[name] !== undefined);
  const has = candidates.length ? given() : null;
  return has ? candidates.filter(name => !has(name)) : [];
};
// The names whose values in `env` are Bun's: those noted above, and, in `process.env`, those this process loaded.
const bunNames = env => new Set([...(env.EXACT_BUN_LOADED ?? '').split(',').filter(Boolean), ...env === process.env ? loadedHere() : []]);

/** Adds to EXACT_BUN_LOADED, which every process this one starts inherits, the names Bun loaded into this process from
 * `.env` files. Each exact2 script calls it before it starts a process (scripts/app.mjs on import), so the first
 * one's note reaches every process below it. */
export function noteBunLoaded() {
  process.env.EXACT_BUN_LOADED = [...bunNames(process.env)].sort().join(',');
}

/** Each of `names` with its value and source (`environment`, `variants.<name>`, `.env.local` or `.env`), or null
 * where none sets it. */
export function settingValues(app, names, env = process.env) {
  const files = Object.fromEntries(ENV_FILES.map(file => [file, assignments(app, file)])), bun = bunNames(env), variant = variantEnv(app, env);
  return Object.fromEntries(names.map(name => {
    if (env[name] !== undefined && !bun.has(name)) return [name, { value: env[name], source: 'environment' }];
    if (variant && Object.hasOwn(variant.values, name)) return [name, { value: variant.values[name], source: `variants.${variant.name}` }];
    const file = ENV_FILES.find(f => Object.hasOwn(files[f], name));
    return [name, file ? { value: files[file][name], source: file } : null];
  }));
}

/** The web build's input digest entries for the app `app` (its directory and manifest): each declared name, keyed
 * `import.meta.env.NAME`, with what the bake inlines for it (its value and source, a default included) as JSON, or
 * null where it would refuse. Defaults come from the build's recorded modules without running them. */
export function settingInputs(app, env = process.env, { modules = new Map(), resolveImport } = {}) {
  const names = settingNames(app.manifest?.env);
  let record = {};
  try {
    if (names.length) {
      const parse = createRequire(import.meta.url)('rolldown/utils').parseSync;
      record = inlineSettings(modules.get('app.ts') ?? '', app.dir, parse, env, {modules,resolveImport}).record;
    }
  } catch { /* A refused graph has no inlined values. */ }
  return Object.fromEntries(names.map(name => [`import.meta.env.${name}`, record[name] ? JSON.stringify(record[name]) : null]));
}

/** The source of a captured package file or a probe beneath it, including checker-only declarations. */
export function packageOrigin(stage, file) {
  for (const [name, rootsOf] of [
    ['__exact_declarations.json', record => record.packages.map(([at,from]) => [resolve(stage,at),from])],
    ['__exact_packages.json', record => record.packages.map(([key,from]) => [resolve(stage,'node_modules/.packages',key),from])],
  ]) {
    try {
      const roots = rootsOf(JSON.parse(readFileSync(resolve(stage,name),'utf8'))).sort((a,b)=>b[0].length-a[0].length);
      const match = roots.find(([at])=>file===at || file.startsWith(at+sep));
      if (match) return resolve(match[1],relative(match[0],file));
    } catch { /* This capture has no packages of that kind. */ }
  }
  if (file.startsWith(stage+sep) && relative(stage,file).split(sep).includes('node_modules')) {
    try { return resolve(JSON.parse(readFileSync(resolve(stage,'__exact_paths.json'),'utf8')).app,relative(stage,file)); }
    catch { /* An unstaged fixture already names its source. */ }
  }
  return file;
}

/** The names a resolution of `source` from `importer` would have chosen, wherever it searched: the specifier with
 * each extension and extension alias of the bundle's `moduleResolution`, as a directory (`package.json`, `index.*`),
 * a bare name in each `node_modules` up the chain, and a package's main fields. Each is recorded present, with its
 * digest, or absent, so a file that would now win makes the build stale and an unrelated one does not. */
export function resolutionCandidates(source, importer, target, packageJsonPath, {base, paths = {}} = {}) {
  const {extensions, extensionAlias, mainFields} = moduleResolution, names = new Set();
  const candidate = path => {
    names.add(path);
    const ext = extname(path);
    for (const alias of extensionAlias[ext] ?? []) names.add(path.slice(0, -ext.length) + alias);
    for (const extension of extensions) names.add(path + extension);
    names.add(resolve(path, 'package.json'));
    for (const extension of extensions) names.add(resolve(path, 'index' + extension));
  };
  const from = dirname(importer);
  if (source.startsWith('.') || isAbsolute(source)) {
    candidate(resolve(from, source));
  } else if (!source.startsWith('\0') && !source.startsWith('node:')) {
    // The checker may stop at a declaration in an earlier alias target while the bundle falls back to another.
    const alias = Object.keys(paths).find(key => key === source) ?? Object.keys(paths).filter(key => {
      const star = key.indexOf('*');
      return star >= 0 && source.length >= key.length - 1 && source.startsWith(key.slice(0,star)) && source.endsWith(key.slice(star+1));
    }).sort((a,b) => b.indexOf('*') - a.indexOf('*') || b.length - a.length)[0];
    if (alias) {
      const star = alias.indexOf('*'), matched = star < 0 ? '' : source.slice(star,source.length-(alias.length-star-1));
      for (const value of paths[alias]) candidate(resolve(base,value.replace('*',matched)));
    }
    const name = source.split('/').slice(0, source.startsWith('@') ? 2 : 1).join('/');
    for (let at = from;; at = dirname(at)) {
      const modules = resolve(at, 'node_modules');
      names.add(resolve(modules, name));
      names.add(resolve(modules, name, 'package.json'));
      if (source !== name) candidate(resolve(modules, source));
      if (at === dirname(at)) break;
    }
  }
  if (target && isAbsolute(target)) names.add(target);
  if (packageJsonPath) {
    // Rolldown reports the selected entry, but a missing earlier main field can win after a file is added.
    const manifest = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
    names.add(packageJsonPath);
    for (const field of mainFields) if (typeof manifest[field] === 'string') candidate(resolve(dirname(packageJsonPath), manifest[field]));
  }
  return [...names];
}

/** Use Rolldown's dependency order after TypeScript erasure, independent of concurrent resolver calls. */
export function orderImportEdges(edges, modules) {
  const positions = new Map([...modules].map(([file, imports]) =>
    [file, new Map([...new Set(imports)].map((id, index) => [id, index]))]));
  return edges.sort(([a,x,k,p],[b,y,l,q]) => a.localeCompare(b)
    || (positions.get(a)?.get(p) ?? Infinity) - (positions.get(b)?.get(q) ?? Infinity)
    || x.localeCompare(y) || k.localeCompare(l));
}

/** Record build inputs, mapping the captured layout back to its sources. */
export function recordSourceInputs(stage, field, files, edges) {
  const {app,mounts=[],privateInputs=false} = existsSync(resolve(stage,'__exact_paths.json')) ? JSON.parse(readFileSync(resolve(stage,'__exact_paths.json'),'utf8')) : {app:stage};
  const roots = [...mounts.map(([name,path]) => [resolve(stage,name),path]),[stage,app]];
  const reads = new Map();
  let declarations = {};
  if (field === 'types' || field === 'resolution') {
    try { declarations = JSON.parse(readFileSync(resolve(stage,'__exact_declarations.json'),'utf8')).digests ?? {}; } catch { /* no staged declarations */ }
  }
  const paths = files.map(input => {
    const [path,original] = Array.isArray(input) ? input : [input,null];
    if (!path) return null;
    const keep = (file,from) => {
      if (!from) return null;
      // Declaration capture deliberately omits implementation sources. Such a checker probe is absent only
      // in the stage; the source's presence and bytes are still resolution inputs.
      let captured; try { captured = statSync(path); } catch { /* absent, or under a file (ENOTDIR) */ }
      const entry = inputEntry(from, captured?.isFile() ? path : from, declarations[from]);
      reads.set(from, entry);
      // Absent names and directories are freshness entries (`reads`) only; the lists name files read.
      return entry.kind === 'file' ? [file,from] : null;
    };
    if (path !== stage && !path.startsWith(stage+sep)) return field !== 'modules' ? keep(path,original ?? path) : null;
    const file = relative(stage,path).split(sep).join('/');
    if (file.startsWith('__exact_') || file === 'app.contract.d.ts') return null;
    if (file.split('/').includes('node_modules')) return field === 'types' || field === 'resolution' ? keep(file,original ?? packageOrigin(stage,path)) : null;
    if (field === 'modules' && /\.d\.[cm]?ts$/.test(file)) return null;
    const [at,from] = roots.find(([at]) => path === at || path.startsWith(at+sep));
    return keep(file,resolve(from,relative(at,path)));
  }).filter(Boolean).sort(([a],[b]) => a.localeCompare(b));
  const path = resolve(stage,'__exact_inputs.json');
  let record = {};
  try { record = JSON.parse(readFileSync(path,'utf8')); } catch { /* first phase */ }
  record[field] = field === 'resolution' ? [...new Map([...(record[field] ?? []),...paths])] : paths;
  if (edges) record.edges = edges;
  record.reads = [...new Map([...reads, ...(record.reads ?? []).map(entry => [entry.path,entry])]).values()].sort((a,b) => a.path.localeCompare(b.path));
  writeFileSync(path,JSON.stringify(record));
  if (app !== stage && !privateInputs) writeSourceInputs(app, record);
}

/** The same discovery record for web, Cargo and Apple builds, including a reused bake. */
export function writeSourceInputs(app, inputs) {
  if (!inputs) return;
  const dir = resolve(app, '.exact'), path = resolve(dir, 'typescript-inputs.json'), bytes = JSON.stringify(inputs);
  mkdirSync(dir, {recursive:true});
  if (existsSync(path) && readFileSync(path,'utf8') === bytes) return;
  const pending = `${path}.${process.pid}.tmp`;
  writeFileSync(pending, bytes);
  renameSync(pending, path);
}
const bytesDigest = bytes => createHash('sha256').update(bytes).digest('hex');
// A directory's presence only: the names inside it that a resolution would choose are entries of their own.
const inputEntry = (path, captured = path, hash) => {
  try {
    return statSync(captured).isDirectory() ? {kind:'directory',path} : {kind:'file',path,digest:hash ?? bytesDigest(readFileSync(captured))};
  } catch { return {kind:'absent',path}; }
};
export const sourceInputChanges = inputs => (inputs?.reads ?? []).filter(entry => {
  const now = inputEntry(entry.path);
  return now.kind !== entry.kind || now.digest !== entry.digest;
}).map(entry => entry.path);

/** TypeScript's resolver reports successful and failed file probes separately from its loaded source list. */
export function checkerOutput(stdout) {
  const lines = String(stdout ?? '').split(/\r?\n/);
  const source = line => isAbsolute(line) && /\.[cm]?[jt]sx?$/.test(line);
  // TypeScript spells Windows paths with `/`; the capture uses native paths.
  const files = lines.filter(source).map(normalize);
  const probes = lines.flatMap(line => Array.from(line.matchAll(/(?:File |file |Directory |directory |at )'([^']+)'/g), match => match[1]));
  const resolution = new Set(probes.filter(isAbsolute).map(normalize));
  for (const line of lines) {
    const imported = /Resolving (?:module|type reference directive) '([^']+)'(?: from |, containing file )'([^']+)'/.exec(line);
    if (imported) for (const path of resolutionCandidates(imported[1], normalize(imported[2]))) resolution.add(path);
  }
  const diagnostics = (lines.some(line => /error TS\d+:/.test(line)) ? lines.filter(line => /error TS\d+:|^\s+\S/.test(line)) : lines.filter(line => !source(line))).join('\n');
  return {files,resolution:[...resolution],diagnostics};
}

/** The app-local code already in a producer's stage. Package code is captured separately by `capturedSources`. */
export function settingSources(stage) {
  const files = new Map();
  const walk = dir => {
    for (const entry of readdirSync(dir, {withFileTypes:true})) {
      if (entry.name === 'node_modules' || dir === stage && entry.name.startsWith('__exact_')) continue;
      const path = resolve(dir, entry.name), file = relative(stage, path).split(sep).join('/');
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile() && /\.(?:[cm]?[jt]sx?|json)$/.test(file) && file !== 'app.js') files.set(file, readFileSync(path, 'utf8'));
    }
  };
  walk(stage);
  return files;
}

const WRAPPERS = ['TSNonNullExpression', 'ParenthesizedExpression', 'ChainExpression', 'TSAsExpression', 'TSSatisfiesExpression', 'TSTypeAssertion'];
const bare = node => { while (node && WRAPPERS.includes(node.type)) node = node.expression; return node; };
const key = node => { const p = bare(node.property); return !node.computed ? p?.name ?? null : p?.type === 'Literal' && typeof p.value === 'string' ? p.value : null; };
const metaEnv = node => node?.type === 'MemberExpression' && key(node) === 'env' && bare(node.object)?.type === 'MetaProperty' && bare(node.object).meta?.name === 'import';
const literal = node => node?.type === 'Literal' && typeof node.value === 'string' ? node.value
  : node?.type === 'TemplateLiteral' && !node.expressions.length ? node.quasis[0].value.cooked : null;
// Every `import.meta.env` in `ast`, and a position's `line:col` in `code`.
const metaEnvs = (ast) => {
  const found = [];
  const walk = node => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { node.forEach(walk); return; }
    if (metaEnv(node)) found.push(node);
    for (const k in node) if (k !== 'parent') walk(node[k]);
  };
  walk(ast);
  return found;
};
const lineColumn = code => offset => { const lines = code.slice(0, offset).split(/\r\n?|\n|\u2028|\u2029/); return `${lines.length}:${lines.at(-1).length + 1}`; };
const READ = 'import.meta.env is read only in a top-level const of an app-local module: const API = import.meta.env.API_BASE_URL, with ?? \'a default\' or without, then API where it is used';

/** The expression a read of `name`, which a launch may give (LLP 1153.002 D2), becomes in a native build's module: the
 * launch's value, which the stage's `__exact_launch.ts` asked for before any module's code ran (`typescript.mjs`
 * `launchModule`), else `value`, the build's. Each reading file imports it as `local` on a line of its own after its last, so no
 * line moves. */
const launchRead = (local, name, value) => `(${local}[${JSON.stringify(name)}] ?? ${JSON.stringify(value)})`;
// Follow the grant's bindings through app-local imports without evaluating module code (LLP 1153.002 D3).
const constsRead = (modules, name, resolveImport) => {
  const seen = new Map(), exportsSeen = new Set();
  const exported = (file, name) => {
    const module = modules.get(file);
    if (!module || exportsSeen.has(file + ':' + name)) return;
    exportsSeen.add(file + ':' + name);
    for (const s of module.body) {
      if (s.exportKind === 'type') continue;
      if (s.type === 'ExportDefaultDeclaration' && name === 'default') walk(file, s.declaration);
      if (s.type === 'ExportNamedDeclaration') {
        if (s.declaration?.declarations?.some(d => d.id.name === name)) binding(file, name);
        for (const x of s.specifiers ?? []) if (x.exportKind !== 'type' && (x.exported.name ?? x.exported.value) === name) {
          if (s.source) exported(resolveImport(file, s.source.value), x.local.name ?? x.local.value);
          else binding(file, x.local.name ?? x.local.value);
        }
      }
      if (s.type === 'ExportAllDeclaration') exported(resolveImport(file, s.source.value), s.exported ? '*' : name);
    }
  };
  const binding = (file, name) => {
    const module = modules.get(file);
    if (!module || seen.get(file)?.has(name)) return;
    if (!seen.has(file)) seen.set(file, new Set());
    seen.get(file).add(name);
    for (const s of module.body) {
      const declaration = s.type === 'ExportNamedDeclaration' ? s.declaration : s;
      if (declaration?.type === 'VariableDeclaration') for (const d of declaration.declarations) if (d.id.name === name) walk(file, d.init);
      if (s.type === 'ImportDeclaration' && s.importKind !== 'type') for (const x of s.specifiers) {
        if (x.local.name !== name || x.importKind === 'type') continue;
        exported(resolveImport(file, s.source.value), x.type === 'ImportDefaultSpecifier' ? 'default' : x.imported?.name ?? x.imported?.value ?? '*');
      }
    }
  };
  const walk = (file, node) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) { node.forEach(n => walk(file, n)); return; }
    if (node.type === 'Identifier') { binding(file, node.name); return; }
    // A namespace import reads the selected export, rather than unrelated exports in that file.
    if (node.type === 'MemberExpression' && bare(node.object)?.type === 'Identifier' && key(node) !== null) {
      const imported = modules.get(file).body.find(s => s.type === 'ImportDeclaration' && s.specifiers.some(x => x.type === 'ImportNamespaceSpecifier' && x.local.name === bare(node.object).name));
      if (imported) { exported(resolveImport(file, imported.source.value), key(node)); return; }
    }
    for (const k in node) if (k !== 'parent' && !(k === 'property' && node.type === 'MemberExpression' && !node.computed) && !(k === 'key' && node.type === 'Property' && !node.computed)) walk(file, node[k]);
  };
  binding('app.ts', name);
  const init = modules.get('app.ts')?.body.flatMap(s => (s.type === 'ExportNamedDeclaration' ? s.declaration : s)?.declarations ?? []).find(d => d.id.name === name)?.init;
  return { seen, init };
};

// The local name of the binding app.ts exports as `grants`, as `staticFacts` reads it.
const grantsBinding = body => {
  for (const statement of body) {
    if (statement.type !== 'ExportNamedDeclaration' || statement.source) continue;
    if (statement.declaration?.declarations?.some(d => d.id.name === 'grants')) return 'grants';
    const named = (statement.specifiers ?? []).find(x => (x.exported.name ?? x.exported.value) === 'grants');
    if (named) return named.local.name ?? named.local.value;
  }
  return null;
};

/** Rewrite top-level setting consts in app.ts's transitive app-local imports and return app.ts's code
 * and each setting's value and source. The manifest declares every name, all reads share its default, and a
 * declared name must be read in this graph. A launch-marked name reads the launch's value before the build's
 * (LLP 1153.002); grants may not depend on it, including through imports. With `launches: false`, the web inlines
 * the build's value and still records `launch: true`. `parse` is Rolldown's parser. */
export function inlineSettings(code, app, parse, env = process.env, { launches = true, modules = new Map([['app.ts', code]]), resolveImport = () => null, syntaxError = false } = {}) {
  const entries = manifestEnv(app), declared = settingNames(entries), launch = launchNames(entries);
  const preloads = manifestOf(app).preload ?? {};
  const refusals = declared.filter(name => typeof name !== 'string' || !NAME.test(name)).map(name => typeof name === 'object' && name !== null
    ? `app.json's "env": ${JSON.stringify(name)} is not a setting: write its name, or { "name": "API_BASE_URL", "launch": true } for one a launch may give`
    : `app.json's "env": ${JSON.stringify(name)} is not a setting's name (import.meta.env names are [A-Z][A-Z0-9_]*)`);
  for (const name of launch) if (name.startsWith('EXACT_')) refusals.push(`app.json's "env": ${name} is marked "launch": true, and a name that begins with EXACT_ is the host's: rename the setting`);
  for (const name of launch) if (entries.some(entry => entry === name || entryName(entry) === name && entry.launch !== true)) refusals.push(`app.json's "env" names ${name} twice, once marked "launch": true: name it once`);
  const reads = [], parsed = new Map();
  for (const [file, source] of modules) {
    if (file.endsWith('.json') || /\.d\.[cm]?ts$/.test(file)) continue;
    const result = parse(file, source), allowed = new Set(), at = lineColumn(source);
    // The type check says what does not parse.
    if (result.errors?.length) { syntaxError = true; continue; }
    parsed.set(file, { body: result.program.body });
    for (const statement of result.program.body) {
      const s = statement.type === 'ExportNamedDeclaration' ? statement.declaration : statement;
      if (s?.type !== 'VariableDeclaration' || s.kind !== 'const') continue;
      for (const d of s.declarations) {
        const init = bare(d.init), fallback = init?.type === 'LogicalExpression' && init.operator === '??' ? bare(init.right) : null;
        const read = fallback ? bare(init.left) : init;
        if (d.id.type !== 'Identifier' || read?.type !== 'MemberExpression' || !metaEnv(bare(read.object)) || key(read) === null) continue;
        allowed.add(bare(read.object).start);
        const wrappers = [];
        for (let node = d.init; node !== init; node = node.expression) wrappers.push(node);
        const typed = wrappers.some(node => ['TSAsExpression', 'TSSatisfiesExpression', 'TSTypeAssertion'].includes(node.type));
        reads.push({ file, name: key(read), id: d.id.name, node: init, wrappers, widen: !d.id.typeAnnotation && !typed, fallback, at: at(read.start) });
      }
    }
    for (const node of metaEnvs(result.program)) if (!allowed.has(node.start)) refusals.push(`${file}:${at(node.start)}: ${READ}`);
  }
  // Grants are the build's (LLP 1153.002 D3): over a marked name's const they would hold the build's value, which a
  // launch replaces. Keyed grants select by `const origin`.
  const grants = grantsBinding(parsed.get('app.ts')?.body ?? []), dependencies = grants ? constsRead(parsed, grants, resolveImport) : null;
  for (const read of reads) {
    if (!launch.has(read.name) || !dependencies || !(dependencies.seen.get(read.file)?.has(read.id) || read.file === 'app.ts' && read.id === 'origin' && bare(dependencies.init)?.type === 'ObjectExpression')) continue;
    refusals.push(`${read.file}:${read.at}: grants read ${read.id}, and import.meta.env.${read.name} is one a launch may give ("launch": true in app.json), while grants are fixed at build: grant what a launch may name (net.fetch https:, net.fetch http:, or each origin it may give) without ${read.id}`);
  }
  // Read twice, a name takes the one default: else the record would say one value and the module hold two.
  const said = (fallback) => (fallback ? JSON.stringify(literal(fallback)) : 'no default'), first = new Map();
  for (const read of reads) {
    const seen = first.get(read.name);
    if (!seen) first.set(read.name, read);
    else if (said(seen.fallback) !== said(read.fallback)) refusals.push(`${read.file}:${read.at}: import.meta.env.${read.name} is read again with ${said(read.fallback)}, and at ${seen.file}:${seen.at} with ${said(seen.fallback)}: read it once, or give every read the same default`);
  }
  for (const { file, name, at } of reads) if (!declared.includes(name)) refusals.push(`${file}:${at}: import.meta.env.${name} is not declared: add "${name}" to app.json's "env" (every declared value is public: it ships in the module on every host)`);
  for (const name of declared) if (!syntaxError && NAME.test(name) && !reads.some(r => r.name === name)) refusals.push(`import.meta.env.${name}: app.json's "env" declares it, and app.ts and its app-local imports read it nowhere: read it as const X = import.meta.env.${name}, or remove it from "env"`);
  const values = settingValues(app, declared.filter(name => reads.some(r => r.name === name)), env), record = {};
  for (const [file, source] of modules) {
    const edits = [];
    // This file's name for the launch's values: one its text never spells, so it neither shadows nor is shadowed.
    let local = '__exact_launch';
    while (source.includes(local)) local += '_';
    for (const { name, node, wrappers, widen, fallback, at } of reads.filter(read => read.file === file)) {
      if (!declared.includes(name)) continue;
      const given = fallback ? literal(fallback) : null;
      if (fallback && given === null) { refusals.push(`${file}:${at}: import.meta.env.${name}'s default after ?? must be a string literal`); continue; }
      const setting = values[name] ?? (fallback ? { value: given, source: 'default' } : null);
      if (!setting) { if (!refusals.some(r => r.startsWith(`import.meta.env.${name} has no value`))) refusals.push(`import.meta.env.${name} has no value: set it in the environment, in .env.local or in .env beside app.json (read in that order), or write a default: import.meta.env.${name} ?? '…'`); continue; }
      record[name] ??= launch.has(name) ? { ...setting, launch: true } : setting;
      // Widen only inferred reads: authored types check the selected literal themselves.
      const selected = JSON.stringify(setting.value);
      const value = !widen ? selected : /\.[cm]?tsx?$/.test(file) ? `(${selected} as string)` : `/** @type {string} */ (${selected})`;
      const launched = launches && launch.has(name);
      edits.push([node.start, node.end, launched ? launchRead(local, name, setting.value) : value]);
      // A launch may give any string: remove literal checks, keeping the other wrappers around the read.
      if (launched) for (const wrapper of wrappers) {
        if (wrapper.type !== 'TSSatisfiesExpression' && wrapper.typeAnnotation?.typeName?.name !== 'const') continue;
        edits.push([wrapper.start, wrapper.expression.start, ''], [wrapper.expression.end, wrapper.end, '']);
      }
    }
    code = source;
    for (const [start, end, value] of edits.sort((a, b) => b[0] - a[0])) code = code.slice(0, start) + value + code.slice(end);
    if (launches && reads.some(read => read.file === file && launch.has(read.name))) {
      const path = './' + relative(dirname(file), '__exact_launch').split(sep).join('/');
      code += `\nimport { launched as ${local} } from '${path}';\n`;
    }
    modules.set(file, code);
  }
  for (const [preload, field, name] of preloadSettings(preloads)) {
    if (!declared.includes(name)) refusals.push(`preload.${preload}.${field}: setting ${name} is not declared in app.json's env`);
    else if (!syntaxError && typeof record[name]?.value !== 'string') refusals.push(`preload.${preload}.${field}: setting ${name} has no build value or ?? default`);
  }
  if (refusals.length) throw new Error(refusals.join('\n'));
  return { code: modules.get('app.ts'), record, reads: reads.map(({file,name}) => ({file,name})) };
}

function* preloadSettings(preloads) {
  for (const [preload, entry] of Object.entries(preloads)) {
    const fields = [['url', entry.url], ['body', entry.body], ...Object.entries(entry.headers ?? {}).map(([key, value]) => [`headers.${key}`, value])];
    for (const [field, text] of fields) if (typeof text === 'string') for (const reference of text.matchAll(/{{|{env:([^{}]*)}/g)) {
      if (reference[1] !== undefined) yield [preload, field, reference[1]];
    }
  }
}

/** A backend selection rewrites a code literal, not the setting the preload reads (LLP 1162.000 D2). */
export function checkPreloadBackend(app, record, previous, selection) {
  if (!selection || !previous || previous === selection.origin) return;
  const refusals = [];
  for (const [preload, field, name] of preloadSettings(manifestOf(app).preload ?? {})) {
    const value = record[name]?.value;
    let url; try { url = new URL(value); } catch { continue; }
    if (url.origin === previous && url.pathname === '/' && !url.search && !url.hash)
      refusals.push(`preload.${preload}.${field}: setting ${name} is ${value}, but ${selection.source} selects ${selection.origin} by rewriting app.ts's origin literal; the preload cannot use that setting with this selection`);
  }
  if (refusals.length) throw new Error(refusals.join('\n'));
}

/** Each `import.meta.env` in the captured module `file`, the app's or a package's, as `{ start, end, why }`: once
 * `inlineSettings` has run, app-local modules have none left, so any is a read where none is allowed. `where(':line:col')` names
 * the place, as the capture names it (`path:line:col`, or `name@version (path:line:col)` in a package). */
export function envReads(file, code, parse, where, inPackage = false) {
  if (!/\bimport\b/.test(code) || /\.d\.[cm]?ts$/.test(file) || !/\.[cm]?[jt]sx?$/.test(file)) return [];
  const lang = /\.[cm]?tsx?$/.test(file) ? (file.endsWith('x') ? 'tsx' : 'ts') : 'js', at = lineColumn(code);
  return metaEnvs(parse(code, { lang })).map(node => ({ start: node.start, end: node.end, why: inPackage
    ? `${where(`:${at(node.start)}`)} reads import.meta.env, which only app-local modules may: an app-local module reads the setting (const API = import.meta.env.API_BASE_URL) and passes it in`
    : `${where(`:${at(node.start)}`)}: ${READ}` }));
}
export const strayReads = (...args) => envReads(...args).map(read => read.why);

/** Each setting of `record` (name to the value and source a build inlined) whose value its sources give otherwise
 * now, as `{ name, built, now }` (a setting, or null where none is set). With `app`, the app's directory beside the
 * build, each is read again from `env` and the app's files; without it (a build copied away from them) only `env` can
 * change one, and a name it leaves unset keeps the build's value. `launch` is the environment the app is started with
 * where a launch gives a marked name's value (Apple, Linux, Windows; LLP 1153.002 D2): a value it gives one, empty or
 * not and however it came there (what Bun loaded from `.env` too), is the launch's (an empty one reads the build's),
 * not a source of the build, and the name is not counted. With `windows`, the app is a Windows process, which reads a
 * name in `launch` without regard to case (`childValue`). */
export function settingChanges(record, env = process.env, app = null, { launch = null, windows = false } = {}) {
  const read = app ? settingValues(app, Object.keys(record), env) : {}, bun = bunNames(env);
  return Object.entries(record).flatMap(([name, was]) => {
    if (launch && was?.launch && childValue(launch, name, windows) !== undefined) return [];
    const now = app ? read[name] ?? (was?.source === 'default' ? was : null)
      : env[name] !== undefined && !bun.has(name) ? { value: env[name], source: 'environment' } : was;
    return now?.value === was?.value ? [] : [{ name, built: was, now }];
  });
}

/** `env`'s value for `name` in a process started with it. Windows reads a name without regard to case, and of
 * entries that differ only in case the child gets the first in sort order, which is the one Node's `child_process`
 * passes (https://nodejs.org/api/child_process.html#child_processspawncommand-args-options, `options.env`). */
const childValue = (env, name, windows) => windows ? env[Object.keys(env).filter(key => key.toUpperCase() === name.toUpperCase()).sort()[0]] : env[name];

/** `settingChanges` for the build in `dist`, from what its `.exact-build.json` says it inlined (`env`). */
export function changedSettings(dist, env = process.env, app = null) {
  let built = {};
  try { built = JSON.parse(readFileSync(resolve(dist, '.exact-build.json'), 'utf8')).env ?? {}; } catch { /* no record: none inlined */ }
  return settingChanges(built, env, app);
}

/** What the bake in `baked` inlined (its receipt's `env`), as a native build's record keeps it beside the app's
 * directory `app`, together with its module and checker inputs. */
export function bakedSettings(baked, app) {
  let record = {};
  try { record = JSON.parse(readFileSync(resolve(baked, 'app.module.json'), 'utf8')); } catch { /* no TypeScript module */ }
  let typescriptSources;
  try { typescriptSources = JSON.parse(readFileSync(resolve(baked,'app.sources.json'),'utf8')); } catch { /* no TypeScript module */ }
  writeSourceInputs(app, typescriptSources);
  return { ...(record.env ? {settings:{dir:app,env:record.env}} : {}), ...(typescriptSources ? {typescriptSources} : {}) };
}

// A native build's settings (a Linux or Android executable, the Canvas reader's APK) are bound to the executable a
// successful build made: `<executable>.settings.json` holds its digest beside its backend and inlined settings. The bake's own
// receipt cannot stand in, as an interrupted rebuild writes a new one and leaves the old executable.
const digest = (path) => createHash('sha256').update(readFileSync(path)).digest('hex');

/** After `executable` of the app `app` (its directory and manifest) built: its backend and inlined settings,
 * bound to it by its digest. `baked` is the bake's output directory, where the build knows it; else the one the
 * executable's dep-info names (`<executable>.d`), whose bytecode it embeds. */
export function recordBuiltSettings(executable, app, baked = null) {
  if (!baked) {
    let line = '';
    try { line = readFileSync(`${executable}.d`, 'utf8').split('\n')[0]; } catch { /* no dep-info */ }
    const bytecode = line.slice(line.indexOf(': ') + 2).split(/(?<!\\) /).map((path) => path.replace(/\\ /g, ' ')).find((path) => /\/build\/[^/]+\/out\/app\.hbc$/.test(path));
    baked = bytecode ? dirname(bytecode) : null;
  }
  let backendOrigin = null;
  try { backendOrigin = JSON.parse(readFileSync(resolve(baked, 'app.module.json'), 'utf8')).backendOrigin ?? null; } catch { /* no TypeScript backend */ }
  const {settings,typescriptSources} = baked ? bakedSettings(baked, app.dir) : {};
  if (!app.manifest?.env?.length && !backendOrigin && !typescriptSources) return;
  writeFileSync(`${executable}.settings.json`, JSON.stringify({ sha256: digest(executable), env:settings?.env ?? {}, backendOrigin, typescriptSources }));
}

/** The backend and settings recorded for `executable`, or null where no record is bound to
 * it: none written, or one written for another build of it. */
export function boundSettings(executable) {
  try {
    const record = JSON.parse(readFileSync(`${executable}.settings.json`, 'utf8'));
    return record.sha256 === digest(executable) ? record : null;
  } catch { return null; }
}

/** A web build's settings and module/checker inputs: from the JS capture, or the paired native build. */
export function builtSettings(out) {
  let record = {}, sourceInputs;
  try { sourceInputs = JSON.parse(readFileSync(resolve(out,'.gen/typescript/__exact_inputs.json'),'utf8')); } catch { /* native receipt below */ }
  try { record = JSON.parse(readFileSync(resolve(out, '.gen/typescript/__exact_env.json'), 'utf8')); } catch {
    try {
      const receipt = JSON.parse(readFileSync(resolve(out, 'app.module.json'), 'utf8'));
      record = receipt.env ?? {};
      sourceInputs = JSON.parse(readFileSync(resolve(out,'app.sources.json'),'utf8'));
    } catch { /* no data module */ }
  }
  return { ...(Object.keys(record).length ? { env: record } : {}), ...(sourceInputs ? {sourceInputs} : {}) };
}

/** What the agent's `logs` and the build say of each setting a build inlined: `import.meta.env.NAME = "value" (source)`,
 * and of one a launch may give, that a launch's value replaces it natively. */
export const settingLines = (record) => Object.entries(record ?? {}).map(([name, { value, source, launch }]) => `import.meta.env.${name} = ${JSON.stringify(value)} (${source}${launch ? '; a launch that sets it replaces it, except on Android and the web' : ''})`);

/** The values in a web build's marker, as a drive reports them. */
export const builtSettingLines = dist => { try { return settingLines(JSON.parse(readFileSync(resolve(dist, '.exact-build.json'), 'utf8')).env); } catch { return []; } };
export const isSettingInput = key => key.startsWith('import.meta.env.');

/** The settings the native build `bin` of `app` inlined (LLP 1153 D4) whose sources give them otherwise now, as
 * `import.meta.env.NAME`: a change no file shows, which only a rebuild takes in. What it inlined is the record bound to
 * this executable (`boundSettings`); an app that declares settings and has none bound, an executable another build
 * made, has its record named as the change, so the drive says to build again. With `launch`, the environment a drive
 * starts a Linux executable with, a marked name it sets is the launch's value, not a change (LLP 1153.002 D2); an
 * Android build reads the build's value, so it counts there. */
export function bakedSettingChanges(bin, app, { launch = null } = {}) {
  if (!app.manifest?.env?.length) return [];
  const record = boundSettings(bin);
  if (!record) return [`${bin}.settings.json (none, or another build's)`];
  return settingChanges(record.env, process.env, app.dir, { launch }).map(({ name }) => `import.meta.env.${name}`);
}
