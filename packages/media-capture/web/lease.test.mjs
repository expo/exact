import { expect, test } from 'bun:test';

// A lease given back and taken again while the device opens: two opens were in flight, both found the track active,
// and the second adopted its browser track over the first without stopping it, which then captured until the page
// closed (the camera's light, the browser's recording indicator). Each open adopts only for its own lease.
test('a lease retaken while the device opens leaves no browser track capturing', async () => {
  const made = [];
  const track = () => {
    const t = { label: 'Fake microphone', readyState: 'live', getSettings: () => ({}), stop() { this.readyState = 'ended'; } };
    made.push(t);
    return t;
  };
  const saved = globalThis.navigator;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { mediaDevices: {
    getUserMedia: async () => { await Bun.sleep(50); const t = track(); return { getAudioTracks: () => [t], getTracks: () => [t] }; },
  } } });
  try {
    const { implementation } = await import('./index.js');
    const added = [];
    const impl = implementation({ changed() {}, agent: false, log() {}, tracks: { add: (row) => added.push(row), held: () => false } });
    const opened = await impl.getUserMedia({ audio: true });
    const id = opened.tracks[0].id, owner = added.find((row) => row.id === id).owner;
    // Used once, then idle: its browser track stops.
    owner.subscribe(id, {}, { start: true })();
    expect(made.filter((t) => t.readyState === 'live')).toEqual([]);
    // Start, leave, start again while the device opens; then the last consumer leaves.
    const first = owner.subscribe(id, {}, { start: true });
    first();
    const second = owner.subscribe(id, {}, { start: true });
    await Bun.sleep(150);
    second();
    expect(made.filter((t) => t.readyState === 'live').length).toBe(0);
  } finally {
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: saved });
  }
});

// A camera moved while its track is idle (no consumer, its browser track stopped): the answer and the next lease are
// the camera moved to, not the one the track last used.
test('a camera moved while idle is the one the next lease opens', async () => {
  const cameras = { back: { deviceId: 'cam-back', facingMode: 'environment' }, front: { deviceId: 'cam-front', facingMode: 'user' } };
  const asked = [], made = [];
  const saved = globalThis.navigator;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { mediaDevices: {
    getUserMedia: async ({ video }) => {
      asked.push(video);
      const want = video?.deviceId?.exact ?? video?.deviceId;
      const c = want ? Object.values(cameras).find((x) => x.deviceId === want) : video?.facingMode === 'user' ? cameras.front : cameras.back;
      const t = { label: c.deviceId, readyState: 'live', getSettings: () => ({ ...c }), stop() { this.readyState = 'ended'; } };
      made.push(t);
      return { getVideoTracks: () => [t], getTracks: () => [t] };
    },
  } } });
  try {
    const { implementation } = await import('./index.js');
    const added = [];
    const impl = implementation({ changed() {}, agent: false, log() {}, tracks: { add: (row) => added.push(row), held: () => false } });
    const opened = await impl.getUserMedia({ video: true });
    const id = opened.tracks.find((t) => t.kind === 'video').id, owner = added.find((row) => row.id === id).owner;
    await Bun.sleep(1100);
    expect(made.filter((t) => t.readyState === 'live')).toEqual([]);
    const moved = await impl.applyConstraints({ track: id, facingMode: 'user' });
    expect(moved.label).toBe('cam-front');
    expect(made.filter((t) => t.readyState === 'live')).toEqual([]);
    const leave = owner.subscribe(id, {}, { start: true });
    await Bun.sleep(10);
    expect(asked.at(-1)).toEqual({ deviceId: { exact: 'cam-front' } });
    expect(made.filter((t) => t.readyState === 'live').map((t) => t.label)).toEqual(['cam-front']);
    leave();
  } finally {
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: saved });
  }
});

// A lease given back while its reopen is pending, then taken again: the two reopens complete in either order, each
// opening or failing. Only the newer decides the track, which captures with the newer browser track or ends with its
// failure; the superseded reopen leaves nothing capturing and ends nothing.
for (const kind of ['audio', 'video']) {
  for (const older of ['opens', 'fails']) {
    for (const newer of ['opens', 'fails']) {
      for (const first of ['older', 'newer']) {
        test(`${kind}: the older reopen ${older} and the newer ${newer}, the ${first} completing first`, async () => {
          const made = [], pending = [];
          const stream = () => {
            const t = { label: `Fake ${kind}`, readyState: 'live', getSettings: () => ({}), stop() { this.readyState = 'ended'; } };
            made.push(t);
            return { getAudioTracks: () => [t], getVideoTracks: () => [t], getTracks: () => [t] };
          };
          const saved = globalThis.navigator;
          Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { mediaDevices: {
            // The first open answers at once; each reopen waits for the test to settle it.
            getUserMedia: () => (made.length === 0 ? Promise.resolve(stream()) : new Promise((resolve, reject) => pending.push({
              opens: () => { const s = stream(); resolve(s); return s.getTracks()[0]; },
              fails: () => reject(Object.assign(new Error('busy'), { name: 'NotReadableError' })),
            }))),
          } } });
          try {
            const { implementation } = await import('./index.js');
            const added = [];
            const impl = implementation({ changed() {}, agent: false, log() {}, tracks: { add: (row) => added.push(row), held: () => false } });
            const opened = await impl.getUserMedia({ [kind]: true });
            const id = opened.tracks.find((t) => t.kind === kind).id, owner = added.find((row) => row.id === id).owner;
            // Used once, so idle; then a reopen given back while pending, and another taken.
            owner.subscribe(id, {}, { start: true })();
            owner.subscribe(id, {}, { start: true })();
            let ended = 0;
            const leave = owner.subscribe(id, { ended: () => ended++ }, { start: true });
            expect(pending.length).toBe(2);
            const outcome = { older: () => pending[0][older](), newer: () => pending[1][newer]() };
            const tracksOf = {};
            tracksOf[first] = outcome[first]();
            await Bun.sleep(0);
            const second = first === 'older' ? 'newer' : 'older';
            tracksOf[second] = outcome[second]();
            await Bun.sleep(0);
            const now = await impl.mediaStreamTrack({ track: id });
            const live = made.filter((t) => t.readyState === 'live');
            if (newer === 'opens') expect([now.readyState, ended, live.length, live[0] === tracksOf.newer]).toEqual(['live', 0, 1, true]);
            else expect([now.readyState, ended, live.length]).toEqual(['ended', 1, 0]);
            leave();
            expect(made.filter((t) => t.readyState === 'live')).toEqual([]);
          } finally {
            Object.defineProperty(globalThis, 'navigator', { configurable: true, value: saved });
          }
        });
      }
    }
  }
}
