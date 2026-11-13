// The modules of an app's own that one of its files imports, and what the app's server reads through them.
import { readFileSync } from 'node:fs';
import { relative, resolve } from 'node:path';

const readIn = (dir, path) => { try { return readFileSync(resolve(dir, path), 'utf8'); } catch { return null; } };

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
