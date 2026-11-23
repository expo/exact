import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { delimiter, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { normalizeDemangled, optimizeWasm, planStages, splitStages, STAGES, Unsplittable, unsplitReason } from './stages.mjs';

// The interaction gallery's symbols for `exact_runner::agent::handle::<Gallery>`, its `{closure#0}`, and the plain
// path a crate without a disambiguator gives.
test.each([
  ['_RINvNtCs41IsHxXs5ek_12exact_runner5agent6handleNtCshkVtqmrZMu0_24interaction_gallery_data7GalleryECs7smTbzDW5eq_23interaction_gallery_web', true],
  ['_RINvNtC12exact_runner5agent6handleNtC8caltrain7AppDataE', true],
  ['_RNCINvNtCs41IsHxXs5ek_12exact_runner5agent6handleNtCshkVtqmrZMu0_24interaction_gallery_data7GalleryE0Cs7smTbzDW5eq_23interaction_gallery_web', false],
  ['_RINvNtCs41IsHxXs5ek_12exact_runner5agent7handlerNtC8caltrain7AppDataE', false],
  ['_RNvNtCs41IsHxXs5ek_12exact_runner4json9field_str', false],
])('the inspection seam is the handler alone: %s', (symbol, seam) => {
  expect(STAGES.find(stage => stage.name === 'inspection').seam.test(symbol)).toBe(seam);
});

test.each([
  ['<alloc[2d4e00666e4053f6]::sync::Arc<[f64]>>::drop_slow', '<alloc::sync::Arc<[f64]>>::drop_slow'],
  ['exact_runner[3c1c0]::agent::handle::<alloc[abc]::sync::Arc<[f64]>>', 'exact_runner::agent::handle::<alloc::sync::Arc<[f64]>>'],
  ['<str>::trim_matches::<<exact_web[e1bdf407d33a72ab]::host::Host<caltrain_web[9c4056522a759cff]::AppData>>::canvas_image::{closure#1}>', '<str>::trim_matches::<<exact_web::host::Host<caltrain_web::AppData>>::canvas_image::{closure#1}>'],
  ['<impl exact_runner[3c1c0]::agent::Agent>::handle::{closure#0}', '<impl exact_runner::agent::Agent>::handle::{closure#0}'],
])('normalizes crate paths without changing types or closures: %s', (name, expected) => {
  expect(normalizeDemangled(name)).toBe(expected);
});

function optimizerFixture(check) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-web-opt-'));
  try {
    const input = resolve(dir, 'input.wasm'), tool = resolve(dir, 'wasm-opt'), calls = resolve(dir, 'calls');
    const cache = resolve(dir, 'cache');
    writeFileSync(input, 'app wasm');
    const code = `#!${process.execPath}
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
appendFileSync(${JSON.stringify(calls)}, 'x');
if (process.argv.includes('--fail')) process.exit(17);
writeFileSync(process.argv[process.argv.indexOf('-o') + 1], readFileSync(process.argv.at(-1)));
// tool A
`;
    writeFileSync(tool, code); chmodSync(tool, 0o755);
    let builds = 0;
    const build = (options = ['-Oz'], command = tool) => {
      const stage = resolve(dir, `stage-${builds++}`); mkdirSync(stage);
      const output = resolve(stage, 'app.wasm');
      const result = optimizeWasm(input, output, options, cache, command);
      if (result.status === 0) expect(readFileSync(output)).toEqual(readFileSync(input));
      return result;
    };
    check({dir, input, tool, code, cache, build, calls: () => readFileSync(calls, 'utf8').length});
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('unchanged app wasm reuses optimization into a fresh build stage', () => {
  optimizerFixture(({build, calls}) => {
    expect(build().status).toBe(0);
    expect(build().cached).toBe(true);
    expect(calls()).toBe(1);
  });
});

test('optimization requires the same input bytes, options, executable and intact output', () => {
  optimizerFixture(({input, tool, code, cache, build, calls}) => {
    expect(build().status).toBe(0);
    const original = statSync(input);
    writeFileSync(input, 'new wasm'); utimesSync(input, original.atime, original.mtime);
    expect(build().cached).not.toBe(true);
    expect(build(['-O2']).cached).not.toBe(true);
    expect(build(['-O2']).cached).toBe(true);
    const identity = statSync(tool);
    writeFileSync(tool, code.replace('tool A', 'tool B')); utimesSync(tool, identity.atime, identity.mtime);
    expect(build(['-O2']).cached).not.toBe(true);
    expect(build(['-O2']).cached).toBe(true);
    writeFileSync(resolve(cache, 'optimized.wasm'), 'corrupt');
    expect(build(['-O2']).cached).not.toBe(true);
    rmSync(resolve(cache, 'optimized.wasm'));
    expect(build(['-O2']).cached).not.toBe(true);
    writeFileSync(resolve(cache, 'receipt.json'), '{');
    expect(build(['-O2']).cached).not.toBe(true);
    rmSync(resolve(cache, 'receipt.json'));
    expect(build(['-O2']).cached).not.toBe(true);
    expect(build(['-O2']).cached).toBe(true);
    expect(calls()).toBe(8);
  });
});

test('optimizer lookup follows PATH changes made by the build after startup', () => {
  optimizerFixture(({dir, code, build, calls}) => {
    const command = resolve(dir, 'exact-web-test-opt'), before = process.env.PATH;
    writeFileSync(command, code); chmodSync(command, 0o755);
    try {
      process.env.PATH = `${dir}${delimiter}${before ?? ''}`;
      expect(build(['-Oz'], 'exact-web-test-opt').status).toBe(0);
      expect(build(['-Oz'], 'exact-web-test-opt').cached).toBe(true);
      expect(calls()).toBe(1);
    } finally {
      if (before === undefined) delete process.env.PATH;
      else process.env.PATH = before;
    }
  });
});

test('a missing or failed optimizer is never cached', () => {
  optimizerFixture(({dir, build, calls}) => {
    expect(build().status).toBe(0);
    expect(build(['--fail']).status).toBe(17);
    expect(build(['--fail']).status).toBe(17);
    expect(calls()).toBe(3);
    expect(build(['-Oz'], resolve(dir, 'missing-tool')).error?.code).toBe('ENOENT');
    expect(build().cached).toBe(true);
  });
});

// The handler and its closure by their v0 symbols, as rustc names them for an app crate `caltrain`.
const HANDLE = '_RINvNtC12exact_runner5agent6handleNtC8caltrain7AppDataE';
const CLOSURE = '_RNCINvNtC12exact_runner5agent6handleNtC8caltrain7AppDataE0C8caltrain';

// The production seam on a module whose boot calls the handler's closure through the table before inspection loads,
// as the core's timers did: the closure stays in the core, and the handler, which `read` reaches by a slot it reads at
// run time as the glue's gated hook is read, answers once its stage is loaded.
test.skipIf(unsplitReason() !== null || !Bun.which('wasm-as'))('a cached names build still splits and loads its capability', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-web-opt-stages-'));
  try {
    const input = resolve(dir, 'app.wasm'), wat = resolve(dir, 'app.wat'), cache = resolve(dir, 'cache');
    writeFileSync(wat, `(module
      (type $read (func (result i32)))
      (table 2 funcref)
      (elem (i32.const 0) $${HANDLE} $${CLOSURE})
      (global $hook (export "hook") (mut i32) (i32.const 0))
      (func $${HANDLE} (type $read) (result i32) i32.const 42)
      (func $${CLOSURE} (type $read) (result i32) i32.const 7)
      (func (export "boot") (result i32) i32.const 1 call_indirect (type $read))
      (func (export "read") (result i32) global.get $hook call_indirect (type $read)))`);
    expect(spawnSync('wasm-as', [wat, '-g', '-o', input]).status).toBe(0);
    let previous;
    for (let i = 0; i < 2; i++) {
      const stage = resolve(dir, `stage-${i}`); mkdirSync(stage);
      const named = resolve(stage, 'app.named.wasm');
      const optimized = optimizeWasm(input, named, ['-Oz', '-g'], cache);
      expect(optimized.status).toBe(0);
      expect(!!optimized.cached).toBe(i === 1);
      const split = splitStages(named);
      if (previous) expect(split.core).toEqual(previous);
      previous = split.core;
      const module = await WebAssembly.compile(split.core), core = await WebAssembly.instantiate(module, await placeholders());
      const card = JSON.parse(Buffer.from(WebAssembly.Module.customSections(module, 'exact.stages')[0]));
      expect(Object.keys(card.stages)).toEqual(['inspection']);
      expect(core.exports.boot()).toBe(7);
      expect(() => core.exports.read()).toThrow('app.wasm called the inspection stage (placeholder 0) before the stage loaded');
      for (const path of Object.values(card.stages)) {
        const capability = await WebAssembly.compile(split.files.get(path));
        const imports = Object.fromEntries(WebAssembly.Module.imports(capability).map(entry => [entry.module, core.exports]));
        await WebAssembly.instantiate(capability, imports);
      }
      expect(core.exports.read()).toBe(42);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A seam the planner cannot show to be the one way into its stage ships the module whole: a seam wasm-opt merged with an
// identical core callback, which then fills the callback's table slot too or is exported in its place; a second crate
// that shares the SDK's path; a start function, which may call through the table at instantiation; a call graph it
// cannot read, as a function name holding a quote or a line break makes it. A direct call from a root, whatever its
// name, a start function included, fails the build.
test.skipIf(unsplitReason() !== null || !Bun.which('wasm-as'))('a seam that may not be the one way into its stage is refused', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-merged-seam-'));
  const other = HANDLE.replace('C12exact_runner', 'Cs1_12exact_runner');
  const cases = [
    ['in 2 table slots', true, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $callback)
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $callback (type $read) (result i32) i32.const 0)
      (global $slot (export "slot") (mut i32) (i32.const 1)) (func (export "boot") (result i32) global.get $slot call_indirect (type $read))`],
    ['exported', true, `(table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $callback (export "boot") (type $read) (result i32) i32.const 0)`],
    ['directly from start', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE}) (global $g (mut i32) (i32.const 0))
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $start (global.set $g (call $${HANDLE}))) (start $start)`],
    ['has a start function', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE}) (global $g (mut i32) (i32.const 0))
      (func $${HANDLE} (type $read) (result i32) i32.const 0)
      (func $start (global.set $g (call_indirect (type $read) (i32.const 0)))) (start $start)`],
    ['2 crates', false, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $${other})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $${other} (type $read) (result i32) i32.const 1)`],
    // A function the graph's key shares its name with ("B") is a table root that calls the seam directly.
    ['directly from B', false, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $B)
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $B (type $read) (result i32) call $${HANDLE})`],
    // Names the call graph prints unescaped: one holding its edge syntax, and others that make that edge read as a call
    // between them; and a line break
    ['holds a quote or a line break', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $"boot\\" -> \\"decoy" (export "boot") (result i32) call $${HANDLE})`],
    ['holds a quote or a line break', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $"boot\\" -> \\"decoy" (export "boot") (result i32) call $${HANDLE})
      (func $boot (export "b") (result i32) i32.const 0) (func $"decoy\\" -> \\"${HANDLE}" (export "d") (result i32) i32.const 0)`],
    ['holds a quote or a line break', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $"a\\nb" (export "boot") (result i32) i32.const 0)`],
    // A direct call fails the build even where the seam, in no slot or exported, would otherwise ship the module whole.
    ['directly from boot', false, `(table 1 funcref) (func $${HANDLE} (type $read) (result i32) i32.const 0)
      (func $boot (export "boot") (result i32) call $${HANDLE})`],
    ['directly from B', false, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $B) (export "seam" (func $${HANDLE}))
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $B (type $read) (result i32) call $${HANDLE})`],
  ];
  try {
    for (const [refusal, optimized, body] of cases) {
      const input = resolve(dir, 'app.wasm'), wat = resolve(dir, 'app.wat'), named = resolve(dir, 'app.named.wasm');
      writeFileSync(wat, `(module (type $read (func (result i32))) ${body} (global $hook (export "hook") (mut i32) (i32.const 0))
        (func (export "read") (result i32) global.get $hook call_indirect (type $read)))`);
      expect(spawnSync('wasm-as', [wat, '-g', '--enable-reference-types', '--enable-bulk-memory', '-o', input]).status).toBe(0);
      if (optimized) expect(optimizeWasm(input, named, ['-Oz', '-g'], resolve(dir, `cache-${refusal}`)).status).toBe(0);
      let thrown;
      try { planStages(optimized ? named : input); } catch (e) { thrown = e; }
      expect([refusal, thrown?.message.includes(refusal), thrown instanceof Unsplittable]).toEqual([refusal, true, !refusal.startsWith('directly')]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// Every stage's direct calls are checked before any stage ships the module whole: a later stage's direct call fails the
// build though an earlier stage's seam is exported, and so does one from a start function that is another stage's
// seam. Where a seam's pattern matches two crates' functions, a direct call to one ships the module whole, and the build
// fails when the core calls each crate's, the other's counted as the core's: from a root, or one through the other.
// The error names each crate's candidate with its disambiguator, as handlers of two crates print alike. Another stage's
// matches stay blocked in each crate's check, so overlapping patterns can ship the module whole.
test.skipIf(unsplitReason() !== null || !Bun.which('wasm-as'))('every stage is checked for a direct call first', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-stage-order-'));
  // Two crates' handlers that print alike, by a pattern whose group takes the disambiguator's digits: none for one
  const first = HANDLE.replace('C12exact_runner', 'Cs_12exact_runner'), handle = 'exact_runner::agent::handle::<caltrain::AppData>';
  const digits = [{ name: 'inspection', seam: /^_RINvNtC(?:s([0-9A-Za-z]*)_)?12exact_runner5agent6handle/ }];
  const cases = [
    ['directly from boot', false, [{ name: 'one', seam: /^one(.*)$/ }, { name: 'two', seam: /^two(.*)$/ }],
      `(table 1 funcref) (elem (i32.const 0) $two) (func $one (export "one") (result i32) i32.const 0)
      (func $two (type $read) (result i32) i32.const 0) (func $boot (export "boot") (result i32) call $two)`],
    ['directly from one', false, [{ name: 'one', seam: /^one(.*)$/ }, { name: 'two', seam: /^two(.*)$/ }],
      `(table 1 funcref) (elem (i32.const 0) $two) (global $g (mut i32) (i32.const 0))
      (func $two (type $read) (result i32) i32.const 0) (func $one (global.set $g (call $two))) (start $one)`],
    ['2 crates', true, [{ name: 'two', seam: /^two(_a|_b)$/ }],
      `(table 2 funcref) (elem (i32.const 0) $two_a $two_b) (func $two_a (type $read) (result i32) i32.const 0)
      (func $two_b (type $read) (result i32) i32.const 1) (func $boot (export "boot") (result i32) call $two_a)`],
    [`${handle} (no crate disambiguator) directly from boot; ${handle} (crate disambiguator "") directly from boot`, false,
      digits, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $${first}) (func $${HANDLE} (type $read) (result i32) i32.const 0)
      (func $${first} (type $read) (result i32) i32.const 1)
      (func $boot (export "boot") (result i32) (i32.add (call $${HANDLE}) (call $${first})))`],
    ['two_a (crate disambiguator "_a") directly from boot; two_b (crate disambiguator "_b") directly from two_a', false,
      [{ name: 'two', seam: /^two(_a|_b)$/ }],
      `(table 2 funcref) (elem (i32.const 0) $two_a $two_b) (func $two_a (type $read) (result i32) call $two_b)
      (func $two_b (type $read) (result i32) i32.const 1) (func $boot (export "boot") (result i32) call $two_a)`],
    ['2 crates', true, [{ name: 'one', seam: /^(a|b)$/ }, { name: 'two', seam: /^(a)$/ }],
      `(table 2 funcref) (elem (i32.const 0) $a $b) (func $a (type $read) (result i32) call $b)
      (func $b (type $read) (result i32) call $a)`],
  ];
  try {
    for (const [refusal, unsplittable, stages, body] of cases) {
      const input = resolve(dir, 'app.wasm'), wat = resolve(dir, 'app.wat');
      writeFileSync(wat, `(module (type $read (func (result i32))) ${body})`);
      expect(spawnSync('wasm-as', [wat, '-g', '--enable-reference-types', '--enable-bulk-memory', '-o', input]).status).toBe(0);
      let thrown;
      try { planStages(input, stages); } catch (e) { thrown = e; }
      expect([refusal, thrown?.message.includes(refusal), thrown instanceof Unsplittable]).toEqual([refusal, true, unsplittable]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A line break in the last function's name, whose second line reads as the next entry of the function map and whose
// two lines name other functions, hides that function's call: the map then names fewer functions than it has lines.
test.skipIf(unsplitReason() !== null || !Bun.which('wasm-as'))('a line break in the last function name is seen', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-last-name-'));
  try {
    const input = resolve(dir, 'app.wasm'), wat = resolve(dir, 'app.wat');
    writeFileSync(wat, `(module (type $read (func (result i32))) (table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $a (export "a") (result i32) i32.const 0)
      (func $b (export "b") (result i32) i32.const 0) (func $"a\\n4:b" (export "boot") (result i32) call $${HANDLE}))`);
    expect(spawnSync('wasm-as', [wat, '-g', '--enable-reference-types', '--enable-bulk-memory', '-o', input]).status).toBe(0);
    expect(() => planStages(input)).toThrow(Unsplittable);
    expect(() => planStages(input)).toThrow('holds a quote or a line break');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A name whose lines after its first forge the call graph's header and a node for each function the map names before
// it: the map ends cleanly, its names are the forged nodes, and the forged graph has no calls; the real one follows.
test.skipIf(unsplitReason() !== null || !Bun.which('wasm-as'))('a name that forges the call graph is seen', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-forged-graph-'));
  const id = (name) => `$"${name.replaceAll('\\', '\\\\').replaceAll('"', '\\"').replaceAll('\n', '\\n')}"`;
  const forged = id(`y\ndigraph call {\n  "${HANDLE}" [style="rounded"];\n  "boot" [fillcolor="gray"];\n  "y" [];`);
  try {
    const input = resolve(dir, 'app.wasm'), wat = resolve(dir, 'app.wat');
    writeFileSync(wat, `(module (type $read (func (result i32))) (table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $boot (export "boot") (result i32) call ${forged})
      (func ${forged} (result i32) call $${HANDLE}))`);
    expect(spawnSync('wasm-as', [wat, '-g', '--enable-reference-types', '--enable-bulk-memory', '-o', input]).status).toBe(0);
    expect(() => planStages(input)).toThrow('holds a quote or a line break');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// Every other way the core may call into a stage before it loads reaches the stage's placeholder, which reports it:
// a call through a slot index the code holds, as the gated hook is held; a constant slot; and a reference to the seam
// returned to a script.
test.skipIf(unsplitReason() !== null || !Bun.which('wasm-as'))('a call into a stage before it loads reports the stage', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-early-stage-'));
  const cases = [
    ['a held slot index', `(func (export "early") (result i32) global.get $hook call_indirect (type $read))`],
    ['a constant slot', `(func (export "early") (result i32) i32.const 0 call_indirect (type $read))`],
    ['an escaping reference', `(elem declare func $${HANDLE}) (func (export "get") (result funcref) ref.func $${HANDLE})`, core => core.get()()],
  ];
  try {
    for (const [way, body, early = core => core.early()] of cases) {
      const wat = resolve(dir, 'app.wat'), named = resolve(dir, 'app.named.wasm');
      writeFileSync(wat, `(module (type $read (func (result i32))) (table 1 funcref) (elem (i32.const 0) $${HANDLE})
        (global $hook (export "hook") (mut i32) (i32.const 0)) (func $${HANDLE} (type $read) (result i32) i32.const 42)
        (func (export "read") (result i32) global.get $hook call_indirect (type $read)) ${body})`);
      expect(spawnSync('wasm-as', [wat, '-g', '--enable-reference-types', '--enable-bulk-memory', '-o', named]).status).toBe(0);
      const split = splitStages(named), core = (await WebAssembly.instantiate(split.core, await placeholders())).instance.exports;
      let thrown;
      try { early(core); } catch (e) { thrown = e; }
      expect([way, thrown?.message]).toEqual([way, 'app.wasm called the inspection stage (placeholder 0) before the stage loaded']);
      const [path] = split.files.keys();
      await WebAssembly.instantiate(split.files.get(path), { primary: core });
      expect([way, early(core)]).toEqual([way, 42]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

/** The glue's imports for a split core (wasm/boot.js `withPlaceholders`): each stage placeholder throws, naming its stage. */
async function placeholders() {
  const { wasmSource, glueDeclarations } = await import('./tests/glue-fixture.mjs');
  const vm = await import('node:vm');
  const context = vm.createContext({});
  vm.runInContext(glueDeclarations(wasmSource, ['StageNotLoaded', 'withPlaceholders']), context);
  return context.withPlaceholders({});
}

// The glue's guard on the core's exports: a stage placeholder's error stops the module, as a trap does, and the report
// says how a call reaches a stage, which a trap's does not.
test('a call that reached an unloaded stage stops the module and says how calls reach a stage', async () => {
  const { wasmSource, glueDeclarations } = await import('./tests/glue-fixture.mjs');
  const vm = await import('node:vm');
  const said = [];
  const context = vm.createContext({ host: {}, root: { dataset: {} }, exact: {}, WebAssembly, String,
    console: { error: (line) => said.push(line) } });
  vm.runInContext(glueDeclarations(wasmSource, ['ModuleStopped', 'StageNotLoaded', 'stopModule', 'guardedExports']), context);
  const exports = vm.runInContext(`guardedExports({ early: () => { throw new StageNotLoaded('x'); }, trap: () => { throw new WebAssembly.RuntimeError('null function'); } })`, context);
  const report = (name) => {
    context.host.trapped = null; said.length = 0;
    expect(() => exports[name]()).toThrow();
    expect(context.host.trapped).toBeTruthy();
    return said[0];
  };
  expect(report('early')).toContain('every call into a stage waits for it to load (glue.js `loadStage`');
  expect(report('trap')).toBe('exact: app.wasm trapped, and the page no longer calls it:');
});
