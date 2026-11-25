// The tool runs inside the simulator. Each drive builds it in its own temporary directory.
import { spawnSync } from 'node:child_process';
import { logEntry } from '../../host/apple/cloak.mjs';
import { waitAtMost } from '../agent-wire.mjs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const hasSimulatorHinge = dev => dev.type === 'iPhone-Duo';

export function hingeHelper(udid, dir) {
  const helper = resolve(dir, 'hinge_helper');
  const source = fileURLToPath(new URL('./hinge_helper.m', import.meta.url));
  const run = (command, args, what, timeout = 60000) => {
    const r = spawnSync(command, args, { encoding: 'utf8', timeout });
    if (r.status !== 0) throw new Error(`${what}: ${r.error?.message || r.stderr?.trim() || r.stdout?.trim() || r.signal || r.status}`);
    return r.stdout.trim();
  };
  run('xcrun', ['-sdk', 'iphonesimulator', 'clang', '-arch', process.arch === 'arm64' ? 'arm64' : 'x86_64',
    '-mios-simulator-version-min=17.0', '-O2', '-fobjc-arc', '-DEXACT_AGENT',
    '-framework', 'CoreMotion', '-framework', 'Foundation', '-framework', 'IOKit', '-framework', 'CoreFoundation',
    '-o', helper, source], 'compiling the hinge helper');
  run('codesign', ['-f', '-s', '-', helper], 'signing the hinge helper');
  const invoke = (command, args, timeout) => run('xcrun', ['simctl', 'spawn', udid, helper, command, ...args.map(String)], `hinge ${command} ${args.join(' ')}`, timeout);
  return Object.assign((command, ...args) => invoke(command, args), {
    orient: (pose, timeout) => invoke('orient', [pose], timeout),
  });
}

// UIKit names an interface landscape from the opposite side to the device.
// The Duo's unfolded display is mounted a quarter turn from its cover.
export const simulatorTurns = [
  { device: 'landscapeRight', helper: 'landscape-right', scene: 'landscapeLeft', inner: 'portraitUpsideDown', pose: 'Lr', type: 'landscape-secondary' },
  { device: 'portraitUpsideDown', helper: 'pud', scene: 'portraitUpsideDown', inner: 'landscapeRight', pose: 'Pu', type: 'portrait-secondary' },
  { device: 'landscapeLeft', helper: 'landscape-left', scene: 'landscapeRight', inner: 'portrait', pose: 'Ll', type: 'landscape-primary' },
  { device: 'portrait', helper: 'portrait', scene: 'portrait', inner: 'landscapeLeft', pose: 'Po', type: 'portrait-primary' },
];

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function coreDevice(udid, timeout, ...args) {
  const r = spawnSync('xcrun', ['devicectl', 'device', ...args, '-d', udid, '-j', '-'], { encoding: 'utf8', timeout });
  if (r.status !== 0) throw new Error(`devicectl ${args.join(' ')}: ${r.error?.message || r.stderr.trim() || r.status}`);
  return JSON.parse(r.stdout).result;
}

// The larger integrated panel is the inner one. During a swap the active
// display and the app can change at different times. Compare the scene with
// both displays in points, allowing for rounding at the display's scale.
function duoPanel(displays, screen) {
  const panels = displays.filter(d => d.type?.integrated && Array.isArray(d.nativeSize))
    .sort((a, b) => Math.max(...a.nativeSize) - Math.max(...b.nativeSize));
  if (panels.length !== 2 || panels[0].active === panels[1].active || !screen) return null;
  const inner = panels[1].active;
  const distance = d => Math.abs(Math.max(screen.w, screen.h) - Math.max(...d.nativeSize) / d.pointScale);
  return (distance(panels[1]) < distance(panels[0])) === inner ? { inner } : null;
}

// SpringBoard writes both the change of surface pose and the end of the fold
// transition. When that pose was spent on the transition it sends no surface
// orientation update. A locked app can also stay still after a delivered pose.
function springBoardTurns(udid, since, timeout) {
  const predicate = 'process == "SpringBoard" AND (category == "DevicePose" OR category == "DeviceOrientationUpdateDeferral")';
  const r = spawnSync('xcrun', ['simctl', 'spawn', udid, 'log', 'show', '--last', '12s', '--style', 'ndjson', '--predicate', predicate], { encoding: 'utf8', timeout });
  if (r.status !== 0) return [];
  return r.stdout.split('\n').map(logEntry).filter(e => e && e.t >= since);
}

