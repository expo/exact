import { test, expect } from 'bun:test';
import { presenceLoader } from '../navigation/motion.js';
import { readFileSync } from 'node:fs';
import { glueDeclarations, wasmSource } from './glue-fixture.mjs';

test('the wasm grouped-sound batch keeps a timed end after its play, including across held batches', () => {
  const source = readFileSync(new URL('../wasm/commit.js', import.meta.url), 'utf8');
  for (const held of [false, true]) for (const split of [false, true]) {
    let holding = held;
    const received = [], noop = () => {};
    const host = { soundOut: { ops: ops => received.push(...ops) }, page: { hold: () => holding } };
    const bindings = { host, presence: { hold: () => false }, navigation: { holdLink: () => null, project: noop },
      letGo: noop, agentMode: false, apply: noop, refitTexts: noop, flowBatch: noop,
      motion: { commit: noop }, arrange: { commit: noop }, clocks: { sync: noop }, globalThis: { exact: {} } };
    const applyBatch = new Function(...Object.keys(bindings), `${glueDeclarations(source, ['heldLinks', 'heldOutputs', 'applyBatch'])}; return applyBatch;`)(...Object.values(bindings));
    // playSounds with two overlapping hits in one choke group, as take_sounds emits it.
    const ops = [{ op: 'play', id: 1, sound: 0, at: 0, gain: 1 }, { op: 'end', id: 1, at: 100 },
      { op: 'play', id: 2, sound: 0, at: 100, gain: 1 }];
    const batches = (split ? [[ops[0]], ops.slice(1)] : [ops]).map(ops => ({ ops: [{ op: 'sound', ops: [...ops] }] }));
    for (const batch of batches) applyBatch(batch);
    if (held) {
      expect(received).toEqual([]);
      holding = false;
      for (const batch of batches) applyBatch(batch);
    }
    expect(received).toEqual(ops);
  }
});

test('the wasm freeze cancels streams and output before presence or page holds, without replaying old output', async () => {
  const source = readFileSync(new URL('../wasm/commit.js', import.meta.url), 'utf8');
  for (const pageHold of [false, true]) {
    for (const loaded of [false, true]) {
      const active = new Set([7]), forgettable = new Map(), controller = new AbortController();
      forgettable.set(controller, 7);
      const holds = ticket => active.has(ticket);
      const letGo = new Function('forgettable', 'holds', `${glueDeclarations(wasmSource, ['letGo'])}; return letGo;`)(forgettable, holds);
      const sounds = new Set([1]), haptics = new Set([2]), midi = new Set([3]), sent = [];
      const soundOut = { files: [], reset() { sounds.clear(); haptics.clear(); }, ops(list) {
        for (const op of list) {
          if (op.op === 'play') sounds.add(op.id);
          if (op.op === 'end') sounds.delete(op.id);
          if (op.op === 'haptic') haptics.add(op.id);
          if (op.op === 'unhaptic') haptics.delete(op.id);
        }
      } };
      const midiOut = { ops(list) {
        for (const op of list) {
          if (op.op === 'send') { midi.add(op.id); sent.push(op.data); }
          if (op.op === 'retract') midi.delete(op.id);
        }
      } };
      let ready, readySound, readyMidi, holding = true;
      const host = { page: { hold: batch => { if (!pageHold || !holding) return false; held.push(batch); return true; } },
        soundModule: new Promise(resolve => { readySound = resolve; }), midiModule: new Promise(resolve => { readyMidi = resolve; }),
        soundOut: loaded ? soundOut : null, midiOut: loaded ? midiOut : null };
      const held = [], presented = [];
      const presence = presenceLoader(() => new Promise(resolve => { ready = resolve; }), {}, batch => applyBatch(batch));
      const noop = () => {};
      const bindings = { presence, host, navigation: { holdLink: () => null, project: noop }, letGo, agentMode: false,
        apply: batch => { presented.push(batch); return batch.timers; }, refitTexts: noop, flowBatch: noop,
        motion: { commit: noop }, arrange: { commit: noop }, clocks: { sync: noop }, globalThis: { exact: {} } };
      const applyBatch = new Function(...Object.keys(bindings), `${glueDeclarations(source, ['heldLinks', 'heldOutputs', 'applyBatch'])}; return applyBatch;`)(...Object.values(bindings));
      const scheduled = { ops: [{ op: 'exit', id: 1 },
        { op: 'sound', ops: [{ op: 'play', id: 1, at: 100 }, { op: 'end', id: 1, at: 200 }, { op: 'haptic', id: 2 }] },
        { op: 'midi', ops: [{ op: 'send', id: 3, data: [0x90, 60, 100] }] }], timers: false };
      applyBatch(scheduled);
      await Promise.resolve();
      expect(controller.signal.aborted).toBe(false);
      expect(sent).toEqual([]);
      active.clear();
      const freeze = { ops: [
        { op: 'sound', ops: [{ op: 'end', id: 1, at: 50 }, { op: 'unhaptic', id: 2 }, { op: 'play', id: 8, at: 50 }, { op: 'haptic', id: 1 }] },
        { op: 'midi', ops: [{ op: 'retract', id: 3 }, { op: 'send', id: 4, data: [0xfc] }, { op: 'send', id: 9, data: [0x90, 62, 100] }] }], timers: false };
      expect(applyBatch(freeze).batch).toBe(freeze);
      expect(controller.signal.aborted).toBe(true);
      if (!loaded) { readySound(soundOut); readyMidi(midiOut); await Promise.resolve(); }
      expect(forgettable.size).toBe(0);
      expect([...sounds, ...haptics]).toEqual([]);
      expect([...midi]).toEqual([4]);
      expect(sent).toEqual([[0xfc]]);
      expect(presented).toEqual([]);
      if (pageHold) { holding = false; for (const batch of held.splice(0)) applyBatch(batch); }
      ready(() => ({}));
      await presence.pending();
      expect(presented).toEqual([scheduled, freeze]);
      expect([...sounds, ...haptics]).toEqual([8, 1]);
      expect([...midi]).toEqual([4, 9]);
      expect(sent).toEqual([[0xfc], [0x90, 62, 100]]);
    }
  }
});

