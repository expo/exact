// `deviceOrientation`'s answer on Apple and the web. Apple's from attitudes composed of W3C angles: the Apple
// implementation's conversion is compiled on this Mac with a main that reads quaternions and prints the event it
// answers. The web's from events the test dispatches, the implementation called in the order host/web-js/modules.js
// calls it (a reader's new arguments asked, then `releaseArgs` for the old). Core Motion's sensors and a browser's own
// events are the phone checklist's.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { conversions, implementation } from '../web/index.js';

const root = resolve(import.meta.dir, '../../..');
const MAIN = `@main struct Main {
  static func main() {
    let asked = try! JSONSerialization.jsonObject(with: Data(CommandLine.arguments[1].utf8)) as! [[String: Any]]
    let out = try! JSONSerialization.data(withJSONObject: asked.map {
      MotionModule.orientationEvent($0["q"] as! [Double], absolute: $0["absolute"] as! Bool)
    })
    print(String(decoding: out, as: UTF8.self))
  }
}
import Foundation
`;

/** The Apple implementation's event for each `{ q, absolute }` of `asked`: Core Motion's attitude quaternion and
 * whether it is in the x-north frame. */
function apple(asked) {
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/motion-orientation-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 120_000 });
    assert.equal(r.status, 0, `${command}: ${r.error ?? r.signal ?? ''}\n${r.stdout}\n${r.stderr}`);
    return r.stdout;
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    writeFileSync(join(out, 'main.swift'), MAIN);
    run('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      'packages/motion/apple/Motion.swift', join(out, 'main.swift'), '-o', join(out, 'orientation')]);
    return JSON.parse(run(join(out, 'orientation'), [JSON.stringify(asked)]));
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}

/** Whether `got` is `want` within 1e-6°, alpha taken round the circle. */
const same = (got, want) => got.every((g, i) => Math.abs(i ? g - want[i] : ((g - want[i] + 540) % 360) - 180) < 1e-6);

test.skipIf(process.platform !== 'darwin')('Apple answers deviceOrientation in the W3C ranges and frame, as Android and the web', () => {
  // The issue's rows, which Core Motion's yaw, pitch and roll read as (−90, 0, 0), (−180, 60, −180), (−90, 80, −160).
  const rows = [[270, 0, 0], [0, 120, 0], [90, 100, 20]];
  // And attitudes over the whole of each range, off the boundaries where the angles are not unique.
  let seed = 7;
  const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  for (let i = 0; i < 200; i++) rows.push([random() * 360, -179 + random() * 358, -89 + random() * 178]);
  const w3c = rows.map(([a, b, g]) => conversions.quaternionFromAngles(a, b, g));
  // Absolute, Core Motion's attitude is in its x-north frame: (0, 0, −sin 45°, cos 45°) ⊗ the W3C's.
  const s = Math.SQRT1_2, xNorth = ([x, y, z, w]) => [s * (x + y), s * (y - x), s * (z - w), s * (w + z)];
  const got = apple([...w3c.map(q => ({ q, absolute: false })), ...w3c.map(q => ({ q: xNorth(q), absolute: true }))]);
  const wrong = [...rows, ...rows].map((want, i) => ({ want, absolute: i >= rows.length, got: got[i] }))
    .filter(r => r.got.absolute !== r.absolute || !same([r.got.alpha, r.got.beta, r.got.gamma], r.want));
  assert.deepEqual(wrong, []);
  for (const { alpha, beta, gamma } of got) {
    assert.ok(alpha >= 0 && alpha < 360 && beta >= -180 && beta < 180 && gamma >= -90 && gamma < 90, `${alpha} ${beta} ${gamma}`);
  }
}, 240_000);

test('the web answers each deviceOrientation reader from the event its absolute names', async () => {
  // A page whose `deviceorientation` reads alpha 10 and `deviceorientationabsolute` alpha 200.
  globalThis.DeviceMotionEvent ??= class extends Event {};
  globalThis.DeviceOrientationEvent ??= class extends Event {};
  const listening = new Map();
  const add = globalThis.addEventListener, remove = globalThis.removeEventListener;
  globalThis.addEventListener = (type, f) => { listening.set(type, (listening.get(type) ?? 0) + 1); add.call(globalThis, type, f); };
  globalThis.removeEventListener = (type, f) => { listening.set(type, listening.get(type) - 1); remove.call(globalThis, type, f); };
  const fire = setInterval(() => {
    dispatchEvent(Object.assign(new Event('deviceorientation'), { alpha: 10, beta: 0, gamma: 0, absolute: false }));
    dispatchEvent(Object.assign(new Event('deviceorientationabsolute'), { alpha: 200, beta: 0, gamma: 0, absolute: true }));
  }, 5);
  const read = o => `${o.alpha} ${o.absolute}`;
  const types = () => [...listening].filter(([, n]) => n > 0).map(([t]) => t).sort().join(' ');
  try {
    let web = implementation({ changed() {} });
    assert.equal(read(await web.deviceOrientation({ absolute: false })), '10 false');
    // The same reader, now absolute.
    const now = web.deviceOrientation({ absolute: true });
    web.releaseArgs('deviceOrientation', { absolute: false });
    assert.equal(read(await now), '200 true');
    assert.equal(types(), 'deviceorientationabsolute');
    // And relative again.
    const back = web.deviceOrientation({ absolute: false });
    web.releaseArgs('deviceOrientation', { absolute: true });
    assert.equal(read(await back), '10 false');
    assert.equal(types(), 'deviceorientation');
    web.release('deviceOrientation');
    assert.equal(types(), '');

    // Two readers, the absolute one first: each reads its own event.
    web = implementation({ changed() {} });
    assert.equal(read(await web.deviceOrientation({ absolute: true })), '200 true');
    assert.equal(read(await web.deviceOrientation({})), '10 false');
    assert.equal(types(), 'deviceorientation deviceorientationabsolute');
    web.release('deviceOrientation');
    assert.equal(types(), '');
  } finally {
    clearInterval(fire);
    globalThis.addEventListener = add;
    globalThis.removeEventListener = remove;
  }
});

test('a canvas input in the screen frame turns an orientation quaternion as a reader does', async () => {
  // A screen turned a quarter (`screen.orientation.angle` 90) and a stand-in sensor with one reading.
  const screen = globalThis.screen;
  globalThis.screen = { orientation: { angle: 90 } };
  globalThis.RelativeOrientationSensor = class extends EventTarget {
    quaternion = conversions.quaternionFromAngles(30, 10, 0);
    timestamp = 5;
    start() { queueMicrotask(() => this.dispatchEvent(new Event('reading'))); }
    stop() {}
  };
  try {
    const pushed = [];
    const web = implementation({ changed() {}, surfaces: { input: () => ({ push: v => pushed.push(v), close() {} }) } });
    web.startSensorInput({ surface: 'canvas', sensor: 'relativeOrientation', referenceFrame: 'screen' });
    const { quaternion } = await web.relativeOrientationSensor({ referenceFrame: 'screen' });
    assert.deepEqual(pushed[0].values, quaternion);
    assert.deepEqual(quaternion, conversions.quaternionToScreen(conversions.quaternionFromAngles(30, 10, 0), 90));
  } finally {
    globalThis.screen = screen;
    delete globalThis.RelativeOrientationSensor;
  }
});
