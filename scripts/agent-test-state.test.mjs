import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { failureMark, runTests, stateExpectation, stateHolds, unarranged } from './agent-test.mjs';
import { serveBuildTree } from '../host/web/serve.mjs';
import { fileURLToPath } from 'node:url';
import { engineTests } from './coverage.mjs';

const engineTest = await engineTests(['chromium']);

test('script state equality compares lists and records structurally', () => {
  const holds = (got, value) => stateHolds(got, { comparison: 'equal', value });
  expect(holds(['a', 'b'], ['a', 'b'])).toBe(true);
  expect(holds(['a', 'b'], ['a', 'c'])).toBe(false);
  expect(holds(['a', 'b'], ['a'])).toBe(false);
  expect(holds(['a', 'b'], ['b', 'a'])).toBe(false);
  expect(holds([[1, -2], [], [3]], [[1, -2], [], [3]])).toBe(true);
  expect(holds([[1, -2], [], [3]], [[1, 2], [], [3]])).toBe(false);
  expect(holds([], [])).toBe(true);
  expect(holds([], [1])).toBe(false);
  expect(holds([1], ['1'])).toBe(false);
  expect(holds({ id: 'a', position: { x: 1, y: -2 } }, { position: { y: -2, x: 1 }, id: 'a' })).toBe(true);
  expect(holds({ id: 'a' }, { key: 'a' })).toBe(false);
  expect(holds({}, [])).toBe(false);
  expect(holds([NaN], [null])).toBe(false);
  expect(holds([NaN], [NaN])).toBe(false);
  expect(holds([-0], [0])).toBe(true);
  expect(stateHolds([{ id: 'one' }, { id: 'two' }], { comparison: 'not-equal', value: [{ id: 'two' }, { id: 'one' }] })).toBe(true);
  expect(stateHolds({ id: 'one', n: 2 }, { comparison: 'not-equal', value: { n: 2, id: 'one' } })).toBe(false);
  expect(stateHolds({ id: 'one' }, { comparison: 'in', value: [{ id: 'one' }] })).toBe(true);
});

