// Simulators, phones, signing identities and provisioning profiles: the
// device side of host/apple/build.mjs, shared with scripts/agent.mjs and the
// smokes, which launch the same bundles.
import { spawn, spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { constants, homedir, networkInterfaces, tmpdir } from 'node:os';
import { createServer as createTCPServer } from 'node:net';
import { basename, dirname, resolve } from 'node:path';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { decodePng, encodePng } from '../../scripts/png.mjs';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
/** Use Xcode when `xcode-select` names the Command Line Tools, which carry
 * no iOS SDK and no `simctl` (LLP 1054 O2): the iOS build failed deep in a
 * crate's build script with `SDK "iphonesimulator" cannot be located`, and a
 * driver that resolved Xcode apart from the build could not find `simctl`
 * where the build had just installed the app (shop F20, recipes F16). Every
 * `xcrun` here goes through it, the build's and the driver's alike; an
 * explicit `DEVELOPER_DIR` is kept. Said on stderr: a drive's stdout is its reply. */
export function useXcode() {
  if (process.platform !== 'darwin' || process.env.DEVELOPER_DIR) return;
  const selected = spawnSync('xcode-select', ['-p'], { encoding: 'utf8' }).stdout?.trim() ?? '', xcode = developerDir();
  if (xcode !== selected) {
    process.env.DEVELOPER_DIR = xcode;
    console.error(`host/apple: xcode-select names the Command Line Tools (${selected}); using ${xcode}`);
  }
}
/** The developer directory an Apple build under `env` uses (`useXcode`). */
export function developerDir(env = process.env) {
  if (env.DEVELOPER_DIR) return env.DEVELOPER_DIR;
  const selected = spawnSync('xcode-select', ['-p'], { encoding: 'utf8', env }).stdout?.trim() ?? '';
  const xcode = '/Applications/Xcode.app/Contents/Developer';
  return selected.includes('CommandLineTools') && existsSync(xcode) ? xcode : selected;
}

/** The oldest SDK each platform's Swift host compiles against: the newest
 * symbol it names, as the SDK declares it, behind `#available` or not (an
 * `@available` on the host's own code is no floor: it compiles on an older SDK
 * when every symbol it names is there). iOS: UIKit's `UIArrangementViewController`,
 * 27.1 (IOS/ArrangementIOS.swift). tvOS: `AVAudioSession.didBecomeInactiveNotification`
 * and its `DeactivationContext` and `ResumptionContext`, 27.0 (AudioSession.swift).
 * macOS: `NSGlassEffectView` (Mac/NodeViewMac.swift) and WKWebView's
 * `obscuredContentInsets` (webarm/WebArm.swift), 26.0; sound_engine.m compiles
 * the 27 SDKs' AVAudioEngine calls only from those SDKs. A newer SDK is no
 * promise that the host compiles; an older one certainly does not. Xcode N.M
 * carries the N.M SDKs. */
export const HOST_SDKS = { ios: '27.1', tvos: '27.0', macos: '26.0' };
/** Each SDK a build compiles with: its platform's floor and its name in a message. */
const SDK_NAMES = { iphoneos: ['ios', 'iOS'], iphonesimulator: ['ios', 'iOS Simulator'], appletvos: ['tvos', 'tvOS'], appletvsimulator: ['tvos', 'tvOS Simulator'], macosx: ['macos', 'macOS'] };
/** The SDK an Apple build compiles with: a device's or a simulator's (`device`), or the Mac's. */
export const appleSdkName = (platform, device) => ({ ios: device ? 'iphoneos' : 'iphonesimulator', tvos: device ? 'appletvos' : 'appletvsimulator', macos: 'macosx' })[platform];

/** Whether a build with `args` is `--embed` alone: what an embedding app links, with no Swift built. */
export function appleEmbedOnly(args) {
  return args.includes('--embed') && !args.includes('--archive') && !args.includes('--run') && !args.includes('--host');
}
/** Whether a build with `args` compiles the Swift host: neither `--embed` alone nor the Mac's painted carrier
 *  (`--painted-benchmark`, host/apple-painted) does. */
export const compilesSwiftHost = (args) => !appleEmbedOnly(args) && !(args.includes('--painted-benchmark') && !['--ios', '--device', '--tvos', '--watchos'].some((flag) => args.includes(flag)));

/** Whether dotted version `have` is older than `need` (`27.0.1` < `27.1`; a missing part is 0). */
export function olderVersion(have, need) {
  const a = have.split('.').map(Number), b = need.split('.').map(Number);
  for (let i = 0; i < Math.max(a.length, b.length); i++) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) < (b[i] ?? 0);
  return false;
}

/** Why the selected Xcode cannot compile the Swift host with SDK `sdkName`
 * (`appleSdkName`'s), or null when it can. An SDK older than the floor is
 * named, with the Xcode it needs and how to select one: it would fail deep in
 * the Swift compile, after the Rust build, on a missing symbol. An SDK whose
 * version cannot be read is said in xcrun's own words. `run` runs a command
 * (as `spawnSync`) in `developer`, the developer directory the build uses. */
export function xcodeRefusal(sdkName, run = (cmd, args) => spawnSync(cmd, args, { encoding: 'utf8' }), developer = developerDir()) {
  const [platform, label] = SDK_NAMES[sdkName], need = HOST_SDKS[platform];
  const probe = run('xcrun', ['--sdk', sdkName, '--show-sdk-version']), sdk = probe.status === 0 ? probe.stdout.trim() : '';
  if (/^\d+(\.\d+)*$/.test(sdk) && !olderVersion(sdk, need)) return null;
  const built = run('xcodebuild', ['-version']), xcode = `${built.status === 0 ? built.stdout.trim().split('\n')[0] : 'no Xcode'}, ${developer || 'none selected'}`;
  if (!/^\d+(\.\d+)*$/.test(sdk)) {
    const said = [...new Set(String(probe.stderr ?? '').split('\n').map(line => line.trim()).filter(line => /error/.test(line)))];
    return `host/apple: the selected Xcode (${xcode}) could not report its ${label} SDK: ${said.join('; ') || sdk || `xcrun exited ${probe.status ?? probe.error?.message}`}`;
  }
  return `host/apple: the selected Xcode (${xcode}) has the ${label} ${sdk} SDK; the Swift host needs the ${label} ${need} SDK, which Xcode ${need} and later carry. `
    + 'Select one for this build with DEVELOPER_DIR=/path/to/Xcode.app/Contents/Developer, or for every build with sudo xcode-select -s /path/to/Xcode.app';
}
const shellQuote = value => `'${value.replaceAll("'", "'\\''")}'`;
const read = (cmd, args, opts = {}) => { useXcode(); return spawnSync(cmd, args, { cwd: root, encoding: 'utf8', ...opts }); };

/** A window capture the app drew (`path`, in `r`'s scale), its pixels then the simulator's own screenshot's, as a
 *  device screenshot has them: `drawHierarchy` draws Liquid Glass and the other backdrop materials only roughly (a tab
 *  bar's glass left the rows under it legible, an alert's the list behind it: the euxy port's judged captures), and the
 *  software keyboard (round 9: "blank keyboard region") and the status bar are other processes' pixels. The rows of the
 *  status bar, its top `appTop` points, stay the app's when the caller did not ask for it (`r.statusBar` unset; with it
 *  set, `xcrun simctl status_bar <udid> override --time 9:41` sets what it shows).
 *
 *  Nothing of the display is read unless the app said it was in front (`r.front`: active, its scene in front, its
 *  window key), and nothing is taken unless `probe`, the app asked again after the reads, answers that it still is, at
 *  the same epoch, incarnation and clock. The display is read three times, the last `gap` ms after the second, so a
 *  caret's blink cannot fall in all three (`displayReads`). The whole display is taken when the three agree between
 *  the status bar and the home indicator (SpringBoard's, in the bottom safe area, which changes its shade on its own),
 *  UIKit's motion had settled (no `r.moving`), and at the points the app named (`r.opaque`: drawn by the app itself,
 *  no glass, keyboard or status bar there) the display shows what the app drew, so no notification banner or other
 *  overlay lies over it. Otherwise, as for a capture the agent marked (`r.unshown`, struck-through header nodes) or
 *  asked to keep (`own`), the picture stays the app's, with only the status bar's rows and the keyboard's band, from its
 *  top (`r.keyboard.windowTop`) down, taken where all three reads agree on them. The caller holds the simulator's
 *  display (`simulatorDisplay`), so no other drive's launch comes in between. `r.pixels` says which (`display` or
 *  `app`, `r.pixelsWhy` why); what could not be done is said in `note`. Returns `r`. */
