import {spawnSync} from 'node:child_process';
import {existsSync, mkdirSync, readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {spawnSync as appleSpawnSync} from '../../host/apple/toolchain.mjs';

const directory = fileURLToPath(new URL('.', import.meta.url));
export function command(program, args, timeout = 15000, run = spawnSync) {
  const r = run(program, args, {encoding: 'utf8', timeout});
  if (r.status !== 0) throw new Error(`${program}: ${r.error?.message || r.stderr?.trim() || r.stdout?.trim() || `exit ${r.status}`}`);
  return r.stdout;
}

export function macHelper() {
  const source = resolve(directory, 'Mac.swift');
  const key = createHash('sha256').update(readFileSync(source)).update(command('xcrun', ['swiftc', '--version'], undefined, appleSpawnSync)).digest('hex').slice(0, 16);
  const binary = resolve(directory, '.cache', `mac-${key}`);
  if (!existsSync(binary)) {
    mkdirSync(resolve(directory, '.cache'), {recursive: true});
    command('xcrun', ['swiftc', '-O', source, '-o', binary], 60000, appleSpawnSync);
  }
  return binary;
}

export function mainWindow(tree) {
  const frame = tree?.mainFrame;
  if (!frame) throw new Error('AXMainWindow has no frame; the capture target is unknown');
  const matches = (tree.windows ?? []).filter(w => w.frame.every((v, i) => Math.abs(v - frame[i]) < 1));
  if (matches.length !== 1) throw new Error(`AXMainWindow joins ${matches.length} window-server windows; capture target is ambiguous or missing`);
  return matches[0];
}

export function platform({host, pid, session, helper}) {
  const mac = (...args) => {
    const r = JSON.parse(command(helper, args));
    if (r.error) throw new Error(r.error);
    return r;
  };
  const ios = async request => {
    const r = await session.carrier.touches.ask(request);
    if (r.error) throw new Error(r.error);
    return r;
  };
  return {
    identity: host === 'macos' ? {host, pid} : {host, simulator: session.carrier.udid},
    async tree() { return host === 'macos' ? mac('tree', String(pid)) : ios({op: 'tree', max: 5000}); },
    async capture(path, tree) {
      if (host === 'ios') {
        command('xcrun', ['simctl', 'io', session.carrier.udid, 'screenshot', '--type=png', path]);
        return {path, source: 'simctl/window-server', region: 'display', frame: tree?.frame};
      }
      const windows = tree?.windows ?? [];
      const main = mainWindow(tree);
      command('/usr/sbin/screencapture', ['-x', '-o', '-l', String(main.id), path]);
      const attachments = [];
      for (const w of windows.filter(w => w.id !== main.id)) {
        const file = path.replace(/\.png$/, `-window-${w.id}.png`);
        command('/usr/sbin/screencapture', ['-x', '-o', '-l', String(w.id), file]);
        attachments.push({...w, path: file});
      }
      return {path, source: 'screencapture/window-server', region: 'window', window: main, attachments};
    },
    async input(action, point, tree) {
      if (host === 'macos') return mac('input', String(pid), ...point.map(String),
        ...(action.kind === 'drag' ? [action.dx, action.dy, action.ms].map(String) : []));
      const origin = tree.frame?.slice(0, 2);
      if (!origin) throw new Error('XCTest snapshot has no application coordinate origin');
      const local = point.map((v, i) => v - origin[i]);
      if (action.kind === 'tap') return ios({op: 'tap', point: local});
      return ios({op: 'drag', point: local, to: [local[0] + action.dx, local[1] + action.dy],
        press: 0, hold: 0, velocity: Math.hypot(action.dx, action.dy) / (action.ms / 1000)});
    },
    activate() { if (host === 'macos') return mac('activate', String(pid)); },
  };
}
