// The web implementation's scan claims while the shared reader starts (LLP 1133.035 D4, D6): a claim is the
// session's from its call, so a cancel or `abortScan` before the browser has started the reader ends it, and the
// last one ending aborts the start. `NDEFReader` is a stand-in whose `scan` the test settles.
import { expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

/** The readers made, each with its scan's signal and the means to settle that scan. */
function standIn() {
  const readers = [];
  globalThis.NDEFReader = class {
    constructor() { readers.push(this); }
    scan({ signal }) {
      this.signal = signal;
      return new Promise((resolve, reject) => { this.started = resolve; this.failed = reject; });
    }
  };
  const emitted = [];
  const impl = implementation({ changed() {}, emit: (name, args) => emitted.push([name, args[0].id]), foreground() {} });
  return { impl, readers, emitted };
}
const turns = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); };
const stopped = { reason: 'stopped', reading: null, message: null };

test('a scan cancelled, or every scan aborted, while the reader starts ends and aborts the start', async () => {
  for (const stop of [(impl) => impl.cancel(7), (impl) => impl.abortScan()]) {
    const { impl, readers } = standIn();
    const scan = impl.scan({ once: false }, { request: 7 });
    stop(impl);
    expect(await scan).toEqual(stopped);
    expect(readers[0].signal.aborted).toBe(true);
    readers[0].started();
    await turns();
    // A scan after it starts a reader of its own.
    const next = impl.scan({ once: true }, { request: 8 });
    expect(readers.length).toBe(2);
    readers[1].started();
    await turns();
    readers[1].onreading({ serialNumber: '04', message: { records: [] } });
    expect(await next).toEqual({ reason: 'read', reading: 'r1', message: null });
  }
});

test('a second claim made while the reader starts keeps it when the first is cancelled', async () => {
  const { impl, readers, emitted } = standIn();
  const first = impl.scan({ once: true }, { request: 1 }), second = impl.scan({ once: true }, { request: 2 });
  impl.cancel(1);
  expect(await first).toEqual(stopped);
  expect(readers.length).toBe(1);
  expect(readers[0].signal.aborted).toBe(false);
  readers[0].started();
  await turns();
  readers[0].onreading({ serialNumber: '04', message: { records: [] } });
  expect(await second).toEqual({ reason: 'read', reading: 'r1', message: null });
  expect(emitted).toEqual([['reading', 'r1']]);
  expect(readers[0].signal.aborted).toBe(true);
});

test('a start that fails ends each claim waiting on it with the failure', async () => {
  const { impl, readers } = standIn();
  const first = impl.scan({}, { request: 1 }), second = impl.scan({}, { request: 2 });
  readers[0].failed(new DOMException('denied', 'NotAllowedError'));
  expect(await first).toEqual({ error: 'NotAllowedError', message: 'denied' });
  expect(await second).toEqual({ error: 'NotAllowedError', message: 'denied' });
  // Nothing is left to abort, and the next scan starts again.
  impl.scan({}, { request: 3 });
  expect(readers.length).toBe(2);
});
