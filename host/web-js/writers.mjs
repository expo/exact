// What Exact's own code can write inside `#exact-root` (LLP 1047.003 D1): the tags it makes and the attribute names it
// writes, read from the text of the modules a page can load. The sheet leaves out a rule only for a tag or an
// attribute that neither this code nor the compiled plan (`planTokens`, below) writes, so whatever this cannot read
// counts as written: a module that begins to write a name costs bytes, not a style. The exception is a writing method
// used other than by a call that names it (LLP 1047.003 §6.5), which the checks catch.
import { parseSync, Visitor } from 'rolldown/utils';
import { walkRuntime } from '../../js/bake/src/__exact_portable.mjs';

// The plan's helpers (tree.js, dataset.js): the compiled plan makes its elements through `h` and `hs`, and writes
// their attributes through them, `P` (a bound one) and `ds` (a bound `dataset`'s words). What their parameters carry
// is the plan's, which `planTokens` reads at each call.
const PLAN_HELPERS = new Set(['h', 'hs', 'P', 'ds']);
// Calls that read an element and write nothing: a string among their arguments names what they read.
const READS = new Set(['matches', 'closest', 'querySelector', 'querySelectorAll', 'getAttribute', 'hasAttribute', 'removeAttribute', 'startsWith']);
// The calls that write an attribute by name, with the position of the name.
const SETS = { setAttribute: 0, toggleAttribute: 0, createAttribute: 0, setAttributeNS: 1 };
// What makes elements from text, whose tags and attributes no call names.
const MARKUP = new Set(['innerHTML', 'outerHTML', 'insertAdjacentHTML', 'setHTML', 'setHTMLUnsafe', 'parseFromString', 'createContextualFragment']);
// Constructors that make an element: `new Image()` is an `img`.
const MADE_BY = { Image: 'img', Audio: 'audio', Option: 'option' };

const escape = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const kebab = s => s.replace(/[A-Z]/g, c => `-${c.toLowerCase()}`);
const literal = node => node?.type === 'Literal' && typeof node.value === 'string' ? node.value
  : node?.type === 'TemplateLiteral' && !node.expressions.length ? node.quasis[0].value.cooked : null;
// What an expression can be, as far as names go: `names`; `built`, the strings around each computed part
// (`"data-" + w` is `["data-", ""]`); `objects`, the object and array literals it can be. Null when this cannot tell.
const none = () => ({ names: new Set(), built: [], objects: [] });
const join = (x, y) => x && y && { names: new Set([...x.names, ...y.names]), built: [...x.built, ...y.built], objects: [...x.objects, ...y.objects] };
const all = list => list.reduce(join, none());
const FUNCTIONS = new Set(['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression']);
// The object that `Object.keys(o)` or `Object.entries(o)` walks, or null; `global(id)` says whether that `Object` is
// the builtin.
const objectWalk = (call, global) => {
  const c = call.type === 'CallExpression' ? call.callee : null;
  return c?.type === 'MemberExpression' && !c.computed && c.object.type === 'Identifier' && c.object.name === 'Object' && global(c.object)
    && ['keys', 'entries'].includes(c.property.name) ? call.arguments[0] ?? null : null;
};

/** What the compiled plan (`plan`, the compiler's `app.js`) and Exact's code (`files`, `[name, code]` each: the modules
 * a page can load, and the files the build copies beside it) can write inside the root. `writes(token)` (`t:div`,
 * `a:data-listrows`, as page-style.mjs `needs` names them) is false only for a tag neither makes and an attribute
 * neither names but to read it. `open` says which kind Exact's code wrote in a way this did not read: a computed tag
 * (every tag counts as made), a computed attribute name (every attribute as written), or an element made from text
 * (both), unless into a shadow root, which the document's sheet does not reach. */
