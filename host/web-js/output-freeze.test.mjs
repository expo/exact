import { expect, test } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

const wasmCases = JSON.parse(readFileSync(new URL('../web/tests/fixtures/output-freeze.json', import.meta.url), 'utf8'));
const starts = data => data[0] === 0xfa || data[0] === 0xfb || data[0] === 0xf8
  || (data[0] & 0xf0) === 0x90 && data[2] > 0;

function serve() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') return new Response(null, { status: 204 });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes={};', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    const local = new URL('../..' + path, import.meta.url), file = existsSync(local) ? local : new URL('../web/' + path.split('/').pop(), import.meta.url);
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
}

// `hapticOnly`: a page whose only output is a haptic, linked as the compiler links it (`watchOutput`), with no sounds or
// MIDI to install the freeze settlement for it.
async function outputPage({ reader = null, sequence = false, hapticOnly = false } = {}) {
  const server = serve(), browser = await suiteBrowser(), page = await browser.newPage();
  await page.goto(server.url.href);
  await page.evaluate(async ({ reader, sequence, hapticOnly }) => {
    const started = [], stopped = [], sent = [], vibrated = [];
    const parameter = value => ({ value, cancelScheduledValues() {}, setValueAtTime() {}, linearRampToValueAtTime() {} });
    let decoded = 0;
    globalThis.AudioContext = class {
      state = 'running'; currentTime = 0; destination = {};
      getOutputTimestamp() { return { contextTime: 0, performanceTime: performance.now() }; }
      async decodeAudioData() { return { duration: 1, decoded: decoded++ }; }
      async resume() {}
    };
    globalThis.AudioBufferSourceNode = class {
      constructor(_, { buffer, playbackRate }) { this.buffer = buffer; this.playbackRate = parameter(playbackRate); }
      connect() { return this; }
      start(at) { started.push([this.buffer.decoded, at]); }
      stop(at) { stopped.push([this.buffer.decoded, at]); }
    };
    globalThis.GainNode = class {
      constructor(_, { gain }) { this.gain = parameter(gain); }
      connect() { return this; }
      disconnect() {}
    };
    globalThis.StereoPannerNode = class {
      constructor(_, { pan }) { this.pan = parameter(pan); }
      connect() { return this; }
      disconnect() {}
    };
    const output = { id: 'synth', name: 'synth', manufacturer: 'test', state: 'connected',
      send(data, at) { sent.push([[...data], at]); } };
    const access = { inputs: new Map(), outputs: new Map([['synth', output]]), onstatechange: null };
    Object.defineProperty(navigator, 'permissions', { value: { query: async () => ({ state: 'granted' }) }, configurable: true });
    Object.defineProperty(navigator, 'requestMIDIAccess', { value: async () => access, configurable: true });
    Object.defineProperty(navigator, 'userActivation', { value: { hasBeenActive: true }, configurable: true });
    Object.defineProperty(navigator, 'vibrate', { value: value => (vibrated.push(value), true), configurable: true });

    const rt = await import('/host/web-js/rt.js');
    const { install } = await import('/host/web-js/sounds.js');
    const { midi } = await import('/host/web-js/midi.js');
    const { haptic } = await import('/host/web-js/commands.js');
    const sourceA = 'data:audio/wav;base64,AAAA', sourceB = 'data:audio/wav;base64,AAAB';
    rt.data.grants = 'device.midi test';
    if (hapticOnly) (await import('/host/web-js/output.js')).watchOutput();
    rt.useCommands({ haptic });
    if (!hapticOnly) {
      install([[sourceA, 48000, 48000], [sourceB, 48000, 48000]]);
      midi({}, true);
    }

    let online;
    if (reader) {
      const facts = await import('/host/web-js/facts.js');
      facts.page({ online: [[reader]] });
    }
    const changed = rt.sig(false), enabled = rt.sig(false);
    const label = text => parent => {
      const el = document.createElement('div');
      el.textContent = text;
      el.dataset.sharedElement = 'panel';
      el.style.setProperty('--exact-layout-transition', '20 0 linear');
      parent.append(el);
      rt.pr(el);
    };
    rt.mount(root => {
      if (reader) {
        online = rt.res('online', 'exactPage', () => [], undefined, undefined, { onLine: 'b' }, [true]);
        online();
      }
      rt.when(root, changed, label('After'), label('Before'));
    });
    if (sequence) {
      const { sequence: declare } = await import('/host/web-js/transport.js');
      const tracks = [[4, 1, 0, 'synth', [[0, '', 1, '', 60, 100, 0, 1, '', 1, 0]]]];
      declare([[() => enabled(), null, () => [tracks, 120, 0, [], '', 'synth'], null, 'held']]);
    }
    await rt.pieces();
    const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
    let transition;
    document.startViewTransition = update => transition = start(async () => { await gate.promise; return update(); });
    const hold = commands => rt.commit(() => { rt.W(changed, true); commands(); }, 'held output');
    const snapshot = () => ({
      started: started.map(([sound]) => sound),
      stopped: stopped.map(([sound]) => sound),
      sent: sent.map(([data]) => data),
      vibrated: [...vibrated],
      outputJournal: rt.journal.filter(line => /midi dropped|command (playSound|sendMIDI|haptic)|sound /.test(line)),
    });
    window.fixture = {
      rt, sourceA, sourceB, snapshot,
      holdHaptic() { hold(() => rt.C('haptic', ['impact-light'])); },
      holdStarts() {
        hold(() => {
          rt.C('playSound', [sourceA, rt.clock.now, 1, '']);
          rt.C('sendMIDI', ['synth', [0x90, 60, 100], rt.clock.now]);
          rt.C('haptic', ['impact-light']);
        });
      },
      startLive() {
        rt.commit(() => {
          rt.C('playSound', [sourceA, rt.clock.now, 1, '']);
          rt.C('sendMIDI', ['synth', [0x90, 60, 100], rt.clock.now]);
        });
      },
      holdCleanup() {
        hold(() => {
          rt.C('sendMIDI', ['synth', [0x80, 60, 0, 0xb0, 123, 0, 0xfc], rt.clock.now]);
          rt.C('stopSounds', []);
        });
      },
      startGroup() { rt.commit(() => rt.C('playSound', [sourceA, rt.clock.now, 1, 'g'])); },
      holdGroup() { hold(() => rt.C('playSound', [sourceB, rt.clock.now, 1, 'g'])); },
      holdInvalid() {
        hold(() => {
          rt.C('sendMIDI', ['synth', [0x90], rt.clock.now]);
          rt.C('sendMIDI', ['synth', [0x80, 60, 0], NaN]);
          rt.C('sendMIDI', ['gone', [0x80, 60, 0], rt.clock.now]);
        });
      },
      enableSequence() { hold(() => rt.W(enabled, true)); },
      freshStarts() {
        rt.commit(() => {
          rt.C('playSound', [sourceB, rt.clock.now, 1, '']);
          rt.C('sendMIDI', ['synth', [0x90, 65, 100], rt.clock.now]);
          rt.C('haptic', ['impact-light']);
        });
      },
      holdParity(messages) {
        hold(() => {
          for (const { data } of messages) rt.C('sendMIDI', ['synth', data, rt.clock.now]);
          rt.C('playSound', [sourceA, rt.clock.now, 1, '']);
          rt.C('haptic', ['impact-light']);
        });
      },
      freeze() { document.dispatchEvent(new Event('freeze')); },
      resume() { document.dispatchEvent(new Event('resume')); },
      async release() {
        gate.resolve();
        await transition.updateCallbackDone;
        await transition.finished;
      },
    };
  }, { reader, sequence, hapticOnly });
  if (!hapticOnly) await page.waitForFunction(() => exact.sounds?.state().output.decoded === '2/2' && exact.midi?.state().state === 'granted');
  return { server, browser, page, close: async () => { await page.close(); await browser.close(); server.stop(true); } };
}