test('a failed presence load releases held and later batches once, in order', async () => {
  let fail, calls = 0;
  const applied = [], journal = [];
  const load = () => { calls++; return new Promise((_, reject) => { fail = reject; }); };
  const apply = batch => { if (!presence.hold(batch)) applied.push(batch); };
  const presence = presenceLoader(load, {}, apply, line => journal.push(line));
  const exit = { ops: [{ op: 'exit', id: 1, css: 'leave 200ms' }, { op: 'destroy', id: 1 }] };
  const update = { ops: [{ op: 'style', id: 2, css: '--exact-layout-transition:200 0 linear' }] };
  apply(exit);
  apply(update);
  expect(applied).toEqual([]);
  fail(new Error('offline'));
  await new Promise(resolve => setTimeout(resolve, 0));
  apply(exit);
  apply(update);
  expect(applied).toEqual([exit, update, exit, update]);
  expect(calls).toBe(1);
  expect(presence.live).toBeNull();
  expect(journal).toHaveLength(1);
  expect(journal[0]).toContain('offline');
});

test('a loaded presence module releases the original batch order', async () => {
  let ready;
  const applied = [], live = {};
  const apply = batch => { if (!presence.hold(batch)) applied.push(batch); };
  const presence = presenceLoader(() => new Promise(resolve => { ready = resolve; }), {}, apply);
  const batches = [{ ops: [{ op: 'exit', id: 1 }] }, presence.resize({ ops: [{ op: 'destroy', id: 1 }] }), { ops: [] }];
  for (const batch of batches) apply(batch);
  ready(() => live);
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(applied).toEqual(batches);
  expect(presence.live).toBe(live);
  expect(applied[1].presenceSnap).toBe(true);
});

// A plan that names a presence row loads the module after first paint (`preload`, glue.js after boot), before any batch
// carries the row: a view whose bound layout transition declares nothing at boot then moves from where it was when it
// first gains one. The agent waits for the scheduled load and the load under way (`pending`), and for none once it lands
// or fails; a failed module is not fetched again by a later boot's preload.
test('preload schedules the one load after paint, which pending names until it lands', async () => {
  const raf = globalThis.requestAnimationFrame, frames = [];
  globalThis.requestAnimationFrame = f => frames.push(f);
  const paint = () => { for (let n = 0; n < 2; n++) for (const f of frames.splice(0)) f(); }; // afterPaint's two frames
  try {
    let ready, calls = 0;
    const live = {};
    const presence = presenceLoader(() => { calls++; return new Promise(resolve => { ready = resolve; }); }, {}, () => {});
    expect(presence.pending()).toBeNull();
    expect(await presence.landed(0)).toBe(true);
    presence.preload();
    presence.preload();
    expect(calls).toBe(0); // not before paint
    const waiting = presence.pending();
    expect(waiting).toBeInstanceOf(Promise);
    expect(await presence.landed(0)).toBe(false);
    paint();
    expect(calls).toBe(1);
    expect(presence.hold({ ops: [{ op: 'create', id: 1, css: '--exact-layout-transition:200 0 linear' }] })).toBe(false);
    ready(() => live);
    await waiting;
    expect([presence.live, presence.pending(), calls]).toEqual([live, null, 1]);
    // A failed one stays failed.
    let fail; calls = 0;
    const failed = presenceLoader(() => { calls++; return new Promise((_, reject) => { fail = reject; }); }, {}, () => {}, () => {});
    failed.preload();
    paint();
    fail(new Error('offline'));
    await failed.pending();
    failed.preload();
    paint();
    expect([failed.pending(), calls]).toEqual([null, 1]);
  } finally { globalThis.requestAnimationFrame = raf; }
});

// Only the two presence rows, or an exit, fetch the module: another custom property (press feedback, a tint) does not.
test('a batch with another custom property fetches nothing', () => {
  let calls = 0;
  const presence = presenceLoader(() => { calls++; return new Promise(() => {}); }, {}, () => {});
  expect(presence.hold({ ops: [{ op: 'style', id: 1, css: '--exact-press: 0.96; --exact-tint: red; font-family: "A;--exact-layout-transition:x"' }] })).toBe(false);
  expect(calls).toBe(0);
  presence.hold({ ops: [{ op: 'style', id: 1, css: '--exact-exit-animation: leave 200ms' }] });
  expect(calls).toBe(1);
});
