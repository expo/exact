// The app server's discovery, scratch databases and process lifetime.
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import { closeSync, mkdirSync, openSync, readFileSync, readdirSync, realpathSync, rmdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { constants, networkInterfaces } from 'node:os';
import { takePort } from './port-lock.mjs';
import { alive, ancestors, closeJob, commandLine, endJob, endRecord, jobActive, jobHas, jobName, openJob, recordedEnd } from './processes.mjs';
import { backendOriginSelection, privateNetwork } from '../js/bake/src/typescript.mjs';
import { appModules, appSettings, appSourceInputs, readsDB } from './app-imports.mjs';

// The app's own server (LLP 1116.006 D3). A local origin the data module calls, with any path under it.
const LOCAL = /\bhttps?:\/\/(?:127\.0\.0\.1|localhost|\[::1\]):\d{2,5}(?:\/[\w\-./]*)?/, LOCAL_SOCKET = new RegExp(LOCAL.source.replace('https?', 'wss?'));
const readIn = (dir, path) => { try { return readFileSync(resolve(dir, path), 'utf8'); } catch { return null; } };

/** Whether `origin` is served from this machine: a loopback address, or a private-network one that names
 *  one of this machine's interfaces (a phone's development build reaches the checkout's fixture there). */
export function onThisMachine(origin) {
  return /^https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?::\d+)?$/.test(origin)
    || privateNetwork(origin) && Object.values(networkInterfaces()).flat().some(n => n?.address === new URL(origin).hostname.replace(/^\[|\]$/g, ''));
}

/** App.ts and its transitive app-local imports, with settings inlined (LLP 1153 D4); authored imports when the bake refuses. */
function builtSettings(dir, env) {
  const inputs = appSourceInputs(dir);
  if (!inputs && readIn(dir, 'app.ts') !== null) throw new Error('build the app before discovering its server: no recorded module inputs');
  const ordered = modules => {
    const seen = new Set(), result = new Map(), imports = new Map();
    for (const [file,,,next] of inputs?.edges ?? []) {
      if (!imports.has(file)) imports.set(file, []);
      imports.get(file).push(next);
    }
    const visit = file => {
      if (seen.has(file)) return;
      seen.add(file);
      if (modules.has(file)) result.set(file, modules.get(file));
      for (const next of imports.get(file) ?? []) visit(next);
    };
    visit('app.ts');
    return result;
  };
  try {
    const { modules, record, reads } = appSettings(dir, env, inputs);
    return { modules:ordered(modules), reads, values: Object.fromEntries(Object.entries(record).map(([name, s]) => [name, s.value])) };
  } catch {
    return { modules: ordered(appModules(dir, inputs).modules), values: {}, reads:[] };
  }
}
/** `text` with its comments blanked, so a URL a comment names is not taken for one the code calls; as written when
 * it does not parse. */
function uncommented(text, path) {
  if (!text) return text;
  try {
    const parsed = createRequire(import.meta.url)('rolldown/utils').parseSync(path, text);
    if (parsed.errors?.length) return text;
    for (const c of [...parsed.comments].reverse()) text = text.slice(0, c.start) + ' '.repeat(c.end - c.start) + text.slice(c.end);
    return text;
  } catch { return text; }
}
/** The server URL: app.ts's local setting first, else the first in a depth-first, pre-order walk of its recorded imports
 * in source order, else the first literal URL. With no HTTP literal, use a socket's origin. Null with none. */
export function serverUrl(dir, env = process.env) { return serverChoice(dir, env).url; }

/** `serverUrl`'s answer as `{ url, note }`: `note` says which origin it took and why when the code names more than one. */
export function serverChoice(dir, env = process.env) {
  const { modules, values, reads } = builtSettings(dir, env);
  const texts = [...modules].map(([path, code]) => [path, uncommented(code, path)]);
  const all = (re) => texts.flatMap(([path, t]) => [...(t ?? '').matchAll(new RegExp(re.source, 'g'))].map((m) => ({ path, url: m[0] })));
  const found = [...all(LOCAL), ...all(LOCAL_SOCKET)].map((f) => ({ ...f, url: f.url.replace(/\/$/, '').replace(/^ws/, 'http') }));
  let setting;
  for (const [file] of texts) {
    const settings = reads.filter(read => read.file === file).map(({name}) => ({name,url:String(values[name]).match(LOCAL)?.[0] ?? String(values[name]).match(LOCAL_SOCKET)?.[0]})).filter(s => s.url);
    const distinct = [...new Set(settings.map(s => s.url.replace(/\/$/, '').replace(/^ws/, 'http')))];
    if (distinct.length > 1) throw new Error(`server discovery: equally first local settings in ${file}: ${settings.map(s => `${s.name}=${s.url}`).join(', ')}; choose one server setting in app.json's "env" and read it in app.ts, keeping other clients' settings in their modules`);
    if (settings.length) { setting = settings[0]; break; }
  }
  const url = setting ? setting.url.replace(/\/$/, '').replace(/^ws/, 'http') : found[0]?.url ?? null;
  const origins = [...new Set([...(setting ? [url] : []), ...found.map((f) => f.url)].map((u) => new URL(u).origin))];
  const why = setting ? `the value of ${setting.name}` : `the first in ${found[0]?.path}'s code`;
  return { url, note: url && origins.length > 1 ? `the app's code names ${origins.join(', ')}: the server is started at ${url}, ${why}` : null };
}