export function writers(plan, files) {
  const planned = planTokens(plan);
  const tags = new Set(), strings = [], properties = new Set(), built = [], open = { tags: false, attributes: false };
  for (const [file, code] of files) {
    const parsed = parseSync(file, code);
    if (parsed.errors.length) throw new Error(`writers: ${file} did not parse: ${parsed.errors[0].message}`);
    // Each name's binding where it is used, by the bake's model of what binds a name at run time (`walkRuntime`): a
    // scope and the name, as `key`. A `const` holds its initializer there; a function's parameter and a loop's
    // variable are known here; every other binding (`let`, `var`, a pattern, a `catch` parameter, an import) holds
    // nothing this reads, and a name no scope binds is a global.
    const scopes = new Map(), keyOf = (scope, name) => scope ? `${scopes.get(scope) ?? scopes.set(scope, scopes.size).get(scope)}:${name}` : null;
    const values = new Map(), at = new Map(), refs = new Map(), calls = new Map(), params = new Map(), loops = new Map(), owners = new Map();
    const reassigned = new Set(), declared = new Set(), iterated = new Set(), exported = new Set(), helpers = [];
    const add = (map, k, v) => map.set(k, [...map.get(k) ?? [], v]);
    // Every name an assignment's target sets: `x = …`, `[x] = …`, `({ x } = …)`, a loop's head without a declaration.
    const targets = node => !node ? [] : node.type === 'Identifier' ? [node]
      : node.type === 'ArrayPattern' ? node.elements.flatMap(targets)
      : node.type === 'ObjectPattern' ? node.properties.flatMap(p => targets(p.type === 'RestElement' ? p.argument : p.value))
      : node.type === 'AssignmentPattern' ? targets(node.left) : node.type === 'RestElement' ? targets(node.argument) : [];
    walkRuntime(parsed.program, (node, local, parent) => {
      const bound = name => keyOf(local.scope(name), name);
      if (FUNCTIONS.has(node.type)) {
        // Its name: a declaration's, resolved here where only its parameters are nearer (one of the same name hides
        // it); or the `const` it initializes, resolved at that declarator. A function no name holds has no calls to read.
        const name = node.type === 'FunctionDeclaration' ? node.id?.name : owners.get(node)?.name ?? null;
        const own = node.type === 'FunctionDeclaration' ? (name && !node.params.some(p => targets(p).some(id => id.name === name)) ? bound(name) : null)
          : owners.get(node)?.key ?? null;
        if (PLAN_HELPERS.has(name)) helpers.push(node);
        if (node.id) declared.add(node.id.start);
        node.params.forEach((p, i) => {
          for (const t of targets(p)) declared.add(t.start);
          const id = p.type === 'AssignmentPattern' ? p.left : p;
          if (id.type === 'Identifier') params.set(bound(id.name), { name, own, i, fallback: p.right });
        });
        return;
      }
      switch (node.type) {
        case 'VariableDeclarator':
          for (const t of targets(node.id)) declared.add(t.start);
          if (node.id.type !== 'Identifier') return;
          if (parent.kind === 'const' && node.init) values.set(bound(node.id.name), node.init);
          if (FUNCTIONS.has(node.init?.type)) owners.set(node.init, { name: node.id.name, key: bound(node.id.name) });
          return;
        case 'CatchClause': for (const t of targets(node.param)) declared.add(t.start); return;
        case 'ForOfStatement': case 'ForInStatement': {
          if (node.right.type === 'Identifier') iterated.add(node.right.start);
          if (node.left.type !== 'VariableDeclaration') { for (const t of targets(node.left)) reassigned.add(bound(t.name)); return; }
          if (node.left.kind === 'var') return;
          const id = node.left.declarations[0].id, kind = node.type === 'ForOfStatement' ? 'of' : 'in';
          if (id.type === 'Identifier') loops.set(bound(id.name), { kind, of: node.right });
          else if (id.type === 'ArrayPattern') id.elements.forEach((e, index) => { if (e?.type === 'Identifier') loops.set(bound(e.name), { kind, of: node.right, index }); });
          return;
        }
        case 'AssignmentExpression': for (const t of targets(node.left)) reassigned.add(bound(t.name)); return;
        case 'UpdateExpression': if (node.argument.type === 'Identifier') reassigned.add(bound(node.argument.name)); return;
        case 'CallExpression': {
          const walked = objectWalk(node, () => !local.scope('Object'));
          if (walked?.type === 'Identifier') iterated.add(walked.start);
          return;
        }
        // Another module may call what this one exports.
        case 'ExportNamedDeclaration':
          for (const id of node.declaration?.type === 'FunctionDeclaration' ? [node.declaration.id] : (node.declaration?.declarations ?? []).map(d => d.id)) {
            if (id?.type === 'Identifier') exported.add(bound(id.name));
          }
          return;
        case 'ExportDefaultDeclaration': if (node.declaration.id) exported.add(bound(node.declaration.id.name)); return;
        case 'Identifier': {
          // A reference: not a property's or a key's name of the same spelling, nor a declaration's.
          if ((parent?.type === 'MemberExpression' && parent.property === node && !parent.computed)
            || (['Property', 'MethodDefinition', 'PropertyDefinition'].includes(parent?.type) && parent.key === node && !parent.computed && !parent.shorthand)
            || declared.has(node.start)) return;
          const k = bound(node.name);
          if (!k) return;
          at.set(node.start, k);
          add(refs, k, node.start);
          if (parent?.type === 'CallExpression' && parent.callee === node) add(calls, k, parent);
        }
      }
    });
    const inHelper = node => helpers.some(f => node.start >= f.start && node.end <= f.end);
    // Whether a list or an object a binding holds is only walked: nothing adds to it.
    const walkedOnly = k => (refs.get(k) ?? []).every(start => iterated.has(start));
    const valueOf = (node, depth = 0) => {
      if (!node || depth > 8) return null;
      const next = n => valueOf(n, depth + 1);
      if (literal(node) !== null) return { ...none(), names: new Set([literal(node)]) };
      switch (node.type) {
        case 'ConditionalExpression': return join(next(node.consequent), next(node.alternate));
        case 'LogicalExpression': return join(next(node.left), next(node.right));
        case 'ObjectExpression': case 'ArrayExpression': return { ...none(), objects: [node] };
        // `new Set([…])`: the list it holds.
        case 'NewExpression': return node.callee.type === 'Identifier' && node.callee.name === 'Set' && !at.has(node.callee.start) ? next(node.arguments[0]) : null;
        case 'TemplateLiteral': return { ...none(), built: [node.quasis.map(q => q.value.cooked ?? q.value.raw)] };
        case 'BinaryExpression': {
          if (node.operator !== '+') return null;
          const flat = n => n.type === 'BinaryExpression' && n.operator === '+' ? [...flat(n.left), ...flat(n.right)] : [n];
          const pieces = [''];
          for (const n of flat(node)) literal(n) === null ? pieces.push('') : pieces[pieces.length - 1] += literal(n);
          return { ...none(), built: [pieces] };
        }
        // An attribute's name copied from another element (a mutation's `attributeName`, an attribute's `name` in a loop
        // over an element's `attributes`): Exact's code copies only between elements inside the root, where only Exact
        // writes, so the name is one this counts already.
        case 'MemberExpression': {
          if (node.computed) return null;
          if (node.property.name === 'attributeName') return none();
          const k = node.property.name === 'name' && node.object.type === 'Identifier' ? at.get(node.object.start) : null;
          const l = k && !reassigned.has(k) ? loops.get(k) : null;
          return l?.kind === 'of' && l.index === undefined && l.of.type === 'MemberExpression' && !l.of.computed && l.of.property.name === 'attributes' ? none() : null;
        }
        case 'Identifier': {
          const k = at.get(node.start);
          if (!k || reassigned.has(k)) return null;
          const held = v => v?.objects.length && !walkedOnly(k) ? null : v;
          if (values.has(k)) return held(next(values.get(k)));
          const p = params.get(k);
          if (p) {
            if (PLAN_HELPERS.has(p.name)) return none();
            // Read at each call of its function, when its name does nothing but call it here.
            const sites = calls.get(p.own) ?? [];
            if (!p.own || exported.has(p.own) || (refs.get(p.own) ?? []).length !== sites.length) return null;
            const argument = c => c.arguments.slice(0, p.i + 1).some(a => a.type === 'SpreadElement') ? null : c.arguments[p.i] ?? p.fallback;
            return held(all([...sites.map(c => next(argument(c))), ...p.fallback ? [next(p.fallback)] : []]));
          }
          // A loop's variable: a key of an object it walks (`in`, or `of` `Object.keys`/`entries`, the entry's first
          // element), or an element of a list.
          const l = loops.get(k);
          if (!l) return null;
          const keyed = objectWalk(l.of, o => !at.has(o.start));
          const keys = l.kind === 'in' || (keyed && (l.index ?? 0) === 0);
          const source = next(l.kind === 'in' ? l.of : keyed ?? l.of);
          if (!source || (keyed && l.index > 0) || source.names.size || source.built.length) return null;
          return held(all(source.objects.map(o => keys
            ? o.type === 'ObjectExpression' && o.properties.every(q => q.type === 'Property' && !q.computed) ? { ...none(), names: new Set(o.properties.map(q => q.key.name ?? q.key.value)) } : null
            : o.type === 'ArrayExpression' ? all(o.elements.map(e => next(l.index === undefined ? e : e?.type === 'ArrayExpression' ? e.elements[l.index] : null))) : null)));
        }
        default: return null;
      }
    };
    // A value that is only names: no list or object (which the DOM would turn into a string), and no name built
    // without a string of its own.
    const names = v => v && !v.objects.length && v.built.every(pieces => pieces.some(Boolean));
    const read = new Set();
    new Visitor({
      CallExpression(node) {
        const name = node.callee.type === 'MemberExpression' && !node.callee.computed ? node.callee.property.name : null;
        const object = node.callee.object?.type === 'Identifier' ? node.callee.object.name : null;
        if (READS.has(name)) for (const a of node.arguments) read.add(a.start);
        if (MARKUP.has(name) || (object === 'document' && (name === 'write' || name === 'writeln'))) open.tags = open.attributes = true;
        if (name === 'createElement' || name === 'createElementNS') {
          const v = valueOf(node.arguments[name === 'createElement' ? 0 : 1]);
          if (v && !v.built.length && !v.objects.length) for (const t of v.names) tags.add(t.toLowerCase());
          else if (!inHelper(node)) open.tags = true;
        }
        if (Object.hasOwn(SETS, name) && !inHelper(node)) {
          const v = valueOf(node.arguments[SETS[name]]);
          if (names(v)) built.push(...v.built); else open.attributes = true;
        }
        if (object === 'customElements' && name === 'define' && literal(node.arguments[0]) !== null) tags.add(literal(node.arguments[0]));
      },
      NewExpression(node) { if (node.callee.type === 'Identifier' && Object.hasOwn(MADE_BY, node.callee.name)) tags.add(MADE_BY[node.callee.name]); },
      AssignmentExpression(node) {
        const target = node.left.type === 'MemberExpression' ? node.left : null;
        if (!target) return;
        // `e.dataset[k] = v`: a computed `data-*` name.
        if (target.computed && target.object.type === 'MemberExpression' && !target.object.computed && target.object.property.name === 'dataset' && !inHelper(node)) {
          const v = valueOf(target.property);
          if (names(v)) { for (const n of v.names) properties.add(n.toLowerCase()); built.push(...v.built.map(([first, ...rest]) => [`data-${kebab(first)}`, ...rest.map(kebab)])); }
          else open.attributes = true;
        }
        if (target.computed || !MARKUP.has(target.property.name)) return;
        const o = target.object;
        const shadow = (o.type === 'MemberExpression' && !o.computed && o.property.name === 'shadowRoot')
          || (o.type === 'CallExpression' && o.callee.type === 'MemberExpression' && o.callee.property.name === 'attachShadow');
        if (!shadow) open.tags = open.attributes = true;
      },
      Literal(node) { if (typeof node.value === 'string' && !read.has(node.start)) strings.push(node.value); },
      TemplateLiteral(node) { if (!read.has(node.start)) for (const q of node.quasis) strings.push(q.value.cooked ?? q.value.raw); },
      // A reflected attribute set by its property (`e.hidden = true`, `e.ariaSelected = v`, `{ tabIndex: 0 }`).
      MemberExpression(node) { if (!node.computed) properties.add(node.property.name.toLowerCase()); },
      Property(node) { const key = node.key.type === 'Identifier' ? node.key.name : literal(node.key); if (key) properties.add(key.toLowerCase()); },
    }).visit(parsed.program);
  }
  const text = strings.join('\n').toLowerCase();
  const patterns = built.filter(pieces => pieces.some(Boolean)).map(pieces => new RegExp(`^${pieces.map(p => escape(p.toLowerCase())).join('.*')}$`));
  const attribute = name => new RegExp(`(^|[^-\\w])${escape(name)}($|[^-\\w])`).test(text)
    || properties.has(name.replace(/-/g, '')) || (name.startsWith('data-') && properties.has(name.slice(5).replace(/-/g, '')))
    || patterns.some(p => p.test(name));
  return {
    open,
    writes: token => planned.tokens.has(token) || (token.startsWith('t:') ? planned.open.tags || open.tags || tags.has(token.slice(2))
      : (planned.open.data && token.startsWith('a:data-')) || open.attributes || attribute(token.slice(2))),
  };
}

