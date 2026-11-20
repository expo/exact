// App-local source capture and the modules the app's server imports.
import { readFileSync, readdirSync, realpathSync, statSync } from 'node:fs';
import { relative, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { fileIdentity, resourceIdentities } from '../host/apple/assets.mjs';
import { beside as besideDist } from '../host/web/beside.mjs';
import { inlineSettings } from '../js/bake/src/__exact_env.mjs';
import { recordedConstants } from '../js/bake/src/__exact_facts.mjs';

const readIn = (dir, path) => { try { return readFileSync(resolve(dir, path), 'utf8'); } catch { return null; } };

/** The web capture's app-local sources, keyed by their path in the stage. The native bake's `sources`
 * (js/bake/src/lib.rs) keeps the same boundary; packages are captured separately. */
export function appSourceFiles(dir, { manifest = JSON.parse(readIn(dir, 'app.json') ?? '{}'), mounts, output = null } = {}) {
  mounts ??= Object.entries(manifest.typescript?.sources ?? {}).map(([name, path]) => [name, realpathSync(resolve(dir, path))]);
  const resources = resourceIdentities(dir, manifest), files = new Map();
  const capture = (from, prefix, top) => {
    for (const entry of readdirSync(from, { withFileTypes: true })) {
      const name = entry.name, path = resolve(from, name), file = prefix ? `${prefix}/${name}` : name;
      if (resources.size && resources.has(fileIdentity(path))) continue;
      if (['.git', 'node_modules', 'target', 'dist'].includes(name) || name.startsWith('.exact-js-bake-') || (top && name === 'app.contract.d.ts')) continue;
      if (name.startsWith('.') && entry.isDirectory()) continue;
      if (top && (name === 'dist-windows' || besideDist(name))) continue;
      if (top && mounts.some(([mount]) => mount === name)) continue;
      if (entry.isDirectory() && name.endsWith('.xcframework') && /^(modules|(?:ios|macos|tvos)\/modules)\//.test(file)) continue;
      // Document links such as CLAUDE.md → AGENTS.md are never read by a build.
      if (entry.isSymbolicLink()) {
        const document = /\.(md|txt)$/.test(name) && !/^(assets|deck|shaders|gpu)$/.test(file.split('/')[0]);
        if (!document || statSync(path, { throwIfNoEntry: false })?.isDirectory()) throw new Error(`source links are not captured: ${path}`);
        continue;
      }
      if (entry.isDirectory()) { if (realpathSync(path) !== output) capture(path, file, false); }
      else if (/\.(ts|[cm]?js|json)$/.test(name)) files.set(file, path);
    }
  };
  capture(dir, '', true);
  for (const [name, path] of mounts) capture(path, name, false);
  return files;
}

/** The latest build's module and checker inputs. Before a first build there is no graph to reuse. */
export function appSourceInputs(dir) {
  const real = path => { try { return realpathSync.native(path); } catch { return resolve(path); } };
  try {
    const inputs = JSON.parse(readFileSync(resolve(dir,'.exact/typescript-inputs.json'),'utf8'));
    return inputs.modules?.some(([file,path]) => file === 'app.ts' && real(path) === real(resolve(dir,'app.ts'))) ? inputs : null;
  } catch { return null; }
}

/** The app-local modules Rolldown loaded, and the union with the type check's loaded files. */
export function appModules(dir, inputs = appSourceInputs(dir)) {
  const files = new Map([...(inputs?.modules ?? []),...(inputs?.types ?? []),...(inputs?.resolution ?? [])].filter(([,path]) => path));
  const modules = new Map((inputs?.modules ?? []).flatMap(([file,path]) => {
    try { return [[file,readFileSync(path,'utf8')]]; } catch { return []; }
  }));
  const edges = new Map((inputs?.edges ?? []).map(([file,source,kind,next]) => [[file,source,kind].join('\0'),next]));
  const resolveImport = (file,source,kind='import') => edges.get([file,source,kind].join('\0')) ?? null;
  return {modules,resolveImport,files};
}

/** Settings from the build's module list, without native launch reads in the discovery text, and a `load` that
 * folds grants over the same graph (`recordedConstants`). */
export function appSettings(dir, env = process.env, inputs = appSourceInputs(dir)) {
  const { modules, resolveImport } = appModules(dir,inputs), authored = new Map(modules);
  const parse = createRequire(import.meta.url)('rolldown/utils').parseSync;
  const load = recordedConstants(resolveImport, file => modules.get(file), file => authored.get(file));
  return { ...inlineSettings(modules.get('app.ts') ?? '', dir, parse, env, { modules, resolveImport, launches:false }), modules, load };
}

/** The modules of the app's own that `path` imports (`from './x'`, `import './x'`, `import('./x')`, `require('./x')`),
 * as paths in `dir` that exist: `./x` is x, x.ts, x.js, x.mjs or x/index.ts. */
export function importsOf(dir, path) {
  const found = [], base = resolve(dir, path, '..');
  for (const [, spec] of (readIn(dir, path) ?? '').matchAll(/(?:\bfrom\s*|\bimport\s*\(?\s*|\brequire\(\s*)['"](\.{1,2}\/[^'"]+)['"]/g)) {
    const at = resolve(base, spec);
    const file = [at, `${at}.ts`, `${at}.js`, `${at}.mjs`, `${at}/index.ts`].find((p) => readIn(dir, p) != null);
    if (file && !relative(dir, file).startsWith('..')) found.push(relative(dir, file));
  }
  return found;
}

/** Whether `entry`, or a module of the app's own it imports (a root shim that only imports the backend), reads the
 * database's path from `DB`: `process.env.DB`, `Bun.env.DB`, `env["DB"]`, `const { DB } = process.env`, and a server in
 * another language's (`os.environ["DB"]`, `os.getenv("DB")`, `ENV["DB"]`, Go's `os.Getenv("DB")`). */
const READS_DB = /\benv\??\.DB\b|\benv\[\s*['"]DB['"]\s*\]|\{[^}]*\bDB\b[^}]*\}\s*=\s*[\w.]*\benv\b|\b(?:environ|ENV)\s*\[\s*['"]DB['"]|\b(?:environ\.get|[Gg]etenv)\(\s*['"]DB['"]/;
export function readsDB(dir, entry) {
  const seen = new Set([entry]);
  for (const path of seen) {
    if (READS_DB.test(readIn(dir, path) ?? '')) return true;
    for (const next of importsOf(dir, path)) if (seen.size < 40) seen.add(next);
  }
  return false;
}

/** A server's entry and the modules of the app's own it imports, transitively (at most 40). */
export function serverModules(dir, entry) {
  const seen = new Set([entry]);
  for (const path of seen) for (const next of importsOf(dir, path)) if (seen.size < 40) seen.add(next);
  return seen;
}
