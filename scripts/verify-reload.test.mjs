import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createApp, applyPattern } from '../game/new.mjs';
import { chromium } from './agent-launch.mjs';
import { draftFields, refusalsOf, report, savedChanges, selectVerifyFiles } from './verify.mjs';
import { linuxDevStaleChanges } from './agent-stdio.mjs';

test('an invalid field is a draft even when another record holds its value', () => {
  const field = (testId, value, accessibilityInvalid) => ({ type: 'TextInput', props: { testId, value, accessibilityInvalid } });
  const states = { before: { slots: { count: 2 }, resources: { items: [{ title: 'Milk' }], goal: 30 } },
    after: { slots: { count: 1 }, resources: { items: [{ title: 'Milk' }], goal: 30 } } };
  for (const invalid of ['true', 'grammar', 'spelling']) {
    const nodes = { before: [field('title', 'Milk', invalid), field('goal', '30', 'false')],
      after: [field('title', '', 'false'), field('goal', '25', 'false')] };
    expect([...draftFields(nodes.before, nodes.after, ['title', 'goal'], states)]).toEqual(['title']);
    expect(savedChanges({ persisted: ['count'], states, nodes, touched: ['title', 'goal'] })).toEqual([
      { kind: 'state', name: 'count', was: 2, now: 1 }, { kind: 'field', id: 'goal', was: '30', now: '25' },
    ]);
  }
});

test.skipIf(!!chromium().unavailable)('verify reload keeps a refused duplicate draft separate from the saved item', () => {
  const root = resolve(import.meta.dir, '..'), parent = mkdtempSync(resolve(root, 'target/verify-reload-'));
  try {
    const dir = resolve(parent, 'items');
    createApp(dir);
    applyPattern(dir, 'list-detail');
    writeFileSync(resolve(dir, 'app.test.contract'), `test "a duplicate title is refused"
  clock data
  tap "empty-add"
  clock settle
  type "add-title" "Milk"
  tap "add-save"
  clock data
  clock settle
  expect tree has "item-1"
  tap "add-item"
  clock settle
  type "add-title" "Milk"
  tap "add-save"
  clock data
  clock settle
  expect text "add-error" == "There’s already an item called Milk. Choose another title."
  expect state lib.length == 1
`);
    const run = spawnSync(process.execPath, [resolve(dir, 'exact.mjs'), 'verify', 'web', '--json'],
      { cwd: dir, encoding: 'utf8', timeout: 180_000, env: { ...process.env, EXACT_FEEDBACK: '0' } });
    expect(run.status, run.stdout + run.stderr).toBe(0);
    const result = JSON.parse(run.stdout);
    expect([result.tests.passed, result.tests.failed]).toEqual([1, 0]);
    expect(result.reloads.map(({ ran, finding }) => ({ ran, finding }))).toEqual([{ ran: true, finding: null }]);
    expect(result.relaunch.problems).toEqual([]);
    expect(result.relaunch.shows).toContain('Milk');
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 240_000);

test('refusalsOf ignores app-authored console logs without exact: while keeping exact: console refusals (LLP 1156.008.000 D8.1)', () => {
  expect(refusalsOf({
    lines: ['t=12 saveFile: refused: missing'],
    host: [
      'console.log: API refused booking',
      'console.warn: payment refusal from server',
      'console.error: request refused by backend',
      'console.error: exact: showModal confirm refused: no dialog or popover has that id',
      'app: exact: media refused: denied',
    ],
  })).toEqual([
    'saveFile: refused: missing',
    'showModal confirm refused: no dialog or popover has that id',
    'media refused: denied',
  ]);
});

test('selectVerifyFiles and partial report mark filtered verify runs distinctly (LLP 1156.008.000 D8.2)', () => {
  const dir = mkdtempSync(resolve(import.meta.dir, '../target/verify-partial-'));
  try {
    writeFileSync(resolve(dir, 'app.test.contract'), 'test "a"\n  expect tree has "x"\n');
    writeFileSync(resolve(dir, 'billing.test.contract'), 'test "b"\n  expect tree has "x"\n');
    const picked = selectVerifyFiles(dir, ['billing.test.contract']);
    expect(picked.files).toEqual([resolve(dir, 'billing.test.contract')]);
    expect(picked.partial).toEqual({ selected: 1, total: 2 });
    const summary = report({ host: 'web', tested: { passed: 1, failed: 0, results: [] }, checks: [], shots: [], ms: 1200, partial: picked.partial });
    expect(summary.lines.at(-1)).toBe('verify (partial: 1/2 test files): clean in 1.2 s');
  } finally { rmSync(dir, { recursive: true, force: true }); }
  const testsOnly = mkdtempSync(resolve(import.meta.dir, '../target/verify-tests-dir-'));
  try {
    mkdirSync(resolve(testsOnly, 'tests'));
    mkdirSync(resolve(testsOnly, 'other'));
    writeFileSync(resolve(testsOnly, 'tests/greeting.test.contract'), 'test "a"\n  expect tree has "x"\n');
    writeFileSync(resolve(testsOnly, 'other/extra.test.contract'), 'test "b"\n  expect tree has "x"\n');
    expect(selectVerifyFiles(testsOnly, [])).toEqual({
      files: [resolve(testsOnly, 'tests/greeting.test.contract')],
      partial: null,
    });
    expect(selectVerifyFiles(testsOnly, ['tests/greeting.test.contract']).partial).toEqual({ selected: 1, total: 1 });
    expect(selectVerifyFiles(testsOnly, ['other/extra.test.contract']).partial).toEqual({ selected: 1, total: 2 });
  } finally { rmSync(testsOnly, { recursive: true, force: true }); }
});

test('linuxDevStaleChanges exempts Contract and TypeScript files only when --url is set (LLP 1156.008.000 D9.4)', () => {
  const inputs = ['app.contract', 'app.ts', 'helper.tsx', 'host/painted/src/lib.rs'];
  expect(linuxDevStaleChanges(inputs, true)).toEqual(['host/painted/src/lib.rs']);
  expect(linuxDevStaleChanges(inputs, false)).toEqual(inputs);
});
