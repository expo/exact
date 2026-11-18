// Build facts from syntax, never module initialization (LLP 1016 D6).
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { relative, resolve, sep } from 'node:path';
import { bare, lineColumn, propertyName, walkRuntime } from './__exact_portable.mjs';
import { grantsDeclaration } from './__exact_launch_setting.mjs';

const named = node => node?.name ?? node?.value;
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
      if (d.id.type !== 'Identifier') continue;
      if (s.kind === 'const') consts.set(d.id.name, d.init);
      if (statement.type === 'ExportNamedDeclaration') exported.set(d.id.name, { local: d.id.name });
    }
    if (statement.type === 'ImportDeclaration' && statement.importKind !== 'type') for (const x of statement.specifiers) {
      if (x.importKind !== 'type') imports.set(x.local.name, { source: statement.source.value, imported: x.type === 'ImportDefaultSpecifier' ? 'default' : named(x.imported), node: x });
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
    if (!load || name !== 'grants') refuse(m, node, name);
    const next = load(source, m.file, message => refuse(m, node, name, message));
    if (!modules.has(next.file)) modules.set(next.file, moduleFacts(next.file, next.code, parse));
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
      if (!entry.imported) refuse(m, entry.node, name, 'cannot read a namespace import; import a named literal constant');
      return exported(imported(m, entry.source, entry.node, name), entry.imported, name, evaluate);
    }
    refuse(m, node, name, `require a top-level const; ${local} is not one`);
  });
  const ambiguous = Symbol('ambiguous export');
  const resolveLocal = (m, local, node, name, resolveSet) => {
    const entry = m.imports.get(local);
    if (!entry) return { m, local, node };
    if (!entry.imported) refuse(m, entry.node, name, 'cannot read a namespace import; import a named literal constant');
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
  return { root, refuse, binding, exported, resolveBinding };
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
export function staticFacts(code, parse, { load, launchNames = new Set(), locations = false, source = code, checkLaunchOnly = false } = {}) {
  const { root, refuse, binding, exported, resolveBinding } = constantBindings(code, parse, { load, source });
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
    const why = node.type === 'CallExpression' ? 'cannot call code; only a literal array .join(separator) is allowed'
      : node.type === 'ConditionalExpression' ? `${syntax}; conditional expressions are not allowed`
      : node.type === 'MemberExpression' ? 'cannot read a computed value or array element; use literal constants' : syntax;
    refuse(m, node, name, why);
  };
  // Inspect dependencies before folding: a web build inlines launch settings too, but grants still cannot use them.
  const checked = new WeakSet();
  const checkLaunch = (m, node, name) => {
    node = bare(node);
    if (!node || typeof node !== 'object') return;
    if (checked.has(node)) return;
    checked.add(node);
    if (node.type === 'Identifier' && (m.consts.has(node.name) || m.imports.has(node.name))) return binding(m, node.name, node, name, checkLaunch);
    if (node.type === 'MemberExpression' && launchNames.has(propertyName(node)) && propertyName(bare(node.object)) === 'env'
      && bare(bare(node.object).object)?.type === 'MetaProperty') refuse(m, node, name, `depend on launch-marked setting ${propertyName(node)}; grants are fixed at build (LLP 1153.002 D3)`);
    for (const [key, child] of Object.entries(node)) {
      if (key === 'property' && node.type === 'MemberExpression' && !node.computed) continue;
      if (key === 'key' && node.type === 'Property' && !node.computed) continue;
      if (Array.isArray(child)) child.forEach(n => checkLaunch(m, n, name));
      else if (child && typeof child === 'object') checkLaunch(m, child, name);
    }
  };
  const value = name => {
    const entry = root.exported.get(name);
    if (name !== 'grants' && (!entry || entry.source)) return undefined;
    if (name === 'grants' && launchNames.size) exported(root, name, name, checkLaunch, true);
    if (checkLaunchOnly) return undefined;
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

/** Read app-local syntax using the same plugin chain and resolution as the data bundle. */
export async function capturedConstants(stage, rolldown, options, parse, code = readFileSync(resolve(stage, 'app.ts'), 'utf8')) {
  stage = realpathSync.native(stage);
  const configFile = ['__exact_tsconfig.json', 'tsconfig.json'].map(file => resolve(stage, file)).find(existsSync);
  const resolutions = new Map(), visited = new Set();
  const local = (source, found, refuse) => {
    const path = found && !found.external && !found.id.startsWith('\0') && existsSync(found.id) ? realpathSync.native(found.id) : null;
    const file = path && relative(stage, path).split(sep).join('/');
    const packageCode = file?.split('/').includes('node_modules') || file?.startsWith('__exact_modules/');
    if (packageCode || !source.startsWith('.') && (!file || file.startsWith('../'))) refuse(`cannot use package binding ${JSON.stringify(source)}; import an app-local literal constant`);
    if (!file || file.startsWith('../') || file === '..' || resolve(stage, file) !== path) refuse(`cannot resolve ${JSON.stringify(source)} inside the captured app`);
    if (!/\.[cm]?[jt]sx?$/.test(file) || /\.d\.[cm]?ts$/.test(file)) refuse(`require literal constants in an app-local code module, received ${file}`);
    return { file, code: readFileSync(path, 'utf8') };
  };
  const build = await rolldown({ ...options, cwd: stage, input: '\0exact-constants', tsconfig: configFile ?? false, logLevel: 'silent',
    plugins: [{ name: 'exact-constants',
      resolveId(id) { if (id === '\0exact-constants') return id; },
      load(id) { if (id === '\0exact-constants') return ''; },
      async buildStart() {
        const visit = async (file, code) => {
          if (visited.has(file)) return;
          visited.add(file);
          const m = moduleFacts(file, code, parse);
          const sources = new Set([...m.imports.values(), ...m.exported.values(), ...m.stars].map(x => typeof x.source === 'string' ? x.source : x.source?.value).filter(Boolean));
          for (const source of sources) {
            let result;
            try {
              let found;
              try { found = await this.resolve(source, resolve(stage, file)); }
              catch { /* The literal reader reports the refused binding at its import site. */ }
              result = local(source, found, message => { throw new Error(message); });
            } catch (error) { result = error; }
            resolutions.set(`${file}\0${source}`, result);
            if (!(result instanceof Error)) await visit(result.file, result.code);
          }
        };
        await visit('app.ts', code);
      },
    }, ...(options.plugins ?? []).map(({ name, resolveId }) => ({ name, resolveId }))],
  });
  try { await build.generate({ format: 'es' }); }
  finally { await build.close(); }
  return (source, importer, refuse) => {
    const result = resolutions.get(`${importer}\0${source}`);
    if (result instanceof Error) refuse(result.message);
    if (!result) refuse(`cannot resolve ${JSON.stringify(source)} inside the captured app`);
    return result;
  };
}
