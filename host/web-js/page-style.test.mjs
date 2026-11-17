import { expect, test } from 'bun:test';
import { absent, analyze, inside, planRules, select, tokens, written } from './page-style.mjs';

// The same sheet and documents as page_style.rs's test: the two renderers select alike (LLP 1048.007 D4).
const CSS = ':root{--a:red;--b:var(--a);--c:blue;--d:green}p{color:var(--b)}@media (hover:hover){:where(html:not([data-x])){button:hover{color:var(--c)}dialog{color:red}}}.c9{x:1}';

test('a page keeps the rules its markup can meet, and the custom properties they read', () => {
  const rules = analyze(CSS);
  expect(JSON.stringify(rules.map(rule => rule.slice(0, 3)))).toBe('[["",":root{--a:red;--b:var(--a);--c:blue;--d:green}",[[]]],["","p{color:var(--b)}",[["t:p"]]],["@media (hover:hover){:where(html:not([data-x])){","button:hover{color:var(--c)}",[["t:button"]]],["@media (hover:hover){:where(html:not([data-x])){","dialog{color:red}",[["t:dialog"]]],["",".c9{x:1}",[["c:c9"]]]]');
  expect(select(rules, tokens('<p title="a>b">x</p><button>y</button>'))).toBe(':root{--a:red;--b:var(--a);--c:blue}p{color:var(--b)}@media (hover:hover){:where(html:not([data-x])){button:hover{color:var(--c)}}}');
  expect(select(rules, tokens('<p style="margin:var(--d)">x</p>'))).toBe(':root{--a:red;--b:var(--a);--d:green}p{color:var(--b)}');
  expect(select(rules, tokens('<b class="c9">'))).toBe('.c9{x:1}');
});

// LLP 1047.003 D1 (page-style.mjs `inside`).
test("a plan's sheet leaves out the rules that need, inside the root, a tag or an attribute nothing writes", () => {
  expect([
    '#exact-root dialog', '#exact-root>div input', '#exact-root a~[data-x]', ':where(#exact-root dialog)::backdrop',
    '#exact-root ~ dialog', 'body:has(#exact-root) input', '#exact-root:has(+ dialog)', '#exact-root[data-x]', 'dialog', 'p, #exact-root q',
  ].map(inside)).toEqual([
    [['t:dialog']], [['t:div', 't:input']], [['t:a', 'a:data-x']], [['t:dialog']],
    [[]], [[]], [[]], [[]], [[]], [[], ['t:q']],
  ]);
  const rules = analyze(':root{--a:red}#exact-root button[data-button-style]{color:var(--a)}#exact-root :is(input,select){x:1}'
    + 'input{x:2}#exact-root ~ dialog{x:3}@media (hover:hover){#exact-root dialog{x:4}}#exact-root{&>[aria-selected]{x:5}[role=tab]{x:6}}');
  // Nested: `&` stands for its parent, and a selector without one is below it.
  expect(rules.map(([, , , within]) => within)).toEqual([[[]], [['t:button', 'a:data-button-style']], [['t:input'], ['t:select']], [[]], [[]], [['t:dialog']],
    [['a:aria-selected']], [['a:role']]]);
  // As if the plan and Exact's code write `button`, `select` and `role`, and nothing else: the rules that need
  // something else inside the root go; `input{}` and the root's sibling's stay, and so does `:root`.
  const made = new Set(['t:button', 't:select', 'a:role']);
  const without = absent(rules, token => made.has(token));
  expect([...without].sort()).toEqual(['a:aria-selected', 'a:data-button-style', 't:dialog', 't:input']);
  expect(written(planRules(rules, without))).toBe(':root{--a:red}#exact-root :is(input,select){x:1}input{x:2}#exact-root ~ dialog{x:3}#exact-root{[role=tab]{x:6}}');
  // A page of that plan selects from the plan's sheet as from the whole one.
  const page = tokens('<div id="exact-root"><select role="tab"></select></div>');
  expect(select(planRules(rules, without), page)).toBe(select(rules, page));
});
