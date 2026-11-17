// A rendered page's own stylesheet (LLP 1048.007 D4): of the shell's sheet,
// the rules that can match the page's document before its runtime runs.
//
// The build analyzes the sheet once (`analyze`): each rule with what its
// selector needs of a document, as alternatives of tokens (a tag `t:a`, a
// class `c:c7`, an id `i:x`, an attribute name `a:role`), any one of which,
// whole, lets it match. Being needed is a necessary condition, not a
// sufficient one: structure, attribute values and states are ignored, so a
// rule left out matches no element of the page. A renderer keeps the rules
// whose needs the page's tokens meet (`select`, and page.rs `page_style`),
// in the sheet's order, so what a kept rule wins it still wins.

// Present in every document whatever its markup: the parser makes them.
const ALWAYS = new Set(['t:html', 't:head', 't:body', 't:tbody']);
// Set by the browser itself, without the runtime: `open` (a details or a
// dialog an invoker opens), so it needs nothing.
const NATIVE = new Set(['open']);

// The selector list `s` split at top-level commas.
function split(s, by = ',') {
  const out = []; let depth = 0, at = 0, quote = null;
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quote) { if (ch === '\\') i++; else if (ch === quote) quote = null; continue; }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === '\\') i++;
    else if (ch === '(' || ch === '[' || ch === '{') depth++;
    else if (ch === ')' || ch === ']' || ch === '}') depth--;
    else if (ch === by && depth === 0) { out.push(s.slice(at, i)); at = i + 1; }
  }
  out.push(s.slice(at));
  return out;
}

// The alternatives a selector may have before its rule is kept whole rather than read.
const WAYS = 64;

// The alternatives a selector list needs: [[token…]…]; [[]] for none.
export function needs(list) {
  const alts = [];
  for (const complex of split(list)) {
    let options = [[]];
    for (const need of compounds(complex)) options = options.flatMap(o => need.map(n => [...o, ...n]));
    alts.push(...options);
    if (alts.length > WAYS) return [[]]; // past this, keep the rule
  }
  return alts.map(a => [...new Set(a)].filter(t => !ALWAYS.has(t)));
}