export async function withDisplay(r, path, udid, appTop = 0, { own = false, probe = null, gap = 400 } = {}) {
  const top = r.keyboard?.windowTop, bar = r.statusBar, scale = r.scale, keyboard = top != null, opaque = r.opaque;
  delete r.opaque; // the check's input, hundreds of points: not the reply's

  const approximate = (why) => `the picture is the app's own drawing, not the display's (${why}): glass and other backdrop materials in it are approximate`;
  const say = (note) => { r.note = r.note ? `${r.note}; ${note}` : note; };
  // The app's picture as it was: none of the display in it.
  const keep = (why) => {
    if (bar > 0) r.statusBar = false;
    r.pixels = 'app'; r.pixelsWhy = why;
    if (r.keyboard?.visible) r.note = `the software keyboard is up: it is not in the capture (${why}), and the app above it may be shortened (state shows keyboard.top)`;
    else if (bar > 0) r.note = `the status bar is not in the capture (${why})`;
    else if (!own) say(approximate(why));
    return r;
  };
  if (!(scale > 0)) return keep('no scale');
  const front = (said) => said === true ? null : typeof said === 'string' ? said : 'the app did not say it was in front';
  const away = front(r.front);
  if (away) return keep(away);
  let why = own ? 'asked for the app\'s own drawing' : r.unshown?.length > 0 ? 'header nodes are struck through in it'
    : r.moving ? 'UIKit\'s motion had not settled' : null;
  if (why && !(bar > 0) && !keyboard) return keep(why);
  const reads = displayReads(udid, gap);
  try {
    const [display, again, last] = await Promise.all(reads.reads), app = decodePng(readFileSync(path));
    if (app.width !== display.width || app.height !== display.height) return keep(`the display is ${display.width}x${display.height}, the capture ${app.width}x${app.height}`);
    const after = probe ? await probe() : { front: 'the app was not asked again' };
    const gone = front(after.front) ?? (['epoch', 'incarnation', 'clock'].some((k) => after[k] !== r[k]) ? 'the app changed while the display was read' : null);
    if (gone) return keep(gone);
    const row = (points) => Math.max(0, Math.min(app.height, Math.round(points * scale))) * app.width * 4;
    // In the bottom safe area, the home indicator's columns (`INDICATOR` points either side of the middle) are left out,
    // unless the keyboard is up there: then they are the keyboard's band, all of which has to agree.
    const stride = app.width * 4, safe = keyboard || r.keyboard?.visible ? display.data.length : row(app.height / scale - (r.safeArea?.bottom ?? 0));
    const [left, right] = [-1, 1].map((side) => Math.max(0, Math.min(app.width, Math.round((app.width / scale / 2 + side * INDICATOR) * scale))) * 4);
    const same = (d, from, to) => from >= to || Buffer.compare(display.data.subarray(from, to), d.data.subarray(from, to)) === 0;
    const agree = (from, to = display.data.length) => [again, last].every((d) => {
      if (!same(d, from, Math.min(to, safe))) return false;
      for (let at = Math.max(from, safe); at < to; at += stride) if (!same(d, at, at + left) || !same(d, at + right, at + stride)) return false;
      return true;
    });
    const from = row(bar > 0 ? 0 : appTop);
    if (!why && !agree(row(bar > 0 ? bar : appTop))) why = 'the display changed while it was read';
    if (!why) why = covered(opaque, app, display, scale);
    if (!why) app.data.set(display.data.subarray(from), from);
    if (why && bar > 0 && agree(0, row(bar))) app.data.set(display.data.subarray(0, row(bar)), 0);
    else if (why && bar > 0) r.statusBar = false;
    const band = keyboard && (!why || agree(row(top)));
    if (why && band) app.data.set(display.data.subarray(row(top)), row(top));
    if (bar > 0 && r.statusBar !== false) r.statusBar = true;
    writeFileSync(path, encodePng(app));
    r.pixels = why ? 'app' : 'display';
    if (why) r.pixelsWhy = why;
    if (band || (!why && r.keyboard?.visible)) {
      r.keyboard.captured = true;
      r.note = `the software keyboard is up and in the capture${why ? ' from keyboard.top down' : ''}, as the simulator draws it; the app above it is shortened`;
    } else if (r.keyboard?.visible) r.note = `the software keyboard is up: it is not in the capture (${why}), and the app above it may be shortened (state shows keyboard.top)`;
    if (why && !own) say(approximate(why));
    return r;
  } catch (e) { return keep(String(e.message ?? e).split('\n')[0]); }
  finally { reads.cancel(); }
}

/** Half the width, in points, of the bottom safe area's middle that the home indicator (134 points wide on an iPhone)
 *  is drawn in, with room for its edge. */
const INDICATOR = 80;

/** The simulator's display read three times for `withDisplay`, started now: the second 150 ms after the first, not
 *  after it ends (each `simctl io` read is ~0.4 s, most of it the tool's own start), and the third `gap` ms after both
 *  have ended, so it is that far from either whenever their pictures were taken. `reads` are their pictures, decoded;
 *  `cancel()` stops what is still reading and removes the files. */
function displayReads(udid, gap = 400) {
  useXcode();
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-display-')), children = new Set(), timers = new Set();
  let cancelled = false;
  const later = (ms, fn) => { const timer = setTimeout(() => { timers.delete(timer); fn(); }, ms); timers.add(timer); };
  const read = (name) => new Promise((done, fail) => {
    if (cancelled) return fail(new Error('the display reads were cancelled'));
    const shot = resolve(dir, name);
    let said = '';
    const child = spawn('xcrun', ['simctl', 'io', udid, 'screenshot', '--type=png', shot], { stdio: ['ignore', 'ignore', 'pipe'] });
    children.add(child);
    const bound = setTimeout(() => child.kill('SIGKILL'), 20000);
    child.stderr.on('data', (d) => { said += d; });
    child.on('error', (e) => { clearTimeout(bound); children.delete(child); fail(e); });
    child.on('close', (status) => {
      clearTimeout(bound); children.delete(child);
      try {
        if (status !== 0 || !existsSync(shot)) throw new Error(`simctl io screenshot: ${said.trim().split('\n').pop() || 'failed'}`);
        done(decodePng(readFileSync(shot)));
      } catch (e) { fail(e); }
    });
  });
  const first = read('display.png'), second = new Promise((done) => later(150, done)).then(() => read('again.png'));
  const third = Promise.allSettled([first, second]).then(() => new Promise((done) => later(gap, done))).then(() => read('last.png'));
  const reads = [first, second, third];
  for (const r of reads) r.catch(() => {}); // what a cancelled or failed drive leaves unread fails quietly
  return {
    reads,
    cancel() {
      cancelled = true;
      for (const t of timers) clearTimeout(t);
      for (const child of children) child.kill('SIGKILL');
      rmSync(dir, { recursive: true, force: true });
    },
  };
}

/** What a drive passes from its own environment to an app that `simctl launch` (which forwards only `SIMCTL_CHILD_`
 *  names) or `devicectl` (only what it is given) starts, as a macOS drive's app inherits it: `EXACT_STORE`, so
 *  `EXACT_STORE=real` gives an iOS drive the Keychain a normal launch reads (host/apple/src/store.rs), and
 *  `EXACT_AGENT_PADS`, a drive's game pads (LLP 1133.022 §21). */
export function forwardedEnv(env = process.env) {
  return Object.fromEntries(['EXACT_STORE', 'EXACT_AGENT_PADS'].filter((k) => env[k]).map((k) => [k, env[k]]));
}

/** A page capture (`path`) whose area holds glass or a blur, its pixels then the display's there too: the app names
 *  that area only then (`r.materials`: x, y, width and height in window points), and `window(p)` takes a window
 *  capture into `p` through `withDisplay`. Where that capture's pixels are the display's, the page's area is cut from
 *  it; otherwise the picture stays the app's and says its materials are approximate, and why. Returns `r`. */