/** The app's own server, the `rest` pattern's shape: an entry Bun runs (app.json's `commands.server`, as
 * `["bun", "server/index.ts"]`, else server.ts, server/index.ts or server/server.ts) at the URL the data module calls,
 * its database's path read from `DB` unless `server.database` is false. Arguments after the named entry are kept in
 * `args`. A `commands.server` another program runs (`["python3", "server.py"]`) is run as written (`command`), told `DB`
 * and `PORT` as Bun's is (LLP 1156.001 D5); its entry is its first argument that is a file of the app, which the `DB`
 * check reads; with none, it is refused unless `server.database` is false, as a test never writes to a database it cannot tell is its own. null without a server; else `why` says what keeps it
 * from being run when the app calls no local URL (null when nothing does), `refusal` (LLP 1116.008 D1) the one line for an
 * entry that needs a database but does not read `DB`, which `test` and `verify` stop on. */
export function appServer(dir, env = process.env) {
  let named = null, serverArgs = [], manifest = {};
  try { manifest = JSON.parse(readIn(dir, 'app.json') ?? '{}') ?? {}; } catch {}
  const argv = manifest.commands?.server, database = manifest.server?.database !== false;
  let command = null;
  if (Array.isArray(argv) && /(^|\/)bun$/.test(argv[0])) {
    const index = argv.findIndex((a, i) => i > 0 && !a.startsWith('-'));
    if (index >= 0) { named = argv[index]; serverArgs = argv.slice(index + 1); }
  } else if (Array.isArray(argv) && argv.length && argv.every((a) => typeof a === 'string')) {
    command = argv; named = argv.find((a, i) => i > 0 && !a.startsWith('-') && readIn(dir, a) != null) ?? null;
  }
  const entry = command ? named ?? command.join(' ') : [named, 'server.ts', 'server/index.ts', 'server/server.ts'].find((p) => p && readIn(dir, p) != null);
  if (!entry) return null;
  const fixture = /(^|[/\\])fixtures?[/\\]/.test(entry);
  const selected = fixture ? backendOriginSelection(dir, env) : null;
  const local = selected && onThisMachine(selected.origin);
  const chosen = selected ? null : serverChoice(dir, env), url = selected ? local ? selected.origin : null : chosen.url;
  const why = !url ? `${entry} is left alone: app.ts calls no local URL (http://127.0.0.1:<port>), so where the app reaches it is unknown` : null;
  const refusal = url && database && command && !named
    ? `app.json's commands.server (${command.join(' ')}) names no file of the app to read, so whether it opens its database at the path DB names cannot be told: name its entry file in the command, or say it keeps none in app.json: "server": { "database": false }`
    : url && database && !readsDB(dir, entry)
    ? `${entry}${entry === named ? ' (app.json\'s commands.server)' : ''} does not read its database's path from DB, so it cannot be run on an empty one: ${command ? 'open the database at the path DB names (os.environ["DB"] in Python), or say it keeps none in app.json: "server": { "database": false }' : "write new Database(process.env.DB ?? 'data.db') in it or a module it imports, or name the entry that does in app.json: \"commands\": { \"server\": [\"bun\", \"server/index.ts\"] }"}`
    : null;
  const port = url ? Number(new URL(url).port || (new URL(url).protocol === 'https:' ? 443 : 80)) : null;
  if (selected && local) { const at = serverArgs.indexOf('--port'); if (at >= 0) serverArgs[at + 1] = String(port); }
  return { entry, args: entry === named ? serverArgs : [], database, url, port, why, refusal, settings: builtSettings(dir, env).values, ...(chosen?.note ? { note: chosen.note } : {}), ...(fixture ? { fixture: true } : {}), ...(command ? { command } : {}) };
}

// The app's settings as `appServer` read them: the server listens where the build calls it, as Bun's .env loading may not.
function serverEnv(server, db) {
  const env = { ...process.env, ...server.settings, PORT: String(server.port) };
  if (server.database) env.DB = db; else delete env.DB;
  return env;
}

export const serverRun = (server, name) => `${(server.command ?? ['bun', server.entry, ...server.args]).join(' ')} at ${server.url}, ${server.database ? `on an empty database${name === 'api-test' ? '' : ' for each test'} (.exact/${name}/db.sqlite, removed after)` : `without a database${name === 'api-test' ? '' : ', restarted empty for each test'}`}${server.note ? `; ${server.note}` : ''}`;

