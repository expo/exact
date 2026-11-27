// Authored tests answering held device and package tickets (`tap "@target" choice`, `type "@target" "text"`).
import { beforeAll, expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));

beforeAll(() => {
  const built = spawnSync('cargo', ['build', '-q', '-p', 'contract', '--bin', 'contract'], { cwd: root, encoding: 'utf8' });
  if (built.status !== 0) throw new Error(`building the Contract compiler failed:\n${built.stderr}`);
}, 600_000);

test('authored ticket steps route tap/type to held tickets, check always properties, and guide file pickers', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const dir = mkdtempSync(join(root, 'target/ticket-step-test-'));
  const file = join(dir, 'app.test.contract'), props = join(dir, 'props.test.contract'), script = join(dir, 'check.mjs');
  writeFileSync(join(dir, 'app.contract'), 'component App\n  state last = ""\n  view\n    text "Test"\n');
  writeFileSync(file, `test "choice and typed tickets"
  tap "open-browser"
  tap "@browser" dismissed
  expect state last == "dismissed"
  tap "open-date"
  type "@date" "2026-10-10"
  expect state last == "2026-10-10"
test "picker typed guard"
  tap "open-pick"
  type "@pick" "file.txt"
test "open-file typed guard"
  tap "open-file"
  type "@open-file" "file.txt"
test "node-id picker typed guard"
  tap "open-upload"
  type "@upload" "file.txt"
test "missing hold"
  tap "@browser" dismissed
test "unsettled hold"
  tap "hang"
  tap "@browser" dismissed
`);
  writeFileSync(props, `always "last stays empty"
  expect state last == ""
test "choice property"
  tap "open-browser"
  tap "@browser" dismissed
test "typed property"
  tap "open-date"
  type "@date" "2026-10-10"
`);
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    let sessionInput, inputs = [];
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({ open: async () => {
      let queued = null, pending = [], last = '', ticket = 10;
      const s = {
        host: 'web', now: 0,
        state: async () => ({ pending, slots: { last }, mutationRequests: 0 }),
        op: async (req) => {
          if (req.op === 'state') return s.state();
          if (req.op === 'tap' && req.ticket != null) {
            assert.equal(pending.length, 1);
            assert.equal(req.ticket, pending[0].ticket);
            pending = [];
            last = req.choice;
            return { ticket: req.ticket, answered: req.choice, delivery: 'substituted' };
          }
          if (req.op === 'type' && req.ticket != null) {
            assert.equal(pending.length, 1);
            assert.equal(req.ticket, pending[0].ticket);
            pending = [];
            last = req.text;
            return { ticket: req.ticket, answered: req.text, delivery: 'substituted' };
          }
          return { clock: 0, lines: [] };
        },
        clock: async (spec) => {
          assert.equal(spec, 'data');
          if (queued === 'hang') return { settled: false, diagnostic: 'still pending' };
          if (queued === 'open-browser') pending = [{ ticket: ++ticket, name: 'openBrowser', device: { capability: 'browser', args: {} } }];
          else if (queued === 'open-date') pending = [{ ticket: ++ticket, name: 'date', device: { capability: 'date', args: {} } }];
          else if (queued === 'open-pick') pending = [{ ticket: ++ticket, name: 'pick', device: { capability: 'pick', args: {} } }];
          else if (queued === 'open-file') pending = [{ ticket: ++ticket, name: 'open-file', device: { capability: 'open-file', args: {} } }];
          else if (queued === 'open-upload') pending = [{ ticket: ++ticket, name: 'upload', device: { capability: 'pick', args: {} } }];
          queued = null;
          return { settled: true };
        },
        landed: (r) => ({ ...r, delivery: 'substituted' }),
        declaration: async () => null,
        logs: async () => ({ lines: [] }),
        close: async () => {},
      };
      Object.assign(s, sessionInput(s, { carrier: {}, host: 'web', timing: 'agent' }));
      const origTap = s.tap;
      s.tap = async (target, opts) => {
        if (String(target).startsWith('@')) return origTap(target, opts);
        queued = target;
        return { delivery: 'substituted' };
      };
      return s;
    } }));
    const { runTests } = await import(${JSON.stringify(fileURLToPath(new URL('./agent-test.mjs', import.meta.url)))});
    ({ sessionInput } = await import(${JSON.stringify(fileURLToPath(new URL('./agent-input.mjs', import.meta.url)))}));
    const r = await runTests({ host: 'web', app: ${JSON.stringify(dir)}, file: ${JSON.stringify(file)}, url: 'https://fixture.test/',
      atStep: async (_, st) => { if (st.op === 'ticket') inputs.push(st); } });
    assert.equal(r.passed, 1, JSON.stringify(r));
    assert.equal(r.failed, 5, JSON.stringify(r));
    assert.equal(inputs.length, 2, 'successful ticket steps count as inputs');
    const failOf = (name) => r.results.find((t) => t.name === name).failures.join(' ');
    assert.match(failOf('picker typed guard'), /file pickers use.*pick "pick"/);
    assert.match(failOf('open-file typed guard'), /file pickers use.*pick "open-file"/);
    assert.match(failOf('node-id picker typed guard'), /file pickers use.*pick "upload"/);
    assert.match(failOf('missing hold'), /@browser:.*no held device request/);
    assert.match(failOf('unsettled hold'), /@browser: waiting for held ticket: still pending/);

    const inv = await runTests({ host: 'web', app: ${JSON.stringify(dir)}, file: ${JSON.stringify(props)}, url: 'https://fixture.test/' });
    assert.equal(inv.failed, 2, JSON.stringify(inv));
    assert.ok(inv.results[0].failures[0].includes('at step 2: tap "@browser" dismissed'), inv.results[0].failures[0]);
    assert.ok(inv.results[1].failures[0].includes('at step 2: type "@date" "2026-10-10"'), inv.results[1].failures[0]);
  `);
  try {
    const r = spawnSync(process.execPath, [script], { cwd: root, encoding: 'utf8', timeout: 60_000 });
    expect(r.status, r.stdout + r.stderr).toBe(0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('an authored test answers held @exact/in-app-browser openBrowser calls from Contract and app.ts on web', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const dir = mkdtempSync(join(root, 'target/in-app-browser-ticket-'));
  try {
    mkdirSync(join(dir, 'node_modules/@exact'), { recursive: true });
    symlinkSync(resolve(root, 'packages/in-app-browser'), join(dir, 'node_modules/@exact/in-app-browser'), 'dir');
    writeFileSync(join(dir, 'package.json'), JSON.stringify({ name: 'browser-ticket-fixture', type: 'module' }));
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Browser Ticket', app: { id: 'dev.exact.browserticket', name: 'Browser Ticket' }, host: { web: {} } }));
    writeFileSync(join(dir, 'app.ts'), `import type { Answer } from './app.contract.d.ts';