export async function withMaterials(r, path, window) {
  const area = r.materials;
  delete r.materials;
  if (!Array.isArray(area) || !(r.scale > 0)) return r;
  const keep = (why) => {
    r.pixels = 'app'; r.pixelsWhy = why;
    const note = `the picture is the app's own drawing, not the display's (${why}): glass and other backdrop materials in it are approximate; \`screenshot <png> window\` takes the display's while the app is in front`;
    r.note = r.note ? `${r.note}; ${note}` : note;
    return r;
  };
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-glass-'));
  try {
    const shot = resolve(dir, 'window.png'), whole = await window(shot);
    if (whole.pixels !== 'display') return keep(whole.pixelsWhy ?? 'the window capture was the app\'s');
    const display = decodePng(readFileSync(shot)), app = decodePng(readFileSync(path));
    const [x, y] = area.map((v) => Math.round(v * r.scale));
    if (x < 0 || y < 0 || x + app.width > display.width || y + app.height > display.height) return keep('the page lies outside the window');
    for (let row = 0; row < app.height; row++) {
      const from = ((y + row) * display.width + x) * 4;
      app.data.set(display.data.subarray(from, from + app.width * 4), row * app.width * 4);
    }
    writeFileSync(path, encodePng(app));
    r.pixels = 'display';
    if (r.keyboard?.visible) { r.keyboard.captured = true; r.note = 'the software keyboard is up and in the capture where it covers the page, as the simulator draws it'; }
    return r;
  } catch (e) { return keep(String(e.message ?? e).split('\n')[0]); }
  finally { rmSync(dir, { recursive: true, force: true }); }
}

/** The simulator's display as it is, into `path` over the app's own picture: whatever is over the app (a system
 *  permission alert, a sheet of the system's) is in it, and nothing checks it against the app's drawing. */
export function displayAsIs(r, path, udid) {
  const taken = read('xcrun', ['simctl', 'io', udid, 'screenshot', '--type=png', path], { timeout: 20000 });
  if (taken.status !== 0) throw new Error(`simctl io screenshot: ${(taken.stderr || '').trim().split('\n').pop() || 'failed'}`);
  r.pixels = 'display'; r.pixelsWhy = 'the display as it is, asked for'; r.statusBar = true;
  if (r.front !== true) r.note = `the app is not in front (${r.front}): the display shows what is over it`;
  return r;
}

/** Why the display is not the app's at the points the app drew itself (`points`, window points), or null: around a
 *  point whose 5x5 pixels the app drew in one colour (no edge of text or shape) the display's must be within 2 levels of
 *  it, as they are exactly where nothing lies over the app (measured on the euxy port, light and dark). A notification
 *  banner's frosted material, 3 or 4 levels off a light page, changes such points; more than 1% of them, or 3, refuse. */
function covered(points, app, display, scale) {
  if (!Array.isArray(points) || points.length === 0) return 'the app named no points to check the display against';
  const at = (png, x, y) => (y * png.width + x) * 4;
  let checked = 0, differing = 0;
  for (const [px, py] of points) {
    const x = Math.round(px * scale), y = Math.round(py * scale);
    if (x < 2 || y < 2 || x >= app.width - 2 || y >= app.height - 2) continue;
    const c = at(app, x, y);
    let flat = true;
    for (let dy = -2; dy <= 2 && flat; dy++) for (let dx = -2; dx <= 2 && flat; dx++) {
      const o = at(app, x + dx, y + dy);
      for (let k = 0; k < 3; k++) if (app.data[o + k] !== app.data[c + k]) flat = false;
    }
    if (!flat) continue;
    checked++;
    let differs = false;
    for (let dy = -2; dy <= 2 && !differs; dy++) for (let dx = -2; dx <= 2 && !differs; dx++) {
      const o = at(display, x + dx, y + dy);
      for (let k = 0; k < 3; k++) if (Math.abs(display.data[o + k] - app.data[c + k]) > 2) differs = true;
    }
    if (differs) differing++;
  }
  if (checked === 0) return 'none of the points the app named is a plain colour to check the display against';
  return differing > Math.max(2, checked / 100) ? `the display differs from the app's drawing at ${differing} of ${checked} plain points (a notification banner or another overlay)` : null;
}

/** Every available simulator: { udid, name, runtime, state, type } — `type`
 *  the device type's last component (`iPhone-18-Pro`, `Apple-TV-4K-…`), which a
 *  renamed simulator keeps. */
