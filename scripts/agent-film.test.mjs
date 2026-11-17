// Film (LLP 1012.001.000 D2) on the agent's clock and, under `--timing platform`, on the wall's.
// `bun test ./scripts/agent-film.test.mjs`.
import { test, expect } from 'bun:test';
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { film } from './agent-film.mjs';
import { encodePng } from './png.mjs';
import { scaledScreenshot } from './agent-inspect.mjs';

/** A session whose captures take `capture(i)` ms of wall-clock time each, and whose clock moves only by `clock`. */
function filming(capture) {
  const steps = [], took = [];
  const s = {
    now: 1000,
    async clock(spec) { steps.push(spec); s.now += Number(spec.slice(1)); },
    async tagged(r) { return { ...r, epoch: 1, incarnation: 1, clock: s.now }; },
    async op() { return { world: [] }; },
    scaled(r) { return scaledScreenshot(r, undefined, 'ios'); },
  };
  const carrier = {
    async screenshot(file) {
      took.push(performance.now());
      await new Promise((done) => setTimeout(done, capture(took.length - 1)));
      writeFileSync(file, encodePng({ width: 2, height: 2, data: Buffer.alloc(16, 255) }));
      return { screenshot: file, w: 2, h: 2, world: [] };
    },
  };
  return { s, carrier, steps, took };
}

test('film on the agent\'s clock steps it by every and is the clock\'s', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'film-'));
  try {
    const { s, carrier, steps } = filming(() => 0);
    const r = await film(s, { carrier, timing: 'agent' }, resolve(dir, 'a.png'), { over: 100, every: 50 });
    expect(steps).toEqual(['+50', '+50']);
    expect(r.at).toEqual([1000, 1050, 1100]);
    expect(r.frames).toBe(3);
    expect(r.timing).toBeUndefined();
    expect(r.reproducible).toBeUndefined();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// Under platform timing the platform's motion runs on its own: the film samples it every `every` ms of wall-clock
// time, never moves the agent's clock, and names the samples a slow capture missed rather than making them up.
test('film under platform timing samples the wall clock and names what it missed', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'film-'));
  try {
    // The second capture takes 160 ms: the samples due at 100 and 150 ms pass while it runs, and the one due at 200 is late.
    const { s, carrier, steps, took } = filming((i) => (i === 1 ? 160 : 5));
    const r = await film(s, { carrier, timing: 'platform' }, resolve(dir, 'a.apng'), { over: 300, every: 50 });
    expect(steps).toEqual([]);
    expect(s.now).toBe(1000);
    expect(r).toEqual(expect.objectContaining({ timing: 'platform', reproducible: false, every: 50, over: 300, form: 'animated' }));
    expect(r.missed).toEqual([100, 150]);
    expect(r.frames).toBe(5);
    expect(r.at).toHaveLength(5);
    expect(r.at[0]).toBeLessThan(20);
    // Each frame at or after when it was due, at its capture time from the start.
    r.at.forEach((ms, i) => expect(ms).toBeGreaterThanOrEqual([0, 50, 200, 250, 300][i] - 1));
    expect(took[2] - took[0]).toBeGreaterThanOrEqual(180);
    expect(readdirSync(r.dir).sort()).toEqual(r.at.map((ms, i) => `${String(i).padStart(3, '0')}-${ms}ms.png`));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
