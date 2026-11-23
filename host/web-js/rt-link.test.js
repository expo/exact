// rt-link.js's object is filled by hand: rt.js, ts-link.js and a few others put names on it, and lazy chunks read them
// off it at run time, where a name nobody put there is `undefined` and fails only on the page that calls it. This
// matches the names across the sources: every name a module reads off the link is one some module puts there, every
// name put there is read, and the link is used only in ways that say which names. Whether the module that puts a name
// is on the page that reads it is the browser tests' to show (watched-topics.test.mjs for ts-link.js).
import { expect, test } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseAst } from 'rolldown/parseAst';

const here = fileURLToPath(new URL('.', import.meta.url));
const reads = new Map(), writes = new Map(); // name -> the modules that read or write it
const unnamed = []; // a use of the link that names nothing: passed on, spread, read by a computed key
const add = (map, name, file) => map.set(name, [...(map.get(name) ?? []), file]);
const key = p => p.key?.name ?? p.key?.value;

for (const dir of [here, resolve(here, '../web')]) for (const f of readdirSync(dir)) {
  if (!/\.m?js$/.test(f) || /\.test\.m?js$/.test(f)) continue;
  const ast = parseAst(readFileSync(resolve(dir, f), 'utf8'));
  const link = ast.body.filter(s => s.type === 'ImportDeclaration' && s.source.value === './rt-link.js')
    .flatMap(s => s.specifiers).find(s => s.imported?.name === 'rt')?.local.name;
  if (!link) continue;
  const isLink = n => n?.type === 'Identifier' && n.name === link;
  const named = new Set(); // the link's identifiers in a use that says which names it reads or writes
  (function walk(n) {
    if (!n || typeof n.type !== 'string' || n.type === 'ImportDeclaration') return;
    if (n.type === 'AssignmentExpression' && n.left.type === 'MemberExpression' && isLink(n.left.object) && !n.left.computed) {
      add(writes, n.left.property.name, f);
      named.add(n.left.object);
    } else if (n.type === 'MemberExpression' && isLink(n.object) && !n.computed) {
      add(reads, n.property.name, f);
      named.add(n.object);
    }
    if (n.type === 'CallExpression' && n.callee.type === 'MemberExpression' && n.callee.object.name === 'Object'
      && n.callee.property.name === 'assign' && isLink(n.arguments[0]) && n.arguments.slice(1).every(a => a.type === 'ObjectExpression')) {
      for (const source of n.arguments.slice(1)) for (const p of source.properties) add(writes, key(p), f);
      named.add(n.arguments[0]);
    }
    if (n.type === 'VariableDeclarator' && isLink(n.init) && n.id.type === 'ObjectPattern' && n.id.properties.every(key)) {
      for (const p of n.id.properties) add(reads, key(p), f);
      named.add(n.init);
    }
    if (isLink(n) && !named.has(n)) unnamed.push(`${f} at offset ${n.start}`);
    for (const v of Object.values(n)) for (const c of Array.isArray(v) ? v : [v]) if (c && typeof c === 'object') walk(c);
  })(ast);
}

test('every name read off the runtime link is put there, and every name put there is read', () => {
  expect(reads.size).toBeGreaterThan(20);
  expect(reads.get('sourceChanged')).toContain('ts-data.js');
  expect(unnamed).toEqual([]);
  expect([...reads].filter(([name]) => !writes.has(name)).map(([name, by]) => `${name} (read by ${by.join(', ')})`)).toEqual([]);
  expect([...writes].filter(([name]) => !reads.has(name)).map(([name, by]) => `${name} (put by ${by.join(', ')})`)).toEqual([]);
});