export function simulators() {
  const r = read('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
  if (r.status !== 0) throw new Error('xcrun simctl list: ' + r.stderr);
  return Object.entries(JSON.parse(r.stdout).devices).flatMap(([runtime, list]) => list.map((d) => ({ udid: d.udid, name: d.name, runtime, state: d.state, type: d.deviceTypeIdentifier?.split('.').pop() ?? '' })));
}

/** The simulator to use — `pick` (a udid or a name; EXACT_SIM by default), else the one booted iPhone, else the iPhone Pro on the newest iOS — booted and waited for. When several are booted, the one `app` (a bundle id) is installed on, and of several so, the one the app in `dir` last used (its `.exact/simulator`, written when a simulator is named or chosen so), said on stderr; refused when that is none or several, naming how to choose: `hint`, the caller's own way. With `tv`, the same choice among Apple TVs on tvOS. An iPhone is one by its device type, not its name: a simulator renamed `work-phone` is still one. */
const saidSimulator = new Set(); // a verify opens a dozen sessions: the choice is said once
export function simulator(pick = process.env.EXACT_SIM, options = {}) { return bootSimulator(chooseSimulator(pick, options)); }
/** `simulators()` as it was a moment ago in this process: a drive chooses its simulator before it takes the store's
 *  lock and again when it launches, a fraction of a second apart. A boot here forgets it; a shutdown by another process
 *  in between fails the launch, where a fresh list would have booted it. */
let chosenFrom = null;
const recentSimulators = () => (chosenFrom && Date.now() - chosenFrom.at < 5000 ? chosenFrom : chosenFrom = { at: Date.now(), all: simulators() }).all;
/** `simulator`'s choice, booting nothing: what a drive locks before it launches (agent.mjs `resolveDevice`). */
export function chooseSimulator(pick = process.env.EXACT_SIM, { tv = false, hint = 'EXACT_SIM=<udid|name>', app, dir } = {}) {
  const all = recentSimulators();
  const version = (d) => Number(/(?:iOS|tvOS)-(\d+)-(\d+)/.exec(d.runtime)?.slice(1).join('.') ?? 0);
  const kind = (d) => d.type || d.name.replaceAll(' ', '-');
  const iphones = all.filter((d) => (tv ? /SimRuntime\.tvOS/.test(d.runtime) && /^Apple-TV/.test(kind(d)) : /SimRuntime\.iOS/.test(d.runtime) && /^iPhone/.test(kind(d)))).sort((a, b) => version(b) - version(a) || a.name.localeCompare(b.name));
  let dev = pick ? all.find((d) => d.udid === pick || d.name === pick) : null;
  if (pick && !dev) throw new Error(`no simulator ${pick} (xcrun simctl list devices available)`);
  // Several booted: whose is which is not this process's to guess (an install onto another drive's simulator,
  // 2026-10-09: build.mjs --ios takes --sim, not --phone, for a simulator). One booted, or none, is still chosen.
  const booted = iphones.filter((d) => d.state === 'Booted');
  const used = dir ? resolve(dir, '.exact', tv ? 'simulator-tv' : 'simulator') : null;
  let last = null; try { last = used && readFileSync(used, 'utf8').trim(); } catch { /* none chosen yet */ }
  // What `used` remembers: a simulator named, or one the installed-app check chose.
  let chosen = Boolean(pick);
  if (!pick && booted.length > 1) {
    const has = app ? booted.filter((d) => read('xcrun', ['simctl', 'get_app_container', d.udid, app]).status === 0) : [];
    const mine = has.length > 1 ? has.find((d) => d.udid === last) : null;
    if (has.length === 1 || mine) {
      dev = mine ?? has[0]; chosen = true;
      if (!saidSimulator.has(dev.udid)) { saidSimulator.add(dev.udid); console.error(`${booted.length} ${tv ? 'Apple TV' : 'iPhone'} simulators are booted; using ${dev.name} ${dev.udid}, ${mine ? `the one this app last used of those with ${app} installed` : `the one with ${app} installed`}; name another with ${hint}`); }
    } else {
      const named = (d) => `${d.name} ${d.udid}${has.includes(d) ? ` (has ${app})` : ''}`;
      throw new Error(`${booted.length} ${tv ? 'Apple TV' : 'iPhone'} simulators are booted (${booted.map(named).join(', ')})${app ? `, ${has.length ? 'several' : 'none'} with ${app} installed` : ''}: name one with ${hint}`);
    }
  }
  dev ??= booted[0] ?? iphones.find((d) => /^iPhone-\d+-Pro$/.test(kind(d))) ?? iphones[0];
  if (!dev) throw new Error(`no ${tv ? 'Apple TV' : 'iPhone'} simulator on ${tv ? 'tvOS' : 'iOS'}; add one in Xcode, or name any simulator by udid or name with EXACT_SIM (or --sim)`);
  if (used && chosen && dev.udid !== last) try { mkdirSync(dirname(used), { recursive: true }); writeFileSync(used, `${dev.udid}\n`); } catch { /* remembered next time, if it can be */ }
  return dev;
}
/** `dev` booted and waited for; `bootedHere` when this call booted it, not when another process had just done so. */
function bootSimulator(dev) {
  // Booted is a boot finished (Booting is its own state): only another waits, which costs a booted one 0.2–0.5 s.
  if (dev.state === 'Booted') return dev;
  chosenFrom = null;
  const b = read('xcrun', ['simctl', 'boot', dev.udid]);
  if (b.status !== 0 && !/current state: Booted/.test(b.stderr)) throw new Error('simctl boot: ' + b.stderr);
  const s = read('xcrun', ['simctl', 'bootstatus', dev.udid, '-b']);
  // A simulator this call booted is not left running when its boot does not finish.
  if (s.status !== 0) { if (b.status === 0) read('xcrun', ['simctl', 'shutdown', dev.udid]); throw new Error('simctl bootstatus: ' + s.stderr); }
  return b.status === 0 ? { ...dev, bootedHere: true } : dev;
}

/** Bring Simulator.app up showing `dev`, so a person watching sees what is
 *  driven there; `background` (`open -g`) leaves keyboard focus where it was.
 *  Xcode 27 has no Simulator.app: its Device Hub (com.apple.dt.Devices) shows
 *  a simulator in a window of that device's own, opened and raised by the
 *  URL Device Hub registers (`devices://device/open?id=<udid>`; launch
 *  arguments never reach a Device Hub that is already running, and its main
 *  window shows whichever device was last picked in its list). */
export function showSimulator(dev, background = false) {
  if (process.env.EXACT_SHOW_SIMULATOR === '0') return;
  const g = background ? ['-g'] : [], fallback = () => spawnSync('open', [...g, `devices://device/open?id=${dev.udid}`], { stdio: 'ignore' });
  // In the background nothing waits on the window: `open` takes up to a second, a drive's whole launch about two.
  if (background) { spawn('open', [...g, '-a', 'Simulator', '--args', '-CurrentDeviceUDID', dev.udid], { stdio: 'ignore' }).on('exit', (code) => { if (code !== 0) fallback(); }).unref(); return; }
  if (spawnSync('open', [...g, '-a', 'Simulator', '--args', '-CurrentDeviceUDID', dev.udid], { stdio: 'ignore' }).status !== 0) fallback();
}

/** Whether the simulator's installed `id` is the bundle at `bundle` as built: its executable, receipt and signature
 *  the same size and time (simctl install keeps both). A drive then skips the install, 0.3–0.5 s of each launch. The
 *  bundle's container is looked in first where `simctl get_app_container` last said it was (~0.1 s, each drive): a
 *  reinstall or a removal takes that directory away, and any other answer asks simctl again. */
export function installedAlready(udid, id, bundle, executable, { records = resolve(homedir(), '.cache/exact/simulator-installs') } = {}) {
  const same = (installed) => {
    try {
      return [executable, 'receipt.json', '_CodeSignature/CodeResources'].every((file) => {
        const a = statSync(resolve(installed, file)), b = statSync(resolve(bundle, file));
        return a.size === b.size && a.mtimeMs === b.mtimeMs;
      });
    } catch { return false; }
  };
  const record = resolve(records, `${udid}-${id}`);
  try { if (same(readFileSync(record, 'utf8'))) return true; } catch { /* none remembered */ }
  const r = read('xcrun', ['simctl', 'get_app_container', udid, id, 'app']);
  if (r.status !== 0) return false;
  const installed = r.stdout.trim();
  try { mkdirSync(records, { recursive: true }); writeFileSync(record, installed); } catch { /* asked again next time */ }
  return same(installed);
}

/** The build of `id` installed on a simulator: its bundle's path, its receipt's `build.trust` (`unreceipted` with
 *  none) and what a person calls it; null with none installed. */
export function installedBuild(udid, id) {
  const where = read('xcrun', ['simctl', 'get_app_container', udid, id, 'app']);
  if (where.status !== 0) return null;
  const installed = where.stdout.trim();
  let trust = 'unreceipted';
  try { trust = JSON.parse(readFileSync(resolve(installed, 'receipt.json'), 'utf8')).build?.trust ?? trust; } catch {}
  return { installed, trust, what: trust === 'production' ? 'Release (production) build' : `${trust} build` };
}

/** What `build.mjs --ios` says when its development build replaced `replaced` (an `installedBuild` of another kind):
 *  one bundle id, so a benchmark or a person then finds the development build (euxy port); how to put it back, from
 *  the Release bundle at `release` when it was built here. */
export function replacedNote(udid, app, replaced, release) {
  const back = existsSync(release) ? `xcrun simctl install ${udid} ${shellQuote(release)}` : `bun host/apple/build.mjs --ios ${app.crate('apple')} --release --sim ${udid}`;
  return `host/apple: this development build replaced the simulator's ${replaced.what} of ${app.id} (one bundle id); put it back with ${back}`;
}

/** Before a drive installs its development build `bundle` of `id` (`executable` its binary's name): whether to install
 *  (`install`: false when that build is installed already, `installedAlready`), and, when the build installed now is
 *  not a development one (its receipt's `build.trust`: a `--release` or production bake, or one with no receipt), an
 *  APFS clone of it under `records`/<udid>-<id>, replacing any older one, and `note`, the one command that puts it back.
 *  Both share the bundle id, so a later benchmark or person would otherwise find the development build with no way
 *  back (euxy port). Nothing is reinstalled for the drive: that is the person's call. A clone that cannot be made
 *  throws, and nothing is installed. A drive's later sessions find the development build and copy nothing. */
export function keepInstalledRelease(udid, id, bundle, executable, { records = resolve(homedir(), '.cache/exact/simulator-releases') } = {}) {
  if (installedAlready(udid, id, bundle, executable)) return { install: false };
  const { installed, trust, what } = installedBuild(udid, id) ?? {};
  if (!installed || trust === 'development') return { install: true };
  // Copied beside the older copy first, which a failed copy leaves in place.
  const dir = resolve(records, `${udid}-${id}`), next = `${dir}.new`, copy = resolve(dir, basename(installed));
  rmSync(next, { recursive: true, force: true });
  mkdirSync(next, { recursive: true });
  if (read('cp', ['-cR', installed, resolve(next, basename(installed))]).status !== 0) {
    rmSync(next, { recursive: true, force: true });
    throw new Error(`${id}'s ${what} on this simulator could not be copied aside (cp -cR into ${next} failed), so the drive's development build was not installed over it`);
  }
  rmSync(dir, { recursive: true, force: true });
  renameSync(next, dir);
  return { install: true, note: `the simulator had ${id}'s ${what} installed; this drive installed its development build over it (one bundle id). Put it back with: xcrun simctl install ${udid} ${shellQuote(copy)}` };
}

/** A simulator's software keyboard for a drive: shown for a focused field, as a person's iPhone shows it. Once a Device
 *  Hub has attached the Mac's keyboard, the device keeps `com.apple.keyboard.preferences` `AutomaticMinimizationEnabled
 *  = 1` and minimizes its software keyboard behind it (LLP 1078, "The pushed route's keyboard, found"). Read and
 *  changed through the device's own `defaults` (`simctl spawn`), which cfprefsd answers before it writes the plist.
 *  On a simulator this drive booted (`owned`) the key is cleared and `restore()` puts it back; on any other it is
 *  left as it is and `note` says the keyboard may be hidden. Before clearing it the drive writes a record under
 *  `records`, removed only once the key is back: a drive that was killed, or whose write failed, leaves it, and the
 *  next drive on that simulator puts the key back first, unless the drive that wrote it is still running. */
const keyboardArgs = (udid, verb) => ['simctl', 'spawn', udid, 'defaults', verb, 'com.apple.keyboard.preferences', 'AutomaticMinimizationEnabled', ...(verb === 'write' ? ['-bool', 'true'] : [])];
const minimized = (r) => r.status === 0 && /^\s*1\s*$/.test(r.stdout ?? '');
const minimizedNote = (udid) => `this simulator minimizes its software keyboard (AutomaticMinimizationEnabled = 1, kept since the Mac's keyboard was attached), so a focused field shows none in a screenshot, unlike an iPhone; a simulator the drive boots has it cleared for the drive, or clear it here: xcrun simctl spawn ${udid} defaults delete com.apple.keyboard.preferences AutomaticMinimizationEnabled`;
/** The setting a killed drive left cleared, put back (`softwareKeyboard`): unless the drive that cleared it still runs. */
function keyboardLeft(udid, record, defaults) {
  const putBack = () => { if (defaults('write').status !== 0) return false; rmSync(record, { force: true }); return true; };
  let left = null;
  try { left = JSON.parse(readFileSync(record, 'utf8')); } catch {}
  if (left && !(Number.isInteger(left.pid) && left.pid !== process.pid && pidAlive(left.pid))) putBack();
  return putBack;
}
export function softwareKeyboard(udid, { owned = false, records = resolve(homedir(), '.cache/exact/simulator-keyboard') } = {}) {
  const defaults = (verb) => read('xcrun', keyboardArgs(udid, verb), { timeout: 20000 });
  const record = resolve(records, `${udid}.json`);
  const putBack = keyboardLeft(udid, record, defaults);
  const r = defaults('read');
  if (!minimized(r)) return { cleared: false, note: null, restore() {} };
  if (!owned) return { cleared: false, restore() {}, note: minimizedNote(udid) };
  mkdirSync(records, { recursive: true });
  writeFileSync(record, JSON.stringify({ pid: process.pid, value: 1 }));
  if (defaults('delete').status !== 0) { rmSync(record, { force: true }); return { cleared: false, note: 'the simulator\'s software keyboard is minimized and could not be shown (simctl spawn defaults delete failed)', restore() {} }; }
  let restored = false;
  return { cleared: true, note: null, restore() { if (!restored) restored = putBack(); } };
}
const saidKeyboard = new Set(); // the hidden keyboard's note, once a process: a smoke or verify opens many sessions
/** `open()`'s carrier with the software keyboard shown for a focused field on a simulator this drive booted
 *  (`softwareKeyboard`), its setting put back when the drive closes, fails to open, or the driver exits (a killed
 *  drive's, by the next drive on that simulator); on any other simulator, the note that the keyboard may be hidden. */
export async function withSoftwareKeyboard(dev, open, { records = resolve(homedir(), '.cache/exact/simulator-keyboard') } = {}) {
  // On a simulator another booted the setting is only read, for the note: beside the launch, not before it (~0.2 s).
  if (!dev.bootedHere) {
    keyboardLeft(dev.udid, resolve(records, `${dev.udid}.json`), (verb) => read('xcrun', keyboardArgs(dev.udid, verb), { timeout: 20000 }));
    const said = saidKeyboard.has(dev.udid) ? null : new Promise((done) => {
      useXcode();
      let out = '';
      const child = spawn('xcrun', keyboardArgs(dev.udid, 'read'), { stdio: ['ignore', 'pipe', 'ignore'] });
      const bound = setTimeout(() => child.kill('SIGKILL'), 20000);
      child.stdout.on('data', (d) => { out += d; });
      child.on('error', () => { clearTimeout(bound); done(); });
      child.on('close', (status) => {
        clearTimeout(bound);
        if (minimized({ status, stdout: out }) && !saidKeyboard.has(dev.udid)) { saidKeyboard.add(dev.udid); console.error(`note: ${minimizedNote(dev.udid)}`); }
        done();
      });
    });
    let carrier;
    try { carrier = await open(); } finally { if (!carrier) await said; }
    const close = carrier.close.bind(carrier);
    carrier.close = async (...args) => { try { return await close(...args); } finally { await said; } };
    return carrier;
  }
  const keyboard = softwareKeyboard(dev.udid, { owned: true, records });
  if (keyboard.note && !saidKeyboard.has(dev.udid)) { saidKeyboard.add(dev.udid); console.error(`note: ${keyboard.note}`); }
  const restore = () => keyboard.restore();
  process.once('exit', restore);
  const done = () => { restore(); process.off('exit', restore); };
  let carrier;
  try { carrier = await open(); } catch (e) { done(); throw e; }
  if (!keyboard.cleared) { done(); return carrier; }
  const close = carrier.close.bind(carrier);
  carrier.close = async (...args) => { try { return await close(...args); } finally { done(); } };
  return carrier;
}
const pidAlive = (pid) => { try { process.kill(pid, 0); return true; } catch (e) { return e.code === 'EPERM'; } };
/** Crash reports macOS wrote since `since` (ms) for an executable named
 *  `name`, newest first — a simulator app crashes as a Mac process, reported
 *  here. The name is all the file names carry, and every simulator's copy of
 *  an app shares it: `bundleId` keeps the reports whose header names that
 *  bundle, and `udid` those whose process ran in that simulator (its path is
 *  under `CoreSimulator/Devices/<udid>/`). */
export function crashReports(name, since, { bundleId, udid, dir = resolve(homedir(), 'Library/Logs/DiagnosticReports') } = {}) {
  let names = [];
  try { names = readdirSync(dir); } catch { return []; }
  const at = (f) => { try { return statSync(f).mtimeMs; } catch { return -1; } };
  const ours = (f) => {
    if (!bundleId && !udid) return true;
    const report = parseReport(f);
    return !!report && (!bundleId || report.bundleID === bundleId) && (!udid || report.paths.some((p) => String(p ?? '').includes(udid)));
  };
  return names.filter((f) => f.startsWith(name + '-') && /\.(ips|crash)$/.test(f))
    .map((f) => resolve(dir, f)).filter((f) => at(f) >= since && ours(f)).sort((a, b) => at(b) - at(a));
}
/** Launch the Mac executable and preserve how it ended for the build command's caller. */
export function runMac(binary, env, { stdio = 'inherit', say = console.error } = {}) {
  const crashed = crashWatch(basename(binary), dirname(binary));
  const result = spawnSync(binary, [], { env, stdio });
  if (result.error) say(`${binary}: launch failed: ${result.error.message}`);
  if (result.signal) {
    say(`${binary}: killed by ${result.signal}`);
    const fatal = ['SIGABRT', 'SIGSEGV', 'SIGBUS', 'SIGILL', 'SIGFPE', 'SIGTRAP', 'SIGSYS'].includes(result.signal);
    for (const line of crashed(fatal ? 5000 : 0)) say(line);
  }
  return result.status ?? (result.signal ? 128 + (constants.signals[result.signal] ?? 0) : 1);
}

/** A crash report as one shape, whichever form it is in: its bundle id, time,
 *  the paths that place it on a simulator, how the process ended and why (the
 *  exception, the termination, the app-specific message, such as a Swift
 *  `fatalError`'s), and the crashing thread's top `frames` frames. An `.ips`
 *  is a JSON header line, then the JSON body; a `.crash` is the older text,
 *  `Field: value` lines and a `Thread N Crashed:` list. Null for neither. */
function parseReport(file, frames = 8) {
  let text;
  try { text = readFileSync(file, 'utf8'); } catch { return null; }
  try {
    const cut = text.indexOf('\n'), head = JSON.parse(text.slice(0, cut)), body = JSON.parse(text.slice(cut + 1));
    const e = body.exception ?? {}, images = body.usedImages ?? [];
    const thread = body.threads?.[body.faultingThread] ?? body.threads?.find((t) => t.triggered);
    return {
      bundleID: head.bundleID, timestamp: head.timestamp, paths: [body.procPath, body.coalitionName],
      ended: [[e.type, e.signal && `(${e.signal})`].filter(Boolean).join(' '), body.termination?.indicator].filter(Boolean).join(', '),
      thread: thread ? `thread ${body.faultingThread ?? thread.id}${thread.queue ? ` (${thread.queue})` : ''}` : null,
      why: Object.values(body.asi ?? {}).flat().map((line) => String(line).trim()).slice(0, 3),
      frames: (thread?.frames ?? []).slice(0, frames).map((f) => `${f.symbol ? `${f.symbol} + ${f.symbolLocation ?? 0}` : `0x${(f.imageOffset ?? 0).toString(16)}`} (${images[f.imageIndex]?.name ?? '?'})`),
    };
  } catch {}
  const field = (name) => text.match(new RegExp(`^${name}:[ \\t]*(.+)$`, 'm'))?.[1].trim();
  if (!field('Identifier') && !field('Path')) return null;
  const lines = text.split('\n'), at = lines.findIndex((l) => /^Thread \d+ Crashed/.test(l));
  const listed = at < 0 ? [] : lines.slice(at + 1);
  const end = listed.findIndex((l) => !/^\d+\s/.test(l));
  return {
    bundleID: field('Identifier'), timestamp: field('Date/Time'), paths: [field('Path'), field('Coalition')],
    ended: [field('Exception Type'), field('Termination Reason')].filter(Boolean).join(', '),
    thread: at < 0 ? null : lines[at].replace(/^Thread (\d+) Crashed:*\s*(?:Dispatch queue:\s*)?/, (_, n) => `thread ${n} `).trim().replace(/^(thread \d+) (.+)$/, '$1 ($2)'),
    // Its own lines, below the heading, up to a blank line.
    why: (() => { const h = lines.findIndex((l) => /^Application Specific Information:/.test(l)); if (h < 0) return []; const after = lines.slice(h + 1), stop = after.findIndex((l) => !l.trim()); return after.slice(0, stop < 0 ? 3 : stop).map((l) => l.trim()).slice(0, 3); })(),
    // `0   libsystem_kernel.dylib   0x1b2c…   __pthread_kill + 8`: the symbol, then its image.
    frames: listed.slice(0, end < 0 ? listed.length : end).slice(0, frames)
      .map((l) => { const m = l.match(/^\d+\s+(\S+)\s+0x[0-9a-f]+\s+(.*)$/); return m ? `${m[2].trim()} (${m[1]})` : l.trim(); }),
  };
}
/** A crash report as a few lines: its path and time, how the process ended
 *  and why, and the crashing thread's top `frames` frames. */
export function crashSummary(file, frames = 8) {
  const r = parseReport(file, frames);
  if (!r) return [`crash report: ${file}`];
  return [
    `crash report: ${file} (${r.timestamp ?? 'no time'})`,
    `  ${[r.ended, r.thread && `${r.thread} crashed:`].filter(Boolean).join('; ')}`,
    ...r.why.map((line) => `  ${line}`),
    ...r.frames.map((f, i) => `  ${String(i).padStart(2)} ${f}`),
  ];
}
/** The newest crash report the scope admits since `bundle` was built (its
 *  `receipt.json`), summarized once: each call answers only a report newer
 *  than the last it answered (`agent ios logs`, and a launch that failed).
 *  Reports written before this launch count. `wait` ms: macOS writes a report
 *  a moment after the process dies, so a launch that just failed polls that
 *  long for one written since the watch began. */
export function crashWatch(name, bundle, scope) {
  const began = Date.now();
  let after = began;
  try { after = statSync(resolve(bundle, 'receipt.json')).mtimeMs; } catch {}
  return (wait = 0) => {
    const until = Date.now() + wait, mtime = (f) => statSync(f).mtimeMs;
    let [latest] = crashReports(name, after, scope);
    while ((!latest || mtime(latest) < began) && Date.now() < until) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
      [latest] = crashReports(name, after, scope);
    }
    if (!latest) return [];
    after = statSync(latest).mtimeMs + 1;
    return crashSummary(latest);
  };
}


