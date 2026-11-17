// @exact/camera's and media-capture's video half's conformance suite on
// Apple (LLP 1133.001 D7, LLP 1133.008 stage 2): the fixture app made as the
// web suite makes it, built for macOS or an iOS simulator, and driven by the
// agent with both implementations bound (`--bind`), so AVFoundation answers
// and not the agent's cameras.
//
// - macOS lets no tool grant the camera: without a person the suite checks
//   what holds before a grant and names what it skips. With --ask, the open
//   shows the system's dialog for a person to answer, and the suite then
//   takes photos with the Mac's built-in camera (never a phone's through
//   Continuity Camera): the preview, the photo written, a hidden viewfinder
//   letting the camera go and reopening it, and stopTrack.
// - The iOS simulator has no camera (Apple: "test your camera app on actual
//   devices"): the suite checks that the status says so and `getUserMedia`
//   fails `NotFoundError` before anything is asked. checklist.md is the
//   iPhone's.
//
// First, on this Mac, two Swift tests compiled with a package's Apple
// sources: the capture session's notifications as the track's mute reason,
// posted by hand (`interruptions.swift`, with media-capture's), and a failed
// capture's answer to `takePhoto` (`photo-failure.swift`, with the camera's);
// and on the iOS simulator, the system camera UI's answer
// (`camera-app.swift`). The camera app (`takePhotoWithCameraApp`) is driven
// under the agent, its sheet's `take` and `cancel`, and bound on the
// simulator, which has no camera: `NotFoundError`.
//
// usage: bun packages/camera/test/apple.mjs [macos] [ios] [--sim <name|udid>] [--ask]
import { existsSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { checker, here, makeFixture, root, run } from './fixture.mjs';

const argv = process.argv.slice(2), sim = argv.includes('--sim') ? argv[argv.indexOf('--sim') + 1] : process.env.EXACT_SIM;
const hosts = argv.filter((a, i) => ['macos', 'ios'].includes(a) && argv[i - 1] !== '--sim');
const { check, failed } = checker();

{
  const out = mkdtempSync(join(tmpdir(), 'exact-camera-swift-')), packages = resolve(root, 'packages');
  run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
    resolve(root, 'host/apple/modules/ExactPackage.swift'), '-o', join(out, 'libExactPackage.a')], out);
  const tests = {
    interruptions: ['media-capture/apple/MediaCapture.swift', 'media-capture/apple/VideoCapture.swift'],
    'photo-failure': ['camera/apple/Camera.swift', 'camera/apple/Viewfinder.swift'],
  };
  for (const [test, sources] of Object.entries(tests)) {
    run('xcrun', ['swiftc', '-parse-as-library', '-swift-version', '5', '-I', out, '-L', out, '-lExactPackage', ...sources.map((f) => join(packages, f)),
      join(here, `${test}.swift`), '-o', join(out, test)], out);
    const r = spawnSync(join(out, test), [], { cwd: out, encoding: 'utf8' }), lines = r.stdout.trim().split('\n');
    for (const line of lines) {
      const [verdict, where, ...name] = line.split(' ');
      if (verdict === 'PASS' || verdict === 'FAIL') check(where, name.join(' '), verdict === 'PASS', line);
    }
    // It exits 1 after a FAIL line; any other way it stopped short.
    if (r.status !== 0 && !lines.some((l) => l.startsWith('FAIL '))) check(`apple-${test}`, 'runs to its end', false, `${r.status ?? r.signal}: ${r.stderr}`);
  }
  rmSync(out, { recursive: true, force: true });
}

