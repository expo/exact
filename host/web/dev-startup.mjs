import {spawn} from 'node:child_process';

/** Whether `exit` settled within a second. */
async function boundedExit(exit) {
  let timer;
  try { return await Promise.race([exit.then(() => true), new Promise(ok => { timer = setTimeout(() => ok(false), 1000); })]); }
  finally { clearTimeout(timer); }
}

const exited = child => new Promise(ok => {
  if (child.exitCode !== null || child.signalCode !== null) ok();
  else { child.once('exit', ok); child.once('error', ok); }
});

/** Stop a detached build's whole group, even if its launcher exits first.
 * `platform` and `spawnTool` are for tests of the Windows path. */
export async function stopBuildTree(child, { platform = process.platform, spawnTool = spawn } = {}) {
  if (!child?.pid) return;
  const exit = exited(child);
  if (platform === 'win32') return stopWindowsTree(child, exit, spawnTool);
  // macOS refuses a group whose members have all exited (zombies) with EPERM,
  // where Linux signals them: either way nothing in it runs.
  const empty = error => error.code === 'ESRCH' || error.code === 'EPERM';
  const kill = signal => {
    try { process.kill(-child.pid, signal); }
    catch (error) { if (!empty(error)) throw error; }
  };
  kill('SIGTERM');
  await boundedExit(exit);
  kill('SIGKILL');
  // Descendants may still be exiting after the launcher's exit. Bound the
  // wait too: on Linux an orphaned zombie keeps its group present until reaped.
  let poll;
  try {
    await boundedExit(new Promise(ok => {
      const gone = () => { try { process.kill(-child.pid, 0); } catch (error) { if (empty(error)) ok(); } };
      poll = setInterval(gone, 50); gone();
    }));
  } finally { clearInterval(poll); }
}

// Windows has no process groups to signal: `taskkill /T /F` ends the tree
// under the launcher's pid, which it finds only while the launcher runs
// (descendants of one that already exited are not reached). A launcher
// taskkill could not end, or could not be started to end, is terminated alone.
async function stopWindowsTree(child, exit, spawnTool) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  let helper = null;
  try { helper = spawnTool('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' }); }
  catch { /* ended alone below */ }
  if (helper && !await boundedExit(exited(helper))) helper.kill();
  if (child.exitCode === null && child.signalCode === null) child.kill();
  await boundedExit(exit);
}

/** Start a build only while its owner accepts work, and report its child until exit. */
export function runBuildTree(command, args, options, own, cancelled) {
  return new Promise((ok, fail) => {
    if (cancelled()) return fail(new Error('build cancelled'));
    const child = spawn(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'], detached: true });
    own(child);
    let output = '';
    child.stdout.on('data', data => { output += data; });
    child.stderr.on('data', data => { output += data; process.stderr.write(data); });
    child.once('error', error => { own(null); fail(error); });
    child.once('exit', code => { own(null); code === 0 ? ok() : fail(new Error(output.trim())); });
  });
}

/** Own compilation until the loop has initialized its shutdown state. */
export function startupBuilds() {
  let child = null, exit = null, cancelled = false, stopping = null, residentStop = null;
  const stop = () => {
    if (cancelled) return stopping;
    cancelled = true;
    stopping = (async () => {
      if (residentStop) return residentStop();
      await stopBuildTree(child);
      process.exit(0);
    })();
    return stopping;
  };
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(signal, stop);
  return {
    async run(command, args, options) {
      if (cancelled) await stopping;
      child = spawn(command, args, {...options, detached:true});
      exit = new Promise((ok, reject) => { child.once('error', reject); child.once('exit', code => ok(code ?? 1)); });
      const code = await exit;
      if (cancelled) await stopping;
      child = null; exit = null;
      return code;
    },
    handoff(stop) { residentStop = stop; },
  };
}
