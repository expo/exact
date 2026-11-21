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
      const module = await WebAssembly.compile(split.core), core = await WebAssembly.instantiate(module);
      const card = JSON.parse(Buffer.from(WebAssembly.Module.customSections(module, 'exact.stages')[0]));
      expect(Object.keys(card.stages)).toEqual(['inspection']);
      expect(core.exports.boot()).toBe(7);
      expect(() => core.exports.read()).toThrow();
      for (const path of Object.values(card.stages)) {
        const capability = await WebAssembly.compile(split.files.get(path));
        const imports = Object.fromEntries(WebAssembly.Module.imports(capability).map(entry => [entry.module, core.exports]));
        await WebAssembly.instantiate(capability, imports);
      }
      expect(core.exports.read()).toBe(42);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// Each way the core could reach a stage before it loads is refused: a seam wasm-opt merged with an identical core
// callback, which then fills the callback's table slot too or is exported in its place; a start function, which the
// call graph does not root; a second crate that shares the SDK's path; a direct call from a table root, whatever its
// name; a call through the seam's slot by a constant index, which is as direct; a table or an import the module shares
// with a script, or code that writes the table; and, once split, a stage that changes a slot the core holds or a
// reference to the seam outside its slot.
test.skipIf(unsplitReason() !== null || !Bun.which('wasm-as'))('a split the core could reach early is refused', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-web-merged-seam-'));
  const other = HANDLE.replace('C12exact_runner', 'Cs1_12exact_runner');
  const cases = [
    ['in 2 table slots', true, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $callback)
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $callback (type $read) (result i32) i32.const 0)
      (global $slot (export "slot") (mut i32) (i32.const 1)) (func (export "boot") (result i32) global.get $slot call_indirect (type $read))`],
    ['exported', true, `(table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $callback (export "boot") (type $read) (result i32) i32.const 0)`],
    ['start function', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE}) (global $g (mut i32) (i32.const 0))
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $boot (global.set $g (call $${HANDLE}))) (start $boot)`],
    ['2 crates', false, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $${other})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $${other} (type $read) (result i32) i32.const 1)`],
    // A function the graph's key shares its name with ("B") is a table root that calls the seam directly.
    ['directly from B', false, `(table 2 funcref) (elem (i32.const 0) $${HANDLE} $B)
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $B (type $read) (result i32) call $${HANDLE})`],
    ['directly from boot', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $boot (export "boot") (result i32) i32.const 0 call_indirect (type $read))`],
    // A table a script or the code may write, whose slots the planner cannot read a call through by their first function
    ['exports a table', false, `(table (export "table") 1 funcref) (elem (i32.const 0) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0)`],
    ['TableSet', false, `(table 2 funcref) (elem (i32.const 0) $${HANDLE}) (elem declare func $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func (export "copy") (table.set (i32.const 1) (ref.func $${HANDLE})))`],
    // An import other than a function's, here a global a segment's offset reads, so a slot's first function is unknown
    ['imports a table, a memory, a global', false, `(import "env" "base" (global $base i32)) (table 1 funcref) (elem (global.get $base) $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0)`],
    // References to the seam outside its slot, in code or a global, which wasm-split would make trampolines for, and a
    // null entry, which keeps them from being counted
    ['leaves the element segments', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE}) (elem declare func $${HANDLE})
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func (export "get") (result funcref) (ref.func $${HANDLE}))`],
    ['leaves the element segments', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE}) (global $held (export "held") funcref (ref.func $${HANDLE}))
      (func $${HANDLE} (type $read) (result i32) i32.const 0)`],
    ['null function reference', false, `(table 3 funcref) (elem (i32.const 0) funcref (ref.func $${HANDLE}) (ref.null func) (ref.func $callback))
      (elem declare func $${HANDLE}) (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $callback (type $read) (result i32) i32.const 1)
      (func (export "get") (result funcref) (ref.func $${HANDLE}))`],
    // Found once split: a stage's segment that, loaded, writes over a slot the core holds
    ['changes slot 0', false, `(table 1 funcref) (elem (i32.const 0) $${HANDLE}) (elem (i32.const 0) $callback)
      (func $${HANDLE} (type $read) (result i32) i32.const 0) (func $callback (type $read) (result i32) i32.const 1)`, true],
  ];
  try {
    for (const [refusal, optimized, body, split] of cases) {
      const input = resolve(dir, 'app.wasm'), wat = resolve(dir, 'app.wat'), named = resolve(dir, 'app.named.wasm');
      writeFileSync(wat, `(module (type $read (func (result i32))) ${body} (global $hook (export "hook") (mut i32) (i32.const 0))
        (func (export "read") (result i32) global.get $hook call_indirect (type $read)))`);
      expect(spawnSync('wasm-as', [wat, '-g', '--enable-reference-types', '--enable-bulk-memory', '-o', input]).status).toBe(0);
      if (optimized) expect(optimizeWasm(input, named, ['-Oz', '-g'], resolve(dir, `cache-${refusal}`)).status).toBe(0);
      // A direct call into the seam fails the build; every other refusal ships the module unsplit.
      let thrown;
      try { (split ? splitStages : planStages)(optimized ? named : input); } catch (e) { thrown = e; }
      expect([refusal, thrown?.message.includes(refusal), thrown instanceof Unsplittable]).toEqual([refusal, true, !refusal.startsWith('directly')]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The glue's report of a trap: an empty table slot while a stage is not loaded names the stage, which the trap does not.
test('a null-function trap while a stage is not loaded names the stage', async () => {
  const { wasmSource, glueDeclarations } = await import('./tests/glue-fixture.mjs');
  const vm = await import('node:vm');
  const report = (stages, loaded, error) => {
    const said = [];
    const context = vm.createContext({ host: { stages }, root: { dataset: {} }, exact: {}, WebAssembly, String,
      stageLoaded: (name) => loaded.includes(name), console: { error: (line) => said.push(line) } });
    vm.runInContext(glueDeclarations(wasmSource, ['stopModule']), context);
    context.stopModule(error);
    return said[0];
  };
  const plain = 'exact: app.wasm trapped, and the page no longer calls it:', empty = new WebAssembly.RuntimeError('null function');
  expect(report({ inspection: 'stages/inspection.wasm' }, [], empty)).toContain('reached the inspection stage before it loaded, around its gate');
  expect(report({ inspection: 'stages/inspection.wasm' }, ['inspection'], empty)).toBe(plain);
  expect(report({}, [], empty)).toBe(plain);
  expect(report({ inspection: 'stages/inspection.wasm' }, [], new WebAssembly.RuntimeError('unreachable'))).toBe(plain);
  // SpiderMonkey's and JavaScriptCore's empty slots, and JavaScriptCore's null reference, which is no call
  expect(report({ inspection: 'stages/inspection.wasm' }, [], new WebAssembly.RuntimeError('indirect call to null'))).toContain('inspection stage');
  expect(report({ inspection: 'stages/inspection.wasm' }, [], new WebAssembly.RuntimeError('call_indirect to a null table entry'))).toContain('inspection stage');
  expect(report({ inspection: 'stages/inspection.wasm' }, [], new WebAssembly.RuntimeError('ref.as_non_null to a null reference'))).toBe(plain);
  // Older V8's one message for an empty slot and a signature mismatch names no stage.
  expect(report({ inspection: 'stages/inspection.wasm' }, [], new WebAssembly.RuntimeError('null function or function signature mismatch'))).toBe(plain);
});