// ---------------------------------------------------------------- a phone: devicectl, a profile, an identity

/** Startup can fail before the app's first line; keep devicectl's evidence and
 * distinguish a reported lock from a prompt the carrier cannot observe. */
export function phoneStartupFailure(reason, lines) {
  const details = [reason, ...lines.filter(line => !reason.includes(line))].join('\n');
  const silent = lines.length ? '' : '\nthe app printed nothing';
  if (/FBSOpenApplication[^\n]*\bLocked\b|\breason:\s*Locked\b|(?:device|iPhone|iPad) is locked|\bDeviceLocked\b/i.test(details)) {
    return `the iPhone is locked: unlock it and run again\n${details}${silent}`;
  }
  const prompt = /No route to host|EHOSTUNREACH|waiting for foreground/i.test(details)
    ? "\nApple's local-network prompt may be waiting on the phone. Allow Local Network there, including a prompt left by another app, then run again."
    : '';
  return `${details}${silent}${prompt}\nUnlock the phone and keep the app in the foreground; the phone must reach this Mac on the same LAN.`;
}

/** One launch, one phone connection; reject other peers before any agent request.
 * The token crosses via the paired device's launch environment, not a public URL. */
export async function phoneBridge() {
  const interfaces = networkInterfaces();
  const address = process.env.EXACT_AGENT_HOST ?? [...(interfaces.en0 ?? []), ...Object.values(interfaces).flat()]
    .find((n) => n.family === 'IPv4' && !n.internal)?.address;
  if (!address) throw new Error('phone agent needs a reachable Mac IPv4 address (EXACT_AGENT_HOST)');
  const token = randomBytes(32).toString('hex');
  const sockets = new Set();
  let accept, fail;
  const ready = new Promise((resolve, reject) => { accept = resolve; fail = reject; });
  const server = createTCPServer((socket) => {
    if (sockets.size >= 8) { socket.destroy(); return; }
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
    socket.on('error', () => {});
    socket.setTimeout(5000, () => socket.destroy());
    let buf = '';
    socket.setEncoding('utf8');
    const hello = (chunk) => {
      buf += chunk;
      if (buf.length > 4096) { socket.destroy(); return; }
      if (!buf.includes('\n')) return;
      let announcement;
      try { announcement = JSON.parse(buf); } catch { socket.destroy(); return; }
      if (!announcement || announcement.token !== token || announcement.ready !== true) { socket.destroy(); return; }
      delete announcement.token;
      socket.pause();
      socket.removeListener('data', hello);
      socket.setTimeout(0);
      server.close();
      for (const other of sockets) if (other !== socket) other.destroy();
      accept({ socket, announcement });
    };
    socket.on('data', hello);
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, address, resolve); });
  server.on('error', fail);
  return {
    ready, fail,
    env: { EXACT_AGENT_CONNECT: `${address}:${server.address().port}`, EXACT_AGENT_TOKEN: token },
    close() { for (const socket of sockets) socket.destroy(); server.close(); },
  };
}


