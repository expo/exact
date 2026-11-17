import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createApp, applyPattern } from '../game/new.mjs';
import { chromium } from './agent-launch.mjs';
import { draftFields, savedChanges } from './verify.mjs';

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
