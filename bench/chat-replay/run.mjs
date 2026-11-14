// One measured run of a chat app on a simulator (README.md):
//   bun bench/chat-replay/run.mjs --sim <udid> --app <bundle id> --out <result.json> [--sample N] [--dump <dir>] [--settle s]
// Launches the app (already installed) with probe/chatprobe.m inserted, waits --settle seconds (default 5), plays
// replay/replay.swift's touches through an XCTest runner and waits for the probe's result. The probe and the runner
// are built into target/chat-replay/ the first time and whenever their sources change.
import { spawnSync, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const HERE = dirname(fileURLToPath(import.meta.url)), ROOT = resolve(HERE, '../..');
const { values: a } = parseArgs({ options: { sim: { type: 'string' }, app: { type: 'string' }, out: { type: 'string' }, sample: { type: 'string', default: '2' },
  dump: { type: 'string' }, settle: { type: 'string', default: '5' } } });
if (!a.sim || !a.app || !a.out) { console.error('usage: run.mjs --sim <udid> --app <bundle id> --out <result.json> [--sample N] [--dump <dir>] [--settle s]'); process.exit(2); }
const must = (cmd, args, opts = {}) => {
  const r = spawnSync(cmd, args, { encoding: 'utf8', ...opts });
  if (r.status !== 0) throw new Error(`${cmd} ${args.slice(0, 4).join(' ')}: ${r.stderr || r.stdout || r.error}`);
  return r.stdout;
};
// An artifact's key: its source, this script (which holds the build recipe) and the selected Xcode.
const xcode = spawnSync('xcodebuild', ['-version'], { encoding: 'utf8' }).stdout ?? '';
const hash = (file) => createHash('sha256').update([readFileSync(file), readFileSync(fileURLToPath(import.meta.url)), xcode].join('\0')).digest('hex').slice(0, 12);
const xml = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const BUILD = resolve(ROOT, 'target/chat-replay');
mkdirSync(BUILD, { recursive: true });

// The probe, for the simulator.
const probeSrc = resolve(HERE, 'probe/chatprobe.m'), probe = resolve(BUILD, `chatprobe-${hash(probeSrc)}.dylib`);
if (!existsSync(probe)) {
  must('xcrun', ['-sdk', 'iphonesimulator', 'clang', '-dynamiclib', '-fobjc-arc', '-O2', '-Wall', '-target', 'arm64-apple-ios17.0-simulator',
    '-framework', 'UIKit', '-framework', 'QuartzCore', '-framework', 'CoreGraphics', '-framework', 'Foundation', probeSrc, '-o', `${probe}.tmp`]);
  renameSync(`${probe}.tmp`, probe);
}

// The runner: Xcode's XCTRunner.app with the test bundle compiled from replay.swift, signed ad hoc (as
// host/apple/touches.mjs builds its own), assembled in a stage and published by one rename once it is complete.
const developer = must('xcode-select', ['-p']).trim(), platform = resolve(developer, 'Platforms/iPhoneSimulator.platform/Developer');
const runnerSrc = resolve(HERE, 'replay/replay.swift'), runnerDir = resolve(BUILD, `runner-${hash(runnerSrc)}`), runner = resolve(runnerDir, 'ChatReplay-Runner.app');
if (!existsSync(runner)) {
  const stage = `${runnerDir}.build-${process.pid}`, app = resolve(stage, 'ChatReplay-Runner.app');
  rmSync(stage, { recursive: true, force: true });
  try {
    cpSync(resolve(platform, 'Library/Xcode/Agents/XCTRunner.app'), app, { recursive: true });
    const info = resolve(app, 'Info.plist');
    for (const [k, v] of [['CFBundleExecutable', 'XCTRunner'], ['CFBundleIdentifier', 'dev.exact.chatreplay.runner'], ['CFBundleName', 'ChatReplay-Runner']]) must('plutil', ['-replace', k, '-string', v, info]);
    const fw = resolve(app, 'Frameworks'); mkdirSync(fw, { recursive: true });
    for (const [from, names] of [[resolve(platform, 'Library/Frameworks'), /^(XCTest|XCUIAutomation|Testing|_Testing_Foundation)\.framework$/],
      [resolve(platform, 'Library/PrivateFrameworks'), /^(XCT|XCUnit)/], [resolve(platform, 'usr/lib'), /^(libXCTestSwiftSupport|lib_TestingInterop)\.dylib$/]])
      for (const n of readdirSync(from).filter((n) => names.test(n))) cpSync(resolve(from, n), resolve(fw, n), { recursive: true });
    const bundle = resolve(app, 'PlugIns/ChatReplay.xctest'); mkdirSync(bundle, { recursive: true });
    must('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', '-emit-library', '-O', '-module-name', 'ChatReplay', '-target', 'arm64-apple-ios17.0-simulator',
      '-F', resolve(platform, 'Library/Frameworks'), '-F', resolve(platform, 'Library/PrivateFrameworks'), '-I', resolve(platform, 'usr/lib'), '-L', resolve(platform, 'usr/lib'),
      '-framework', 'XCTest', '-framework', 'XCUIAutomation', '-Xlinker', '-rpath', '-Xlinker', '@executable_path/Frameworks',
      '-module-cache-path', resolve(stage, 'cache'), '-o', resolve(bundle, 'ChatReplay'), runnerSrc]);
    writeFileSync(resolve(bundle, 'Info.plist'), `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleExecutable</key><string>ChatReplay</string><key>CFBundleIdentifier</key><string>dev.exact.chatreplay.xctest</string>
<key>CFBundlePackageType</key><string>BNDL</string><key>CFBundleName</key><string>ChatReplay</string><key>CFBundleVersion</key><string>1</string>
<key>CFBundleSupportedPlatforms</key><array><string>iPhoneSimulator</string></array></dict></plist>\n`);
    must('codesign', ['--force', '--sign', '-', bundle]);
    for (const n of readdirSync(fw)) must('codesign', ['--force', '--sign', '-', resolve(fw, n)]);
    must('codesign', ['--force', '--sign', '-', '--entitlements', resolve(app, 'RunnerEntitlements.plist'), app]);
    rmSync(resolve(stage, 'cache'), { recursive: true, force: true });
    renameSync(stage, runnerDir);
  } finally { rmSync(stage, { recursive: true, force: true }); }
}
must('xcrun', ['simctl', 'install', a.sim, runner]);