/** Every phone this Mac knows (devicectl, which lists simulators too): { id, udid, name, model, os, reachable, simulated }. */
export function phones() {
  const out = resolve(mkdtempSync(resolve(tmpdir(), 'exact-devices-')), 'devices.json');
  const r = read('xcrun', ['devicectl', 'list', 'devices', '--json-output', out]);
  if (r.status !== 0) throw new Error('xcrun devicectl list devices: ' + r.stderr);
  const list = JSON.parse(readFileSync(out, 'utf8')).result.devices.map((d) => ({
    id: d.identifier, udid: d.hardwareProperties?.udid, name: d.deviceProperties?.name, model: d.hardwareProperties?.marketingName,
    os: d.deviceProperties?.osVersionNumber, reachable: d.connectionProperties?.tunnelState !== 'unavailable', paired: d.connectionProperties?.pairingState === 'paired',
    simulated: d.hardwareProperties?.reality === 'simulated',
  }));
  rmSync(resolve(out, '..'), { recursive: true, force: true });
  return list;
}

/** The phone to use: `pick` (a udid or a name; EXACT_PHONE by default), else a reachable phone, else the one phone this Mac knows — the bundle is built and signed for it either way; installing needs it connected (`reachable`). Unpicked, a simulator devicectl lists is no phone. With `tv`, the same choice among physical Apple TVs. */
export function phone(pick = process.env.EXACT_PHONE, { tv = false } = {}) {
  const named = phones().filter((d) => !tv || (!d.simulated && /Apple TV/.test(d.model ?? ''))), all = pick ? named : named.filter((d) => !d.simulated);
  const dev = pick ? all.find((d) => d.udid === pick || d.id === pick || d.name === pick) : all.find((d) => d.reachable) ?? (all.length === 1 ? all[0] : null);
  if (!dev) throw new Error(pick ? `no phone ${pick} (xcrun devicectl list devices)` : `no phone is known to this Mac (${all.length ? all.map((d) => `${d.name}, not connected`).join('; ') : 'xcrun devicectl list devices shows none'}): plug one in, unlock it, and trust this Mac`);
  return dev;
}

/**
 * A phone cannot read a plan or asset directory on this Mac. When the caller
 * names the dev server with EXACT_DEV_PLAN, carry that URL into the launched
 * process; the envelope supplies its own complete asset URLs.
 */
export function deviceLaunchArgs(device, id, environment = process.env) {
  const args = ['devicectl', 'device', 'process', 'launch', '--terminate-existing', '--device', device];
  const locator = environment.EXACT_DEV_PLAN;
  if (locator) {
    let url;
    try { url = new URL(locator); }
    catch { throw new Error(`--device cannot open EXACT_DEV_PLAN=${locator} on this Mac; name the dev server's http(s) URL`); }
    if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error(`--device requires EXACT_DEV_PLAN to be an http(s) dev-server URL, not ${locator}`);
    args.push('--environment-variables', JSON.stringify({ EXACT_DEV_PLAN: url.href }));
  }
  args.push(id);
  return args;
}

/** The explicit app URL: the launch locator, and the one dev-server origin a
 * development build admits opening links for (`developmentAdmission`).
 * Validate it before invoking any build or signing tools. @ref LLP 1030.000 §7 */