for (const reader of [null, 'onLine']) {
  test(`a freeze discards standalone starts with ${reader ? 'an onLine-only page reader' : 'no page reader or sequence'}`, async () => {
    const fixture = await outputPage({ reader });
    try {
      await fixture.page.evaluate(() => { fixture.holdStarts(); fixture.freeze(); });
      expect(await fixture.page.evaluate(() => fixture.snapshot())).toMatchObject({ started: [], sent: [], vibrated: [] });
      await fixture.page.evaluate(() => fixture.release());
      expect(await fixture.page.evaluate(() => fixture.snapshot())).toMatchObject({ started: [], sent: [], vibrated: [] });
    } finally { await fixture.close(); }
  }, 30_000);
}

test('a freeze discards a held haptic on a page whose only output is haptics', async () => {
  const fixture = await outputPage({ hapticOnly: true });
  try {
    await fixture.page.evaluate(() => { fixture.holdHaptic(); fixture.freeze(); });
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toMatchObject({ vibrated: [] });
    await fixture.page.evaluate(() => fixture.release());
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toMatchObject({ vibrated: [] });
  } finally { await fixture.close(); }
}, 30_000);

test('a freeze delivers held standalone cleanup before presentation releases', async () => {
  const fixture = await outputPage();
  try {
    await fixture.page.evaluate(() => { fixture.startLive(); fixture.holdCleanup(); fixture.freeze(); });
    const frozen = await fixture.page.evaluate(() => fixture.snapshot());
    expect(frozen.started).toEqual([0]);
    expect(frozen.stopped).toEqual([0]);
    expect(frozen.sent).toEqual([[0x90, 60, 100], [0x80, 60, 0], [0xb0, 123, 0], [0xfc]]);
    await fixture.page.evaluate(() => fixture.release());
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toEqual(frozen);
  } finally { await fixture.close(); }
}, 30_000);

