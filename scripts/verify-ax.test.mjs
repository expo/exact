// @ref LLP 1166.002 D1 — verify's accessibility tree read over hand-written `tree --ax` replies (agent-ax.mjs `axFinish`
// computes their findings, as a real read's are): an unnamed field is a counted finding under its screen, app.json's
// `verify.accept` takes it, `--strict` fails on it, and an incomplete read keeps its findings and says it is incomplete.
// `bun test ./scripts/verify-ax.test.mjs`
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { axFinish } from './agent-ax.mjs';
import { axHere, axRead } from './verify-ax.mjs';
import { report } from './verify.mjs';

const plain = { nodes: [
  { id: 1, props: { testId: 'root' }, children: [2, 3, 4] },
  { id: 2, props: { testId: 'title' }, children: [] },
  { id: 3, props: { testId: 'save' }, children: [] },
  { id: 4, props: {}, children: [] }] };
const el = (i, id, role, name) => ({ i, parent: i ? 0 : null, id, via: 'self', role, name, states: {}, interactive: role !== 'main', native: { role } });
/** A Chrome read of `elements`, its findings computed as `tree --ax` computes them. */
const chrome = (elements, { complete = true, truncated, spanned } = {}) => axFinish({ epoch: 1, incarnation: 1, clock: 0, ...(spanned ? { spanned } : {}),
  ax: { source: 'chrome-cdp', platform: 'Chrome 1', order: 'tree', coverage: { roots: ['document'], complete, visited: elements.length },
    modal: { present: false }, elements, ...(truncated ? { truncated } : {}) } }, plain, null);
// A form whose title field has no name; its Save button and a checkbox with no testId are named.
const form = () => [el(0, 1, 'main', ''), el(1, 2, 'textbox', ''), el(2, 3, 'button', 'Save'), el(3, 4, 'checkbox', 'Done')];
const base = { host: 'web', built: null, tested: { passed: 1, failed: 0, results: [] }, checks: [], shots: [], ms: 1000 };
const UNNAMED = '  unnamed  title: a textbox with no name: a screen reader says only its role; give it aria-label="…", or aria-labelledby="<id>" of the text that labels it';

test('an unnamed field is a finding under its screen, counted as check\'s are, failing the run only under --strict', () => {
  const read = axRead(chrome(form()), 'first screen');
  assert.equal(read.source, 'chrome-cdp');
  assert.equal(read.incomplete, undefined);
  assert.deepEqual(read.findings.map((f) => [f.kind, f.testId, f.node, f.line]), [['unnamed', 'title', 'title', UNNAMED]]);
  assert.deepEqual(read.accepted, []);
  // The same field at the relaunch is said once; one only a test end shows is tagged with its screen. The report groups
  // them by kind as it groups check's, and every line is in its full listing.
  const end = axRead(chrome([...form(), el(4, 9, 'button', '')]), 'adds a note');
  const r = report({ ...base, ax: [read, end, axRead(chrome(form()), 'relaunch')], listed: '.exact/verify/web-findings.txt' });
  const head = 'tree --ax first screen, 1 test end, relaunch: 2 findings';
  assert.deepEqual(r.lines.slice(1, 5), [head, '  unnamed ×2', `    ${UNNAMED.trim()}`, '    also #9']);
  assert.deepEqual(r.full, [head, UNNAMED,
    '  ("adds a note" only) unnamed  #9: a button with no name: a screen reader says only its role; give it aria-label="…", or aria-labelledby="<id>" of the text that labels it']);
  assert.equal(r.lines.at(-1), 'verify: 2 problems (2 findings; the tests passed) in 1.0 s');
  assert.equal(r.exit, 0);
  const strict = report({ ...base, ax: [read], strict: true });
  assert.equal(strict.exit, 1);
  assert.equal(strict.lines.at(-1), 'verify: 1 problem (1 finding; the tests passed) in 1.0 s');
  // A clean read says so in one line, and passes --strict; every screen's tree was read whole (`--json`'s `axUnread`).
  const clean = report({ ...base, ax: [axRead(chrome(form().slice(0, 1)), 'first screen')], strict: true });
  assert.deepEqual([clean.lines[1], clean.exit, clean.lines.at(-1), clean.axUnread], ['tree --ax first screen: no findings', 0, 'verify: clean in 1.0 s', 0]);
});

