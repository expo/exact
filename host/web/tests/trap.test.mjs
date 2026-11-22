// One trap in app.wasm stops the page's calls into it (wasm/boot.js `guardedExports`): a trap leaves the module's
// state as it stopped, so each later call would trap again, and a repeating timer then logged one each frame (#500).
// The functions are read from the source and run against a real module that traps and stand-in frames.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { glueDeclarations } from './glue-fixture.mjs';
import { createTimerScheduler } from '../timer-glue.js';

const boot = readFileSync(new URL('../wasm/boot.js', import.meta.url), 'utf8');
// (module (func (export "trap") (result i32) unreachable) (func (export "one") (result i32) i32.const 1))
const MODULE = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 3, 2, 0, 0,
  7, 14, 2, 4, 116, 114, 97, 112, 0, 0, 3, 111, 110, 101, 0, 1, 10, 10, 2, 3, 0, 0, 11, 4, 0, 65, 1, 11]);

test('the first trap stops the clock and every later call, and is said once', async () => {
  const { instance } = await WebAssembly.instantiate(MODULE);
  const host = {}, root = { dataset: {} }, said = [], told = [];
  const exact = { onTrap: (error) => told.push(error) };
  const { guardedExports, ModuleStopped } = new Function('host', 'root', 'globalThis', 'console',
    `${glueDeclarations(boot, ['ModuleStopped', 'StageNotLoaded', 'guardedExports', 'stopModule'])}\nreturn { guardedExports, ModuleStopped };`)(
    host, root, { exact }, { error: (...args) => said.push(args) });
  const wasm = guardedExports(instance.exports);
  expect(wasm.one()).toBe(1);

  // A clock whose wake is overdue every frame, as `every(50, …)` is after a trap.
  const frames = [];
  let calls = 0;
  host.ticker = createTimerScheduler({ now: () => 1000, advance: () => { calls++; wasm.trap(); }, raf: (fn) => frames.push(fn), cancel: () => {} });
  host.ticker.update(0);
  expect(frames).toHaveLength(1);
  expect(() => frames.shift()(0)).toThrow(WebAssembly.RuntimeError);
  expect(frames).toHaveLength(0); // the scheduler was disposed, so it asks for no frame
  expect(host.ticker).toBe(null);
  expect(calls).toBe(1);

  // Later calls do not enter the module, and nothing is said again.
  expect(() => wasm.one()).toThrow(ModuleStopped);
  expect(() => wasm.trap()).toThrow(ModuleStopped);
  expect(said).toHaveLength(1);
  expect(told).toHaveLength(1);
  expect(told[0]).toBeInstanceOf(WebAssembly.RuntimeError);
  expect(root.dataset.error).toMatch(/^RuntimeError/);
});
