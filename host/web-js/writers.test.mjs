import { expect, test } from 'bun:test';
import { writers } from './writers.mjs';

// LLP 1047.003 D1: what the plan and Exact's code can write inside the root, read from their text. Whatever is not
// read counts as written, so each case below that widens is one the sheet then keeps.
const PLAN = 'import{P,h,hs,mount}from"./rt.js";export default function(){mount($R=>{const e0=h($R,"button",1,{"data-exact-on":"press"},"Go");'
  + 'P(e0,"aria-selected",()=>"true");const e1=hs(e0,"svg",0,{"viewBox":"0 0 1 1"},0);});}';
const writes = (code, plan = PLAN) => writers(plan, [['m.js', code]]);

test("the plan's elements and Exact's createElement calls are the tags made; a tag this cannot read makes every tag count", () => {
  const plain = writes(`
    const svg = name => document.createElementNS('http://www.w3.org/2000/svg', name);
    svg('clipPath'); svg('path');
    document.createElement(wide ? 'span' : 'div');
    document.createElement('select', {});
    document.createElementNS('http://www.w3.org/1999/xhtml', 'summary', {});
    new Image();
    customElements.define('exact-caret-twin', class extends HTMLElement {});
    export function h(p, tag) { return document.createElement(tag); }
    host.attachShadow({ mode: 'open' }).innerHTML = '<dialog></dialog>';`);
  const made = t => plain.writes(`t:${t}`);
  expect(['button', 'svg', 'clippath', 'path', 'span', 'div', 'select', 'summary', 'img', 'exact-caret-twin'].filter(t => !made(t))).toEqual([]);
  // What this cannot trace to the code makes every tag count: a default export another module may call, a name set
  // again (by any assignment), a list something adds to, a property that only spells a copy.
  for (const source of [
    'export default function make(tag) { return document.createElement(tag); }',
    "function make(tag, next) { tag = next; return document.createElement(tag); } make('span', data.tag);",
    "function make(tag, next) { [tag] = [next]; return document.createElement(tag); } make('span', data.tag);",
    "const tags = ['span']; tags.push(data.tag); for (const tag of tags) document.createElement(tag);",
    "const tags = ['span']; Object.assign(tags, { 1: data.tag }); for (const tag of tags) document.createElement(tag);",
    "const config = { tagName: data.tag }; document.createElement(config.tagName);",
    // A name is its binding's, as JavaScript scopes it: a destructured `const`, a repeated `var`, a `var` in a loop.
    "const tag = 'span'; { const { tag } = data; document.createElement(tag); }",
    "var tag = 'span'; var tag = data.tag; document.createElement(tag);",
    "const tag = 'span'; function make() { for (var tag of data.tags) {} document.createElement(tag); } make();",
    "const tags = ['span']; const keys = 'assign'; Object[keys](tags, { 1: data.tag }); for (const tag of tags) document.createElement(tag);",
    // Builtins by their binding, not their spelling; a list is no name; a loop's list holds the same rule.
    "const Object = { keys: list => list.push(data.tag) }; const tags = ['span']; Object.keys(tags); for (const tag of tags) document.createElement(tag);",
    "const Set = function (list) { return [...list, data.tag]; }; const tags = new Set(['span']); for (const tag of tags) document.createElement(tag);",
    "document.createElement(['select']);",
    "for (const tags of [['span']]) { tags.push(data.tag); for (const tag of tags) document.createElement(tag); }",
  ]) expect([source, writes(source).open.tags]).toEqual([source, true]);
  // A named function expression is called by the name that holds it.
  expect(writes("const make = function make(tag) { return document.createElement(tag); }; make('select');").writes('t:select')).toBe(true);
  // A name is its binding's: the inner `tag` is `next`, read at the call.
  expect(writes("function make(tag, next) { if (tag === 'span') { const tag = next; return document.createElement(tag); } } make('span', 'select');").writes('t:select')).toBe(true);
  expect(['dialog', 'input', 'time'].filter(made)).toEqual([]);
  expect(plain.open).toEqual({ tags: false, attributes: false });
  // A function whose name is also passed as a value is not read at its calls; text made into the document is.
  expect(writes(`const make = t => document.createElement(t); make('a'); list.map(make);`).open).toEqual({ tags: true, attributes: false });
  const markup = writes(`el.innerHTML = html;`);
  expect([markup.writes('t:dialog'), markup.writes('a:data-listrows'), markup.open]).toEqual([true, true, { tags: true, attributes: true }]);
});

