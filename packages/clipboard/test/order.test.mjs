// The web implementation's write order (LLP 1133.006 D2): a write that reads an app file is superseded from before
// that read, so a later write, `cancel` or `destroy` that comes while the file is read ends it. Under the agent, the
// browser's decoder and canvas stood in for, since only the order is tested here.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

// A 1×1 PNG's signature and IHDR: what `pngSize` reads.
const PNG = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
const stood = {};
beforeAll(() => {
  for (const name of ['ImageDecoder', 'OffscreenCanvas']) stood[name] = Object.getOwnPropertyDescriptor(globalThis, name);
  globalThis.ImageDecoder = class { decode() { return Promise.resolve({ image: { displayWidth: 1, displayHeight: 1, close() {} } }); } close() {} };
  globalThis.OffscreenCanvas = class { getContext() { return { drawImage() {} }; } convertToBlob() { return Promise.resolve(new Blob([PNG], { type: 'image/png' })); } };
});
afterAll(() => {
  for (const [name, was] of Object.entries(stood)) if (was) Object.defineProperty(globalThis, name, was); else delete globalThis[name];
});

/** The implementation under the agent, its file read held until the test lets it finish. */
function reading() {
  let finish;
  const read = new Promise(ok => { finish = () => ok(PNG.buffer.slice(0)); });
  return { clip: implementation({ agent: true, log: () => {}, files: { read: () => read } }), finish: () => { finish(); return new Promise(ok => setTimeout(ok)); } };
}

test('a write that completes while an earlier one reads its file stays on the clipboard', async () => {
  const { clip, finish } = reading();
  const first = clip.write({ data: [{ 'image/png': 'app:/data/big.png' }] }, { request: 1 });
  expect(await clip.writeText({ data: 'later' }, { request: 2 })).toBe(null);
  await finish();
  expect((await first)?.error).toBe('AbortError');
  expect(clip.state()).toEqual({ writes: 1, lastWrite: { types: ['text/plain'], pasteboard: 'memory', text: 'later' } });
});

for (const end of ['cancel', 'destroy']) {
  test(`a write whose operation ends (${end}) while it reads its file writes nothing`, async () => {
    const { clip, finish } = reading();
    const first = clip.write({ data: [{ 'image/png': 'app:/data/big.png' }] }, { request: 1 });
    if (end === 'cancel') clip.cancel(1); else clip.destroy();
    await finish();
    expect((await first)?.error).toBe('AbortError');
    expect(clip.state()).toEqual({ writes: 0 });
  });
}

test('a write that reads its file and is not superseded writes its image', async () => {
  const { clip, finish } = reading();
  const first = clip.write({ data: [{ 'image/png': 'app:/data/big.png' }] }, { request: 1 });
  await finish();
  expect(await first).toBe(null);
  expect(clip.state().lastWrite.types).toEqual(['image/png']);
});

// Off the agent, over a `navigator.clipboard` stand-in whose writes settle when the test says: the clipboard holds
// the write that settles last.
test('the browser is given one write at a time, and a write superseded while it waits is never given', async () => {
  const had = { clipboard: Object.getOwnPropertyDescriptor(navigator, 'clipboard'), item: globalThis.ClipboardItem };
  const given = [], pending = [];
  let held = '';
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: {
    write: async ([item]) => {
      const text = await item.data['text/plain'].text();
      given.push(text);
      await new Promise(ok => pending.push(ok));
      held = text;
    },
  } });
  globalThis.ClipboardItem = class { constructor(data) { this.data = data; } };
  const settle = () => new Promise(ok => setTimeout(ok));
  try {
    const clip = implementation({ agent: false, log: () => {}, files: {} });
    const first = clip.writeText({ data: 'first' }, { request: 1 });
    await settle();
    const second = clip.writeText({ data: 'second' }, { request: 2 });
    await settle();
    const third = clip.writeText({ data: 'third' }, { request: 3 });
    await settle();
    expect(given).toEqual(['first']);
    pending.shift()();
    expect(await first).toBe(null);
    expect((await second)?.error).toBe('AbortError');
    await settle();
    expect(given).toEqual(['first', 'third']);
    pending.shift()();
    expect(await third).toBe(null);
    expect(held).toBe('third');
    expect(clip.state()).toMatchObject({ writes: 2, lastWrite: { text: 'third' } });
  } finally {
    if (had.clipboard) Object.defineProperty(navigator, 'clipboard', had.clipboard); else delete navigator.clipboard;
    if (had.item) globalThis.ClipboardItem = had.item; else delete globalThis.ClipboardItem;
  }
});
