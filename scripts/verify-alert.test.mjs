import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createApp } from '../game/new.mjs';
import { chromium } from './agent-launch.mjs';
import * as verify from './verify.mjs';

test('alert ends use each host\'s presented state and keep one finding', async () => {
  for (const [host, state] of [
    ...['alert', 'sheet'].map(presentation => ['ios', { navigation: { popover: { kind: 'confirmation', presentation } } }]),
    ['macos', { dialog: { kind: 'alert', presentation: 'NSAlert', sheet: true } }],
    ['android', { surface: { kind: 'alert' } }],
    ['web', { navigation: { popover: { kind: 'confirmation' } } }],
  ]) {
    const calls = [];
    let reloaded = false;
    const s = { host, clock: async v => calls.push(v), state: async () => ({ ...state, slots: { count: reloaded ? 0 : 1 } }),
      tree: async () => ({ nodes: [] }), layout: async () => ({ viewport: { w: 390, h: 844 } }),
      prefer: async facts => { expect(facts).toEqual({}); return { media: { 'prefers-color-scheme': 'dark' } }; },
      screenshot: async path => calls.push(path), logs: async () => ({ lines: [] }),
      op: async () => { throw new Error('the page must not be checked'); } };
    const end = await verify.testEnd(host, '/shots', ['count'])(s, { name: 'asks first', index: 0, failures: [],
      ...(host === 'web' ? { restart: async () => { reloaded = true; } } : {}) });
    expect(end.runs).toHaveLength(1);
    expect(end.runs[0]).toMatchObject({ screen: 'asks first', size: [390, 844], scheme: 'dark', findings: [{ kind: 'alert' }] });
    expect(end.large).toBeUndefined();
    if (host === 'web') {
      expect(end.reloaded?.ran).toBe(true);
      expect(end.reloaded.finding.saved).toEqual([{ kind: 'state', name: 'count', was: 1, now: 0 }]);
    }
    expect(calls).toEqual(['settle', end.shot, ...(host === 'web' ? ['settle'] : [])]);
    expect(await verify.largestText(s, 'asks first')).toBeNull();
  }
  for (const state of [{}, { dialog: { kind: 'dialog', presentation: 'sheet' } },
    { navigation: { popover: { kind: 'sheet' } } },
    { surface: { kind: 'menu' } }]) expect(verify.systemAlert(state)).toBe(false);
});

test('a popover confirmation leaves the page judged', () => {
  expect(verify.systemAlert({ navigation: { popover: { kind: 'confirmation', presentation: 'popover' } } })).toBe(false);
});

test.skipIf(!!chromium().unavailable)('verify names an unanswered alert once and leaves its screen unjudged', () => {
  const root = resolve(import.meta.dir, '..'), parent = mkdtempSync(resolve(root, 'target/verify-alert-'));
  try {
    const dir = resolve(parent, 'profile');
    createApp(dir);
    writeFileSync(resolve(dir, 'app.ts'), "export const grants = '';\nexport const answer = () => { throw new Error('no sources'); };\n");
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state name = ""
  action edit(v: string)
    name = v
  action clear
    name = ""
  view
    column padding=16 gap=12
      text "Profile" role="heading" aria-level=1
      input value=name input=edit placeholder="Name" aria-label="Name" testId="name"
      text "Shown on your receipts." color="-exact-secondary-label"
      button command="show-modal" commandfor="confirm" testId="remove"
        text "Remove Profile"
      dialog id="confirm" role="alertdialog" aria-label="Remove Profile?"
        text "This can't be undone."
        button press=clear commandfor="confirm" command="close" testId="confirm-remove"
          text "Remove"
        button commandfor="confirm" command="close" testId="confirm-cancel"
          text "Cancel"
`);
    writeFileSync(resolve(dir, 'app.test.contract'), `test "the first screen"
  expect tree has "name"

test "remove asks first"
  tap "remove"
  clock settle
  expect tree has "confirm-remove"

test "a name is typed"
  type "name" "Ana"
  expect state name == "Ana"

test "an alert is answered"
  tap "remove"
  clock settle
  tap "confirm-cancel"
  clock settle
  expect tree has "name"
`);
    const run = spawnSync(process.execPath, [resolve(dir, 'exact.mjs'), 'verify', 'web', '--json'],
      { cwd: dir, encoding: 'utf8', timeout: 180_000, env: { ...process.env, EXACT_FEEDBACK: '0' } });
    expect(run.status, run.stdout + run.stderr).toBe(0);
    const result = JSON.parse(run.stdout);
    expect([result.tests.passed, result.tests.failed]).toEqual([4, 0]);
    const alert = result.checks.filter(c => c.screen === 'remove asks first');
    expect(alert).toHaveLength(1);
    expect(alert[0].lines).toEqual(['  alert    the test ends on a system alert: the screen under it is not checked; answer the alert before the test ends to check it']);
    expect(result.reloads.find(c => c.test === 'remove asks first')?.ran).toBe(true);
    expect(result.large.some(c => c.screen === 'remove asks first')).toBe(false);
    expect(result.large.flatMap(c => c.lines)).toEqual([]);
    expect(result.checks.filter(c => c.screen === 'an alert is answered')).toHaveLength(4);
    expect(result.large.some(c => c.screen === 'an alert is answered')).toBe(true);
    expect(result.problems).toBe(1);
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 240_000);