/** Whether anything answers HTTP at `url` (any status). */
export const answering = (url) => fetch(url, { signal: AbortSignal.timeout(1000) }).then(() => true, () => false);

/** The process listening on `server.port`, when the system says: its `pid`, and `holder`, " already answers (pid …:
 * <command>)" to follow the URL. */
function portHolder(server) {
  const pid = spawnSync('lsof', ['-nP', `-iTCP:${server.port}`, '-sTCP:LISTEN', '-Fp'], { encoding: 'utf8' }).stdout?.match(/^p(\d+)/m)?.[1];
  const command = pid ? commandLine(pid).slice(0, 60) : '';
  return { pid, holder: ` already answers${pid ? ` (pid ${pid}${command ? `: ${command}` : ''})` : ''}` };
}

/** The one line a server's URL already answering gets: who holds the port, when the system says, and what to do. A
 * server this did not start is never stopped, nor written to. */
export function heldLine(server) {
  const { pid, holder } = portHolder(server);
  return `${server.url}${holder}, and the tests would ${server.database ? "write into that server's database" : 'change that server\'s state'}: stop it (${pid ? `kill ${pid}` : 'Ctrl-C where it runs'}) and run this again, which starts ${server.entry} itself${server.database ? ' on an empty one' : ''}${server.note ? `; ${server.note}` : ''}`;
}

/** Keep the URL appServer accepted, including its path and explicit default port, for exact record comparisons. */
export function recordedServerAddress(value) {
  if (typeof value !== 'string') return null;
  let url; try { url = new URL(value); } catch { return null; }
  const authority = /^[a-z]+:\/\/([^/]*)/i.exec(value)?.[1];
  if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || !authority || authority.includes('@') || value.includes('?') || value.includes('#')) return null;
  return { url: value, port: Number(url.port || (url.protocol === 'https:' ? 443 : 80)) };
}

/** A managed server needs this directory's launch record and the recorded process still listening. The record is named
 * by the server's group: on POSIX the id of the process group every listener the server starts is in, its keeper's pid;
 * on Windows the pid of the supervisor whose job every listener is in, `group`'s (this invocation's launch) or the one
 * the record names. */
function ownsServer(dir, server, expectedPid, group = null) {
  const listeners = process.platform === 'win32'
    ? spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `(Get-NetTCPConnection -State Listen -LocalPort ${server.port} -ErrorAction SilentlyContinue).OwningProcess`], { encoding: 'utf8' }).stdout?.match(/\d+/g) ?? []
    : [...(spawnSync('lsof', ['-nP', `-iTCP:${server.port}`, '-sTCP:LISTEN', '-Fp'], { encoding: 'utf8' }).stdout?.matchAll(/^p(\d+)/gm) ?? [])].map(m => m[1]);
  const root = realpathSync(dir), lease = resolve(dir, '.exact/agent-server');
  for (const listener of listeners) {
    const pid = process.platform !== 'win32' ? spawnSync('ps', ['-o', 'pgid=', '-p', listener], { encoding: 'utf8' }).stdout?.trim()
      : group?.job ? (jobHas(group.job, Number(listener)) ? String(group.pid) : null) : recordedJobOf(lease, Number(listener));
    if (!pid) continue;
    if (expectedPid !== undefined && Number(pid) !== expectedPid) continue;
    let record; try { record = JSON.parse(readFileSync(resolve(lease, `server-${pid}`), 'utf8')); } catch { continue; }
    if (!record || record.directory !== root || record.origin !== server.url || typeof record.token !== 'string' || !alive(Number(pid))) continue;
    const command = commandLine(pid);
    // On POSIX the pid is the keeper's, whose command line names its launch's token.
    if (!command.includes(server.entry) || process.platform !== 'win32' && !command.includes(record.token)) continue;
    // CIM exposes no cwd: the job, which only the recorded supervisor's tree is in, stands for it.
    if (process.platform === 'win32') return true;
    const cwd = spawnSync('lsof', ['-a', '-p', pid, '-d', 'cwd', '-Fn'], { encoding: 'utf8' }).stdout?.match(/^n(.+)$/m)?.[1];
    try { if (cwd && realpathSync(cwd) === root) return true; } catch { /* The process or its directory disappeared. */ }
  }
  return false;
}

/** The pid named by a `server-<pid>` record in `lease` whose launch's job, named by the record's token, holds
 * `listener`, else null. A token names one launch: a record outlived by its server names no later launch's job. */
function recordedJobOf(lease, listener) {
  let names = []; try { names = readdirSync(lease); } catch {}
  for (const name of names) {
    const pid = /^server-(\d+)$/.exec(name)?.[1];
    let token = null; try { token = pid && JSON.parse(readFileSync(resolve(lease, name), 'utf8')).token; } catch {}
    const job = typeof token === 'string' && openJob(jobName(token));
    if (!job) continue;
    try { if (jobHas(job, listener)) return pid; } finally { closeJob(job); }
  }
  return null;
}

