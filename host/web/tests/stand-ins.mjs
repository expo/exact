// Stand-ins for the JS runtime's modules a test does not load. Each exports every name the test's real modules take
// from it, read from their source, so a name the runtime comes to import cannot leave a stand-in short ("export
// 'svgGeometry' not found") and fail every test in the file.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const WEB_JS = resolve(import.meta.dirname, '../../web-js');

/** Each binding `file` of host/web-js imports or re-exports from a sibling module: the name the module exports, the
 * name `file` binds it to (null for a re-export, which binds none), and the module. */
export function imports(file) {
  const out = [];
  for (const [, verb, names, from] of readFileSync(resolve(WEB_JS, file), 'utf8').matchAll(/(import|export)\s*\{([^}]*)\}\s*from\s*['"]\.\/([\w-]+\.js)['"]/g)) {
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
    for (const [, from] of readFileSync(resolve(WEB_JS, file), 'utf8').matchAll(/import\s*['"]\.\/([\w-]+\.js)['"]/g)) if (!real.includes(from) && !taken.has(from)) taken.set(from, new Set());
  }
  return new Map([...taken].map(([file, names]) => {
    const stated = given[file] ?? {};
    const lines = Object.entries(stated).map(([n, value]) => `export const ${n} = ${value};`);
    for (const n of names) if (!(n in stated)) lines.push(`export function ${n}() {}`);
    return [file, lines.join('\n')];
  }));
}
