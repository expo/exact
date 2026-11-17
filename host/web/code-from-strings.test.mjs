// Code from a string in a data module on the web (#128): it throws as on Hermes (`SyntaxError: Parsing source code
// unsupported`), however `Function` or `eval` is reached by name, on the JS target (host/web-js/ts-fetch.js, through
// the bindings host/web-js/build.mjs `scopedModule` gives the module) and in the wasm route's realm
// (host/web/storage-environment.js `refuseCodeFromStrings`). A guarded probe (zod's) takes its fallback; what a
// function needs of `Function` is the page's. A function's own `constructor` is still the page's (LLP 1027 D5).
import { expect, test } from 'bun:test';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const realHelpers = `export { ownWithout, redirectOf, signalDeadline, socketProtocols, within } from ${JSON.stringify(pathToFileURL(resolve(ROOT, 'host/web-js/admission.js')).href)}; `;

const STRING_CODE = `// A copy of the globals written back, as a module may: the module's own, never the page's.
for (const name of ['Function', 'eval', 'Date']) globalThis[name] = globalThis[name];
const F = Function, forms = {
  call: () => Function('return 1'), construct: () => new Function('return 1'), alias: () => new F('return 1'),
  global: () => new globalThis.Function('return 1'), reflect: () => Reflect.construct(Function, ['return 1']), apply: () => Function.call(null, 'return 1'),
  eval: () => eval('1'), indirect: () => (0, eval)('1'), globalEval: () => globalThis.eval('1'),
  // Each stand-in's own \`constructor\`; what goes through \`Function.prototype\` reaches the realm's (LLP 1027 D5).
  evalConstructor: () => eval.constructor('return 1'), functionConstructor: () => Function.constructor('return 1'),
  descriptor: () => Object.getOwnPropertyDescriptor(globalThis, 'Function').value('return 1'),
  restored: () => globalThis.Function('return 1'),
  // The own \`constructor\` cannot be deleted or written, by any key.
  undeleted: () => { const key = 'constructor'; try { delete Function[key]; } catch {} try { eval[key] = null; } catch {} return Function[key]('return 1') ?? eval[key]('return 1'); },
};
const thrown = f => { try { f(); return 'ran'; } catch (e) { return e.name + ': ' + e.message; } };
const add = (a, b) => a + b;
globalThis.exact = { thrown: Object.fromEntries(Object.entries(forms).map(([k, f]) => [k, thrown(f)])),
  kept: [add instanceof Function, typeof Function, Function.prototype.call.call(add, null, 1, 2), add.apply(null, [2, 3]), Function.prototype.bind.call(add, null, 3)(4), typeof eval,
    Function instanceof Function, eval instanceof Function],
  constructor: thrown(() => (function () {}).constructor('return 1')), prototype: thrown(() => Object.getPrototypeOf(eval).constructor('return 1')),
  // \`eval\` is no constructor, as on Hermes (js/bake/tests/packages.rs \`eval_is_no_constructor_on_hermes\`).
  evalNew: [thrown(() => new eval('1')), thrown(() => Reflect.construct(eval, [])), typeof eval.prototype, eval instanceof Function].map(s => String(s).split(':')[0]).join(' ') };
`;
const refusedCode = 'SyntaxError: Parsing source code unsupported: a data source runs no code from a string (Function, eval), as on Hermes';
// Hermes's answers (js/bake/tests/packages.rs \`eval_is_no_constructor_on_hermes\`).
const EVAL_NEW = 'TypeError TypeError undefined true';
const expectRefused = ({ thrown, kept, constructor, prototype, evalNew }) => {
  expect(evalNew).toBe(EVAL_NEW);
  expect(thrown).toEqual(Object.fromEntries(Object.keys(thrown).map(k => [k, refusedCode])));
  expect(Object.keys(thrown).length).toBe(14);
  expect(prototype).toBe('ran');
  expect(kept).toEqual([true, 'function', 3, 5, 7, 'function', true, true]);
  expect(constructor).toBe('ran');
};

// The JS target's stand-ins themselves (ts-fetch.js), as the module's `Function`, `eval` and `globalThis`. That the build
// gives a module those bindings, every spelling included, host/web-js/build.test.mjs checks on a built page.
test('the JS target\'s Function and eval throw as on Hermes, and Function is still a function', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-ts-string-code-'));
  cpSync(resolve(ROOT, 'host/web-js/ts-fetch.js'), resolve(dir, 'ts-fetch.js'));
  writeFileSync(resolve(dir, 'admission.js'), '' + realHelpers + 'export const fetchWith = () => Promise.reject(new Error("no fetch here")); export class FetchError extends Error {} export const coversPath = () => false; export const overlaying = { on: false }; export const inOverlay = api => new Error(api);\n');
  writeFileSync(resolve(dir, 'admission-data.js'), 'export const tsGrantSet = null;\n');
  const page = globalThis.Function, pageDate = globalThis.Date;
  try {
    const guards = await import(pathToFileURL(resolve(dir, 'ts-fetch.js')).href);
    // A sloppy function's parameters may be named `eval`: the module's bindings, as the build binds them.
    page('Function', 'eval', 'globalThis', STRING_CODE)(guards.Function, guards.eval, guards.appGlobal);
    expectRefused(globalThis.exact);
    // The page's own is untouched, the module's writes included.
    expect([globalThis.Function, new Function('return 1')(), globalThis.Date, new Date(0).getTime()]).toEqual([page, 1, pageDate, 0]);
  } finally {
    delete globalThis.exact;
    rmSync(dir, { recursive: true, force: true });
  }
});