/** Runs `kill` when this process ends however it does (a signal ends it with the shell's code too); the function returned
 * takes that back. */
export function whenEnded(kill) {
  const signals = { SIGINT: 130, SIGTERM: 143, SIGHUP: 129 }, onSignal = (signal) => { try { kill(); } finally { process.exit(signals[signal]); } };
  const onExit = () => { try { kill(); } catch {} };
  process.on('exit', onExit);
  for (const s of Object.keys(signals)) process.on(s, onSignal);
  return () => { process.off('exit', onExit); for (const s of Object.keys(signals)) process.off(s, onSignal); };
}

const exitStatus = child => child?.exitCode ?? (child?.signalCode ? 128 + constants.signals[child.signalCode] : null);
const exitReason = child => child.signalCode ? `signal ${child.signalCode}` : `code ${child.exitCode}`;

/** Waits for this launch to answer; a signal exit is a death even though exitCode stays null. */
async function untilAnswers(server, child, tail, group) {
  const deadline = Date.now() + 10_000;
  for (;;) {
    if (group) groupAlive(group);
    const answered = await answering(server.url);
    if (exitStatus(child) != null || !answered && Date.now() >= deadline)
      return `${server.entry} ${exitStatus(child) != null ? `exited (${exitReason(child)})` : 'did not answer in 10 s'} before it answered at ${server.url}${tail() ? `: ${tail()}` : ''}`;
    if (answered) { if (group) groupAlive(group); return null; }
    await Bun.sleep(25);
  }
}

/** The last error line of what a server said, else its last line. */
const lastError = (said) => { const lines = said.split('\n').map((l) => l.trim()).filter(Boolean); return lines.findLast((l) => /error/i.test(l)) ?? lines.at(-1) ?? ''; };

/** Runs the app's server (`appServer`) on an empty scratch database, `.exact/<name>/db.sqlite` (none with database: false), and answers once it
 * answers; refused (`heldLine`) when its URL already does. `beforeTest` restarts after any earlier test, across
 * files and passes; `fresh` restarts unconditionally. Both retain the port turn and owner file. `stop` ends the
 * process group and removes the database; `check` attributes a server exit to its test. `forward` passes stderr on. */
export async function startServer(dir, server, name, { forward = false } = {}) {
  // Its port in turn with other runs (scripts/port-lock.mjs), then refused
  // if something that took no turn (a server run by hand) answers there.
  const give = await takePort(server.port, { label: `${name}: ${server.entry} in ${dir}` });
  let owned = false;
  try { return await started(dir, server, name, forward, give, () => { owned = true; }); }
  catch (e) { if (!owned) give(); throw e; }
}

const SUPERVISOR = fileURLToPath(new URL('processes.mjs', import.meta.url));
/** A POSIX server's process group `pid`, whose id is its keeper's pid, held while `running` says the supervisor, which
 * never reaps the keeper, runs. `end` asks the keeper to end what remains and exit; `release` waits for that and for its
 * record of the end of launch `token` in `dir` (processes.mjs `recordedEnd`), which `close` removes once the whole stop
 * has succeeded. On Linux the keeper also holds processes outside the group, so a keeper that exited without the
 * record may have left one running; on macOS it holds none. */
function supervisedGroup(pid, running, end, dir, token) {
  return {
    pid, running, ended: () => recordedEnd(dir, pid, token), end: () => { if (running()) end(); },
    release: async (deadline) => {
      if (running()) end();
      while (running()) {
        if (Date.now() >= deadline) throw new Error(`server shutdown did not finish: the keeper of process group ${pid} is still running, with a process the server started that it cannot end`);
        await Bun.sleep(25);
      }
      if (process.platform === 'linux' && !recordedEnd(dir, pid, token))
        throw new Error(`server shutdown cannot verify that the keeper of process group ${pid} ended every process the server started: it exited without recording it`);
    },
    close: () => rmSync(endRecord(dir, pid), { force: true }),
  };
}
/** A Windows server's job, `job` a handle to it (processes.mjs `openJob`); its supervisor `pid` is a member. */
const jobGroup = (pid, job) => ({ pid, job, close: () => closeJob(job) });
/** Starts the app server under its supervisor (processes.mjs `supervise`), in a session of its own: the handle is the
 * supervisor's, whose exit status is the server's once reported, else its own; `started` is the server's group's id
 * once the server runs (on Windows the supervisor's pid, which names its job), else null. `owner`: whether this
 * process's end ends the server, through the supervisor's stdin, which only this process writes. `dir` takes the
 * keeper's record of the end. */
