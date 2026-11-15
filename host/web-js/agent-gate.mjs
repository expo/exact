// The release gate of the page's `agent` parameter (LLP 1069.007 D2), apart from glue.mjs so that what imports that
// loads no parser.
import { parseSync } from 'rolldown/utils';

// Gate sources before bundling: Rolldown can simplify a true branch even with constant inlining disabled. Lazy modules
// pass through the same gate (LLP 1069.007 D2). A host module asks the page's `agent` parameter only as the one
// question, guarded in place by the module's own admission,
// `AGENT_ADMITTED && ['', '1'].includes(params.get('agent'))`, where AGENT_ADMITTED is a constant the module itself
// declares, never one it imports, whose value the gate cannot see. The gate writes each one false, so a release asks
// none of its reads. The app's own code (`host: false`) is not the gate's: a lookup of its own named `agent` reads no
// query, and its text is left as written. A classic script is parsed as one (`sourceType: 'script'`).
export function gateAgent(code, name, { host = true, sourceType = 'module' } = {}) {
  if (!host) return code;
  const use = agentUse(code, name, { sourceType });
  if (!use.parsed) {
    throw new Error(`${name} does not parse (${use.error}), so a production build cannot tell whether it reads ?agent (LLP 1069.007 D2); fix its syntax`);
  }
  const unguarded = use.reads.find(read => { const guard = agentGuard(read, use.up); return guard?.type !== 'Identifier' || !use.bound.has(guard.name); });
  if (unguarded) {
    throw new Error(`${name}:${code.slice(0, unguarded.start).split('\n').length} reads ?agent other than as \`AGENT_ADMITTED && ['', '1'].includes(params.get('agent'))\` with an AGENT_ADMITTED it declares, so a production build cannot keep it unasked (LLP 1069.007 D2); ask it that way, or, where the call looks up another collection's \`agent\` key, name that key otherwise`);
  }
  // Write the last value first so that the earlier offsets still hold
  return use.values.reduceRight((out, [start, end]) => out.slice(0, start) + 'false' + out.slice(end), code);
}

const ADMISSION = /^AGENT_ADMITTED(?:\$\d+)?$/;

