// Interactive TTY platform launcher and single-key controller for the dev loop (LLP 1189 D1).
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { appleExecutableName } from '../../scripts/app.mjs';
import { stopBuildTree } from './dev-startup.mjs';
import { canPromptInteractive, resolveHumanSimulator } from '../apple/sim-pick.mjs';
import { parseLaunchedPid, streamSimulatorLogs } from '../apple/sim-logs.mjs';

/** Parse the dev launcher's own flags using spellings `scripts/help-parity.mjs` scans. */
export function parseDevLaunchFlags(argv) {
  const value = (flag) => { const i = argv.indexOf(flag); return i >= 0 ? argv[i + 1] : null; };
  return {
    ios: argv.includes('--ios'),
    mac: argv.includes('--mac'),
    android: argv.includes('--android'),
    open: argv.includes('--open'),
    sim: value('--sim'),
    noInteractive: argv.includes('--no-interactive'),
  };
}

/** Atomic single-launch mutex shared between keyboard shortcuts and `host/web/local-install.mjs`. */
export function createLaunchGate() {
  let active = null;
  return {
    get active() { return active; },
    tryAcquire(label) {
      if (active) return false;
      active = label;
      return true;
    },
    release(label) {
      if (!label || active === label) active = null;
    },
  };
}

/** Open `url` in the system default browser. */
export function openBrowserURL(url, { platform = process.platform, spawnTool = spawn } = {}) {
  const [cmd, args] = platform === 'darwin' ? ['open', [url]]
    : platform === 'win32' ? ['cmd', ['/c', 'start', '', url]]
      : ['xdg-open', [url]];
  const child = spawnTool(cmd, args, { stdio: 'ignore', detached: true });
  child.unref?.();
  return child;
}

/** Legacy line-mode stdin listener for non-TTY wasm dev loops (`r`, `t`, `f` + Enter). */
export function attachLineKeys({ stdin = process.stdin, onReload, onTypescript, onFresh } = {}) {
  if (!stdin || typeof stdin.on !== 'function') return () => {};
  stdin.setEncoding('utf8');
  const onData = (input) => {
    for (const command of String(input).trim().split(/\s+/)) {
      if (command === 'f') onFresh?.();
      if (command === 't') onTypescript?.();
      if (command === 'r') onReload?.();
    }
  };
  stdin.on('data', onData);
  return () => stdin.off('data', onData);
}

/** Build command + args to launch a native platform from `dev`.
 * External `exact new` apps route through `<app>/exact.mjs` so `bringPlatform` and `before_build` run. */
export function platformLaunchCommand(platform, { app, root, url, simUdid = null }) {
  const ownExact = app.outside && existsSync(resolve(app.dir, 'exact.mjs'));
  if (platform === 'ios') {
    const extra = ['--run', '--url', url, ...(simUdid ? ['--sim', simUdid] : [])];
    return ownExact
      ? { command: process.execPath, args: [resolve(app.dir, 'exact.mjs'), 'ios', ...extra], cwd: app.dir }
      : { command: process.execPath, args: [resolve(root, 'host/apple/build.mjs'), '--ios', app.crate('apple'), ...extra], cwd: root };
  }
  if (platform === 'mac') {
    const extra = ['--run', '--url', url];
    return ownExact
      ? { command: process.execPath, args: [resolve(app.dir, 'exact.mjs'), 'mac', ...extra], cwd: app.dir }
      : { command: process.execPath, args: [resolve(root, 'host/apple/build.mjs'), app.crate('apple'), ...extra], cwd: root };
  }
  if (platform === 'android') {
    // As on Apple, the APK follows the loop (agent-android.mjs reverses its loopback port onto the device).
    const extra = ['--run', '--url', url];
    return ownExact
      ? { command: process.execPath, args: [resolve(app.dir, 'exact.mjs'), 'android', ...extra], cwd: app.dir }
      : { command: process.execPath, args: [resolve(root, 'scripts/agent-android.mjs'), 'build', app.name, '--apk', '--install', ...extra], cwd: root };
  }
  throw new Error(`unknown platform ${platform}`);
}