engineTest.chromium('authored state comparisons resolve literals and paths and report both values', async () => {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const dir = mkdtempSync(join(root, 'target/state-literals-'));
  const app = join(dir, 'app.contract'), file = join(dir, 'app.test.contract');
  const plan = join(dir, 'app.plan'), dist = join(dir, 'dist');
  const server = createServer((req, res) => serveBuildTree(dist, req, res));
  try {
    writeFileSync(app, `shape Point
  x: number
  y: number
component App
  state picks = ["a", "b"] persist
  state grid = [[1, -2], [], [3]]
  state empty: number[] = []
  state picked = some(["a", "b"])
  state point = Point(x=1, y=-2)
  state locked = [Point(x=1, y=-2), Point(x=2, y=3)]
  state pinned = [Point(x=1, y=-2), Point(x=2, y=3)]
  state shown = "hint" persist
  state n = 2
  state minimum = 1
  state maximum = 3
  state choices = [1, 2, 3]
  mutation saved as shape Point
  mutation other as shape Point
  action advance
    locked = [Point(x=2, y=3), Point(x=1, y=-2)]
    shown = "advanced"
    saved = some(point)
  action reconcile
    pinned = locked
    other = saved
  view
    column
      button "Advance" testId="advance" press=advance
      button "Reconcile" testId="reconcile" press=reconcile
`);
    writeFileSync(file, `test "equal literals"
  expect state shown == "hint"
  expect state picks == ["a", "b"]
  expect state grid == [[1, -2], [], [3,],]
  expect state empty == []
  expect state picked == some(["a", "b"])
  expect state point == Point(y=-2, x=1)
test "unequal literals"
  expect state picks == ["a", "c"]
  expect state picks == ["a"]
  expect state grid == [[1, 2], [], [3]]
  expect state empty == [1]
  expect state picked == none
  expect state point == Point(x=2, y=-2)
  expect state n != 2
  expect state n >= 3
  expect state n <= 1
  expect state n in 0 1
test "relations advance and reconcile"
  expect state saved == none
  expect state saved == state other
  expect state pinned == state locked
  expect state n != state minimum
  expect state n >= state minimum
  expect state n <= state maximum
  expect state n in state choices
  tap "advance"
  expect state saved != state other
  expect state saved.x == state point.x
  expect state shown != "hint"
  expect state pinned != state locked
  tap "reconcile"
  expect state saved == state other
  expect state pinned.0.x == state locked.0.x
  expect state pinned == state locked
test "relation mismatches"
  expect state n == state minimum
  expect state pinned != state locked
  expect state n >= state maximum
  expect state n <= state minimum
  expect state maximum in state empty
test "an upgrade starts from stored settings"
  stored shown "2:15"
  stored picks [true]
  expect state shown == "2:15"
  expect state picks == ["a", "b"]
  tap "advance"
  reload
  expect state shown == "advanced"
test "the next install owns its store"
  expect state shown == "hint"
`);
    const compiler = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'debug', 'contract');
    const compile = spawnSync(compiler, ['build', app, '-o', plan], { cwd: root, encoding: 'utf8' });
    expect(compile.status, compile.stderr).toBe(0);
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'ui-gallery', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: root, encoding: 'utf8' });
    expect(build.status, build.stderr).toBe(0);
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const r = await runTests({ host: 'web', file, app: dir, url: `http://127.0.0.1:${server.address().port}/` });
    expect(r.results.slice(4).map(t => t.failures)).toEqual([[], []]);
    expect([r.passed, r.failed]).toEqual([4, 2]);
    expect(r.results[0].failures).toEqual([]);
    const messages = r.results[1].failures.map(f => f.slice(f.indexOf(': ', f.indexOf(':') + 1) + 2));
    expect(messages).toEqual([
      'picks is ["a","b"], expected ["a","c"]',
      'picks is ["a","b"], expected ["a"]',
      'grid is [[1,-2],[],[3]], expected [[1,2],[],[3]]',
      'empty is [], expected [1]',
      'picked is ["a","b"], expected null',
      'point is {"x":1,"y":-2}, expected {"x":2,"y":-2}',
      'n is 2, expected != 2',
      'n is 2, expected >= 3',
      'n is 2, expected <= 1',
      'n is 2, expected in [0,1]',
    ]);
    expect(r.results[2].failures).toEqual([]);
    expect(r.results[3].failures.map(f => f.slice(f.indexOf(': ', f.indexOf(':') + 1) + 2))).toEqual([
      'n is 2, expected == state minimum (1)',
      'pinned is [{"x":1,"y":-2},{"x":2,"y":3}], expected != state locked ([{"x":1,"y":-2},{"x":2,"y":3}])',
      'n is 2, expected >= state maximum (3)',
      'n is 2, expected <= state minimum (1)',
      'maximum is 3, expected in state empty ([])',
    ]);
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
}, 180_000);


test('state relations share comparisons and report both operands', () => {
  const state = { slots: { a: 2, b: 3, same: 2, list: [2, 3], row: { id: "one" }, absent: null } };
  const check = (comparison, operand) => stateExpectation(state, { name: 'a', comparison, ...operand });
  for (const [comparison, value, path, failure] of [
    ['equal', 2, 'same', 'b'], ['not-equal', 3, 'b', 'same'],
    ['at-least', 2, 'same', 'b'], ['at-most', 3, 'b', 'missing'],
    ['in', [1, 2], 'list', 'row'],
  ]) {
    expect(check(comparison, { value })).toBeUndefined();
    expect(check(comparison, { state: path })).toBeUndefined();
    expect(check(comparison, { state: failure })).toBeDefined();
  }
  expect(check('equal', { state: 'b' }).message).toBe('a is 2, expected == state b (3)');
  expect(check('not-equal', { value: 2 }).message).toBe('a is 2, expected != 2');
  expect(check('not-equal', { state: 'same' }).message).toBe('a is 2, expected != state same (2)');
  expect(check('at-least', { state: 'b' }).message).toBe('a is 2, expected >= state b (3)');
  expect(stateExpectation(state, { name: 'b', comparison: 'at-most', state: 'a' }).message)
    .toBe('b is 3, expected <= state a (2)');
  expect(stateExpectation(state, { name: 'b', comparison: 'in', state: 'list.0' }).message)
    .toBe('b is 3, expected in state list.0 (2)');
  for (const path of ['typo', 'row.typo', 'list.9', 'row.toString', 'constructor', 'absent.id']) {
    for (const comparison of ['equal', 'not-equal']) {
      expect(check(comparison, { state: path }).message).toMatch(/no state named|has no field/);
      expect(stateExpectation(state, { name: path, comparison, value: 2 }).message).toMatch(/no state named|has no field/);
    }
  }
  expect(check('not-equal', { state: 'row.typo' }).message).toBe('row has no field "typo" (its fields: id)');
  expect(stateExpectation(state, { name: 'absent', comparison: 'equal', value: null })).toBeUndefined();
  expect(stateHolds(2, { comparison: 'at-most', value: '3' })).toBe(false);
  expect(stateHolds(2, { comparison: 'in', value: 2 })).toBe(false);
});

