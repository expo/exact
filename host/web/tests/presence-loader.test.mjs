import { test, expect } from 'bun:test';
import { presenceLoader } from '../navigation/motion.js';
import { readFileSync } from 'node:fs';
import { glueCase, glueDeclarations, wasmSource } from './glue-fixture.mjs';
import '../midi-glue.js';
import '../sound-glue.js';

const runnerOutput = JSON.parse(readFileSync(new URL('./fixtures/output-freeze.json', import.meta.url), 'utf8'));
const copy = value => structuredClone(value);

function midiClock() {
  let time = 0;
  const timers = [];
  const real = { setTimeout: globalThis.setTimeout, clearTimeout: globalThis.clearTimeout };
  globalThis.setTimeout = (call, delay = 0) => { const timer = { call, at: time + delay }; timers.push(timer); return timer; };
  globalThis.clearTimeout = timer => { const i = timers.indexOf(timer); if (i >= 0) timers.splice(i, 1); };
  return {
    now: () => time,
    advance(to) {
      while (true) {
        timers.sort((a, b) => a.at - b.at);
        const timer = timers[0];
        if (!timer || timer.at > to) break;
        timers.shift(); time = Math.max(time, timer.at); timer.call();
      }
      time = to;
    },
    restore() { Object.assign(globalThis, real); },
  };
}

function heldCommit(host, holding = true, apply = () => {}) {
  const held = [], noop = () => {};
  host.page = { hold: batch => {
    if (!holding) return false;
    if (!held.includes(batch)) held.push(batch);
    return true;
  } };
  const bindings = { host, presence: { hold: () => false }, navigation: { holdLink: () => null, project: noop },
    letGo: noop, agentMode: false, apply, refitTexts: noop, flowBatch: noop,
    motion: { commit: noop }, arrange: { commit: noop }, clocks: { sync: noop }, globalThis: { exact: {} } };
  const source = readFileSync(new URL('../wasm/commit.js', import.meta.url), 'utf8');
  const applyBatch = new Function(...Object.keys(bindings), `${glueDeclarations(source, ['heldLinks', 'heldOutputs', 'applyBatch'])}; return applyBatch;`)(...Object.values(bindings));
  return { applyBatch, release() { holding = false; for (const batch of held) applyBatch(batch); } };
}

function commandDispatcher() {
  const source = readFileSync(new URL('../wasm/commit.js', import.meta.url), 'utf8');
  const command = glueCase(source, 'command');
  return new Function('batch', 'host', `const AGENT_ADMITTED = false, focusCommands = []; for (const op of batch.ops ?? []) { switch (op.op) { ${command} } }`);
}

function documentHolder() {
  const source = readFileSync(new URL('../document-glue.js', import.meta.url), 'utf8');
  const documentBoot = new Function(`${glueDeclarations(source, ['documentBoot'])}; return documentBoot;`)();
  const saved = { document: globalThis.document, addEventListener: globalThis.addEventListener, removeEventListener: globalThis.removeEventListener };
  const root = { addEventListener() {}, removeEventListener() {}, querySelector: () => null,
    querySelectorAll: () => [], replaceChildren() {}, contains: () => false };
  globalThis.document = { querySelector: () => null, readyState: 'loading', activeElement: null };
  globalThis.addEventListener = globalThis.removeEventListener = () => {};
  try { return documentBoot({ root, views: new Map(), dispatch() {} }); }
  finally { Object.assign(globalThis, saved); }
}

function replayedCommit(host, holder, apply) {
  let applyBatch, release;
  const noop = () => {};
  if (holder === 'document') {
    host.page = documentHolder();
    release = () => host.page.release(applyBatch);
  } else {
    let ready;
    const presence = presenceLoader(() => new Promise(resolve => { ready = resolve; }), {}, batch => applyBatch(batch));
    release = async () => { ready(() => ({})); await presence.pending(); };
    host.page = { hold: () => false };
    host.presence = presence;
  }
  const bindings = { host, presence: host.presence ?? { hold: () => false }, navigation: { holdLink: () => null, project: noop },
    letGo: noop, agentMode: false, apply: batch => apply(batch, host), refitTexts: noop, flowBatch: noop,
    motion: { commit: noop }, arrange: { commit: noop }, clocks: { sync: noop }, globalThis: { exact: {} } };
  const source = readFileSync(new URL('../wasm/commit.js', import.meta.url), 'utf8');
  applyBatch = new Function(...Object.keys(bindings), `${glueDeclarations(source, ['heldLinks', 'heldOutputs', 'applyBatch'])}; return applyBatch;`)(...Object.values(bindings));
  return { applyBatch, release };
}

