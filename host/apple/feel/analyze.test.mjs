import { expect, test } from 'bun:test';
import { measures, measuresReport } from './analyze.mjs';

// A measured step's window: the frames in it, its late frames and whose their sampled time was; frames and samples
// outside the window are not counted.
test('a measured step counts its window alone', () => {
  const t = {
    hello: { media: 100, wall: 0, maxFps: 60 },
    frames: [
      { t: 99.9, dt: 16.7, busy: 50 },
      { t: 100.1, dt: 16.7, busy: 4 }, { t: 100.2, dt: 100, busy: 80 }, { t: 100.3, dt: 16.7, busy: 3 },
      { t: 103, dt: 200, busy: 150 },
    ],
    wheres: [
      { t: 100.2, dt: 100, ms: { 'Exact host · UIKit': 60, 'app code · maps': 15 } },
      { t: 103, dt: 200, ms: { 'app JS': 150 } },
    ],
  };
  const [m] = measures(t, [{ name: 'measure tap "Info"', kind: 'measure', start: 0, end: 1000 }, { name: 'step tap', kind: 'tap', start: 0, end: 10 }]);
  expect(m.step).toBe('tap "Info"');
  expect(m.busyMs).toBe(87);
  expect(m.lateFrames).toBe(1);
  expect(m.lateMs).toBe(83);
  expect(m.by.map((b) => [b.who, b.ms])).toEqual([['Exact host', 60], ['app code', 15]]);
  expect(measuresReport([m])).toContain('Exact host 60 ms (in UIKit 60) · app code 15 ms (in maps 15)');
  expect(measures({ ...t, hello: null }, [])).toEqual([]);
});
