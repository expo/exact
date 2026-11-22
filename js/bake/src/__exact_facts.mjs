// Build facts from syntax, never module initialization (LLP 1016 D6).
import { bare, lineColumn, propertyName, walkRuntime } from './__exact_portable.mjs';
import { grantsDeclaration } from './__exact_launch_setting.mjs';

const named = node => node?.name ?? node?.value;
/** The names a declaration's binding pattern binds. */
const bound = (pattern, names = []) => {
  if (pattern?.type === 'Identifier') names.push(pattern.name);
  else if (pattern?.type === 'ObjectPattern') for (const p of pattern.properties) bound(p.type === 'RestElement' ? p.argument : p.value, names);
  else if (pattern?.type === 'ArrayPattern') for (const p of pattern.elements) bound(p, names);
  else if (pattern?.type === 'AssignmentPattern') bound(pattern.left, names);
  else if (pattern?.type === 'RestElement') bound(pattern.argument, names);
  return names;
};
const syntax = "must be a constant the build reads without running the module: a string, a template or + of constants, an array of them .join('\\n'), or a top-level const holding one";

function moduleFacts(file, code, parse, source = code) {
  const parsed = parse(file, code), consts = new Map(), imports = new Map(), exported = new Map(), stars = [];
  const original = new WeakMap();
  if (source !== code) {
    // Settings and backend selection rewrite values; receipt locations still name the authored expressions.
    const align = (node, authored) => {
      node = bare(node); authored = bare(authored);
      if (!node || !authored || typeof node !== 'object' || typeof authored !== 'object') return;
      original.set(node, authored);
      for (const [key, child] of Object.entries(node)) {
        const previous = authored.type === node.type ? authored[key] : authored;
        if (Array.isArray(child)) child.forEach((n, i) => align(n, Array.isArray(previous) ? previous[i] : previous));
        else if (child && typeof child === 'object') align(child, previous);
      }
    };
    align(parsed.program, parse(file, source).program);
  }
  for (const statement of parsed.program.body) {
    const s = ['ExportNamedDeclaration', 'ExportDefaultDeclaration'].includes(statement.type) ? statement.declaration ?? statement : statement;
    if (s.type === 'VariableDeclaration') for (const d of s.declarations) {
      if (d.id.type !== 'Identifier') {
        // A destructured export is no constant the build reads, and it is still the module's export of each name
        // it binds: without an entry, the name would resolve through an `export *` beside it (ECMA-262 ResolveExport).
        if (statement.type === 'ExportNamedDeclaration') for (const name of bound(d.id)) exported.set(name, { local: name });
        continue;
      }
      if (s.kind === 'const') consts.set(d.id.name, d.init);
      if (statement.type === 'ExportNamedDeclaration') exported.set(d.id.name, { local: d.id.name });
    }
    if (statement.type === 'ImportDeclaration' && statement.importKind !== 'type') for (const x of statement.specifiers) {
      if (x.importKind !== 'type') imports.set(x.local.name, { source: statement.source.value, imported: x.type === 'ImportDefaultSpecifier' ? 'default' : named(x.imported), namespace: x.type === 'ImportNamespaceSpecifier', node: x });
    }
    if (statement.type === 'ExportNamedDeclaration' && statement.exportKind !== 'type') {
      if (s.id?.name) exported.set(s.id.name, { local: s.id.name });
      for (const x of statement.specifiers ?? []) if (x.exportKind !== 'type') exported.set(named(x.exported), statement.source
        ? { source: statement.source.value, imported: named(x.local), node: x } : { local: named(x.local) });
    }
    if (statement.type === 'ExportDefaultDeclaration') exported.set('default', { node: s });
    if (statement.type === 'ExportAllDeclaration' && statement.exportKind !== 'type') {
      if (statement.exported) exported.set(named(statement.exported), { namespace: true, node: statement });
      else stars.push(statement);
    }
  }
  return { file, code: source, parsed, consts, imports, exported, stars, original, at: lineColumn(source) };
}

