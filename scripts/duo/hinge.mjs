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
  { device: 'landscapeRight', helper: 'landscape-right', tiltedCover: 'landscape-left', scene: 'landscapeLeft', inner: 'portraitUpsideDown', uid: 4, type: 'landscape-secondary' },
  { device: 'portraitUpsideDown', helper: 'pud', scene: 'portraitUpsideDown', inner: 'landscapeRight', uid: 2, type: 'portrait-secondary' },
  { device: 'landscapeLeft', helper: 'landscape-left', tiltedCover: 'landscape-right', scene: 'landscapeRight', inner: 'portrait', uid: 3, type: 'landscape-primary' },
  { device: 'portrait', helper: 'portrait', scene: 'portrait', inner: 'landscapeLeft', uid: 1, type: 'portrait-primary' },
];

const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

// Choose a carrier before issuing the turn. A second setter could turn the
// device again after a slow first one had already reached a locked scene.
export function coreDeviceAvailable(udid) {
  return spawnSync('xcrun', ['devicectl', 'device', 'orientation', 'get', '-d', udid, '--quiet'], { stdio: 'ignore', timeout: 2000 }).status === 0;
}

export function coreDeviceTurn(udid, device, timeout) {
  const r = spawnSync('xcrun', ['devicectl', 'device', 'orientation', 'set', '-d', udid, device, '-j', '-'], { encoding: 'utf8', timeout });
  if (r.status !== 0) {
    const error = new Error(`devicectl orientation set: ${r.error?.message || r.stderr.trim() || r.status}`);
    error.unsupported = !r.error && /unsupported|not supported|not available|unrecognized|unknown subcommand/i.test(r.stderr);
    throw error;
  }
  return JSON.parse(r.stdout).result.deviceOrientation;
}

// A partial fold can leave either display active. SpringBoard names the incoming
// display as it starts the swap, before the app moves to it. The log rounds
// UIKit's display angle to tenths of a degree.
export function partialFoldDisplay(udid, since, timeout) {
  const predicate = 'process == "SpringBoard" AND ((category == "DisplayContentMode" AND (eventMessage BEGINSWITH "STARTING:" OR eventMessage BEGINSWITH "COMPLETED:" OR eventMessage BEGINSWITH "transitioning ")) OR (category == "DevicePose" AND eventMessage BEGINSWITH "DCMPose:"))';
  // Include the last primary display when this fold leaves it in place.
  const seconds = Math.min(600, Math.max(60, Math.ceil((Date.now() - since) / 1000) + 1));
  const run = spawnSync('xcrun', ['simctl', 'spawn', udid, 'log', 'show', '--last', `${seconds}s`, '--style', 'ndjson',
    '--predicate', predicate], { encoding: 'utf8', timeout });
  let inner, angle;
  if (run.status === 0) for (const line of run.stdout.split('\n')) {
    const e = logEntry(line);
    if (!e) continue;
    const pose = /^DCMPose: ([-.\d]+)°/.exec(e.message);
    if (pose && e.t >= since) angle = Number(pose[1]);
    const swap = /^(?:STARTING:|COMPLETED:|transitioning )\s+\.cover:(?:\w+→)?(\w+) \.inner:(?:\w+→)?(\w+)/.exec(e.message);
    if (swap) inner = swap[2] === 'pri';
  }
  return { inner, angle };
}

