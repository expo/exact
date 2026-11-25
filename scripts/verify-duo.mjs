// `verify ios` on an iPhone Duo (LLP 1188 D4): the app's tests run once more on a Duo simulator, closed and upright, where the
// app is as wide as a phone's, and its first screen and each test's end are checked closed, open and half open, then closed
// with the keyboard up on the lowest field, where one rises.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { simulator, simulators, withSoftwareKeyboard } from '../host/apple/devices.mjs';
import { simulatorDisplay } from './agent-launch.mjs';
import { hasSimulatorHinge, hingeHelper } from './duo/hinge.mjs';

/** The poses, by name and hinge angle (LLP 1127 §1: the posture and the segments are the same at every half-open angle). */
const POSES = [['closed', 0], ['open', 180], ['half open', 130]];
const firstLine = (e) => e.message.split('\n')[0];

/** The Duo to read: `pick` (`--duo`, a udid or a name), else the one that is booted, else the first there is. Which of
 *  several is this run's to use is not its to guess. `{ dev }`, or `{ why }` there is none. */
export function chooseDuo(pick, all = simulators()) {
  const duos = all.filter(hasSimulatorHinge), booted = duos.filter((d) => d.state === 'Booted');
  const named = duos.filter((d) => d.udid === pick || d.name === pick);
  const several = (some, why) => ({ why: `${some.length} iPhone Duo simulators are ${why} (${some.map((d) => `${d.name} ${d.udid}`).join(', ')}): name one with --duo <udid>` });
  if (pick && named.length > 1) return several(named, `named ${pick}`);
  if (!pick && booted.length > 1) return several(booted, 'booted');
  const dev = pick ? named[0] : booted[0] ?? duos[0];
  return dev ? { dev } : { why: `no iPhone Duo simulator ${pick ?? 'here (with the iOS 27.1 runtime: xcrun simctl create "iPhone Duo" "iPhone Duo")'}` };
}

/** Folds to a pose, upright, and settles there; why the app did not take it, or null. */
async function unposed(s, degrees) {
  try {
    const r = await s.fold(degrees);
    // A test can end with the device turned
    if (degrees === 0 && (await s.op({ op: 'layout', simulatorRotation: true })).device > 1) await s.rotate('portrait');
    const settled = await s.clock('settle');
    return r.landError ?? (r.reason === 'hinge' ? 'the app did not take the pose' : settled?.settled === false ? `the app did not settle (${settled.reason ?? settled.diagnostic})` : null);
  } catch (e) { return firstLine(e); }
}

/** The lowest field presented whole and reachable, by its id: the one a keyboard covers first. */
async function lowestField(s) {
  const fields = ((await s.op({ op: 'check' })).items ?? []).filter((it) => it.control === 'field' && !it.disabled && !it.cover && !it.vis);
  return fields.sort((a, b) => b.y + b.h - (a.y + a.h))[0]?.id ?? null;
}

/** The screen `s` shows, read in each pose, then closed with the keyboard up: each read pushed on `runs` and what could not be on
 *  `notes`, as it happens. `check(pose)` is one check, as `verify`'s runs are. */
export async function readPoses(s, check, { runs, notes }) {
  const url = async () => (await s.state()).navigation?.url, at = await url();
  const here = async () => { const now = await url(); if (now !== at) throw new Error(`the app is at ${now}, not ${at}`); };
  const read = async (pose) => { try { await here(); runs.push(await check(pose)); } catch (e) { notes.push(`${pose}: not read: ${firstLine(e)}`); } };
  for (const [pose, degrees] of POSES) {
    const why = await unposed(s, degrees);
    if (!why) await read(pose); else notes.push(`${pose}: ${why}`);
    if (why && degrees === 0) return; // not upright, in no pose
  }
  const why = await unposed(s, 0);
  if (why) return notes.push(`the keyboard: ${why}`);
  await here();
  // A field the test left in focus has had its keyboard in the closed check
  const field = (await s.state()).keyboard?.visible ? null : await lowestField(s);
  if (field == null) return;
  await s.tap(field);
  await s.clock('settle');
  const node = (await s.tree()).nodes?.find((n) => n.id === field), name = node?.props?.testId ?? `field #${field}`;
  if (!(await s.state()).keyboard?.visible) notes.push(`the keyboard: none rose for ${name}`);
  else if (!node?.focused) notes.push(`the keyboard: ${name} did not keep the focus`);
  else await read('closed, keyboard up');
}

const remembers = (path) => (existsSync(path) ? readFileSync(path, 'utf8') : null);

/** The leg: `{ device, runs, failed, notes }`, with `error` where it ended early, or `{ why }` there is no Duo to read. `drive`
 *  opens the first screen, then runs the tests: it calls `atScreen(s, name, skip)` on each screen (`skip`: why it is not read, if
 *  it is not), and `atResult` with each test that failed only here, or null. */
export async function duoLeg({ pick, check, drive, remembered }) {
  let chosen;
  try { chosen = chooseDuo(pick); } catch (e) { return { why: firstLine(e) }; }
  if (!chosen.dev) return chosen;
  const { udid, name: device } = chosen.dev, out = { device, runs: [], failed: [], notes: [] }, said = process.env.EXACT_SIM;
  const attempt = async (what, fn) => { try { await fn(); } catch (e) { out.notes.push(`${what}: ${firstLine(e)}`); } };
  let used, dir, dev, keyboard, hinge;
  // Closed and upright, as each test starts
  const start = () => simulatorDisplay(udid, () => { hinge('orient', 'portrait'); hinge('fold', 0); });
  try {
    used = remembers(remembered);
    dir = mkdtempSync(resolve(tmpdir(), 'exact-duo-'));
    dev = simulator(udid);
    process.env.EXACT_SIM = udid;
    // Booted here, so its keyboard is the leg's to show, as a drive's that booted it is
    keyboard = await withSoftwareKeyboard(dev, async () => ({ close() {} }));
    hinge = hingeHelper(udid, dir);
    await start();
    await drive({
      atScreen: async (s, name, skip) => {
        const notes = skip ? [skip] : [], ended = (e) => { out.error ??= `at ${name}: ${firstLine(e)}`; };
        if (skip === undefined) {
          await readPoses(s, (pose) => check(s, name, pose), { runs: out.runs, notes }).catch((e) => notes.push(`not read: ${firstLine(e)}`));
          // An app that has gone is no note: it can have crashed as it folded
          await s.state().catch(ended);
        }
        out.notes.push(...notes.map((note) => `${name}: ${note}`));
        await start().catch(ended);
      },
      atResult: (failed) => { if (failed) out.failed.push(failed); },
    });
  } catch (e) { out.error ??= firstLine(e); }
  if (said === undefined) delete process.env.EXACT_SIM; else process.env.EXACT_SIM = said;
  // As they were: flat, not running, and the app's own choice of simulator (devices.mjs `chooseSimulator`), or what another
  // run has made it
  await attempt(`${device} was left folded`, () => hinge && simulatorDisplay(udid, () => hinge('fold', 180)));
  await attempt(`${device}'s keyboard setting was not put back`, () => keyboard?.close());
  await attempt(`${device} was left running`, () => { if (dev?.bootedHere) execFileSync('xcrun', ['simctl', 'shutdown', udid], { stdio: 'pipe' }); });
  await attempt('the app\'s simulator was not put back', () => {
    if (dev && remembers(remembered)?.trim() === udid) { if (used == null) rmSync(remembered); else writeFileSync(remembered, used); }
  });
  if (dir) rmSync(dir, { recursive: true, force: true });
  return out;
}
