// The bake's syntax checks: package portability, code-from-string warnings, ambient calls and the packages app.ts imports.
import { expect, test } from 'bun:test';
import { parseSync } from 'rolldown/utils';
import { packageRefusals } from '../src/__exact_portable.mjs';
import { ambientRefusals, usedPackages } from '../src/typescript.mjs';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const check = (code, lang) => {
  const warnings = [];
  const refused = packageRefusals(code, (at) => `x${at}`, (c, o) => parseSync(`x.${lang}`, c, o).program, warnings, lang);
  return { refused, warnings };
};

// The refusals of package JavaScript, each as where and what: DOM and unshimmed Node globals
// (`process` is shimmed per LLP 1195.000 D1.3; `try { ... } catch` guards absent-global probes
// per D1.1; computed `require(expr)` and `import(expr)` stay refused even inside `try/catch`).
const REFUSALS = [
  ["process.cwd();", []],
  ["const Function = globalThis.Buffer; Function.from('');", ["1:37 Buffer"]],
  ["var Function = globalThis.Buffer; Function.from('');", ["1:16 Buffer"]],
  ["function f() { const eval = globalThis.document; return eval.body; }", ["1:57 document"]],
  ["function f(Buffer) { return Buffer.from(''); }", []],
  ["function f() { function g() { var Buffer = {}; } return Buffer.from(''); }", ["1:57 Buffer"]],
  ["class C { static { var Buffer = {}; Buffer.from(''); } }", ["1:24 Buffer", "1:37 Buffer"]],
  ["class C { static { let Buffer = {}; Buffer.from(''); } }", []],
  ["class C { static { var Buffer = {}; } m() { return Buffer.from(''); } }", ["1:24 Buffer", "1:52 Buffer"]],
  ["try { 0; } catch (document) { document.body; }", []],
  ["try { localStorage.getItem('k'); } catch {}", []],
  ["try { require(name); } catch {}", ["1:7 require"]],
  ["try { import(name); } catch {}", ["1:7 import()"]],
  ["for (const window of []) window.x;", []],
  ["for (var window of []) window.x;", []],
  ["out: { var require = () => 0; } require(name);", []],
  ["switch (1) { case 1: const navigator = {}; navigator.x; }", []],
  ["const f = (a = 1, location = a) => location.href;", []],
  ["const f = (a = location, location = 1) => a;", []],
  ["class C { read = (document) => document.body; }", []],
  ["class C { read = document.body; }", ["1:18 document"]],
  ["{ function Buffer() {} Buffer.from(''); }", []],
  ["globalThis.document.body;", ["1:1 document"]],
  ["typeof window !== 'undefined' && window.x;", []],
  ["import(name);", ["1:1 import()"]],
  ["import('./x.js');", []],
  ["require(name);", ["1:1 require"]],
  ["require('x');", []],
  ["process.env.NODE_ENV; process.env.DEBUG; process.versions.node; process.release.name;", []],
  ["const g = globalThis.document; g.body;", ["1:32 document"]],
  ["const d = typeof document !== 'undefined' ? document : null;", []],
  ["new Function('return 1');", []],
  ["eval('1'); globalThis.eval('1'); Function.prototype.bind;", []],
  ["const F = Function; new F('');", []],
  ["x instanceof Function;", []],
  ["var Function = 1; function f() { return Function; }", []],
  ["function eval2() { var eval = 1; } eval('x');", []],
  ["const { Buffer } = globalThis; Buffer.from('');", []],
  ["let [document] = []; document.body;", []],
  ["(function document() { return document.body; })();", []],
  ["(class window { m() { return window.x; } });", []],
  ["export { Buffer } from './x.js'; Buffer.from('');", ["1:34 Buffer"]],
  ["import { Buffer } from 'buffer'; Buffer.from('');", []],
  ["label: for (;;) { const __dirname = 1; __dirname; break label; } __filename;", ["1:66 __filename"]],
];

test('package refusals stay as they were decided, whatever the warnings\' scopes', () => {
  const said = REFUSALS.map(([code]) => [code, check(code, 'js').refused.map((why) => why.replace(/^x:(\d+:\d+) .*?(?:'([\w()]+)'.*|computed name.*)$/, (_, at, name) => `${at} ${name ?? 'require'}`))]);
  expect(said).toEqual(REFUSALS);
});

