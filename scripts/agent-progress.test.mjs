// `test` says each test's result as it lands, and where a long test is every so often (a 221-drag test said
// nothing for 20 minutes after the server's line).
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { heartbeat } from './agent.mjs';

test('a long test says its line, its last and how long it has run; a drag says why it is slow', () => {
  const at = { name: 'flings', line: 150, last: 265, since: 0 };
  assert.equal(heartbeat({ ...at, op: 'expect' }, 45_000), 'still running test "flings": 45s, at line 150 of 265 (expect)');
  assert.equal(heartbeat({ ...at, op: 'drag' }, 130_000), 'still running test "flings": 2m10s, at line 150 of 265 (drag); a drag past the viewport is several strokes, each landing before the next');
});

test('test prints each result as it lands and, while a test runs long, where it is', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-test-progress-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Progress', app: { id: 'dev.exact.progress', name: 'Progress' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n');
    writeFileSync(resolve(dir, 'app.test.contract'), 'test "quick"\n  expect text "x" == "Test"\n');
    // A run whose first test lands at once and whose second sits at a drag; the second's result comes after a beat.
    const source = `
      const { main } = await import(${JSON.stringify(resolve(import.meta.dir, 'agent.mjs'))});
      const wait = (ms) => new Promise((r) => setTimeout(r, ms));
      process.exit(await main(['web', '--app', ${JSON.stringify(dir)}, '--test', ${JSON.stringify(resolve(dir, 'app.test.contract'))}], { heartbeatMs: 50, runTests: async ({ atLine, atResult }) => {
        const quick = { name: 'quick', failures: [], notes: [] }, slow = { name: 'slow', failures: ['slow: line 9: no'], notes: [] };
        atLine({ name: 'quick', index: 0, line: 2, last: 2, op: 'expect' });
        atResult(quick);
        console.log('-- between');
        atLine({ name: 'slow', index: 1, line: 5, last: 9, op: 'drag' });
        await wait(200);
        atResult(slow);
        return { results: [quick, slow], passed: 1, failed: 1, skipped: 0 };
      } }));`;
    const r = spawnSync(process.execPath, ['-e', source], { cwd: dir, encoding: 'utf8', timeout: 20_000, env: { ...process.env, EXACT_FEEDBACK: '0' } });
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.deepEqual(r.stdout.split('\n').filter(Boolean), ['test "quick": ok', '-- between', 'test "slow": FAIL', '1 passed, 1 failed']);
    assert.match(r.stderr, /^still running test "slow": \d+s, at line 5 of 9 \(drag\); a drag past the viewport/m);
    assert.doesNotMatch(r.stderr, /running test "quick"/);
    assert.match(r.stderr, /^ {2}slow: line 9: no$/m);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
