/** Other processes: whether one is alive, its command line, and this process's ancestors. */
import { spawnSync } from 'node:child_process';

export const alive = (pid) => { try { process.kill(pid, 0); return true; } catch (e) { return e.code === 'EPERM'; } };
// A process's command line: `ps` has no Windows build, where CIM's Win32_Process reports it. Empty when neither answers.
export const commandLine = (pid) => (process.platform === 'win32'
  ? spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `(Get-CimInstance Win32_Process -Filter 'ProcessId=${Number(pid)}').CommandLine`], { encoding: 'utf8' })
  : spawnSync('ps', ['-o', 'command=', '-p', String(pid)], { encoding: 'utf8' })).stdout?.trim() ?? '';

/** Every process's parent, by pid: `ps` has no Windows build, where CIM's Win32_Process reports them, as for
 * `commandLine`. Empty when neither answers. */
function processParents() {
  const r = process.platform === 'win32'
    ? spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Get-CimInstance Win32_Process | ForEach-Object { "$($_.ProcessId) $($_.ParentProcessId)" }'], { encoding: 'utf8' })
    : spawnSync('ps', ['-A', '-o', 'pid=,ppid='], { encoding: 'utf8' });
  return parentsOf(r.stdout ?? '');
}

/** `<pid> <parent pid>` lines as a map from pid to parent pid. */
export function parentsOf(text) {
  const parents = new Map();
  for (const line of text.split('\n')) {
    const [pid, parent] = line.trim().split(/\s+/).map(Number);
    if (Number.isInteger(pid) && Number.isInteger(parent)) parents.set(pid, parent);
  }
  return parents;
}

/** The ancestors' pids of the process whose parent is `pid`, nearest first, as strings: `pid`, its parent, and so
 * on up to the first pid that `parents` does not know, the root (0 or 1), or a cycle. */
export function ancestors(pid = process.ppid, parents = processParents()) {
  const out = [];
  while (pid > 1 && out.length < 32 && !out.includes(String(pid))) {
    out.push(String(pid));
    pid = parents.get(pid) ?? 0;
  }
  return out;
}
