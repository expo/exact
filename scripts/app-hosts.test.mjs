import { expect, test } from 'bun:test';
import { hostOf, shippedHost } from './app-cli.mjs';
import { main } from './agent.mjs';

// The host before `test`'s files and `agent`'s ops is optional: a word is taken as the host only when it names one, so
// `agent "tap like" "state liked"` keeps both ops and `test app.test.contract` runs that file, both on the web.
test('the first word is the host only when it names one', () => {
  for (const [verb, words, host, left] of [
    ['agent', ['tap like', 'state liked'], 'web', ['tap like', 'state liked']],
    ['agent', ['ios', 'tree'], 'ios', ['tree']],
    ['test', ['app.test.contract'], 'web', ['app.test.contract']],
    ['test', ['web', 'app.test.contract'], 'web', ['app.test.contract']],
    ['verify', ['--strict'], 'web', ['--strict']],
    ['build', ['ios'], 'web', ['ios']],
  ]) {
    const rest = [...words];
    expect([hostOf(verb, rest), rest]).toEqual([host, left]);
  }
});

// A test, agent or verify that names no host runs on one the app ships (#402): app.json's `host`, `deploy.store` and
// `deploy.binaries`, as the build and deploy read them. The web is an app's only when it names the web or names nothing.
test("a drive that names no host runs on one the app's manifest names", () => {
  const native = { host: { ios: {}, macos: {} }, deploy: { store: { ios: '0', macos: '0' } } };
  for (const [manifest, os, verify, drive = verify] of [
    [{}, 'darwin', 'web'],
    [{ host: {}, deploy: {} }, 'linux', 'web'],
    [{ host: { web: {} }, deploy: { store: { web: '0' } } }, 'darwin', 'web'],
    [{ host: { ios: {}, macos: {}, web: {} } }, 'darwin', 'web'],
    [{ host: { ios: {}, linux: {} }, deploy: { store: { web: '0' } } }, 'darwin', 'web'],
    [native, 'darwin', 'ios'],
    [{ host: { android: {} } }, 'darwin', 'android'],
    [{ host: { ios: {}, android: {} } }, 'linux', 'android'],
    [{ host: { ios: {}, windows: {} } }, 'win32', 'windows'],
    [{ host: { ios: {}, windows: {} } }, 'darwin', 'ios'],
    // test and agent drive the Mac and the headless Linux host too, which verify has no leg for.
    [{ host: { macos: {} } }, 'darwin', null, 'macos'],
    [{ host: { macos: {}, linux: {} } }, 'linux', null, 'linux'],
    [{ host: { linux: {}, android: {} } }, 'darwin', 'android', 'linux'],
    // On Windows the driver has no Linux host binary to open, so a drive takes Android.
    [{ host: { linux: {}, android: {} } }, 'win32', 'android'],
    [{ host: { linux: {} } }, 'win32', null],
    [native, 'win32', null],
    [{ host: { tvos: {} } }, 'darwin', null],
  ]) {
    for (const [verb, host] of [['verify', verify], ['test', drive], ['agent', drive]]) {
      expect([verb, JSON.stringify(manifest), os, shippedHost(verb, manifest, os).host]).toEqual([verb, JSON.stringify(manifest), os, host]);
      const rest = ['--strict'];
      expect(hostOf(verb, rest, manifest, os)).toBe(host);
      expect(rest).toEqual(['--strict']);
    }
  }
  // A host named on the command line is the run's, whatever the manifest says; another verb's is the web's.
  for (const verb of ['verify', 'test', 'agent']) expect(hostOf(verb, ['web'], native, 'darwin')).toBe('web');
  expect(hostOf('test', ['app.test.contract'], native, 'darwin')).toBe('ios');
  expect(hostOf('build', [], native, 'darwin')).toBe('web');
  // With none it drives here, the line says why, and what follows for the verb.
  for (const [verb, manifest, os, why] of [['verify', native, 'win32', /names ios, macos and not the web.*ios: its simulator needs macOS; macos: verify has no leg for it.*so only the semantics check runs$/],
    ['verify', { host: { macos: {} } }, 'darwin', /names macos and not the web/], ['verify', { host: { windows: {} } }, 'linux', /windows: it runs on Windows/],
    ['test', native, 'win32', /ios: its simulator needs macOS; macos: it needs macOS\), so test runs nowhere unless named \(`test web` runs it on the web anyway\)$/],
    ['agent', { host: { tvos: {} } }, 'darwin', /tvos: agent has no host for it/],
    ['test', { host: { linux: {} } }, 'win32', /linux: its headless host is driven on macOS and Linux/]]) {
    const chosen = shippedHost(verb, manifest, os);
    expect(chosen.host).toBeNull();
    expect(chosen.why).toMatch(why);
    expect(hostOf(verb, [], manifest, os)).toBeNull();
  }
});