function constantBindings(code, parse, { load, source = code } = {}) {
  const root = moduleFacts('app.ts', code, parse, source), modules = new Map([['app.ts', root]]);
  const refuse = (m, node, name, why = syntax) => { throw new Error(`${m.file}:${m.at((m.original.get(node) ?? node)?.start ?? 0)}: ${name} ${why}`); };
  const imported = (m, source, node, name) => {
    if (!load || !['grants', 'turnBudgets'].includes(name)) refuse(m, node, name);
    const next = load(source, m.file, message => refuse(m, node, name, message));
    if (!modules.has(next.file)) modules.set(next.file, moduleFacts(next.file, next.code, parse, next.source ?? next.code));
    const result = modules.get(next.file);
    if (result.parsed.errors?.length) refuse(m, node, name, `cannot read ${next.file}: the module does not parse`);
    return result;
  };
  const active = new Set();
  const visit = (m, key, node, name, run) => {
    const id = `${m.file}:${key}`;
    if (active.has(id)) refuse(m, node, name, `contain a constant cycle at ${key}`);
    active.add(id);
    try { return run(); } finally { active.delete(id); }
  };
  const binding = (m, local, node, name, evaluate) => visit(m, local, node, name, () => {
    if (m.consts.has(local)) return evaluate(m, m.consts.get(local), name);
    const entry = m.imports.get(local);
    if (entry) {
      if (entry.namespace) refuse(m, entry.node, name, 'cannot read a namespace import whole; read one of its constants, as in ns.NAME');
      return exported(imported(m, entry.source, entry.node, name), entry.imported, name, evaluate);
    }
    refuse(m, node, name, `require a top-level const; ${local} is not one`);
  });
  const ambiguous = Symbol('ambiguous export');
  const resolveLocal = (m, local, node, name, resolveSet) => {
    const entry = m.imports.get(local);
    if (!entry) return { m, local, node };
    if (entry.namespace) refuse(m, entry.node, name, 'cannot read a namespace import whole; read one of its constants, as in ns.NAME');
    return resolveExport(imported(m, entry.source, entry.node, name), entry.imported, name, resolveSet);
  };
  // ECMA-262 ResolveExport: an export path may loop without its initializer cycling.
  const resolveExport = (m, key, name, resolveSet = new Set()) => {
    const entry = m.exported.get(key), node = entry?.node ?? m.consts.get(entry?.local) ?? m.imports.get(entry?.local)?.node ?? m.stars[0] ?? m.parsed.program;
    const id = `${m.file}:export ${key}`;
    if (resolveSet.has(id)) return undefined;
    resolveSet.add(id);
    if (entry?.namespace) refuse(m, node, name, 'cannot read a namespace export; export a named literal constant');
    if (entry?.local) return resolveLocal(m, entry.local, node, name, resolveSet);
    if (entry?.source) {
      return resolveExport(imported(m, entry.source, entry.node, name), entry.imported, name, resolveSet);
    }
    if (entry?.node) return { m, node: entry.node };
    if (key === 'default') return undefined;
    let found;
    for (const s of m.stars) {
      const next = resolveExport(imported(m, s.source.value, s, name), key, name, resolveSet);
      if (next === ambiguous) return ambiguous;
      if (!next) continue;
      if (found && (found.m !== next.m || (found.local ? found.local !== next.local : found.node !== next.node)))
        return ambiguous;
      found = next;
    }
    return found;
  };
  const resolveBinding = (m, key, name) => {
    const target = resolveExport(m, key, name);
    if (target === ambiguous) refuse(m, m.stars[0] ?? m.parsed.program, name, `cannot resolve the literal export ${key} unambiguously`);
    return target;
  };
  const exported = (m, key, name, evaluate, optional = false) => {
    const target = resolveBinding(m, key, name);
    if (!target) {
      if (optional && !m.exported.has(key)) return undefined;
      refuse(m, m.stars[0] ?? m.parsed.program, name, `cannot resolve the literal export ${key}`);
    }
    return target.local ? binding(target.m, target.local, target.node, name, evaluate) : evaluate(target.m, target.node, name);
  };
  return { root, refuse, imported, binding, exported, resolveBinding };
}

/** Freeze the optional source budgets into the bytecode. The checker's source
 * roster covers every Contract entry and excludes sources owned by Rust. */
