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
const displayRotation = display => display.match(/\bmRotation=([0-3])\b/)?.[1];

export function displaySize(wmSize, display) {
  const size = [...wmSize.matchAll(/^(?:Physical|Override) size: (\d+)x(\d+)\s*$/gm)].at(-1);
  const rotation = displayRotation(display);
  if (!size || rotation == null) throw new Error('Cannot read the Android display size or rotation');
  const dimensions = size.slice(1).map(Number);
  return Number(rotation) % 2 ? dimensions.reverse() : dimensions;
}

export function displayDensity(wmDensity) {
  const dpi = [...wmDensity.matchAll(/^(?:Physical|Override) density: (\d+)\s*$/gm)].at(-1);
  if (!dpi) throw new Error('Cannot read the Android display density');
  return Number(dpi[1]) / 160;
}

export function readyDisplay(app, power, policy, windows, user = '0') {
  const { wakefulness, showing } = androidScreenState(power, policy);
  const screen = policy.match(/\bscreenState=SCREEN_STATE_(\w+)/)?.[1];
  if (wakefulness !== 'Awake' || showing !== false || screen !== 'ON') {
    const keyguard = showing == null ? 'unknown' : showing ? 'showing' : 'hidden';
    throw new Error(`${app}: wake and unlock the device before measuring (${wakefulness}, screen ${screen ?? 'unknown'}, keyguard ${keyguard})`);
  }
  const display = defaultDisplay(windows);
  focusedWindow(app, user, display);
  return display;
}

function focusedWindow(app, user, display) {
  const focus = display.match(/\bmCurrentFocus=(Window\{\S+ u(\d+) ([\w.]+\/[\w.]+)\})/);
  const activity = APPS[app];
  if (focus?.[2] !== user || (focus[3] !== activity && focus[3] !== activity.replace('/.', `/${pkg(app)}.`))) {
    throw new Error(`${app}: expected ${activity} for user ${user} to have focus, found ${focus?.[1] ?? 'no focused activity'}`);
  }
  return focus[1];
}