export async function rotateSimulator({ duo, fold, display, issue, ask, land }, orientation) {
  let expectedPanel = fold === 180 ? true : fold === 0 ? false : undefined;
  const requested = simulatorTurns.find(t => t.scene === orientation);
  const end = performance.now() + 10000;
  // Allow the last observation its own reply, including when the setter timed out.
  const bounded = operation => waitAtMost(operation, Math.max(1000, end - performance.now()), () => { throw new Error('scene did not answer'); });
  let partial, turn, scene, device, inner, reported, landed, lastResult, commandError, reissueAt, reissues = 0;
  let confirmed = false, nextDisplayRead = 0;
  try {
    for (;;) {
      const sample = await bounded(ask({ op: 'layout', simulatorRotation: true }));
      if (sample.error) throw new Error(sample.error);
      if (confirmed && !sample.orientation) return landed;
      scene = sample.orientation;
      inner = sample.inner;
      const observed = simulatorTurns.find(t => t.uid === sample.device);
      device = observed?.device ?? lastResult;
      const now = performance.now();
      // A turn at a book angle can itself change which display is primary.
      // Until UIKit and SpringBoard agree, keep sampling both sides of the swap.
      if (display && now >= nextDisplayRead && (expectedPanel === undefined || inner !== expectedPanel ||
        (!turn && partial?.angle !== undefined && Math.abs(sample.angle - partial.angle) >= 0.1))) {
        const update = display(Math.max(1, Math.min(3000, Math.ceil(end - now))));
        if (update.inner !== undefined || (update.angle !== undefined && Math.abs(sample.angle - update.angle) < 0.1)) {
          partial = update;
          expectedPanel = update.inner ?? inner;
        }
        nextDisplayRead = performance.now() + 200;
      }
      const afterFold = !display || (expectedPanel !== undefined && (turn ||
        (sample.angle > 0 && sample.angle < 180 && (partial.angle === undefined || Math.abs(sample.angle - partial.angle) < 0.1))));
      const onPanel = afterFold && (!duo || (typeof inner === 'boolean' && (expectedPanel === undefined || inner === expectedPanel)));
      const ready = onPanel && (!duo || sample.ready);
      const target = duo && inner ? simulatorTurns.find(t => t.inner === orientation) : requested;
      confirmed = !!turn && onPanel && scene === orientation;
      if (confirmed) {
        if (landed) return landed;
        landed = await land({ orientation });
        continue; // Landing a pending answer can present a differently locked route.
      }
      if (performance.now() >= end) break;
      if (turn && (device === target.device || lastResult === target.device)) reported = target;
      const canRetry = duo && reissues < 2 && !sample.locked && sample.allowed?.includes(requested.type);
      const retry = canRetry && now >= reissueAt && reported !== target && (fold !== undefined || target !== turn);
      if (ready && (!turn || retry)) {
        // A lost HID event can leave the picker at the requested pose while UIKit
        // still reports its old one. Reissue from that reported pose so the picker
        // sends a change. The cover ignores upside down and keeps its last scene.
        const from = turn && observed && (observed[inner ? 'inner' : 'scene'] === scene ||
          (!inner && observed.device === 'portraitUpsideDown')) ? observed : undefined;
        let restored;
        if (from) {
          try { await issue(from, Math.min(1000, Math.ceil(end - now)), sample); restored = from.device; }
          catch { /* The requested turn can still reach UIKit. */ }
        }
        if (turn) console.error(`rotate ${orientation}: reissue ${++reissues}/2 on ${inner ? 'inner' : 'cover'} display (device ${device ?? 'unknown'}, scene ${scene})${restored ? ` from the reported ${restored} pose` : ''}`);
        turn = target;
        reported = undefined;
        landed = undefined;
        reissueAt = now + 3000;
        try {
          commandError = undefined;
          lastResult = await issue(turn, Math.max(1, Math.ceil(end - performance.now())), sample);
        } catch (e) { commandError = e.message; }
        continue; // A setter can time out after UIKit received its turn.
      }
      await sleep(Math.min(100, end - now));
    }
  } catch (e) {
    if (confirmed) return landed ?? { orientation, landError: e.message };
    commandError ??= e.message;
  }
  const pose = turn && (reported === turn || lastResult === turn.device || device === turn.device) ? `device turned ${turn.device}` : `device ${device ?? 'unknown'} (wanted ${turn?.device ?? 'settled display'})`;
  const panel = !duo ? '' : ` on ${inner === undefined ? 'unknown' : inner ? 'inner' : 'cover'} display`;
  throw new Error(`rotate ${orientation}: ${pose}, ${scene === orientation ? 'last scene' : 'scene stayed'} ${scene ?? 'unknown'}${panel} (${commandError ?? '10 s elapsed'})`);
}
