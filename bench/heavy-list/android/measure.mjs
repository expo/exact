// The heavy list on an Android phone: the Exact, Views and Compose apps driven by the same touches and measured the
// same way, from outside each app (README.md). One series:
//
//   ANDROID_SERIAL=<device> bun bench/heavy-list/android/measure.mjs <out-dir> [rounds] [apps]
//
// Per round and app, in a rotated order: `cold` (process start to the first frame, `am start -W`, five launches),
// `fling` (24 swipes at four speeds), `live` (the same with live mode on) and `rest` (10 s untouched), each read
// from `dumpsys gfxinfo` (frames the app's renderer drew), `/proc` (process and main-thread CPU) and
// `dumpsys meminfo` (PSS). `blank` films 8 fast flings with screenrecord and counts frames with a blank band.
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { androidScreenState } from '../../../scripts/agent-android.mjs';

const APPS = {
  exact: 'dev.exact.heavybench.exact/dev.exact.app.MainActivity',
  views: 'dev.exact.heavybench.views/.MainActivity',
  compose: 'dev.exact.heavybench.compose/.MainActivity',
};
const [out = 'target/bench/heavy-list/android', roundsArg = '3', appsArg = 'exact,views,compose'] = process.argv.slice(2);
const apps = appsArg.split(',');

const adb = (...args) => {
  const r = spawnSync('adb', args, { encoding: 'utf8', maxBuffer: 256 << 20 });
  if (r.status !== 0) throw new Error(`adb ${args.join(' ')}: ${r.stderr || r.stdout}`);
  return r.stdout;
};
const sh = cmd => adb('shell', cmd);
const sleep = ms => Bun.sleepSync(ms);
const pkg = app => APPS[app].split('/')[0];

const defaultDisplay = text => text.split(/^[ \t]*Display: mDisplayId=/m).find(part => /^0\b/.test(part)) ?? '';

export function displaySize(wmSize, display) {
  const size = [...wmSize.matchAll(/^(?:Physical|Override) size: (\d+)x(\d+)\s*$/gm)].at(-1);
  const rotation = display.match(/\bmRotation=([0-3])\b/)?.[1];
  if (!size || rotation == null) throw new Error('Cannot read the Android display size or rotation');
  const dimensions = size.slice(1).map(Number);
  return Number(rotation) % 2 ? dimensions.reverse() : dimensions;
}

export function displayDensity(wmDensity) {
  const dpi = [...wmDensity.matchAll(/^(?:Physical|Override) density: (\d+)\s*$/gm)].at(-1);
  if (!dpi) throw new Error('Cannot read the Android display density');
  return Number(dpi[1]) / 160;
}

export function readyDisplay(app, power, policy, windows) {
  const { wakefulness, showing } = androidScreenState(power, policy);
  const screen = policy.match(/\bscreenState=SCREEN_STATE_(\w+)/)?.[1];
  if (wakefulness !== 'Awake' || showing !== false || (screen && screen !== 'ON')) {
    const keyguard = showing == null ? 'unknown' : showing ? 'showing' : 'hidden';
    throw new Error(`${app}: wake and unlock the device before measuring (${wakefulness}, screen ${screen ?? 'unknown'}, keyguard ${keyguard})`);
  }
  const display = defaultDisplay(windows);
  const focus = display.match(/\bmCurrentFocus=Window\{[^}\n]*\s([\w.]+\/[\w.]+)(?=[\s}])/)?.[1];
  const activity = APPS[app];
  if (focus !== activity && focus !== activity.replace('/.', `/${pkg(app)}.`)) {
    throw new Error(`${app}: expected ${activity} to have focus, found ${focus ?? 'no focused activity'}`);
  }
  return display;
}
const ready = app => readyDisplay(app, sh('dumpsys power'), sh('dumpsys window policy'), sh('dumpsys window displays'));

/** Swipes up the middle of the screen: [px/s] × count. `input swipe` moves at constant speed and lifts, so each app
 * flings at that velocity (capped at the platform's maximum) with its own physics. */
const SPEEDS = [[3000, 6], [6000, 6], [12000, 6], [24000, 6]];
function flings([W, H], speeds = SPEEDS, wait = 1500) {
  const x = Math.round(W / 2), y0 = Math.round(H * 0.8), y1 = Math.round(H * 0.25), d = y0 - y1;
  for (const [v, n] of speeds) for (let i = 0; i < n; i++) {
    sh(`input swipe ${x} ${y0} ${x} ${y1} ${Math.max(10, Math.round(d / v * 1000))}`);
    sleep(wait);
  }
}