test('app.json\'s verify.accept takes an ax finding by its kind and the testId of the view it joined', () => {
  const entry = { finding: 'unnamed', node: 'tit*', why: 'the reference has no label' };
  const read = axRead(chrome(form()), 'first screen', [entry, { finding: 'contrast', node: 'title', why: 'x' }]);
  assert.deepEqual(read.findings, []);
  assert.deepEqual(read.accepted.map((f) => [f.kind, f.accept, f.line]), [['unnamed', entry, '  unnamed  title: a textbox with no name; accepted: the reference has no label']]);
  // Another kind's entry, or a field with no testId, takes nothing.
  assert.equal(axRead(chrome(form()), 'first screen', [{ finding: 'exposed-hidden', node: 'title', why: 'x' }]).findings.length, 1);
  assert.equal(axRead(chrome([el(0, 1, 'main', ''), el(1, 4, 'textbox', '')]), 'first screen', [{ finding: 'unnamed', node: 'root', why: 'x' }]).findings.length, 1);
  const r = report({ ...base, ax: [read], accept: [entry, { finding: 'unnamed', node: 'gone', why: 'x' }], strict: true });
  assert.deepEqual(r.lines.slice(1), [
    'tree --ax first screen: no findings',
    'accepted (app.json verify.accept, not counted): 1 finding',
    '  unnamed  title: a textbox with no name; accepted: the reference has no label',
    '  accepts nothing: unnamed gone (no such finding in this run)',
    'open it: bun exact.mjs web (serves what verify checked)',
    'verify: clean in 1.0 s',
  ]);
  assert.equal(r.exit, 0);
});

test('an incomplete read keeps its findings and says so beside them; with none the run is not called clean', () => {
  const cut = axRead(chrome(form(), { complete: false, truncated: { elements: 40, fields: 0 } }), 'adds a note');
  assert.deepEqual([cut.incomplete, cut.findings.map((f) => f.testId)], ['the reply was cut short', ['title']]);
  const r = report({ ...base, ax: [axRead(chrome(form().slice(0, 1)), 'first screen'), cut] });
  assert.deepEqual(r.lines.slice(1, 3), ['tree --ax first screen, 1 test end: 1 finding; coverage incomplete on "adds a note" (the reply was cut short): more may be unread there', '  ("adds a note" only) ' + UNNAMED.trim()]);
  assert.deepEqual([r.lines.at(-1), r.axUnread], ['verify: 1 problem (1 finding; the tests passed), and the accessibility tree was read incompletely on "adds a note" (the reply was cut short) in 1.0 s', 1]);
  // Found nothing, read incompletely: the closing line says so, though nothing is counted and --strict passes.
  const none = axRead(chrome(form().slice(0, 1), { spanned: true }), 'relaunch'), unsure = report({ ...base, ax: [none], strict: true });
  assert.equal(unsure.lines[1], 'tree --ax relaunch: no findings; coverage incomplete on relaunch (the screen changed during the read): not a clean result there');
  assert.deepEqual([unsure.lines.at(-1), unsure.exit, unsure.problems, unsure.axUnread], ['verify: clean, but the accessibility tree was read incompletely on relaunch (the screen changed during the read) in 1.0 s', 0, 0, 1]);
  // What the reply says it left out: a frame on the web; on iOS the host's reason, as for the native tab bar.
  const leaving = (excluded, source = 'chrome-cdp') => { const r = chrome(form()); r.ax.source = source; r.ax.coverage = { roots: ['document'], complete: false, excluded }; return axRead(r, 'first screen'); };
  assert.equal(leaving([{ frame: 'https://x.test/', reason: 'frame-scoped AX tree; the carrier attaches one page target' }]).incomplete, 'a frame is not read');
  const tabs = leaving([{ root: 'tab-bar', reason: 'the native tab bar is excluded by the view walk; its accessibility projection cannot be verified' }], 'uikit');
  assert.deepEqual([tabs.incomplete, tabs.findings.length], ['the native tab bar is excluded by the view walk', 1]);
});

