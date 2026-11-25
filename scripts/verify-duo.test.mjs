// LLP 1188 D4: which Duo `verify ios` reads, how a screen is taken through its poses, and what the report says.
import { expect, test } from 'bun:test';
import { report } from './verify.mjs';
import { chooseDuo, readPoses } from './verify-duo.mjs';

const duo = (name, state = 'Shutdown') => ({ udid: `U-${name}`, name, state, type: 'iPhone-Duo' });
const phone = { udid: 'U-pro', name: 'iPhone 18 Pro', state: 'Booted', type: 'iPhone-18-Pro' };

test('the Duo: the one named, else the one booted, else the first; never a guess among several', () => {
  expect(chooseDuo(undefined, [phone, duo('A'), duo('B', 'Booted')]).dev.name).toBe('B');
  expect(chooseDuo(undefined, [phone, duo('A'), duo('B')]).dev.name).toBe('A');
  expect(chooseDuo('U-A', [duo('A'), duo('B', 'Booted'), duo('C', 'Booted')]).dev.name).toBe('A');
  expect(chooseDuo(undefined, [duo('B', 'Booted'), duo('C', 'Booted')]).why).toContain('--duo <udid>');
  expect(chooseDuo(undefined, [phone]).why).toContain('no iPhone Duo simulator here');
  expect(chooseDuo('iPhone 18 Pro', [phone]).why).toBe('no iPhone Duo simulator iPhone 18 Pro');
  expect(chooseDuo('A', [duo('A'), { ...duo('A'), udid: 'U-2' }]).why).toContain('2 iPhone Duo simulators are named A');
});

/** A session whose folds, turns and taps are recorded; `takes` false for the angles the app does not take, `device` how the test
 *  left it turned. */
function session({ fields = [], takes = () => true, keyboard = true, url = () => '/', focused = (id) => id, device = 1 } = {}) {
  const did = [];
  let on = null;
  return { did, fold: async (degrees) => { did.push(degrees); return takes(degrees) ? {} : { settled: false, reason: 'hinge' }; }, clock: async () => ({}),
    rotate: async (to) => { did.push(to); device = 1; },
    tree: async () => ({ nodes: fields.map((f, id) => ({ id, type: 'TextInput', props: { testId: f.name }, focused: on != null && id === focused(on) })) }),
    op: async ({ op }) => (op === 'layout' ? { device } : { items: fields.map((f, id) => ({ id, control: 'field', x: 0, w: 300, h: 34, ...f })) }),
    tap: async (id) => { did.push(`tap ${id}`); on = id; }, state: async () => ({ keyboard: { visible: on != null && keyboard }, navigation: { url: url(on != null) } }) };
}
const poses = async (s, check = async (pose) => pose) => { const runs = [], notes = []; await readPoses(s, check, { runs, notes }); return { read: runs, notes, did: s.did }; };