export function moduleMetadata(code, parse, options, declaration) {
  return async (checker, type, _api, program) => {
    const names = (await checker.getPropertiesOfType(type)).map(symbol => symbol.name);
    if (!names.includes('turnBudgets')) return { names };
    const file = await program.getSourceFile(declaration);
    const exports = await checker.getExportsOfModule(await checker.getSymbolAtLocation(file));
    const sources = exports.find(symbol => symbol.name === 'TypeScriptSources');
    const rows = await checker.getPropertiesOfType(await checker.getDeclaredTypeOfSymbol(sources));
    return { names, budgets: sourceTurnBudgets(code, parse, options, new Set(rows.map(row => row.name))) };
  };
}

export function sourceTurnBudgets(code, parse, options, sources) {
  const { root, binding, exported, refuse } = constantBindings(code, parse, options);
  const read = (module, expression, name) => {
    const node = bare(expression);
    if (node?.type === 'Identifier') {
      if (node.name === 'NaN' || node.name === 'Infinity') refuse(module, node, name, 'requires finite milliseconds from 100 to 1000');
      return binding(module, node.name, node, name, read);
    }
    if (node?.type === 'Literal' && typeof node.value === 'number') return node.value;
    if (node?.type !== 'ObjectExpression') refuse(module, node, name, 'must be a constant map of source names to milliseconds');
    const budgets = Object.create(null);
    for (const property of node.properties) {
      if (property.type === 'SpreadElement') {
        const part = read(module, property.argument, name);
        if (typeof part !== 'object') refuse(module, property, name, 'can only spread a source budget map');
        Object.assign(budgets, part);
        continue;
      }
      const source = propertyName(property);
      if (!sources.has(source)) refuse(module, property, name, `names unknown TypeScript source ${source ?? '(computed name)'}`);
      const value = read(module, property.value, name);
      if (!Number.isFinite(value) || value < 100 || value > 1000)
        refuse(module, property, name, `requires a finite budget from 100 to 1000 ms for ${source}`);
      budgets[source] = value;
    }
    return budgets;
  };
  const result = exported(root, 'turnBudgets', 'turnBudgets', read);
  if (typeof result !== 'object') throw new Error('app.ts: turnBudgets must map source names to milliseconds');
  return result;
}

/** Native exports retained by the bundle after its module metadata is checked. */
export function nativeModuleEntry({ names: exported, budgets }) {
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
  const policy = budgets && Object.keys(budgets).length ? 'export const turnBudgetsJson = '+JSON.stringify(JSON.stringify(budgets))+';\n' : '';
  return entry + policy;
}

/** The initializer actually exported as grants, with its authored node for receipt locations. */
export function grantsInitializer(code, parse, options) {
  const { root, resolveBinding } = constantBindings(code, parse, options);
  const target = resolveBinding(root, 'grants', 'grants');
  if (!target) return undefined;
  const node = target.m.consts.get(target.local) ?? target.node;
  return { node, authored: target.m.original.get(bare(node)) ?? node, file: target.m.file };
}