async function heldMidi(holding = true) {
  const sent = [], clock = midiClock();
  const output = { id: 'synth', name: 'synth', manufacturer: 'test', state: 'connected', send(data, at) { sent.push([[...data], at]); } };
  const access = { inputs: new Map(), outputs: new Map([['synth', output]]), onstatechange: null };
  const midiOut = globalThis.exact.installMidi({ log: () => {}, origin: () => 0, deliver: () => {}, now: clock.now,
    nav: { permissions: { query: async () => ({ state: 'granted' }) }, requestMIDIAccess: async () => access } });
  await Promise.resolve(); await Promise.resolve();
  return { sent, clock, ...heldCommit({ midiOut }, holding) };
}

const send = (id, data, at) => ({ op: 'send', id, output: 'synth', data, at });
const retract = id => ({ op: 'retract', id });
const midiBatch = (ops, clock, freeze = false) => ({ ops: [{ op: 'midi', ops }], clock, ...(freeze ? { freeze: true } : {}) });

test('the wasm presentation hold cannot reveal MIDI note starts after their freeze cleanup', async () => {
  const cases = [
    { name: 'one note', scheduled: [send(1, [0x90, 60, 100], 100), send(2, [0x80, 60, 0], 500)],
      freeze: [retract(2), send(3, [0x80, 60, 0], 150)], expected: [[0x80, 60, 0]] },
    { name: 'two notes', scheduled: [send(1, [0x90, 60, 100], 100), send(2, [0x80, 60, 0], 500),
      send(3, [0x90, 61, 100], 120), send(4, [0x80, 61, 0], 520)],
      freeze: [retract(2), retract(4), send(5, [0x80, 60, 0], 150), send(6, [0x80, 61, 0], 150)],
      expected: [[0x80, 60, 0], [0x80, 61, 0]] },
    { name: 'no held note-off', scheduled: [send(1, [0x90, 60, 100], 100)],
      freeze: [send(2, [0x80, 60, 0], 150)], expected: [[0x80, 60, 0]] },
  ];
  for (const scenario of cases) {
    const fixture = await heldMidi();
    try {
      fixture.applyBatch(midiBatch(scenario.scheduled, 0));
      fixture.clock.advance(150);
      fixture.applyBatch(midiBatch(scenario.freeze, 150, true));
      fixture.clock.advance(300);
      fixture.release();
      fixture.clock.advance(600);
      expect(fixture.sent.map(([data]) => data), scenario.name).toEqual(scenario.expected);
    } finally {
      fixture.clock.restore();
    }
  }
});

test('the runner\'s ordinary overlapping notes survive the wasm output path, held or not', async () => {
  for (const holding of [false, true]) {
    const fixture = await heldMidi(holding);
    try {
      fixture.applyBatch(copy(runnerOutput.overlap));
      if (holding) fixture.release();
      fixture.clock.advance(700);
      expect(fixture.sent.map(([data]) => data), holding ? 'held' : 'unheld').toEqual([
        [0x90, 60, 100], [0x80, 60, 0], [0x90, 60, 100], [0x80, 60, 0],
      ]);
    } finally {
      fixture.clock.restore();
    }
  }
});

test('a runner live refill keeps a note whose start and end are already due', async () => {
  const fixture = await heldMidi(false);
  try {
    fixture.clock.advance(200);
    fixture.applyBatch(copy(runnerOutput.late));
    fixture.clock.advance(400);
    expect(fixture.sent.map(([data]) => data).filter(data => data[1] === 60)).toEqual([
      [0x80, 60, 0], [0x90, 60, 100],
    ]);
  } finally {
    fixture.clock.restore();
  }
});

test('a freeze discards the runner\'s held short note, even if the page resumes before release', async () => {
  for (const resume of [false, true]) {
    const fixture = await heldMidi();
    try {
      fixture.applyBatch(copy(runnerOutput.short.scheduled));
      fixture.clock.advance(150);
      fixture.applyBatch(copy(runnerOutput.short.freeze));
      if (resume) fixture.applyBatch({ ops: [], clock: 200 });
      fixture.clock.advance(300);
      fixture.release();
      fixture.clock.advance(600);
      expect(fixture.sent.map(([data]) => data), resume ? 'resumed' : 'frozen').toEqual([[0x80, 60, 0]]);
    } finally {
      fixture.clock.restore();
    }
  }
});