test("an attribute is written where Exact's code names it outside a read, sets it by its property, or builds a name it could be", () => {
  const code = writes(`
    el.setAttribute('role', 'tab');
    if (el.matches('[data-listrows]') && el.getAttribute('data-liststyle')) el.removeAttribute('swipecontent');
    el.ariaChecked = 'true';
    el.dataset.buttonStyle = 'plain';
    el.tabIndex = 0;
    el.setAttribute('aria-' + kind, '');
    const MARK = 'data-exact-field';
    const set = (name, value) => value && el.setAttribute(name, value);
    set('aria-label', label); set('title', label);
    const booleans = new Set(['muted', 'loop']);
    for (const name of booleans) el.toggleAttribute(name, true);
    for (const key in { 'data-exact-width': 1 }) el.setAttribute(key, '');
    for (const attr of other.attributes) el.setAttribute(attr.name, attr.value);
    for (const m of records) el.setAttribute(m.attributeName, m.oldValue);
    el.setAttribute(MARK, '');
    cache.set(key, label);
    function adopt(e, attrs) { for (const k in attrs) e.setAttribute(k, attrs[k]); }
    export function h(p, tag, cls, attrs) { const e = document.createElement(tag); adopt(e, attrs); return e; }
    export function P(e, name, f) { e.setAttribute(name, f()); }`);
  const written = ['role', 'aria-checked', 'data-button-style', 'tabindex', 'aria-invalid', 'aria-label', 'title', 'muted', 'loop',
    'data-exact-width', 'data-exact-field', 'data-exact-on', 'aria-selected', 'viewbox'];
  expect([written.filter(a => !code.writes(`a:${a}`)), ['data-listrows', 'data-liststyle', 'swipecontent', 'data-destructive'].filter(a => code.writes(`a:${a}`))])
    .toEqual([[], []]);
  expect(code.open).toEqual({ tags: false, attributes: false });
  // A name the build cannot trace to the code (data from elsewhere) makes every attribute count.
  for (const source of ['for (const [k, v] of decoded.attrs) line.setAttribute(k, v);', 'el.dataset[key] = v;', 'export function mark(el, name) { el.setAttribute(name, ""); }',
    // A `const` is its block's: the second `name` is the data's, not the first block's literal.
    "if (d) { const name = 'role'; el.setAttribute(name, ''); } else { const name = d.key; el.setAttribute(name, ''); }",
    // A name built with no string of its own, and a default export another module may call.
    'export function mark(el, name) { el.setAttribute(`${name}`, ""); }', 'export default function mark(el, name) { el.setAttribute(name, ""); }',
    // A copy's variable set again is no copy.
    'for (let attr of other.attributes) { attr = { name: data.name }; el.setAttribute(attr.name, ""); }']) {
    const traced = writes(source);
    expect([traced.writes('a:data-listrows'), traced.open]).toEqual([true, { tags: false, attributes: true }]);
  }
});

test("the tags data can give the plan's elements count as made; a computed tag or a bound dataset makes every name of its kind count", () => {
  const plain = writes('');
  expect([plain.writes('t:dialog'), plain.writes('a:data-listrows')]).toEqual([false, false]);
  const bound = writes('', 'import{h}from"./rt.js";export const boundTokens=["a:data-exact-switch","t:h1","t:h5"];h(p,"h2",0,0,0);');
  expect(['t:h1', 't:h5', 'a:data-exact-switch', 't:dialog', 'a:data-exact-field'].map(bound.writes)).toEqual([true, true, true, false, false]);
  for (const [plan, tag, data] of [
    ['import{h}from"./rt.js";h(p,tag,0,0,0);', true, false],
    ['import{h,ds}from"./rt.js";const e0=h(p,"div",0,0,0);ds(e0,j);', false, true],
  ]) {
    const w = writes('', plan);
    expect([w.writes('t:dialog'), w.writes('a:data-listrows'), w.writes('a:aria-checked')]).toEqual([tag, data, false]);
  }
});
