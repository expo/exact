// Stand-ins for the JS runtime's modules a test does not load. Each exports every name the test's real modules take
// from it, read from their source, so a name the runtime comes to import cannot leave a stand-in short ("export
// 'svgGeometry' not found") and fail every test in the file.
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { localImports } from '../../web-js/glue.mjs';

const WEB_JS = resolve(import.meta.dirname, '../../web-js');
// Names whose do-nothing function would break their caller: `router.js` spreads `launchVisits()`'s list, and
// mount.js iterates `checkpointFaults()`'s.
const VALUES = { 'navigation.js': { launchVisits: '() => []' }, 'faults.js': { checkpointFaults: '() => []' } };

/** `file` of host/web-js as a stage holds it: its imports of the web host's modules (`../web/…`) point beside it, where
 * the stage puts each as it does a sibling, real or a stand-in. A module that only forwards the web host's module of
 * its own name is that module, as the build copies it. */
export function runtimeSource(file) {
  const source = readFileSync(resolve(WEB_JS, file), 'utf8');
  const forwarded = new RegExp(`^export \\* from (['"])\\.\\./web/${file}\\1;?$`).test(source.trim());
  return localImports(forwarded ? readFileSync(resolve(WEB_JS, '../web', file), 'utf8') : source);
}

/** Each binding `file` of host/web-js imports or re-exports from a sibling module: the name the module exports, the
 * name `file` binds it to (null for a re-export, which binds none), and the module. */
export function imports(file) {
  const out = [];
  for (const [, verb, names, from] of runtimeSource(file).matchAll(/(import|export)\s*\{([^}]*)\}\s*from\s*['"]\.\/([\w-]+\.js)['"]/g)) {
    for (const [name, local = name] of names.split(',').map(n => n.trim().split(/\s+as\s+/)).filter(([n]) => n)) out.push({ name, local: verb === 'import' ? local : null, from });
  }
  return out;
}

/** Each module the `real` files of host/web-js import from and `real` does not hold, as a stand-in's source. A name
 * is a function that does nothing, so a class can extend it too (rt.js extends shape.js's `Refusal`); `given` maps a
 * module to names whose values it states as source text, exported whether or not `real` imports them. */
export function standIns(real, given = {}) {
  const taken = new Map(Object.keys(given).map(file => [file, new Set()]));
  for (const file of real) {
    for (const { name, from } of imports(file)) if (!real.includes(from)) taken.set(from, (taken.get(from) ?? new Set()).add(name));
    // A module imported for its side effects alone (native.js's `import './native-glue.js'`): an empty stand-in.
    for (const [, from] of runtimeSource(file).matchAll(/import\s*['"]\.\/([\w-]+\.js)['"]/g)) if (!real.includes(from) && !taken.has(from)) taken.set(from, new Set());
  }
  return new Map([...taken].map(([file, names]) => {
    const stated = { ...VALUES[file], ...given[file] };
    const lines = Object.entries(stated).map(([n, value]) => `export const ${n} = ${value};`);
    for (const n of names) if (!(n in stated)) lines.push(`export function ${n}() {}`);
    return [file, lines.join('\n')];
  }));
}

/** The `real` files of host/web-js in `dir`, each as `runtimeSource` gives it, beside stand-ins for what they import
 * and what the files `own` import, which the caller writes itself. */
export function stageRuntime(dir, real, given = {}, own = []) {
  for (const [file, source] of standIns([...real, ...own], given)) writeFileSync(resolve(dir, file), source);
  for (const file of real) writeFileSync(resolve(dir, file), runtimeSource(file));
}
