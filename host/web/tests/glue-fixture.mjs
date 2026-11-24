import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { bundleHostModule } from '../../web-js/glue.mjs';

const root = resolve(import.meta.dir, '../../..');
// As a page with popovers has it (glue.js, or a JS target page with a dialog, a menu or a popover: navigation.js
// `placePopovers`, `stampOpenings`).
export const navigationSource = await bundleHostModule(root, 'navigation.js') + 'placePopovers();\nstampOpenings();\n';
export const wasmSource = await bundleHostModule(root, 'glue.js');
const admissionSource = await bundleHostModule(root, 'grant-admission.js');

// Browser fixtures serve the same single modules that an app build ships.
export function hostFixtureFile(path, encoding) {
  const file = path instanceof URL ? fileURLToPath(path) : resolve(path);
  const source = file === resolve(root, 'host/web/glue.js') ? wasmSource
    : file === resolve(root, 'host/web/navigation.js') ? navigationSource
    : file === resolve(root, 'host/web/grant-admission.js') ? admissionSource : null;
  return source === null ? readFileSync(path, encoding)
    : encoding ? source : Buffer.from(source);
}

import { parseSync } from 'rolldown/utils';

// Select declarations, not text between neighbouring functions: a new neighbour
// must not silently become part of a fixture's scope.
export function glueDeclarations(source, names) {
  const parsed = parseSync('glue.js', source);
  if (parsed.errors.length) throw new Error(`glue.js fixture: ${JSON.stringify(parsed.errors)}`);
  const declarations = new Map();
  for (const entry of parsed.program.body) {
    const statement = entry.type === 'ExportNamedDeclaration' ? entry.declaration : entry;
    if (!statement) continue;
    if (statement.type === 'FunctionDeclaration' || statement.type === 'ClassDeclaration') declarations.set(statement.id.name, source.slice(statement.start, statement.end));
    if (statement.type === 'VariableDeclaration') for (const declaration of statement.declarations) {
      if (declaration.id.type === 'Identifier') declarations.set(declaration.id.name, `${statement.kind} ${source.slice(declaration.start, declaration.end)};`);
    }
  }
  const geometry = names.includes('refresh') || names.includes('refreshSymbols')
    ? readFileSync(resolve(root, 'host/web/symbol-geometry.js'), 'utf8').replaceAll('export function ', 'function ') : '';
  return geometry + names.map(name => {
    if (!declarations.has(name)) throw new Error(`glue.js fixture: missing declaration ${name}`);
    return declarations.get(name);
  }).join('\n');
}

// Inert stand-ins for the module bindings of glue.js that `names`' declarations read, for a page that runs them: the
// page declares its own doubles and the declarations in a block under these, so a binding the glue comes to read is
// one that does nothing, not a ReferenceError the fixture must be taught. A stand-in is any function or object: called
// or read it gives itself, iterated nothing, awaited itself. `imported`: what the page imports beside them, at their scope.
export function glueStandIns(source, names, imported = []) {
  const top = new Set();
  for (const entry of parseSync('glue.js', source).program.body) {
    const statement = entry.type === 'ExportNamedDeclaration' ? entry.declaration : entry;
    if (statement?.id?.name) top.add(statement.id.name);
    if (statement?.type === 'VariableDeclaration') for (const d of statement.declarations) if (d.id.type === 'Identifier') top.add(d.id.name);
    if (entry.type === 'ImportDeclaration') for (const s of entry.specifiers) top.add(s.local.name);
  }
  const read = new Set();
  (function visit(node) {
    if (!node || typeof node !== 'object') return;
    if (node.type === 'Identifier' && top.has(node.name) && !names.includes(node.name) && !imported.includes(node.name)) read.add(node.name);
    for (const value of Object.values(node)) if (value && typeof value === 'object') visit(value);
  })(parseSync('glue.js', glueDeclarations(source, names)).program);
  return "const standIn = new Proxy(function () {}, { get: (_, k) => k === Symbol.iterator ? function* () {} : k === 'then' || typeof k === 'symbol' ? undefined : standIn, apply: () => standIn });\n"
    + (read.size ? `const ${[...read].map(name => `${name} = standIn`).join(', ')};\n` : '');
}

export function checkedGlueCall(call) {
  return (...args) => {
    try { return call(...args); }
    catch (error) {
      if (error instanceof ReferenceError) throw new Error(`glue.js fixture: supply the module binding used by ${call.name}: ${error.message}`, { cause: error });
      throw error;
    }
  };
}

export function glueCase(source, name) {
  const parsed = parseSync('glue.js', source);
  let found;
  function visit(node) {
    if (!node || typeof node !== 'object') return;
    if (node.type === 'SwitchCase' && node.test?.value === name) {
      found = source.slice(node.start, node.end);
      return;
    }
    for (const value of Object.values(node)) {
      if (Array.isArray(value)) value.forEach(visit);
      else if (value && typeof value === 'object') visit(value);
    }
  }
  visit(parsed.program);
  if (!found) throw new Error(`glue.js fixture: missing case ${name}`);
  return found;
}

export function symbolSheet(source) {
  const parsed = parseSync('symbols.js', source);
  let sheet;
  function visit(node) {
    if (!node || typeof node !== 'object') return;
    if (node.type === 'Literal' && typeof node.value === 'string' && node.value.startsWith('@property --exact-tint')) sheet = node.value;
    for (const value of Object.values(node)) {
      if (Array.isArray(value)) value.forEach(visit);
      else if (value && typeof value === 'object') visit(value);
    }
  }
  visit(parsed.program);
  if (!sheet) throw new Error('symbol fixture: missing stylesheet');
  return sheet;
}

// Select a nested expression or statement without depending on the printer's layout.
export function glueNode(source, select) {
  let found;
  function visit(node) {
    if (found || !node || typeof node !== 'object') return;
    found = select(node);
    if (found) return;
    for (const value of Object.values(node)) {
      if (Array.isArray(value)) value.forEach(visit);
      else if (value && typeof value === 'object') visit(value);
    }
  }
  visit(parseSync('glue.js', source).program);
  if (!found) throw new Error('glue.js fixture: missing selected node');
  return source.slice(found.start, found.end);
}