test('a screen is read closed, open and half open, then closed with the keyboard up on its lowest field', async () => {
  const three = ['closed', 'open', 'half open'];
  expect(await poses(session())).toEqual({ read: three, notes: [], did: [0, 180, 130, 0] });
  const fields = [{ name: 'title', y: 90 }, { name: 'last', y: 600 }, { name: 'clipped', y: 640, vis: { x: 0, y: 640, w: 300, h: 20 } },
    { name: 'done', y: 640, disabled: true }, { name: 'under', y: 650, cover: { kind: 'bar' } }];
  expect(await poses(session({ fields }))).toEqual({ read: [...three, 'closed, keyboard up'], notes: [], did: [0, 180, 130, 0, 'tap 1'] });
  // A field that raises none, one that loses the focus, and one whose focus leaves the screen are notes
  expect((await poses(session({ fields, keyboard: false }))).notes).toEqual(['the keyboard: none rose for last']);
  expect((await poses(session({ fields, focused: () => 0 }))).notes).toEqual(['the keyboard: last did not keep the focus']);
  expect(await poses(session({ fields, url: (up) => (up ? '/search' : '/') }))).toEqual({ read: three, notes: ['closed, keyboard up: not read: the app is at /search, not /'], did: [0, 180, 130, 0, 'tap 1'] });
  // A test can end turned
  expect((await poses(session({ device: 4 }))).did).toEqual([0, 'portrait', 180, 130, 0]);
  // What a check could not read is said, and the rest is read
  const partly = await poses(session(), async (pose) => { if (pose === 'open') throw new Error('gone\nmore'); return pose; });
  expect([partly.read, partly.notes]).toEqual([['closed', 'half open'], ['open: not read: gone']]);
  // A pose the app did not take, or a fold that failed, is said, and not read; where it did not close upright, no pose is
  expect(await poses(session({ takes: (d) => d !== 0 }))).toEqual({ read: [], notes: ['closed: the app did not take the pose'], did: [0] });
  expect((await poses(session({ takes: (d) => { if (d === 180) throw new Error('busy'); return true; } }))).notes).toEqual(['open: busy']);
  expect(await poses(session({ takes: (d) => d !== 130 }))).toEqual({ read: ['closed', 'open'], notes: ['half open: the app did not take the pose'], did: [0, 180, 130, 0] });
});

test('the report: what the other simulator did not say, by its pose; a test that fails here fails the run; why none was read counts for nothing', () => {
  const finding = (kind, node, line, id = 3) => ({ kind, id, node: `${node}#${id}`, line: `  ${kind.padEnd(8)} ${node}#${id} ${line}` });
  const run = (pose, findings) => ({ screen: 'first screen', pose, size: [466, 678], scheme: 'light', findings, lines: findings.map((f) => f.line), accepted: [], hidden: 0 });
  const small = finding('target', 'View', '[10,20 30×30]: 30×30 pt, under the 44×44 minimum');
  const phone = { size: [402, 874], scheme: 'light', asked: 'light', findings: [small], lines: [small.line], hidden: 0 };
  const alert = { screen: 'x', size: [402, 874], scheme: 'light', asked: 'light', findings: [{ kind: 'alert', line: '  alert    up' }], lines: ['  alert    up'], hidden: 0 };
  const on = (duo) => report({ host: 'ios', built: null, tested: { none: true }, checks: [phone, alert], duo, shots: [], ms: 1, cwd: '/' });
  const r = on({ device: 'Duo', failed: [{ name: 'adds', file: 'app.test.contract', failures: ['adds: line 3: no'] }], notes: ['first screen: half open: the app did not take the pose (hinge)'],
    runs: [run('closed', [finding('target', 'View', '[12,24 30×30]: 30×30 pt, under the 44×44 minimum', 8)]), run('open', [finding('edge', 'add', '[796,619 55×34]: 18 pt of it lie outside')]),
      run('half open', [finding('edge', 'add', '[311,628 55×34]: 10 pt of it lie outside')])] });
  expect(r.lines.filter((l) => /Duo|edge|note|adds/.test(l))).toEqual(['iPhone Duo (Duo), closed, open, half open, 1 screen: 1 more finding',
    '  (first screen, open, half open) edge     add#3 [796,619 55×34]: 18 pt of it lie outside', '  note     first screen: half open: the app did not take the pose (hinge)', '  FAIL on an iPhone Duo "adds" (app.test.contract): line 3: no']);
  expect([r.problems, r.exit]).toEqual([4, 1]);
  expect(on({ device: 'Duo', runs: [], failed: [], notes: [], error: 'gone' }).lines).toContain('  unread   the drive of Duo ended: gone');
  const unread = on({ why: 'no iPhone Duo simulator here' });
  expect([unread.lines.includes('iPhone Duo: not read (no iPhone Duo simulator here)'), unread.problems, unread.exit]).toEqual([true, 2, 0]);
});
