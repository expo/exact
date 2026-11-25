// Native desktop and phone carriers over the agent JSON-lines wire.
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { appleRunBinary, appleArtifacts, assertAppleIdentity } from '../host/apple/build.mjs';
import { noWindowsHost } from '../host/windows/build.mjs';
import { phone, phoneBridge, phoneStartupFailure, phoneLaunch, crashReports, displayPictureReply } from '../host/apple/devices.mjs';
import { resolveApp, executableName, linuxBinary, bakeOutput, bakeTarget } from './app.mjs';
import { linuxRebuild, macSandboxed, nativeBuild, ownVerb, planOverride, rebuildCommand, refuseBuild } from './agent-build.mjs';
import { refuseStale, refuseBackendOrigin, refuseBinaryBackend, proofProductChanges, unchecked, packagedBuildChanges, depInfoChanges, depInfoNewer, newerThan, shown, bakedSettingChanges, watchedNewer } from './agent-launch.mjs';
import { androidBinary, androidDeploy, apkRecord } from './agent-android.mjs';
import { iosTouchInput, openTouches } from '../host/apple/touches.mjs';
import { nativeKey, mouseContact } from './agent-keys.mjs';
import { activateIOS, backgroundIOS, jsonLines, hangup, waitAtMost, bootRefusal, REPLY_MS } from './agent-wire.mjs';
import { platformWheel } from './agent-wheel.mjs';
const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

// A descendant can retain the pipes after its carrier exits, even after the parent kills the carrier.
export async function closeStdio(child, closed, cleanup) {
  await waitAtMost(closed, 2000);
  if (child.exitCode === null && child.signalCode === null) { try { child.kill('SIGKILL'); } catch {} }
  await waitAtMost(closed, 2000, () => {
    try { cleanup?.(); }
    finally { for (const stream of [child.stdin, child.stdout, child.stderr]) stream?.destroy(); }
  });
}

/** The command that builds what an Android drive runs: the APK (`reader`), else the Linux host for Android. */
export function androidRebuild(a, reader) {
  const selected = reader && apkRecord(a)?.abis, abis = selected?.length ? ` --abis ${selected.join(',')}` : '';
  return ownVerb(a, reader ? `android${abis}` : 'android --headless') ?? `bun scripts/agent-android.mjs build ${a.name}${reader ? ` --apk${abis}` : ''}`;
}
/** What changed since the APK was built, by the record its build left (agent-android.mjs `apkRecord`): each library's
 * Cargo inputs and separate bake inputs, the Kotlin by time, and the icons by digest. */
export function apkChanges(a, apk) {
  const record = apkRecord(a, apk);
  if (!record?.libraries?.length) return [`${apk} (no complete ABI build record beside it: build it again)`];
  const since = Date.parse(record.started), digest = (file) => { try { return createHash('sha256').update(readFileSync(file)).digest('hex'); } catch { return null; } };
  const baked = (record.bakes ?? []).flatMap(({ paths, missing, watchedDirectories, products }) => {
    const bound = new Map(products.map(({path, sha256}) => [path, sha256]));
    return [
      ...watchedNewer(Date.parse(record.bakeStarted), paths.filter(path => !missing.includes(path) && !bound.has(path)),
        watchedDirectories, bound, [a.target]),
      ...[...bound].filter(([path, sha256]) => digest(path) !== sha256).map(([path]) => shown(path)),
      ...missing.filter(existsSync).map(shown),
    ];
  });
  return [...new Set([...baked, ...record.libraries.flatMap(library => depInfoNewer(since, library, library.replace(/\.so$/, '.d'), record.workspace))]),
    ...newerThan(since, record.sources.filter((root) => !root.sha256)),
    ...record.sources.filter((root) => root.sha256 && digest(root.file) !== root.sha256).map((root) => shown(root.file))];
}

