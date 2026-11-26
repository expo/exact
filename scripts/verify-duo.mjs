// `verify ios` on an iPhone Duo (LLP 1188 D4): the app's tests run once more on a Duo simulator, closed and upright, where the
// app is as wide as a phone's, and its first screen and each test's end are checked closed, open and half open, then closed
// with the keyboard up on the lowest field, where one rises.
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { simulator, simulators, withSoftwareKeyboard } from '../host/apple/devices.mjs';
import { simulatorDisplay } from './agent-launch.mjs';
import { whenEnded } from './app-server.mjs';
import { hasSimulatorHinge, hingeHelper } from './duo/hinge.mjs';
import { FIRST } from './verify-findings.mjs';

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

/** Whether `rot` (`layout simulatorRotation`) already confirms the Duo is closed, upright, and settled. */
export const alreadyClosedUpright = (rot) =>
  Number.isFinite(rot?.angle) && Math.abs(rot.angle) < 1 && rot.inner === false && rot.ready === true && (rot.device ?? 1) <= 1;

/** Folds to a pose, upright, and settles there; why the app did not take it, or null. */
async function unposed(s, degrees) {
  try {
    let r = {};
    if (degrees === 0) {
      const before = await s.op({ op: 'layout', simulatorRotation: true });
      if (!alreadyClosedUpright(before)) {
        r = await s.fold(0);
        // A test can end with the device turned
        if ((await s.op({ op: 'layout', simulatorRotation: true })).device > 1) await s.rotate('portrait');
      }
    } else {
      r = await s.fold(degrees);
    }
    const settled = await s.clock('settle');
    return r.landError ?? (r.reason === 'hinge' ? 'the app did not take the pose' : settled?.settled === false ? `the app did not settle (${settled.reason ?? settled.diagnostic})` : null);
  } catch (e) { return firstLine(e); }
}

/** The lowest field presented whole and reachable, by its id: the one a keyboard covers first. */
async function lowestField(s) {
  const fields = ((await s.op({ op: 'check' })).items ?? []).filter((it) => it.control === 'field' && !it.disabled && !it.cover && !it.vis);
  return fields.sort((a, b) => b.y + b.h - (a.y + a.h))[0]?.id ?? null;
}

/** The screen `s` shows, read in each pose, then closed with the keyboard up when the closed screen holds an eligible
 *  field: each read pushed on `runs` and what could not be on `notes`, as it happens. `check(pose)` is one check, as
 *  `verify`'s runs are. Returns `{ atClosed }` indicating whether the device already rests closed and upright. */
export async function readPoses(s, check, { runs, notes }) {
  const url = async () => (await s.state()).navigation?.url, at = await url();
  const here = async () => { const now = await url(); if (now !== at) throw new Error(`the app is at ${now}, not ${at}`); };
  const read = async (pose) => { try { await here(); runs.push(await check(pose)); } catch (e) { notes.push(`${pose}: not read: ${firstLine(e)}`); } };
  let needKeyboard = null, closedSlots = null;
  for (const [pose, degrees] of POSES) {
    const why = await unposed(s, degrees);
    if (!why) {
      await read(pose);
      // While already closed, determine whether any eligible field needs the keyboard pass, avoiding an extra fold back.
      if (degrees === 0) {
        try {
          const st = await s.state();
          closedSlots = JSON.stringify(st.slots ?? null);
          needKeyboard = st.keyboard?.visible ? false : (await lowestField(s)) != null;
        } catch { needKeyboard = null; }
      } else if (needKeyboard === false) {
        // Invalidate negative field evidence if folding altered screen state or exposed a field.
        try {
          const st = await s.state();
          if (JSON.stringify(st.slots ?? null) !== closedSlots || (await lowestField(s)) != null) needKeyboard = null;
        } catch { needKeyboard = null; }
      }
    } else notes.push(`${pose}: ${why}`);
    if (why && degrees === 0) return { atClosed: false }; // not upright, in no pose
  }
  if (needKeyboard === false) return { atClosed: false };
  const why = await unposed(s, 0);
  if (why) { notes.push(`the keyboard: ${why}`); return { atClosed: false }; }
  await here();
  // A field the test left in focus has had its keyboard in the closed check
  const field = (await s.state()).keyboard?.visible ? null : await lowestField(s);
  if (field == null) return { atClosed: true };
  await s.tap(field);
  await s.clock('settle');
  const node = (await s.tree()).nodes?.find((n) => n.id === field), name = node?.props?.testId ?? `field #${field}`;
  if (!(await s.state()).keyboard?.visible) notes.push(`the keyboard: none rose for ${name}`);
  else if (!node?.focused) notes.push(`the keyboard: ${name} did not keep the focus`);
  else await read('closed, keyboard up');
  return { atClosed: true };
}