/** appId stays local; grants may follow imports through `load`, which returns captured app-local syntax. */
export function staticFacts(code, parse, { load, locations = false, source = code } = {}) {
  const { root, refuse, imported, binding, exported, resolveBinding } = constantBindings(code, parse, { load, source });
  if (root.parsed.errors?.length) return { appId: undefined, grants: undefined, exported: [], ...(locations ? { grantSources: [] } : {}) };
  const literal = (m, node, value) => {
    node = m.original.get(node) ?? node;
    const text = String(value), offsets = [node.start];
    if (node.type === 'TemplateElement' || node.type === 'Literal' && typeof node.value === 'string') {
      const raw = m.code.slice(node.start + 1, node.end - (node.type === 'TemplateElement' && !node.tail ? 2 : 1));
      // LF escapes and template line breaks emit newlines; a line continuation emits nothing.
      for (const match of raw.matchAll(/\\(?:u\{[\da-fA-F]+\}|u[\da-fA-F]{4}|x[\da-fA-F]{2}|\r\n|[\s\S])|\r\n|[\s\S]/g)) {
        const token = match[0], start = node.start + 1 + match.index, end = start + token.length;
        if (token === '\n' || node.type === 'TemplateElement' && token[0] === '\r'
          || token === '\\n' || /^\\(?:x0[aA]|u000[aA]|u\{0*[aA]\})$/.test(token)) offsets.push(end);
        else if (/^\\[\r\n\u2028\u2029]/.test(token) && offsets.at(-1) === start) offsets[offsets.length - 1] = end;
      }
    }
    const parts = text.split('\n').map((line, i, lines) => {
      const [row, column] = m.at(offsets[i] ?? node.start).split(':').map(Number);
      return { text: line + (i < lines.length - 1 ? '\n' : ''), file: m.file, line: row, column };
    });
    return { value, parts };
  };
  const combine = (left, right) => {
    const value = left.value + right.value;
    return { value, parts: typeof value === 'number' ? [{ ...left.parts[0], text: String(value) }] : [...left.parts, ...right.parts] };
  };
  const scalar = (m, node, name) => {
    const result = constant(m, node, name);
    if (Array.isArray(result)) refuse(m, node, name, 'require a string; join the literal array');
    return result;
  };
  const constant = (m, node, name) => {
    node = bare(node);
    if (!node) refuse(m, node, name, 'cannot contain a hole in a literal array');
    if (node.type === 'Literal' && ['string', 'number'].includes(typeof node.value)) return literal(m, node, node.value);
    if (node.type === 'Identifier') return binding(m, node.name, node, name, constant);
    if (node.type === 'TemplateLiteral') return node.quasis.reduce((result, q, i) => {
      const next = combine(result, literal(m, q, q.value.cooked));
      return i < node.expressions.length ? combine(next, scalar(m, node.expressions[i], name)) : next;
    }, { value: '', parts: [] });
    if (node.type === 'BinaryExpression' && node.operator === '+') return combine(scalar(m, node.left, name), scalar(m, node.right, name));
    if (node.type === 'ArrayExpression') return node.elements.flatMap(element => {
      if (element?.type !== 'SpreadElement') return [scalar(m, element, name)];
      const values = constant(m, element.argument, name);
      if (!Array.isArray(values)) refuse(m, element, name, 'may spread only a literal array of constants');
      return values;
    });
    if (node.type === 'CallExpression' && !node.optional && node.arguments.length <= 1 && propertyName(bare(node.callee)) === 'join') {
      const array = constant(m, bare(node.callee).object, name);
      if (!Array.isArray(array)) refuse(m, node, name, 'may join only a literal array of constants');
      const separator = node.arguments.length ? scalar(m, node.arguments[0], name) : literal(m, node, ',');
      return array.reduce((result, entry, i) => combine(i ? combine(result, { ...separator, value: String(separator.value) }) : result, { ...entry, value: String(entry.value) }), { value: '', parts: [] });
    }
    // `ns.NAME` through `import * as ns` reads the export NAME, as a named import would. Only the dot form: the
    // launch-setting walk (__exact_env.mjs `key`) does not read every key this file's `propertyName` does.
    const namespace = node.type === 'MemberExpression' && !node.computed && bare(node.object)?.type === 'Identifier' ? m.imports.get(bare(node.object).name) : undefined;
    if (namespace?.namespace && propertyName(node) !== null)
      return exported(imported(m, namespace.source, namespace.node, name), propertyName(node), name, constant);
    const why = node.type === 'CallExpression' ? 'cannot call code; only a literal array .join(separator) is allowed'
      : node.type === 'ConditionalExpression' ? `${syntax}; conditional expressions are not allowed`
      : node.type === 'MemberExpression' ? 'cannot read a computed value or array element; use literal constants' : syntax;
    refuse(m, node, name, why);
  };
  const value = name => {
    const entry = root.exported.get(name);
    if (name !== 'grants' && (!entry || entry.source)) return undefined;
    const result = exported(root, name, name, constant, true);
    if (!result) return undefined;
    if (Array.isArray(result)) {
      const target = resolveBinding(root, name, name);
      refuse(target.m, target.m.consts.get(target.local) ?? target.node, name, 'must be a string; join the literal array');
    }
    if (typeof result.value !== 'string') throw new Error(`app.ts: ${name} must be a string; it is the number ${result.value}`);
    return result;
  };
  const appId = value('appId'), grants = value('grants');
  const result = { appId: appId?.value, grants: grants?.value, exported: [...root.exported.keys()] };
  if (locations) {
    const lines = [];
    let current = '', source;
    for (const part of grants?.parts ?? []) for (const piece of part.text.split(/(?<=\n)/)) {
      if (!source && piece.trim()) { const { text, ...location } = part; source = location; }
      current += piece;
      if (piece.endsWith('\n')) { if (current.trim()) lines.push({ grant: current.trim(), ...source }); current = ''; source = undefined; }
    }
    if (current.trim()) lines.push({ grant: current.trim(), ...source });
    result.grantSources = lines;
  }
  return result;
}

