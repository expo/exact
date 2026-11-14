import { test, expect, afterEach } from 'bun:test';
import { implementation } from '../web/index.js';

// The web implementation's browser path over a stand-in `speechSynthesis`
// that behaves as Chrome's does: an utterance spoken in the turn of a
// `cancel()` or of an utterance's `error` event is dropped, and fires
// neither `end` nor `error`.
function browser() {
  const synth = { given: [], dropped: [], unsettled: false };
  const unsettle = () => {
    synth.unsettled = true;
    setTimeout(() => { synth.unsettled = false; });
  };
  synth.speak = s => (synth.unsettled ? synth.dropped : synth.given).push(s);
  synth.cancel = unsettle;
  synth.fail = s => { unsettle(); s.onerror({ error: 'synthesis-failed' }); };
  synth.pause = synth.resume = () => {};
  synth.getVoices = () => [];
  globalThis.speechSynthesis = synth;
  globalThis.SpeechSynthesisUtterance = class { constructor(text) { this.text = text; } };
  const impl = implementation({ agent: false, now: () => 0, log: () => {}, changed: () => {} });
  const spoken = () => synth.given.map(s => s.text);
  return { synth, impl, spoken };
}
const turn = () => new Promise(resolve => setTimeout(resolve));

afterEach(() => {
  delete globalThis.speechSynthesis;
  delete globalThis.SpeechSynthesisUtterance;
});

test('dropping the speaking utterance gives the next to the browser after the cancel turn', async () => {
  const { synth, impl, spoken } = browser();
  const a = await impl.speak({ text: 'one' });
  const b = await impl.speak({ text: 'two' });
  expect(spoken()).toEqual(['one']);
  impl.dispose([a.id]);
  await turn();
  expect(synth.dropped).toEqual([]);
  expect(spoken()).toEqual(['one', 'two']);
  expect(await impl.speechUtterance({ utterance: b.id })).toMatchObject({ state: 'speaking' });
  // Its `end` ends it, and the queue is free for a later line.
  synth.given[1].onend();
  await impl.speak({ text: 'three' });
  expect(spoken()).toEqual(['one', 'two', 'three']);
});

test('an utterance that fails gives the next to the browser after the error turn', async () => {
  const { synth, impl, spoken } = browser();
  const a = await impl.speak({ text: 'one' });
  await impl.speak({ text: 'two' });
  synth.fail(synth.given[0]);
  expect(await impl.speechUtterance({ utterance: a.id })).toMatchObject({ outcome: 'error', error: 'synthesis-failed' });
  await turn();
  expect(synth.dropped).toEqual([]);
  expect(spoken()).toEqual(['one', 'two']);
});

test('an utterance dropped before its turn comes is never given to the browser', async () => {
  const { impl, spoken } = browser();
  const a = await impl.speak({ text: 'one' });
  const b = await impl.speak({ text: 'two' });
  await impl.speak({ text: 'three' });
  impl.dispose([a.id, b.id]);
  await turn();
  expect(spoken()).toEqual(['one', 'three']);
});