const remembers = (path) => (existsSync(path) ? readFileSync(path, 'utf8') : null);

/** The leg: `{ device, runs, failed, notes }`, with `error` where it ended early, or `{ why }` there is no Duo to read. `drive`
 *  opens the first screen, then runs the tests: it calls `atScreen(s, name, skip)` on each screen (`skip`: why it is not read, if
 *  it is not), and `atResult` with each test that failed only here, or null. */
export async function duoLeg({ pick, check, drive, remembered }) {
  let chosen;
  try { chosen = chooseDuo(pick); } catch (e) { return { why: firstLine(e) }; }
  if (!chosen.dev) return chosen;
  const { udid, name: device } = chosen.dev, out = { device, runs: [], failed: [], notes: [] }, said = process.env.EXACT_SIM, prevNoRemember = process.env.EXACT_SIM_NO_REMEMBER;
  const attempt = async (what, fn) => { try { await fn(); } catch (e) { out.notes.push(`${what}: ${firstLine(e)}`); } };
  let used, dir, dev, keyboard, hinge;
  let unfolded = false, restoredKeyboard = false, shutDown = false, restoredChoice = false, removedDir = false;
  // Closed and upright, as each test starts
  const start = () => simulatorDisplay(udid, () => { hinge('orient', 'portrait'); hinge('fold', 0); });
  // Best-effort synchronous cleanup if the process receives SIGINT/SIGTERM before async teardown finishes.
  const forgetEnd = whenEnded(() => {
    try { if (!unfolded && hinge) { unfolded = true; hinge('fold', 180); } } catch {}
    // Restore the software-keyboard preference while the simulator is still booted.
    try { if (!restoredKeyboard && keyboard?.restoreSync) { restoredKeyboard = true; keyboard.restoreSync(); } } catch {}
    try { if (!shutDown && dev?.bootedHere) { shutDown = true; execFileSync('xcrun', ['simctl', 'shutdown', udid], { stdio: 'pipe' }); } } catch {}
    try { if (!restoredChoice && dev && remembers(remembered)?.trim() === udid) { restoredChoice = true; if (used == null) rmSync(remembered); else writeFileSync(remembered, used); } } catch {}
    try { if (!removedDir && dir) { removedDir = true; rmSync(dir, { recursive: true, force: true }); } } catch {}
  });
  try {
    try {
      used = remembers(remembered);
      dir = mkdtempSync(resolve(tmpdir(), 'exact-duo-'));
      dev = simulator(udid, { rememberChoice: false });
      process.env.EXACT_SIM = udid;
      process.env.EXACT_SIM_NO_REMEMBER = '1';
      // Booted here, so its keyboard is the leg's to show, as a drive's that booted it is
      keyboard = await withSoftwareKeyboard(dev, async () => ({ close() {} }));
      hinge = hingeHelper(udid, dir);
      await start();
      await drive({
        atScreen: async (s, name, skip) => {
          const notes = skip ? [skip] : [], ended = (e) => { out.error ??= `at ${name}: ${firstLine(e)}`; };
          let atClosed = false;
          if (skip === undefined) {
            const posed = await readPoses(s, (pose) => check(s, name, pose), { runs: out.runs, notes }).catch((e) => { notes.push(`not read: ${firstLine(e)}`); return { atClosed: false }; });
            atClosed = !!posed?.atClosed;
            // An app that has gone is no note: it can have crashed as it folded
            await s.state().catch(ended);
          }
          out.notes.push(...notes.map((note) => `${name}: ${note}`));
          if (!atClosed) await start().catch(ended);
        },
        atResult: (failed) => { if (failed) out.failed.push(failed); },
      });
    } catch (e) { out.error ??= firstLine(e); }
    if (said === undefined) delete process.env.EXACT_SIM; else process.env.EXACT_SIM = said;
    if (prevNoRemember === undefined) delete process.env.EXACT_SIM_NO_REMEMBER; else process.env.EXACT_SIM_NO_REMEMBER = prevNoRemember;
    // As they were: flat, not running, and the app's own choice of simulator (devices.mjs `chooseSimulator`), or what another
    // run has made it. Keep `forgetEnd` registered until every step completes.
    await attempt(`${device} was left folded`, async () => { if (!unfolded && hinge) { await simulatorDisplay(udid, () => hinge('fold', 180)); unfolded = true; } });
    await attempt(`${device}'s keyboard setting was not put back`, async () => { if (!restoredKeyboard && keyboard) { await keyboard.close(); restoredKeyboard = true; } });
    await attempt(`${device} was left running`, () => { if (!shutDown && dev?.bootedHere) { execFileSync('xcrun', ['simctl', 'shutdown', udid], { stdio: 'pipe' }); shutDown = true; } });
    await attempt('the app\'s simulator was not put back', () => {
      if (!restoredChoice && dev && remembers(remembered)?.trim() === udid) { if (used == null) rmSync(remembered); else writeFileSync(remembered, used); }
      restoredChoice = true;
    });
    if (!removedDir && dir) { rmSync(dir, { recursive: true, force: true }); removedDir = true; }
  } finally { forgetEnd(); }
  return out;
}