// Where a use of `Function` is the global, and so warned: each row's code, then the lines of the uses warned. A scope
// that binds the name hides it there alone. TypeScript emits a namespace as a function, so its block is a `var` scope;
// its blocks see each other's exports, never their other names, and an enum, class or function merged with it adds
// nothing to its scope. What TypeScript erases (`declare`, a type-only namespace) hides nothing.
const LOCAL = '(_: string) => 0';
const SCOPES = [
  ['a function body\'s var', `function f() { var Function = ${LOCAL}; return Function(''); }\nFunction('');`, [2]],
  ['a nested function\'s var stays there', `function f() { function g() { var Function = ${LOCAL}; } return Function(''); }`, [1]],
  ['a sloppy block\'s function, also the function\'s (Annex B)', `function f() { { function Function() {} } return Function(''); }`, []],
  ['a sloppy block\'s function, also the script\'s', `{ function Function() {} }\nFunction('');`, []],
  ['a module block\'s function stays there', `export function f() { { function Function() {} } return Function(''); }`, [1]],
  ['a strict function block\'s function stays there', `function f() { 'use strict'; { function Function() {} } return Function(''); }`, [1]],
  ['a class method block\'s function stays there', `class C { m() { { function Function() {} } return Function(''); } }`, [1]],
  ['a sloppy block\'s generator stays there', `function f() { { function* Function() {} } return Function('return 7')(); }`, [1]],
  ['a sloppy block\'s async function stays there', `function f() { { async function Function() {} } return Function('return 7')(); }`, [1]],
  ['a sloppy block\'s function under a block\'s let stays there', `function f() { { let Function; { function Function() {} } } return Function('return 7')(); }`, [1]],
  ['the module\'s var', `var Function = ${LOCAL};\nFunction('');`, []],
  ['a class static block\'s var', `class C { static { var Function = ${LOCAL}; Function(''); } }\nFunction('');`, [2]],
  ['a class static block\'s let', `class C { static { let Function = ${LOCAL}; Function(''); } }\nFunction('');`, [2]],
  ['a catch parameter', `try { 0; } catch (Function: any) { Function(''); }\nFunction('');`, [2]],
  ['a for head\'s const', `for (const Function of [${LOCAL}]) Function('');\nFunction('');`, [2]],
  ['a for head\'s var, the function\'s', `function f() { for (var Function of [${LOCAL}]) 0; return Function(''); }\nFunction('');`, [2]],
  ['a labelled block\'s const', `out: { const Function = ${LOCAL}; Function(''); }\nFunction('');`, [2]],
  ['a switch block\'s const', `switch (0) { case 0: const Function = ${LOCAL}; Function(''); }\nFunction('');`, [2]],
  ['a switch discriminant, outside its cases\' bindings', `switch (Function('')) { case 0: const Function = ${LOCAL}; }`, [1]],
  ['an enum\'s member, in a later initializer', `enum E { Function = 1, n = (Function as any)('') }\nFunction('');`, [2]],
  ['an arrow\'s parameter, a default naming an earlier one', `const f = (a = 1, Function = (_: string) => a) => Function('');\nFunction('');`, [2]],
  ['a class field\'s arrow parameter', `class C { read = (Function: (s: string) => number) => Function(''); }\nFunction('');`, [2]],
  ['a class field\'s initializer', `class C { read = Function(''); }`, [1]],
  ...['var', 'let', 'const'].flatMap((kind) => ['', 'export '].map((exported) =>
    [`a namespace's ${exported}${kind}`, `namespace N { ${exported}${kind} Function = ${LOCAL}; Function(''); }\nFunction('');`, [2]])),
  ...['', 'export '].flatMap((exported) => [
    [`a namespace's ${exported}function`, `namespace N { ${exported}function Function(_: string) { return 0; } Function(''); }\nFunction('');`, [2]],
    [`a namespace's ${exported}class`, `namespace N { ${exported}class Function { constructor(_: string) {} } new Function(''); }\nFunction('');`, [2]],
  ]),
  ['a labelled var in a nested block of a namespace', `namespace N { out: { { var Function = ${LOCAL}; } } Function(''); }\nFunction('');`, [2]],
  ['a merged namespace\'s export', `namespace N { export const Function = ${LOCAL}; }\nnamespace N { Function(''); }\nFunction('');`, [3]],
  ['a merged namespace\'s own name stays in its block', `namespace N { const Function = ${LOCAL}; }\nnamespace N { Function(''); }`, [2]],
  ['a dotted namespace sees its parent\'s exports', `namespace A { export const Function = ${LOCAL}; }\nnamespace A.B { Function(''); }\nFunction('');`, [3]],
  ['a nested namespace it does not export is not the dotted one', `namespace A { namespace B { export const Function = ${LOCAL}; } }\nnamespace A.B { Function(''); }`, [2]],
  ['an enum\'s member, in a namespace merged with it', `enum N { Function = 1 }\nnamespace N { Function(''); }`, [2]],
  ['a namespace merged with an enum', `enum N { A = 1 }\nnamespace N { export const Function = ${LOCAL}; }\nnamespace N { Function(''); }`, []],
  ['a namespace merged with a class', `class N { static Function = 1; }\nnamespace N { Function(''); }`, [2]],
  ['a type-only namespace', `namespace Function { export type T = number; }\nFunction('');`, [2]],
  ['declare global', `declare global { var Function: (s: string) => number; }\nFunction('');\nexport {};`, [2]],
  ['import X = require()', `import Function = require('x');\nFunction('');`, []],
  ['import X = N.Y', `namespace N { export const Y = ${LOCAL}; }\nimport Function = N.Y;\nFunction('');`, []],
  ['a const enum', `const enum Function { A }\nFunction('');`, [2]],
];

