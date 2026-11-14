// The app server's discovery, scratch databases and process lifetime.
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:net';
import { closeSync, mkdirSync, openSync, readFileSync, readdirSync, realpathSync, rmdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { constants, networkInterfaces } from 'node:os';
import { takePort } from './port-lock.mjs';
import { alive, ancestors, commandLine } from './processes.mjs';
import { backendOriginSelection, privateNetwork } from '../js/bake/src/typescript.mjs';
import { inlineSettings } from '../js/bake/src/__exact_env.mjs';
import { importsOf, readsDB } from './app-imports.mjs';

// The app's own server (LLP 1116.006 D3). A local origin the data module calls, with any path under it.
const LOCAL = /\bhttps?:\/\/(?:127\.0\.0\.1|localhost|\[::1\]):\d{2,5}(?:\/[\w\-./]*)?/, LOCAL_SOCKET = new RegExp(LOCAL.source.replace('https?', 'wss?'));
const readIn = (dir, path) => { try { return readFileSync(resolve(dir, path), 'utf8'); } catch { return null; } };

/** Whether `origin` is served from this machine: a loopback address, or a private-network one that names
 *  one of this machine's interfaces (a phone's development build reaches the checkout's fixture there). */
export function onThisMachine(origin) {
  return /^https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?::\d+)?$/.test(origin)
    || privateNetwork(origin) && Object.values(networkInterfaces()).flat().some(n => n?.address === new URL(origin).hostname.replace(/^\[|\]$/g, ''));
}

/** app.ts as the build has it (LLP 1153 D4), each setting read replaced by what `inlineSettings` gives under `env` (a
 * default included), and those values; as written, with none, where the bake refuses it. */