export function keybarText({ wasm = false, platform = process.platform } = {}) {
  const parts = [
    ...(platform === 'darwin' ? ['i iOS', 'I pick sim', 'm macOS'] : []),
    'a Android',
    'w web',
    'r reload',
    ...(wasm ? ['t TS', 'f fresh'] : []),
    ...(platform === 'darwin' ? ['l logs'] : []),
    '? keys',
    'q quit',
  ];
  return `  keys: ${parts.join(' · ')}`;
}

/** Attach interactive TTY single-key controls (or non-TTY wasm line controls) and run any startup launch flags. */
export function attachDevControls({
  app,
  root,
  url,
  wasm = false,
  flags = {},
  launchGate = createLaunchGate(),
  onReload = () => {},
  onTypescript = () => {},
  onFresh = () => {},
  onStop = () => {},
  stdin = process.stdin,
  stdout = process.stdout,
  platform = process.platform,
  env = process.env,
  spawnTool = spawn,
  resolveSim = resolveHumanSimulator,
  openBrowser = openBrowserURL,
  streamLogs = streamSimulatorLogs,
  log = console.log,
} = {}) {
  const argv = flags.noInteractive ? ['--no-interactive'] : [];
  const interactive = canPromptInteractive(argv, { env, stdin, stdout });
  let closed = false;
  let paused = false;
  let rawEnabled = false;
  let activeChild = null;
  let macChild = null;
  let logStream = null;
  let lastSimulator = null; // { udid, name, pid }
  let detachLine = null;

  const setRaw = (on) => {
    if (!interactive || !stdin?.isTTY || typeof stdin.setRawMode !== 'function') return;
    if (rawEnabled === on) return;
    try { stdin.setRawMode(on); rawEnabled = on; } catch {}
  };

  const stopLogs = () => {
    if (!logStream) return;
    logStream.stop();
    logStream = null;
  };

  const startLogs = () => {
    if (!lastSimulator) {
      log('logs: launch iOS first (press i) so there is a simulator to stream from');
      return;
    }
    stopLogs();
    const executable = appleExecutableName(app, true);
    log(`logs: streaming from ${lastSimulator.name}${lastSimulator.pid ? ` (pid ${lastSimulator.pid})` : ''} (press l again to stop)`);
    logStream = streamLogs(lastSimulator.udid, { pid: lastSimulator.pid, executable });
    logStream.child.once('exit', () => { logStream = null; });
  };

  const toggleLogs = () => {
    if (logStream) { stopLogs(); log('logs: stopped'); return; }
    startLogs();
  };

  const runPlatform = async (kind, { forcePick = false } = {}) => {
    if (closed) return;
    if ((kind === 'ios' || kind === 'mac') && platform !== 'darwin') {
      log(`${kind}: requires macOS`);
      return;
    }
    if (kind === 'mac' && macChild && macChild.exitCode === null && macChild.signalCode === null) {
      log('macOS: app is already running (quit it to relaunch)');
      return;
    }
    if (kind !== 'mac' && !launchGate.tryAcquire(kind)) {
      log(`${kind}: wait for the active ${launchGate.active} launch to finish`);
      return;
    }
    let sim = null;
    try {
      if (kind === 'ios') {
        paused = true;
        setRaw(false);
        try {
          sim = await resolveSim(forcePick ? undefined : (flags.sim ?? process.env.EXACT_SIM), {
            app: app.id,
            dir: app.dir,
            forcePick,
            interactive,
          });
        } finally {
          paused = false;
          if (!closed) setRaw(true);
        }
        if (!sim) { log('ios: cancelled'); launchGate.release(kind); return; }
        log(`ios: launching ${app.displayName ?? app.name} on ${sim.name} (${url})`);
      } else {
        log(`${kind}: launching ${app.displayName ?? app.name}${kind === 'mac' ? ` (${url})` : ''}`);
      }
      const spec = platformLaunchCommand(kind, { app, root, url, simUdid: sim?.udid ?? null });
      const child = spawnTool(spec.command, spec.args, {
        cwd: spec.cwd,
        env: { ...env, EXACT_APP_DIR: app.dir, EXACT_NON_INTERACTIVE: '1' },
        stdio: kind === 'ios' ? ['ignore', 'pipe', 'inherit'] : ['ignore', 'inherit', 'inherit'],
        detached: true,
      });
      if (kind === 'mac') macChild = child; else activeChild = child;
      let launchedPid = null;
      if (kind === 'ios') {
        child.stdout?.on('data', (chunk) => {
          stdout.write(chunk);
          const parsed = parseLaunchedPid(chunk.toString('utf8'));
          if (parsed) launchedPid = parsed;
        });
      }
      await new Promise((done) => {
        child.once('error', (error) => { log(`${kind}: could not start (${error.message})`); done(); });
        child.once('exit', (code) => {
          if (kind === 'mac') { if (macChild === child) macChild = null; }
          else if (activeChild === child) activeChild = null;
          if (kind === 'ios' && code === 0 && sim) {
            const wasStreaming = Boolean(logStream);
            lastSimulator = { udid: sim.udid, name: sim.name, pid: launchedPid };
            if (wasStreaming) startLogs();
          } else if (code && !closed) {
            log(`${kind}: exited with code ${code}`);
          }
          done();
        });
      });
    } catch (error) {
      log(`${kind}: ${error.message}`);
    } finally {
      if (kind !== 'mac') launchGate.release(kind);
    }
  };

  const handleKey = (key) => {
    if (closed || paused) return;
    if (key === '\x03' || key === 'q') {
      setRaw(false);
      void onStop();
      return;
    }
    if (key === 'i') { void runPlatform('ios'); return; }
    if (key === 'I') { void runPlatform('ios', { forcePick: true }); return; }
    if (key === 'm') { void runPlatform('mac'); return; }
    if (key === 'a') { void runPlatform('android'); return; }
    if (key === 'w') { log(`web: opening ${url}`); openBrowser(url); return; }
    if (key === 'r') { log('reload: rebuilding…'); onReload(); return; }
    if (wasm && key === 't') { onTypescript(); return; }
    if (wasm && key === 'f') { onFresh(); return; }
    if (key === 'l') { if (platform === 'darwin') toggleLogs(); else log('logs: iOS Simulator requires macOS'); return; }
    if (key === '?') { log(keybarText({ wasm, platform })); }
  };

  const onData = (chunk) => {
    const text = String(chunk);
    if (text.startsWith('\x1b')) return; // ignore stray escape/arrow sequences
    for (const ch of text) handleKey(ch);
  };

  const onExit = () => setRaw(false);

  if (interactive) {
    log(keybarText({ wasm, platform }));
    setRaw(true);
    stdin.resume();
    stdin.setEncoding('utf8');
    stdin.on('data', onData);
    process.once('exit', onExit);
  } else if (wasm) {
    detachLine = attachLineKeys({ stdin, onReload, onTypescript, onFresh });
  }

  // Trigger launch-on-start flags once listening.
  if (flags.open) { log(`web: opening ${url}`); openBrowser(url); }
  if (flags.ios) void runPlatform('ios');
  if (flags.mac) void runPlatform('mac');
  if (flags.android) void runPlatform('android');

  return {
    interactive,
    handleKey,
    async stop() {
      if (closed) return;
      closed = true;
      process.off('exit', onExit);
      if (interactive) { stdin.off('data', onData); setRaw(false); stdin.pause(); }
      detachLine?.();
      stopLogs();
      await Promise.all([
        activeChild ? stopBuildTree(activeChild) : null,
        macChild ? stopBuildTree(macChild) : null,
      ]);
      activeChild = null;
      macChild = null;
    },
  };
}