test('a freeze sends the runner\'s MIDI Stop without replaying its held Start or ticks', async () => {
  const fixture = await heldMidi();
  try {
    fixture.applyBatch(copy(runnerOutput.clock.scheduled));
    fixture.clock.advance(150);
    fixture.applyBatch(copy(runnerOutput.clock.freeze));
    fixture.clock.advance(300);
    fixture.release();
    fixture.clock.advance(600);
    expect(fixture.sent.map(([data]) => data)).toEqual([[0xfc]]);
  } finally {
    fixture.clock.restore();
  }
});

test('a freeze discards the runner\'s expired held sample and past-due haptic through the real sound output', async () => {
  const clock = midiClock(), started = [], vibrated = [];
  const real = { AudioBufferSourceNode: globalThis.AudioBufferSourceNode, GainNode: globalThis.GainNode,
    StereoPannerNode: globalThis.StereoPannerNode, vibrate: globalThis.navigator?.vibrate };
  const parameter = value => ({ value, cancelScheduledValues() {}, setValueAtTime() {}, linearRampToValueAtTime() {} });
  globalThis.AudioBufferSourceNode = class {
    constructor(_, { playbackRate }) { this.playbackRate = parameter(playbackRate); }
    connect() { return this; }
    start(at) { started.push(at); }
    stop() {}
  };
  globalThis.GainNode = class { constructor(_, { gain }) { this.gain = parameter(gain); } connect() { return this; } disconnect() {} };
  globalThis.StereoPannerNode = class { constructor(_, { pan }) { this.pan = parameter(pan); } connect() { return this; } disconnect() {} };
  Object.defineProperty(globalThis.navigator, 'vibrate', { value: value => (vibrated.push(value), true), configurable: true });
  const context = { state: 'running', get currentTime() { return clock.now() / 1000; }, destination: {},
    decodeAudioData: async () => ({ duration: 0.02 }) };
  const soundOut = globalThis.exact.installSound({ files: ['assets/tick.wav'], log: () => {}, origin: () => 0,
    context, timestamp: () => ({ contextTime: clock.now() / 1000, performanceTime: clock.now() }), now: clock.now,
    activated: () => true, fetchFile: async () => new ArrayBuffer(0) });
  for (let i = 0; i < 10 && soundOut.state().decoded !== '1/1'; i++) await Promise.resolve();
  expect(soundOut.state().decoded).toBe('1/1');
  const fixture = heldCommit({ soundOut });
  try {
    fixture.applyBatch(copy(runnerOutput.sound.scheduled));
    clock.advance(150);
    fixture.applyBatch(copy(runnerOutput.sound.freeze));
    clock.advance(300);
    fixture.release();
    clock.advance(300);
    await Promise.resolve();
    expect({ started, vibrated }).toEqual({ started: [], vibrated: [] });
  } finally {
    clock.restore();
    globalThis.AudioBufferSourceNode = real.AudioBufferSourceNode;
    globalThis.GainNode = real.GainNode;
    globalThis.StereoPannerNode = real.StereoPannerNode;
    Object.defineProperty(globalThis.navigator, 'vibrate', { value: real.vibrate, configurable: true });
  }
});