function builtSettings(dir, env, code = readIn(dir, 'app.ts')) {
  try { const { code: built, record } = inlineSettings(code, dir, createRequire(import.meta.url)('rolldown/utils').parseSync, env); return { code: built, values: Object.fromEntries(Object.entries(record).map(([name, s]) => [name, s.value])) }; } catch { return { code, values: {} }; }
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
/** The server URL the data module calls: a setting's value that is a local URL (`http://127.0.0.1:8787`, localhost), as the
 * build inlines it; else the first local one in app.ts's code (not its comments), else in a module of its own it imports
 * (`./api`'s `API`); with no http one, a socket's origin (`ws://…`) is its server's. Null with none. */
export function serverUrl(dir, env = process.env) { return serverChoice(dir, env).url; }

/** `serverUrl`'s answer as `{ url, note }`: `note` says which origin it took and why when the code names more than one. */
export function serverChoice(dir, env = process.env) {
  const { code, values } = builtSettings(dir, env);
  const texts = [['app.ts', uncommented(code, 'app.ts')], ...importsOf(dir, 'app.ts').map((p) => [p, uncommented(readIn(dir, p), p)])];
  const all = (re) => texts.flatMap(([path, t]) => [...(t ?? '').matchAll(new RegExp(re.source, 'g'))].map((m) => ({ path, url: m[0] })));
  const found = [...all(LOCAL), ...all(LOCAL_SOCKET)].map((f) => ({ ...f, url: f.url.replace(/\/$/, '').replace(/^ws/, 'http') }));
  const setting = Object.entries(values).map(([name, value]) => ({ name, url: String(value).match(LOCAL)?.[0] ?? String(value).match(LOCAL_SOCKET)?.[0] })).find((s) => s.url);
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

// app.ts's settings as `appServer` read them: a server importing it (site's) listens where it looked, as Bun's .env loading may not.
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

/** A managed server needs this directory's launch record and the recorded process still listening. */
function ownsServer(dir, server, expectedPid, child = null) {
  const listeners = process.platform === 'win32'
    ? spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `(Get-NetTCPConnection -State Listen -LocalPort ${server.port} -ErrorAction SilentlyContinue).OwningProcess`], { encoding: 'utf8' }).stdout?.match(/\d+/g) ?? []
    : [...(spawnSync('lsof', ['-nP', `-iTCP:${server.port}`, '-sTCP:LISTEN', '-Fp'], { encoding: 'utf8' }).stdout?.matchAll(/^p(\d+)/gm) ?? [])].map(m => m[1]);
  const root = realpathSync(dir), lease = resolve(dir, '.exact/agent-server');
  for (const pid of listeners) {
    if (expectedPid !== undefined && Number(pid) !== expectedPid) continue;
    let record; try { record = JSON.parse(readFileSync(resolve(lease, `server-${pid}`), 'utf8')); } catch { continue; }
    if (!record || record.directory !== root || record.origin !== server.url || !alive(Number(pid))) continue;
    const command = commandLine(pid);
    if (!command.includes(server.entry)) continue;
    // CIM exposes no cwd. A copied record cannot vouch for one; only this invocation's child can.
    if (process.platform === 'win32') {
      if (child?.pid === Number(pid) && child.exitCode == null) return true;
      continue;
    }
    const cwd = spawnSync('lsof', ['-a', '-p', pid, '-d', 'cwd', '-Fn'], { encoding: 'utf8' }).stdout?.match(/^n(.+)$/m)?.[1];
    try { if (cwd && realpathSync(cwd) === root) return true; } catch { /* The process or its directory disappeared. */ }
  }
  return false;
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

// CreationDate distinguishes an observed Windows process from a later use of its PID.
function windowsProcesses() {
  const r = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'Get-CimInstance Win32_Process | ForEach-Object { if ($null -ne $_.CreationDate) { "$($_.ProcessId) $($_.ParentProcessId) $($_.CreationDate.ToUniversalTime().Ticks)" } }'], { encoding: 'utf8' });
  const table = new Map();
  for (const line of (r.stdout ?? '').split('\n')) {
    const [pid, parent, created] = line.trim().split(/\s+/);
    if (/^\d+$/.test(pid) && /^\d+$/.test(parent) && /^\d+$/.test(created)) table.set(Number(pid), { parent: Number(parent), created });
  }
  if (r.status !== 0 || !table.size) throw new Error('cannot read the Windows process table to stop the app server');
  return table;
}
function posixGroup(pid) {
  const r = spawnSync('ps', ['-A', '-o', 'pid=,pgid=,stat=,lstart='], { encoding: 'utf8' });
  if (r.status !== 0) throw new Error('cannot read the process table to stop the app server');
  return new Map((r.stdout ?? '').split('\n').flatMap(line => {
    const m = line.trim().match(/^(\d+)\s+(\d+)\s+(\S+)\s+(.+)$/);
    return m && Number(m[2]) === pid && !m[3].startsWith('Z') ? [[Number(m[1]), m[4]]] : [];
  }));
}
const windowsIdentity = (pid, created) => `${pid}:${created}`;
const windowsGroup = (pid, created) => ({ pid, members: new Map([[windowsIdentity(pid, created), { pid, created }]]) });
const processGroup = pid => {
  if (process.platform !== 'win32') return { pid, members: null, observed: posixGroup(pid) };
  const root = windowsProcesses().get(pid);
  if (!root) throw new Error(`cannot identify the Windows app server process ${pid}`);
  return windowsGroup(pid, root.created);
};
// CIM can lag a new launch. A query error is different from a root not yet listed.
async function launchedGroup(child) {
  try {
    if (process.platform !== 'win32') return processGroup(child.pid);
    const deadline = Date.now() + 1000;
    for (;;) {
      const root = windowsProcesses().get(child.pid);
      if (root) return windowsGroup(child.pid, root.created);
      if (exitStatus(child) != null || Date.now() >= deadline) throw new Error(`cannot identify the Windows app server process ${child.pid}`);
      await Bun.sleep(25);
    }
  } catch (e) {
    if (exitStatus(child) == null) child.kill('SIGKILL');
    await child.exited;
    throw e;
  }
}
function liveMembers(group) {
  const table = windowsProcesses();
  let added;
  do {
    added = false;
    for (const [pid, { parent, created }] of table) {
      const ancestor = table.get(parent), identity = windowsIdentity(pid, created);
      if (ancestor && group.members.has(windowsIdentity(parent, ancestor.created)) && BigInt(created) >= BigInt(ancestor.created) && !group.members.has(identity)) {
        group.members.set(identity, { pid, created }); added = true;
      }
    }
  } while (added);
  return [...group.members.values()].filter(({ pid, created }) => table.get(pid)?.created === created);
}
function signalGroup(group, signal, deadline = Date.now() + 3000) {
  if (!group) return;
  if (group.members) {
    const attempted = new Set();
    for (;;) {
      // Observation inserts parents before children. Stop one leaf, then observe children born during teardown.
      const member = liveMembers(group).findLast(({ pid, created }) => !attempted.has(windowsIdentity(pid, created)));
      if (!member) break;
      if (Date.now() >= deadline) throw new Error(`server shutdown did not finish: process group ${group.pid} is still alive`);
      const { pid, created } = member;
      attempted.add(windowsIdentity(pid, created));
      if (windowsProcesses().get(pid)?.created !== created) continue;
      spawnSync('taskkill', ['/PID', String(pid), '/F'], { stdio: 'ignore' });
    }
  } else if (groupAlive(group)) try { process.kill(-group.pid, signal); } catch (e) { if (e.code !== 'ESRCH') throw e; }
}
function groupAlive(group) {
  if (group.members) return liveMembers(group).length > 0;
  const current = posixGroup(group.pid);
  if (!current.size) return false;
  if (![...current].some(([pid, created]) => group.observed.get(pid) === created))
    throw new Error(`server shutdown cannot verify ownership of process group ${group.pid}: no observed member survives`);
  group.observed = current;
  return true;
}
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
async function stopGroup(group, server) {
  if (!group) throw new Error('server shutdown cannot verify process ownership');
  const deadline = Date.now() + 3000;
  if (groupAlive(group)) signalGroup(group, 'SIGTERM', deadline);
  while (groupAlive(group) && Date.now() < deadline) await Bun.sleep(25);
  const forcedDeadline = Date.now() + 3000;
  if (groupAlive(group)) signalGroup(group, 'SIGKILL', forcedDeadline);
  while (groupAlive(group) || (server && !(await portFree(server)))) {
    if (Date.now() >= forcedDeadline) throw new Error(`server shutdown did not finish: process group ${group.pid} is still alive or port ${server?.port} is still in use`);
    await Bun.sleep(25);
  }
}

