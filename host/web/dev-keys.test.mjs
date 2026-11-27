import { test, expect } from 'bun:test';
import { EventEmitter } from 'node:events';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { attachDevControls, createLaunchGate, keybarText, parseDevLaunchFlags, platformLaunchCommand } from './dev-keys.mjs';

class FakeStdin extends EventEmitter {
  constructor(isTTY) {
    super();
    this.isTTY = isTTY;
    this.isRaw = false;
    this.rawHistory = [];
  }
  setRawMode(on) { this.isRaw = on; this.rawHistory.push(on); }
  setEncoding() {}
  resume() {}
  pause() {}
}

function fakeChild() {
  const child = new EventEmitter();
  child.pid = 0;
  child.exitCode = null;
  child.signalCode = null;
  child.stdout = new EventEmitter();
  child.kill = () => { child.exitCode = 0; child.emit('exit', 0); };
  return child;
}

test('parseDevLaunchFlags extracts all dev launcher flags', () => {
  expect(parseDevLaunchFlags(['--ios', '--sim', 'iPhone 17', '--open', '--no-interactive'])).toEqual({
    ios: true, mac: false, android: false, open: true, sim: 'iPhone 17', noInteractive: true,
  });
});

test('platformLaunchCommand routes external exact new apps through exact.mjs for bringPlatform', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-dev-cmd-'));
  try {
    writeFileSync(resolve(dir, 'exact.mjs'), '// launcher\n');
    const outside = { dir, name: 'notes', outside: true, crate: () => 'notes-apple' };
    const ext = platformLaunchCommand('ios', { app: outside, root: '/repo', url: 'http://127.0.0.1:8765/', simUdid: 'U1' });
    expect(ext.args).toEqual([resolve(dir, 'exact.mjs'), 'ios', '--run', '--url', 'http://127.0.0.1:8765/', '--sim', 'U1']);
    const internal = { dir: '/repo/apps/caltrain', name: 'caltrain', outside: false, crate: () => 'caltrain-apple' };
    const inRepo = platformLaunchCommand('ios', { app: internal, root: '/repo', url: 'http://127.0.0.1:8765/', simUdid: 'U1' });
    expect(inRepo.args).toEqual(['/repo/host/apple/build.mjs', '--ios', 'caltrain-apple', '--run', '--url', 'http://127.0.0.1:8765/', '--sim', 'U1']);
    // Android follows the loop as iOS does: a save reloads the installed APK.
    expect(platformLaunchCommand('android', { app: outside, root: '/repo', url: 'http://127.0.0.1:8765/' }).args).toEqual([resolve(dir, 'exact.mjs'), 'android', '--run', '--url', 'http://127.0.0.1:8765/']);
    expect(platformLaunchCommand('android', { app: internal, root: '/repo', url: 'http://127.0.0.1:8765/' }).args).toEqual(['/repo/scripts/agent-android.mjs', 'build', 'caltrain', '--apk', '--install', '--run', '--url', 'http://127.0.0.1:8765/']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('interactive TTY controls dispatch keys, enforce single-launch mutex, and restore raw mode on quit', async () => {
  const stdin = new FakeStdin(true);
  const stdout = { isTTY: true, write() {} };
  const logs = [], opened = [], spawns = [];
  let reloaded = 0, stopped = 0;
  const gate = createLaunchGate();
  const child = fakeChild();
  const controls = attachDevControls({
    app: { dir: '/repo/apps/caltrain', name: 'caltrain', displayName: 'Caltrain', id: 'com.exact.caltrain', outside: false, crate: () => 'caltrain-apple' },
    root: '/repo',
    url: 'http://127.0.0.1:8765/',
    wasm: false,
    launchGate: gate,
    stdin, stdout, platform: 'darwin', env: {},
    onReload: () => reloaded++,
    onStop: async () => { stopped++; await controls.stop(); },
    openBrowser: (u) => opened.push(u),
    resolveSim: async () => ({ udid: 'SIM-1', name: 'iPhone 17 Pro' }),
    spawnTool: (cmd, args) => { spawns.push([cmd, args]); return child; },
    log: (line) => logs.push(line),
  });
  expect(controls.interactive).toBe(true);
  expect(stdin.isRaw).toBe(true);
  expect(logs[0]).toBe(keybarText({ wasm: false, platform: 'darwin' }));
  expect(logs[0]).not.toContain('plan ready');

  stdin.emit('data', 'w');
  expect(opened).toEqual(['http://127.0.0.1:8765/']);

  stdin.emit('data', 'r');
  expect(reloaded).toBe(1);

  // Start iOS launch and verify concurrent press is blocked by the shared gate.
  stdin.emit('data', 'i');
  await new Promise((ok) => setTimeout(ok, 10));
  expect(spawns).toHaveLength(1);
  expect(gate.active).toBe('ios');
  stdin.emit('data', 'i');
  expect(logs.some((l) => l.includes('wait for the active ios launch to finish'))).toBe(true);
  child.stdout.emit('data', Buffer.from('com.exact.caltrain: 4321\n'));
  child.exitCode = 0;
  child.emit('exit', 0);
  await new Promise((ok) => setTimeout(ok, 10));
  expect(gate.active).toBeNull();

  // Quit restores raw mode cleanly.
  stdin.emit('data', 'q');
  await new Promise((ok) => setTimeout(ok, 10));
  expect(stopped).toBe(1);
  expect(stdin.isRaw).toBe(false);
});

test('non-TTY wasm mode keeps legacy line commands without touching raw mode', async () => {
  const stdin = new FakeStdin(false);
  let r = 0, t = 0, f = 0;
  const controls = attachDevControls({
    app: { dir: '/repo/apps/caltrain', name: 'caltrain', id: 'com.exact.caltrain', outside: false, crate: () => 'caltrain-apple' },
    root: '/repo', url: 'http://127.0.0.1:8765/', wasm: true,
    stdin, stdout: { isTTY: false, write() {} }, env: {},
    onReload: () => r++, onTypescript: () => t++, onFresh: () => f++,
    log: () => {},
  });
  expect(controls.interactive).toBe(false);
  expect(stdin.rawHistory).toEqual([]);
  stdin.emit('data', 'r t f\n');
  expect([r, t, f]).toEqual([1, 1, 1]);
  await controls.stop();
});