// The JS target's module global (ts-fetch.js `appGlobal`): the names it substitutes (`Date`, `Math`, `Function`, `eval`,
// `fetch`, …) are its own properties, with an object's semantics, and none of it reaches the page's.
test("the JS target's module global keeps its own names as properties, never the page's", async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-ts-global-'));
  cpSync(resolve(ROOT, 'host/web-js/ts-fetch.js'), resolve(dir, 'ts-fetch.js'));
  writeFileSync(resolve(dir, 'admission.js'), '' + realHelpers + 'export const fetchWith = () => Promise.reject(new Error("no fetch here")); export class FetchError extends Error {} export const coversPath = () => false; export const overlaying = { on: false }; export const inOverlay = api => new Error(api);\n');
  writeFileSync(resolve(dir, 'admission-data.js'), 'export const tsGrantSet = null;\n');
  const page = { Function: globalThis.Function, Date: globalThis.Date, Math: globalThis.Math, eval: globalThis.eval };
  const untouched = () => expect([globalThis.Function, globalThis.Date, globalThis.Math, globalThis.eval]).toEqual([page.Function, page.Date, page.Math, page.eval]);
  try {
    const { appGlobal: g, Date: ModuleDate, Math: ModuleMath } = await import(pathToFileURL(resolve(dir, 'ts-fetch.js')).href);
    // Attribute-only: the value stays, the attribute holds.
    expect(Reflect.defineProperty(g, 'Date', { writable: false })).toBe(true);
    expect(new g.Date(0).getTime()).toBe(0);
    expect([g.Date, Reflect.set(g, 'Date', 1), Object.getOwnPropertyDescriptor(g, 'Date').writable]).toEqual([ModuleDate, false, false]);
    untouched();
    // An accessor: its getter runs at each read, not at the definition.
    let reads = 0;
    expect(Reflect.defineProperty(g, 'Math', { get() { reads++; return 7; }, configurable: true })).toBe(true);
    expect([reads, g.Math, g.Math, reads]).toEqual([0, 7, 7, 2]);
    untouched();
    // Deleting it defines the module's own again, never the page's.
    expect(delete g.Math).toBe(true);
    expect(g.Math).toBe(ModuleMath);
    // Non-configurable: a definition that succeeds holds, one that fails changes nothing.
    const Kept = function Kept() {};
    expect(Reflect.defineProperty(g, 'eval', { value: Kept, writable: false, configurable: false })).toBe(true);
    expect([Reflect.defineProperty(g, 'eval', { value: () => 1 }), g.eval, Reflect.deleteProperty(g, 'eval'), g.eval]).toEqual([false, Kept, false, Kept]);
    expect(() => Object.defineProperty(g, 'eval', { value: () => 2 })).toThrow(TypeError);
    expect(g.eval).toBe(Kept);
    untouched();
    // Write-back: the module's own, and the page's untouched.
    g.Function = g.Function;
    expect(() => g.Function('return 1')).toThrow(SyntaxError);
    untouched();
  } finally {
    for (const [name, value] of Object.entries(page)) globalThis[name] = value;
    rmSync(dir, { recursive: true, force: true });
  }
});

// The wasm route's stand-ins (storage-environment.js `refuseCodeFromStrings`), made in this realm for another, as
// module-glue.js makes them for its iframe; host/web/module-storage.test.mjs drives the iframe's and the worker's.
test("code from a string throws in the wasm route's module realm as on Hermes", async () => {
  const vm = await import('node:vm');
  const { refuseCodeFromStrings } = await import('./storage-environment.js');
  const context = vm.createContext({}), realm = vm.runInContext('globalThis', context);
  const evaluate = (indirect => source => indirect(source))(realm.eval);
  refuseCodeFromStrings(realm);
  evaluate(STRING_CODE);
  expectRefused(realm.exact);
  expect(vm.runInContext('exact.thrown.call', context)).toBe(refusedCode);
  expect(vm.runInContext('(() => { try { Function("x"); } catch (e) { return e instanceof SyntaxError; } })()', context)).toBe(true);
  expect(globalThis.Function('return 2')()).toBe(2);
});