test('document and presence replays preserve output created after a freeze', async () => {
  const dispatch = commandDispatcher();
  for (const holder of ['document', 'presence']) {
    const clock = midiClock(), sent = [], started = [], vibrated = [];
    const real = { AudioBufferSourceNode: globalThis.AudioBufferSourceNode, GainNode: globalThis.GainNode,
      StereoPannerNode: globalThis.StereoPannerNode, vibrate: globalThis.navigator.vibrate };
    const parameter = value => ({ value, cancelScheduledValues() {}, setValueAtTime() {}, linearRampToValueAtTime() {} });
    globalThis.AudioBufferSourceNode = class {
      constructor(_, { playbackRate }) { this.playbackRate = parameter(playbackRate); }
      connect() { return this; }
      start(at) { started.push(at); }
      stop() {}
    };
    globalThis.GainNode = class { constructor(_, { gain }) { this.gain = parameter(gain); } connect() { return this; } disconnect() {} };
    globalThis.StereoPannerNode = class { constructor(_, { pan }) { this.pan = parameter(pan); } connect() { return this; } disconnect() {} };
    Object.defineProperty(globalThis.navigator, 'vibrate', { value: value => (vibrated.push(value), true), configurable: true });
    const output = { id: 'synth', name: 'synth', manufacturer: 'test', state: 'connected',
      send(data, at) { sent.push([[...data], Math.max(at, clock.now())]); } };
    const access = { inputs: new Map(), outputs: new Map([['synth', output]]), onstatechange: null };
    const midiOut = globalThis.exact.installMidi({ log: () => {}, origin: () => 0, deliver: () => {}, now: clock.now,
      nav: { permissions: { query: async () => ({ state: 'granted' }) }, requestMIDIAccess: async () => access } });
    await Promise.resolve(); await Promise.resolve();
    const context = { state: 'running', get currentTime() { return clock.now() / 1000; }, destination: {},
      decodeAudioData: async () => ({ duration: 1 }) };
    const soundOut = globalThis.exact.installSound({ files: ['assets/tick.wav'], log: () => {}, origin: () => 0,
      context, timestamp: () => ({ contextTime: clock.now() / 1000, performanceTime: clock.now() }), now: clock.now,
      activated: () => true, fetchFile: async () => new ArrayBuffer(0) });
    for (let i = 0; i < 10 && soundOut.state().decoded !== '1/1'; i++) await Promise.resolve();
    expect(soundOut.state().decoded).toBe('1/1');
    const fixture = replayedCommit({ midiOut, soundOut }, holder, dispatch);
    try {
      fixture.applyBatch(copy(runnerOutput.standalone.scheduled));
      clock.advance(150);
      fixture.applyBatch(copy(runnerOutput.standalone.freeze));
      clock.advance(200);
      fixture.applyBatch(copy(runnerOutput.standalone.resume));
      fixture.applyBatch(copy(runnerOutput.standalone.fresh));
      clock.advance(300);
      await fixture.release();
      clock.advance(400);
      expect(sent.sort((a, b) => a[1] - b[1]), holder).toEqual([
        [[0x80, 64, 0], 280], [[0x90, 64, 100], 300], [[0x80, 64, 0], 380],
      ]);
      expect(started, holder).toEqual([expect.closeTo(0.22, 8)]);
      expect(vibrated, holder).toEqual([12]);
    } finally {
      clock.restore();
      globalThis.AudioBufferSourceNode = real.AudioBufferSourceNode;
      globalThis.GainNode = real.GainNode;
      globalThis.StereoPannerNode = real.StereoPannerNode;
      Object.defineProperty(globalThis.navigator, 'vibrate', { value: real.vibrate, configurable: true });
    }
  }
});

test('a direct haptic in the held freeze commit is discarded', async () => {
  const clock = midiClock(), sent = [], sounds = [], vibrated = [], real = globalThis.navigator.vibrate;
  Object.defineProperty(globalThis.navigator, 'vibrate', { value: value => (vibrated.push(value), true), configurable: true });
  const output = { id: 'synth', name: 'synth', manufacturer: 'test', state: 'connected', send(data, at) { sent.push([[...data], at]); } };
  const access = { inputs: new Map(), outputs: new Map([['synth', output]]), onstatechange: null };
  const midiOut = globalThis.exact.installMidi({ log: () => {}, origin: () => 0, deliver: () => {}, now: clock.now,
    nav: { permissions: { query: async () => ({ state: 'granted' }) }, requestMIDIAccess: async () => access } });
  await Promise.resolve(); await Promise.resolve();
  const fixture = replayedCommit({ midiOut, soundOut: { files: [], ops: ops => sounds.push(...ops) } }, 'document', commandDispatcher());
  try {
    const freeze = copy(runnerOutput.standalone.scheduled);
    freeze.freeze = true;
    fixture.applyBatch(freeze);
    clock.advance(300);
    await fixture.release();
    expect({ sent, sounds, vibrated }).toEqual({ sent: [[[0x80, 64, 0], 280]], sounds: [], vibrated: [] });
  } finally {
    clock.restore();
    Object.defineProperty(globalThis.navigator, 'vibrate', { value: real, configurable: true });
  }
});

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
    const batches = (split ? [[ops[0]], ops.slice(1)] : [ops]).map(ops => ({ ops: [{ op: 'sound', ops: [...ops] }], clock: 0 }));
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
        { op: 'midi', ops: [{ op: 'send', id: 3, data: [0x90, 60, 100] }] }], timers: false, clock: 0 };
      applyBatch(scheduled);
      await Promise.resolve();
      expect(controller.signal.aborted).toBe(false);
      expect(sent).toEqual([]);
      active.clear();
      const freeze = { ops: [
        { op: 'sound', ops: [{ op: 'end', id: 1, at: 50 }, { op: 'unhaptic', id: 2 }, { op: 'play', id: 8, at: 50 }, { op: 'haptic', id: 1 }] },
        { op: 'midi', ops: [{ op: 'retract', id: 3 }, { op: 'send', id: 4, data: [0xfc] }, { op: 'send', id: 9, data: [0x90, 62, 100] }] }], timers: false, clock: 50, freeze: true };
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
      expect([...sounds, ...haptics]).toEqual([]);
      expect([...midi]).toEqual([4]);
      expect(sent).toEqual([[0xfc]]);
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