// End to end: a test or agent with no host the app ships here refuses before it builds anything, and says why.
test('a drive with no host the app ships here refuses and says why', async () => {
  const { createApp } = await import('../game/new.mjs');
  const root = mkdtempSync(resolve(tmpdir(), 'exact-drive-host-')), dir = resolve(root, 'tv-only');
  try {
    createApp(dir);
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ ...manifest, host: { tvos: {} }, deploy: {} }, null, 2));
    for (const verb of ['test', 'agent']) {
      const r = spawnSync(process.execPath, [resolve(dir, 'exact.mjs'), verb, ...(verb === 'agent' ? ['tree'] : [])], { cwd: dir, encoding: 'utf8', timeout: 60_000, env: { ...process.env, EXACT_FEEDBACK: '0' } });
      expect([verb, r.status]).toEqual([verb, 2]);
      expect(r.stderr).toMatch(new RegExp(`^${verb}: app\\.json names tvos and not the web, .*\\(\`${verb} web\` runs it on the web anyway\\)$`, 'm'));
      expect(existsSync(resolve(dir, 'dist'))).toBe(false);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 120_000);

// End to end: an app that ships only the Mac app runs the semantics check alone, and builds and drives no web.
test('verify of an app that ships no host verify drives builds nothing and says so', () => {
  const root = mkdtempSync(resolve(tmpdir(), 'exact-verify-host-')), dir = resolve(root, 'mac-only');
  mkdirSync(dir);
  writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n');
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Mac Only', app: { id: 'com.example.maconly', name: 'Mac Only' }, host: { macos: {} }, deploy: { store: { macos: '0' } } }));
  const preload = resolve(root, 'stub.mjs'), scripts = import.meta.dir, at = (p) => JSON.stringify(resolve(scripts, p));
  // The semantics check answers at once; a build or a drive would be recorded as a failure.
  writeFileSync(preload, `
    import { mock } from 'bun:test';
    mock.module(${at('verify-semantics.mjs')}, () => ({ contractVerify: () => ({ stop() {}, done: Promise.resolve({ exit: 0, output: '', errors: '' }) }), semanticsSummary: () => ['contract verify: stubbed'] }));
    mock.module(${at('agent.mjs')}, () => ({ open: async () => { throw new Error('drove a host'); } }));
  `);
  try {
    const run = (...args) => spawnSync(process.execPath, ['--preload', preload, resolve(scripts, 'verify.mjs'), '--app', 'mac-only', ...args],
      { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8', timeout: 30_000 });
    const json = run('--json');
    expect(json.status).toBe(0);
    const out = JSON.parse(json.stdout.trim().split('\n').at(-1));
    expect([out.host, out.exit, out.built]).toEqual([null, 0, undefined]);
    expect(out.skipped).toMatch(/names macos and not the web/);
    const said = run();
    expect(said.status).toBe(0);
    expect(said.stderr).not.toMatch(/verify web/);
    expect(said.stdout).toMatch(/^verify: app\.json names macos and not the web, .*so only the semantics check runs; contract verify: stubbed$/m);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 60_000);

// The driver carries a drive only on a host it knows; any other word fell to the web and was recorded as that host's.
test('the driver refuses a host it does not carry', async () => {
  const said = [];
  const error = console.error;
  console.error = (line) => said.push(String(line));
  try {
    for (const host of ['iOS', 'tvos', 'lniux']) expect(await main([host, 'tree'])).toBe(2);
  } finally { console.error = error; }
  expect(said.every((line) => line.startsWith('agent: no host'))).toBe(true);
});

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, resolve } from 'node:path';
import { iosTarget } from '../host/apple/build.mjs';
import { HOST_SDKS, olderVersion, xcodeRefusal } from '../host/apple/devices.mjs';

/** A fake `xcrun` whose `--show-sdk-version` answers for each SDK from `SDK_<name>`, and for an SDK without one fails
 *  in xcrun's words (Xcode 27.2 for an SDK it has not), and which does `rest` (shell) for any other command. */
const fakeXcrun = (rest) => `#!/bin/sh\ncase "$*" in *--show-sdk-version*)\n  sdk=$(echo "$*" | sed -n 's/.*--sdk \\([a-z]*\\).*/\\1/p'); eval "v=\\\${SDK_$sdk:-}"\n  [ -n "$v" ] && { echo "$v"; exit 0; }; echo "xcrun: error: SDK \\"$sdk\\" cannot be located" >&2; exit 1;;\nesac\n${rest}\n`;

test('the Apple build names the Xcode it found and the one the Swift host needs', () => {
  assert.equal(olderVersion('26.4', '27.1'), true);
  assert.equal(olderVersion('27.0.1', '27.1'), true);
  assert.equal(olderVersion('27.1', '27.1'), false);
  assert.equal(olderVersion('27.1.1', '27.1'), false);
  assert.equal(olderVersion('27.10', '27.2'), false, 'parts compare as numbers');
  assert.equal(olderVersion('28', '27.1'), false);
  // What a command answers: an SDK version, or (`failed`) xcrun's own error.
  const xcode = (sdk, failed = null) => (cmd) => cmd === 'xcrun' ? (failed ? { status: 1, stdout: '', stderr: failed } : { status: 0, stdout: `${sdk}\n` })
    : { status: 0, stdout: `Xcode ${sdk || '27.1'}\nBuild version 17E5\n` };
  const developer = process.env.DEVELOPER_DIR;
  process.env.DEVELOPER_DIR = '/Applications/Xcode_26.4.app/Contents/Developer';
  try {
    assert.equal(xcodeRefusal('iphoneos', xcode('27.1')), null);
    assert.equal(xcodeRefusal('iphonesimulator', xcode('27.1')), null);
    // Each floor is the newest API the host names as its SDK declares it: an `@available` on the host's own code is no floor.
    assert.equal(xcodeRefusal('macosx', xcode('26.0')), null, "NSGlassEffectView and WKWebView's obscuredContentInsets are macOS 26");
    for (const sdk of ['15.4', '25.0']) assert.match(xcodeRefusal('macosx', xcode(sdk)), /the Swift host needs the macOS 26\.0 SDK/);
    assert.equal(xcodeRefusal('appletvos', xcode('27.0')), null, "AVAudioSession's deactivation and resumption contexts are tvOS 27.0");
    assert.match(xcodeRefusal('appletvsimulator', xcode('26.4')), /the Swift host needs the tvOS Simulator 27\.0 SDK/);
    const old = xcodeRefusal('iphoneos', xcode('27.0'));
    assert.match(old, /the selected Xcode \(Xcode 27\.0, \/Applications\/Xcode_26\.4\.app\/Contents\/Developer\) has the iOS 27\.0 SDK; the Swift host needs the iOS 27\.1 SDK, which Xcode 27\.1 and later carry/);
    assert.match(old, /DEVELOPER_DIR=.*xcode-select -s/);
    // A probe that fails is said in the tool's own words, and is not called an old Xcode.
    const unread = xcodeRefusal('appletvos', xcode('', 'xcrun: error: SDK "appletvos" cannot be located\nxcrun: error: unable to lookup item \'SDKVersion\' in SDK \'appletvos\'\n'));
    assert.match(unread, /\(Xcode 27\.1, .*\) could not report its tvOS SDK: xcrun: error: SDK "appletvos" cannot be located; xcrun: error: unable to lookup item 'SDKVersion' in SDK 'appletvos'/);
    assert.doesNotMatch(unread, /which Xcode .* carry|Select one/);
  } finally { if (developer === undefined) delete process.env.DEVELOPER_DIR; else process.env.DEVELOPER_DIR = developer; }
});

test.skipIf(process.platform !== 'darwin')('the Swift host\'s floor holds only builds that compile it, at the SDK they compile with', () => {
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-xcode-floor-'));
  try {
    const bin = resolve(scratch, 'bin'), app = resolve(scratch, 'floor'), log = resolve(scratch, 'log');
    mkdirSync(bin); mkdirSync(app);
    writeFileSync(resolve(bin, 'rustup'), `#!/bin/sh\nprintf '${[iosTarget, 'aarch64-apple-ios', 'aarch64-apple-tvos', 'aarch64-apple-tvos-sim'].join('\\n')}\\n'\n`, { mode: 0o755 });
    // Every compiler records that it was started and compiles nothing.
    writeFileSync(resolve(bin, 'xcrun'), fakeXcrun(`echo "xcrun $*" >> '${log}'\nexit 9`), { mode: 0o755 });
    writeFileSync(resolve(bin, 'xcodebuild'), `#!/bin/sh\necho "Xcode old"\n`, { mode: 0o755 });
    writeFileSync(resolve(bin, 'xcode-select'), `#!/bin/sh\necho /Applications/Xcode-old.app/Contents/Developer\n`, { mode: 0o755 });
    for (const tool of ['cargo', 'rustc', 'swift', 'swiftc']) writeFileSync(resolve(bin, tool), `#!/bin/sh\necho "${tool} $*" >> '${log}'\nexit 9\n`, { mode: 0o755 });
    writeFileSync(resolve(app, 'app.contract'), 'component A\n');
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Floor', id: 'com.example.floor', app: { id: 'com.example.floor', name: 'Floor' } }));
    const build = (sdks, ...args) => {
      rmSync(log, { force: true });
      const result = spawnSync(process.execPath, ['host/apple/build.mjs', ...args, app], { encoding: 'utf8',
        env: { ...process.env, ...Object.fromEntries(Object.entries(sdks).map(([sdk, v]) => [`SDK_${sdk}`, v])), DEVELOPER_DIR: '', EXACT_APP_DIR: '', RUSTUP_TOOLCHAIN: 'stable', PATH: bin + delimiter + process.env.PATH } });
      return { ...result, compiled: existsSync(log) ? readFileSync(log, 'utf8') : '' };
    };
    const iphone = build({ iphoneos: '27.0', iphonesimulator: '27.0' }, '--ios');
    assert.equal(iphone.status, 1);
    assert.match(iphone.stderr, /has the iOS Simulator 27\.0 SDK; the Swift host needs the iOS Simulator 27\.1 SDK/);
    assert.equal(iphone.compiled, '', 'refused before anything compiles');
    assert.match(build({ iphoneos: '26.4', iphonesimulator: '27.1' }, '--device').stderr, /has the iOS 26\.4 SDK; the Swift host needs the iOS 27\.1 SDK/);
    assert.match(build({ appletvos: '26.4', appletvsimulator: '27.0' }, '--tvos', '--device').stderr, /has the tvOS 26\.4 SDK/);
    assert.match(build({ macosx: '25.0' }).stderr, /the Swift host needs the macOS 26\.0 SDK/);
    const unread = build({ iphonesimulator: '27.1' }, '--device');
    assert.match(unread.stderr, /could not report its iOS SDK: xcrun: error: SDK "iphoneos" cannot be located/);
    assert.doesNotMatch(unread.stderr, /Swift host needs/);
    for (const [sdks, args, why] of [[{ iphonesimulator: '27.1' }, ['--ios'], 'a simulator build compiles with the simulator SDK; the device SDK is absent'],
      [{ appletvos: '26.4', appletvsimulator: '27.0' }, ['--tvos'], 'the tvOS simulator host needs 27.0'], [{ macosx: '26.0' }, [], 'the Mac host needs 26.0'],
      [{ macosx: '25.0' }, ['--painted-benchmark'], 'the painted carrier compiles no Swift'], [{ iphonesimulator: '27.0' }, ['--ios', '--embed'], '--embed alone compiles no Swift']]) {
      const result = build(sdks, ...args);
      assert.doesNotMatch(result.stderr, /the Swift host needs|could not report/, why);
      assert.notEqual(result.compiled, '', `${why}: it went on to build`);
    }
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 60000);

test.skipIf(process.platform !== 'darwin')('Mac setup reports a failed Xcode version probe without probing its SDK', async () => {
  const { sdkReport } = await import('./exact.mjs');
  const bin = mkdtempSync(resolve(tmpdir(), 'exact-xcode-error-'));
  try {
    const log = resolve(bin, 'sdk-called');
    const message = 'Xcode license has not been accepted. Run sudo xcodebuild -license.';
    writeFileSync(resolve(bin, 'xcodebuild'), `#!/bin/sh\necho '${message}' >&2\nexit 69\n`, { mode: 0o755 });
    writeFileSync(resolve(bin, 'xcrun'), `#!/bin/sh\ntouch '${log}'\nexit 1\n`, { mode: 0o755 });
    const row = sdkReport({ ...process.env, DEVELOPER_DIR: '/Applications/Xcode.app/Contents/Developer',
      PATH: bin + delimiter + process.env.PATH }, { only: 'macos' }).find(row => row.name === 'Xcode');
    assert.equal(row.ok, false);
    assert.equal(row.fix, message);
    assert.equal(existsSync(log), false);
  } finally { rmSync(bin, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('a first build asks setup for the Xcode the build will use, at its destination\'s SDK', async () => {
  const { platformReport, sdkReport } = await import('./exact.mjs');
  const bin = mkdtempSync(resolve(tmpdir(), 'exact-xcode-row-'));
  try {
    writeFileSync(resolve(bin, 'xcrun'), fakeXcrun('exec /usr/bin/xcrun "$@"'), { mode: 0o755 });
    // xcodebuild answers for an Xcode, and refuses the Command Line Tools in its own words.
    writeFileSync(resolve(bin, 'xcodebuild'), `#!/bin/sh\ncase "$DEVELOPER_DIR" in *CommandLineTools*) echo "xcode-select: error: tool 'xcodebuild' requires Xcode, but active developer directory '$DEVELOPER_DIR' is a command line tools instance" >&2; exit 1;; esac\necho "Xcode 27.1"\n`, { mode: 0o755 });
    writeFileSync(resolve(bin, 'xcode-select'), `#!/bin/sh\necho /Library/Developer/CommandLineTools\n`, { mode: 0o755 });
    // xcode-select names the Command Line Tools; DEVELOPER_DIR, which the build honours, names an Xcode.
    const env = { ...process.env, DEVELOPER_DIR: '/Applications/Xcode-27.1.app/Contents/Developer', SDK_iphonesimulator: '27.1', SDK_appletvsimulator: '27.0', SDK_appletvos: '26.4',
      PATH: bin + delimiter + process.env.PATH };
    const whole = sdkReport({ ...env, SDK_macosx: '26.0', SDK_iphonesimulator: '26.0' });
    assert.equal(whole.find(row => row.name === 'Xcode').ok, true, 'the general Xcode row does not blame a Mac-capable Xcode for the iOS floor');
    assert.equal(whole.find(row => row.name === 'iOS SDK').ok, false);
    // What app-cli.mjs reads before a native verb's first build (`ios --tvos` is the ios verb).
    const xcode = async (options) => (await platformReport('ios', sdkReport(env, { only: 'ios', ...options }), options)).find(row => row.name === 'Xcode');
    assert.equal((await xcode({})).ok, true, 'a simulator build, with no device SDK, by the Xcode DEVELOPER_DIR names');
    assert.equal((await xcode({ tv: true })).ok, true, 'an Apple TV simulator build needs the tvOS Simulator 27.0 SDK');
    const phone = await xcode({ device: true });
    assert.equal(phone.ok, false, 'a device build needs the iphoneos SDK');
    assert.equal(phone.want, 'Xcode 27.1 or later');
    assert.equal((await xcode({ tv: true, device: true })).ok, false);
    // Apple's tools take an Xcode's bundle path, with or without a trailing slash, as well as its Contents/Developer.
    for (const DEVELOPER_DIR of ['/Applications/Xcode-27.1.app', '/Applications/Xcode-27.1.app/', '/Applications/Xcode-27.1.app/Contents/Developer/']) {
      const row = (await platformReport('ios', sdkReport({ ...env, DEVELOPER_DIR }, { only: 'ios' }), {})).find(row => row.name === 'Xcode');
      assert.equal(row.ok, true, DEVELOPER_DIR);
    }
    const tools = (await platformReport('ios', sdkReport({ ...env, DEVELOPER_DIR: '/Library/Developer/CommandLineTools' }, { only: 'ios' }), {})).find(row => row.name === 'Xcode');
    assert.equal(tools.ok, false, 'the Command Line Tools are no Xcode');
    const missing = (await platformReport('ios', sdkReport({ ...env, SDK_appletvos: '' }, { only: 'ios', device: true, tv: true }), { device: true })).find(row => row.name === 'Xcode');
    assert.match(missing.fix, /could not report its tvOS SDK: xcrun: error: SDK "appletvos" cannot be located/, 'a failed probe is said in its own words');
  } finally { rmSync(bin, { recursive: true, force: true }); }
}, 120000);

test.skipIf(process.platform !== 'darwin')('a first build through the app CLI holds only what compiles the Swift host to its floor', async () => {
  const { createApp } = await import('../game/new.mjs');
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-xcode-cli-'));
  try {
    const bin = resolve(scratch, 'bin'), app = resolve(scratch, 'first-build');
    mkdirSync(bin);
    createApp(app);
    writeFileSync(resolve(bin, 'xcrun'), fakeXcrun('exec /usr/bin/xcrun "$@"'), { mode: 0o755 });
    writeFileSync(resolve(bin, 'xcodebuild'), `#!/bin/sh\necho "Xcode 25.0"\n`, { mode: 0o755 });
    writeFileSync(resolve(bin, 'xcode-select'), `#!/bin/sh\necho /Applications/Xcode-25.0.app/Contents/Developer\n`, { mode: 0o755 });
    // The checks the report runs are real; what would compile stops.
    writeFileSync(resolve(bin, 'cargo'), `#!/bin/sh\ncase "$1" in build|rustc|check) echo "compile stopped: cargo $1" >&2; exit 9;; esac\nexec '${Bun.which('cargo')}' "$@"\n`, { mode: 0o755 });
    writeFileSync(resolve(bin, 'swift'), `#!/bin/sh\necho "compile stopped: swift" >&2\nexit 9\n`, { mode: 0o755 });
    const env = { ...process.env, DEVELOPER_DIR: '', EXACT_APP_DIR: '', EXACT_FEEDBACK: '0', SDK_iphoneos: '25.0', SDK_iphonesimulator: '25.0', SDK_macosx: '25.0', PATH: bin + delimiter + process.env.PATH };
    const verb = (...args) => spawnSync(process.execPath, [resolve(app, 'exact.mjs'), ...args], { cwd: app, env, encoding: 'utf8' });
    const manifest = () => JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8'));
    const refused = verb('ios', '--no-install');
    assert.match(refused.stderr, /lacks what a ios build needs; nothing was written:\n  Xcode: Xcode 25\.0/);
    assert.equal(manifest().host?.ios, undefined);
    for (const [args, platform] of [[['ios', '--embed'], 'ios'], [['mac', '--painted-benchmark'], 'macos']]) {
      const result = verb(...args);
      assert.doesNotMatch(result.stderr, /lacks what|the Swift host needs/, args.join(' '));
      assert.ok(manifest().host?.[platform], `${args.join(' ')} named ${platform}: ${result.stderr}`);
    }
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 300000);

test('setup accepts the Bun range the builds accept', async () => {
  const { sdkReport } = await import('./exact.mjs');
  const descriptor = Object.getOwnPropertyDescriptor(process.versions, 'bun');
  try {
    for (const [version, ok] of [['1.4.2', true], ['1.5.0', true], ['1.4.1', false]]) {
      Object.defineProperty(process.versions, 'bun', { ...descriptor, value: version });
      assert.equal(sdkReport({ PATH: '' }, { only: 'web' }).find(row => row.name === 'Bun').ok, ok, version);
    }
  } finally { Object.defineProperty(process.versions, 'bun', descriptor); }
});

test('a CI setup check names only the requested build toolchain and fails when it is missing', () => {
  const result = spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), 'setup', '--check', '--platform', 'android'], {
    encoding: 'utf8', env: { HOME: process.env.HOME, PATH: '', BUN: process.execPath,
      ANDROID_HOME: '/absent-exact-ci-sdk', JAVA_HOME: '/absent-exact-ci-jdk', CARGO_HOME: '/absent-exact-ci-cargo' },
  });
  assert.equal(result.status, 1);
  assert.match(result.stdout, /Android JDK: missing/);
  assert.match(result.stdout, /Android NDK: missing/);
  assert.doesNotMatch(result.stdout, /Chrome|web nightly|wasm-bindgen|Xcode/);
});

// Homebrew's `rust` puts a cargo that refuses `+<pin>` ahead of rustup's; a platform named before, no setup row reads it.
test.skipIf(process.platform === 'win32')("a native build that fails under a cargo that is not rustup's names the directory to put first", async () => {
  const { createApp } = await import('../game/new.mjs');
  const { namePlatform } = await import('./app-shells.mjs');
  const pin = Bun.TOML.parse(readFileSync(resolve(import.meta.dir, '../rust-toolchain.toml'), 'utf8')).toolchain.channel;
  const scratch = mkdtempSync(resolve(tmpdir(), 'exact-brew-cargo-'));
  try {
    const app = resolve(scratch, 'brewed'), brew = resolve(scratch, 'brew'), proxy = resolve(scratch, 'home/bin');
    createApp(app);
    namePlatform(app, 'linux');
    const tool = (bin, name, body) => { mkdirSync(bin, { recursive: true }); writeFileSync(resolve(bin, name), `#!/bin/sh\n${body}\n`, { mode: 0o755 }); };
    tool(brew, 'cargo', `echo "error: no such command: \\\`+${pin}\\\`" >&2; exit 101`);
    for (const name of ['cargo', 'rustup']) tool(proxy, name, 'exit 0');
    const env = { ...process.env, EXACT_APP_DIR: '', EXACT_FEEDBACK: '0', CARGO_HOME: resolve(scratch, 'home'), PATH: [brew, proxy, process.env.PATH].join(delimiter) };
    const built = spawnSync(process.execPath, [resolve(app, 'exact.mjs'), 'linux'], { cwd: app, env, encoding: 'utf8' });
    assert.notEqual(built.status, 0);
    assert.ok(built.stderr.includes(`exact: ${brew}/cargo is not rustup's cargo and refuses +${pin}: put ${proxy} before ${brew} on PATH`), built.stderr);
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}, 120000);

// `test` runs each file in turn and goes on past a failing one, but a signal passed to a file's drive ends the run:
// no later file starts, and the exit code is the signal's.
test('a signal during one test file stops the run; a failing file does not', async () => {
  const root = mkdtempSync(resolve(tmpdir(), 'exact-test-signal-')), sdk = resolve(root, 'sdk'), dir = resolve(root, 'app');
  mkdirSync(resolve(sdk, 'scripts'), { recursive: true });
  mkdirSync(dir);
  // The drive says which file it started; `wait.test.contract` waits to be stopped, `fail.test.contract` fails.
  writeFileSync(resolve(sdk, 'scripts/agent.mjs'), `const file = process.argv[process.argv.indexOf('--test') + 1];
    console.log('STARTED ' + file.split('/').at(-1));
    if (file.endsWith('wait.test.contract')) setInterval(() => {}, 1000); else process.exit(file.endsWith('fail.test.contract') ? 1 : 0);`);
  writeFileSync(resolve(sdk, 'scripts/verify.mjs'), 'export const appServer = () => null;\nexport const withAppServer = (dir, fn) => fn(null);\n');
  const driver = resolve(root, 'driver.mjs');
  writeFileSync(driver, `import { run } from ${JSON.stringify(resolve(import.meta.dir, 'app-cli.mjs'))};\nprocess.exit(await run({ dir: ${JSON.stringify(dir)}, name: 'app', exact2: ${JSON.stringify(sdk)}, args: process.argv.slice(2) }));\n`);
  const files = (...names) => names.map((name) => resolve(dir, `${name}.test.contract`));
  const env = { ...process.env, EXACT_FEEDBACK: '0' };
  try {
    for (const [signal, code] of [['SIGINT', 130], ['SIGTERM', 143]]) {
      const child = Bun.spawn([process.execPath, driver, 'test', ...files('wait', 'pass'), '--web-dist', dir], { cwd: dir, env, stdout: 'pipe', stderr: 'ignore' });
      const reader = child.stdout.pipeThrough(new TextDecoderStream()).getReader();
      let said = '';
      while (!said.includes('STARTED wait')) {
        const { value, done } = await reader.read();
        if (done) break;
        said += value;
      }
      child.kill(signal);
      for (let read; !(read = await reader.read()).done;) said += read.value;
      expect([signal, await child.exited, said.includes('STARTED pass')]).toEqual([signal, code, false]);
    }
    const failed = spawnSync(process.execPath, [driver, 'test', ...files('fail', 'pass'), '--web-dist', dir], { cwd: dir, encoding: 'utf8', env });
    expect([failed.status, failed.stdout.includes('STARTED pass')]).toEqual([1, true]);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 30_000);