function spawnServer(server, dir, env, stderr, owner, records) {
  const command = server.command ?? [process.execPath, server.entry, ...server.args], token = randomUUID();
  const supervisor = Bun.spawn([process.execPath, SUPERVISOR, 'supervise', owner ? '1' : '0', records, token, ...command], { cwd: dir, env, detached: true, stdin: owner ? 'pipe' : 'ignore', stdout: 'pipe', stderr });
  // Its lines: `started` with the group's id, then the server's exit status.
  let reported = null, begun;
  const started = new Promise((done) => { begun = done; });
  (async () => {
    let text = '';
    for await (const chunk of supervisor.stdout) {
      text += new TextDecoder().decode(chunk);
      for (let at; (at = text.indexOf('\n')) >= 0; text = text.slice(at + 1)) {
        const line = JSON.parse(text.slice(0, at));
        if (line.started) begun(line.group); else reported ??= line;
      }
    }
  })().catch(() => {}).finally(() => begun(null));
  return {
    pid: supervisor.pid, stderr: supervisor.stderr, exited: supervisor.exited, started, records, token,
    // Held, so the pipe stays open while this process lives.
    stdin: supervisor.stdin,
    running: () => supervisor.exitCode === null && supervisor.signalCode === null,
    get exitCode() { return (reported ?? supervisor).exitCode; },
    get signalCode() { return (reported ?? supervisor).signalCode; },
    kill: (signal) => supervisor.kill(signal),
  };
}
/** The group `pid` of a server another process started as launch `token`, with its records in `dir`: on POSIX the
 * keeper's group, on Windows the supervisor's job. */
const processGroup = (pid, dir, token) => {
  // The keeper's command line names its launch's token: a later process given that pid does not.
  const keeps = () => { const command = commandLine(pid); return command.includes(SUPERVISOR) && command.includes(token); };
  if (process.platform !== 'win32') return supervisedGroup(pid, keeps, () => { if (keeps()) process.kill(pid, 'SIGTERM'); }, dir, token);
  const job = openJob(jobName(token));
  if (!job) throw new Error(`cannot find the job of the Windows app server's supervisor ${pid}`);
  return jobGroup(pid, job);
};
/** The launch's group once its supervisor has started the server: before then it may not yet handle SIGTERM, or have
 * its job; null if it exited first. A supervisor starts the server only in a job it created, never in an earlier
 * launch's of the same name. */
async function launchedGroup(child) {
  const group = await child.started;
  if (group == null) { await child.exited; return null; }
  if (process.platform !== 'win32') {
    return supervisedGroup(group, child.running, () => child.kill('SIGTERM'), child.records, child.token);
  }
  let job = null;
  try { job = openJob(jobName(child.token)); } catch (e) { if (exitStatus(child) == null) child.kill('SIGKILL'); await child.exited; throw e; }
  if (job) return jobGroup(group, job);
  if (exitStatus(child) == null) child.kill('SIGKILL');
  await child.exited;
  throw new Error(`cannot find the job of the Windows app server's supervisor ${group}`);
}
/** A job has no signals: either signal ends it at once. */
function signalGroup(group, signal) {
  if (!group) return;
  if (group.job) endJob(group.job);
  else if (groupAlive(group)) try { process.kill(-group.pid, signal); } catch (e) { if (e.code !== 'ESRCH') throw e; }
}
function groupAlive(group) {
  if (group.job) return jobActive(group.job) > 0;
  // Recorded once its keeper had emptied it: the id may since be another program's.
  if (group.ended()) return false;
  try { process.kill(-group.pid, 0); } catch (e) { if (e.code === 'ESRCH') return false; if (e.code !== 'EPERM') throw e; }
  if (group.running()) return true;
  // A keeper exits only once its group is empty, and nothing joins an empty group: asked again, that group has ended.
  // One that is not empty has lost its keeper, and its id may now be another program's.
  try { process.kill(-group.pid, 0); } catch (e) { if (e.code === 'ESRCH') return false; }
  throw new Error(`server shutdown cannot verify ownership of process group ${group.pid}: its keeper exited`);
}
/** Kills the group at once and asks its supervisor to finish, for a process that is ending. */
function killGroup(group) { try { signalGroup(group, 'SIGKILL'); } finally { group.end?.(); } }
async function portFree(server) {
  const host = new URL(server.url).hostname.replace(/^\[|\]$/g, '');
  const hosts = host === 'localhost' ? ['127.0.0.1', '::1'] : [host];
  for (const host of hosts) {
    const free = await new Promise((done) => {
      const probe = createServer();
      probe.once('error', e => done(host === '::1' && ['EAFNOSUPPORT', 'EADDRNOTAVAIL'].includes(e.code)));
      probe.listen({ host, port: server.port, exclusive: true }, () => probe.close(() => done(true)));
    });
    if (!free) return false;
  }
  return true;
}
/** Ends the server's group, then its supervisor (which on Linux ends the processes that left the group), and waits for
 * the server's port. Retryable: it fails naming what survives, and closes the group's job handle only once all ended. */
