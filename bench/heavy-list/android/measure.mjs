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

const APPS = {
  exact: 'dev.exact.heavybench.exact/dev.exact.app.MainActivity',
  views: 'dev.exact.heavybench.views/.MainActivity',
  compose: 'dev.exact.heavybench.compose/.MainActivity',
};
const [out = 'target/bench/heavy-list/android', roundsArg = '3', appsArg = 'exact,views,compose'] = process.argv.slice(2);
const apps = appsArg.split(',');
mkdirSync(out, { recursive: true });

const adb = (...args) => {
  const r = spawnSync('adb', args, { encoding: 'utf8', maxBuffer: 256 << 20 });
  if (r.status !== 0) throw new Error(`adb ${args.join(' ')}: ${r.stderr || r.stdout}`);
  return r.stdout;
};
const sh = cmd => adb('shell', cmd);
const sleep = ms => Bun.sleepSync(ms);
const pkg = app => APPS[app].split('/')[0];

const [W, H] = sh('wm size').match(/(\d+)x(\d+)/).slice(1).map(Number);
const density = Number(sh('wm density').match(/(\d+)\s*$/m)[1]) / 160;
const device = {
  model: sh('getprop ro.product.model').trim(), android: sh('getprop ro.build.version.release').trim(),
  size: [W, H], density, refresh: sh('dumpsys display | grep -m1 -o "renderFrameRate [0-9.]*" || true').trim(),
  fontScale: sh('settings get system font_scale').trim(),
};
writeFileSync(resolve(out, 'device.json'), JSON.stringify(device, null, 1));
console.log(device);

/** Swipes up the middle of the screen: [px/s] × count. `input swipe` moves at constant speed and lifts, so each app
 * flings at that velocity (capped at the platform's maximum) with its own physics. */
const SPEEDS = [[3000, 6], [6000, 6], [12000, 6], [24000, 6]];
function flings(speeds = SPEEDS, wait = 1500) {
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

function gfx(app) {
  const t = sh(`dumpsys gfxinfo ${pkg(app)}`);
  const n = re => Number(t.match(re)?.[1] ?? NaN);
  return {
    frames: n(/Total frames rendered: (\d+)/), janky: n(/Janky frames: (\d+)/), jankyLegacy: n(/Janky frames \(legacy\): (\d+)/),
    p50: n(/^50th percentile: (\d+)ms/m), p90: n(/^90th percentile: (\d+)ms/m), p95: n(/^95th percentile: (\d+)ms/m), p99: n(/^99th percentile: (\d+)ms/m),
    missedVsync: n(/Number Missed Vsync: (\d+)/), slowUi: n(/Number Slow UI thread: (\d+)/), slowBitmap: n(/Number Slow bitmap uploads: (\d+)/),
    slowDraw: n(/Number Slow issue draw commands: (\d+)/), deadlineMissed: n(/Number Frame deadline missed: (\d+)/),
    gpuP50: n(/50th gpu percentile: (\d+)ms/), gpuP90: n(/90th gpu percentile: (\d+)ms/), gpuP99: n(/99th gpu percentile: (\d+)ms/),
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
  const secs = (Date.now() - t0) / 1000, c1 = cpu(p), frames = gfx(app);
  return { secs, cpuMsPerS: (c1.proc - c0.proc) / secs, mainMsPerS: (c1.main - c0.main) / secs, pssMB: pss(app), ...frames };
}

function scenario(app, name) {
  if (name === 'cold') {
    const runs = [];
    for (let i = 0; i < 5; i++) { runs.push(launch(app).total); sleep(1500); }
    return { runs };
  }
  const { pid: p } = launch(app, name === 'live' ? '--es live 1' : '');
  sleep(5000);
  if (name === 'rest') return measure(app, p, () => sleep(10_000));
  if (name === 'blank') return blank(app);
  return measure(app, p, () => flings());
}

/** Films 8 flings at 24k px/s and counts the frames whose list area has a band of background at least a quarter
 * of the screen tall: rows the app had not drawn yet (no row of the feed is that empty). */
function blank(app) {
  const file = `/data/local/tmp/blank-${app}.mp4`, local = resolve(out, `blank-${app}.mp4`);
  const rec = Bun.spawn(['adb', 'shell', `screenrecord --bit-rate 40000000 --time-limit 30 ${file}`]);
  sleep(1500);
  flings([[24000, 8]], 1200);
  sleep(1000);
  sh('pkill -INT screenrecord');
  sleep(2000);
  rec.kill();
  adb('pull', file, local);
  sh(`rm ${file}`);
  const w = 90, h = Math.round(w * H / W);
  const ff = spawnSync('ffmpeg', ['-v', 'error', '-i', local, '-fps_mode', 'passthrough', '-vf', `scale=${w}:${h},format=gray`, '-f', 'rawvideo', '-'], { maxBuffer: 1 << 30 });
  const raw = ff.stdout;
  if (ff.status !== 0 || !raw.length || raw.length % (w * h)) throw new Error(`${local}: ffmpeg (${ff.status}) gave ${raw.length} bytes, not whole ${w}x${h} frames: ${ff.stderr}`);
  const frames = raw.length / (w * h), top = Math.round(h * 0.12), band = Math.round(h * 0.25);
  let blanks = 0, worst = 0;
  for (let f = 0; f < frames; f++) {
    let run = 0, longest = 0;
    for (let y = top; y < h; y++) {
      let white = true;
      for (let x = 0; x < w && white; x++) white = raw[f * w * h + y * w + x] >= 245;
      run = white ? run + 1 : 0; longest = Math.max(longest, run);
    }
    if (longest >= band) blanks++;
    worst = Math.max(worst, longest / h);
  }
  if (!blanks) rmSync(local); // a video with blanks stays, to look at
  return { frames, blanks, worstBand: worst };
}

const SCENARIOS = (process.env.SCENARIOS ?? 'cold,fling,live,rest,blank').split(',');
const results = {};
for (let round = 0; round < Number(roundsArg); round++) {
  const order = apps.map((_, i) => apps[(i + round) % apps.length]);
  for (const app of order) for (const s of SCENARIOS) {
    const r = scenario(app, s);
    console.log(round, app, s, JSON.stringify(r));
    ((results[app] ??= {})[s] ??= []).push(r);
    writeFileSync(resolve(out, 'results.json'), JSON.stringify({ device, results }, null, 1));
  }
  apps.forEach(a => sh(`am force-stop ${pkg(a)}`));
}
