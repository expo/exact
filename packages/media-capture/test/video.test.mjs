// The web video half's leases over a stand-in `getUserMedia` whose opens the test finishes in the order it chooses:
// a camera move and a lease's open in flight together leave one browser track running, the track's own.
import { expect, test } from 'bun:test';
import { video } from '../web/video.js';

class Source {
  constructor(deviceId) { Object.assign(this, { deviceId, readyState: 'live', muted: false, enabled: true, label: deviceId }); }
  stop() { this.readyState = 'ended'; }
  getSettings() { return { deviceId: this.deviceId }; }
  getCapabilities() { return {}; }
}

function camera() {
  const opens = [], made = [];
  const media = {
    getUserMedia: ({ video: c }) => new Promise(resolve => {
      const deviceId = c?.deviceId?.exact ?? c?.deviceId ?? 'back';
      opens.push({ deviceId, finish: () => { const s = new Source(deviceId); made.push(s); resolve({ getVideoTracks: () => [s] }); } });
    }),
  };
  let owner;
  const impl = video({ changed() {}, agent: false, log() {}, tracks: { add: t => { owner = t.owner; } }, nextId: () => 1, media, shown: () => true });
  return { impl, opens, made, owner: () => owner };
}
const settle = () => new Promise(r => setTimeout(r, 0));

for (const first of ['move', 'activation']) {
  for (const before of [true, false]) {
    test(`a move and an activation that opened ${before ? 'before' : 'after'} it, the ${first} finishing first, leave only the track's source running`, async () => {
      const { impl, opens, made, owner } = camera();
      const given = new Source('back');
      const { id } = await impl.open({ videoConstraints: { deviceId: 'back' } }, 'stream-1', given);
      // Idle: the given track is stopped once nothing leases it.
      const live = [];
      const leave = owner().subscribe(id, { live: s => live.push(s) });
      leave();
      expect(given.readyState).toBe('ended');
      let consumer;
      const subscribe = () => { consumer = owner().subscribe(id, { live: s => live.push(s) }); };
      if (before) subscribe();
      const moved = impl.applyConstraints({ track: id, deviceId: 'front' });
      if (!before) subscribe();
      await settle();
      // Each asks `getUserMedia` as it is called.
      const [move, activation] = before ? [opens[1], opens[0]] : opens;
      expect([move.deviceId, activation.deviceId]).toEqual(['front', before ? 'back' : 'front']);
      for (const o of first === 'move' ? [move, activation] : [activation, move]) { o.finish(); await settle(); }
      await moved;
      const running = made.filter(s => s.readyState === 'live');
      expect(running).toHaveLength(1);
      expect(impl.track(id).readyState).toBe('live');
      expect(live.at(-1)).toBe(running[0]);
      consumer();
      expect(made.every(s => s.readyState === 'ended')).toBe(true);
    });
  }
}
