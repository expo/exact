// `--attach` (LLP 1012.000): the driver joins the development app a person already has open, a Mac app or an app in
// the iOS simulator, and reads the window they allow over the app's own socket. It builds, installs and launches
// nothing, and holds no clock.
import { spawnSync } from 'node:child_process';
import { lstatSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { connect } from 'node:net';
import { basename, dirname, resolve } from 'node:path';
import { chooseSimulator } from '../host/apple/devices.mjs';
import { openTouches, realTap } from '../host/apple/touches.mjs';
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

/** Input an attached session never sends; and what it sends for the app to check against the person's grant. */
const WITHHELD = new Set(['pick', 'prefer', 'post', 'share', 'midi', 'resize', 'back', 'close', 'background']);
const GRANTED = new Set(['tap', 'type', 'call', 'called']);

/** The attached carrier (LLP 1012.000 D1): the development app a person has open, a Mac app or one in the iOS
 * simulator, read over its own socket once the person allows it in the app. It reads; it launches nothing and holds
 * no clock. */
export async function openAttached({ host, app, pid = null, session, device = false, find = host === 'ios' ? attachTargetIOS : attachTarget, dial = connect, say = (line) => console.error(line), grantMs = 60000 }) {
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
  say(`attach: reading ${name}${where} pid ${attached.pid}, window “${attached.window}” (session ${attached.session}); built from ${build.commit ? `${build.commit.slice(0, 9)}${build.dirty ? ' (dirty)' : ''}` : 'an unstamped commit'}; on its wall clock; input once the person chooses Allow Agent Input`);
  // In the simulator a granted tap is a real touch the touch runner plays (touches.mjs), opened at the first one.
  let runner = null;
  // Input the app checks against the person's grant. Refused for want of it, it took no effect, so it is sent again
  // once the person chooses Allow Agent Input, within `grantMs`. A reply lost once it was sent leaves its outcome
  // unknown, and it is never sent again.
  const send = (req) => wire.ask(req).catch((e) => {
    if (e.sent) e.message = `${req.op}: the connection ended after the request was sent (${e.message}); whether it took effect is unknown, and it was not sent again`;
    throw e;
  });
  // A call the app queued: its record once the runner has taken it from its queue, within 10 s. One still queued when
  // the connection ends is cancelled by the app, never run later, so the driver waits for it to leave the queue.
  const dispatched = async (r) => {
    if (r.error || !Number.isInteger(r.call) || r.state !== 'waiting') return r;
    for (const end = Date.now() + 10000; Date.now() < end;) {
      await new Promise((ok) => setTimeout(ok, 100));
      const record = await send({ op: 'called', id: r.call });
      // An ended call's record is LLP 1111 D4's envelope, which has no `state`.
      if (record.error || record.state !== 'waiting') return { ...r, state: 'ended', ...record, call: r.call };
    }
    return r;
  };
  const granted = async (req) => {
    let r = await send(req);
    if (r.refused !== 'input-not-granted') return r;
    say(`attach: waiting for Allow Agent Input in ${name}${where}`);
    for (const end = Date.now() + grantMs; Date.now() < end && r.refused === 'input-not-granted';) {
      await new Promise((ok) => setTimeout(ok, 250));
      // A grant seen past the deadline sends nothing: the person's moment to allow it is the deadline.
      if ((await wire.ask({ op: 'grants' })).input && Date.now() < end) r = await send(req);
    }
    return r;
  };
  const carrier = {
    host: host === 'mac' ? 'macos' : host, attached, hostLines: [], plan: null, sessions: null, state: null,
    // What the app would refuse as input or clock control is refused here, before it is sent. Input the app checks
    // against the person's grant: a reply lost once it was sent leaves its outcome unknown, and it is never sent again.
    ask: (req) => (req?.op === 'clock' ? Promise.reject(new Error('clock: live-clock: an attached app keeps its own wall clock'))
      : WITHHELD.has(req?.op) ? Promise.reject(new Error(`${req.op}: unsupported: an attached session's input is a plain tap, a text entry and a declared tool's call`))
      : req?.op === 'call' ? granted(req).then(dispatched)
      : GRANTED.has(req?.op) ? granted(req)
      : wire.ask(req)),
    input: null,
    async screenshot(path, area, film, opts = {}) {
      if (film) throw new Error('screenshot: film of an attached app is not built yet: it would sample the wall clock (LLP 1012.000 D3)');
      if (area !== false && area !== true || opts.status || opts.own) throw new Error('screenshot: an attached app gives its window\'s picture alone');
      const r = await wire.ask({ op: 'screenshot' });
      if (r.error) throw Object.assign(new Error(`screenshot: ${r.error}`), { reply: r });
      const bytes = Buffer.from(r.data, 'base64');
      if (bytes.length !== r.bytes) throw new Error('screenshot: the picture arrived short');
      writeFileSync(path, bytes);
      const { data, ...rest } = r;
      return { ...rest, screenshot: path };
    },
    close: async () => { wire.close(); if (runner) await (await runner.catch(() => null))?.close(); },
    // A wait's read still out at its deadline: the connection ends, so the late reply answers nothing.
    abandon: (why) => wire.abandon(why),
    get failure() { return wire.failure; },
  };
  // The runner is the input's alone, opened at the first simulator tap: never handed out, so no touch is played but
  // after an aim the app granted. Another drive of the simulator may be letting go of it: its lock is waited for.
  const touches = () => (runner ??= (async () => {
    for (const end = Date.now() + 10000; ;) {
      try { return await openTouches({ udid: m.device.udid, appId: a.id }); }
      catch (e) { if (!/touches are held by PID/.test(e.message) || Date.now() > end) throw e; await new Promise((ok) => setTimeout(ok, 500)); }
    }
  })().catch((e) => { runner = null; throw e; }));
  carrier.input = attachedInput({ host: carrier.host, ask: carrier.ask, abandon: carrier.abandon, touches });
  return carrier;
}

/** An attached session's input (LLP 1012.000 D3): a plain tap and a whole text entry at a target, a test ID or `#<id>`,
 *  which the app resolves in the window the person allowed and refuses without their input grant. On the Mac the app
 *  clicks and sets the text itself. In the simulator the app aims and the touch runner (`touches`) plays the touch
 *  (`realTap`), every aim naming the target as given and the window's dispatch log checking it landed; a text entry
 *  is checked to be an editable field before its tap and to have the keyboard before its keystrokes, which go at the
 *  caret. One input at a time. A failure once a touch may have been played is reported as of unknown outcome. */
export function attachedInput({ host, ask, abandon, touches }) {
  const answered = (op, r) => { if (r.error) throw Object.assign(new Error(`${op}: ${r.error}`), { reply: r }); return r; };
  const targetOf = (t) => (/^#\d+$/.test(String(t)) ? Number(String(t).slice(1)) : String(t));
  let queue = Promise.resolve();
  const one = (run) => (queue = queue.catch(() => {}).then(run));
  const touch = async (target) => {
    const first = answered('tap', await ask({ op: 'tap', target, aim: { press: true } }));
    const runner = await touches();
    const refusals = new Set();
    // `realTap` asks by view id; each aim here names the target as given, and one that resolves elsewhere is refused.
    const aimed = async (req) => {
      if (req.log !== undefined) return ask({ op: 'tap', log: req.log });
      const r = await ask({ op: 'tap', target, aim: { press: true } });
      if (!r.error && r.id !== first.id) return { error: `target: ${target} is now view #${r.id}, not #${first.id}; nothing was played` };
      if (r.error) refusals.add(r.error);
      return r;
    };
    try { return await realTap({ ask: aimed, touches: runner, id: first.id, abandon }); }
    catch (e) {
      if (refusals.has(e.message)) throw Object.assign(new Error(`tap: ${e.message}`), { reply: { refused: true } });
      throw new Error(`tap: ${e.message}; the touch may have been played, so whether it took effect is unknown, and it was not played again`);
    }
  };
  return {
    tap: (target, opts = {}) => one(async () => {
      const forms = Object.keys(opts ?? {}).filter((k) => opts[k] !== undefined && opts[k] !== false);
      if (forms.length) throw new Error(`tap: unsupported: an attached session taps plainly, with no ${forms.join(', ')}`);
      const t = targetOf(target);
      return host === 'ios' ? touch(t) : answered('tap', await ask({ op: 'tap', target: t }));
    }),
    type: (target, text) => one(async () => {
      if (typeof text !== 'string') throw new Error('type: unsupported: an attached session types a whole text entry, with no key, clipboard or message');
      const t = targetOf(target);
      if (host !== 'ios') return answered('type', await ask({ op: 'type', target: t, text }));
      // The runner types at most 500 characters, refused here before the tap acts.
      if ([...text].length > 500) throw new Error('type: in the simulator an attached text entry is at most 500 characters');
      answered('type', await ask({ op: 'type', target: t, editable: true }));
      const tapped = await touch(t);
      answered('type', await ask({ op: 'type', target: t, focused: true }));
      const r = await (await touches()).ask({ op: 'type', text });
      if (r.error) throw new Error(`type: the touch runner: ${r.error}; part of the text may have been typed`);
      return { typed: tapped.tapped, delivery: 'platform', at: 'caret', note: 'in the simulator the text is typed where the tap left the caret; it does not replace the field\'s text' };
    }),
  };
}
