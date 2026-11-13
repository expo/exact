// The frequency the web starts a Generic Sensor at: the fastest of its readers and canvas inputs, 60 Hz for a reader
// that names none (Apple's default), over a stand-in `Accelerometer` that records what it was constructed with.
import { afterEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

const started = [];
globalThis.Accelerometer = class extends EventTarget {
  constructor(options) {
    super();
    this.options = options;
    this.timestamp = 0;
    [this.x, this.y, this.z] = [0, 0, 9.8];
  }
  start() {
    started.push(this.options.frequency ?? null);
    queueMicrotask(() => this.dispatchEvent(new Event('reading')));
  }
  stop() {}
};
afterEach(() => { started.length = 0; });

const web = () => implementation({ changed() {}, surfaces: { input: () => ({ push() {}, close() {} }) } });

test('a canvas input starts its sensor at its own frequency', () => {
  const motion = web();
  expect(motion.startSensorInput({ surface: 'canvas', sensor: 'accelerometer', frequency: 200 })).toEqual({ surface: 'canvas', sensor: 'accelerometer' });
  expect(started).toEqual([200]);
});

test('a reader at 200 Hz beside one that names none starts the sensor at 200 Hz', async () => {
  const motion = web();
  await motion.accelerometer({});
  await motion.accelerometer({ frequency: 200 });
  expect(started).toEqual([60, 200]);
});