function launch(app, extras = '') {
  sh(`am force-stop ${pkg(app)}`);
  sleep(500);
  const r = sh(`am start -W -n ${APPS[app]} ${extras}`);
  const total = Number(r.match(/TotalTime: (\d+)/)?.[1]);
  return { total, pid: pid(app) };
}
const pid = app => Number(sh(`pidof ${pkg(app)}`).trim().split(/\s+/)[0]);

/** utime + stime in ms of the process and of its main thread (whose tid is its pid). */
function cpu(p) {
  const ticks = s => { const f = s.slice(s.lastIndexOf(')') + 2).split(' '); return Number(f[11]) + Number(f[12]); };
  const [proc, main] = sh(`cat /proc/${p}/stat /proc/${p}/task/${p}/stat`).trim().split('\n');
  return { proc: ticks(proc) * 10, main: ticks(main) * 10 };
}

export function gfxCounters(text) {
  const n = re => {
    const value = Number(text.match(re)?.[1]);
    return Number.isFinite(value) ? value : null;
  };
  const count = name => n(new RegExp(`^[ \\t]*${name}: (\\d+)(?=\\s|$)`, 'm'));
  return {
    frames: count('Total frames rendered'), janky: count('Janky frames'), jankyLegacy: count('Janky frames \\(legacy\\)'),
    p50: n(/^50th percentile: (\d+)ms\s*$/m), p90: n(/^90th percentile: (\d+)ms\s*$/m), p95: n(/^95th percentile: (\d+)ms\s*$/m), p99: n(/^99th percentile: (\d+)ms\s*$/m),
    missedVsync: count('Number Missed Vsync'), slowUi: count('Number Slow UI thread'), slowBitmap: count('Number Slow bitmap uploads'),
    slowDraw: count('Number Slow issue draw commands'), deadlineMissed: count('Number Frame deadline missed'),
    gpuP50: n(/50th gpu percentile: (\d+)ms\s*$/m), gpuP90: n(/90th gpu percentile: (\d+)ms\s*$/m), gpuP99: n(/99th gpu percentile: (\d+)ms\s*$/m),
  };
}
const pss = app => {
  const t = sh(`dumpsys meminfo ${pkg(app)}`);
  return Number((t.match(/TOTAL PSS:\s+(\d+)/) ?? t.match(/TOTAL\s+(\d+)/))?.[1]) / 1024;
};

/** One measured stretch: what [drive] costs the app at [p]. */
function measure(app, p, drive) {
  sh(`dumpsys gfxinfo ${pkg(app)} reset`);
  const c0 = cpu(p), t0 = Date.now();
  drive();
  const secs = (Date.now() - t0) / 1000, c1 = cpu(p), frames = gfxCounters(sh(`dumpsys gfxinfo ${pkg(app)}`));
  return { secs, cpuMsPerS: (c1.proc - c0.proc) / secs, mainMsPerS: (c1.main - c0.main) / secs, pssMB: pss(app), ...frames };
}

async function scenario(app, name, round, device, wmSize) {
  if (name === 'cold') {
    const runs = [];
    for (let i = 0; i < 5; i++) {
      const { total } = launch(app);
      const display = ready(app);
      device.size ??= displaySize(wmSize, display);
      runs.push(total);
      sleep(1500);
    }
    return { runs };
  }
  const { pid: p } = launch(app, name === 'live' ? '--es live 1' : '');
  sleep(5000);
  const size = displaySize(wmSize, ready(app));
  device.size ??= size;
  if (name === 'rest') return measure(app, p, () => sleep(10_000));
  if (name === 'blank') return blank(app, round, size, device.density);
  return measure(app, p, () => flings(size));
}