export function developmentLaunchEnvironment(args, environment = process.env) {
  const index = args.indexOf('--url');
  if (index < 0) return { ...environment };
  if (args.lastIndexOf('--url') !== index) throw new Error('--url may be specified only once');
  if (!['--run', '--bundle', '--ios', '--device'].some((flag) => args.includes(flag))) throw new Error('--url requires --run or a client bundle (--bundle, --ios, --device)');
  let url;
  try { url = new URL(args[index + 1]); } catch { /* diagnosed below */ }
  if (!url || !['http:', 'https:'].includes(url.protocol) || !url.hostname) {
    throw new Error('--url requires an absolute http(s) app URL');
  }
  return { ...environment, EXACT_DEV_PLAN: url.href };
}

/** The environment a build signs under a profile that is (`debuggable`) or is not a development one: App Attest's
 * and push's (LLP 1133.028 D15 item 1; LLP 1133.004 §9 P2). */
export const signedEnvironment = (debuggable) => debuggable ? 'development' : 'production';

/** Whether profile `p` allows entitlement `name` as the build signs it under `p`: the key, and for push's environment,
 * which a profile carries as a value, the value the build writes there. The Live Activity path and the grant path
 * both ask this. */
export const allowsEntitlement = (p, name) => name === 'aps-environment' ? p.aps === signedEnvironment(p.dev) : p.entitlements.includes(name);

/** What `p` lacks of `required`, each as a refusal names it. */
export const lacks = (p, required = []) => required.filter((name) => !allowsEntitlement(p, name)).map((name) => name === 'aps-environment' && p.aps
  ? `aps-environment ${signedEnvironment(p.dev)} (it allows ${p.aps ?? 'no value'}, and this build signs ${signedEnvironment(p.dev)} under a ${p.dev ? 'development' : 'distribution'} profile)`
  : name);

/** Whether a profile allows every entitlement in `required` (a grant's
 * signing entitlements, LLP 1069.008.000 D4). */
export const allows = (p, required = []) => !!p && lacks(p, required).length === 0;

/** What a profile that lacks a required entitlement needs, said once. */
const missing = (bundle, required, why) => new Error(required.includes('com.apple.developer.web-browser')
  ? `browser-device-profile: ${required.join(', ')} required by ${bundle}, and ${why}. Request Apple's managed default-browser approval, enable it for this App ID, then install a profile that grants it (or name it with EXACT_PROFILE).`
  : required.includes('com.apple.developer.kernel.increased-memory-limit')
  ? `memory-device-profile: ${required.join(', ')} required by ${bundle} (app.json's host.ios.increasedMemoryLimit${required.length > 1 ? ' and its grants' : ''}), and ${why}. In the Apple Developer portal, enable Increased Memory Limit${required.length > 1 ? ' and the grants\' capabilities' : ''} for the App ID ${bundle}, then install a profile that grants ${required.length > 1 ? 'them' : 'it'} (or name it with EXACT_PROFILE).`
  : `grant-device-profile: ${required.join(', ')} ${required.length > 1 ? 'are' : 'is'} needed by this app's grants, and ${why} (a team wildcard never allows HealthKit). In the Apple Developer portal, register the App ID ${bundle} with that capability, make a development profile for it that includes this phone, and install it (or name it with EXACT_PROFILE).`);

/** A development profile on this Mac covering the phone and the bundle id (the team's wildcard or the id itself, under the profile's App ID prefix, which an older team's App IDs keep apart from its team id), unexpired; DEVELOPMENT_TEAM restricts the team, and EXACT_PROFILE overrides that choice.
 * With `required` (a grant's signing entitlements), only one that allows them all, which is one for the id itself. */
export function profile(udid, bundle, required = [], { installed = installedProfiles(), named = process.env.EXACT_PROFILE ? decodeProfile(process.env.EXACT_PROFILE) : null, team = process.env.DEVELOPMENT_TEAM?.trim(), phoneName = udid } = {}) {
  if (named) {
    if (!iosProfile(named)) throw new Error(`EXACT_PROFILE (${named.name}) is not an iOS profile; its platform is ${named.platforms.join(', ')}`);
    if (!allows(named, required)) throw missing(bundle, required, `EXACT_PROFILE (${named.name}) does not allow ${required.length > 1 ? 'them' : 'it'}`);
    return named;
  }
  const covering = coveringProfiles(udid, bundle, installed, team);
  if (!covering.length) throw missingDevelopmentProfile(udid, bundle, team, phoneName);
  const found = covering.filter((p) => allows(p, required));
  if (!found.length) throw missing(bundle, required, `no development profile on this Mac for ${bundle} on this phone allows ${required.length > 1 ? 'them' : 'it'}`);
  return found[0];
}

/** A missing development profile, naming what Xcode must sign to create one. */
export function missingDevelopmentProfile(udid, bundle, team, phoneName = udid) {
  const phone = phoneName && phoneName !== udid ? `${phoneName}, ${udid}` : udid;
  return new Error(`no development provisioning profile on this Mac${team ? ` for team ${team} (DEVELOPMENT_TEAM)` : ''} covers ${bundle} on this phone (${phone}); in Xcode, run a project with bundle ID ${bundle}${team ? ` and team ${team}` : ' and your team'} on that phone with Automatically manage signing enabled, then retry (or ${team ? "unset DEVELOPMENT_TEAM to take any team's profile, or " : ''}name a profile with EXACT_PROFILE)`);
}

/** The unexpired development profiles on this Mac that cover the phone and the bundle id (the team's wildcard or
 * the id itself), the longest-lived first. */
export function coveringProfiles(udid, bundle, installed = installedProfiles(), team = null) {
  return profilesFor(bundle, installed).filter((p) => (!team || p.team === team) && iosProfile(p) && p.dev && !profileMisfit(p, udid)).sort(byExpiry);
}
/** The profile for a bundle the app embeds under an App ID of its own (its App Clip), chosen as `entryProfiles`
 * chooses the widget extension's: for `bundle` itself or the team's wildcard, of the app's profile's (`like`) team,
 * App ID prefix and method, naming the phone or, for an archive, every device the app's names, allowing `required`
 * (the clip's parent-application-identifiers, which only an App ID registered with the App Clip capability grants);
 * the longest-lived. Or a refusal naming what each profile for the id lacked. */
export function embeddedProfile(udid, bundle, like, required = [], installed = installedProfiles()) {
  const candidates = profilesFor(bundle, installed).filter(iosProfile).sort(byExpiry);
  const why = candidates.map((p) => [p, profileMisfit(p, udid, like) ?? (allows(p, required) ? null : `does not allow ${lacks(p, required).join(', ')}`)]);
  const found = why.find(([, reason]) => !reason)?.[0];
  if (found) return found;
  throw new Error(`host/apple: no ${METHODS[like.method]} profile on this Mac covers ${bundle} as the app's (${like.name}) does: ${why.length ? why.map(([p, reason]) => `"${p.name}" ${reason}`).join('; ') : 'none is for it'}. In the Apple Developer portal, register the App ID ${bundle}${required.length ? ` with ${required.join(', ')}` : ''}, make a ${METHODS[like.method]} profile for it of team ${like.team}${udid ? ' that includes this phone' : ''}, and install it.`);
}
export const byExpiry = (a, b) => b.expires - a.expires;
export const iosProfile = p => !p.platforms?.length || p.platforms.includes('iOS');
/** The profiles among `installed` whose App ID is `bundle` itself or its team's wildcard. */
export const profilesFor = (bundle, installed) => installed.filter((p) => p.appId === `${p.prefix}.*` || p.appId === `${p.prefix}.${bundle}`);
/** How a profile distributes: development, iOS Ad Hoc or enterprise, Mac Developer ID, or the App Store. */
export const METHODS = { development: 'development', adhoc: 'Ad Hoc', enterprise: 'enterprise', developerid: 'Developer ID', appstore: 'App Store' };
/** Why `p` cannot sign on `udid`, or null when it can: its expiry, and with a phone named, that phone (an App Store
 * profile names none). `like`, another profile (the app's, when `p` is for its extension), asks for its team, App
 * ID prefix and method too, and that `p` name the phone, or, for an archive, every device `like` names. */
export function profileMisfit(p, udid, like = null) {
  if (!(p.expires > new Date())) return `expired on ${p.expires.toISOString().slice(0, 10)}`;
  if (!like) return !udid || p.devices.includes(udid) ? null : p.devices.length ? 'does not include this phone' : `is ${METHODS[p.method]} and names no device`;
  if (p.team !== like.team) return `is team ${p.team}'s, not ${like.team}'s`;
  if (p.prefix !== like.prefix) return `has the App ID prefix ${p.prefix}, not ${like.prefix}`;
  if (p.method !== like.method) return `is ${METHODS[p.method]}, not ${METHODS[like.method]}`;
  if (!like.devices.length) return null;
  const missing = (udid ? [udid] : like.devices).filter((d) => !p.devices.includes(d));
  return !missing.length ? null : udid ? 'does not include this phone' : `does not include ${missing.length} of the ${like.devices.length} devices the app's profile names`;
}