function readyWindow(app, user, wmSize) {
  const power = sh('dumpsys power'), windows = sh('dumpsys window -a');
  const display = readyDisplay(app, power, windows, windows, user);
  if (!/\bmStayOn=true\b/.test(power)) throw new Error('Android stay-on is inactive. Connect the device to power and check its screen timeout policy.');
  const wake = power.match(/\bmLastWakeTime=(\d+)/)?.[1], sleep = power.match(/\bmLastSleepTime=(\d+)/)?.[1];
  const focus = focusedWindow(app, user, display);
  const windowDump = windows.split(/^WINDOW MANAGER WINDOWS\b/m)[1] ?? '';
  const window = windowDump.split(/^[ \t]{0,2}(?:Window #\d+ |(?=\S))/m)
    .find(w => w.startsWith(`${focus}:`)) ?? '';
  const session = window.match(/\bmSession=Session\{\S+ (\d+):u(\d+)a\d+\}/);
  const pid = Number(session?.[1]);
  if (!pid || session[2] !== user || wake == null || sleep == null) {
    throw new Error(`${app}: cannot identify the focused process or its power state`);
  }
  const size = displaySize(wmSize, display);
  const config = window.match(/^[ \t]*mFullConfiguration=(.*)$/m)?.[1] ?? '';
  const frame = window.match(/^[ \t]*Frames:[^\r\n]*(?:[ \t]frame=|\r?\n[ \t]*mFrame=)(\S+)/m)?.[1];
  if (!config.includes('mWindowingMode=fullscreen') || frame !== `[0,0][${size.join(',')}]`) {
    throw new Error(`${app}: expected a full-screen window at ${size.join('×')}, found frame ${frame ?? 'unknown'}`);
  }
  const orientation = config.match(/ (port|land) /)?.[1];
  if (orientation !== (size[0] < size[1] ? 'port' : 'land')) throw new Error(`${app}: cannot confirm the full-screen window orientation`);
  const rotation = displayRotation(display);
  return { focus, pid, size, orientation: orientation === 'port' ? 'portrait' : 'landscape', rotation, wake, sleep };
}

/** Swipes up the middle of the screen: [px/s] × count. `input swipe` moves at constant speed and lifts, so each app
 * flings at that velocity (capped at the platform's maximum) with its own physics. */
const SPEEDS = [[3000, 6], [6000, 6], [12000, 6], [24000, 6]];
function swipeEndpoints([W, H], [left, top, right, bottom]) {
  const x = Math.round(W / 2), y0 = Math.round(H * 0.8), y1 = Math.round(H * 0.25);
  if (x < left || x >= right || y1 < top || y0 >= bottom) {
    throw new Error(`Swipe endpoints (${x},${y0}) and (${x},${y1}) fall outside list bounds [${left},${top}][${right},${bottom}]`);
  }
  return [x, y0, y1];
}

function flings([x, y0, y1], checkFocus, speeds = SPEEDS, wait = 1500) {
  const d = y0 - y1;
  for (const [v, n] of speeds) for (let i = 0; i < n; i++) {
    checkFocus();
    sh(`input swipe ${x} ${y0} ${x} ${y1} ${Math.max(10, Math.round(d / v * 1000))}`);
    sleep(wait);
  }
}

const asCurrent = (user, cmd) => sh(`if [ "$(am get-current-user)" != ${user} ]; then echo 'Android current user changed' >&2; exit 1; fi; ${cmd}`);
const forceStop = (app, user) => asCurrent(user, `am force-stop --user current ${pkg(app)}`);

function launch(app, user, extras = '') {
  forceStop(app, user);
  sleep(500);
  const r = asCurrent(user, `am start --user current -W -n ${APPS[app]} ${extras}`);
  return Number(r.match(/TotalTime: (\d+)/)?.[1]);
}

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
  const bins = (text.match(/^[ \t]*GPU HISTOGRAM:([^\r\n]*)/m)?.[1] ?? '').matchAll(/\b(\d+)ms=(\d+)(?=\s|$)/g);
  const recorded = new Set([...bins].filter(([, , count]) => Number(count) > 0).map(([, label]) => Number(label)));
  const gpu = percentile => {
    const value = n(new RegExp(`^${percentile}th gpu percentile: (\\d+)ms\\s*$`, 'm'));
    if (!recorded.has(value)) return null;
    // HWUI labels its bucket for durations of 25 ms and up as 4950 ms.
    return value === 4950 ? { atLeast: 25 } : value;
  };
  return {
    frames: count('Total frames rendered'), janky: count('Janky frames'), jankyLegacy: count('Janky frames \\(legacy\\)'),
    p50: n(/^50th percentile: (\d+)ms\s*$/m), p90: n(/^90th percentile: (\d+)ms\s*$/m), p95: n(/^95th percentile: (\d+)ms\s*$/m), p99: n(/^99th percentile: (\d+)ms\s*$/m),
    missedVsync: count('Number Missed Vsync'), slowUi: count('Number Slow UI thread'), slowBitmap: count('Number Slow bitmap uploads'),
    slowDraw: count('Number Slow issue draw commands'), deadlineMissed: count('Number Frame deadline missed'),
    gpuP50: gpu(50), gpuP90: gpu(90), gpuP99: gpu(99),
  };
}
const pss = pid => {
  const t = sh(`dumpsys meminfo ${pid}`);
  return Number((t.match(/TOTAL PSS:\s+(\d+)/) ?? t.match(/TOTAL\s+(\d+)/))?.[1]) / 1024;
};

/** One measured stretch: what [drive] costs the app at [p]. */
function measure(app, p, drive, checkReady) {
  const gfx = (option = '') => {
    const text = sh(`dumpsys gfxinfo ${p}${option}`);
    if (!text.includes(`** Graphics info for pid ${p} [${pkg(app)}] **`)) {
      throw new Error(`${app}: gfxinfo did not find the launched process ${p}`);
    }
    return text;
  };
  gfx(' reset');
  const c0 = cpu(p), t0 = Date.now();
  drive();
  const secs = (Date.now() - t0) / 1000;
  const c1 = cpu(p), frames = gfxCounters(gfx()), pssMB = pss(p);
  checkReady();
  return { secs, cpuMsPerS: (c1.proc - c0.proc) / secs, mainMsPerS: (c1.main - c0.main) / secs, pssMB, ...frames };
}

function checkSeriesGeometry(app, device, { size, orientation }) {
  if (device.orientation && (device.orientation !== orientation || device.size.join() !== size.join())) {
    throw new Error(`${app}: refusing mixed window geometry in this series (${orientation} ${size.join('×')}, expected ${device.orientation} ${device.size.join('×')})`);
  }
  device.size = size;
  device.orientation = orientation;
}

async function scenario(app, name, round, device, wmSize, user) {
  const ready = () => readyWindow(app, user, wmSize);
  if (name === 'cold') {
    const runs = [];
    for (let i = 0; i < 5; i++) {
      const total = launch(app, user);
      checkSeriesGeometry(app, device, ready());
      runs.push(total);
      sleep(1500);
    }
    return { runs, orientation: device.orientation };
  }
  launch(app, user);
  sleep(5000);
  let before = ready();
  checkSeriesGeometry(app, device, before);
  try {
    const remote = `/data/local/tmp/heavy-list-${crypto.randomUUID()}`;
    let bounds, endpoints;
    if (name !== 'rest') {
      const xml = `${remote}.xml`;
      bounds = listBounds(sh(`uiautomator dump ${xml} >/dev/null && cat ${xml} && rm ${xml}`), pkg(app));
      endpoints = swipeEndpoints(before.size, bounds);
    }
    // Live updates keep UI Automator from reaching idle. Both launches have the same list layout.
    if (name === 'live') {
      launch(app, user, '--es live 1');
      sleep(5000);
      const live = ready();
      checkSeriesGeometry(app, device, live);
      if (live.rotation !== before.rotation) throw new Error('The display rotated after the list bounds were read');
      before = live;
    }
    const { pid, size, orientation } = before;
    const checkFocus = () => {
      const display = defaultDisplay(sh('dumpsys window displays'));
      if (focusedWindow(app, user, display) !== before.focus || displayRotation(display) !== before.rotation) {
        throw new Error('The focused window changed during the drive');
      }
    };
    const since = sh('date +%s.%N').trim();
    if (!/^\d+\.\d{9}$/.test(since)) throw new Error('Cannot read the device clock for focus checks');
    const checkReady = () => {
      const after = ready();
      if (after.wake !== before.wake || after.sleep !== before.sleep) throw new Error('Device power changed during the drive');
      if (after.focus !== before.focus || after.pid !== pid || after.rotation !== before.rotation || after.size.join() !== size.join()) {
        throw new Error('The window changed during the drive');
      }
      // InputDispatcher records focus loss even when the same window regains focus.
      const events = sh(`logcat -b events -v brief -t ${since} -s input_focus:I`);
      if (/\binput_focus\s*\(/.test(events)) throw new Error('Input focus changed during the drive');
    };
    if (name === 'rest') return { ...measure(app, pid, () => sleep(10_000), checkReady), orientation };
    const sample = name === 'blank'
      ? await blank(app, round, size, device.density, bounds, `${remote}.mp4`, file =>
        recordBlank(file, () => flings(endpoints, checkFocus, [[24000, 8]], 1200), checkReady))
      : measure(app, pid, () => flings(endpoints, checkFocus), checkReady);
    return { ...sample, orientation };
  } catch (error) {
    console.warn(`${app} ${name} round ${round}: discarded sample (${error.message})`);
    return { orientation: before.orientation, error: error.message };
  }
}

export function listBounds(xml, packageName) {
  for (const [node] of xml.matchAll(/<node\b[^>]*>/g)) {
    if (!node.includes(`package="${packageName}"`) || !node.includes('scrollable="true"')) continue;
    const bounds = node.match(/\bbounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/);
    if (bounds) return bounds.slice(1).map(Number);
  }
  throw new Error(`${packageName}: UI Automator did not report the list's bounds`);
}

async function recordBlank(file, drive, checkReady) {
  const rec = Bun.spawn(['adb', 'shell', '-T', `echo $$ > ${file}.pid && exec screenrecord --verbose --bit-rate 40000000 --time-limit 30 ${file}`],
    { stdout: 'pipe', stderr: 'inherit' });
  const output = new Response(rec.stdout).text();
  try {
    sleep(1500);
    drive();
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
  if (rec.exitCode !== 0) throw new Error(`screenrecord exited with status ${rec.exitCode}`);
  checkReady();
  return output;
}

export function recordingCrop(encoded, recorder, [W, H]) {
  const size = encoded.trim().match(/^(\d+)x(\d+)$/);
  const areas = [...recorder.matchAll(/^Content area is (\d+)x(\d+) at offset x=(\d+) y=(\d+)\s*$/gm)];
  if (!size || areas.length !== 1) throw new Error('Cannot verify the encoded size and a single screenrecord content rectangle');
  const [width, height] = size.slice(1).map(Number);
  const [w, h, x, y] = areas[0].slice(1).map(Number);
  if (!w || !h || x + w > width || y + h > height) {
    throw new Error(`Recorded content ${w}×${h} at (${x},${y}) falls outside the encoded ${width}×${height} video`);
  }
  // screenrecord projects in float32, then truncates the fitted side to an integer pixel.
  const aspect = Math.fround(H / W);
  const fittedHeight = Math.trunc(Math.fround(width * aspect));
  const matchesProjection = height > fittedHeight
    ? w === width && h === fittedHeight
    : w === Math.trunc(Math.fround(height / aspect)) && h === height;
  if (Math.abs(w * H - h * W) >= Math.max(W, H) && !matchesProjection) {
    throw new Error(`Recorded content ${w}×${h} does not match the display aspect ratio ${W}×${H}`);
  }
  return `crop=${w}:${h}:${x}:${y}:exact=1`;
}

/** Films 8 flings at 24k px/s and counts the frames whose list area has a band of background at least a quarter
 * of the screen tall: rows the app had not drawn yet (no row of the feed is that empty). */
async function blank(app, round, size, density, bounds, file, record) {
  const video = `blank-${app}-r${round}.mp4`, local = resolve(out, video);
  let recorder;
  try {
    recorder = await record(file);
    adb('pull', file, local);
  } finally {
    try { sh(`rm -f ${file} ${file}.pid`); } catch (error) { console.warn(error.message); }
  }
  const probe = spawnSync('ffprobe', ['-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height',
    '-of', 'csv=p=0:s=x', local], { encoding: 'utf8' });
  if (probe.status !== 0) throw new Error(`Cannot read the encoded size of ${video}: ${probe.error ?? probe.stderr}`);
  const crop = recordingCrop(probe.stdout, recorder, size);
  const w = 90, h = Math.round(w * size[1] / size[0]);
  const ff = spawnSync('ffmpeg', ['-v', 'error', '-i', local, '-fps_mode', 'passthrough', '-vf', `${crop},scale=${w}:${h},format=gray`, '-f', 'rawvideo', '-'], { maxBuffer: 1 << 30 });
  const raw = ff.stdout;
  if (ff.status !== 0 || !raw?.length || raw.length % (w * h)) throw new Error(`${local}: ffmpeg (${ff.status}) gave ${raw?.length ?? 0} bytes, not whole ${w}x${h} frames: ${ff.error ?? ff.stderr}`);
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
  const user = sh('am get-current-user').trim();
  if (!/^\d+$/.test(user)) throw new Error('Cannot identify the current Android user');
  const setting = 'global stay_on_while_plugged_in';
  const previous = sh(`settings get ${setting}`).trim();
  if (previous !== 'null' && !/^\d+$/.test(previous)) throw new Error('Cannot read the previous Android stay-on setting');
  const restore = () => sh(previous === 'null' ? `settings delete ${setting}` : `settings put ${setting} ${previous}`);
  for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143]]) {
    process.once(signal, () => { restore(); process.exit(code); });
  }
  try {
    sh('svc power stayon true');
    for (let round = 0; round < Number(roundsArg); round++) {
      const order = apps.map((_, i) => apps[(i + round) % apps.length]);
      for (const app of order) for (const s of SCENARIOS) {
        const r = await scenario(app, s, round, device, wmSize, user);
        if (!Object.keys(results).length) {
          writeFileSync(resolve(out, 'device.json'), JSON.stringify(device, null, 1));
          console.log(device);
        }
        console.log(round, app, s, JSON.stringify(r));
        ((results[app] ??= {})[s] ??= []).push(r);
        writeFileSync(resolve(out, 'results.json'), JSON.stringify({ device, results }, null, 1));
      }
      apps.forEach(a => forceStop(a, user));
    }
  } finally {
    restore();
  }
}