// The parts of a complex selector, in order: `{ step }`, a combinator (`' '`, `>`, `+` or `~`); `{ token }`, a tag, a
// class, an id or an attribute name, as `needs` names it; `{ pseudo, inner }`, a pseudo-class or -element with what
// its parentheses hold, if any; or `{}`, a part that needs nothing (`*`, `&`, a `NATIVE` attribute).
function* parts(s) {
  let i = 0;
  const name = () => { const m = /^(?:\\.|[-\w -￿])+/.exec(s.slice(i)); i += m ? m[0].length : 0; return m ? m[0].replace(/\\(.)/g, '$1') : ''; };
  while (i < s.length) {
    const ch = s[i];
    if (/[\s>+~]/.test(ch)) { const run = /^[\s>+~]+/.exec(s.slice(i))[0]; i += run.length; yield { step: run.trim() || ' ' }; continue; }
    if (ch === '&' || ch === '*') { i++; yield {}; continue; }
    if (ch === '.') { i++; yield { token: `c:${name()}` }; continue; }
    if (ch === '#') { i++; yield { token: `i:${name()}` }; continue; }
    if (ch === '[') {
      const end = closing(s, i, '[', ']');
      const attr = /^\[\s*([-\w:|]+)/.exec(s.slice(i))?.[1]?.replace(/^.*\|/, '').toLowerCase();
      i = end + 1;
      yield attr && !NATIVE.has(attr) ? { token: `a:${attr}` } : {};
      continue;
    }
    if (ch === ':') {
      const pseudo = /^::?([-\w]+)/.exec(s.slice(i));
      i += pseudo[0].length;
      let inner = null;
      if (s[i] === '(') { const end = closing(s, i, '(', ')'); inner = s.slice(i + 1, end); i = end + 1; }
      yield { pseudo: pseudo[1].toLowerCase(), inner };
      continue;
    }
    if (/[-\w]/.test(ch)) { yield { token: `t:${name().toLowerCase()}` }; continue; }
    i++; // anything else needs nothing
  }
}

// Each compound of a complex selector, as the alternatives it needs.
function compounds(complex) {
  const out = []; let current = [[]];
  for (const part of parts(complex)) {
    if (part.step) { out.push(current); current = [[]]; }
    else if (part.token) current = current.map(o => [...o, part.token]);
    // One of the list's selectors must match the element (or, for `:has`, a relative): its alternatives join this
    // compound's.
    else if (part.inner !== null && ['is', 'where', 'matches', 'has'].includes(part.pseudo)) {
      const alts = needs(part.inner.replace(/^\s*[>+~]/, ''));
      current = current.flatMap(o => alts.map(a => [...o, ...a]));
    }
  }
  out.push(current);
  return out;
}

// What a selector list needs of elements inside `#exact-root` (LLP 1047.003 D1), as alternatives of tokens, or null
// past `WAYS` of them. An element is inside when the selector puts it below a compound that is the root (by its own
// `#exact-root`), through descendant and child steps, or beside an element already inside; a compound that is the
// root itself, its ancestors, and what `:has` asks are not inside. So a rule whose every alternative here needs a
// token no element inside the root can have matches nothing there.
export function inside(list) {
  const ways = split(list).flatMap(placed);
  return ways.length > WAYS ? null : ways.map(w => [...new Set(w.inside)]);
}

// The ways a complex selector can match, each `{ inside, at, own }`: the tokens it needs of elements inside the root,
// where its subject is (`root`, `in` or `out`), and the subject's own tokens.
function placed(complex) {
  // The compound being read, as its ways: its own tokens, whether it is the root, and what an `:is` in it places.
  const fresh = () => [{ own: [], root: false, inside: [], at: 'out' }];
  let ways = fresh(), current = fresh(), step = null, started = false;
  const flush = () => {
    ways = ways.flatMap(w => current.map(c => {
      const at = c.root ? 'root' : c.at === 'in' ? 'in'
        : (step === ' ' || step === '>') && w.at !== 'out' ? 'in' : (step === '+' || step === '~') && w.at === 'in' ? 'in' : 'out';
      return { inside: [...w.inside, ...c.inside, ...at === 'in' ? c.own : []], at, own: c.own };
    }));
    current = fresh(); started = false;
  };
  for (const part of parts(complex)) {
    if (part.step) { if (started) { flush(); step = part.step; } continue; }
    started = true;
    if (part.token) current = current.map(w => ({ ...w, own: [...w.own, part.token], root: w.root || part.token === 'i:exact-root' }));
    // An `:is` describes this element and its ancestors: its subject's tokens are this compound's.
    else if (part.inner !== null && ['is', 'where', 'matches'].includes(part.pseudo)) {
      const alts = split(part.inner).flatMap(placed);
      current = current.flatMap(w => alts.map(a => ({ own: [...w.own, ...a.own], root: w.root || a.at === 'root', inside: [...w.inside, ...a.inside], at: w.at === 'in' || a.at === 'in' ? 'in' : 'out' })));
    }
  }
  if (started) flush();
  return ways;
}

function closing(s, at, open, close) {
  let depth = 0, quote = null;
  for (let i = at; i < s.length; i++) {
    const ch = s[i];
    if (quote) { if (ch === '\\') i++; else if (ch === quote) quote = null; continue; }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === '\\') i++;
    else if (ch === open) depth++;
    else if (ch === close && --depth === 0) return i;
  }
  return s.length;
}

