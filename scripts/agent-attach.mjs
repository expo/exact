// `--attach` (LLP 1012.000): the driver joins the development app a person already has open, a Mac app or an app in
// the iOS simulator, and reads the window they allow over the app's own socket. It builds, installs and launches
// nothing, and holds no clock.
import { spawnSync } from 'node:child_process';
import { lstatSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { connect } from 'node:net';
import { basename, dirname, resolve } from 'node:path';
import { chooseSimulator } from '../host/apple/devices.mjs';
import { resolveApp } from './app.mjs';
import { attachWire } from './agent-wire.mjs';

/** Why `--attach` refuses a host or a target before any platform tool runs (LLP 1012.000 D2), or null where it
 *  attaches: a development macOS app, or an app in the iOS simulator. */
export function attachUnsupported(host, { device = false } = {}) {
  const why = device || host === 'phone' ? 'a physical iPhone keeps its app’s socket on the phone, out of this Mac’s reach, and a device build carries no attach'
    : host === 'tvos' ? 'an Apple TV build carries no attach'
    : host === 'watchos' ? 'a watch app carries no attach'
    : host === 'host-ios' || host === 'host' ? 'the embedding fixture carries no attach'
    : ['ios', 'macos', 'mac'].includes(host) ? null
    : `attach on ${host} is not built yet`;
  return why && `--attach: unsupported on ${device ? 'a physical iPhone' : host}: ${why}. An agent attaches to a development macOS app or an app in the iOS simulator (LLP 1012.000 D2)`;
}

const ps = (run, p, field) => run('ps', ['-o', `${field}=`, '-p', String(p)], { encoding: 'utf8', env: { ...process.env, LC_ALL: 'C' } }).stdout?.trim() ?? ''; // `lstart` in English, which Date.parse reads
const plist = (run, file, key) => run('plutil', ['-extract', key, 'raw', file], { encoding: 'utf8' }).stdout?.trim();

/** A file or socket only this user owns and reaches, and no link to one. */
function owned(path, kind) {
  try { const st = lstatSync(path); return st[kind]() && st.uid === process.getuid() && (st.mode & 0o077) === 0; } catch { return false; }
}

/** The markers in `dir` of `id`'s live processes: this user's own, naming a process started when it says that runs the
 *  executable it names, whose bundle (`plistOf(executable)`) says `id`. `ps` gives the start in whole seconds; a
 *  recycled PID starts later, or runs another executable. */
function liveMarkers(dir, id, plistOf, run) {
  const live = [];
  let files = [];
  try { files = readdirSync(dir).filter((f) => /^exact-attach-\d+\.json$/.test(f)); } catch { /* no directory: no markers */ }
  for (const file of files) {
    const path = resolve(dir, file);
    if (!owned(path, 'isFile')) continue;
    let m;
    try { m = JSON.parse(readFileSync(path, 'utf8')); } catch { continue; }
    if (m.v !== 1 || `exact-attach-${m.pid}.json` !== file || m.bundle !== id) continue;
    const started = Date.parse(ps(run, m.pid, 'lstart')) / 1000, info = plistOf(String(m.executable));
    if (ps(run, m.pid, 'comm') === m.executable && Math.abs(started - Math.floor(m.started)) <= 1 && info
      && plist(run, info, 'CFBundleIdentifier') === id) live.push(m);
  }
  return live;
}

/** The one marker `pid` chooses among `live`, its socket checked; or null. */
function chosenMarker(live, pid, name) {
  const chosen = (pid == null ? live : live.filter((m) => m.pid === pid)).sort((a, b) => a.pid - b.pid);
  if (chosen.length > 1) throw new Error(`attach: ${chosen.length} running ${name} processes have Agent Attach on (PIDs ${chosen.map((m) => m.pid).join(', ')}): name one, --attach <pid>`);
  const [m] = chosen;
  if (m && (!owned(m.socket, 'isSocket') || !owned(dirname(m.socket), 'isDirectory'))) throw new Error(`attach: ${m.socket} is not a socket in a directory only you can reach; not connecting`);
  return m ?? null;
}

/** Each running process whose executable `match` reads (`[pid, info.plist, …]`) and whose bundle says `id`. */
function running(id, pid, match, run) {
  return (run('ps', ['-axo', 'pid=,comm='], { encoding: 'utf8' }).stdout ?? '').split('\n').flatMap((line) => {
    const found = match(line.trim());
    if (!found || plist(run, found.info, 'CFBundleIdentifier') !== id || (pid != null && found.pid !== pid)) return [];
    return [{ ...found, purpose: plist(run, found.info, 'ExactBuildPurpose'), kind: plist(run, found.info, 'ExactBuildKind') || 'unstamped' }];
  });
}

/** The running development Mac app `--attach` joins (LLP 1012.000 D1): a marker Develop ▸ Enable Agent Attach wrote
 * beside `trace --pid`'s, naming a live process of the app, and a socket only this user can reach. Several match: a
 * PID chooses. None: why, as far as the running processes say. */
export function attachTarget({ id, name, app, pid = null, run = spawnSync, dir = run('getconf', ['DARWIN_USER_TEMP_DIR'], { encoding: 'utf8' }).stdout?.trim() }) {
  const infoOf = (exe) => { const bundle = exe.match(/^(.*\.app)\/Contents\/MacOS\/[^/]+$/)?.[1]; return bundle && resolve(bundle, 'Contents/Info.plist'); };
  const m = chosenMarker(dir ? liveMarkers(dir, id, infoOf, run) : [], pid, name);
  if (m) return m;
  const apps = running(id, pid, (line) => {
    const [, p, exe] = line.match(/^(\d+) (.*\.app)\/Contents\/MacOS\/[^/]+$/) ?? [];
    return p && { pid: Number(p), info: resolve(exe, 'Contents/Info.plist') };
  }, run);
  const open = `open a development build (\`bun exact.mjs mac --run\` in the app, or \`bun host/apple/build.mjs ${app} --run\` in exact2, or the .app either made), then choose Develop ▸ Enable Agent Attach in its window`;
  if (!apps.length) throw new Error(pid != null ? `attach: pid ${pid} is not a running ${name} (${id}) on this Mac` : `attach: no ${name} (${id}) is running on this Mac: ${open}`);
  throw new Error('attach: ' + apps.map((r) => r.purpose === 'local-development'
    ? `${name} pid ${r.pid} has not turned Agent Attach on: choose Develop ▸ Enable Agent Attach in its window`
    : `${name} pid ${r.pid} is a ${r.kind} build, which carries no agent attach: ${open}`).join('; '));
}

/** An installed simulator app's executable: its device and its Info.plist. */
const simulatorApp = (exe) => {
  const [, bundle, udid] = exe.match(/^(.*\/Devices\/([0-9A-F-]{36})\/data\/Containers\/Bundle\/Application\/[0-9A-F-]{36}\/[^/]+\.app)\/[^/]+$/) ?? [];
  return bundle ? { udid, info: resolve(bundle, 'Info.plist') } : null;
};
/** `ps`'s line for a simulator app's process: its PID, device and Info.plist. */
const simulatorProcess = (line) => {
  const [, p, exe] = line.match(/^(\d+) (.*)$/) ?? [];
  const found = p && simulatorApp(exe);
  return found ? { pid: Number(p), ...found } : null;
};

/** The running development app in the selected iOS simulator that `--attach` joins (LLP 1012.000 D1, D2): the
 * simulator `--sim` or `EXACT_SIM` names, else the one a drive of the app in `appDir` would choose, booting nothing; the marker the
 * developer menu's Enable Agent Attach wrote in the app's container `tmp`, which `simctl get_app_container` names; a
 * live process of the app on that device. An Apple TV, a watch or a simulator that is not running is refused. */
export function attachTargetIOS({ id, name, app, appDir, pid = null, run = spawnSync, choose = chooseSimulator }) {
  const dev = choose(undefined, { app: id, dir: appDir, hint: '--sim <udid|name> (or EXACT_SIM)' });
  const other = /SimRuntime\.(tvOS|watchOS|xrOS)/.exec(dev.runtime)?.[1];
  if (other) throw new Error(`--attach: unsupported on ${other} (simulator ${dev.name}): ${{ tvOS: 'an Apple TV build', watchOS: 'a watch app', xrOS: 'a visionOS app' }[other]} carries no attach. An agent attaches to a development macOS app or an app in the iOS simulator (LLP 1012.000 D2)`);
  const device = `simulator ${dev.name} (${dev.udid})`;
  const open = `build and open it there (\`bun exact.mjs ios --run\` in the app, or \`bun host/apple/build.mjs ${app} --ios --run\` in exact2), then open the developer menu (four fingers, or ⌘D) and choose Enable Agent Attach`;
  if (dev.state !== 'Booted') throw new Error(`attach: ${device} is not running: ${open}`);
  const container = run('xcrun', ['simctl', 'get_app_container', dev.udid, id, 'data'], { encoding: 'utf8' });
  const data = container.status === 0 ? container.stdout?.trim() : '';
  const onDevice = (exe) => { const found = simulatorApp(exe); return found?.udid === dev.udid ? found.info : null; };
  const m = data ? chosenMarker(liveMarkers(resolve(data, 'tmp'), id, onDevice, run), pid, name) : null;
  if (m) return { ...m, device: dev };
  const apps = running(id, pid, simulatorProcess, run), here = apps.filter((r) => r.udid === dev.udid), elsewhere = apps.filter((r) => r.udid !== dev.udid);
  if (!here.length && elsewhere.length) {
    throw new Error(`attach: ${name} is not running in the selected ${device}; it runs in ${[...new Set(elsewhere.map((r) => r.udid))].join(', ')}: name that one with --sim <udid>`);
  }
  if (!data) throw new Error(`attach: ${name} (${id}) is not installed in ${device}: ${open}`);
  if (!here.length) throw new Error(pid != null ? `attach: pid ${pid} is not a running ${name} (${id}) in ${device}` : `attach: ${name} (${id}) is not running in ${device}: ${open}`);
  throw new Error('attach: ' + here.map((r) => r.purpose === 'local-development'
    ? `${name} pid ${r.pid} in ${device} has not turned Agent Attach on: open its developer menu (four fingers, or ⌘D) and choose Enable Agent Attach`
    : `${name} pid ${r.pid} in ${device} is a ${r.kind} build, which carries no agent attach: ${open}`).join('; '));
}

/** What an attached session refuses as input before it reaches the app. */
const ATTACHED_INPUT = new Set(['tap', 'type', 'pick', 'prefer', 'call', 'post', 'share', 'midi', 'resize', 'back', 'close', 'background']);

/** The attached carrier (LLP 1012.000 D1): the development app a person has open, a Mac app or one in the iOS
 * simulator, read over its own socket once the person allows it in the app. It reads; it launches nothing and holds
 * no clock. */
export async function openAttached({ host, app, pid = null, session, device = false, find = host === 'ios' ? attachTargetIOS : attachTarget, dial = connect, say = (line) => console.error(line) }) {
  const refused = attachUnsupported(host, { device });
  if (refused) throw new Error(refused);
  const a = resolveApp(app), name = a.manifest?.name ?? a.name;
  const m = find({ id: a.id, name, app: basename(a.dir), appDir: a.dir, pid });
  const where = m.device ? ` in simulator ${m.device.name}` : '';
  const wire = await attachWire(dial({ path: m.socket }), { nonce: m.nonce, session, onWaiting: () => say(`attach: waiting for Allow in ${name}${where} (pid ${m.pid})`) }).catch((e) => {
    // iOS suspends an app in the background, listener and all, until it is opened again.
    if (m.device && /did not answer the handshake/.test(e.message)) e.message = `attach: ${name}${where} did not answer: iOS suspends an app in the background, so bring it to the front in the simulator and run again`;
    throw e;
  });
  const { attached } = wire, build = attached.build ?? {};
  if (attached.pid !== m.pid) { wire.close(); throw new Error(`attach: the socket of pid ${m.pid} answered as pid ${attached.pid}; not reading it`); }
  say(`attach: reading ${name}${where} pid ${attached.pid}, window “${attached.window}” (session ${attached.session}); built from ${build.commit ? `${build.commit.slice(0, 9)}${build.dirty ? ' (dirty)' : ''}` : 'an unstamped commit'}; reads only, on its wall clock`);
  return {
    host: host === 'mac' ? 'macos' : host, attached, hostLines: [], plan: null, sessions: null, state: null,
    // What the app would refuse as input or clock control is refused here, before it is sent.
    ask: (req) => (req?.op === 'clock' ? Promise.reject(new Error('clock: live-clock: an attached app keeps its own wall clock'))
      : ATTACHED_INPUT.has(req?.op) ? Promise.reject(new Error(`${req.op}: input-not-granted: an attached session reads; input needs Allow Agent Input, which this build does not offer yet`))
      : wire.ask(req)),
    async screenshot(path, area, film, opts = {}) {
      if (film) throw new Error('screenshot: film of an attached app is not built yet (LLP 1012.000 piece 4)');
      if (area !== false && area !== true || opts.status || opts.own) throw new Error('screenshot: an attached app gives its window\'s picture alone');
      const r = await wire.ask({ op: 'screenshot' });
      if (r.error) throw Object.assign(new Error(`screenshot: ${r.error}`), { reply: r });
      const bytes = Buffer.from(r.data, 'base64');
      if (bytes.length !== r.bytes) throw new Error('screenshot: the picture arrived short');
      writeFileSync(path, bytes);
      const { data, ...rest } = r;
      return { ...rest, screenshot: path };
    },
    close: async () => wire.close(),
  };
}