/** Run the iPhone Duo leg in a separate child process in parallel with the Phone leg when no owned server is running
 *  and Phone and Duo are distinct simulators. Returns `null` when the caller should fall back to sequential `onDuo`,
 *  or `{ device, promise, stop }`. */
export function startParallelDuo({ app, pick, shots, selectedFiles = null, accept = [], phoneUdid = null }) {
  let chosen;
  try { chosen = chooseDuo(pick); } catch (e) { return { device: null, promise: Promise.resolve({ why: firstLine(e) }), stop() {} }; }
  if (!chosen.dev) return { device: null, promise: Promise.resolve(chosen), stop() {} };
  if (!phoneUdid || chosen.dev.udid === phoneUdid) return null;
  const work = mkdtempSync(resolve(tmpdir(), 'exact-duo-worker-'));
  const cfgPath = resolve(work, 'config.json'), outPath = resolve(work, 'result.json');
  mkdirSync(shots, { recursive: true });
  writeFileSync(cfgPath, JSON.stringify({ appSelector: app.selector, appDir: app.dir, pick: chosen.dev.udid, shots, selectedFiles, accept }));
  const workerScript = fileURLToPath(import.meta.url);
  const child = spawn(process.execPath, [workerScript, '--worker', cfgPath, '--out', outPath], {
    env: { ...process.env, EXACT_SIM: chosen.dev.udid, EXACT_SIM_NO_REMEMBER: '1', EXACT_APP_DIR: app.dir },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  let stderr = '';
  child.stderr.on('data', (chunk) => { stderr += String(chunk); });
  const stop = () => {
    try { if (child.exitCode == null && child.signalCode == null) child.kill('SIGTERM'); } catch {}
  };
  const forget = whenEnded(stop);
  const promise = new Promise((done) => {
    child.on('error', (e) => {
      forget();
      rmSync(work, { recursive: true, force: true });
      done({ device: chosen.dev.name, runs: [], failed: [], notes: [], error: firstLine(e) });
    });
    child.on('close', (code, signal) => {
      forget();
      let duo = null;
      try { if (existsSync(outPath)) duo = JSON.parse(readFileSync(outPath, 'utf8')).duo; } catch {}
      rmSync(work, { recursive: true, force: true });
      if (duo) return done(duo);
      const why = stderr.trim().split('\n').filter(Boolean).at(-1) ?? (signal ? `worker killed (${signal})` : `worker exited ${code}`);
      done({ device: chosen.dev.name, runs: [], failed: [], notes: [], error: why });
    });
  });
  return { device: chosen.dev.name, udid: chosen.dev.udid, promise, stop };
}

/** Standalone worker entry for running the iPhone Duo leg in a separate child process in parallel with the Phone leg
 *  (when the app has no owned server and Phone and Duo are distinct simulators). Writes `{ duo }` JSON to `outPath`. */
async function workerMain(argv) {
  const value = (flag) => argv.indexOf(flag) >= 0 ? argv[argv.indexOf(flag) + 1] : undefined;
  const configPath = value('--worker'), outPath = value('--out');
  if (!configPath || !outPath) throw new Error('verify-duo worker requires --worker <cfg> --out <out>');
  const cfg = JSON.parse(readFileSync(configPath, 'utf8'));
  const [{ open }, { runTests }, { checkOp }, { authoredTests }, { systemAlert }] = await Promise.all([
    import('./agent.mjs'), import('./agent-test.mjs'), import('./agent-check.mjs'), import('./app.mjs'), import('./verify.mjs'),
  ]);
  const check = async (s, screen, pose) => {
    if (systemAlert(await s.state())) throw new Error('a system alert is up');
    const shot = screen === FIRST ? resolve(cfg.shots, `ios-duo-${pose.replace(/\W+/g, '-')}.png`) : null;
    const [run] = (await checkOp(s, ['light'], { ...(shot ? { shots: () => shot } : {}), accept: cfg.accept ?? [] })).check;
    if (run.incomplete) throw new Error(run.incomplete);
    return { screen, pose, size: [run.viewport.w, run.viewport.h], scheme: run.scheme, ...(shot ? { shot } : {}), findings: run.findings, accepted: run.accepted, lines: run.findings.map((f) => f.line), hidden: 0, advice: [] };
  };
  const files = cfg.selectedFiles ?? authoredTests(cfg.appDir);
  const drive = async ({ atScreen, atResult }) => {
    const s = await open({ host: 'ios', app: cfg.appSelector, storage: 'verify-duo', env: { EXACT_AGENT_STORAGE_FRESH: '1' }, chrome: 'platform' });
    try { await s.clock('data'); await s.clock('settle'); await atScreen(s, FIRST); } finally { await s.close(); }
    for (const [i, file] of files.entries()) {
      await runTests({ host: 'ios', file, app: cfg.appSelector, storage: `verify-duo${i}`, replayShots: resolve(cfg.shots, 'duo', String(i)), chrome: 'platform',
        atEnd: (t, { name, failures }) => atScreen(t, name, failures.length ? null : undefined),
        atResult: (t) => atResult(t.failures.length ? { name: t.name, file: basename(file), index: t.index, failures: t.failures } : null) });
    }
  };
  const duo = await duoLeg({ pick: cfg.pick, check, drive, remembered: resolve(cfg.appDir, '.exact', 'simulator') });
  const tmp = `${outPath}.${process.pid}.tmp`;
  writeFileSync(tmp, JSON.stringify({ duo }));
  renameSync(tmp, outPath);
}

if (import.meta.main) {
  workerMain(process.argv.slice(2)).then(() => process.exit(0), (e) => { console.error(e.message); process.exit(1); });
}