/** Freeze the app's exported grants while keeping the other exports and module side effects. */
export function freezeGrants(code, parse, grants) {
  const body = parse('app.ts', code).program.body, declaration = grantsDeclaration(body);
  const value = JSON.stringify(grants), edits = [], referenced = new Set();
  if (declaration) {
    walkRuntime(declaration.init, (node, _, parent) => {
      if (node.type === 'Identifier' && !(parent?.type === 'MemberExpression' && parent.property === node && !parent.computed)) referenced.add(node.name);
    });
    edits.push([declaration.init.start, declaration.init.end, value]);
  }
  for (const s of declaration ? [] : body) {
    if (s.type !== 'ExportNamedDeclaration' || s.exportKind === 'type') continue;
    const keep = s.specifiers?.filter(x => x.exportKind === 'type' || named(x.exported) !== 'grants');
    if (!keep || keep.length === s.specifiers.length) continue;
    const list = keep.map(x => code.slice(x.start, x.end)).join(', ');
    const from = s.source ? code.slice(s.source.start, s.source.end) : null;
    if (!from) for (const x of s.specifiers) if (x.exportKind !== 'type' && named(x.exported) === 'grants') referenced.add(named(x.local));
    edits.push([s.start, s.end, (from ? `import ${from};` : '') + (keep.length ? `export { ${list} }${from ? ` from ${from}` : ''};` : '')]);
  }
  // TypeScript erases a value import once freezing removes its last reference; its module must still initialize.
  for (const s of body) if (s.type === 'ImportDeclaration' && s.importKind !== 'type'
    && s.specifiers.some(x => x.importKind !== 'type' && referenced.has(x.local.name))) {
    edits.push([s.start, s.start, `import ${code.slice(s.source.start, s.end)}\n`]);
  }
  for (const [start, end, text] of edits.sort((a, b) => b[0] - a[0])) code = code.slice(0, start) + text + code.slice(end);
  if (declaration) return code;
  let local = '__exact_resolved_grants';
  while (code.includes(local)) local += '_';
  return `${code}\nconst ${local} = ${value};\nexport { ${local} as grants };\n`;
}

/** A `load` for grants over a build's recorded import graph (`__exact_inputs.json` edges, `resolveImport`): each
 * import's app-local code module, as `code(file)` holds it and as `source(file)` was authored, else a refusal. */
export function recordedConstants(resolveImport, code, source = code) {
  return (specifier, importer, refuse) => {
    const file = resolveImport(importer, specifier) || null;
    const packageCode = file?.split('/').includes('node_modules') || file?.startsWith('__exact_modules/');
    if (packageCode || !specifier.startsWith('.') && (!file || file.startsWith('../'))) refuse(`cannot use package binding ${JSON.stringify(specifier)}; import an app-local literal constant`);
    if (!file || file.startsWith('../')) refuse(`cannot resolve ${JSON.stringify(specifier)} inside the captured app`);
    if (!/\.[cm]?[jt]sx?$/.test(file) || /\.d\.[cm]?ts$/.test(file)) refuse(`require literal constants in an app-local code module, received ${file}`);
    const text = code(file);
    if (text === undefined) refuse(`cannot resolve ${JSON.stringify(specifier)} inside the captured app`);
    return { file, code: text, source: source(file) ?? text };
  };
}
