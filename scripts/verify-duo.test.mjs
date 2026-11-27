// LLP 1188 D4: which Duo `verify ios` reads, how a screen is taken through its poses, and what the report says.
import { expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { matchSimulatorQuery } from '../host/apple/devices.mjs';
import { report } from './verify.mjs';
import { chooseDuo, duoLeg, isDuoCandidate, readPoses, rememberDuo, resolveDuoChoice } from './verify-duo.mjs';

const duo = (name, state = 'Shutdown', runtime = 'com.apple.CoreSimulator.SimRuntime.iOS-27-2-iPhone19-4') =>
  ({ udid: `U-${name}`, name, state, runtime, type: 'iPhone-Duo' });
const phone = { udid: 'U-pro', name: 'iPhone 18 Pro', state: 'Booted', runtime: 'com.apple.CoreSimulator.SimRuntime.iOS-27-2', type: 'iPhone-18-Pro' };

test('the Duo: explicit pick, remembered .exact/duo, sole booted, or sole available; never guesses among several', () => {
  // Sole booted beats shut-down siblings.
  expect(chooseDuo(undefined, [phone, duo('A'), duo('B', 'Booted')]).dev.name).toBe('B');
  // Sole shut-down Duo is chosen automatically.
  expect(chooseDuo(undefined, [phone, duo('A')]).dev.name).toBe('A');
  // Several shut-down Duos are never guessed: names both with runtime, UDID, --duo, EXACT_DUO, and --duo none.
  const multi = chooseDuo(undefined, [phone, duo('A'), duo('B')]);
  expect(multi.dev).toBeUndefined();
  expect(multi.why).toContain('2 iPhone Duo simulators are available');
  expect(multi.why).toContain('A (iOS 27.2, U-A)');
  expect(multi.why).toContain('B (iOS 27.2, U-B)');
  expect(multi.why).toContain('--duo <udid>');
  expect(multi.why).toContain('EXACT_DUO');
  expect(multi.why).toContain('--duo none');
  // Explicit pick (UDID or name) wins even over booted siblings.
  expect(chooseDuo('U-A', [duo('A'), duo('B', 'Booted'), duo('C', 'Booted')])).toMatchObject({ dev: { name: 'A' }, explicit: true });
  // Several booted Duos: refuse without app, disambiguate when exactly one has the app installed.
  expect(chooseDuo(undefined, [duo('B', 'Booted'), duo('C', 'Booted')]).why).toContain('--duo <udid>');
  expect(chooseDuo(undefined, [duo('B', 'Booted'), duo('C', 'Booted')], { app: 'dev.example.app', hasApp: (u) => u === 'U-C' })).toMatchObject({
    dev: { name: 'C' },
    reason: 'has dev.example.app installed',
  });
  expect(chooseDuo(undefined, [duo('B', 'Booted'), duo('C', 'Booted')], { app: 'dev.example.app', hasApp: () => true }).why).toContain('2 iPhone Duo simulators are booted');
  // No Duo, non-Duo named, and duplicate name.
  expect(chooseDuo(undefined, [phone]).why).toContain('no iPhone Duo simulator here');
  expect(chooseDuo('iPhone 18 Pro', [phone]).why).toBe('no iPhone Duo simulator iPhone 18 Pro');
  expect(chooseDuo('A', [duo('A'), { ...duo('A'), udid: 'U-2' }]).why).toContain('2 iPhone Duo simulators are named A');
  // Pre-27.1 Duo and malformed/missing runtimes are ignored; 27.1, device-suffixed, and future majors pass.
  for (const bad of [
    duo('Old26', 'Shutdown', 'com.apple.CoreSimulator.SimRuntime.iOS-26-5'),
    duo('Old270', 'Shutdown', 'com.apple.CoreSimulator.SimRuntime.iOS-27-0'),
    duo('Old2701', 'Shutdown', 'com.apple.CoreSimulator.SimRuntime.iOS-27-0-1'),
    duo('Malformed', 'Shutdown', 'com.apple.CoreSimulator.SimRuntime.iOS-27-1garbage'),
    { udid: 'U-NoRuntime', name: 'NoRuntime', state: 'Shutdown', type: 'iPhone-Duo' },
  ]) {
    expect(isDuoCandidate(bad)).toBe(false);
    expect(chooseDuo(undefined, [phone, bad]).why).toContain('no iPhone Duo simulator here');
  }
  for (const good of [
    duo('Min271', 'Shutdown', 'com.apple.CoreSimulator.SimRuntime.iOS-27-1'),
    duo('Suffixed', 'Shutdown', 'com.apple.CoreSimulator.SimRuntime.iOS-27-2-iPhone19-4'),
    duo('Future28', 'Shutdown', 'com.apple.CoreSimulator.SimRuntime.iOS-28-0'),
  ]) {
    expect(isDuoCandidate(good)).toBe(true);
    expect(chooseDuo(undefined, [phone, good]).dev.udid).toBe(good.udid);
  }
  // Exact UDID wins over a simulator whose name happens to equal that UDID (both in chooseDuo and matchSimulatorQuery).
  const decoyPhone = { ...phone, udid: 'U-Decoy', name: 'U-Target' };
  const decoyDuo = { ...duo('U-Target'), udid: 'U-Other' };
  const targetDuo = { ...duo('Real'), udid: 'U-Target' };
  expect(chooseDuo('U-Target', [decoyDuo, targetDuo]).dev.udid).toBe('U-Target');
  expect(matchSimulatorQuery([decoyPhone, targetDuo], 'U-Target').dev.udid).toBe('U-Target');
  expect(matchSimulatorQuery([decoyPhone, targetDuo], 'U-Target', { exactUdid: true }).dev.udid).toBe('U-Target');
  expect(() => matchSimulatorQuery([decoyPhone], 'U-Target', { exactUdid: true })).toThrow(/no iOS simulator with udid/);
  // Phone leg itself is a Duo: selects it among multiple shut-down Duos.
  expect(chooseDuo(undefined, [duo('A'), duo('B')], { phoneUdid: 'U-B' })).toMatchObject({ dev: { name: 'B' }, reason: 'same as phone leg' });
  // Per-app remembered .exact/duo beats foreign booted Duos; resolveDuoChoice saves ONLY when remember=true (flag, not env).
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-duo-remember-'));
  try {
    expect(resolveDuoChoice({ pick: 'U-A', dir, remember: false, all: [duo('A'), duo('B')] }).dev.udid).toBe('U-A');
    expect(existsSync(resolve(dir, '.exact', 'duo'))).toBe(false);
    expect(resolveDuoChoice({ pick: 'U-A', dir, remember: true, all: [duo('A'), duo('B')] }).dev.udid).toBe('U-A');
    expect(readFileSync(resolve(dir, '.exact', 'duo'), 'utf8')).toBe('U-A\n');
    expect(chooseDuo(undefined, [duo('A'), duo('B', 'Booted')], { dir })).toMatchObject({ dev: { name: 'A' }, reason: 'remembered' });
    const stale = chooseDuo(undefined, [duo('B')], { dir });
    expect(stale.dev.name).toBe('B');
    expect(stale.note).toContain('U-A');
    expect(stale.note).toContain('.exact/duo');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('duoLeg uses actual boot outcome for headless default, preserves .exact/simulator, and surfaces cleanup failures as notes', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-duo-lifecycle-'));
  const simChoice = resolve(dir, '.exact', 'simulator');
  mkdirSync(resolve(dir, '.exact'), { recursive: true });
  writeFileSync(simChoice, 'U-Duo\n');
  const prevShow = process.env.EXACT_SHOW_SIMULATOR;
  delete process.env.EXACT_SHOW_SIMULATOR;
  try {
    // Stale discovery snapshot says Booted, but actual boot discovers Shutdown and boots it here:
    // EXACT_SHOW_SIMULATOR must become '0' for drive(), shutdown runs once, keyboard restore failure becomes a note,
    // and another drive's `.exact/simulator` file pointing at U-Duo stays untouched.
    let seenShow = null, shutdowns = 0, bootExactUdid = false;
    const chosen = { dev: duo('Duo', 'Booted'), reason: 'test' };
    chosen.dev.udid = 'U-Duo';
    const res = await duoLeg({
      chosen,
      announce: false,
      boot: async (udid, opts) => {
        bootExactUdid = Boolean(opts.exactUdid);
        opts.onBooted?.();
        return { ...chosen.dev, state: 'Booted', bootedHere: true };
      },
      keyboardHelper: async () => ({
        async close() { throw new Error('defaults write failed'); },
        restoreSync() { return false; },
      }),
      hingeFactory: () => () => {},
      shutdown: () => { shutdowns++; },
      drive: async () => { seenShow = process.env.EXACT_SHOW_SIMULATOR; },
      check: async () => ({}),
    });
    expect(bootExactUdid).toBe(true);
    expect(seenShow).toBe('0');
    expect(shutdowns).toBe(1);
    expect(res.error).toBeUndefined();
    expect(res.notes).toContain("Duo's keyboard setting was not put back: defaults write failed");
    expect(readFileSync(simChoice, 'utf8')).toBe('U-Duo\n');
    expect(process.env.EXACT_SHOW_SIMULATOR).toBeUndefined();

    // Failed bootstatus after boot succeeded retains ownership and surfaces shutdown failure as a note.
    let failedShutdowns = 0;
    const bootFail = await duoLeg({
      chosen: { dev: { ...chosen.dev, state: 'Shutdown' } },
      announce: false,
      boot: async (_udid, opts) => {
        opts.onBooted?.();
        throw new Error('simctl bootstatus: failed');
      },
      shutdown: () => { failedShutdowns++; throw new Error('shutdown refused'); },
      drive: async () => {},
      check: async () => ({}),
    });
    expect(bootFail.error).toBe('simctl bootstatus: failed');
    expect(failedShutdowns).toBe(1);
    expect(bootFail.notes).toContain('Duo was left running: shutdown refused');
  } finally {
    if (prevShow === undefined) delete process.env.EXACT_SHOW_SIMULATOR; else process.env.EXACT_SHOW_SIMULATOR = prevShow;
    rmSync(dir, { recursive: true, force: true });
  }
});

/** A session whose folds, turns and taps are recorded; `takes` false for the angles the app does not take, `device` how the test
 *  left it turned. */
function session({ fields = [], takes = () => true, keyboard = true, url = () => '/', focused = (id) => id, device = 1, closedAtStart = false } = {}) {
  const did = [];
  let on = null;
  let angle = closedAtStart ? 0 : undefined, inner = closedAtStart ? false : undefined, ready = closedAtStart;
  return { did, fold: async (degrees) => { did.push(degrees); if (takes(degrees)) { angle = degrees; inner = degrees !== 0; ready = true; return {}; } return { settled: false, reason: 'hinge' }; }, clock: async () => ({}),
    rotate: async (to) => { did.push(to); device = 1; },
    tree: async () => ({ nodes: fields.map((f, id) => ({ id, type: 'TextInput', props: { testId: f.name }, focused: on != null && id === focused(on) })) }),
    op: async ({ op }) => (op === 'layout' ? { device, angle, inner, ready } : { items: fields.map((f, id) => ({ id, control: 'field', x: 0, w: 300, h: 34, ...f })) }),
    tap: async (id) => { did.push(`tap ${id}`); on = id; }, state: async () => ({ keyboard: { visible: on != null && keyboard }, navigation: { url: url(on != null) } }) };
}
const poses = async (s, check = async (pose) => pose) => { const runs = [], notes = []; const res = await readPoses(s, check, { runs, notes }); return { read: runs, notes, did: s.did, atClosed: res.atClosed }; };

test('a screen is read closed, open and half open, then closed with the keyboard up on its lowest field', async () => {
  const three = ['closed', 'open', 'half open'];
  // No eligible field in closed pose: skip folding back to 0° for keyboard pass.
  expect(await poses(session())).toEqual({ read: three, notes: [], did: [0, 180, 130], atClosed: false });
  // Already closed and upright at session start: skip the initial 0° fold too.
  expect(await poses(session({ closedAtStart: true }))).toEqual({ read: three, notes: [], did: [180, 130], atClosed: false });
  const fields = [{ name: 'title', y: 90 }, { name: 'last', y: 600 }, { name: 'clipped', y: 640, vis: { x: 0, y: 640, w: 300, h: 20 } },
    { name: 'done', y: 640, disabled: true }, { name: 'under', y: 650, cover: { kind: 'bar' } }];
  expect(await poses(session({ fields }))).toEqual({ read: [...three, 'closed, keyboard up'], notes: [], did: [0, 180, 130, 0, 'tap 1'], atClosed: true });
  expect(await poses(session({ fields, closedAtStart: true }))).toEqual({ read: [...three, 'closed, keyboard up'], notes: [], did: [180, 130, 0, 'tap 1'], atClosed: true });
  // A field that raises none, one that loses the focus, and one whose focus leaves the screen are notes
  expect((await poses(session({ fields, keyboard: false }))).notes).toEqual(['the keyboard: none rose for last']);
  expect((await poses(session({ fields, focused: () => 0 }))).notes).toEqual(['the keyboard: last did not keep the focus']);
  expect(await poses(session({ fields, url: (up) => (up ? '/search' : '/') }))).toEqual({ read: three, notes: ['closed, keyboard up: not read: the app is at /search, not /'], did: [0, 180, 130, 0, 'tap 1'], atClosed: true });
  // A test can end turned
  expect((await poses(session({ device: 4 }))).did).toEqual([0, 'portrait', 180, 130]);
  // What a check could not read is said, and the rest is read
  const partly = await poses(session(), async (pose) => { if (pose === 'open') throw new Error('gone\nmore'); return pose; });
  expect([partly.read, partly.notes]).toEqual([['closed', 'half open'], ['open: not read: gone']]);
  // A pose the app did not take, or a fold that failed, is said, and not read; where it did not close upright, no pose is
  expect(await poses(session({ takes: (d) => d !== 0 }))).toEqual({ read: [], notes: ['closed: the app did not take the pose'], did: [0], atClosed: false });
  expect((await poses(session({ takes: (d) => { if (d === 180) throw new Error('busy'); return true; } }))).notes).toEqual(['open: busy']);
  expect(await poses(session({ takes: (d) => d !== 130 }))).toEqual({ read: ['closed', 'open'], notes: ['half open: the app did not take the pose'], did: [0, 180, 130], atClosed: false });
  // A throwing closed-pose field probe still reads open/half-open and conservatively runs the final closed probe.
  let probeThrows = true;
  const flaky = session({ fields: [{ name: 'late', y: 400 }] });
  const origOp = flaky.op;
  flaky.op = async (req) => { if (req.op === 'check' && probeThrows) { probeThrows = false; throw new Error('transient'); } return origOp(req); };
  expect(await poses(flaky)).toEqual({ read: [...three, 'closed, keyboard up'], notes: [], did: [0, 180, 130, 0, 'tap 0'], atClosed: true });
  // If opening introduces a field on the same route, negative closed-pose field evidence is invalidated and keyboard pass runs.
  let openedOnce = false;
  const reveals = session();
  const origFold = reveals.fold, origRevealsOp = reveals.op;
  reveals.fold = async (deg) => { if (deg === 180) openedOnce = true; return origFold(deg); };
  reveals.op = async (req) => (req.op === 'layout' ? origRevealsOp(req) : { items: openedOnce ? [{ id: 0, control: 'field', x: 0, y: 400, w: 300, h: 34, name: 'revealed' }] : [] });
  reveals.tree = async () => ({ nodes: openedOnce ? [{ id: 0, type: 'TextInput', props: { testId: 'revealed' }, focused: true }] : [] });
  expect(await poses(reveals)).toEqual({ read: [...three, 'closed, keyboard up'], notes: [], did: [0, 180, 130, 0, 'tap 0'], atClosed: true });
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

test('verify.mjs imports resolveDuoChoice and readdirSync, and duoLeg honors boot-start abort ownership', async () => {
  const verifySrc = readFileSync(resolve(import.meta.dir, 'verify.mjs'), 'utf8');
  expect(verifySrc).toMatch(/import\s*\{[^}]*\breaddirSync\b[^}]*\}\s*from\s*'node:fs'/);
  expect(verifySrc).toMatch(/import\s*\{[^}]*\bresolveDuoChoice\b[^}]*\}\s*from\s*'\.\/verify-duo\.mjs'/);
  // A normal non-aborted boot failure clears bootAttempted so shutdown is not invoked.
  const shutdowns = [];
  const res = await duoLeg({
    chosen: { dev: { udid: 'DUO-FAIL', name: 'iPhone Duo', state: 'Shutdown' }, reason: null, explicit: false, note: null },
    announce: false,
    boot: async (_udid, opts) => {
      opts.onBootStart?.();
      opts.onBootCleared?.();
      throw new Error('simctl boot: invalid device');
    },
    shutdown: (udid) => { shutdowns.push(udid); },
    check: async () => ({}),
    drive: async () => {},
  });
  expect(res.error).toContain('invalid device');
  expect(shutdowns).toEqual([]);
});