import { openBrowser } from '@exact/in-app-browser';
export const grants = '';
export const answer: Answer = (async (source: string) => {
  if (source === 'launchCheckout') return await openBrowser({ url: 'https://example.com/ts-checkout' });
  throw { kind: 'UnknownSource', message: source };
}) as unknown as Answer;
`);
    writeFileSync(join(dir, 'app.contract'), `use openBrowser, BrowserResult from "@exact/in-app-browser"

component App
  mutation direct as shape BrowserResult
  mutation viaTs as shape BrowserResult
  action goDirect
    send direct = openBrowser(url="https://example.com/checkout")
  action goTs
    send viaTs = launchCheckout()
  view
    column
      button press=goDirect testId="direct"
        text "Direct"
      button press=goTs testId="ts"
        text "Via TS"
      text (match direct { case some(r) => r.action, case none => "none" }) testId="direct-result"
      text (match viaTs { case some(r) => r.action, case none => "none" }) testId="ts-result"
`);
    writeFileSync(join(dir, 'app.test.contract'), `test "direct openBrowser answers dismissed, opened, and cancel"
  tap "direct"
  tap "@browser" dismissed
  clock data
  expect text "direct-result" == "dismissed"
  tap "direct"
  tap "@browser" opened
  clock data
  expect text "direct-result" == "opened"
  tap "direct"
  tap "@browser" cancel
  clock data
  expect text "direct-result" == "dismissed"

test "data-module app.ts openBrowser answers dismissed"
  tap "ts"
  tap "@browser" dismissed
  clock data
  expect text "ts-result" == "dismissed"
`);
    const built = spawnSync(process.execPath, [resolve(root, 'host/web/build.mjs')], {
      cwd: root,
      encoding: 'utf8',
      timeout: 180_000,
      env: { ...process.env, EXACT_APP_DIR: dir },
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const ran = spawnSync(process.execPath, [resolve(root, 'scripts/agent.mjs'), 'web', '--test', join(dir, 'app.test.contract')], {
      cwd: root,
      encoding: 'utf8',
      timeout: 180_000,
      env: { ...process.env, EXACT_APP_DIR: dir },
    });
    expect(ran.status, ran.stdout + ran.stderr).toBe(0);
    expect(ran.stdout).toContain('2 passed, 0 failed');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 240_000);