async function stopGroup(group, server) {
  if (!group) throw new Error('server shutdown cannot verify process ownership');
  const deadline = Date.now() + 3000;
  if (groupAlive(group)) signalGroup(group, 'SIGTERM');
  while (groupAlive(group) && Date.now() < deadline) await Bun.sleep(25);
  const forcedDeadline = Date.now() + 3000;
  if (groupAlive(group)) signalGroup(group, 'SIGKILL');
  const unfinished = () => new Error(`server shutdown did not finish: ${group.job ? 'the job of supervisor' : 'process group'} ${group.pid} is still alive or port ${server?.port} is still in use`);
  while (groupAlive(group)) {
    if (Date.now() >= forcedDeadline) throw unfinished();
    await Bun.sleep(25);
  }
  // The supervisor's end and the port's release each have their own time after the group's.
  await group.release?.(Date.now() + 3000);
  const portDeadline = Date.now() + 3000;
  while (server && !(await portFree(server))) {
    if (Date.now() >= portDeadline) throw unfinished();
    await Bun.sleep(25);
  }
  group.close?.();
}

async function started(dir, server, name, forward, give, takeOwnership) {
  if (await answering(server.url)) throw new Error(heldLine(server));
  const runDir = resolve(dir, '.exact', name), db = server.database ? resolve(runDir, 'db.sqlite') : null, owner = resolve(runDir, 'owner');
  const clearDB = () => { if (db) for (const end of ['', '-wal', '-shm', '-journal']) rmSync(`${db}${end}`, { force: true }); };
  mkdirSync(runDir, { recursive: true });
  clearDB();
  // Its keepers' records of their ends; one left by a run that ended, its server with it, vouches for no later keeper.
  for (const file of readdirSync(runDir)) if (file.startsWith('ended-')) rmSync(resolve(runDir, file), { force: true });
  writeFileSync(owner, `${process.pid}\n`);
  takeOwnership();
  let child = null, group = null, said = { text: '' }, used = false, stopped = false, recorded = false;
  const deaths = [];
  const rememberExit = () => {
    if (exitStatus(child) == null || recorded) return;
    deaths.push(`${server.entry} exited (${exitReason(child)}) during the run${tail() ? `: ${tail()}` : ''}`);
    recorded = true;
  };
  const tail = () => lastError(said.text);
  const endChild = async () => { if (child) { rememberExit(); recorded = true; if (group) await stopGroup(group, server); await child.exited; child = null; } };
  const launch = async () => {
    const buffer = { text: '' }; said = buffer; recorded = false;
    group = null;
    child = spawnServer(server, dir, serverEnv(server, db), 'pipe', true, runDir);
    group = await launchedGroup(child);
    const launched = child;
    (async () => { for await (const chunk of launched.stderr) { if (child !== launched) continue; buffer.text = (buffer.text + new TextDecoder().decode(chunk)).slice(-4000); if (forward) process.stderr.write(chunk); } })().catch(() => {});
    const why = await untilAnswers(server, child, tail, group);
    if (why) { await endChild(); throw new Error(why); }
  };
  // Before its supervisor has said it started, POSIX still has the launch's group; Windows has no job yet.
  // Before the supervisor has said the server runs, it ends with the end of its stdin, as when this process exits.
  const off = whenEnded(() => { if (child && group) killGroup(group); });
  const stop = async () => {
    if (stopped) return;
    await endChild();
    clearDB(); rmSync(owner, { force: true }); give();
    stopped = true; off();
    if (name !== 'verify') try { rmdirSync(runDir); } catch {}
  };
  const fresh = async () => { await endChild(); clearDB(); used = false; await launch(); };
  try { await launch(); } catch (e) { await stop(); throw e; }
  return {
    stop, fresh, tail, exited: () => exitStatus(child),
    exits: () => { rememberExit(); return [...deaths]; },
    beforeTest: async () => { if (used || !child || exitStatus(child) != null) await fresh(); used = true; },
    check: () => { rememberExit(); if (group) groupAlive(group); if (exitStatus(child) != null) throw new Error(`${server.entry} exited (${exitReason(child)}) during the test${tail() ? `: ${tail()}` : ''}`); },
  };
}

/** The app's server for one `agent` drive (LLP 1116.008 D1): null when the app has none, or the URL it calls already answers
 * (another drive's: used, said only when on another database; one no drive started, the builder's or another program's:
 * used, and one line names its pid and command); else started where app.ts calls it, on a database of the
 * drive's own, `.exact/agent/<storage>/db.sqlite` kept for the `--storage` name so a second drive on it sees the first's data
 * (without one, an empty database removed after; `fresh`, a drive that starts its store empty, empties it first, and is
 * refused while another drive or the builder's server is using the app server), and one line says so. `{ stop() }` ends it. Drives that overlap share the
 * server: each is a `drive-<pid>` file in `.exact/agent-server/`, the servers a drive started are `server-<pid>` (their
 * database as the file's text), and the last drive out stops them; one that did not start the server stops nothing, and the
 * builder's own is never in the list. With database: false, no database is passed, created or cleared; otherwise an entry
 * that does not read `DB` is refused (`appServer`'s `refusal`), as `test` refuses it. */