// The sheet's top-level blocks, each `prelude{body}`.
function blocks(css) {
  const out = []; let depth = 0, at = 0, quote = null, open = -1;
  for (let i = 0; i < css.length; i++) {
    const ch = css[i];
    if (quote) { if (ch === '\\') i++; else if (ch === quote) quote = null; continue; }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === '\\') i++;
    else if (ch === '{') { if (depth++ === 0) open = i; }
    else if (ch === '}' && --depth === 0) { out.push([css.slice(at, open).trim(), css.slice(open + 1, i)]); at = i + 1; }
    else if (ch === ';' && depth === 0) { out.push([css.slice(at, i + 1).trim(), null]); at = i + 1; }
  }
  return out;
}

const GROUPING = /^@(media|supports|container|layer|scope|document)\b/i;

// Whether a rule's body holds only nested rules, no declaration of its own.
const onlyRules = body => {
  const inner = blocks(body);
  return inner.length > 0 && inner.every(([, b]) => b !== null) && !body.slice(body.lastIndexOf('}') + 1).trim();
};

// Every alternative of `a` joined with every one of `b`: a nested rule needs its parent's and its own.
const both = (a, b) => a.flatMap(x => b.map(y => [...new Set([...x, ...y])]));

/** The sheet as rules in order: `[wrap, rule, needs, inside]`, `wrap` the grouping rules around it (`@media …{`, a
 * parent selector's `…{`, each opened), `needs` as `needs` gives, with every parent's, and `inside` as `inside` gives
 * of its selector as nested (null for a rule with no selector). A renderer reads the first three (page.rs). */