export async function phoneScreenshot({ ask, udid, appId, container, path, window, film, own, skipMotion, run = spawnSync }) {
  // Held-contact stills set film to skip the motion wait. They still picture the display.
  const display = window === 'display' || (!own && (!film || skipMotion));
  const source = 'tmp/exact-agent.png';
  const request = { op: 'screenshot', window: display ? 'display' : window, ...(skipMotion ? { skipMotion } : {}) };
  const observe = async (req, deadline) => {
    try {
      const reply = await ask(req, deadline);
      return reply?.error ? null : reply;
    } catch { return null; }
  };
  // Preparation can wait for images, motion and a canvas. The post-capture probe only reads geometry and focus.
  const r = display ? (await observe(request, 30000) ?? {})
    : await ask({ ...request, path: `${container}/${source}`, ...(film ? { film } : {}) });
  if (r.error) throw new Error(r.error);
  const args = display
    ? ['capture', 'screenshot', '--device', udid]
    : ['copy', 'from', '--quiet', '--device', udid, '--domain-type', 'appDataContainer',
        '--domain-identifier', appId, '--source', source];
  const out = resolve(path);
  const temp = display ? mkdtempSync(resolve(dirname(out), '.exact-display-')) : null;
  const destination = temp ? resolve(temp, 'display.png') : out;
  try {
    const taken = run('xcrun', ['devicectl', 'device', ...args, '--destination', destination], { encoding: 'utf8', timeout: 20000 });
    if (taken.status !== 0) {
      const detail = taken.stderr?.trim() || taken.error?.message || taken.stdout?.trim() || `exit ${taken.status}`;
      throw new Error(`phone screenshot (${display ? 'devicectl capture' : 'devicectl copy'}): ${detail}`);
    }
    if (!display) return { ...r, screenshot: path, pixels: 'app' };
    const reply = await displayPictureReply(req => observe(req, 2000), r, destination, { keepPixels: true });
    renameSync(destination, out);
    return { ...reply, screenshot: path };
  } finally {
    if (temp) rmSync(temp, { recursive: true, force: true });
  }
}

/** One JSON-lines protocol over stdio on desktop hosts, or a phone's outbound socket. `reader`: on Android, the app
 * on the Canvas reader (its APK), not the Linux host headless; unsaid, EXACT_ANDROID_READER decides (how a script
 * that opens a drive says it). */