export async function driveServer(dir, storage, say = (line) => console.error(line), { fresh = false } = {}) {
  const server = appServer(dir);
  if (!server?.url) return null;
  if (server.refusal) throw new Error(`agent: ${server.refusal}`);
  const lease = resolve(dir, '.exact/agent-server'), mine = resolve(lease, `drive-${process.pid}`), starting = resolve(lease, 'starting');
  const db = !server.database ? null : storage ? resolve(dir, '.exact/agent', storage.replace(/[^\w.-]+/g, '-'), 'db.sqlite') : resolve(lease, 'db.sqlite');
  const files = (kind) => { try { return readdirSync(lease).filter((f) => f.startsWith(`${kind}-`)).map((f) => [resolve(lease, f), Number(f.slice(kind.length + 1))]); } catch { return []; } };
  let spawned = null, spawnedGroup = null;
  mkdirSync(lease, { recursive: true });
  writeFileSync(mine, '');
  // A refused drive may have found a copied record. Ending it must not lend that record authority to stop a process.
  const leave = () => {
    rmSync(mine, { force: true });
    for (const [file, pid] of files('drive')) if (!alive(pid)) rmSync(file, { force: true });
    if (files('drive').length) return null;
    return files('server').flatMap(([file, pid]) => {
      // Overlapping drives may choose different ports; each record names the listener to validate.
      let recordedServer = null, token = null;
      try {
        const record = JSON.parse(readIn(lease, file)), address = recordedServerAddress(record?.origin);
        if (address && typeof record.token === 'string') { recordedServer = { ...server, ...address }; token = record.token; }
      } catch { /* An unknown record grants no authority to stop a process. */ }
      const mine = spawnedGroup?.pid === pid, ours = mine || recordedServer && ownsServer(dir, recordedServer, pid);
      if (!ours && alive(pid)) return [];
      if (!ours) {
        // Its keeper is gone. Kept, with the lease and its database, while what the server started may still run: its
        // group has members, or on Linux the keeper did not record that it ended what left the group.
        let members = false; try { process.kill(-pid, 0); members = true; } catch (e) { members = e.code === 'EPERM'; }
        if (process.platform !== 'win32' && (members || process.platform === 'linux' && !recordedEnd(lease, pid, token))) return [];
        // Or while something still listens where it served: on macOS a process that left its group may.
        if (recordedServer && portHolder(recordedServer).pid) return [];
        rmSync(file, { force: true }); rmSync(endRecord(lease, pid), { force: true });
        return [];
      }
      return [{ file, group: mine ? spawnedGroup : processGroup(pid, lease, token), server: recordedServer ?? server }];
    });
  };
  const clean = () => {
    if (files('server').length) return;
    if (db) rmSync(lease, { recursive: true, force: true });
    else { for (const [file] of [...files('ended'), [resolve(lease, 'log')]]) rmSync(file, { force: true }); try { rmdirSync(lease); } catch {} }
  };
  // A launch whose supervisor has not said the server runs is ended through it: the keeper ends with its supervisor.
  // A launch in flight keeps the lease: its keeper, ending with its supervisor, may not yet have ended the server.
  const off = whenEnded(() => { const launching = spawned && !spawnedGroup; if (launching) spawned.kill('SIGKILL'); const ending = leave(); for (const { file, group } of ending ?? []) try { killGroup(group); rmSync(file, { force: true }); } catch {} if (ending && !launching) clean(); });
  const stop = async () => {
    const ending = leave();
    for (const { file, group, server } of ending ?? []) {
      await stopGroup(group, server);
      rmSync(file, { force: true });
    }
    off();
    if (ending) clean();
  };
  // One drive starts the server at a time: another waits, then finds it answering.
  let locked = false;
  for (let i = 0; i < 150 && !locked; i++) {
    try { writeFileSync(starting, String(process.pid), { flag: 'wx' }); locked = true; }
    catch { const owner = Number(readIn(lease, 'starting')); if (owner && !alive(owner)) rmSync(starting, { force: true }); else await Bun.sleep(100); }
  }
  try {
    // A store asked empty cannot be emptied under a server another drive (or the builder) is using: refused, and this
    // drive's lease given up, never the database deleted or kept as it was.
    const others = files('drive').filter(([file, pid]) => file !== mine && alive(pid)).map(([, pid]) => pid);
    if (fresh && (others.length || await answering(server.url))) {
      off(); rmSync(mine, { force: true });
      throw new Error(`agent: --storage ${storage} is to start empty, but ${others.length ? `another drive (pid ${others.join(', ')}) is using` : 'something already answers at'} ${server.url}, the app server${db ? ', on its database' : ''}: let it end, or name another store`);
    }
    if (await answering(server.url)) {
      if (server.fixture && !ownsServer(dir, server)) {
        await stop();
        throw new Error(`agent: ${server.url} answers, but its fixture does not belong to ${realpathSync(dir)}; stop it to let agent start its own, or use --backend http://127.0.0.1:<free-port> or PORT=<free-port> for this checkout`);
      }
      const records = files('server').filter(([, pid]) => alive(pid)).map(([file]) => { try { return JSON.parse(readIn(lease, file)); } catch { return null; } }).filter((r) => r?.origin === server.url);
      const other = db && records.map((r) => r.db).find((was) => was && was !== db);
      if (!records.length) say(`agent: ${server.url}${portHolder(server).holder}, a server no drive started: this drive uses it and its data${server.database ? ', not a database of its own' : ''}; stop it to have the drive start ${server.entry} itself`);
      else if (other) say(`agent: ${server.entry} at ${server.url} is another drive's, on its database (${relative(dir, other)}); this drive uses it`);
      return { stop };
    }
    if (db) mkdirSync(dirname(db), { recursive: true });
    if (db && (!storage || fresh)) for (const end of ['', '-wal', '-shm', '-journal']) rmSync(`${db}${end}`, { force: true });
    const log = openSync(resolve(lease, 'log'), 'a');
    // A drive's server outlives the drive that started it while another drive uses it, so it has no owner.
    const child = spawnServer(server, dir, serverEnv(server, db), log, false, lease);
    closeSync(log);
    spawned = child;
    const failed = async (e) => {
      spawned = null;
      if (locked) { rmSync(starting, { force: true }); locked = false; }
      await stop(); throw e;
    };
    try { spawnedGroup = await launchedGroup(child); } catch (e) { await failed(e); }
    if (!spawnedGroup) await failed(new Error(`agent: ${await untilAnswers(server, child, () => lastError(readIn(lease, 'log') ?? ''), null)}`));
    writeFileSync(resolve(lease, `server-${spawnedGroup.pid}`), JSON.stringify({ directory: realpathSync(dir), origin: server.url, db, token: child.token }));
    const why = await untilAnswers(server, child, () => lastError(readIn(lease, 'log') ?? ''), spawnedGroup);
    if (why) { await stop(); throw new Error(`agent: ${why}`); }
    if (server.fixture && !ownsServer(dir, server, spawnedGroup.pid, spawnedGroup)) {
      await stop();
      throw new Error(`agent: ${server.url} answered before this checkout's fixture owned its port; choose another port with --backend or PORT`);
    }
    say(`agent: started ${server.entry} at ${server.url} for this drive (${!db ? 'no database' : storage ? `database ${relative(dir, db)}, kept for --storage ${storage}` : 'an empty database'}); stopped after${server.note ? `; ${server.note}` : ''}`);
    return { stop };
  } finally { if (locked) rmSync(starting, { force: true }); }
}

