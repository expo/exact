import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const target = fileURLToPath(new URL('../../../target/', import.meta.url));
const threeArms = ['native', 'gpui', 'ink'];
const fourArms = [...threeArms, 'egui'];

function withSeries(callbacks, check, arms = fourArms) {
  mkdirSync(target, { recursive: true });
  const directory = mkdtempSync(join(target, 'gpui-aggregate-'));
  const folder = join(directory, 'series', 'fling');
  mkdirSync(folder, { recursive: true });
  // Each fixture has its own publication, so --verify needs no retained Mac series.
  const script = join(directory, 'aggregate.mjs');
  copyFileSync(new URL('./aggregate.mjs', import.meta.url), script);
  const windows = [[1, 2], [3.5, 4.5], [6, 7], [8.5, 9.5]];
  try {
    for (const arm of arms) for (const [index, counts] of callbacks.entries()) {
      const raw = {
        arm, accepted: true, gpuHookEnabled: false, coldMs: 1,
        inputWindows: windows,
        presented: windows.flatMap(([start]) => [start + 0.25, start + 0.5, start + 0.75]),
        fling: {
          start: { time: 1, cpu: 0, main: 0 },
          end: { time: 11, cpu: 1, main: 0.5, footprint: 2 ** 20 },
          peak: 2 ** 21, wheelEvents: 8,
          displayLink: [1, 1.25, 1.5].map(timestamp => ({ timestamp })),
        },
        rest: {
          start: { time: 11, cpu: 1, main: 0.5 },
          end: { time: 21, cpu: 1.01, main: 0.51, footprint: 2 ** 20 },
        },
      };
      const stem = join(folder, `${arm}-${index + 1}`);
      writeFileSync(stem + '.json', JSON.stringify(raw));
      writeFileSync(stem + '.log', counts.map(emptyCallbacks => JSON.stringify({
        posted: 2, maxGapMs: 10,
        ...(emptyCallbacks === undefined ? {} : { emptyCallbacks }),
      })).join('\n'));
    }
    const run = mode => spawnSync(process.execPath, [script, directory, ...(mode ? [mode] : [])], { encoding: 'utf8' });
    const output = mode => {
      const result = run(mode);
      assert.equal(result.status, 0, result.stderr);
      return result.stdout;
    };
    check({ directory, folder, run, output });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function verifyPublication({ directory, output }) {
  const report = output();
  writeFileSync(join(directory, 'RESULTS.md'), report);
  assert.match(output('--verify'), /reproduced exactly: two tables/);
  return report;
}

test('historical driver records leave empty callbacks unmeasured and still verify', () => {
  withSeries([[undefined, undefined, undefined, undefined]], fixture => {
    const summary = JSON.parse(fixture.output('--json'));
    for (const arm of threeArms) {
      assert(!('emptyCallbacks' in summary.samples[arm][0]));
      assert(!('emptyCallbacks' in summary.metrics[arm]));
      assert.deepEqual(summary.metrics[arm].posted, { median: 8, min: 8, max: 8 });
    }
    assert(!fixture.output().includes('Driver empty callbacks per sample'));
    const report = verifyPublication(fixture);
    writeFileSync(join(fixture.directory, 'RESULTS.md'), report.replace('100.00 (100.00–100.00)', '99.00 (99.00–99.00)'));
    const mismatch = fixture.run('--verify');
    assert.notEqual(mismatch.status, 0, 'publication must still match the other metrics');
    assert.match(mismatch.stderr, /G3c table differs/);
  }, threeArms);
});

test('measured zero and nonzero callbacks retain their totals and distribution', () => {
  withSeries([[0, 0, 0, 0], [1, 2, 3, 4]], fixture => {
    const summary = JSON.parse(fixture.output('--json'));
    for (const arm of fourArms) {
      assert.deepEqual(summary.samples[arm].map(sample => sample.emptyCallbacks), [0, 10]);
      assert.deepEqual(summary.metrics[arm].emptyCallbacks, { median: 5, min: 0, max: 10 });
    }
    assert(fixture.output().includes('| Driver empty callbacks per sample | ' + Array(4).fill('5 (0–10)').join(' | ') + ' |'));
    verifyPublication(fixture);
  });
});

test('mixed availability reports only complete sample totals with their measured count', () => {
  withSeries([
    [undefined, undefined, undefined, undefined], [0, 0, 0, 0], [1, 2, 3, 4], [1, undefined, 3, 4],
  ], fixture => {
    const summary = JSON.parse(fixture.output('--json'));
    for (const arm of fourArms) {
      assert(!('emptyCallbacks' in summary.samples[arm][0]));
      assert(!('emptyCallbacks' in summary.samples[arm][3]));
      assert.deepEqual(summary.metrics[arm].emptyCallbacks, { median: 5, min: 0, max: 10 });
    }
    assert(fixture.output().includes('| Driver empty callbacks per sample | '
      + Array(4).fill('5 (0–10); 2/4 measured').join(' | ') + ' |'));
    verifyPublication(fixture);
  });
  withSeries([[1, undefined, 3, 4]], fixture => {
    assert(fixture.output().includes('| Driver empty callbacks per sample | — | — | — | — |'));
  });
});

test('missing optional callbacks do not relax validation of required metrics', () => {
  withSeries([[undefined, undefined, undefined, undefined]], fixture => {
    for (const [extension, change] of [
      ['json', raw => { delete raw.fling.end.cpu; }],
      ['log', row => { delete row.posted; }],
      ['log', row => { delete row.maxGapMs; }],
    ]) {
      const path = join(fixture.folder, 'native-1.' + extension);
      const original = readFileSync(path, 'utf8');
      const rows = original.split('\n').map(JSON.parse);
      rows.forEach(change);
      writeFileSync(path, rows.map(row => JSON.stringify(row)).join('\n'));
      const result = fixture.run('--json');
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /missing or nonfinite metric|four gesture reports/);
      writeFileSync(path, original);
    }
  });
});

test('a supplied invalid callback measurement is rejected even beside an absent field', () => {
  for (const invalid of [null, '0']) withSeries([[0, invalid, undefined, 0]], fixture => {
    const result = fixture.run('--json');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /invalid emptyCallbacks/);
  });
});