test('a use of Function is warned where no scope binds the name at run time', () => {
  const said = SCOPES.map(([scope, code]) => [scope, check(code, 'ts').warnings.map((w) => Number(/^x:(\d+):/.exec(w)[1]))]);
  expect(said).toEqual(SCOPES.map(([scope, , lines]) => [scope, lines]));
});

test('destructuring a global reads its code constructor at the property', () => {
  const cases = [
    ["const { Function: compile } = globalThis; compile(source);", ['1:9 Function']],
    ["const { eval: execute } = self; execute(source);", ['1:9 eval']],
    ["const { Function } = globalThis; Function(source);", ['1:9 Function']],
    ["let compile; ({ Function: compile } = globalThis);", ['1:17 Function']],
    ["const { ['Function']: compile, [`eval`]: execute } = globalThis;", ['1:10 Function', '1:33 eval']],
    ["function run({ Function: compile } = globalThis) { return compile(source); }", ['1:16 Function']],
    ["const { Function: compile = fallback } = (globalThis);", ['1:9 Function']],
    ["const { 'Function': compile } = globalThis;", ['1:9 Function']],
    ["const { nested: { Function: compile } } = globalThis;", []],
    ["const { [key]: compile } = globalThis;", []],
    ["const { Function: compile } = local;", []],
    ["function run(globalThis) { const { Function: compile } = globalThis; }", []],
    ["function run(self) { const { eval: execute } = self; }", []],
    ["type T = { Function: typeof Function };", []],
  ];
  for (const [code, sites] of cases) {
    const { refused, warnings } = check(code, 'ts');
    expect([code, refused, warnings.map(w => w.replace(/^x:(\d+:\d+) warning \[code-from-string\] uses '([^']+)'.*$/, '$1 $2'))])
      .toEqual([code, [], sites]);
  }
});

test('parameter initializers see parameters and outer bindings before the body scope', () => {
  const cases = [
    ['function run(make = Function) { const Function = 1; return make(source); }', 1],
    ['function run(make = Function) { var Function = 1; return make(source); }', 1],
    ['function run(make = Function) { function Function() {} return make(source); }', 1],
    ['function run(make = Function) { class Function {} return make(source); }', 1],
    ['const run = (make = Function) => { let Function = 1; return make(source); };', 1],
    ['function run(make = eval) { const eval = 1; return make(source); }', 1],
    ['function run(make = globalThis.Function) { var globalThis = {}; }', 1],
    ['function run({ Function: make } = globalThis) { var globalThis = {}; }', 1],
    ['function run(make = () => Function(source)) { var Function = 1; }', 1],
    ['function run(Function, make = Function) { return make(source); }', 0],
    ['function run(make = Function, Function = local) { return make(source); }', 0],
    ['function run({ Function }, make = Function) { return make(source); }', 0],
    ['const run = function Function(make = Function) { return make(source); };', 0],
    ['const Function = local; function run(make = Function) { var Function = 1; }', 0],
    ['function outer() { var Function = local; function run(make = Function) { var Function = 1; } }', 0],
    ['function run(make = local) { { var Function = local; } return Function(source); }', 0],
    ['function run(make = local) { return () => Function(source); var Function; }', 0],
    ['function run(make: typeof Function = local) { const Function = 1; }', 0],
    ['function run(make = typeof Function) { var Function; }', 0],
    ['function run(make = typeof globalThis.Function) { var globalThis; }', 0],
    ['function run(make = typeof (globalThis as any).eval) { var globalThis; }', 0],
  ];
  for (const [code, count] of cases) expect([code, check(code, 'ts').warnings.length]).toEqual([code, count]);
});

test('JSX component names refer to values once, while tags and attributes do not', () => {
  const cases = [
    ['<Function />;', 1],
    ['<Function></Function>;', 1],
    ['<Function.Component></Function.Component>;', 1],
    ['<globalThis.Function />;', 1],
    ['<self.eval />;', 1],
    ['Function.prototype.bind;', 0],
    ['self.eval.prototype.bind;', 0],
    ['<Function.prototype.bind />;', 0],
    ['<self.eval.prototype.bind />;', 0],
    ['<globalThis.Function.prototype.call></globalThis.Function.prototype.call>;', 0],
    ['<eval.prototype.apply />;', 0],
    ['Function.prototype.constructor;', 1],
    ['self.eval.prototype.constructor;', 1],
    ['<Function.prototype.constructor />;', 1],
    ['<self.eval.prototype.constructor />;', 1],
    ['<eval />;', 0],
    ['<Function:tag />;', 0],
    ['<div Function="eval" />;', 0],
    ['<Function Function={Function} />;', 2],
    ['<components.Function />;', 0],
    ['const Function = local; <Function />;', 0],
    ['function run(Function) { return <Function />; }', 0],
    ['function run(globalThis) { return <globalThis.Function />; }', 0],
    ['import type { Function } from "x"; <Function />;', 1],
    ['type Function = () => void; <Function />;', 1],
  ];
  for (const lang of ['jsx', 'tsx']) {
    for (const [code, count] of cases) {
      if (lang === 'jsx' && /^(import type|type )/.test(code)) continue;
      expect([lang, code, check(code, lang).warnings.length]).toEqual([lang, code, count]);
    }
  }
});

test('ambient timer refusals resolve bindings in their lexical scope (LLP 1195.000 D2.8)', () => {
  const fn = '(_: () => void) => 1';
  const cases = [
    ['an unshadowed call', 'export const tick = () => setInterval(() => {}, 1);', [1]],
    ['an unrelated parameter', 'export function f(setInterval: number) { return setInterval; }\nexport const tick = () => setInterval(() => {}, 1);', [2]],
    ['an unrelated namespace var', 'namespace N { var setInterval = 1 }\nexport const tick = () => setInterval(() => {}, 1);', [2]],
    ['a local parameter', 'export const tick = (setInterval: (f: () => void) => number) => setInterval(() => {});', []],
    ['a nested function', `export function f() { function g() { var setInterval = ${fn}; } return setInterval(() => {}); }`, [1]],
    ['a block binding', `{ const setInterval = ${fn}; setInterval(() => {}); }\nsetInterval(() => {});`, [2]],
    ['a function var', `export function f() { if (true) { var setInterval = ${fn}; } return setInterval(() => {}); }\nsetInterval(() => {});`, [2]],
    ['a sloppy block function, also the function\'s (Annex B)', 'function f() { { function setInterval() { return 7; } } return setInterval(); }', []],
    ['a sloppy if function, also the function\'s', 'function f() { if (true) function setInterval() { return 7; } return setInterval(); }', []],
    ['a module block function', 'export function f() { { function setInterval() { return 7; } } return setInterval(); }', [1]],
    ['a strict script block function', '\'use strict\';\nfunction f() { { function setInterval() { return 7; } } return setInterval(); }', [2]],
    ['a class method block function', 'class C { m() { { function setInterval() { return 7; } } return setInterval(); } }', [1]],
    ['a sloppy block generator', 'function f() { { function* setInterval() {} } return setInterval(); }', [1]],
    ['a sloppy block async function', 'function f() { { async function setInterval() {} } return setInterval(); }', [1]],
    ['a sloppy block function under a block let', 'function f() { { let setInterval; { function setInterval() { return 7; } } } return setInterval(); }', [1]],
    ['a sloppy block function under a loop const', 'function f() { for (const setInterval of []) { function setInterval() { return 7; } } return setInterval(); }', [1]],
    ['a sloppy block function in a catch of the name', 'function f() { try {} catch (setInterval) { { function setInterval() { return 7; } } } return setInterval(); }', []],
    ['a sloppy block function in a destructuring catch', 'function f() { try {} catch ({ setInterval }) { { function setInterval() { return 7; } } } return setInterval(); }', [1]],
    ['a sloppy block function under a later block let', 'function f() { { { function setInterval() { return 7; } } let setInterval; } return setInterval(); }', [1]],
    ['a sloppy labelled block function', 'function f() { { l: function setInterval() { return 7; } } return setInterval(); }', []],
    ['a parameter default outside body vars', `export function f(n = setInterval(() => {})) { var setInterval = ${fn}; return n; }`, [1]],
    ['a parameter default outside body lexicals', `export function f(n = setInterval(() => {})) { const setInterval = ${fn}; return n; }`, [1]],
    ['a parameter default outside body functions', 'export function f(n = setInterval(() => {})) { function setInterval() {} return n; }', [1]],
    ['a parameter default using a parameter', `export function f(setInterval = ${fn}, n = setInterval(() => {})) { return n; }`, []],
    ['a destructured parameter', `export function f({ setInterval } = { setInterval: ${fn} }, n = setInterval(() => {})) { return n; }`, []],
    ['a parameter property', 'class C { constructor(private setInterval: (f: () => void) => number, n = setInterval(() => {})) {} }', []],
    ['a destructured binding', `const { setInterval } = { setInterval: ${fn} }; setInterval(() => {});`, []],
    ['a named function expression', '(function setInterval(n = setInterval(() => {})) { return n; });\nsetInterval(() => {});', [2]],
    ['a named class expression', '(class setInterval { n = setInterval(() => {}); });\nsetInterval(() => {});', [2]],
    ['a static block var', `class C { static { var setInterval = ${fn}; setInterval(() => {}); } }\nsetInterval(() => {});`, [2]],
    ['a catch parameter', 'try {} catch (setInterval) { setInterval(() => {}); }\nsetInterval(() => {});', [2]],
    ['a loop binding', `for (const setInterval of [${fn}]) setInterval(() => {});\nsetInterval(() => {});`, [2]],
    ['a switch binding', `switch (0) { case 0: const setInterval = ${fn}; setInterval(() => {}); }\nsetInterval(() => {});`, [2]],
    ['a switch discriminant outside case bindings', `switch (setInterval(() => {})) { case 0: const setInterval = ${fn}; }`, [1]],
    ...['var', 'let', 'const'].map(kind => [
      `a namespace ${kind}`, `namespace N { export ${kind} setInterval = ${fn}; setInterval(() => {}); }\nsetInterval(() => {});`, [2],
    ]),
    ['a merged namespace export', `namespace N { export const setInterval = ${fn}; }\nnamespace N { setInterval(() => {}); }\nsetInterval(() => {});`, [3]],
    ['a private namespace binding', `namespace N { const setInterval = ${fn}; }\nnamespace N { setInterval(() => {}); }`, [2]],
    ['a dotted namespace', `namespace N { export const setInterval = ${fn}; }\nnamespace N.M { setInterval(() => {}); }\nsetInterval(() => {});`, [3]],
    ['a value namespace', 'namespace setInterval { export function x() { return 1; } }\nsetInterval(() => {});', []],
    ['an enum binding', 'enum setInterval { a }\nsetInterval(() => {});', []],
    ['an enum member binding', 'enum E { setInterval = 1, n = (setInterval as any)() }\nsetInterval(() => {});', [2]],
    ...[
      'declare const setInterval: (f: () => void) => number;',
      'declare class setInterval {}',
      'declare namespace setInterval { function x(): number; }',
      'namespace setInterval { export type T = number; }',
      'type setInterval = number;',
      'interface setInterval {}',
      'const enum setInterval { n }',
      "import type { setInterval } from './x';",
      "import { type setInterval } from './x';",
    ].map(binding => [binding, `${binding}\nsetInterval(() => {});`, [2]]),
    ['a value import', "import { setInterval } from './x';\nsetInterval(() => {});", []],
    ['an import alias', 'namespace N { export const D = 1; }\nimport setInterval = N.D;\nsetInterval(() => {});', []],
  ];
  const stage = resolve(import.meta.dir);
  const said = cases.map(([scope, code]) => [scope, ambientRefusals(stage, resolve(stage, 'logic.ts'), code,
    (c, o) => parseSync('logic.ts', c, o).program).map(why => Number(/^logic.ts:(\d+):/.exec(why)[1]))]);
  expect(said).toEqual(cases.map(([scope, , lines]) => [scope, lines]));
});

test('a block function in sloppy CommonJS also binds in its function', () => {
  const stage = resolve(import.meta.dir);
  const refused = (code) => ambientRefusals(stage, resolve(stage, 'logic.cjs'), code, (c, o) => parseSync('logic.cjs', c, o).program);
  expect(refused('function f() {\n  { function setInterval() { return 7; } }\n  return setInterval();\n}\nmodule.exports = f();')).toEqual([]);
  expect(refused('\'use strict\';\nfunction f() {\n  { function setInterval() { return 7; } }\n  return setInterval();\n}')[0]).toStartWith('logic.cjs:4:10: setInterval()');
});

test('ambientRefusals retains timers only and retires clock/Math.random refusals (LLP 1195.000 D2.8)', () => {
  const stage = resolve(import.meta.dir);
  const cases = [
    ...['setInterval', 'requestAnimationFrame', 'requestIdleCallback'].map(name => [name, `${name}(() => {})`, `${name}()`]),
    ...['globalThis', 'self', 'window', 'global'].map(name => [name, `${name}.setInterval(() => {})`, 'setInterval()']),
  ];
  for (const extension of ['ts', 'js']) {
    const file = `logic.${extension}`;
    const refused = code => ambientRefusals(stage, resolve(stage, file), code, (c, o) => parseSync(file, c, o).program);
    for (const [name, call, api] of cases) {
      const code = `export function f(${name}) { return ${call}; }\nexport const tick = () => ${call};`;
      const diagnostics = refused(code);
      expect(diagnostics).toHaveLength(1);
      expect(diagnostics[0]).toStartWith(`${file}:2:27: ${api} is unavailable in data sources`);
      expect(refused(`export function f(${name}) { return ${call}; }`)).toEqual([]);
    }
    expect(refused('const setInterval = () => 1; setInterval();')).toEqual([]);
    expect(refused('const setInterval = {}; globalThis.setInterval(() => {});')[0]).toStartWith(`${file}:1:25: setInterval()`);
    // Clock and Math.random are virtualized (LLP 1195.000 D2) and no longer refused statically.
    expect(refused('Date.now(); new Date(); Date(); performance.now(); Math.random(); setTimeout(() => {}, 1);')).toEqual([]);
  }
});

// The packages app.ts imports are read from its syntax tree: a string, template, regular expression or comment that spells
// an import neither adds a package nor hides one. The compiler reads the same cases by tokens (contract/cli/src/sources/data.rs).
test('an import spelled in a string or a comment is none', () => {
  const app = mkdtempSync(join(tmpdir(), 'exact-used-packages-'));
  const used = (code) => { writeFileSync(join(app, 'app.ts'), code); return [...usedPackages(app).names].sort(); };
  try {
    expect(used('const note = "/*";\nimport { rows } from "@exact/ledger";\nconst end = "*/";\n')).toEqual(['@exact/ledger']);
    expect(used('const hint = \'import { takePhoto } from "@exact/camera"\';\nconst u = "a//b"; import { x } from "@exact/real";\n')).toEqual(['@exact/real']);
    expect(used('const t = `import "tpl" ${await import(\'inside\')} ${{ a: 1 }.a}`;\nimport y from \'after-template\';\n')).toEqual(['after-template', 'inside']);
    expect(used('const r = /import "rx"|\'/g; const q = a / b / c;\nimport z from \'after-regex\';\n')).toEqual(['after-regex']);
    expect(used('// a plain list, not the exact-camera view\n')).toEqual([]);
    expect(used('const s = "\\\r\nimport { open } from \'@exact/hid\'";\n')).toEqual([]);
    expect(used('\uFEFF// notes\u2028import\u00A0{ open } from "@exact/hid";\n')).toEqual(['@exact/hid']);
    expect(used([
      "import { a } from '@exact/notifications';", "import type { B } from '@exact/clipboard';", 'import { type C } from "zod/v4";',
      "export type { D } from 'types-only';", 'const later = await import("lazy-pkg");', "// import { d } from 'commented';",
      "/* import { e } from 'blocked'; */", "const url = 'https://example.com'; import x from 'after-url';",
      "import type from 'named-type';", 'import { "a-b" as ab } from \'string-name\';', 'const meta = import.meta.url;',
    ].join('\n'))).toEqual(['@exact/notifications', 'after-url', 'lazy-pkg', 'named-type', 'string-name', 'zod']);
  } finally { rmSync(app, { recursive: true, force: true }); }
});
