// PID-scoped simulator launch and foreground log streaming (LLP 1189 D5).
import { spawn, spawnSync } from 'node:child_process';
import { showSimulator, simulatorAppContainer } from './devices.mjs';

/** Parse `<bundle-id>: <pid>` from `xcrun simctl launch` stdout; returns positive integer PID or null. */
export function parseLaunchedPid(stdout) {
  const m = /:\s*(\d+)\s*$/m.exec(String(stdout ?? ''));
  const pid = m ? Number.parseInt(m[1], 10) : NaN;
  return Number.isInteger(pid) && pid > 0 ? pid : null;
}

/** Escape `\` and `"` for Apple's NSPredicate string literals. */
export function escapePredicateString(value) {
  return String(value ?? '').replace(/\\/g, '\\\\').replace(/"/g, '\\"');
}

/** Build the `log stream` predicate: PID-scoped when known, else escaped executable + `dev.exact` subsystems. */
export function logPredicate({ pid, executable }) {
  if (Number.isInteger(pid) && pid > 0) return `processID == ${pid}`;
  const escaped = escapePredicateString(executable);
  return `process == "${escaped}" OR subsystem == "dev.exact" OR subsystem BEGINSWITH "dev.exact."`;
}

/** Launch `appId` on simulator `dev`, raising Simulator and returning `{ pid }`. */
export function launchSimulatorApp(dev, appId, childEnv = {}, {
  runSync = spawnSync,
  show = showSimulator,
  container = simulatorAppContainer,
} = {}) {
  show(dev);
  const env = {
    ...process.env,
    SIMCTL_CHILD_EXACT_ASSETS: container(dev.udid, appId, 'app'),
    ...childEnv,
  };
  const r = runSync('xcrun', ['simctl', 'launch', '--terminate-running-process', dev.udid, appId], { encoding: 'utf8', env });
  if (r.status !== 0) {
    const detail = (r.stderr || r.stdout || r.error?.message || '').trim();
    throw new Error(`xcrun simctl launch failed (${r.status ?? r.error?.message})${detail ? `: ${detail}` : ''}`);
  }
  const out = (r.stdout ?? '').trim();
  if (out) process.stdout.write(`${out}\n`);
  return { pid: parseLaunchedPid(out) };
}

/** Start streaming unified logs from simulator `udid` filtered to the launched app. */
export function streamSimulatorLogs(udid, {
  pid = null,
  executable = '',
  spawnTool = spawn,
  stdout = process.stdout,
  stderr = process.stderr,
} = {}) {
  const predicate = logPredicate({ pid, executable });
  const child = spawnTool('xcrun', ['simctl', 'spawn', udid, 'log', 'stream', '--style', 'compact', '--predicate', predicate], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout?.on('data', (chunk) => stdout.write(chunk));
  child.stderr?.on('data', (chunk) => stderr.write(chunk));
  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
  };
  return { child, predicate, stop };
}

/** Launch a simulator app and optionally remain attached streaming its logs until Ctrl+C / SIGTERM. */
export async function launchAndOptionallyStream(dev, appId, childEnv, { logs = false, executable = '' } = {}) {
  const { pid } = launchSimulatorApp(dev, appId, childEnv);
  if (!logs) return { pid };
  console.error(`host/apple: streaming logs from ${dev.name}${pid ? ` (pid ${pid})` : ''} (ctrl-c to stop)`);
  const stream = streamSimulatorLogs(dev.udid, { pid, executable });
  await new Promise((done) => {
    const finish = () => {
      for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.off(sig, finish);
      stream.stop();
      done();
    };
    for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, finish);
    stream.child.once('exit', finish);
    stream.child.once('error', finish);
  });
  return { pid };
}
