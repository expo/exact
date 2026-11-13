import { existsSync, readFileSync, statSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { within } from './paths.mjs';

// What app.ts reaches (LLP 1116.003 D1.1). The bake captures every TypeScript
// and JSON file but type-checks and bundles from the entry alone (its tsconfig's
// `files` is `__exact_entry.ts`, which imports app.ts: js/bake/src/typescript.mjs),
// so a file app.ts never imports — a backend the app reaches only over HTTP
// (round 5's book-club `server/index.ts`), a script — is no build's input. Read
// by scanning each source's module specifiers (type-only imports and references
// included) and taking every file a relative one can resolve to as the bake's
// resolution would, a `typescript.sources` mount as if beside app.ts under its
// name: over-inclusion costs only a rebuild. Null (every TypeScript file counts)
// without an app.ts, or when tsconfig maps bare specifiers (`paths`, `baseUrl`).
const SPECIFIER = /(?:\bfrom\s*|\bimport\s*\(?\s*|\brequire\s*\(\s*|<reference\s+path\s*=\s*)(['"])([^'"\n]+)\1/g;
export const MODULE_FILE = /\.(tsx?|[cm]?js|json)$/;
export const CONFIG_JSON = /^(app|package|jsconfig|tsconfig[^/]*)\.json$/;
/** The declarations the bake writes beside app.ts: its output, never an input. */
export const DECLARATIONS = 'app.contract.d.ts';
/** The files app.ts imports, transitively (and itself), by path; or null. */
export function moduleGraph(dir, manifest) {
  const entry = resolve(dir, 'app.ts');
  if (!existsSync(entry)) return null;
  try { if (/"(paths|baseUrl)"\s*:/.test(readFileSync(resolve(dir, 'tsconfig.json'), 'utf8'))) return null; } catch {}
  const mounts = Object.entries(manifest?.typescript?.sources ?? {}).map(([name, path]) => [resolve(dir, name), resolve(dir, String(path))]);
  const disk = path => { for (const [at, real] of mounts) if (within(at, path)) return real + path.slice(at.length); return path; };
  const graph = new Set(), seen = new Set(), queue = [entry];
  while (queue.length) {
    const at = queue.pop(), file = disk(at);
    let isFile = false; try { isFile = statSync(file).isFile(); } catch {}
    if (seen.has(at) || !isFile) continue;
    seen.add(at); graph.add(file);
    if (!/\.(tsx?|[cm]?jsx?)$/.test(file)) continue;
    for (const [, , spec] of readFileSync(file, 'utf8').matchAll(SPECIFIER)) {
      if (!spec.startsWith('.')) continue;
      const base = resolve(dirname(at), spec), stem = base.replace(/\.(m?js|jsx)$/, '');
      // Each file the bundler or TypeScript may resolve it to: an extension added, an index file, a `.ts` its `.js` names.
      const ends = ['.ts', '.tsx', '.d.ts', '.js', '.jsx', '.mjs', '.cjs', '.json'];
      queue.push(base, ...ends.map(end => base + end), ...ends.map(end => resolve(base, 'index' + end)), ...['.ts', '.tsx', '.mts', '.d.ts'].map(end => stem + end));
    }
  }
  return graph;
}