test('a read UIKit answered without its accessibility runtime is a note with the host\'s fix, and the run is not called clean', () => {
  const reason = "UIKit's accessibility runtime is not loaded (no assistive technology or automation is on), so it derives no labels, frames or field elements; on a simulator: xcrun simctl spawn <udid> defaults write com.apple.Accessibility ApplicationAccessibilityEnabled -bool true, then relaunch";
  const kit = (screen) => { const r = chrome(form()); r.ax.source = 'uikit'; r.ax.coverage = { roots: ['ExactView'], complete: false, excluded: [{ root: 'process', reason }] }; return axRead(r, screen); };
  assert.deepEqual(kit('first screen'), { screen: 'first screen', note: reason });
  const r = report({ ...base, host: 'ios', ax: ['first screen', 'adds a note', 'relaunch'].map(kit), strict: true });
  assert.deepEqual(r.lines.slice(1, 3), ['tree --ax: not read (not counted)', `  could not read first screen, "adds a note", relaunch: ${reason}`]);
  assert.deepEqual([r.exit, r.problems, r.axUnread, r.lines.at(-1)], [0, 0, 3, 'verify: clean, but the accessibility tree was not read on first screen, "adds a note", relaunch (UIKit\'s accessibility runtime is not loaded) in 1.0 s']);
  // Both at once: each said, with its screens.
  const both = report({ ...base, host: 'ios', ax: [kit('first screen'), axRead(chrome(form().slice(0, 1), { spanned: true }), 'relaunch')] });
  assert.deepEqual([both.lines.at(-1), both.axUnread], ['verify: clean, but the accessibility tree was not read on first screen (UIKit\'s accessibility runtime is not loaded) and was read incompletely on relaunch (the screen changed during the read) in 1.0 s', 2]);
  // A run with counted problems says it too: what it counted is not all there is.
  const failing = report({ ...base, host: 'ios', tested: { passed: 0, failed: 1, results: [{ name: 'adds', failures: ['line 2: no view "add"'] }] }, ax: [kit('first screen')] });
  assert.deepEqual([failing.lines.at(-1), failing.exit, failing.axUnread], ['verify: 1 problem (1 failed test), and the accessibility tree was not read on first screen (UIKit\'s accessibility runtime is not loaded) in 1.0 s', 1, 1]);
});

test('a host with no tree, or a read that fails, is a note and never a problem; Windows and Android are not read', async () => {
  const firefox = axRead({ ax: { unavailable: true, reason: 'the Accessibility domain is CDP\'s; the firefox carrier has none' } }, 'first screen');
  const r = report({ ...base, ax: [firefox, axRead(chrome(form().slice(0, 1)), 'relaunch')] });
  assert.deepEqual(r.lines.slice(1, 3), ['tree --ax relaunch: no findings', '  could not read first screen: unavailable: the Accessibility domain is CDP\'s; the firefox carrier has none']);
  assert.deepEqual([r.lines.at(-1), r.axUnread], ['verify: clean, but the accessibility tree was not read on first screen (unavailable: the Accessibility domain is CDP\'s) in 1.0 s', 1]);
  assert.deepEqual(report({ ...base, ax: [firefox] }).lines.slice(1, 3), ['tree --ax: not read (not counted)', '  could not read first screen: unavailable: the Accessibility domain is CDP\'s; the firefox carrier has none']);
  const failing = { host: 'ios', tree: async () => { throw new Error('tree --ax: the reply cannot be bounded to 262144 bytes\nstack'); } };
  assert.deepEqual(await axHere(failing, 'relaunch'), { screen: 'relaunch', note: 'tree --ax: the reply cannot be bounded to 262144 bytes' });
  for (const host of ['windows', 'android']) assert.equal(await axHere({ host, tree: () => assert.fail('read') }, 'first screen'), null);
  // There the report's own note says the tree is not read, the closing line is the plain one and `axUnread` is absent.
  const android = report({ ...base, host: 'android' });
  assert.ok(android.lines.includes("not on Android yet: check's findings (each screen is captured unjudged, light and dark: look at them), a read of the accessibility tree (tree --ax), the largest text size, the scrolling probe (iOS's), a test end's reload (the web's)"), android.lines.join('\n'));
  assert.deepEqual([android.lines.at(-1), android.axUnread], ['verify: clean in 1.0 s', undefined]);
  // A read asks for the whole screen, at tree --ax's largest limit.
  let asked;
  await axHere({ host: 'web', tree: async (target, options) => { asked = [target, options]; return chrome(form()); } }, 'first screen');
  assert.deepEqual(asked, [undefined, { ax: true, limit: 2000 }]);
});
