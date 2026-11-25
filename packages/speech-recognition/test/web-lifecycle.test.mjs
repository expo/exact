import { afterEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

const savedRecognition = globalThis.SpeechRecognition;
const savedNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');

afterEach(() => {
  globalThis.SpeechRecognition = savedRecognition;
  if (savedNavigator) Object.defineProperty(globalThis, 'navigator', savedNavigator);
});

function installBrowser({ chrome = 135 } = {}) {
  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: { userAgent: `Mozilla/5.0 Chrome/${chrome}.0.0.0` },
  });
  const instances = [];
  class Recognizer extends EventTarget {
    constructor() {
      super();
      this.starts = [];
      this.stops = 0;
      this.aborts = 0;
      instances.push(this);
    }
    start(source) { this.starts.push(source ?? null); }
    stop() { this.stops++; }
    abort() { this.aborts++; }
  }
  globalThis.SpeechRecognition = Recognizer;
  return instances;
}

function makeTracks() {
  let listener = null;
  let unsubscribed = 0;
  return {
    lookup: id => (id === 'audio-1' ? { kind: 'audio' } : null),
    hold: () => () => {},
    subscribe(_id, consumer) {
      listener = consumer;
      return () => { unsubscribed++; };
    },
    emitLive(source) { listener?.live(source); },
    emitEnded() { listener?.ended(); },
    get unsubscribed() { return unsubscribed; },
  };
}

test('repeated stopSpeechRecognition calls while stopping both resolve and call stop once', async () => {
  const instances = installBrowser();
  const impl = implementation({ changed() {}, agent: false, now: () => 0, log() {}, tracks: makeTracks(), files: {} });
  const started = await impl.startSpeechRecognition({ microphone: true });
  const first = impl.stopSpeechRecognition({ recognition: started.id });
  const second = impl.stopSpeechRecognition({ recognition: started.id });
  expect(instances[0].stops).toBe(1);
  instances[0].onend();
  const [a, b] = await Promise.all([first, second]);
  expect(a).toEqual(b);
  expect(a.error).toBeNull();
});

test('stopSpeechRecognition on a track before live arrives ends cleanly without starting or stopping the recognizer', async () => {
  const instances = installBrowser();
  const tracks = makeTracks();
  const impl = implementation({ changed() {}, agent: false, now: () => 0, log() {}, tracks, files: {} });
  const started = await impl.startSpeechRecognition({ track: 'audio-1' });
  expect(instances[0].starts).toEqual([]);
  const done = await impl.stopSpeechRecognition({ recognition: started.id });
  expect(done.error).toBeNull();
  expect(instances[0].stops).toBe(0);
  expect(tracks.unsubscribed).toBe(1);
  tracks.emitLive({ id: 'late-source' });
  expect(instances[0].starts).toEqual([]);
});

test('track ending while stopping aborts the recognizer and resolves the pending stop', async () => {
  const instances = installBrowser();
  const tracks = makeTracks();
  const impl = implementation({ changed() {}, agent: false, now: () => 0, log() {}, tracks, files: {} });
  const started = await impl.startSpeechRecognition({ track: 'audio-1' });
  tracks.emitLive({ id: 'live-source' });
  expect(instances[0].starts).toEqual([{ id: 'live-source' }]);
  const stopping = impl.stopSpeechRecognition({ recognition: started.id });
  expect(instances[0].stops).toBe(1);
  tracks.emitEnded();
  const done = await stopping;
  expect(instances[0].aborts).toBe(1);
  expect(done.error).toBeNull();
});

test('abortSpeechRecognition while stopping resolves the waiting stop with aborted', async () => {
  const instances = installBrowser();
  const impl = implementation({ changed() {}, agent: false, now: () => 0, log() {}, tracks: makeTracks(), files: {} });
  const started = await impl.startSpeechRecognition({ microphone: true });
  const stopping = impl.stopSpeechRecognition({ recognition: started.id });
  const aborted = await impl.abortSpeechRecognition({ recognition: started.id });
  const done = await stopping;
  expect(aborted.error).toBe('aborted');
  expect(done.error).toBe('aborted');
  expect(instances[0].aborts).toBe(1);
});