async function started(dir, server, name, forward, give, takeOwnership) {
  if (await answering(server.url)) throw new Error(heldLine(server));
  const runDir = resolve(dir, '.exact', name), db = server.database ? resolve(runDir, 'db.sqlite') : null, owner = resolve(runDir, 'owner');
  const clearDB = () => { if (db) for (const end of ['', '-wal', '-shm', '-journal']) rmSync(`${db}${end}`, { force: true }); };
  mkdirSync(runDir, { recursive: true });
  clearDB();
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
    child = Bun.spawn(server.command ?? [process.execPath, server.entry, ...server.args], { cwd: dir, env: serverEnv(server, db), detached: true, stdin: 'ignore', stdout: 'ignore', stderr: 'pipe' });
    group = await launchedGroup(child);
    const launched = child;
    (async () => { for await (const chunk of launched.stderr) { if (child !== launched) continue; buffer.text = (buffer.text + new TextDecoder().decode(chunk)).slice(-4000); if (forward) process.stderr.write(chunk); } })().catch(() => {});
    const why = await untilAnswers(server, child, tail, group);
    if (why) { await endChild(); throw new Error(why); }
  };
  const off = whenEnded(() => { if (child) { if (group) signalGroup(group, 'SIGKILL'); else child.kill('SIGKILL'); } });
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
      let recordedServer = null;
      try {
        const address = recordedServerAddress(JSON.parse(readIn(lease, file))?.origin);
        if (address) recordedServer = { ...server, ...address };
      } catch { /* An unknown record grants no authority to stop a process. */ }
      const ours = spawned?.pid === pid || recordedServer && ownsServer(dir, recordedServer, pid);
      if (!ours && alive(pid)) return [];
      if (!ours) rmSync(file, { force: true });
      return ours ? [{ file, group: spawned?.pid === pid ? spawnedGroup : processGroup(pid), server: recordedServer ?? server }] : [];
    });
  };
  const clean = () => {
    if (files('server').length) return;
    if (db) rmSync(lease, { recursive: true, force: true });
    else { rmSync(resolve(lease, 'log'), { force: true }); try { rmdirSync(lease); } catch {} }
  };
  const off = whenEnded(() => { if (spawned && !spawnedGroup) spawned.kill('SIGKILL'); const ending = leave(); for (const { file, group } of ending ?? []) try { signalGroup(group, 'SIGKILL'); rmSync(file, { force: true }); } catch {} if (ending) clean(); });
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
        throw new Error(`agent: ${server.url} answers, but its fixture does not belong to ${realpathSync(dir)}; use --backend http://127.0.0.1:<free-port> or PORT=<free-port> for this checkout`);
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
    const child = Bun.spawn(server.command ?? [process.execPath, server.entry, ...server.args], { cwd: dir, env: serverEnv(server, db), detached: true, stdin: 'ignore', stdout: 'ignore', stderr: log });
    closeSync(log);
    spawned = child;
    try { spawnedGroup = await launchedGroup(child); }
    catch (e) {
      spawned = null;
      if (locked) { rmSync(starting, { force: true }); locked = false; }
      await stop(); throw e;
    }
    writeFileSync(resolve(lease, `server-${child.pid}`), JSON.stringify({ directory: realpathSync(dir), origin: server.url, db }));
    const why = await untilAnswers(server, child, () => lastError(readIn(lease, 'log') ?? ''), spawnedGroup);
    if (why) { await stop(); throw new Error(`agent: ${why}`); }
    if (server.fixture && !ownsServer(dir, server, child.pid, child)) {
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