export function listBounds(xml, packageName) {
  for (const [node] of xml.matchAll(/<node\b[^>]*>/g)) {
    if (!node.includes(`package="${packageName}"`) || !node.includes('scrollable="true"')) continue;
    const bounds = node.match(/\bbounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
    if (bounds) return bounds.slice(1).map(Number);
  }
  throw new Error(`${packageName}: UI Automator did not report the list's bounds`);
}

async function recordBlank(file, size) {
  const rec = Bun.spawn(['adb', 'shell', '-T', `echo $$ > ${file}.pid && exec screenrecord --bit-rate 40000000 --time-limit 30 ${file}`],
    { stdout: 'ignore', stderr: 'inherit' });
  try {
    sleep(1500);
    flings(size, [[24000, 8]], 1200);
    sleep(1000);
  } finally {
    try {
      // The file argument distinguishes this recorder if Android has reused its PID.
      sh(`if read pid < ${file}.pid && tr '\\000' '\\n' < /proc/$pid/cmdline | grep -Fxq '${file}'; then kill -INT "$pid"; fi`);
    } catch (error) {
      console.warn(`Could not stop ${file}: ${error.message}`);
    }
    await rec.exited;
  }
  return rec.exitCode;
}

/** Films 8 flings at 24k px/s and counts the frames whose list area has a band of background at least a quarter
 * of the screen tall: rows the app had not drawn yet (no row of the feed is that empty). */
async function blank(app, round, size, density) {
  const video = `blank-${app}-r${round}.mp4`, local = resolve(out, video);
  const remote = `/data/local/tmp/heavy-list-${crypto.randomUUID()}`;
  const xml = `${remote}.xml`, file = `${remote}.mp4`;
  const bounds = listBounds(sh(`uiautomator dump ${xml} >/dev/null && cat ${xml} && rm ${xml}`), pkg(app));
  try {
    const status = await recordBlank(file, size);
    if (status !== 0) throw new Error(`screenrecord exited with status ${status}`);
    adb('pull', file, local);
  } catch (error) {
    console.warn(`${video}: ${error.message}`);
    return { error: error.message };
  } finally {
    try { sh(`rm -f ${file} ${file}.pid`); } catch (error) { console.warn(error.message); }
  }
  const w = 90, h = Math.round(w * size[1] / size[0]);
  const ff = spawnSync('ffmpeg', ['-v', 'error', '-i', local, '-fps_mode', 'passthrough', '-vf', `scale=${w}:${h},format=gray`, '-f', 'rawvideo', '-'], { maxBuffer: 1 << 30 });
  const raw = ff.stdout;
  if (ff.status !== 0 || !raw.length || raw.length % (w * h)) throw new Error(`${local}: ffmpeg (${ff.status}) gave ${raw.length} bytes, not whole ${w}x${h} frames: ${ff.stderr}`);
  const result = blankFrames(raw, [w, h], size, density, bounds);
  if (result.blanks) return { ...result, video };
  rmSync(local);
  return result;
}

export function blankFrames(raw, [w, h], [W, H], density, [x0, y0, x1, y1]) {
  const left = Math.round(x0 * w / W), top = Math.round(y0 * h / H), bottom = Math.min(h, Math.round(y1 * h / H));
  // Clear the scrollbar and one downsampled pixel for the scaling filter.
  const right = Math.min(w, Math.round(x1 * w / W)) - Math.ceil(8 * density * w / W) - 1;
  if (left >= right || top >= bottom) throw new Error('The measured list bounds are empty');
  const frames = raw.length / (w * h), band = Math.ceil(h * 0.25);
  let blanks = 0, worst = 0;
  for (let f = 0; f < frames; f++) {
    let run = 0, longest = 0;
    for (let y = top; y < bottom; y++) {
      let white = true;
      for (let x = left; x < right && white; x++) white = raw[f * w * h + y * w + x] >= 245;
      run = white ? run + 1 : 0; longest = Math.max(longest, run);
    }
    if (longest >= band) blanks++;
    worst = Math.max(worst, longest / h);
  }
  return { frames, blanks, worstBand: worst };
}

if (import.meta.main) {
  mkdirSync(out, { recursive: true });
  const wmSize = sh('wm size');
  const device = {
    model: sh('getprop ro.product.model').trim(), android: sh('getprop ro.build.version.release').trim(),
    density: displayDensity(sh('wm density')), refresh: sh('dumpsys display | grep -m1 -o "renderFrameRate [0-9.]*" || true').trim(),
    fontScale: sh('settings get system font_scale').trim(),
  };
  const SCENARIOS = (process.env.SCENARIOS ?? 'cold,fling,live,rest,blank').split(',');
  const results = {};
  for (let round = 0; round < Number(roundsArg); round++) {
    const order = apps.map((_, i) => apps[(i + round) % apps.length]);
    for (const app of order) for (const s of SCENARIOS) {
      const r = await scenario(app, s, round, device, wmSize);
      if (!Object.keys(results).length) {
        writeFileSync(resolve(out, 'device.json'), JSON.stringify(device, null, 1));
        console.log(device);
      }
      console.log(round, app, s, JSON.stringify(r));
      ((results[app] ??= {})[s] ??= []).push(r);
      writeFileSync(resolve(out, 'results.json'), JSON.stringify({ device, results }, null, 1));
    }
    apps.forEach(a => sh(`am force-stop ${pkg(a)}`));
  }
}