// What a host module does with the page's `agent` parameter, from its code parsed as a module (or as the classic script
// `sourceType` names), without parentheses and seen through optional chains:
// - `reads`: each call of a method `get`, `getAll` or `has`, named or by a computed string, whose first argument is
//   'agent' as a method that takes a string converts it: a string, a template, a tagged template's strings, an array
//   literal or a spread of one (a string or a comment that names a read reads nothing);
// - `program`, its parse (as far as it goes where the code does not parse), and `up`, which gives a node's holder;
// - `bound`, the AGENT_ADMITTED names it declares, and `values`, the offsets of their values.
// It refuses a module that binds the name in any other form or place than such a declaration or an import under its
// own name, or that uses `with`, so a guard that names a bound constant reads that constant. It sees names and
// arguments written as literals, not one a named constant holds or another expression gives, a spread computed at run
// time, a call through a function value or code built from strings at run time. In the pages the Rust render host
// serves, its default Content-Security-Policy refuses to compile code with `eval` or `Function` or to run an inline
// script it did not hash, unless the app turns it off or widens it (`host.web.csp`, host/render/src/csp.rs); a worker
// made from a Blob, which the policy admits, can still run such code.
export function agentUse(code, name, { sourceType = 'module' } = {}) {
  const { program, errors } = parseSync(name, code, { sourceType, preserveParens: false });
  if (errors.length) return { parsed: false, error: errors[0].message, program, reads: [], bound: new Set(), values: [] };
  const holders = new Map(), use = { parsed: true, program, reads: [], up: node => holders.get(node), bound: new Set(), values: [] };
  const refuse = what => {
    throw new Error(`${name} ${what}; a guard must read the one constant a production build writes false (LLP 1069.007 D2), so declare it once at the top of the module, as \`const AGENT_ADMITTED = true\``);
  };
  const topLevel = declaration => holders.get(declaration)?.type === 'Program'
    || holders.get(declaration)?.type === 'ExportNamedDeclaration' && holders.get(holders.get(declaration))?.type === 'Program';
  // A string's value, a template's without substitutions included
  const text = node => node?.type === 'Literal' ? node.value
    : node?.type === 'TemplateLiteral' && !node.expressions.length ? node.quasis[0].value.cooked
    : node?.type === 'ArrayExpression' ? joined(node.elements) : undefined;
  // An array literal as a string, its elements, spreads of array literals opened, joined by commas, where each is known
  // (ECMAScript, Array.prototype.join)
  const joined = elements => {
    const parts = opened(elements)?.map(element => element === null || element.type === 'Literal' && element.value === null ? '' : text(element));
    return parts?.every(part => ['string', 'number', 'boolean'].includes(typeof part)) ? parts.join(',') : undefined;
  };
  const opened = elements => {
    const out = [];
    for (const element of elements) {
      if (element?.type !== 'SpreadElement') out.push(element);
      else if (element.argument.type !== 'ArrayExpression') return undefined;
      else {
        const inner = opened(element.argument.elements);
        if (!inner) return undefined;
        out.push(...inner);
      }
    }
    return out;
  };
  const method = callee => callee.type === 'MemberExpression' && (callee.computed ? text(callee.property) : callee.property.name);
  const asks = callee => ['get', 'getAll', 'has'].includes(method(unchained(callee)));
  // The string a call passes first, through spreads of array literals, a spread that passes none skipped; undefined
  // where it passes none, passes no string, or passes a spread computed at run time, as `has(key)` passes a name
  const first = args => {
    for (const argument of args) {
      if (argument?.type !== 'SpreadElement') return { value: text(argument) };
      if (argument.argument.type !== 'ArrayExpression') return {};
      const passed = first(argument.argument.elements);
      if (passed) return passed;
    }
  };
  // Each name a binding place binds
  const names = place => !place ? [] : place.type === 'Identifier' ? [place.name]
    : place.type === 'ObjectPattern' ? place.properties.flatMap(property => names(property.type === 'RestElement' ? property.argument : property.value))
    : place.type === 'ArrayPattern' ? place.elements.flatMap(element => names(element))
    : place.type === 'AssignmentPattern' ? names(place.left) : place.type === 'RestElement' ? names(place.argument) : [];
  const binds = (places, what) => { for (const bound of places.flatMap(names)) if (ADMISSION.test(bound)) refuse(what(bound)); };
  const walk = (node, holder) => {
    if (Array.isArray(node)) return node.forEach(item => walk(item, holder));
    if (!node || typeof node !== 'object') return;
    if (typeof node.type === 'string') holders.set(node, holder);
    // A tagged template passes its strings, which a method that takes a string reads as their one string
    if (node.type === 'CallExpression' && asks(node.callee) && first(node.arguments)?.value === 'agent'
      || node.type === 'TaggedTemplateExpression' && asks(node.tag) && text(node.quasi) === 'agent') {
      use.reads.push(node);
    }
    if (node.type === 'VariableDeclarator' && node.id.type === 'Identifier' && ADMISSION.test(node.id.name)) {
      if (holder.kind !== 'const' || node.init?.type !== 'Literal' || typeof node.init.value !== 'boolean') refuse(`declares ${node.id.name} other than as a const true or false`);
      if (!topLevel(holder)) refuse(`declares ${node.id.name} inside a function or a block`);
      use.values.push([node.init.start, node.init.end]);
      use.bound.add(node.id.name);
    } else if (node.type === 'VariableDeclarator') binds([node.id], bound => `binds ${bound} in a pattern`);
    if (['FunctionDeclaration', 'FunctionExpression', 'ArrowFunctionExpression'].includes(node.type)) {
      binds([node.id, ...node.params], bound => `binds ${bound} as a function or a parameter`);
    }
    if (['ClassDeclaration', 'ClassExpression'].includes(node.type)) binds([node.id], bound => `declares ${bound} as a class`);
    if (node.type === 'CatchClause') binds([node.param], bound => `binds ${bound} as a caught error`);
    if (node.type === 'WithStatement') {
      throw new Error(`${name} uses \`with\`, under which a guard can read an object's property instead of its constant (LLP 1069.007 D2); write it without \`with\``);
    }
    if (node.type?.startsWith('Import') && ADMISSION.test(node.local?.name ?? '')
      && (node.type !== 'ImportSpecifier' || !ADMISSION.test(node.imported.name ?? node.imported.value))) refuse(`imports another binding as ${node.local.name}`);
    const exported = node.exported?.name ?? node.exported?.value ?? '';
    if (ADMISSION.test(exported) && !(node.type === 'ExportSpecifier' && ADMISSION.test(node.local.name ?? node.local.value))) {
      refuse(`exports another binding as ${exported}`);
    }
    for (const value of Object.values(node)) walk(value, node.type === 'ChainExpression' ? holder : node);
  };
  walk(program);
  return use;
}

// Where a read is the one question, `['', '1'].includes(params.get('agent'))`, the first operand of the `&&` chain
// whose right operand the question is, which keeps the question unasked while it is false: null where no `&&` holds the
// question so, and undefined where the read is not the question
export function agentGuard(read, up) {
  const callee = unchained(read.callee), question = up(read), argument = unchained(question?.arguments?.[0]);
  if (read.type !== 'CallExpression' || callee.computed || callee.property.name !== 'get' || read.arguments.length !== 1
    || read.arguments[0].type !== 'Literal' || read.arguments[0].value !== 'agent' || question?.type !== 'CallExpression' || question.arguments.length !== 1
    || argument !== read || question.callee.type !== 'MemberExpression' || question.callee.computed
    || question.callee.property.name !== 'includes' || question.callee.object.type !== 'ArrayExpression' || question.callee.object.elements.length !== 2
    || !question.callee.object.elements.every((element, i) => element?.type === 'Literal' && element.value === ['', '1'][i])) {
    return undefined;
  }
  const chain = up(question);
  if (chain?.type !== 'LogicalExpression' || chain.operator !== '&&' || chain.right !== question) return null;
  let first = chain.left;
  while (first.type === 'LogicalExpression' && first.operator === '&&') first = first.left;
  return first;
}

const unchained = node => node?.type === 'ChainExpression' ? node.expression : node;