const sawPose = (entry, surface, pose) => entry.message.includes('surface orientation changed:') &&
  new RegExp(`\\b${surface} \\w+ -> ${pose}\\b`).test(entry.message);
function lostTurn(entries, surface, turn) {
  const pose = entries.find(e => sawPose(e, surface, turn.pose));
  if (!pose) return false;
  const afterPose = entries.filter(e => e.t >= pose.t);
  return afterPose.some(e => e.message.startsWith('Transition style .fade -> .continuousSnapshot'))
    && !afterPose.some(e => e.message.startsWith('Updated device orientation update to:') && e.message.includes(`surfaceType: .${surface}`));
}

export async function rotateSimulator({ udid, orient, fold, ask, land, allowed }, orientation) {
  const end = performance.now() + 10000;
  const remaining = () => {
    const ms = Math.ceil(end - performance.now());
    if (ms <= 0) throw new Error('10 s elapsed');
    return ms;
  };
  const bounded = operation => {
    const ms = remaining();
    return waitAtMost(operation(), ms, () => { throw new Error('10 s elapsed'); });
  };
  const read = async op => {
    const r = await bounded(() => ask({ op }));
    if (r.error) throw new Error(r.error);
    return r;
  };
  let turn, device, scene, issued, nextLog, recovery, result;
  try {
    for (;;) {
      remaining();
      const screen = (await read('layout')).screen;
      scene = screen?.orientation;
      let panel;
      // info displays reports a surface-relative orientation on the inner
      // screen. Only orientation get reports the physical pose we commanded.
      device = coreDevice(udid, remaining(), 'orientation', 'get').deviceOrientation;
      if (orient) panel = duoPanel(coreDevice(udid, remaining(), 'info', 'displays').displays, screen);
      // Intermediate angles can leave either panel active. If the display
      // switches during the turn, aim again using the new panel's mounting.
      if (!orient || (panel && !(fold === 0 && panel.inner) && !(fold === 180 && !panel.inner))) {
        const name = panel?.inner ? 'inner' : 'scene';
        const target = simulatorTurns.find(t => t[name] === orientation);
        if (turn !== target) {
          turn = target;
          issued = Date.now();
          recovery = undefined;
          nextLog = performance.now() + 1000;
          result = undefined;
          if (orient) orient(turn.helper, remaining());
          else coreDevice(udid, remaining(), 'orientation', 'set', turn.device);
        } else if (device === turn.device && scene === orientation) {
          remaining();
          if (result) return result;
          result = await bounded(() => land({ orientation, device }));
          continue; // landing a pending answer can present a differently locked route
        } else if (panel && (panel.inner || turn.pose !== 'Pu') && performance.now() >= nextLog) {
          const surface = panel.inner ? 'inner' : 'cover';
          const turns = springBoardTurns(udid, recovery?.at ?? issued, remaining());
          nextLog = performance.now() + 500;
          if (recovery && turns.some(e => sawPose(e, surface, recovery.pose))) {
            orient(turn.helper, remaining());
            nextLog = Infinity;
          } else if (!recovery && device === turn.device && lostTurn(turns, surface, turn)) {
            const lock = (await read('state')).orientationlock?.applied;
            const type = simulatorTurns.find(t => t.scene === orientation).type;
            // The cover ignores upside down. On the inner panel use the pose
            // for the scene that is still showing, so the recovery keeps it there.
            const via = simulatorTurns.find(t => panel.inner ? t.inner === scene : t.pose === 'Pu');
            if (!via || lock?.locked || (lock?.asked && !lock.asked.includes(type)) || !allowed(orientation)) {
              nextLog = Infinity;
            } else {
              console.error(`rotate ${orientation}: SpringBoard spent the ${surface} pose on the fold transition; retrying once`);
              recovery = { at: Date.now(), pose: via.pose };
              orient(via.helper, remaining());
            }
          }
        }
      }
      await sleep(Math.min(100, remaining()));
    }
  } catch (error) {
    const pose = turn && device === turn.device ? `device turned ${device}` : `device ${device ?? 'unknown'} (wanted ${turn?.device ?? 'active Duo panel'})`;
    const seen = scene === orientation ? `last scene ${scene}` : `scene stayed ${scene ?? 'unknown'}`;
    throw new Error(`rotate ${orientation}: ${pose}, ${seen} (${error.message})`);
  }
}