/** Every `.mobileprovision` or `.provisionprofile` Xcode or a download installed, decoded once per process (a hundred of them is two seconds). */
let decoded = null;
export function installedProfiles() {
  const dirs = [...(process.env.EXACT_PROFILE_DIR ? [resolve(process.env.EXACT_PROFILE_DIR)] : []), ...['Library/Developer/Xcode/UserData/Provisioning Profiles', 'Library/MobileDevice/Provisioning Profiles'].map((d) => resolve(homedir(), d))].filter(existsSync);
  return decoded ??= dirs.flatMap((d) => readdirSync(d).filter((f) => /\.(mobileprovision|provisionprofile)$/.test(f)).map((f) => decodeProfile(resolve(d, f))));
}

/** The fields of a `.mobileprovision` or `.provisionprofile` this script reads (a CMS-signed XML plist). */
export function decodeProfile(path, os = process.platform, { read: readProfile = read } = {}) {
  const result = os === 'linux' ? readProfile('openssl', ['smime', '-verify', '-inform', 'DER', '-in', path, '-noverify'])
    : readProfile('security', ['cms', '-D', '-i', path]);
  if (os === 'linux' && result.status !== 0) throw new Error(`host/apple: cannot decode provisioning profile ${path}: ${result.stderr || result.error?.message}`);
  const xml = result.stdout ?? '';
  const str = (key) => new RegExp(`<key>${key}</key>\\s*<string>([^<]*)</string>`).exec(xml)?.[1];
  const strings = (key) => [...(new RegExp(`<key>${key}</key>\\s*<array>([\\s\\S]*?)</array>`).exec(xml)?.[1] ?? '').matchAll(/<string>([^<]*)<\/string>/g)].map((m) => m[1]);
  const team = /<key>TeamIdentifier<\/key>\s*<array>\s*<string>([^<]*)<\/string>/.exec(xml)?.[1];
  const devices = strings('ProvisionedDevices');
  const platforms = strings('Platform'), mac = platforms.length > 0 && platforms.every(p => p === 'OSX');
  const expires = /<key>ExpirationDate<\/key>\s*<date>([^<]*)<\/date>/.exec(xml)?.[1];
  // The keys of its Entitlements whose value is not `false`: what it allows.
  const allowed = /<key>Entitlements<\/key>\s*<dict>([\s\S]*?)<\/dict>/.exec(xml)?.[1] ?? '';
  const entitlements = [...allowed.matchAll(/<key>([^<]+)<\/key>\s*(<false\/>)?/g)].filter((m) => !m[2]).map((m) => m[1]);
  const appId = str('application-identifier') ?? str('com.apple.application-identifier');
  // What an app with widgets or Live Activities needs of it (LLP 1126.001 WP13): the App Groups it grants, the
  // Keychain groups (the team's `<prefix>.*`, usually) and the push environment.
  // Mac development profiles name devices; their unrestricted debugger entitlement need not be in the profile.
  // Developer ID covers all devices, and the App Store names none (Apple TN3125):
  // https://developer.apple.com/documentation/technotes/tn3125-inside-code-signing-provisioning-profiles
  const dev = mac ? devices.length > 0 : /<key>(?:com\.apple\.security\.)?get-task-allow<\/key>\s*<true\/>/.test(xml);
  const allDevices = /<key>ProvisionsAllDevices<\/key>\s*<true\/>/.test(xml);
  const aps = mac ? str('com.apple.developer.aps-environment') ?? str('aps-environment')
    : str('aps-environment') ?? str('com.apple.developer.aps-environment');
  return { path, name: str('Name'), team, appId, prefix: appId?.split('.')[0], devices, dev, expires: new Date(expires ?? 0), entitlements, platforms,
    method: dev ? 'development' : devices.length ? 'adhoc' : allDevices ? (mac ? 'developerid' : 'enterprise') : 'appstore',
    groups: strings('com.apple.security.application-groups'), keychain: strings('keychain-access-groups'), aps: aps ?? null };
}

/** The Apple Development identity (its SHA-1) for a team, from the keychain; EXACT_IDENTITY names one. */
export function identity(team, { env = process.env, read: readSigning = read } = {}) {
  if (env.EXACT_IDENTITY) return env.EXACT_IDENTITY;
  const valid = [...(readSigning('security', ['find-identity', '-v', '-p', 'codesigning']).stdout ?? '').matchAll(/\d+\) ([0-9A-F]{40}) "(Apple Development: [^"]+)"/g)].map((m) => ({ sha1: m[1], name: m[2] }));
  const pems = (readSigning('security', ['find-certificate', '-a', '-c', 'Apple Development', '-p']).stdout ?? '').split('-----END CERTIFICATE-----').filter((c) => c.includes('BEGIN CERTIFICATE'));
  for (const pem of pems) {
    const x = readSigning('openssl', ['x509', '-noout', '-subject', '-fingerprint', '-sha1'], { input: pem + '-----END CERTIFICATE-----\n' }).stdout ?? '';
    const sha1 = /Fingerprint=([0-9A-F:]+)/i.exec(x)?.[1].replace(/:/g, '');
    const ou = /OU\s*=\s*([A-Z0-9]+)/.exec(x)?.[1];
    const id = valid.find((v) => v.sha1 === sha1);
    if (id && ou === team) return id.sha1;
  }
  throw new Error(`no valid "Apple Development" identity for team ${team} in the keychain (security find-identity -v -p codesigning); name one with EXACT_IDENTITY=<sha1>`);
}

/** The first valid "Apple Development" identity in the keychain (EXACT_IDENTITY names one), or null: the macOS binary is then ad-hoc signed. */
export function macIdentity() {
  if (process.env.EXACT_IDENTITY) return process.env.EXACT_IDENTITY;
  const m = /\d+\) ([0-9A-F]{40}) "Apple Development: /.exec(read('security', ['find-identity', '-v', '-p', 'codesigning']).stdout ?? '');
  return m ? m[1] : null;
}

/** This Mac's Provisioning UDID, which a Mac development profile names; on Apple silicon it is not the hardware UUID. */
export function macHardwareID(readOut = (command, args) => read(command, args).stdout ?? '') {
  try {
    const hardware = JSON.parse(readOut('system_profiler', ['SPHardwareDataType', '-json']));
    const id = hardware.SPHardwareDataType?.find(row => row.provisioning_UDID)?.provisioning_UDID;
    if (id) return id;
  } catch { /* Older system_profiler versions may supply only IOPlatformUUID. */ }
  return /"IOPlatformUUID"\s*=\s*"([^"]+)"/.exec(readOut('ioreg', ['-rd1', '-c', 'IOPlatformExpertDevice']))?.[1] ?? null;
}

/** Every orientation: an iPad app supports them all, as its multitasking requires. Without this key iPad reads
 * the iPhone's. */
export const IPAD_ORIENTATIONS = ['UIInterfaceOrientationPortrait', 'UIInterfaceOrientationPortraitUpsideDown', 'UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight'];

/** The iPhone orientations the manifest's `orientation` allows, or null for `any` or none, which leaves iOS's own
 * (portrait and both landscapes). A `-primary` is the device's own way up, as the Screen Orientation API has it:
 * `landscape-primary` is turned left, the home indicator on the right, which UIKit calls LandscapeRight. Every
 * portrait is upright, the only portrait an iPhone with Face ID offers. */
export function phoneOrientations(orientation) {
  const up = 'UIInterfaceOrientationPortrait';
  const right = 'UIInterfaceOrientationLandscapeRight', left = 'UIInterfaceOrientationLandscapeLeft';
  return { natural: [up], portrait: [up], 'portrait-primary': [up], 'portrait-secondary': [up], landscape: [left, right],
    'landscape-primary': [right], 'landscape-secondary': [left] }[orientation] ?? null;
}

/** The Info.plist keys that lock the iPhone to the manifest's `orientation` and leave iPad all four, over a
 * distribution's default orientations; none for `any` or no `orientation`. */
export function orientationKeys(orientation) {
  const phone = phoneOrientations(orientation);
  return phone ? { UISupportedInterfaceOrientations: phone, 'UISupportedInterfaceOrientations~ipad': IPAD_ORIENTATIONS } : {};
}
