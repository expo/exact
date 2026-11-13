import { afterEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

const originals = new Map(['BarcodeDetector', 'createImageBitmap', 'OffscreenCanvas']
  .map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
afterEach(() => {
  for (const [name, original] of originals) {
    if (original) Object.defineProperty(globalThis, name, original);
    else delete globalThis[name];
  }
});

function fixture(formats, live = true, { readFrame, files } = {}) {
  const constructed = [], observed = [], detected = [], released = [];
  const frame = { width: 32, height: 24 };
  globalThis.BarcodeDetector = class {
    static async getSupportedFormats() { return typeof formats === 'function' ? formats() : formats; }
    constructor(options) {
      constructed.push(options.formats);
      // The Shape Detection constructor rejects an explicitly empty list.
      if (options.formats?.length === 0) throw new TypeError('Hint option provided, but is empty.');
    }
    async detect(image) { detected.push(image); return []; }
  };
  const impl = implementation({
    changed() {}, agent: true, now: () => 0,
    tracks: {
      lookup: () => ({ kind: 'video', frame: () => readFrame ? readFrame() : frame }),
      observe(track, observer) {
        observed.push(track);
        if (typeof live === 'function' ? live() : live) observer.live();
        return () => { released.push(track); };
      },
    },
    files: files ?? { read() { throw new Error('an unsupported detector must not read the image'); } },
  });
  return { impl, constructed, observed, detected, released, frame };
}

for (const [label, options] of [['formats omitted', {}], ['formats specified', { formats: ['qr_code'] }]]) {
  test(`a still answers NotSupportedError when the browser reads no formats: ${label}`, async () => {
    const { impl, constructed } = fixture([]);
    expect(await impl.getSupportedFormats()).toEqual({ formats: [], tracks: false });
    expect(await impl.detectBarcodes({ image: 'app:/data/a.png', ...options })).toEqual({
      error: 'NotSupportedError', message: 'this browser reads no format',
    });
    expect(constructed).toEqual([]);
  });

  for (const live of [true, false]) {
    test(`${live ? 'a live' : 'an idle'} track answers NotSupportedError when the browser reads no formats: ${label}`, async () => {
      const { impl, constructed, observed } = fixture([], live);
      try {
        expect(await impl.detectedBarcodes({ track: 'camera', ...options })).toEqual({
          error: 'NotSupportedError', message: 'this browser reads no format',
        });
        expect(constructed).toEqual([]);
        expect(observed).toEqual([]);
        expect(impl.state().scans).toEqual([]);
      } finally { impl.destroy(); }
    });
  }

  test(`a supported live scan analyses frames and reuses its detector: ${label}`, async () => {
    const { impl, constructed, detected, frame } = fixture(['qr_code']);
    try {
      expect(await impl.getSupportedFormats()).toEqual({ formats: ['qr_code'], tracks: true });
      for (let count = 1; count <= 2; count++) {
        expect(await impl.detectedBarcodes({ track: 'camera', ...options })).toEqual({
          state: 'scanning', barcodes: [], seen: [], frame: count, width: 32, height: 24, error: null,
        });
      }
      expect(constructed).toEqual([['qr_code']]);
      expect(detected).toEqual([frame, frame]);
    } finally { impl.destroy(); }
  });

  for (const [method, args] of [
    ['detectBarcodes', { image: 'app:/data/a.png' }],
    ['detectedBarcodes', { track: 'camera' }],
  ]) {
    test(`${method} answers NotSupportedError when a pending support query is refreshed to empty: ${label}`, async () => {
      const initial = Promise.withResolvers(), refreshed = Promise.withResolvers();
      const queries = [initial.promise, refreshed.promise];
      const { impl, constructed, observed } = fixture(() => queries.shift());
      const detection = impl[method]({ ...args, ...options });
      impl.foreground(true);
      const answers = Promise.all([detection, impl.getSupportedFormats()]);
      initial.resolve(['qr_code']);
      refreshed.resolve([]);
      try {
        expect(await answers).toEqual([
          { error: 'NotSupportedError', message: 'this browser reads no format' },
          { formats: [], tracks: false },
        ]);
        expect(constructed).toEqual([]);
        expect(observed).toEqual([]);
        expect(impl.state().scans).toEqual([]);
      } finally { impl.destroy(); }
    });

    test(`${method} uses formats refreshed while an empty support query is pending: ${label}`, async () => {
      const initial = Promise.withResolvers(), refreshed = Promise.withResolvers();
      const queries = [initial.promise, refreshed.promise];
      globalThis.createImageBitmap = async () => ({ width: 32, height: 24 });
      globalThis.OffscreenCanvas = class {
        constructor(width, height) { Object.assign(this, { width, height }); }
        getContext() { return { drawImage() {} }; }
      };
      const { impl, constructed, detected } = fixture(() => queries.shift(), true, {
        files: { read: () => new Uint8Array() },
      });
      const detection = impl[method]({ ...args, ...options });
      impl.foreground(true);
      const support = impl.getSupportedFormats();
      initial.resolve([]);
      refreshed.resolve(['qr_code']);
      try {
        expect(await detection).toEqual(method === 'detectBarcodes'
          ? { barcodes: [], width: 32, height: 24 }
          : { state: 'scanning', barcodes: [], seen: [], frame: 1, width: 32, height: 24, error: null });
        expect(await support).toEqual({ formats: ['qr_code'], tracks: true });
        expect(constructed).toEqual([['qr_code']]);
        expect(detected).toHaveLength(1);
      } finally { impl.destroy(); }
    });
  }

  test(`a scan releases its observer if support disappears while a frame is pending: ${label}`, async () => {
    const requested = Promise.withResolvers(), pending = Promise.withResolvers();
    let formats = ['qr_code'];
    const { impl, observed, released, detected, frame } = fixture(() => formats, true, {
      readFrame() { requested.resolve(); return pending.promise; },
    });
    const detection = impl.detectedBarcodes({ track: 'camera', ...options });
    await requested.promise;
    try {
      expect(observed).toEqual(['camera']);
      formats = [];
      impl.foreground(true);
      expect(await impl.getSupportedFormats()).toEqual({ formats: [], tracks: false });
      pending.resolve(frame);
      expect(await detection).toEqual({
        error: 'NotSupportedError', message: 'this browser reads no format',
      });
      expect(detected).toEqual([]);
      expect(released).toEqual(['camera']);
      expect(impl.state().scans).toEqual([]);
    } finally { pending.resolve(frame); await detection; impl.destroy(); }
  });

  test(`an older unsupported read preserves a newer scan after a foreground refresh: ${label}`, async () => {
    const initial = Promise.withResolvers();
    let formats = initial.promise, live = false;
    const { impl, observed, released, detected, frame } = fixture(() => formats, () => live);
    const args = { track: 'camera', ...options };
    const support = impl.getSupportedFormats();
    const previous = impl.detectedBarcodes(args);
    const current = support.then(() => {
      live = true;
      formats = ['qr_code'];
      impl.foreground(true);
      return impl.detectedBarcodes(args);
    });
    initial.resolve([]);
    try {
      await previous;
      expect(await current).toEqual({
        state: 'scanning', barcodes: [], seen: [], frame: 1, width: 32, height: 24, error: null,
      });
      expect(observed).toEqual(['camera']);
      expect(detected).toEqual([frame]);
      expect(released).toEqual([]);
      expect(impl.state().scans).toHaveLength(1);
      impl.releaseArgs('detectedBarcodes', args);
      expect(released).toEqual(['camera']);
      expect(impl.state().scans).toEqual([]);
    } finally { await current; impl.destroy(); }
  });
}

test('an explicitly empty formats option still answers TypeError', async () => {
  const { impl, constructed, observed } = fixture([]);
  expect(await impl.detectBarcodes({ image: 'app:/data/a.png', formats: [] })).toMatchObject({ error: 'TypeError' });
  expect(await impl.detectedBarcodes({ track: 'camera', formats: [] })).toMatchObject({ error: 'TypeError' });
  expect(constructed).toEqual([]);
  expect(observed).toEqual([]);
});

for (const pending of [true, false]) {
  test(`releasing invalid formats preserves ${pending ? 'a pending' : 'an active'} all-formats scan`, async () => {
    const formats = Promise.withResolvers();
    const { impl, observed, released } = fixture(formats.promise);
    const args = { track: 'camera' };
    const read = impl.detectedBarcodes(args);
    if (!pending) { formats.resolve(['qr_code']); await read; }
    try {
      for (const invalid of [[], ['unknown'], 'qr_code']) {
        const badArgs = { ...args, formats: invalid };
        expect(await impl.detectedBarcodes(badArgs)).toMatchObject({ error: 'TypeError' });
        impl.releaseArgs('detectedBarcodes', badArgs);
        expect(released).toEqual([]);
        expect(impl.state().scans.map(s => [s.track, s.formats])).toEqual([['camera', null]]);
      }
      formats.resolve(['qr_code']);
      expect((await read).frame).toBe(1);
      expect((await impl.detectedBarcodes(args)).frame).toBe(2);
      expect(observed).toEqual(['camera']);
      impl.releaseArgs('detectedBarcodes', args);
      expect(released).toEqual(['camera']);
      expect(impl.state().scans).toEqual([]);
    } finally { formats.resolve(['qr_code']); await read; impl.destroy(); }
  });
}

const cleanup = {
  releaseArgs: (impl, args) => impl.releaseArgs('detectedBarcodes', args),
  release: impl => impl.release('detectedBarcodes'),
  destroy: impl => impl.destroy(),
};

test('pending readers share one observer once formats are supported', async () => {
  const formats = Promise.withResolvers();
  const { impl, observed, released } = fixture(formats.promise);
  const args = { track: 'camera' };
  const reads = [impl.detectedBarcodes(args), impl.detectedBarcodes(args)];
  formats.resolve(['qr_code']);
  try {
    expect((await Promise.all(reads)).map(s => s.state)).toEqual(['scanning', 'scanning']);
    expect(observed).toEqual(['camera']);
    impl.releaseArgs('detectedBarcodes', args);
    expect(released).toEqual(['camera']);
    expect(impl.state().scans).toEqual([]);
  } finally { impl.destroy(); }
});

for (const [name, release] of Object.entries(cleanup)) {
  test(`${name} cancels scans waiting for supported formats`, async () => {
    const formats = Promise.withResolvers();
    const { impl, observed, detected } = fixture(formats.promise);
    const args = { track: 'camera' };
    const reads = [impl.detectedBarcodes(args), impl.detectedBarcodes(args)];
    release(impl, args);
    formats.resolve(['qr_code']);
    try {
      await Promise.all(reads);
      expect(observed).toEqual([]);
      expect(detected).toEqual([]);
      expect(impl.state().scans).toEqual([]);
    } finally { impl.destroy(); }
  });
}

test('a released pending scan cannot join a new reader of the same scan', async () => {
  const formats = Promise.withResolvers();
  const { impl, observed, detected, released, frame } = fixture(formats.promise);
  const args = { track: 'camera', formats: ['qr_code'] };
  const previous = impl.detectedBarcodes(args);
  impl.releaseArgs('detectedBarcodes', args);
  const current = impl.detectedBarcodes(args);
  formats.resolve(['qr_code']);
  try {
    expect((await previous).frame).toBe(0);
    expect((await current).frame).toBe(1);
    expect(observed).toEqual(['camera']);
    expect(detected).toEqual([frame]);
    expect(impl.state().scans).toHaveLength(1);
    impl.releaseArgs('detectedBarcodes', args);
    expect(released).toEqual(['camera']);
    expect(impl.state().scans).toEqual([]);
  } finally { impl.destroy(); }
});

test('releasing one pending scan preserves readers with other tracks or formats', async () => {
  const formats = Promise.withResolvers();
  const { impl, observed } = fixture(formats.promise);
  const args = { track: 'camera', formats: ['qr_code'] };
  const reads = [args, { track: 'camera' }, { ...args, track: 'other' }]
    .map(args => impl.detectedBarcodes(args));
  impl.releaseArgs('detectedBarcodes', args);
  formats.resolve(['qr_code']);
  try {
    await Promise.all(reads);
    expect(observed).toEqual(['camera', 'other']);
    expect(impl.state().scans.map(s => [s.track, s.formats])).toEqual([
      ['camera', null], ['other', ['qr_code']],
    ]);
  } finally { impl.destroy(); }
});

// The runner releases each argument group on its own: another interval, or the
// same formats in another order, is another reader of the one scan.
test('a scan ends only when the last argument group reading it is released', async () => {
  const { impl, observed, released } = fixture(['qr_code', 'ean_13']);
  const groups = [
    { track: 'camera', formats: ['qr_code', 'ean_13'], interval: 100 },
    { track: 'camera', formats: ['qr_code', 'ean_13'], interval: 250 },
    { track: 'camera', formats: ['ean_13', 'qr_code'], interval: 100 },
  ];
  try {
    for (const args of groups) expect((await impl.detectedBarcodes(args)).state).toBe('scanning');
    expect(observed).toEqual(['camera']);
    for (const args of groups.slice(0, -1)) {
      impl.releaseArgs('detectedBarcodes', { ...args });
      expect(released).toEqual([]);
      expect(impl.state().scans).toHaveLength(1);
    }
    impl.releaseArgs('detectedBarcodes', { interval: 100, formats: ['ean_13', 'qr_code'], track: 'camera' });
    expect(released).toEqual(['camera']);
    expect(impl.state().scans).toEqual([]);
  } finally { impl.destroy(); }
});