export async function openStdio({ host, plan, world, size, app, env: extra = {}, session, documents = [], device = false, phone: pick, touch = 'agent', reader = !!process.env.EXACT_ANDROID_READER, onProcess }) {
  const a = resolveApp(app);
  const linux = host === 'linux', android = host === 'android'; // @ref LLP 1107 — Android: the Linux host on a phone, under adb
  const windows = host === 'windows', portable = linux || windows || android;
  const sample = host === 'host';
  const artifacts = portable ? null : appleArtifacts(a, { destination: device ? 'ios' : 'macos', host: sample });
  const deviceBundle = artifacts?.bundle;
  const bin = windows ? (process.env.EXACT_WINDOWS_BIN ?? resolve(process.env.EXACT_WINDOWS_OUTPUT ?? resolve(a.dir, 'dist-windows'), `${executableName(a)}.exe`))
    : android ? (process.env.EXACT_ANDROID_BIN ?? androidBinary(a, undefined, reader)) : linux ? (process.env.EXACT_LINUX_BIN ?? linuxBinary(a)) : (process.env.EXACT_MAC_BIN ?? appleRunBinary(a, sample));
  // The macOS build by its own record (agent-build.mjs, which an app's exact.mjs runs first to build it, or bakes its plan-only edit).
  // What a desktop app is started with, where a launch gives a marked setting (LLP 1153.002 D2): what its freshness reads.
  const childEnv = { ...process.env, ...extra };
  const macBuild = !portable && !device && !process.env.EXACT_MAC_BIN ? nativeBuild('macos', a, { sample, checked: process.env.EXACT_BUILD_CHECKED, launch: childEnv }) : null;
  if (macBuild && !macBuild.missing) refuseBackendOrigin(macBuild, a);
  const macPlan = macBuild && !plan && !Object.hasOwn(extra, 'EXACT_PLAN') && !sample ? planOverride('macos', a, macBuild) : null;
  if (macPlan) plan = macPlan.plan; else if (macBuild) refuseBuild('macos', macBuild, a, { sample });
  const windowsRebuild = () => ownVerb(a, 'windows') ?? `bun host/windows/build.mjs ${a.crate('windows')}`;
  if (windows && !process.env.EXACT_WINDOWS_BIN && !ownVerb(a, 'windows') && noWindowsHost(a)) throw new Error(noWindowsHost(a));
  const rebuild = device ? 'bun host/apple/build.mjs --device' : windows ? windowsRebuild() : android ? androidRebuild(a, reader) : linux ? linuxRebuild(a) : null;
  // `missingBuild`: every test of a run would fail on it alike, so a test run stops at the first (LLP 1156.006.000 D8).
  if (!existsSync(device ? deviceBundle : bin)) throw Object.assign(new Error(rebuild ? `run ${rebuild} first` : `EXACT_MAC_BIN names no file: ${bin}`), { missingBuild: !!rebuild });
  if (!macBuild) {
    const receipt = portable ? undefined : device ? resolve(deviceBundle, 'receipt.json')
      : /\.app\/Contents\/MacOS\/[^/]+$/.test(resolve(bin)) ? resolve(bin, '../../Resources/receipt.json') : resolve(dirname(bin), 'receipt.json');
    refuseBinaryBackend(bin, a, { receipt, packaged: windows, command: rebuild });
  }
  if (!portable) assertAppleIdentity(a, device ? resolve(deviceBundle, artifacts.executable) : bin);
  if (portable && process.env.EXACT_PROOF_PRODUCTS) refuseStale(host, bin, proofProductChanges(process.env.EXACT_PROOF_PRODUCTS, bin, a), 'rerun the proof'); else if (windows && process.env.EXACT_WINDOWS_BIN) unchecked('windows', 'EXACT_WINDOWS_BIN');
  else if (windows) {
    const receipt = resolve(bakeOutput(a), `windows-${bakeTarget('windows')}.build.json`);
    refuseStale('windows', bin, packagedBuildChanges(receipt, dirname(bin), a, { launch: childEnv }), windowsRebuild());
  }
  else if ((linux && process.env.EXACT_LINUX_BIN) || (android && process.env.EXACT_ANDROID_BIN)) unchecked(host, android ? 'EXACT_ANDROID_BIN' : 'EXACT_LINUX_BIN');
  else if (android && bin.endsWith('.apk')) refuseStale(host, bin, [...apkChanges(a, bin), ...bakedSettingChanges(bin, a)], androidRebuild(a, true));
  else if (linux || android) refuseStale(host, bin, [...depInfoChanges(bin, a), ...bakedSettingChanges(bin, a, { launch: linux ? childEnv : null })], android ? androidRebuild(a, false) : linuxRebuild(a));
  else if (!device && process.env.EXACT_MAC_BIN) unchecked(host, 'EXACT_MAC_BIN');
  if (device && (plan || extra.EXACT_PLAN || extra.EXACT_ASSETS)) throw new Error('a phone cannot read host-local plan/assets paths; use --url or its embedded app');
  const ph = device ? phone(pick) : null;
  const installPhoneApp = () => {
    const installed = spawnSync('xcrun', ['devicectl', 'device', 'install', 'app', '--device', ph.udid, deviceBundle], { encoding: 'utf8' });
    if (installed.status !== 0) throw new Error(`device install: ${installed.stderr || installed.stdout || installed.error?.message}`);
    const copyFixture = (path, name) => {
      const copied = spawnSync('xcrun', ['devicectl', 'device', 'copy', 'to', '--quiet', '--device', ph.udid,
        '--domain-type', 'appDataContainer', '--domain-identifier', a.id, '--source', resolve(path), '--destination', 'tmp/' + name], { encoding: 'utf8', timeout: 45000 });
      if (copied.status !== 0) throw new Error(`phone ${name} copy: ${copied.stderr || copied.error || copied.stdout}`);
      return '~/tmp/' + name;
    };
    if (world) extra = { ...extra, EXACT_WORLD: copyFixture(world, 'exact-agent.world') };
    if (extra.EXACT_AGENT_SUBSTITUTES?.startsWith('@')) {
      extra = { ...extra, EXACT_AGENT_SUBSTITUTES: '@' + copyFixture(extra.EXACT_AGENT_SUBSTITUTES.slice(1), 'exact-agent-substitutes.json') };
    }
  };
  // A Mac bundle carries its own assets (bundle.mjs copies the capture into Contents/Resources); a bare binary
  // reads the capture of this target directory, which another one's build did not write.
  const assets = windows ? dirname(bin) : linux || android ? a.dir
    : !device && process.env.EXACT_MAC_BIN && /\.app\/Contents\/MacOS\/[^/]+$/.test(resolve(bin)) ? resolve(bin, '../../Resources') : artifacts.capture;
  if (!portable && !device && !process.env.EXACT_ASSETS && !extra.EXACT_ASSETS && !existsSync(assets))
    throw new Error(`${bin}'s assets are not at ${assets}: ${process.env.EXACT_MAC_BIN ? 'export the CARGO_TARGET_DIR it was built with, or name its assets with EXACT_ASSETS' : `run ${sample ? `bun host/apple/build.mjs ${a.crate('apple')} --host` : rebuildCommand(a, 'mac')}`}`);
  const env = { EXACT_ASSETS: assets, ...process.env, EXACT_AGENT: '1' };
  if (plan) env.EXACT_PLAN = plan;
  if (portable && size) env.EXACT_SIZE = `${size[0]}x${size[1]}`;
  // @ref LLP 1039 §5 — measure the requested Mac content viewport.
  if (!portable && size) { env.EXACT_WINDOW_WIDTH = String(size[0]); env.EXACT_WINDOW_HEIGHT = String(size[1]); }
  if (portable) {
    // The pinned font: DejaVu Sans from scripts/fixtures/fonts shapes and
    // paints the host's text on every machine, so a pixel fixture recorded
    // here matches on a builder (LLP 1015 §5). The environment still wins.
    env.EXACT_PAINTER ??= 'cpu';
    if (windows) env.EXACT_GPU_RENDER ??= '1';
    env.EXACT_FONTS ??= android ? '/system/fonts' : resolve(ROOT, 'scripts/fixtures/fonts/assets'); // a phone draws with its own faces
    if (!android) env.EXACT_FONT ??= 'DejaVu Sans';
  }
  Object.assign(env, extra);
  const bridge = device ? await phoneBridge() : null, droid = android ? androidDeploy(a, bin, { ...env, ...extra }) : null;
  let touches = null;
  try {
    if (device && touch !== 'agent') {
      touches = await openTouches({ udid: ph.udid, appId: a.id, device: true, installApp: installPhoneApp });
    } else if (device) installPhoneApp();
  } catch (error) { bridge?.close(); throw error; }
  const launched = Date.now(); let closing = false;
  const child = device
    ? spawn('xcrun', ['devicectl', 'device', 'process', 'launch', '--quiet', '--console', '--terminate-existing', '--device', ph.udid,
        '--environment-variables', JSON.stringify(phoneLaunch({ ...(size ? { EXACT_WINDOW_WIDTH: env.EXACT_WINDOW_WIDTH, EXACT_WINDOW_HEIGHT: env.EXACT_WINDOW_HEIGHT } : {}), ...extra, ...bridge.env })), a.id], { stdio: ['pipe', 'pipe', 'pipe'] })
    : droid ? droid.spawn(env.EXACT_LAUNCH_URL ? [env.EXACT_LAUNCH_URL] : []) : spawn(bin, portable && env.EXACT_LAUNCH_URL ? [env.EXACT_LAUNCH_URL] : documents, { env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide:true });
  onProcess?.(child);
  const hostLines = [];
  // hostLines takes each chunk as it arrives, so a diagnostic with no newline yet still reaches a timeout or exit
  // message; the Android startup lines are forwarded to the driver's stderr only once complete.
  let partial = '';
  child.stderr.on('data', (d) => {
    const text = String(d);
    for (const l of text.split('\n')) if (l) hostLines.push('app: ' + l);
    if (!droid) return;
    const parts = (partial + text).split('\n');
    partial = parts.pop();
    for (const line of parts) if (/^Android (?:before launch|focused activity):/.test(line)) console.error(line);
  });
  if (device) child.stdout.on('data', (d) => { for (const l of String(d).split('\n')) if (l) hostLines.push('app: ' + l); });
  let lines = device ? null : jsonLines(child.stdout, child.stdin, hostLines);
  const fail = (why) => { lines?.fail(why); bridge?.fail(new Error(why)); };
  const closed = new Promise(resolve => {
    child.once('close', resolve);
    child.once('error', resolve);
  });
  let closingPromise;
  const close = () => {
    if (closingPromise) return closingPromise;
    closing = true; bridge?.close();
    // Ending the stdin of a process that has exited raises EPIPE on the stream, which would end the driver.
    if (child.exitCode === null && child.signalCode === null) {
      child.stdin.on('error', () => {});
      try { child.stdin.end(); if (device) child.kill('SIGTERM'); } catch {}
    }
    return closingPromise = closeStdio(child, closed, droid?.cleanup).finally(() => touches?.close());
  };
  const cleanupAfterExit = () => { if (!closing) void close().catch(error => console.error(`app cleanup: ${error.message}`)); };
  child.on('error', error => { fail(`launch failed: ${error.message}`); cleanupAfterExit(); });
  child.on('exit', (code, signal) => {
    fail(closing ? 'the app was closed' : hangup({ what: 'the app exited', exit: { code, signal }, hostLines, reports: device || droid ? [] : crashReports(basename(bin), launched) }));
    cleanupAfterExit();
  });
  let readyTimeout;
  try {
    const readyLine = device ? bridge.ready.then(({ socket, announcement }) => {
      lines = jsonLines(socket, socket, hostLines);
      socket.on('close', () => lines.fail('the phone agent connection closed; ' + hostLines.slice(-20).join('\n')));
      socket.resume();
      return announcement;
    }) : lines.next();
    // An Android launch is adb's round trips (about a second each to an emulator through a tunnel) and the app's cold start.
    const ready = await Promise.race([readyLine, new Promise((_, reject) => {
      readyTimeout = setTimeout(() => reject(new Error('the app never became ready; ' + (hostLines.join('\n') || 'the app printed nothing'))), android ? 60000 : 20000);
    })]);
    clearTimeout(readyTimeout);
    if (!ready.ready) throw new Error('unexpected first line: ' + JSON.stringify(ready));
    if (ready.error) throw new Error(bootRefusal(ready.error));
    // The sample host routes by label: the session the caller named, and
    // `s.session = "b"` moves every later request to another.
    const state = { session: session ?? null };
    const heldButton = mouseContact();
    const heldWheel = { open: null }; // a `wheel … hold` not yet released (LLP 1142 D3)
    const replyMs = device ? 45000 : REPLY_MS;
    const ask = (req, session = state.session, optionalMs) => {
      const limit = optionalMs ?? (replyMs + (req.op === 'perf' && req.live > 0 ? req.live : 0));
      return waitAtMost(lines.ask(session ? { ...req, session } : req), limit, () => {
        if (optionalMs != null) return null;
        const why = device ? `phone ${req.op} did not answer within 45 s; check app health, foreground state and network\n` + hostLines.slice(-20).join('\n')
          : hangup({ what: `${req.op} did not answer within ${REPLY_MS / 1000} s`, pid: child.pid, hostLines });
        if (device) bridge.close(); else lines.fail(why);
        throw new Error(why);
      });
    };
    return {
      host, touches, touch, reader: android && bin.endsWith('.apk'), boot: ready.boot, hostLines, gpuMs: () => null, sessions: ready.sessions ?? null, state, plan,
      background: device ? ms => backgroundIOS(lines, ms, a.id, bundle => activateIOS(['devicectl', 'device', 'process', 'launch', '--device', ph.udid], bundle)) : undefined,
      ask, reveal: (id) => ask({ op: 'reveal', id }),
      async input(id, kind, opts) {
        if (kind === 'key') {
          const session = state.session;
          return nativeKey({id, opts: portable ? {...opts, ownedRelease:false} : opts, ask: req => ask(req, session)});
        }
        if (kind === 'pictureinpicture') { const r = await ask({ op: 'tap', id, pictureInPicture: opts.pictureInPicture }); if (r.error) throw new Error(r.error); return r; }
        const guest = { selector: opts.selector, x: opts.x, y: opts.y, entity: opts.entity, world: opts.world, under: opts.under, phase: opts.phase };
        const gesture = iosTouchInput({ ask, touches, touch, id, kind, opts, guest, abandon: why => lines.fail(why) });
        if (gesture) return gesture;
        const phase = ['down', 'move', 'hold', 'up', 'cancel'].includes(kind);
        // A contact that went down with `mouse` (a drag's) holds the left button through its phases (review A1).
        // A button's point and the modifiers held, as the request says them.
        const at = opts.at !== undefined ? { at: opts.at } : {}, held = opts.modifiers ? { modifiers: opts.modifiers } : {};
        const r = phase ? await heldButton.ask(kind, opts, (button) => ask({ op: 'tap', phase: kind, ...(id != null ? { id } : {}), x: opts.x, y: opts.y, dx: opts.dx, dy: opts.dy, ms: opts.ms, ...(opts.virtual ? { virtual: true } : {}), ...held, ...button })) : ['contextmenu', 'dblclick', 'mouse', 'auxclick'].includes(kind) ? await ask({ op: 'tap', id, [kind]: true, ...(kind === 'dblclick' && opts.touch ? { touch: true } : {}), ...at, ...held }) : kind === 'clicks' ? await ask({ op: 'tap', id, clicks: opts.clicks, ...at, ...held }) : kind === 'pinch' ? await ask({ op: 'tap', id, pinch: opts.pinch, at: opts.at }) : kind === 'wheel' && (opts.fling || opts.hold || opts.release) ? await platformWheel(ask, id, opts, heldWheel) : kind === 'wheel' ? await ask({ op: 'tap', id, wheel: opts.wheel, ...(opts.gesture ? { gesture: true } : {}), ...at, ...held }) : ['drop', 'dragenter', 'dragleave'].includes(kind) ? await ask({ op: 'tap', id, [kind]: opts[kind], text: opts.text, url: opts.url }) : kind === 'hover' ? await ask({ op: 'tap', id, hover: true }) : kind === 'press' ? await ask({ op: 'tap', id, ...guest, ...at, ...(opts.modifiers ? { modifiers: opts.modifiers } : {}) }) : kind === 'mediasession' ? await ask({ op: 'tap', id, mediaSession: opts.mediaSession, ...(opts.seconds != null ? { seconds: opts.seconds } : {}) }) : await ask({ op: 'type', id, text: opts.text, ...(opts.clipboard ? { clipboard: opts.clipboard } : {}), ...(opts.files ? { files: opts.files } : {}), ...(opts.image ? { image: opts.image } : {}), ...guest });
        if (r.error) throw new Error(r.error);
        return r;
      },
      async screenshot(path, window = false, film = false, { own, skipMotion } = {}) {
        if (device) return phoneScreenshot({ ask: (req, optionalMs) => ask(req, state.session, optionalMs),
          udid: ph.udid, appId: a.id, container: ready.container, path, window, film, own, skipMotion });
        // A sandboxed Mac app writes only in its container (`mac --sandbox`, LLP 1030.005 D4); the driver moves the file out.
        const contained = macBuild && macSandboxed(a) ? resolve(homedir(), 'Library/Containers', a.id, 'Data/tmp', `exact-agent-${process.pid}${extname(path)}`) : null;
        const remote = droid ? droid.shot : contained ?? path;
        const r = await ask({ op: 'screenshot', path: remote, window, ...(film ? { film } : {}) });
        if (r.error) throw new Error(r.error);
        if (droid) { droid.pull(path); r.screenshot = path; }
        if (contained) { mkdirSync(dirname(resolve(path)), { recursive: true }); copyFileSync(contained, path); rmSync(contained, { force: true }); r.screenshot = path; }
        return r;
      },
      // A drive that ends with a held wheel lifts it, as a finger leaves the trackpad.
      async close() { if (heldWheel.open) await platformWheel(ask, heldWheel.open.tapped, { wheel: [0, 0], release: true }, heldWheel).catch(() => {}); await close(); },
    };
  } catch (e) {
    await close();
    if (device) throw new Error(phoneStartupFailure(e.message, hostLines), { cause: e });
    throw e;
  } finally {
    clearTimeout(readyTimeout);
  }
}