test('a freeze does not flush a deferred sequence start before its background stop', async () => {
  const fixture = await outputPage({ sequence: true });
  try {
    await fixture.page.evaluate(() => { fixture.enableSequence(); fixture.freeze(); });
    const frozen = await fixture.page.evaluate(() => fixture.snapshot());
    expect(frozen.sent.some(starts)).toBe(false);
    expect(frozen.sent).toContainEqual([0xfc]);
    await fixture.page.evaluate(() => fixture.release());
    expect((await fixture.page.evaluate(() => fixture.snapshot())).sent).toEqual(frozen.sent);
  } finally { await fixture.close(); }
}, 30_000);

test('discarding a held grouped sound start still stops the earlier voice', async () => {
  const fixture = await outputPage();
  try {
    await fixture.page.evaluate(() => { fixture.startGroup(); fixture.holdGroup(); fixture.freeze(); });
    const frozen = await fixture.page.evaluate(() => fixture.snapshot());
    expect(frozen.started).toEqual([0]);
    expect(frozen.stopped).toEqual([0]);
    await fixture.page.evaluate(() => fixture.release());
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toEqual(frozen);
  } finally { await fixture.close(); }
}, 30_000);

test('invalid held MIDI commands keep their diagnostics and send nothing at freeze', async () => {
  const fixture = await outputPage();
  try {
    await fixture.page.evaluate(() => { fixture.holdInvalid(); fixture.freeze(); });
    const frozen = await fixture.page.evaluate(() => fixture.snapshot());
    expect(frozen.sent).toEqual([]);
    expect(frozen.outputJournal.filter(line => line.includes('midi dropped:')).map(line => line.replace(/^t=\S+ /, ''))).toEqual([
      'midi dropped: the message at byte 1 needs 3 bytes (sendMIDI to synth)',
      'midi dropped: at is not a number (sendMIDI to synth)',
      'midi dropped: gone is not a connected output (sendMIDI)',
    ]);
    await fixture.page.evaluate(() => fixture.release());
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toEqual(frozen);
  } finally { await fixture.close(); }
}, 30_000);

test('output created after resume survives an older held freeze', async () => {
  const fixture = await outputPage();
  try {
    await fixture.page.evaluate(() => { fixture.holdStarts(); fixture.freeze(); fixture.resume(); fixture.freshStarts(); });
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toMatchObject({ started: [], sent: [], vibrated: [] });
    await fixture.page.evaluate(() => fixture.release());
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toMatchObject({ started: [1], sent: [[0x90, 65, 100]], vibrated: [12] });
  } finally { await fixture.close(); }
}, 30_000);

test('JS freeze settlement classifies the expressible wasm output-freeze fixture sends identically', async () => {
  const rows = [
    wasmCases.short.scheduled.ops[0],
    wasmCases.clock.scheduled.ops[0],
    wasmCases.clock.freeze.ops[0],
    wasmCases.standalone.scheduled.ops.find(row => row.op === 'midi'),
  ];
  const messages = rows.flatMap(row => row.ops).filter(op => op.op === 'send').map(({ data, at }) => ({ data, at }));
  const expected = messages.filter(message => !starts(message.data)).map(message => message.data);
  const fixture = await outputPage();
  try {
    await fixture.page.evaluate(messages => { fixture.holdParity(messages); fixture.freeze(); }, messages);
    const frozen = await fixture.page.evaluate(() => fixture.snapshot());
    expect(frozen).toMatchObject({ started: [], sent: expected, vibrated: [] });
    await fixture.page.evaluate(() => fixture.release());
    expect(await fixture.page.evaluate(() => fixture.snapshot())).toEqual(frozen);
  } finally { await fixture.close(); }
}, 30_000);