// What the compiled plan writes on its elements, as tokens: each element's tag (`h` and `hs`'s) and every string or
// key it names, which holds each attribute it writes, static (the call's object) or bound (`P`'s name). `open`: it
// writes a name this cannot read (a computed tag, or a bound `dataset`'s words, which are any `data-*`), so every name
// of that kind counts as written.
function planTokens(code) {
  const tokens = new Set(), open = { tags: false, data: false };
  const parsed = parseSync('app.js', code);
  if (parsed.errors.length) throw new Error(`writers: the compiled plan did not parse: ${parsed.errors[0].message}`);
  new Visitor({
    CallExpression(node) {
      const name = node.callee.type === 'Identifier' ? node.callee.name : null;
      if (name === 'h' || name === 'hs') {
        const tag = literal(node.arguments[1]);
        if (tag === null) open.tags = true;
        else tokens.add(`t:${tag.toLowerCase()}`);
      }
      if (name === 'ds') open.data = true;
    },
    // What data can give its elements beside what it writes (style/bound.rs `bound_tokens`).
    VariableDeclarator(node) {
      if (node.id.type === 'Identifier' && node.id.name === 'boundTokens') for (const e of node.init.elements) tokens.add(literal(e));
    },
    Literal(node) { if (typeof node.value === 'string' && node.value.length < 64) tokens.add(`a:${node.value.toLowerCase()}`); },
    TemplateLiteral(node) { for (const part of node.quasis) tokens.add(`a:${(part.value.cooked ?? part.value.raw).toLowerCase()}`); },
    Property(node) { if (node.key?.type === 'Identifier') tokens.add(`a:${node.key.name.toLowerCase()}`); },
  }).visit(parsed.program);
  return { tokens, open };
}