const out = resolve(a.out), log = out.replace(/\.json$/, '') + '.xcodebuild.log';
mkdirSync(dirname(out), { recursive: true });
rmSync(out, { force: true });
let code = null, ok = false;
const t0 = Date.now();
try {
  spawnSync('xcrun', ['simctl', 'terminate', a.sim, a.app]);
  await Bun.sleep(1000);
  const env = { ...process.env, SIMCTL_CHILD_DYLD_INSERT_LIBRARIES: probe, SIMCTL_CHILD_CHAT_OUT: out, SIMCTL_CHILD_CHAT_SAMPLE: a.sample };
  if (a.dump) { mkdirSync(resolve(a.dump), { recursive: true }); env.SIMCTL_CHILD_CHAT_DUMP = resolve(a.dump); }
  must('xcrun', ['simctl', 'launch', a.sim, a.app], { env });
  await Bun.sleep(Number(a.settle) * 1000);

  const xctestrun = resolve(BUILD, `replay-${a.sim}.xctestrun`);
  const vars = { REPLAY_APP: a.app, ...Object.fromEntries(['REPLAY_PERIOD', 'REPLAY_FLINGS', 'REPLAY_TOP_TEXT', 'REPLAY_UNVERIFIED'].filter((k) => process.env[k]).map((k) => [k, process.env[k]])) };
  writeFileSync(xctestrun, `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>ChatReplay</key><dict><key>IsUITestBundle</key><true/><key>TestHostPath</key><string>${xml(runner)}</string>
<key>TestBundlePath</key><string>__TESTHOST__/PlugIns/ChatReplay.xctest</string><key>UITargetAppPath</key><string>${xml(runner)}</string>
<key>TestingEnvironmentVariables</key><dict>${Object.entries(vars).map(([k, v]) => `<key>${k}</key><string>${xml(v)}</string>`).join('')}</dict></dict>
<key>__xctestrun_metadata__</key><dict><key>FormatVersion</key><integer>1</integer></dict></dict></plist>\n`);
  const x = spawn('xcodebuild', ['test-without-building', '-xctestrun', xctestrun, '-derivedDataPath', resolve(BUILD, 'derived', a.sim), '-destination', `id=${a.sim}`,
    '-only-testing:ChatReplay/ChatReplay/testReplay'], { stdio: ['ignore', 'pipe', 'pipe'], detached: true });
  const chunks = []; x.stdout.on('data', (d) => chunks.push(d)); x.stderr.on('data', (d) => chunks.push(d));
  // The replay's own bound is 150 s up and about 20 s of flings; past 300 s the test is hung.
  const timer = setTimeout(() => { try { process.kill(-x.pid, 'SIGKILL'); } catch {} }, 300_000); // its whole group
  code = await new Promise((r) => { x.on('exit', (c) => r(c)); x.on('error', () => r(-1)); });
  clearTimeout(timer);
  writeFileSync(log, Buffer.concat(chunks));
  for (let i = 0; i < 100 && !existsSync(out); i++) await Bun.sleep(100);
  // A result counts only when the test passed (message 1 reached, every touch played) and every phase was seen.
  if (code === 0 && existsSync(out)) {
    const r = JSON.parse(readFileSync(out, 'utf8'));
    ok = ['launch', 'up', 'flings'].every((p) => r.phases?.some((x) => x.phase === p)) && r.topMs != null;
  }
} finally {
  spawnSync('xcrun', ['simctl', 'terminate', a.sim, a.app]);
}
const took = ((Date.now() - t0) / 1000).toFixed(1);
if (!ok) {
  if (existsSync(out)) renameSync(out, out.replace(/\.json$/, '') + '.failed.json');
  console.error(`no result (xcodebuild exited ${code} after ${took} s; ${log})`); process.exit(1);
}
console.log(`ok ${out} (${took} s)`);