export function analyze(css, wrap = '', parent = [[]], scope = null) {
  const out = [];
  // A nested selector's full form: `&` is its parent's, and one without it is below its parent.
  const nest = prelude => scope === null ? prelude
    : prelude.includes('&') ? prelude.replaceAll('&', `:is(${scope})`) : split(prelude).map(p => `:is(${scope}) ${p.trim()}`).join(',');
  for (const [prelude, body] of blocks(css)) {
    if (body === null) out.push([wrap, prelude, parent, null]);
    else if (GROUPING.test(prelude) && !/^@layer\s+[^{]*$/.test(prelude) && blocks(body).length) out.push(...analyze(body, `${wrap}${prelude}{`, parent, scope));
    else if (prelude.startsWith('@')) out.push([wrap, `${prelude}{${body}}`, parent, null]);
    // A selector that only groups nested rules (the app's classes in one `#exact-root#exact-root{…}`, the hover
    // rules under `:where(html:not([data-exact-input="touch"]))`): each nested rule its own, needing the group's too.
    else if (onlyRules(body)) out.push(...analyze(body, `${wrap}${prelude}{`, both(parent, needs(prelude)), nest(prelude)));
    else out.push([wrap, `${prelude}{${body}}`, both(parent, needs(prelude)), inside(nest(prelude))]);
  }
  return out;
}

/** The markup of a page's root: from its `<div id="exact-root"` to the checkpoint script after it, or none. */
export function rootOf(html) {
  const to = html.lastIndexOf('<script type="application/vnd.exact.checkpoint"'), from = to < 0 ? -1 : html.lastIndexOf('<div id="exact-root"', to);
  return from < 0 ? '' : html.slice(from, to);
}

/** The tokens a document's markup offers (as `needs` names them). */
export function tokens(html) {
  const out = new Set();
  for (let i = html.indexOf('<'); i >= 0; i = html.indexOf('<', i + 1)) {
    const tag = /^<([a-zA-Z][-\w]*)/.exec(html.slice(i, i + 64));
    if (!tag) continue;
    out.add(`t:${tag[1].toLowerCase()}`);
    // Its attributes, to the `>` outside a quoted value.
    i += tag[0].length;
    while (i < html.length && html[i] !== '>') {
      const name = /^[\s\/]*([^\s"'=<>\/]+)/.exec(html.slice(i, i + 256));
      if (!name) { i++; continue; }
      i += name[0].length;
      const attr = name[1].toLowerCase();
      let value = '';
      const eq = /^\s*=\s*/.exec(html.slice(i, i + 16));
      if (eq) {
        i += eq[0].length;
        const q = html[i];
        if (q === '"' || q === "'") { const end = html.indexOf(q, i + 1); value = html.slice(i + 1, end < 0 ? html.length : end); i = end < 0 ? html.length : end + 1; }
        else { const m = /^[^\s>]*/.exec(html.slice(i)); value = m[0]; i += value.length; }
      }
      out.add(`a:${attr}`);
      if (attr === 'class') for (const c of value.split(/\s+/).filter(Boolean)) out.add(`c:${c}`);
      if (attr === 'id') out.add(`i:${value}`);
      if (attr === 'style') for (const v of value.matchAll(VAR)) out.add(`v:${v[1]}`);
    }
  }
  return out;
}

const VAR = /var\(\s*(--[-\w]+)/g;
const ROOT = ':root{';

/** The rules of `rules` (`analyze`'s) a document offering `have` can need, as one sheet. A `:root` rule keeps only
 * the custom properties the kept rules or the document's inline styles read (`v:` tokens), and those they read in
 * turn; one left with nothing goes. */
export function select(rules, have) {
  const kept = rules.filter(([, , alts]) => alts.some(a => a.every(t => have.has(t))));
  // The custom properties read, then those their `:root` values read, to a fixed point.
  const read = new Set([...have].filter(t => t.startsWith('v:')).map(t => t.slice(2)));
  for (const [, rule] of kept) if (!rule.startsWith(ROOT)) for (const v of rule.matchAll(VAR)) read.add(v[1]);
  const declared = new Map();
  for (const [, rule] of kept) if (rule.startsWith(ROOT)) for (const d of declarations(rule)) if (d.startsWith('--')) {
    const name = d.slice(0, d.indexOf(':'));
    declared.set(name, (declared.get(name) ?? '') + d);
  }
  for (const queue = [...read]; queue.length;) for (const v of (declared.get(queue.pop()) ?? '').matchAll(VAR)) if (!read.has(v[1])) { read.add(v[1]); queue.push(v[1]); }
  return written(kept, rule => rule.startsWith(ROOT) ? ROOT + declarations(rule).filter(d => !d.startsWith('--') || read.has(d.slice(0, d.indexOf(':')))).join(';') + '}' : rule);
}

/** `rules` (`analyze`'s) as one sheet, each as `text` gives it, its grouping rules opened once for each run they
 * wrap. */
export function written(rules, text = rule => rule) {
  let out = '', open = '';
  for (const [wrap, rule] of rules) {
    const t = text(rule);
    if (t === ROOT + '}') continue;
    if (wrap !== open) { out += '}'.repeat(open.split('{').length - 1) + wrap; open = wrap; }
    out += t;
  }
  return out + '}'.repeat(open.split('{').length - 1);
}

// A `:root{…}` rule's declarations, split at the semicolons outside parentheses.
function declarations(rule) {
  return split(rule.slice(ROOT.length, -1), ';').map(d => d.trim()).filter(Boolean);
}

// The runtime's sheet (LLP 1047.003 D1; LLP 1048.007 D4 as amended): of the whole sheet, the rules an element inside
// `#exact-root` can match. Only Exact writes there, so a rule whose every alternative needs there a tag or an
// attribute that neither the plan nor Exact's code writes (writers.mjs) matches nothing the page shows, before or
// after any action. A rule outside the root stays, so whatever else writes the document (a head script, an
// extension) meets every rule it met before.

/** The tokens `rules` (`analyze`'s) need of elements inside the root that no element there can have, as `writes`
 * says. */
export function absent(rules, writes) {
  const needed = new Set(rules.flatMap(([, , , within]) => within?.flat() ?? []).filter(t => /^[ta]:/.test(t)));
  return new Set([...needed].filter(t => !writes(t)));
}

/** The rules of `rules` (`analyze`'s) but those whose every alternative needs, inside the root, a token of
 * `without`. */
export function planRules(rules, without) {
  return rules.filter(([, , , within]) => !within?.every(a => a.some(t => without.has(t))));
}