test('unarranged accepts expected resource refusals and 4xx/5xx answer fetch fixtures (LLP 1156.007)', async () => {
  const base = { failures: {}, failed: {}, mutations: {}, faults: { 'https://api.test/ok': 0, 'https://api.test/err': 0 }, journal: 0 };
  const withResourceFailure = { ...base, failures: { feed: 1 }, failed: { feed: 'HTTP 404' } };
  expect(unarranged(base, withResourceFailure)).toEqual(['resource feed was refused once (HTTP 404)']);
  expect(unarranged(base, withResourceFailure, { refusals: ['feed'] })).toEqual([]);
  const failedOnly = { ...base, failed: { feed: 'HTTP 500' } };
  expect(unarranged(base, failedOnly)).toEqual(['resource feed failed (HTTP 500)']);
  expect(unarranged(base, failedOnly, { refusals: ['feed'] })).toEqual([]);
  // A 4xx/5xx answer fetch fault counts as an arranged failure, whereas a 200 fixture does not silence unrelated resource refusals.
  const mark = await failureMark({
    state: async () => ({
      failures: { feed: { count: 1 } },
      failed: { feed: 'HTTP 404' },
      faults: [
        { prefix: 'https://api.test/ok', hits: 2, response: { status: 200 } },
        { prefix: 'https://api.test/err', hits: 1, response: { status: 404 } },
      ],
    }),
    op: async () => ({ next: 5 }),
  });
  expect(mark.faults).toEqual({ 'https://api.test/ok': 0, 'https://api.test/err': 1 });
  expect(unarranged(base, mark)).toEqual([]);
  const okOnly = await failureMark({
    state: async () => ({
      failures: { feed: { count: 1 } },
      failed: { feed: 'HTTP 500' },
      faults: [{ prefix: 'https://api.test/ok', hits: 2, response: { status: 200 } }],
    }),
    op: async () => ({ next: 5 }),
  });
  expect(okOnly.faults).toEqual({ 'https://api.test/ok': 0 });
  expect(unarranged(base, okOnly)).toEqual(['resource feed was refused once (HTTP 500)']);

  // Stream fixtures key separately from fetch fixtures on the same prefix.
  const mixed = await failureMark({
    state: async () => ({
      failures: {},
      failed: {},
      faults: [
        { prefix: 'https://api.test/chat', hits: 1, failureHits: 0, response: { status: 200 } },
        { family: 'stream', prefix: 'https://api.test/chat', method: 'POST', hits: 1, failureHits: 1, stream: { status: 200, mode: 'events', steps: [] } },
      ],
    }),
    op: async () => ({ next: 9 }),
  });
  expect(mixed.faults).toEqual({
    'https://api.test/chat': 0,
    '["stream","https://api.test/chat","POST"]': 1,
  });
});

