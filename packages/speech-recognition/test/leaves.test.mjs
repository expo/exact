import { afterEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

// `leavesDevice` read whether the browser's recognizer has `processLocally` at all, not whether this recognition
// set it: on a browser that can recognise locally, a recognition with it false or omitted, which the browser may send
// to its remote service, answered that its audio stays on the device.

const saved = globalThis.SpeechRecognition;
afterEach(() => { globalThis.SpeechRecognition = saved; });

/** A browser recognizer, with `processLocally` (false until set) where `local`; it records the mode it started in. */
function browser(local) {
  class Recognizer extends EventTarget {
    start() { started.push(local ? this.processLocally : null); }
    abort() {}
    stop() {}
  }
  if (local) Recognizer.prototype.processLocally = false;
  const started = [];
  globalThis.SpeechRecognition = Recognizer;
  return started;
}

const start = (processLocally) => {
  const impl = implementation({ changed() {}, agent: false, now: () => 0, log() {}, tracks: { hold: () => () => {} }, files: {} });
  return impl.startSpeechRecognition({ microphone: true, ...(processLocally === undefined ? {} : { processLocally }) });
};

for (const [processLocally, local, leaves, mode] of [
  [undefined, true, true, false],
  [false, true, true, false],
  [true, true, false, true],
  [undefined, false, true, null],
  [false, false, true, null],
]) {
  test(`processLocally ${processLocally} on a browser ${local ? 'with' : 'without'} local recognition: leavesDevice ${leaves}`, async () => {
    const started = browser(local);
    const r = await start(processLocally);
    expect([r.leavesDevice, started]).toEqual([leaves, [mode]]);
  });
}

test('processLocally true on a browser without local recognition is refused, and nothing leaves', async () => {
  const started = browser(false);
  const r = await start(true);
  expect([r.leavesDevice, r.error, started]).toEqual([false, 'service-not-allowed', []]);
});
