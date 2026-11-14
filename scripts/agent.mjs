#!/usr/bin/env bun
//   tree | layout | state | logs [--all] | check [light|dark|both] | screenshot <png> [window
//     [status|app|pictureinpicture [<image>]]] (an iOS simulator's window: the display's pixels when they are this
//     app's, still; `app` the app's own drawing; `pictureinpicture` Exact's floating window on macOS, the image's where
//     several float) | screenshot <png> compare <reference.png> | screenshot <png|apng> over <ms> every <ms> [window] |
//     screenshot <path> <canvas> save | screenshot <png> <canvas> from <yaw> <pitch> <distance> | from top <height> |
//     from camera [over <ms> every <ms>] (a world's look frame, LLP 1127.000) | state <canvas> draws | look · state
//     <canvas>:<entity> visible
//   tap <target> [at <x> <y> (a press at a point; plain, it presses what it names) | wheel <dx> <dy> [gesture | fling |
//     hold] [at <x> <y>] [modifiers <Control…>] | wheel 0 0 release | drop <path…> | dragenter <path…> | dragleave |
//     into <key> [block <v>] [inline <v>] | hover | hold-state active|hover|focus-visible|none | history <n> |
//     {"history":n} | mouse | dblclick | contextmenu | auxclick | clicks <1-3> (each [at <x> <y>] [modifiers
//     <Shift+Meta…>]) | modifiers <Shift+Meta…> | pinch <scale> [at <x> <y>] | mediasession <action> [seconds] |
//     pictureinpicture minimize|back|close|hover (Exact's picture-in-picture window, macOS) | drag <dx> <dy> [from <x>
//     <y>] [mouse] [modifiers <M>] [press <ms>] [over <ms>] [hold <ms>] [into keyboard] [during "<op>" …, inside the hold under --touch
//     platform] [then <dx> <dy> over <ms> hold <ms> during "<op>" …: another leg, same iOS touch] | drag to <target>
//     [at <x> <y>] […]] | type <target> <text…> (the navigation root: a path or a deep link) | type <target> key <Name>
//     | type <target> copy|cut|paste <text…> | type <target> paste --file <path…> | type <target> paste --image <png>
//   tap <target> down [at <x> <y>] [modifiers <M>] · tap move [by] <x> <y> [over <ms>] [modifiers <M>] · tap hold <ms>
//     · tap up [modifiers <M>] · tap cancel (a held contact, LLP 1035.003 D1; its modifiers held until another names
//     others; a hold moves the app's clock by <ms>; a plain tap is pointerdown, pointerup, then the press; the target
//     shows pressed until the lift, so `tap row down` "screenshot held.png" `tap up` pictures it, under --touch
//     platform too, where the holds, reads and clock +<ms> between run inside one real touch played at tap up)
//   tap @N <choice> | type @N <value> (a held device request, by ticket: LLP 1069.007 D4; a listed entry's click takes
//     no choice; a listed call's, the person's act: answer, hangup, …)
//   tap @<id> <choice> | type @<id> <value> (by the node it answers at, or its capability: files F11)
//   clock <ms|+ms|+ms real [until <path>]|settle|data|held [<target>…] [<n>s]|until state <path> == <literal> within
//     <ms>> (+ms real until: a test harness's signal file, ending the real wait early; until state: the wall clock, the
//     clock frozen, until state reads the JSON literal) | resize <w>x<h> | resize <w> <h> | post [<surface>] <text>
//     (type <surface> message <text>: a world's Input::messages) | close | sample <x> <y> […] (extended linear sRGB
//     under view points, LLP 1100 D12) | prefer <media feature or page fact> <value> […] | perf [<target>] [during
//     "<op>" …] | perf frames [late <n>] | perf hatches [during "<op>" …] | perf streams [start]
//   bun scripts/agent.mjs trace <file> | trace --phone <name|udid> | trace --device | trace --pid <pid> [--plan
//     <plan|map|dir>] [--out <file>] (a development session's trace, LLP 1079 D5: no app runs; a phone's last, copied
//     off it; a running Mac or simulator app's, asked by SIGUSR1, LLP 1142 D5)
// A target is a testId, a view id, or else a view's exact label or text (interactive first; several refuse, naming
// them; `tap "Sky off"` quotes several words); each op is one argument (quote it).
// `tap … wheel <dx> <dy> gesture` sends the wheel as a trackpad's gesture — began, changed, and the zero-delta lift
// that ends it (LLP 1033 D4a, macOS only); `fling` and `hold` are a trackpad's through the window server, which AppKit
// runs with its momentum and rubber band, and `wheel 0 0 release` lifts a hold (LLP 1142 D3, macOS only, the terminal's
// Accessibility grant). Linux scrollers with a `scrollend` handler instead receive a reader sample with a 100 ms quiet
// deadline. `tap … hover` moves the pointer onto the target (LLP 1005 §3). --device: build/install first with build.mjs
// --device; no Mac-local plan/assets paths.
import {
  refusalMessage,
  MalformedOperation,
  inputJSON,
  holdOf,
  typeCommand,
  tapWords,
  postCommand,
  refuseDriveFlag,
  resizeArguments,
} from './agent-keys.mjs';
import {
  Cdp,
  browserDiagnosticNoise,
  exclusiveNativeStore,
  resolveDevice,
  defaultStore,
  noteDefaultStore,
  traceLocators,
  parseFlags,
  launchEntries,
} from './agent-launch.mjs';
import { widgetStep } from './agent-widget.mjs';
import {
  render,
  perfOp,
  readTrace,
  renderTrace,
  pidTrace,
  writeTrace,
  phoneTrace,
  stateOp,
  LOOK_READS,
  worldLook,
  layoutArgs,
} from './agent-inspect.mjs';
import { guardFlags } from './help.mjs';
import { relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { checkOp } from './agent-check.mjs';
import { compareScreenshot, referenceNote } from './agent-compare.mjs';
import { parseDrag, heldRead } from './agent-drag.mjs';
import { driveNotes, runTests, reloadWeb, fetchOrigins, loopbackOrigins, faultRequest } from './agent-test.mjs';
import { phone } from '../host/apple/devices.mjs';
import { resolveApp, webDist as defaultWebDist } from './app.mjs';
import { open } from './agent-session.mjs';
export {
  sourceMapReader,
  identifyInspectedNode,
  render,
  tapRefusal,
  worldView,
  lookPicture,
} from './agent-inspect.mjs';
export { jsonLines, hangup } from './agent-wire.mjs';
export {
  MalformedOperation,
  resizeArguments,
  postCommand,
  cdpKey,
  browserKey,
  nativeKey,
  typeFor,
  ticketOf,
  holdOf,
  heldTicket,
  typeArguments,
  typeCommand,
  pickedPaths,
  mouseContact,
  pasteChord,
  deliverClipboard,
  tapWords,
  pointerGap,
} from './agent-keys.mjs';
// `prefer`'s tables and the web carrier's CDP path
export { LAUNCH_MEDIA, PREFERENCES, PAGE_FACTS, FOLD_FACTS, displayFeatures } from './agent-prefer.mjs';
/** `fail fetch <url-prefix> [times <n>]` and `pass fetch <url-prefix>` as the `prefer` request with `faults` every
 * carrier answers (LLP 1103 D3): the network is the environment, so this is a form of `prefer`, not another
 * operation (LLP 1012 §1). */
export { faultRequest } from './agent-test.mjs';
export { VIEWPORT, assertWebDistApp, webFrame } from './agent-web.mjs';
export { open, clockSpan } from './agent-session.mjs';
export { runTests, textOf } from './agent-test.mjs';
export { Cdp, browserDiagnosticNoise };
/** The hosts `open` carries a drive on. */
/** How long a test runs before `test` says where it is, and how often after. */
export const HEARTBEAT_MS = 30000;

/** Where a long test is: its line of its last, how long it has run, and, at a drag, why a drag is slow. Not a
 * `test "…":` line, which conform.mjs reads as a result. */
export const heartbeat = ({ name, line, last, op, since }, now) => {
  const s = Math.round((now - since) / 1000), took = s < 60 ? `${s}s` : `${Math.floor(s / 60)}m${String(s % 60).padStart(2, '0')}s`;
  const drag = op === 'drag' ? '; a drag past the viewport is several strokes, each landing before the next' : '';
  return `still running test "${name}": ${took}, at line ${line} of ${last} (${op})${drag}`;
};

export const DRIVEN_HOSTS = ['web', 'macos', 'mac', 'ios', 'linux', 'android', 'windows', 'host', 'host-ios'];
export async function main(argv, injected = {}) {
  const { flags, rest } = parseFlags(argv);
  guardFlags(flags.test ? 'test' : 'agent', rest, { parsed: true }); // --help, or a flag parseFlags left (never an op)
  // the simulator, named as EXACT_SIM names it (devices.mjs `simulator`)
  if (flags.sim) process.env.EXACT_SIM = flags.sim;
  const [host, ...ops] = rest;
  // A host the driver does not carry is refused: `open` falls to the web for any other word, so `iOS` or a typo ran
  // the drive on the web and recorded it as that host's.
  if (host && host !== 'trace' && !DRIVEN_HOSTS.includes(host)) {
    console.error(`agent: no host \`${host}\`; name one of ${DRIVEN_HOSTS.join(', ')}`);
    return 2;
  }
  if (flags.live) {
    if (!['macos', 'mac'].includes(host) || flags.test)
      throw new Error('--live requires a macOS drive without --test');
    if (flags.timing && flags.timing !== 'platform')
      throw new Error('--live keeps the platform clock; --timing agent is incompatible');
    flags.timing = 'platform';
  }
  const browser = flags.browser ?? (host === 'web' ? process.env.EXACT_WEB_BROWSER : undefined),
    help = argv.includes('--help') || argv.includes('-h'); // help before any work
  if (host && flags.test && !help) {
    // A test's `drag` on an iOS simulator is a real gesture by itself; `--touch platform` makes every tap one too
    // (LLP 1080.000).
    if (flags.touch && !['agent', 'platform'].includes(flags.touch))
      throw new Error(
        `--touch ${flags.touch} is not taken with --test: a test's \`drag\` is a real gesture by itself on an iOS simulator; --touch platform makes every tap one too (LLP 1080.000)`,
      );
    // Each test's line as it ends, and, while one runs past HEARTBEAT_MS, where it is every HEARTBEAT_MS: a run of
    // long tests (a few hundred drags) said nothing after the server's line until the last test ended.
    const said = new Set(),
      report = (t) => {
        said.add(t);
        console.log(`test "${t.name}": ${t.skipped ? `skipped on ${host} (${t.skipped})` : t.failures.length ? 'FAIL' : 'ok'}`);
        for (const f of t.failures) console.error('  ' + f);
        for (const n of t.notes ?? []) console.error(`  note: ${n}`);
      };
    let at = null;
    const atLine = (step) => { at = at?.index === step.index ? { ...at, ...step } : { ...step, since: Date.now() }; };
    const every = injected.heartbeatMs ?? HEARTBEAT_MS,
      beat = setInterval(() => {
        if (at && Date.now() - at.since >= every) console.error(heartbeat(at, Date.now()));
      }, every);
    beat.unref?.();
    return (await import('./verify.mjs')).withAppServer(
      resolveApp(flags.app).dir,
      async () => {
        const r = await (injected.runTests ?? runTests)({
          atLine,
          atResult: (t) => { at = null; report(t); },
          host,
          browser,
          file: flags.test,
          plan: flags.plan,
          app: flags.app,
          size: flags.size,
          device: flags.device,
          phone: flags.phone,
          reader: flags.reader,
          url: flags.url,
          seed: flags.seed,
          locale: flags.locale,
          formatLocale: flags.formatLocale,
          timeZone: flags.timeZone,
          epoch: flags.epoch,
          failFetch: flags.failFetch,
          storage: flags.storage,
          touch: flags.touch,
          chrome: flags.chrome,
          scale: flags.scale,
          port: flags.port,
        }).finally(() => clearInterval(beat));
        for (const t of r.results) if (!said.has(t)) report(t);
        console.log(`${r.passed} passed, ${r.failed} failed${r.skipped ? `, ${r.skipped} skipped` : ''}`);
        // The run, against the build it used (LLP 1146 D2: `bun scripts/evidence.mjs`).
        (await import('./evidence.mjs')).recordRun(flags.app, {
          host,
          what: `test ${relative(process.cwd(), resolve(flags.test)).startsWith('..') ? resolve(flags.test) : relative(process.cwd(), resolve(flags.test))}`,
          result: {
            tests: true,
            passed: r.passed,
            failed: r.failed,
            skipped: r.skipped ?? 0,
            failing: r.results.filter(t => t.failures.length).map(t => t.name),
          },
          timing: process.env.EXACT_AGENT_TIMING ?? 'agent',
          touch: flags.touch,
          device: flags.device || flags.phone,
          sim: flags.sim,
          webDist: flags.webDist,
        });
        return r.failed ? 1 : 0;
      },
      { verb: 'test', optional: true },
    );
  }
  // A trace a person's session saved (LLP 1079 D5), read back with no app running.
  if (host === 'trace' && (ops.length === 1 || (!ops.length && (flags.phone || flags.device || flags.pid)))) {
    const traces = name => {
      try {
        return resolve(resolveApp(flags.app ?? name).target, 'traces');
      } catch {
        return resolve(process.cwd(), 'target', 'traces');
      }
    };
    const file =
      ops[0] ??
      (flags.pid ? await pidTrace(Number(flags.pid), traces) : phoneTrace(phone(flags.phone), resolveApp(flags.app)));
    const t = await readTrace(file, traceLocators, flags.plan);
    // A trace asked of a process is kept with its map, so it reads on any machine (LLP 1142 D2).
    if (flags.out || flags.pid) writeTrace(t, flags.out ?? file);
    console.log(flags.json ? JSON.stringify(t) : renderTrace(t));
    return 0;
  }
  if (!host || !ops.length) {
    console.error(
      [
        'usage: bun scripts/agent.mjs <web|macos|ios|linux|android|windows|host|host-ios> [--browser chrome|firefox|webkit] [--app <name>] [--plan <file> | --url <url>] [--world <file>] [--device] [--phone <name|udid>] [--session <label>] [--open <document>] [--storage <name>] [--seed <n>] [--locale <tag>] [--format-locale <tag>] [--time-zone <zone>] [--epoch <ISO|ms>] [--live] [--timing platform] [--touch platform] [--chrome platform] [--size <w>x<h>] [--scale <n>] [--port <n>] [--json] <op> [<op> …]',
        '  desktop carriers open 420x900 unless --size names another viewport; a target is a testId, a view id, or a view\'s exact label or text (`tap "Sky off"`); tap and type scroll a target out of view into it first; web defaults to chrome; EXACT_WEB_BROWSER selects the same option. Install the other engines outside the repo: bunx playwright@1.64.1 install firefox webkit',
        '  ' +
          [
            'tree',
            'layout',
            'state',
            'logs [--all]',
            'check [light|dark|both] (what is wrong with the screen, in text: run it before a screenshot)',
            "screenshot <png> [window [status|app|pictureinpicture [<image>]]] (an iOS simulator's window: the display's pixels when they are this app's, still; `app` the app's own drawing; `pictureinpicture` Exact's floating window on macOS)",
            "screenshot <png> compare <reference.png> (where and on which nodes the screen differs from it, drawn in <png>.diff.png and the regions up close in <png>.regions.png; an iOS simulator's window with its bars, elsewhere the viewport)",
            'screenshot <png|apng> over <ms> every <ms> [window]',
            'screenshot <path> <canvas> save',
            'screenshot <png> <canvas> from <yaw> <pitch> <distance>',
            'from top <height>',
            "from camera [over <ms> every <ms>] (a world's look frame: the clock's tick drawn offscreen from that view, with its tells)",
            'state <canvas> draws',
            'state <canvas> look',
            'state <canvas>:<entity> visible',
            'tap <target> [at <x> <y> (a press at a point; plain, it presses what it names)',
            'wheel <dx> <dy> [gesture',
            'fling',
            'hold] [at <x> <y>] [modifiers <Control…>]',
            'wheel 0 0 release',
            'drop <path…> (files and folders)',
            'dragenter <path…> (a drag of them held over it)',
            'dragleave (it leaves undropped)',
            'hover',
            'hold-state active|hover|focus-visible|none (held by the host as the real state, until real input on it or none)',
            'history <n>',
            '{"history":n}',
            'mouse',
            'dblclick',
            'contextmenu',
            'auxclick (the middle button)',
            'clicks <1-3> (each [at <x> <y>: from its top left] [modifiers <Shift+Meta…>])',
            'modifiers <Shift+Meta…>',
            'pinch <scale> [at <x> <y>]',
            'mediasession <action> [seconds]',
            "pictureinpicture minimize|back|close|hover (Exact's picture-in-picture window, macOS)",
            'drag <dx> <dy> [from <x> <y>] [mouse: the left button, desktop pointers] [modifiers <M>] [press <ms>] [over <ms>] [hold <ms>] [into keyboard: iOS --touch platform, keyboard up; end above the screen bottom safe area] [during "<op>" …: after the move, the finger down; inside the hold under --touch platform] [then <dx> <dy> [over <ms>] [hold <ms>] [during "<op>" …]: another leg, same iOS touch]',
            'drag to <target> [at <x> <y>: from its top left] [same options]]',
            'tap <target> down [at <x> <y>] [modifiers <M>], then tap move [by] <x> <y> [over <ms>] [modifiers <M>]',
            'tap hold <ms>',
            'tap up [modifiers <M>]',
            'tap cancel (the target shows pressed until the lift: "tap row down" "screenshot held.png" "tap up"; under --touch platform one real touch at tap up, the holds, reads and clock +<ms> between run inside it; a word a form does not use is refused; a form a carrier cannot deliver answers unsupported)',
            'type <target> <text…> (a select, date, time, range or checkbox: its value; the navigation root: a path, or a deep link as euxy://p/1, through protocol_handlers on macOS and iOS)',
            'type <target> key <Name> [for <ms>]',
            'type <target> copy',
            'cut',
            'paste <text…>',
            'paste --file <path…>',
            'paste --image <png> (the clipboard event at the focus; --file pastes files and folders, --image the data of one image with no file)',
            'type <surface> message <text…>',
            "post [<surface>] <text> (one word or one quoted string: a message the world reads from Input::messages next tick, as the page's postMessage(text, surface) sends it; no surface, the page's one world)",
            'tap @N|@<id> <choice>',
            'type @N|@<id> <value>',
            "clock <ms|+ms|+ms real [until <path>]|settle|data|held [<target>…] [<n>s]|until state <path> == <literal> within <ms> (real time, the clock frozen, until state <path> reads the JSON literal; a miss fails naming the last value: a stream's state after its first message)>",
            'sample <x> <y> […]',
            'resize <w>x<h>',
            'resize <w> <h> (the window: web, macOS, Linux)',
            "close (the window's close button, asking its beforeunload: web, macOS)",
            "back (the platform's Back: an open dialog's cancel, else the top sheet dismissed, else the pushed route popped)",
            'prefer <media feature, page fact, posture folded|continuous, segments <cols>x<rows> [gap <points>]> […]',
            'prefer extension <extension>.<source>=<JSON>|clear (the source of a native module, LLP 1124 D7)',
            "prefer permission.<name> granted|denied|prompt (the permission of a module; --permission <name>=<state> sets it at launch, --bind <extension> answers it from its implementation on Apple and Android; its tickets are `tap @t allow|deny|cancel` and `type @t <value>`, LLP 1133.002 §6; a module's listed entry, `state.notifications`, is `tap @ref`, and --notification '<entry>' hands a relaunch one, LLP 1133.004 §7)",
            'fail fetch <url-prefix> [times <n>]',
            'pass fetch <url-prefix> (LLP 1103: a matching fetch fails as a refused connection; --fail-fetch <url-prefix> arms one before the first data load)',
            "call <tool> <json> [--door <door>] (a system's call of a tool, as its door hands it, LLP 1133.023 §10: state.intents lists the tools and the calls, state intents.entities <kind> [<text>] answers as the content index's query; --call '<tool> <json>' is a cold one at launch)",
            'perf [<target>] [during "<op>" …]',
            'perf world:<surface> [during "<op>" …] (tick spans; during refuses a remount, restore or recording replacement)',
            'perf frames [late <n>]',
            'perf frames health [app|scroll|window] [during "<op>" …]',
            'perf frames capture start [ms]|stop|clear',
            'perf frames report [app|scroll|window]',
            'perf frames live <ms>',
            "perf streams [start] (each stream message's receipt to mapper entry, decode and commit, LLP 1133.003 §9)",
          ].join(' | '),
        "       bun scripts/agent.mjs trace <file>   (a development session's trace, LLP 1079 D5)",
        "       bun scripts/agent.mjs trace --phone <name|udid> | trace --device [--app <name>]   (a phone's last Save Trace, copied off it)",
        "       bun scripts/agent.mjs trace --pid <pid> [--app <name>]   (a running development Mac or simulator app's, asked by SIGUSR1 and kept with its source map, LLP 1142 D5)",
        '       trace … --plan <plan|map|directory> joins sites from that build; --out <file> writes the trace with its map joined (LLP 1142 D2)',
        "       bun scripts/agent.mjs <host> --test <file.test.contract>   (LLP 1017 P7: the file's `test` blocks, run here)",
      ].join('\n'),
    );
    return 2;
  }
  const named = flags.storage !== undefined;
  flags.storage = defaultStore(host, flags);
  referenceNote(ops, resolveApp(flags.app).dir);
  // A named store is kept between drives on a native host unless the drive asks it emptied (`--reset-storage`); on the
  // web, Chrome's profile for it starts empty unless the drive keeps it (`--keep-storage`, LLP 1086 EX4). A native host
  // empties the whole scratch tree at its first boot (picker.rs
  // `empty_fresh_tree`), refusing one under a link. A drive that compares with a reference starts empty on every host
  // unless kept: what an earlier drive stored is no part of the screen (euxy port); it, or --reset-storage, empties the
  // app server's database too.
  const compares = ops.some(l => /^\s*screenshot\s.*\scompare\s/.test(l)),
    fresh = flags.storage !== undefined && (host === 'web' || compares ? !flags.keepStorage : flags.resetStorage);
  if (fresh && host !== 'web' && !flags.resetStorage)
    console.error(
      `note: --storage ${flags.storage} starts empty, as a drive that compares with a reference does, where one that does not keeps it (--keep-storage keeps it here)`,
    );
  // The app's own server, if it has one (LLP 1116.008 D1): started under the store's lock, and stopped if the launch
  // fails.
  let served = null;
  const launch = async () => {
    served = injected.driveServer
      ? await injected.driveServer()
      : await (
          await import('./verify.mjs')
        ).driveServer(resolveApp(flags.app).dir, flags.storage, undefined, {
          fresh: fresh && (compares || !!flags.resetStorage),
        });
    try {
      return await (injected.open ?? open)({
        host,
        browser,
        ax: ops.some(line => /^\s*tree\s+--ax(?:\s|$)/.test(line)),
        env: {
          ...(flags.live ? { EXACT_AGENT: 'live' } : {}),
          ...(fresh ? { EXACT_AGENT_STORAGE_FRESH: '1' } : {}),
          ...(flags.login ? { EXACT_LOGIN_LAUNCH: flags.login } : {}),
        },
        plan: flags.plan,
        world: flags.world,
        size: flags.size,
        app: flags.app,
        session: flags.session,
        documents: flags.open,
        url: flags.url,
        device: flags.device,
        phone: flags.phone,
        reader: flags.reader,
        timing: flags.timing,
        touch: flags.touch,
        chrome: flags.chrome,
        bind: flags.bind,
        storage: flags.storage,
        seed: flags.seed,
        locale: flags.locale,
        formatLocale: flags.formatLocale,
        timeZone: flags.timeZone,
        epoch: flags.epoch,
        failFetch: flags.failFetch,
        scale: flags.scale,
        port: flags.port,
      });
    } catch (e) {
      await served?.stop();
      throw e;
    }
  };
  // A native named store is one drive's at a time, on its device: another drive's reset would empty it under this one.
  // The device is resolved first, and the drive launches that one, so a default and its name are one lock.
  const locking = host !== 'web' && flags.storage !== undefined,
    appId = locking ? resolveApp(flags.app).id : null;
  const device = locking ? (injected.resolveDevice ?? resolveDevice)(host, flags, appId) : null;
  if (locking && !named && !fresh) noteDefaultStore({ host, device, appId, dir: resolveApp(flags.app).dir });
  // A miss names a kept store (`find`): the euxy port's `tap play` missed on a mode an earlier drive persisted.
  const s = Object.assign(
    locking
      ? await (injected.exclusiveNativeStore ?? exclusiveNativeStore)(
          { host: host === 'mac' ? 'macos' : host, device, appId, storage: flags.storage },
          launch,
        )
      : await launch(),
    locking && !fresh ? { keptStore: flags.storage } : {},
  );
  let at = 0;
  // One op line; `perf … during "<op>" …` drives its own through here.
  const step = async line => {
    if (flags.widget || flags.activity)
      return widgetStep(s, flags.widget ?? flags.activity, line, flags.activity ? 'activity' : 'widget');
    // inside a played contact, answered at its tap up (agent-drag.mjs)
    if (heldRead(s, line, () => step(line))) return null;
    const trimmed = line.trim();
    // `type` keeps the text tail, including newlines and \n, which a
    // whitespace split would flatten (notes: a body typed as one line).
    if (/^pick\s/.test(trimmed)) {
      const [target, value] = typeCommand(trimmed.replace(/^pick/, 'type'));
      return ['pick', await s.type(target.startsWith('@') ? target : '@' + target, value + '\n')];
    }
    if (/^(?:type|post)(\s|$)/.test(trimmed))
      return [trimmed.slice(0, 4), await s.type(...(trimmed.startsWith('post') ? postCommand : typeCommand)(trimmed))];
    // A target of several words is quoted inside the op: `tap "Sky off"`.
    const named = trimmed.match(/^(tap|tree|state|layout|perf)\s+"((?:[^"\\]|\\.)*)"(?:\s+([\s\S]*))?$/);
    // A picture's path with spaces is quoted, in either quote, and read as written: `C:\Users\…` keeps its backslashes.
    const pathed = trimmed.match(/^(screenshot)\s+(["'])([^"']+)\2(?:\s+([\s\S]*))?$/);
    const [op, ...args] = named
      ? [named[1], inputJSON(`"${named[2]}"`), ...(named[3] ? named[3].split(/\s+/) : [])]
      : pathed
        ? [pathed[1], pathed[3], ...(pathed[4] ? pathed[4].split(/\s+/) : [])]
        : trimmed.split(/\s+/);
    if (
      op === 'screenshot' &&
      !pathed &&
      args.length > 1 &&
      !/\.a?png$/i.test(args[0]) &&
      /\.a?png$/i.test(args.at(-1))
    )
      throw new MalformedOperation(`screenshot: a path with spaces is quoted, as screenshot '${args.join(' ')}'`);
    let r, how;
    switch (op) {
      case 'tree':
        r =
          args[0] === '--ax'
            ? await s.tree(args[1], { ax: true })
            : await s.tree(args[0], args[1] === 'under' ? args[2] : undefined);
        break;
      case 'state': {
        refuseDriveFlag(host, op, args, ['--full']);
        const words = args.filter(a => a !== '--full');
        how = { full: words.length < args.length, name: words[0], look: LOOK_READS.includes(words[1]) };
        r = await stateOp(s, words);
        break;
      } // over STATE_CAP, its sections' sizes unless --full
      case 'reload':
        if (args.length) throw new MalformedOperation('reload takes no arguments');
        r = await reloadWeb(s);
        break;
      case 'logs':
        if (args.some(a => a !== '--all')) throw new MalformedOperation('logs [--all]');
        r = await s.logs({ all: args.includes('--all') });
        break;
      case 'layout':
        r = await s.layout(...layoutArgs(args));
        break;
      case 'screenshot':
        refuseDriveFlag(host, op, args); // `"screenshot x.png --scale 3"`: --scale is the drive's
        if (args.includes('compare') || args[2] === 'from')
          // `from`: a world's look frame (LLP 1127.000 D3)
          r = await (args[2] === 'from' ? worldLook : compareScreenshot)(s, args);
        else if (args[1] === 'over') {
          if (args[3] !== 'every' || !(args.length === 5 || (args.length === 6 && args[5] === 'window')))
            throw new MalformedOperation('screenshot <png|apng> over <ms> every <ms> [window]');
          r = await s.screenshot(args[0], {
            over: Number(args[2]),
            every: Number(args[4]),
            window: args[5] === 'window',
          });
        } else if (args[2] === 'pictureinpicture') {
          if (args[1] !== 'window' || args.length > 4)
            throw new MalformedOperation('screenshot <png> window pictureinpicture [<image>]');
          r = await s.screenshot(args[0], true, 'pictureinpicture', false, args[3]);
        } else
          r = await s.screenshot(
            args[0] ?? 'screenshot.png',
            args[2] === 'save' ? args[1] : args[1] === 'display' ? 'display' : args[1] === 'window',
            args[2],
            !!s.held,
          ); // inside a held touch, as it stands
        break;
      case 'tap':
        // The contact's phases (LLP 1035.003 D1) read as `tap move …`,
        // `tap hold`, `tap up`, `tap cancel` only while a contact is down;
        // with none down those words are targets like any other.
        if (holdOf(args[0]))
          r = await s.tap(args[0], { choice: args[1] }); // `tap @7 cancel` is the hold, even while a contact is down
        else if (args[1] === 'drag' && !(s.contact && ['move', 'hold', 'up', 'cancel'].includes(args[0]))) {
          const tail = named ? named[3] : trimmed.replace(/^\S+\s+\S+\s+/, '');
          const drag = parseDrag(s, tail.replace(/^drag(?:\s+|$)/, ''));
          r = await s.tap(args[0], { drag });
        }
        // Every other form, strictly (#107): the contact's phases (LLP 1035.003 D1) read as `tap move …`, `tap hold`,
        // `tap up`, `tap cancel` only while a contact is down; with none down those words are targets like any other.
        else {
          const [target, { phase, ...opts }] = tapWords(args, !!s.contact);
          r = target === null && !opts.midi ? await s.pointer(phase, opts) : await s.tap(target, opts);
        }
        break;
      case 'clock':
        r = await s.clock(trimmed.slice(5).trim() || 'settle');
        break;
      /* whole: a literal keeps its spaces */ case 'sample': {
        const n = args.map(Number);
        if (!n.length || n.length % 2 || n.some(v => !Number.isFinite(v)))
          throw new MalformedOperation("sample <x> <y> [<x> <y> …]: view points; layout <target> gives a node's frame");
        r = await s.op({ op: 'sample', points: n.flatMap((v, i) => (i % 2 ? [] : [[v, n[i + 1]]])) });
        break;
      }
      case 'resize':
        r = await s.resize(...resizeArguments(args));
        break;
      case 'close':
        if (args.length) throw new MalformedOperation("close takes nothing: it presses the window's close button");
        r = await s.closeWindow();
        break;
      case 'back':
        if (args.length)
          throw new MalformedOperation("back takes nothing: it is the platform's Back from what is on top");
        r = await s.back();
        break;
      // A module's value is the line's whole tail, so a JSON string keeps its spaces (`…heard="pick up milk"`).
      case 'prefer':
        r = await s.prefer(
          args[0] === 'extension' && args.length > 2
            ? { extension: trimmed.replace(/^prefer\s+extension\s+/, '') }
            : Object.fromEntries(args.flatMap((a, i) => (i % 2 ? [] : [[a, args[i + 1]]]))),
        );
        break;
      // A system's call of a tool (LLP 1133.023 §10): `call <tool> <json> [--door <door>]`, run as the door would.
      case 'call': {
        const door = trimmed.match(/\s--door\s+(\S+)\s*$/);
        r = await s.op({
          op: 'call',
          tool: args[0],
          args: trimmed.replace(/\s--door\s+\S+\s*$/, '').replace(/^call\s+\S+\s*/, '') || '{}',
          door: door?.[1] ?? 'agent',
        });
        break;
      }
      case 'fail':
      case 'pass':
        r = await s.op(
          faultRequest(op, args, fetchOrigins(resolve(flags.webDist ?? defaultWebDist(resolveApp(flags.app))))),
        );
        break;
      case 'perf':
        r = await perfOp(s, args, line, step);
        break;
      case 'check':
        r = await checkOp(s, args);
        break;
      /* LLP 1116.003 D8: the presented screen, judged in text */ case 'scroll':
        throw new MalformedOperation(
          `scroll is not an op: a finger scrolls, \`tap ${args[0] ?? '<scroller>'} drag ${-(Number(args[1]) || 0)} ${-(Number(args[2] ?? 300) || 0)}\` (the content follows the finger: a scroll of 300 down is a drag of -300), a wheel \`tap <scroller> wheel <dx> <dy>\`, a list's row by key \`tap <list> into <key>\``,
        );
      case 'open':
      case 'openurl':
      case 'url':
      case 'link':
        throw new MalformedOperation(
          `${op} is not an op: a link arrives as \`type <root> ${args[0] ?? 'euxy://p/abc'}\`, the root being the node with navigate= (its testId or id, from \`tree\`; LLP 1038 D11); on macOS and iOS a URL with a scheme goes through the app's protocol_handlers as the system delivers it, with no "Open in …?" alert`,
        );
      default:
        throw new MalformedOperation(
          `unknown op: ${op} (tree, layout, state, logs, check, screenshot, sample, tap, type, post, pick, reload, clock, resize, close, back, prefer, call, perf, fail fetch, pass fetch)`,
        );
    }
    return [op, r, how];
  };
  // The drive, against the build it used (LLP 1146 D2: `bun scripts/evidence.mjs`).
  const recordDrive = async result =>
    (await import('./evidence.mjs')).recordRun(flags.app, {
      host,
      what: `drive ${ops.join(' · ').slice(0, 300)}`,
      result,
      timing: flags.timing ?? 'agent',
      touch: flags.touch,
      device: flags.device || flags.phone,
      sim: flags.sim,
      webDist: flags.webDist,
    });
  // What the replies leave out, on stderr, which a --json reader's stdout never carries (agent-test.mjs `driveNotes`).
  const notes = driveNotes(s, {
    host,
    storage: flags.storage,
    origins: flags.url ? [] : loopbackOrigins(resolve(flags.webDist ?? defaultWebDist(resolveApp(flags.app)))),
  });
  try {
    if (flags.permission) await s.prefer(flags.permission); // `--permission name=state` (agent-launch.mjs)
    await launchEntries(s, flags); // `--notification '<entry>'`
    for (const [k, line] of ops.entries()) {
      at = k + 1;
      const done = await step(line);
      for (const [op, r, how] of [...(s.heldReads?.splice(0) ?? []), ...(done ? [done] : [])]) {
        console.log(flags.json ? JSON.stringify(r) : render(op, r, how));
        await notes(op, r);
      }
    }
    if (s.contact?.whole?.queue.some(q => q.line))
      throw new Error(
        'the steps after tap <target> down run inside the touch at its tap up, and the drive ended before it',
      );
    await notes('end');
    await recordDrive({ ok: true });
    return 0;
  } catch (e) {
    // A drive that fails (an `expect`, a refused op) is where a missing store or a failed resource is found: say it
    // first.
    await notes('end');
    await recordDrive({ ok: false, error: `op ${at}: ${String(e.message).split('\n')[0].slice(0, 200)}` });
    e.message = `op ${at}/${ops.length} \`${ops[at - 1]?.trim()}\`: ${e.message}`;
    throw e;
  } finally {
    await s.close();
    await served?.stop();
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  (await import('./tool-drive.mjs'))
    .toolDrive('agent', process.argv.slice(2))
    .then(main)
    .then(
      code => process.exit(code),
      e => {
        if (refusalMessage(e.reply)) console.error(refusalMessage(e.reply));
        else console.error(e.message);
        if (process.env.EXACT_AGENT_DEBUG || process.argv.includes('--json')) console.error(e.stack);
        if (e.steps && !refusalMessage(e.reply)) console.error(render('type', { steps: e.steps }));
        process.exit(e.stale ? 3 : 1);
      },
    );
}