engineTest.chromium('authored answer stream parks chunks off clock settle and supports mid-stream actions without a server (#2087)', async () => {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const dir = mkdtempSync(join(root, 'target/answer-stream-e2e-'));
  const dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  const server = createServer((req, res) => serveBuildTree(dist, req, res));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const appOrigin = `http://127.0.0.1:${server.address().port}`;
  try {
    writeFileSync(join(dir, 'app.json'), JSON.stringify({
      name: 'Stream Fixture',
      app: { id: 'test.stream-fixture', name: 'Stream Fixture' },
      host: { web: {} },
      rust: false,
    }));
    writeFileSync(join(dir, 'app.contract'), `shape ChatState
  phase: string
  output: string
component App
  state runId = 1
  state active = true
  resource chat = chatStream(runId, active) as shape ChatState
  action stop
    active = false
  action restart
    active = true
    runId = runId + 1
  view
    column
      if failed(chat)
        text "failed" testId="phase"
        text "" testId="output"
      else
        text chat.phase testId="phase"
        text chat.output testId="output"
      button "Stop" testId="stop" press=stop
      button "Restart" testId="restart" press=restart
`);
    writeFileSync(join(dir, 'app.ts'), `export const grants = 'net.fetch https://stream-fixture.invalid';
let acc = '';
export async function answer(_source: string, args: unknown[]) {
  const active = args[1] as boolean;
  if (!active) return { phase: 'stopped', output: acc };
  acc = '';
  let head = '';
  return fetch('https://stream-fixture.invalid/chat', {
    method: 'POST',
    exactStreamOpen: true,
    exactIdleTimeout: 500,
    exactStream(ev: any) {
      if (ev.type === 'open') {
        head = 'open:' + ev.status + ':' + (ev.headers.get('x-stream') ?? '');
        return { phase: 'streaming', output: head + '|' + acc };
      }
      if (ev.type === 'chunk') {
        const parsed = JSON.parse(ev.data);
        acc += parsed.text;
        return { phase: 'streaming', output: head + '|' + acc };
      }
      if (ev.type === 'error') {
        const phase = ev.message === 'the event stream ended' ? 'done' : 'error:' + ev.message;
        return { phase, output: head + '|' + acc };
      }
      return { phase: 'streaming', output: head + '|' + acc };
    },
  });
}
`);
    writeFileSync(file, `test "stop mid-stream before finish"
  answer stream "https://stream-fixture.invalid/chat" method POST with 200 header "x-stream" "s1"
    event {"text":"Hello"} name "chunk" after 0
    event {"text":" world"} name "chunk" after 200
    end after 400
  expect text "output" == "open:200:s1|Hello"
  expect text "phase" == "streaming"
  clock settle
  expect text "output" == "open:200:s1|Hello"
  expect text "phase" == "streaming"
  tap "stop"
  clock data
  expect text "phase" == "stopped"
  clock +500
  expect text "output" == "Hello"
  expect text "phase" == "stopped"
  expect no failures
test "step virtual clock through chunks, reload, and mid-stream fail"
  answer stream "https://stream-fixture.invalid/chat" method POST with 200
    event {"text":"A"} name "chunk" after 0
    event {"text":"B"} name "chunk" after 150
    end after 300
  expect text "output" == "open:200:|A"
  expect text "phase" == "streaming"
  clock +150
  expect text "output" == "open:200:|AB"
  expect text "phase" == "streaming"
  clock +150
  expect text "phase" == "done"
  reload
  expect text "output" == "open:200:|A"
  answer stream "https://stream-fixture.invalid/chat" method POST with 200
    event {"text":"X"} name "chunk" after 0
    fail "connection reset" after 50
  tap "restart"
  clock data
  expect text "output" == "open:200:|X"
  clock +50
  expect text "phase" == "error:connection reset"
  expect no failures
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'stream-fixture', '--out', dist, '--render', 'none'], {
      cwd: root,
      env: { ...process.env, EXACT_APP_DIR: dir },
      stdout: 'pipe',
      stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    const result = await runTests({ host: 'web', app: dir, file, webDist: dist, url: `${appOrigin}/` });
    expect(result.results.map(t => t.failures)).toEqual([[], []]);
    expect([result.passed, result.failed]).toEqual([2, 0]);
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
}, 180_000);