/** `fn` with the app's server running on an empty database, as verify runs it: its exit code, or 1 when the server cannot
 * run, said in one line. `bun exact.mjs test` runs its tests so (LLP 1116.007 D2: `optional`: an app with no server, or one
 * whose URL app.ts does not say, runs them as they are, the second said once per test file, and the tests use whatever answers there);
 * a server entry that needs a database but does not read `DB` is refused whatever the verb (LLP 1116.008 D1);
 * a server an ancestor process runs (an older app's api-test wrapper) stays shared, said in one line.
 * `api-test` is the same call without `optional`: it wants a server, and says why it has none. */
export async function withAppServer(dir, fn, { verb = 'api-test', optional = false } = {}) {
  const server = appServer(dir);
  const owners = server ? ['api-test', 'test', 'verify'].map((name) => readIn(dir, `.exact/${name}/owner`)?.trim()).filter(Boolean) : [];
  if (owners.length && ancestors().some((pid) => owners.includes(pid))) { console.error(`${verb}: using an ancestor’s server; tests share its data`); return fn(null); }
  let running;
  try {
    if (server?.refusal) throw new Error(server.refusal);
    if (!server || server.why) {
      if (!optional) throw new Error(server?.why ?? 'no server: server.ts, server/index.ts, server/server.ts, or app.json\'s commands.server');
      if (server) console.error(`${verb}: ${server.why}; tests share the data of whatever answers there`);
    } else {
      running = await startServer(dir, server, verb, { forward: true });
      console.error(`${verb}: ${serverRun(server, verb)}`);
    }
  } catch (e) { console.error(`${verb}: ${e.message}`); return 1; }
  let code;
  try { code = await fn(running); } finally { await running?.stop(); }
  const deaths = running?.exits() ?? [];
  for (const death of deaths) console.error(`${verb}: ${death}`);
  return deaths.length ? code || 1 : code;
}
