import { afterAll, beforeAll, expect, test } from 'bun:test';
import { implementation } from './index.js';
import { clip } from './clip.js';
import { video } from '../../media-capture/web/video.js';

// A take whose file the store would not write (a full quota) was answered as a success: a path, `reason`
// "requested" and no error, for a file that was empty or missing a part. A recorder now stops at its first failed
// write, writes nothing after it, and answers the failure from its stop and its state.

let recorders = [];
class FakeRecorder extends EventTarget {
  static isTypeSupported() { return true; }
  constructor(stream) { super(); this.stream = stream; this.state = 'inactive'; recorders.push(this); }
  start() { this.state = 'recording'; queueMicrotask(() => this.dispatchEvent(new Event('start'))); }
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
const recordedClip = Uint8Array.from(atob(clip), c => c.charCodeAt(0)).buffer;
const readClip = async () => recordedClip;
beforeAll(() => {
  for (const name of ['MediaRecorder', 'MediaStream', 'document']) saved[name] = globalThis[name];
  // These lifecycle tests supply a saved file and a decoder; browser tests exercise actual decoding.
  globalThis.document = { createElement: () => ({
    duration: 0.3, videoWidth: 16, videoHeight: 16,
    set src(_) { queueMicrotask(() => this.onloadedmetadata()); }, removeAttribute() {}, load() {},
  }) };
  globalThis.MediaRecorder = FakeRecorder;
  globalThis.MediaStream = class {
    constructor(tracks) {
      if (tracks.some(t => !t)) throw new TypeError('a MediaStream needs tracks');
      this.tracks = tracks;
    }
  };
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

test('a video recorder keeps every stream track and stops after all tracks end', async () => {
  recorders = [];
  const consumers = new Map(), released = [];
  const impl = implementation({ changed() {}, agent: false, now: () => 0,
    tracks: {
      lookup: id => id === 'stream' ? { kind: 'stream', tracks: ['camera', 'mic'] } : { kind: id === 'camera' ? 'video' : 'audio' },
      subscribe(id, consumer) {
        consumers.set(id, consumer);
        consumer.live({ id, getSettings: () => ({ width: 1280, height: 720, frameRate: 30 }) });
        return () => released.push(id);
      },
    },
    files: { read: readClip, async write() {}, async append() {} },
  });
  const start = await impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/video' });
  expect(recorders).toHaveLength(1);
  expect(recorders[0].stream.tracks.map(t => t.id)).toEqual(['camera', 'mic']);
  consumers.get('mic').ended();
  expect((await impl.mediaRecorderState({ recorder: start.id })).state).toBe('recording');
  consumers.get('camera').ended();
  const result = await impl.stopMediaRecorder({ recorder: start.id });
  expect(result).toMatchObject({ mimeType: 'video/mp4', width: 16, height: 16, frameRate: 10 });
  expect(released).toEqual(['camera', 'mic']);
});

function pendingRecording(ended = [], read = readClip) {
  recorders = [];
  let now = 0;
  const consumers = new Map(), timers = new Set(), written = [];
  const impl = implementation({ changed() {}, agent: false, now: () => now,
    after(ms, fire) { const timer = { at: now + ms, fire }; timers.add(timer); return () => timers.delete(timer); },
    tracks: {
      lookup: id => id === 'stream' ? { kind: 'stream', tracks: ['camera', 'mic'] }
        : { kind: id === 'camera' ? 'video' : 'audio', state: () => ({ readyState: ended.includes(id) ? 'ended' : 'live' }) },
      subscribe(id, consumer) {
        consumers.set(id, consumer);
        if (ended.includes(id)) consumer.ended();
        return () => consumers.delete(id);
      },
    },
    files: { read, async write(_, bytes) { written.push(bytes); }, async append(_, bytes) { written.push(bytes); } },
  });
  return { impl, consumers, timers, written,
    advance(to) { now = to; for (const t of [...timers]) if (t.at <= now) { timers.delete(t); t.fire(); } },
    live(id) { consumers.get(id).live({ id, getSettings: () => ({ width: 1280, height: 720, frameRate: 30 }) }); },
  };
}

test('the recording clock starts when a reopened camera begins recording', async () => {
  const r = pendingRecording();
  const start = await r.impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/delayed', maxDuration: 500 });
  r.live('mic');
  r.advance(2000);
  expect(await r.impl.mediaRecorderState({ recorder: start.id })).toMatchObject({ state: 'recording', duration: 0, result: null });
  expect(r.timers.size).toBe(0);
  r.live('camera');
  expect(r.timers.size).toBe(0); // MediaRecorder's start event is still queued.
  await Promise.resolve();
  expect([...r.timers].map(t => t.at)).toEqual([2500]);
  r.advance(2500);
  const result = await r.impl.stopMediaRecorder({ recorder: start.id });
  expect(result).toMatchObject({ duration: 300, size: 3, width: 16, height: 16, frameRate: 10 });
  expect(r.written).toHaveLength(1);
  expect(r.consumers.size).toBe(0);
  expect(await r.impl.mediaRecorderState({ recorder: start.id })).toMatchObject({ reason: 'max-duration', error: null, result });
});

test('a recorder joining while the preview reopens an idle camera waits for its source', async () => {
  recorders = [];
  const entries = new Map();
  const tracks = {
    add: entry => entries.set(entry.id, entry),
    lookup: id => entries.get(id),
    subscribe: (id, consumer) => entries.get(id).owner.subscribe(id, consumer),
  };
  const source = () => ({ readyState: 'live', label: 'camera',
    getSettings: () => ({ width: 320, height: 240, frameRate: 30 }),
    stop() { this.readyState = 'ended'; },
  });
  let reopen, ready;
  const resumed = new Promise(resolve => { ready = resolve; });
  const capture = video({ changed() {}, agent: false, tracks, nextId: () => 1, shown: () => true,
    media: { getUserMedia: () => new Promise(resolve => { reopen = resolve; }) },
  });
  const first = source(), camera = await capture.open({}, 'stream', first);
  tracks.add({ id: 'stream', kind: 'stream', tracks: [camera.id] });
  tracks.subscribe(camera.id, {})();
  expect(first.readyState).toBe('ended');
  const leave = tracks.subscribe(camera.id, { live: ready });
  const impl = implementation({ changed() {}, agent: false, now: () => 0, tracks,
    files: { read: readClip, async write() {}, async append() {} },
  });
  let start;
  try {
    start = await impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/reopened' });
    expect(recorders).toHaveLength(0);
    expect(start).toMatchObject({ state: 'recording', error: null, duration: 0 });
    const next = source();
    reopen({ getVideoTracks: () => [next] });
    await resumed;
    expect(recorders).toHaveLength(1);
    expect(recorders[0].stream.tracks).toEqual([next]);
    const result = await impl.stopMediaRecorder({ recorder: start.id });
    expect(result).toMatchObject({ mimeType: 'video/mp4', size: 3, width: 16, height: 16, frameRate: 10 });
    expect(next.readyState).toBe('live');
  } finally {
    if (start) await impl.stopMediaRecorder({ recorder: start.id });
    leave();
    capture.stopTrack(camera.id);
  }
});

test.each(['before subscribing', 'while opening'])('an audio track ended %s does not block the live camera', async when => {
  const r = pendingRecording(when === 'before subscribing' ? ['mic'] : []);
  const start = await r.impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/camera' });
  r.live('camera');
  if (when === 'while opening') r.consumers.get('mic').ended();
  expect(recorders).toHaveLength(1);
  expect(recorders[0].stream.tracks.map(t => t.id)).toEqual(['camera']);
  const result = await r.impl.stopMediaRecorder({ recorder: start.id });
  expect(result).toMatchObject({ mimeType: 'video/mp4', size: 3, width: 16, height: 16, frameRate: 10 });
  expect(r.consumers.size).toBe(0);
});

test.each(['pending', 'empty'])('a recording that is %s cannot succeed without media', async state => {
  const r = pendingRecording();
  const start = await r.impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/empty' });
  if (state === 'empty') {
    r.live('camera'); r.live('mic');
    recorders[0].slice = () => {};
  }
  const result = await r.impl.stopMediaRecorder({ recorder: start.id });
  expect(result.error).toBe('NotReadableError');
  expect(r.written).toHaveLength(0);
  expect(r.consumers.size).toBe(0);
  expect(await r.impl.mediaRecorderState({ recorder: start.id })).toMatchObject({ state: 'inactive', reason: 'error', result: null });
});

for (const [name, read] of [
  ['unreadable', async () => { throw new Error('file read failed'); }],
  ['malformed', async () => new ArrayBuffer(1)],
]) test(`a ${name} video cannot publish guessed metadata`, async () => {
  const r = pendingRecording([], read);
  const start = await r.impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/bad' });
  r.live('camera'); r.live('mic');
  const result = await r.impl.stopMediaRecorder({ recorder: start.id });
  expect(result.error).toBe('NotReadableError');
  expect(r.consumers.size).toBe(0);
  expect(await r.impl.mediaRecorderState({ recorder: start.id })).toMatchObject({ state: 'inactive', reason: 'error', result: null });
  expect(await r.impl.stopMediaRecorder({ recorder: start.id })).toEqual(result);
});
