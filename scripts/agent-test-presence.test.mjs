import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

test('authored presence answers wait for a prompt and use the driver ticket path on every host', () => {
  const root = fileURLToPath(new URL('..', import.meta.url));
  mkdirSync(join(root, 'target'), { recursive: true });
  const dir = mkdtempSync(join(root, 'target/presence-test-'));
  const file = join(dir, 'app.test.contract'), script = join(dir, 'check.mjs');
  const properties = join(dir, 'properties.test.contract');
  const answers = ['allow', 'cancel', 'fail', 'lockout', 'invalidated'];
  const pairs = ['unlock', 'authenticate'].flatMap(call => answers
    .filter(answer => call === 'unlock' || answer !== 'invalidated').map(answer => [call, answer]));
  const cases = pairs.map(([call, answer]) =>
    `test "${call} ${answer}"\n  tap "${call}"\n  presence ${answer}\n  expect state choice == "${answer}"\n`);
  writeFileSync(join(dir, 'app.contract'), 'component App\n  state choice = ""\n  view\n    text "Test"\n');
  writeFileSync(file, cases.join('') + `test "missing"
  presence allow
test "consumed"
  tap "unlock"
  presence allow
  presence allow
test "ambiguous"
  tap "two"
  presence allow
test "unsettled"
  tap "timeout"
  presence allow
test "host refusal"
  tap "refused"
  presence allow
test "scoped" on ios, android
  tap "unlock"
  presence allow
`);
  writeFileSync(properties, `always "presence keeps choice empty"
  expect state choice == ""
always "choice stays empty"
  expect state choice stays ""
` + pairs.map(([call, answer]) =>
    `test "${call} ${answer}"\n  tap "${call}"\n  presence ${answer}\n`).join(''));
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    let sessionInput, currentHost, replies, inputs;
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({open: async () => {
      let queued, pending = [], choice = '', ticket = 70;
      const s = {
        host: currentHost, now: 0,
        state: async () => ({pending, slots: {choice}, mutationRequests: 0}),
        op: async req => {
          if (req.op === 'state') return s.state();
          if (req.op === 'tap') {
            assert.equal(pending.length, 1);
            assert.equal(req.ticket, pending[0].ticket);
            const call = pending[0].device.args.call;
            pending = [];
            if (call === 'refused') return {error: 'presence answer refused'};
            choice = req.choice;
            replies.push({call, ...req});
          }
          return {clock: 0, lines: []};
        },
        clock: async spec => {
          assert.equal(spec, 'data');
          if (queued === 'timeout') return {settled: false, diagnostic: 'requests still in flight'};
          if (queued) {
            const hold = () => ({ticket: ++ticket, name: 'check', device: {capability: 'presence', args: {call: queued}}});
            pending = queued === 'two' ? [hold(), hold()] : [hold()];
            queued = null;
          }
          return {settled: true};
        },
        landed: r => ({...r, delivery: 'substituted'}),
        declaration: async () => null,
        logs: async () => ({lines: []}), close: async () => {},
      };
      Object.assign(s, sessionInput(s, {carrier: {}, host: currentHost, timing: 'agent'}));
      const tap = s.tap;
      s.tap = async (target, options) => {
        if (target.startsWith('@')) return tap(target, options);
        queued = target;
        return {delivery: 'substituted'};
      };
      return s;
    }}));
    const {runTests} = await import(${JSON.stringify(fileURLToPath(new URL('./agent-test.mjs', import.meta.url)))});
    ({sessionInput} = await import(${JSON.stringify(fileURLToPath(new URL('./agent-input.mjs', import.meta.url)))}));
    for (currentHost of ['web', 'macos', 'ios', 'tvos', 'linux', 'windows', 'android']) {
      replies = []; inputs = [];
      const scoped = ['ios', 'android'].includes(currentHost);
      const r = await runTests({host: currentHost, app: ${JSON.stringify(dir)}, file: ${JSON.stringify(file)},
        url: 'https://fixture.test/', atStep: async (_, st) => {if (st.op === 'presence') inputs.push(st);}});
      assert.equal(r.passed, 9 + Number(scoped), JSON.stringify(r));
      assert.equal(r.failed, 5, JSON.stringify(r));
      assert.deepEqual(replies.slice(0, 9).map(r => [r.call, r.choice]), ${JSON.stringify(pairs)});
      assert.equal(inputs.length, 10 + Number(scoped), 'presence counts as input');
      const failure = name => r.results.find(t => t.name === name).failures.join(' ');
      for (const name of ['missing', 'consumed']) {
        assert.match(failure(name), /presence:.*no held device request/);
        assert.match(failure(name), /authenticate or unlocks a presence secret/);
      }
      assert.match(failure('ambiguous'), /2 holds match/);
      assert.match(failure('unsettled'), /presence: waiting for the prompt: requests still in flight/);
      assert.match(failure('host refusal'), /presence answer refused/);
      if (!scoped) assert.equal(r.results.at(-1).skipped, 'on ios, android');
      const invariant = await runTests({host: currentHost, app: ${JSON.stringify(dir)},
        file: ${JSON.stringify(properties)}, url: 'https://fixture.test/'});
      assert.equal(invariant.failed, ${pairs.length}, JSON.stringify(invariant));
      for (const result of invariant.results) {
        assert.equal(result.failures.length, 2, JSON.stringify(result));
        const failures = result.failures.join(' ');
        assert.ok(failures.includes('PROPERTY "presence keeps choice empty" is false on ' + currentHost), failures);
        assert.ok(failures.includes('PROPERTY "choice stays empty" is false on ' + currentHost), failures);
        const answer = result.name.split(' ')[1];
        assert.ok(failures.includes('at step 2: presence ' + answer), failures);
        assert.ok(failures.includes('before ' + currentHost + ': slot choice ""'), failures);
      }
    }
  `);
  try {
    const r = spawnSync(process.execPath, [script], { cwd: root, encoding: 'utf8', timeout: 60_000 });
    expect(r.status, r.stdout + r.stderr).toBe(0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);
