/** Other processes: whether one is alive, its command line, and this process's ancestors; and the app server's
 * supervisor (`supervise`). */
import { spawnSync } from 'node:child_process';
import { closeSync, fsyncSync, openSync, readdirSync, readFileSync, writeSync } from 'node:fs';
import { resolve } from 'node:path';
import { constants } from 'node:os';
import { orphaned } from './check-group.mjs';

export const alive = (pid) => { try { process.kill(pid, 0); return true; } catch (e) { return e.code === 'EPERM'; } };
// A process's command line: `ps` has no Windows build, where CIM's Win32_Process reports it. Empty when neither answers.
export const commandLine = (pid) => (process.platform === 'win32'
  ? spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `(Get-CimInstance Win32_Process -Filter 'ProcessId=${Number(pid)}').CommandLine`], { encoding: 'utf8' })
  // -ww: without a terminal, ps may cut the line at 80 columns.
  : spawnSync('ps', ['-ww', '-o', 'command=', '-p', String(pid)], { encoding: 'utf8' })).stdout?.trim() ?? '';

/** Every process's parent, by pid: `ps` has no Windows build, where CIM's Win32_Process reports them, as for
 * `commandLine`. Empty when neither answers. */
export function processParents() {
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

// Win32 job objects (https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), for the supervisor.
const JOB_OBJECT_QUERY = 0x4, JOB_OBJECT_TERMINATE = 0x8, PROCESS_QUERY_LIMITED_INFORMATION = 0x1000;
const JobObjectBasicAccountingInformation = 1, JobObjectExtendedLimitInformation = 9, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000;
const ERROR_FILE_NOT_FOUND = 2, ERROR_ALREADY_EXISTS = 183;
let kernel32;
function jobs() {
  if (kernel32) return kernel32;
  const { dlopen } = import.meta.require('bun:ffi');
  const { symbols } = dlopen('kernel32.dll', {
    CreateJobObjectW: { args: ['ptr', 'ptr'], returns: 'u64' },
    OpenJobObjectW: { args: ['u32', 'i32', 'ptr'], returns: 'u64' },
    SetInformationJobObject: { args: ['u64', 'i32', 'ptr', 'u32'], returns: 'i32' },
    QueryInformationJobObject: { args: ['u64', 'i32', 'ptr', 'u32', 'ptr'], returns: 'i32' },
    AssignProcessToJobObject: { args: ['u64', 'u64'], returns: 'i32' },
    TerminateJobObject: { args: ['u64', 'u32'], returns: 'i32' },
    IsProcessInJob: { args: ['u64', 'u64', 'ptr'], returns: 'i32' },
    OpenProcess: { args: ['u32', 'i32', 'u32'], returns: 'u64' },
    GetCurrentProcess: { args: [], returns: 'u64' },
    CloseHandle: { args: ['u64'], returns: 'i32' },
    GetLastError: { args: [], returns: 'u32' },
    SetLastError: { args: ['u32'], returns: 'void' },
  });
  return kernel32 = symbols;
}
const wide = (name) => Buffer.from(`${name}\0`, 'utf16le');
const fail = (operation, code = jobs().GetLastError()) => new Error(`${operation}: Win32 error ${code}`);

/** The name of the job the app server's supervisor creates on Windows for the launch `token`. The job, and so the
 * name, lasts while a handle to it is open; the supervisor holds one while it lives. */
export const jobName = (token) => `exact-app-server-${token}`;
/** A handle to the job named `name` that can query and terminate it; null when there is none. */
export function openJob(name) {
  const handle = jobs().OpenJobObjectW(JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE, 0, wide(name));
  if (BigInt(handle)) return handle;
  // Read the error in a second call: bun:ffi returns none, and nothing between the calls is expected to set it.
  const code = jobs().GetLastError();
  if (code === ERROR_FILE_NOT_FOUND) return null;
  throw fail(`open job ${name}`, code);
}
/** How many processes in job `handle` have not yet exited. */
export function jobActive(handle) {
  const info = new Uint8Array(48);
  if (!jobs().QueryInformationJobObject(handle, JobObjectBasicAccountingInformation, info, info.length, null)) throw fail('query the app server\'s job');
  // JOBOBJECT_BASIC_ACCOUNTING_INFORMATION.ActiveProcesses follows four LARGE_INTEGERs and two DWORDs.
  return new DataView(info.buffer).getUint32(40, true);
}
/** Whether process `pid` is in job `handle`. */
export function jobHas(handle, pid) {
  const target = jobs().OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
  if (!BigInt(target)) return false;
  const result = new Int32Array(1);
  try { return !!jobs().IsProcessInJob(target, handle, result) && result[0] !== 0; } finally { jobs().CloseHandle(target); }
}
/** Ends every process in job `handle`. */
export function endJob(handle) { if (!jobs().TerminateJobObject(handle, 1)) throw fail('terminate the app server\'s job'); }
export function closeJob(handle) { jobs().CloseHandle(handle); }

/** The job the supervisor runs the server in on Windows: created under `jobName(token)`, this process assigned to
 * it, so every process it starts afterwards is a member; none can leave, as the job allows no breakaway. Closing its
 * last handle ends every member (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). */
function ownJob(token) {
  const k = jobs(), name = jobName(token);
  k.SetLastError(0);
  const job = k.CreateJobObjectW(null, wide(name));
  if (!BigInt(job)) throw fail(`create job ${name}`);
  if (k.GetLastError() === ERROR_ALREADY_EXISTS) throw new Error(`job ${name} already exists`);
  // JOBOBJECT_EXTENDED_LIMIT_INFORMATION is 144 bytes on 64-bit Windows; BasicLimitInformation.LimitFlags is at 16.
  const limits = new Uint8Array(144);
  new DataView(limits.buffer).setUint32(16, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, true);
  if (!k.SetInformationJobObject(job, JobObjectExtendedLimitInformation, limits, limits.length)) throw fail(`limit job ${name}`);
  if (!k.AssignProcessToJobObject(job, k.GetCurrentProcess())) throw fail(`join job ${name}`);
  return job;
}

// Linux prctl(2) and waitpid(2)
const PR_SET_CHILD_SUBREAPER = 36, WNOHANG = 1;

/** This process's children, from Linux's `/proc/self/task/<tid>/children`: the pids of those running and of those
 * that have exited and wait to be reaped. */
function children() {
  const read = (file) => { try { return readFileSync(file, 'utf8'); } catch { return ''; } };
  const running = [], exited = [];
  for (const task of readdirSync('/proc/self/task'))
    for (const pid of read(`/proc/self/task/${task}/children`).trim().split(/\s+/).filter(Boolean)) {
      const stat = read(`/proc/${pid}/stat`);
      if (stat) (stat[stat.lastIndexOf(')') + 2] === 'Z' ? exited : running).push(Number(pid));
    }
  return { running, exited };
}

/** The record a POSIX server's keeper writes in `dir` once it has ended every process it held: whoever stops the server
 * reads it, the process that started it or a later drive. It names the launch's `token`, so an earlier keeper's, with
 * the same pid, vouches for no later one. */
export const endRecord = (dir, group) => resolve(dir, `ended-${group}`);
export function recordedEnd(dir, group, token) {
  try { const record = JSON.parse(readFileSync(endRecord(dir, group), 'utf8')); return record.group === group && record.token === token; }
  catch { return false; }
}

// posix_spawn(3) and waitid(2): the supervisor starts its keeper itself, so that Bun, which reaps only the children it
// spawned, never reaps it. idtype P_PID is 1 on both; the option bits and siginfo_t's si_status offset differ.
const WAITID = process.platform === 'darwin' ? { WEXITED: 0x4, WNOHANG: 0x1, WNOWAIT: 0x20, status: 20 } : { WEXITED: 0x4, WNOHANG: 0x1, WNOWAIT: 0x1000000, status: 24 };
const P_PID = 1, CLD_EXITED = 1;

/** The supervisor `scripts/app-server.mjs` starts in a session of its own (`detached`), so that no signal to its
 * owner's process group, SIGKILL included, reaches it. `owner` is 1 when the process that started it owns the server:
 * its end closes this process's stdin, and that ends the server; 0 for a drive's server, which outlives its drive.
 * `token` names this launch. On POSIX it runs the server under a keeper (`keep`) it starts as its child and never
 * reaps: it exits, as the keeper did, once the keeper has, leaving it to its new parent, so the keeper's pid, the
 * server's group's id, is no other process's while this one runs. On Windows it keeps the server itself. Its stdout
 * carries the keeper's lines. */
function supervise(owner, dir, token, command) {
  if (process.platform === 'win32') return keep(owner, dir, token, 0, command);
  for (const signal of ['SIGINT', 'SIGHUP']) process.on(signal, () => {});
  const { dlopen, ptr } = import.meta.require('bun:ffi');
  const { symbols: libc } = dlopen(process.platform === 'darwin' ? '/usr/lib/libSystem.B.dylib' : 'libc.so.6', {
    posix_spawn: { args: ['ptr', 'ptr', 'ptr', 'ptr', 'ptr', 'ptr'], returns: 'i32' },
    waitid: { args: ['i32', 'u32', 'ptr', 'i32'], returns: 'i32' },
  });
  const text = (value) => Buffer.from(`${value}\0`);
  const array = (texts) => { const out = new BigUint64Array(texts.length + 1); texts.forEach((t, i) => { out[i] = BigInt(ptr(t)); }); return out; };
  const argv = [process.execPath, import.meta.path, 'keep', String(owner), dir, token, String(process.pid), ...command].map(text);
  const envp = Object.entries(process.env).map(([name, value]) => text(`${name}=${value}`));
  const pid = new Int32Array(1), path = text(process.execPath);
  // Its standard streams are this process's: the owner's stdin pipe, the stdout pipe the owner reads, stderr.
  const failed = libc.posix_spawn(pid, path, null, null, array(argv), array(envp));
  if (failed) { console.error(`the app server's supervisor cannot start its keeper: error ${failed}`); process.exit(1); }
  const keeper = pid[0];
  process.on('SIGTERM', () => { try { process.kill(keeper, 'SIGTERM'); } catch {} });
  const info = new Int32Array(32);
  setInterval(() => {
    info.fill(0);
    if (libc.waitid(P_PID, keeper, info, WAITID.WEXITED | WAITID.WNOHANG | WAITID.WNOWAIT) !== 0 || !info[0]) return;
    const code = info[2], status = info[WAITID.status / 4];
    process.exit(code === CLD_EXITED ? status : 128 + status);
  }, 25);
}

/** The keeper takes authority over every process the server will start before the server runs, by the mechanism each
 * platform has:
 * - POSIX: it runs `command` in a process group whose id is its pid, then returns to its supervisor's group, so every
 *   descendant of the server is a member from its fork (a worker forked while handling SIGTERM included), and the id
 *   cannot be another program's while the keeper lives and then while its supervisor holds it unreaped. On Linux it
 *   is also the subreaper of the server's tree (`PR_SET_CHILD_SUBREAPER`), so a descendant that leaves the group
 *   (`setsid`, as a daemon does) is re-parented here when its parent exits instead of escaping to init, and is ended
 *   with the group. macOS has no subreaper: there such a descendant is not followed, and one that keeps the server's
 *   port fails the shutdown, which waits for the port.
 * - Windows: it runs `command` in a job it created and joined (`ownJob`), which no member can leave.
 * It says on stdout, one JSON line each, that it has started the server (with its group's id: on Windows its own pid,
 * which names the job), and how the server exited. SIGTERM (POSIX), the owner gone (EOF on stdin) or its supervisor
 * gone ends whatever remains; on POSIX it then writes `endRecord(dir, pid)` and exits as the server did. Without an
 * owner it ends so once the server has exited and its group is empty. */
function keep(owner, dir, token, supervisor, command) {
  const windows = process.platform === 'win32', linux = process.platform === 'linux';
  let job = null, server = null, ending = false, libc = null, ownerGone = false;
  if (owner) (async () => { for await (const _ of Bun.stdin.stream()); })().catch(() => {}).finally(() => { ownerGone = true; });
  // On Linux, the processes re-parented here that still run, after reaping those that exited (the server is Bun's to
  // reap); null when one exited, since its children may have come here after the list was read. A stray is a child of
  // this process until it is reaped, so its pid cannot be reused before it is killed. With none running and none
  // exited in one reading, no descendant is left: each has an ancestor that is a child of this process.
  const strays = () => {
    if (!linux) return [];
    const { running, exited } = children();
    const adopted = exited.filter((pid) => pid !== server?.pid);
    for (const pid of adopted) libc.waitpid(pid, null, WNOHANG);
    const left = running.filter((pid) => pid !== server?.pid);
    return adopted.length && !left.length ? null : left;
  };
  const serverRuns = () => server && server.exitCode === null && server.signalCode === null;
  const members = () => {
    if (windows) return jobActive(job) > 1;
    try { process.kill(-process.pid, 0); return true; } catch (e) { return e.code === 'EPERM'; }
  };
  const status = () => server ? server.exitCode ?? 128 + constants.signals[server.signalCode] : 1;
  const end = async () => {
    if (ending) return;
    ending = true;
    // Terminating the job ends this process too: the server's exit, if any, was reported when it happened.
    if (windows) { if (members()) endJob(job); process.exit(status()); }
    for (;;) {
      const left = strays();
      if (!members() && left?.length === 0 && !serverRuns()) break;
      try { process.kill(-process.pid, 'SIGKILL'); } catch {}
      // The server itself is killed through its handle: it may have left the group.
      if (serverRuns()) server.kill('SIGKILL');
      for (const pid of left ?? []) try { process.kill(pid, 'SIGKILL'); } catch {}
      await Bun.sleep(25);
    }
    await server?.exited;
    // Durable before this process exits: one that exits without it may have left a process running.
    const record = openSync(endRecord(dir, process.pid), 'w');
    try { writeSync(record, `${JSON.stringify({ group: process.pid, token })}\n`); fsyncSync(record); } finally { closeSync(record); }
    process.exit(status());
  };
  process.on('SIGTERM', end);
  for (const signal of ['SIGINT', 'SIGHUP']) process.on(signal, () => {});
  const spawn = () => {
    try { server = Bun.spawn(command, { stdin: 'ignore', stdout: 'ignore', stderr: 'inherit' }); }
    catch (e) { console.error(e.message); process.exit(1); }
  };
  if (windows) {
    try { job = ownJob(token); } catch (e) { console.error(`the app server's supervisor cannot create its job: ${e.message}`); process.exit(1); }
    spawn();
  } else {
    const { dlopen, FFIType } = import.meta.require('bun:ffi');
    libc = dlopen(linux ? 'libc.so.6' : '/usr/lib/libSystem.B.dylib', {
      getpgid: { args: [FFIType.i32], returns: FFIType.i32 },
      setpgid: { args: [FFIType.i32, FFIType.i32], returns: FFIType.i32 },
      ...linux && {
        prctl: { args: [FFIType.i32, FFIType.u64, FFIType.u64, FFIType.u64, FFIType.u64], returns: FFIType.i32 },
        waitpid: { args: [FFIType.i32, FFIType.ptr, FFIType.i32], returns: FFIType.i32 },
      },
    }).symbols;
    if (linux && libc.prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) !== 0) { console.error('the app server\'s keeper cannot become a subreaper'); process.exit(1); }
    const home = libc.getpgid(0);
    if (home < 0 || libc.setpgid(0, 0) !== 0) { console.error('the app server\'s keeper cannot lead a process group'); process.exit(1); }
    spawn();
    if (libc.setpgid(0, home) !== 0) { console.error('the app server\'s keeper cannot leave its group'); process.kill(-process.pid, 'SIGKILL'); process.exit(1); }
  }
  // Its reader may be gone, a drive that started it with it: a write to the broken pipe fails with EPIPE, which a
  // synchronous write raises here rather than as a rejection no one handles, which would end this process.
  const say = (line) => { try { writeSync(1, `${JSON.stringify(line)}\n`); } catch { /* No one to tell. */ } };
  say({ started: true, group: process.pid });
  server.exited.then(() => say({ exitCode: server.exitCode, signalCode: server.signalCode }));
  setInterval(() => {
    if (ending) return;
    strays();
    const orphan = !windows && orphaned(supervisor);
    if (owner ? ownerGone || orphan : orphan || !serverRuns() && !members()) end();
  }, 100);
}

const [verb, owner, dir, token, ...rest] = import.meta.main ? process.argv.slice(2) : [];
if (verb === 'supervise') supervise(Number(owner), dir, token, rest);
if (verb === 'keep') keep(Number(owner), dir, token, Number(rest[0]), rest.slice(1));
