import { afterAll, beforeAll, expect, test } from 'bun:test';
import { implementation } from './index.js';

// A take whose file the store would not write (a full quota) was answered as a success: a path, `reason`
// "requested" and no error, for a file that was empty or missing a part. A recorder now stops at its first failed
// write, writes nothing after it, and answers the failure from its stop and its state.

let recorders = [];
class FakeRecorder extends EventTarget {
  static isTypeSupported() { return true; }
  constructor() { super(); this.state = 'inactive'; recorders.push(this); }
  start() { this.state = 'recording'; }
  pause() { this.state = 'paused'; }
  resume() { this.state = 'recording'; }
  requestData() {}
  /** The browser delivers a slice. */
  slice(text) { this.ondataavailable({ data: new Blob([text]) }); }
  stop() {
    this.state = 'inactive';
    // As a browser does: the last slice, then `stop`, after the call returns.
    queueMicrotask(() => { this.slice('end'); this.dispatchEvent(new Event('stop')); });
  }
}
const saved = {};
beforeAll(() => {
  for (const name of ['MediaRecorder', 'MediaStream']) saved[name] = globalThis[name];
  globalThis.MediaRecorder = FakeRecorder;
  globalThis.MediaStream = class { constructor(tracks) { this.tracks = tracks; } };
});
afterAll(() => { for (const [name, value] of Object.entries(saved)) globalThis[name] = value; });

/** A recorder over one track, writing to a store whose `failAt`th write (from 1) throws `QuotaExceededError`. */
async function recording(failAt) {
  recorders = [];
  const written = [], released = [];
  let writes = 0;
  const store = async (kind, bytes) => {
    if (++writes === failAt) throw new DOMException('the quota is used up', 'QuotaExceededError');
    written.push([kind, new TextDecoder().decode(bytes)]);
  };
  const tracks = {
    lookup: (id) => (id === 'stream-1' ? { kind: 'stream', tracks: ['track-1'] } : { kind: 'audio' }),
    subscribe(id, consumer) { consumer.live({ id }); return () => released.push(id); },
  };
  const files = { write: (_, bytes) => store('write', bytes), append: (_, bytes) => store('append', bytes) };
  const impl = implementation({ changed() {}, agent: false, now: () => 0, log() {}, tracks, files });
  const started = await impl.startMediaRecorder({ stream: 'stream-1', mimeType: 'audio/webm', path: 'app:/data/take' });
  return { impl, recorder: started.id, media: recorders[0], written, released };
}
const settle = () => new Promise((r) => setTimeout(r, 10));

test('a take whose first write fails answers the failure, not a path', async () => {
  const { impl, recorder, written, released } = await recording(1);
  const stopped = await impl.stopMediaRecorder({ recorder });
  expect(stopped.error).toBe('QuotaExceededError');
  expect(stopped.message).toContain('app:/data/take.webm');
  const state = await impl.mediaRecorderState({ recorder });
  expect([state.state, state.reason, state.error, state.result]).toEqual(['inactive', 'error', stopped.message, null]);
  expect([written, released]).toEqual([[], ['track-1']]);
  expect(await impl.stopMediaRecorder({ recorder })).toEqual(stopped);
});

test('a later append that fails stops the recorder, and nothing after it is written', async () => {
  const { impl, recorder, media, written, released } = await recording(2);
  media.slice('one');
  media.slice('two');
  media.slice('three');
  await settle();
  const state = await impl.mediaRecorderState({ recorder });
  expect([state.state, state.reason, state.result, media.state]).toEqual(['inactive', 'error', null, 'inactive']);
  expect(state.error).toContain('the quota is used up');
  expect([written, released]).toEqual([[['write', 'one']], ['track-1']]);
  const stopped = await impl.stopMediaRecorder({ recorder });
  expect([stopped.error, stopped.message]).toEqual(['QuotaExceededError', state.error]);
});

test('a take written whole answers its file', async () => {
  const { impl, recorder, media, written, released } = await recording(0);
  media.slice('one');
  const stopped = await impl.stopMediaRecorder({ recorder });
  expect(stopped).toEqual({ path: 'app:/data/take.webm', mimeType: 'audio/webm', duration: 0, size: 6 });
  const state = await impl.mediaRecorderState({ recorder });
  expect([state.reason, state.error, state.result]).toEqual(['requested', null, stopped]);
  expect([written, released]).toEqual([[['write', 'one'], ['append', 'end']], ['track-1']]);
});