// The system camera UI's answer, compiled for the iOS simulator and run there (`simctl spawn`): the simulator has no
// camera to show the UI with, so the delegate is handed what the picker would hand it.
if (hosts.length === 0 || hosts.includes('ios')) {
  const out = mkdtempSync(join(tmpdir(), 'exact-camera-ios-')), packages = resolve(root, 'packages'), target = 'arm64-apple-ios17.0-simulator';
  const swiftc = (...args) => run('xcrun', ['-sdk', 'iphonesimulator', 'swiftc', '-target', target, '-parse-as-library', ...args], out);
  swiftc('-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
    resolve(root, 'host/apple/modules/ExactPackage.swift'), '-o', join(out, 'libExactPackage.a'));
  swiftc('-swift-version', '5', '-I', out, '-L', out, '-lExactPackage', join(packages, 'camera/apple/Camera.swift'), join(packages, 'camera/apple/Viewfinder.swift'),
    join(here, 'camera-app.swift'), '-o', join(out, 'camera-app'));
  // The simulator the drive uses, booted and waited for (host/apple/devices.mjs, as build.mjs chooses it).
  const { simulator } = await import(resolve(root, 'host/apple/devices.mjs'));
  const device = simulator(sim);
  const r = spawnSync('xcrun', ['simctl', 'spawn', device.udid, join(out, 'camera-app')], { encoding: 'utf8' }), lines = r.stdout.trim().split('\n');
  for (const line of lines) {
    const [verdict, where, ...name] = line.split(' ');
    if (verdict === 'PASS' || verdict === 'FAIL') check(where, name.join(' '), verdict === 'PASS', line);
  }
  if (r.status !== 0 && !lines.some((l) => l.startsWith('FAIL '))) check('apple-camera-app', `runs to its end on ${device.name}`, false, `${r.status ?? r.signal}: ${r.stderr}`);
  rmSync(out, { recursive: true, force: true });
}

