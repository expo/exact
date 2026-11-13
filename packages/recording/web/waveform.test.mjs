// `<audio-waveform>`'s bars (LLP 1133.009 D13): a level's height in decibels, a meter's ballistics, and one bar a step
// that scrolls with time however the levels arrive. The Apple implementation's `WaveformBars` is compiled on this Mac
// and must draw the same bars from the same levels at the same times, and its `Meter` must count a silent span whole,
// as the web's does (`level.test.mjs`).
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { synthetic, waveformHeight, WaveformBars } from './index.js';

const root = resolve(import.meta.dir, '../../..');

test('a bar is the level in decibels over the 50 dB below full scale', () => {
  expect(waveformHeight(0)).toBe(0);
  expect(waveformHeight(1)).toBe(1);
  expect(waveformHeight(2)).toBe(1);
  expect(waveformHeight(10 ** (-50 / 20))).toBeCloseTo(0, 12);
  expect(waveformHeight(10 ** (-60 / 20))).toBe(0);
  expect(waveformHeight(0.1)).toBeCloseTo(0.6, 12);
  // Speech near an RMS of 0.03, which the linear level drew a thirtieth of the height.
  expect(waveformHeight(0.03)).toBeGreaterThan(0.38);
  expect(waveformHeight(0.03)).toBeLessThan(0.4);
});

test('a bar rises to a louder level within a step and falls back over a few', () => {
  const bars = new WaveformBars(0);
  bars.advance(250, [0.1, 0.1, 10 ** (-50 / 20), 10 ** (-50 / 20), 10 ** (-50 / 20)]);
  const [rise, held, ...falls] = bars.bars;
  expect(rise).toBeCloseTo(0.6 * (1 - Math.exp(-50 / 20)), 12);
  expect(held).toBeGreaterThan(rise);
  expect(falls[0]).toBeCloseTo(held * Math.exp(-50 / 120), 12);
  expect(falls[2]).toBeGreaterThan(0.1);
  expect(falls).toEqual([...falls].sort((a, b) => b - a));
});

/** The bars' place at each frame: how many slots the newest bar stands from the right edge, as a count from the
 * start, so a bar's own place is `place - k` for the k-th bar ever drawn. */
const place = (bars) => bars.bars.length + bars.phase;

test('levels that arrive 100 ms at a time scroll a step every 50 ms, without a jump', () => {
  const bars = new WaveformBars(0);
  let last = place(bars), delivered = 0;
  for (let t = 0; t <= 5000; t += 1000 / 60) {
    // The input tap's 100 ms: two 50 ms windows, a few milliseconds late.
    while ((delivered + 2) * 50 + 7 <= t) { bars.advance(t, [0.05, 0.05]); delivered += 2; }
    bars.advance(t, []);
    const now = place(bars);
    expect(now - last).toBeGreaterThanOrEqual(-1e-9);
    expect(now - last).toBeLessThanOrEqual((1000 / 60) / 50 + 1e-9);
    last = now;
  }
  expect(bars.bars.length).toBeGreaterThanOrEqual(delivered - 2);
});

test('the bars wait at a full step when no level has come, and a late burst is drawn at once', () => {
  const bars = new WaveformBars(0);
  bars.advance(400, []);
  expect(bars.bars.length).toBe(0);
  expect(bars.phase).toBe(1);
  bars.advance(420, Array(9).fill(0.1));
  expect(bars.bars.length).toBe(5); // one by its step, then down to four behind
  expect(bars.phase).toBeCloseTo(0.4, 12);
});

test("the agent's three seconds draw sixty bars at a step's start", () => {
  const bars = new WaveformBars(1000);
  bars.advance(1000, []);
  bars.advance(4000, Array.from({ length: 60 }, (_, i) => synthetic(Math.floor((1000 + (i + 1) * 50) / 50))));
  expect(bars.bars.length).toBe(60);
  expect(bars.phase).toBe(0);
});

/** The frames of a drive: levels delivered at times, then advanced, as each implementation is asked. */
const drive = [
  ...Array.from({ length: 40 }, (_, i) => ({ t: 1000 + i * 16.7, levels: i % 6 === 0 ? [0.03, 0.2] : [] })),
  { t: 1700, levels: Array.from({ length: 12 }, (_, i) => synthetic(i)) },
  { t: 1710, levels: [] }, { t: 2500, levels: [] }, { t: 2510, levels: [0, 1, 0.5, 1e-4] },
];
const run = (frames) => {
  const bars = new WaveformBars(1000);
  return frames.map(({ t, levels }) => { bars.advance(t, levels); return { bars: [...bars.bars], phase: bars.phase }; });
};

test.skipIf(process.platform !== 'darwin')('Apple draws the same bars from the same levels, and counts silence whole', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/recording-waveform-'));
  const sh = (command, args) => {
    const r = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 180_000 });
    expect(r.status, `${command}: ${r.error ?? r.signal ?? ''}\n${r.stdout}\n${r.stderr}`).toBe(0);
    return r.stdout;
  };
  try {
    sh('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    writeFileSync(join(out, 'main.swift'), `import Foundation
@main struct Main {
  static func main() {
    let frames = try! JSONSerialization.jsonObject(with: Data(CommandLine.arguments[1].utf8)) as! [[String: Any]]
    var bars = WaveformBars(start: 1000)
    let out = frames.map { f -> [String: Any] in
      bars.advance(f["t"] as! Double, f["levels"] as! [Double])
      return ["bars": bars.bars, "phase": bars.phase]
    }
    // A meter on an idle track, read after ten seconds and after an hour more: the whole span, the history's levels.
    let meter = Meter(interval: 50, history: 100, agent: false, now: 0)
    let reads = [10_000.0, 3_610_000].map { t -> [String: Any] in
      let r = meter.read(t)
      return ["duration": r["totalSamplesDuration"]!, "levels": (r["levels"] as! [Double]).count, "level": r["audioLevel"]!]
    }
    print(String(decoding: try! JSONSerialization.data(withJSONObject: ["frames": out, "meter": reads]), as: UTF8.self))
  }
}
`);
    sh('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      'packages/recording/apple/Recording.swift', join(out, 'main.swift'), '-o', join(out, 'waveform')]);
    const { frames: apple, meter } = JSON.parse(sh(join(out, 'waveform'), [JSON.stringify(drive)]));
    expect(meter).toEqual([{ duration: 10, levels: 100, level: 0 }, { duration: 3610, levels: 100, level: 0 }]);
    const web = run(drive);
    expect(apple.length).toBe(web.length);
    apple.forEach((frame, i) => {
      expect(frame.bars.length).toBe(web[i].bars.length);
      frame.bars.forEach((h, k) => expect(Math.abs(h - web[i].bars[k])).toBeLessThan(1e-9));
      expect(Math.abs(frame.phase - web[i].phase)).toBeLessThan(1e-9);
    });
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 240_000);