const { app } = makeFixture(), name = basename(app);
Object.assign(process.env, { EXACT_APP_DIR: app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1', EXACT_IDENTITY: process.env.EXACT_IDENTITY ?? '-' });
const { open } = await import(resolve(root, 'scripts/agent.mjs'));

/** Each testId's text in the tree. */
const texts = async (s) => {
  const { nodes } = await s.tree(), byId = new Map(nodes.map((n) => [n.id, n]));
  const text = (n) => n.props?.text ?? n.children.map((c) => text(byId.get(c))).join('');
  return Object.fromEntries(nodes.filter((n) => n.props?.testId).map((n) => [n.props.testId, text(n)]));
};
/** The text at `testId` once `want` holds, or the last seen; AVFoundation answers on its own time. */
const until = async (s, testId, want, ms = 15000) => {
  const end = Date.now() + ms;
  let got;
  while (Date.now() < end) {
    await s.clock('+250 real');
    if (want(got = (await texts(s))[testId])) return got;
  }
  return got;
};

/** Under the agent the camera app is the person's sheet: `take` writes the agent's picture, `cancel` is AbortError.
 *  macOS has no system camera UI. */
async function cameraAppUnderTheAgent(host, where) {
  const s = await open({ host, app: name, ...(host === 'ios' && sim ? { sim } : {}) });
  try {
    await s.tap('camera-app');
    if (host === 'macos') {
      const none = await until(s, 'shot-error', (t) => !!t);
      return check(where, 'takePhotoWithCameraApp is NotSupportedError on a Mac', none === 'NotSupportedError', none);
    }
    const sheet = async () => (await s.state()).pending?.find((p) => p.device?.capability === 'camera-app');
    let held = await sheet();
    check(where, "under the agent the camera app waits on the person's sheet", !!held, JSON.stringify((await s.state()).pending));
    if (held) await s.tap(`@${held.ticket}`, { choice: 'take' });
    const taken = await until(s, 'shot', (t) => t && t !== 'none');
    check(where, "take writes the agent's picture and answers a Photo", /^app:\/tmp\/camera\/1\.jpg image\/jpeg true [1-9]\d*x[1-9]\d*$/.test(taken ?? ''), taken);
    await s.tap('camera-app');
    held = await sheet();
    if (held) await s.tap(`@${held.ticket}`, { choice: 'cancel' });
    const cancelled = await until(s, 'shot-error', (t) => t === 'AbortError');
    check(where, 'cancel is AbortError', cancelled === 'AbortError', cancelled);
  } finally {
    await s.close();
  }
}

for (const host of hosts.length ? hosts : ['macos', 'ios']) {
  const where = host === 'ios' ? 'ios-simulator' : 'macos';
  run('bun', [resolve(root, 'host/apple/build.mjs'), ...(host === 'ios' ? ['--ios', ...(sim ? ['--sim', sim] : [])] : ['--bundle']), `${name}-apple`], root);
  // On macOS the drive runs the binary inside the `.app`: the camera's dialog names a bundled app.
  if (host === 'macos') {
    const bundle = readdirSync(resolve(app, 'target'), { recursive: true }).find((f) => /macos/.test(f) && f.endsWith('.app/Contents/Info.plist'));
    if (!bundle) throw new Error('no macOS .app under target/ (build.mjs --bundle)');
    const contents = resolve(app, 'target', bundle, '..'), executable = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', resolve(contents, 'Info.plist')])).CFBundleExecutable;
    process.env.EXACT_MAC_BIN = resolve(contents, 'MacOS', executable);
  }
  // The build's Info.plist (D6): the usage key, its text the purpose.
  const plistAt = readdirSync(resolve(app, 'target'), { recursive: true }).find((f) => host === 'ios' ? /iphonesimulator|ios-sim/.test(f) && f.endsWith('.app/Info.plist') : /macos/.test(f) && f.endsWith('-Info.plist'));
  const plist = plistAt ? JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', resolve(app, 'target', plistAt)])) : {};
  check(where, 'Info.plist carries NSCameraUsageDescription with the purpose', plist.NSCameraUsageDescription === 'The conformance suite takes a photo.', plistAt ?? 'no Info.plist under target/');
  await cameraAppUnderTheAgent(host, where);
  const s = await open({ host, app: name, bind: 'media-capture,camera', ...(host === 'ios' && sim ? { sim } : {}) });
  try {
    const state = await s.state();
    check(where, 'both extensions are bound, not substituted', state.extensions?.camera?.bound === true && state.extensions?.['media-capture']?.bound === true, JSON.stringify(state.extensions));
    const status = await until(s, 'status', (t) => /^(true|false) (prompt|granted|denied)$/.test(t ?? ''));
    if (host === 'ios') {
      check(where, 'the simulator has no camera: cameraStatus says so', status === 'false prompt' || status === 'false granted' || status === 'false denied', status);
      await s.tap('open');
      const none = await until(s, 'open-error', (t) => !!t);
      check(where, 'getUserMedia(video) fails NotFoundError and asks nothing', none === 'NotFoundError', none);
      await s.tap('camera-app');
      const noCameraApp = await until(s, 'shot-error', (t) => !!t);
      check(where, 'takePhotoWithCameraApp: the system camera UI has no camera here, NotFoundError', noCameraApp === 'NotFoundError', noCameraApp);
      continue;
    }
    check(where, 'cameraStatus answers from AVFoundation', /^true (prompt|granted|denied)$/.test(status ?? ''), status);
    const before = (await texts(s)).cameras;
    if (status === 'true prompt') check(where, 'before a grant enumerateDevices names no camera', before === '1', before);
    if (status === 'true prompt' && argv.includes('--ask')) {
      console.log(`${where}: answer the camera dialog (Allow), within two minutes`);
      await s.tap('open-builtin');
      const asked = await until(s, 'status', (t) => t !== 'true prompt', 120000);
      check(where, 'the request asks, and the answer reaches the state', /^true (granted|denied)$/.test(asked ?? ''), asked);
    }
    if ((await texts(s)).status !== 'true granted') {
      console.log(`SKIP ${where} capture: the camera is not granted, and only a person grants it here (--ask)`);
      continue;
    }
    if ((await texts(s)).live !== 'video live false none true') await s.tap('open-builtin');
    const live = await until(s, 'live', (t) => t === 'video live false none true');
    check(where, 'the built-in camera opens, leased by the displayed viewfinder', live === 'video live false none true', live);
    const size = await until(s, 'size', (t) => /^[1-9]\d*x[1-9]\d*$/.test(t ?? ''));
    check(where, "the track's settings are the device's", /^[1-9]\d*x[1-9]\d*$/.test(size ?? ''), size);
    await s.tap('shutter');
    const shot = await until(s, 'shot', (t) => t && t !== 'none', 20000);
    check(where, 'takePhoto writes a JPEG at app:/tmp/camera/1.jpg, sized', /^app:\/tmp\/camera\/1\.jpg image\/jpeg true [1-9]\d*x[1-9]\d*$/.test(shot ?? ''), `${shot} ${(await texts(s))['shot-error']}`);
    await s.tap('toggle');
    const idle = await until(s, 'live', (t) => t === 'video live true idle true');
    check(where, 'a hidden viewfinder leaves the track idle', idle === 'video live true idle true', idle);
    const logs = JSON.stringify(await s.logs());
    check(where, 'and the capture stops', logs.includes('capture stopped'), 'no "capture stopped" in the journal');
    await s.tap('toggle');
    const again = await until(s, 'live', (t) => t === 'video live false none true');
    check(where, 'shown again, the viewfinder takes the camera back', again === 'video live false none true', again);
    await s.tap('stop');
    const ended = await until(s, 'live', (t) => t?.startsWith('video ended'));
    check(where, 'stopTrack ends the track', ended?.startsWith('video ended'), ended);
  } finally {
    await s.close();
  }
}
if (existsSync(app)) rmSync(dirname(app), { recursive: true, force: true });
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
