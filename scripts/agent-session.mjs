// A drive's session (scripts/agent.mjs `open`): one carrier opened on a host, and the operations a drive performs
// on it — reads, the clock, screenshots; its input is agent-input.mjs's.
import { webPort, launchFacts, bakedPlans, launchEnvironment } from './agent-launch.mjs';
import {
  sourceMapReaders,
  identifyResources,
  identifyInspectedNode,
  identifyLayoutNodes,
  withRegions,
  nextCommitTasks,
  settleBound,
  worldView,
  worldTells,
  WORLD_LIMIT,
  scaledScreenshot,
} from './agent-inspect.mjs';
import { preferGroups, preferOp } from './agent-prefer.mjs';
import { axTree } from './agent-ax.mjs';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { encodePng } from './png.mjs';
import { dirname, resolve } from 'node:path';
import {
  peekLogs,
  takeWorldLogs,
  STORAGE_REFUSED,
  STORAGE_NOTE,
  nodeNamed,
  targetsIn,
  unopenedHint,
} from './agent-test.mjs';
import { MalformedOperation, operationReply } from './agent-keys.mjs';
import { AWAY } from './agent-wire.mjs';
import { openStdio } from './agent-stdio.mjs';
import { appShareTarget, driverShare, webShareLocation } from './share-target.mjs';
import { heldClock, inFlightNote, untilClock } from './agent-held.mjs';
import { film } from './agent-film.mjs';
import { developmentLaunchEnvironment } from '../host/apple/devices.mjs';
import { bakeOutput, executableName, linuxBinary, resolveApp, webDist as defaultWebDist } from './app.mjs';
import { androidBinary } from './agent-android.mjs';
import { VIEWPORT, openWeb } from './agent-web.mjs';
import { openIOS } from './agent-ios.mjs';
import { sessionInput } from './agent-input.mjs';
const CLOCK_STEP_MS = 1000,
  CLOCK_BUDGET_MS = 3000,
  CLOCK_SPAN_MS = 600_000,
  REAL_STEP_MS = 50;
/** The next step of a split clock: aimed at CLOCK_BUDGET_MS of wall clock from the last step's cost, growing at most
 * 4x and never past CLOCK_SPAN_MS of world time. */
export const clockSpan = (span, elapsedMs) =>
  Math.max(CLOCK_STEP_MS, Math.min(span * 4, CLOCK_SPAN_MS, (span * CLOCK_BUDGET_MS) / Math.max(1, elapsedMs)));
/** Open a session on `host` ('web' | 'macos' | 'ios' | 'linux'); `url` opens
 * the same app address on each host; `plan` boots a local compiled contract;
 * `env` adds to a native host's environment; `reader` drives Android's app
 * on the Canvas reader (`--reader`; unsaid, EXACT_ANDROID_READER decides).
 * @ref LLP 1030.000 §7 */
export async function open({
  onProcess,
  host = 'web',
  browser,
  plan,
  world,
  size,
  env,
  app,
  session,
  documents,
  url,
  webDist,
  reuse,
  device = false,
  phone: pick,
  reader,
  timing = 'agent',
  touch = 'agent',
  chrome = 'agent',
  bind,
  storage,
  seed,
  locale,
  formatLocale,
  timeZone,
  epoch,
  failFetch,
  mediaClock = 'wall',
  lineHeight = null,
  scale,
  port,
  ax = false,
} = {}) {
  browser ??= 'chrome';
  if (!['chrome', 'firefox', 'webkit'].includes(browser))
    throw new Error(`browser: chrome, firefox or webkit, not ${browser}`);
  const facts = launchFacts({ seed, locale, formatLocale, timeZone, epoch, failFetch, env });
  env = { ...env, ...launchEnvironment(facts) };
  if (world && !['web', 'mac', 'macos', 'ios', 'linux', 'windows'].includes(host))
    throw new Error(`world restore unavailable on this host yet: ${host}`);
  if (world && statSync(world).size > WORLD_LIMIT)
    throw new Error(
      'world carrier exceeds 256 MiB limit; inspect `state world:*` and reduce saved entities before `screenshot checkpoint.world world save`',
    );
  if (world && host !== 'web' && !device) env = { ...env, EXACT_WORLD: resolve(world) };
  if (device && host !== 'ios') throw new Error('--device is supported for the standalone ios client');
  if (host !== 'web' && browser !== 'chrome')
    throw new Error(`--browser is only supported by the web carrier, not ${host}`);
  if ((mediaClock !== 'wall' || lineHeight != null) && host !== 'web')
    throw new Error("mediaClock and lineHeight are the web's (conformance)");
  // `timing: 'platform'` (LLP 1035.003 D5, opt-in): the carrier stays and the driver still owns the runner's clock,
  // but UIKit's own transitions, sheet presentations and keyboard animations run at their natural timing — the
  // ordinary app with a socket, for observing an interactive gesture's native motion. The frozen clock is the
  // default the smoke depends on. Replies say `mode: "platform"`.
  if (!['agent', 'platform'].includes(timing)) throw new Error(`timing: agent or platform, not ${timing}`);
  if (timing === 'platform') env = { ...(env ?? {}), EXACT_AGENT_TIMING: 'platform' };
  // `touch: 'platform'` (LLP 1080.000, opt-in): a plain `tap` on an iOS simulator is a real touch from the XCTest
  // runner, `delivery: platform`.
  // `touch: 'drag'`: only `tap … drag` is (§11), every other input as under `agent` — what an authored test's drag
  // takes (chat2 diary).
  if (
    !['agent', 'drag', 'platform'].includes(touch) ||
    (touch !== 'agent' && (device || !['ios', 'host-ios'].includes(host)))
  )
    throw new Error(`touch: ${touch} is an iOS simulator's (LLP 1080.000), not ${device ? 'a phone' : host}'s`);
  // UIKit's menus, which a real touch opens (D7)
  if (touch === 'platform') env = { ...(env ?? {}), EXACT_AGENT_PRESENTATION: 'native' };
  // `chrome: 'platform'` (opt-in; splitter rough 4, 11): UIKit's navigation bar and tab bar show as a person sees them,
  // and `screenshot <png> window` draws them, a presented sheet's too; a tap naming a tab or a header button the bars
  // stand for presses it (`native: "tab-bar-item"`, `"bar-button-item"`). The default paints the authored header and
  // tablist, which the tree, layout and every tap address as on the web; a header node the bar would not show is
  // marked `unshown` in the tree, struck through in a screenshot, and fails an `expect` or a `tap` (LLP 1116 D2).
  // macOS: AppKit's sidebar for a root tab bar, `native: "sidebar-row"` (LLP 1115.002 D3); a `dialog` as its sheet and
  // a popover that is no menu as AppKit's popover, windows of their own that `screenshot <png> window` draws over the
  // window (D7.2, D7.3).
  if (!['agent', 'platform'].includes(chrome) || (chrome !== 'agent' && !['ios', 'host-ios', 'macos'].includes(host)))
    throw new Error(`chrome: ${chrome} is iOS's and macOS's (agent or platform), not ${host}'s`);
  if (chrome === 'platform') env = { ...(env ?? {}), EXACT_AGENT_CHROME: 'platform' };
  if (bind) {
    if (host === 'web')
      throw new Error('--bind: the web drives its implementations with no agent (packages/<name>/test/web.mjs)');
    // A sideloaded Android build signed in with an account that is not a license tester pays real money
    // (LLP 1133.015 §9).
    if (host === 'android' && String(bind).split(',').includes('purchases'))
      throw new Error(
        'bind-real-money: --bind purchases is refused on Android, where exact2 cannot tell a license tester from a paying account; the conformance suite runs Play Billing on a phone signed in as a license tester (LLP 1133.015 §9)',
      );
    env = { ...(env ?? {}), EXACT_AGENT_BIND: bind };
  }
  // `scale` (--scale <n>): a screenshot's pixels a point, shrunk by area from the device's (an iPhone draws 3), never
  // enlarged.
  if (scale !== undefined && !(Number.isFinite(scale) && scale > 0))
    throw new Error(
      `--scale ${typeof scale === 'string' ? JSON.stringify(scale) : scale}: pixels a point, a number above 0 (1 is the web's CSS pixel; an iPhone draws 3)`,
    );
  // A drive has no app storage unless it names a scratch store apart from the app's real files (`--storage <name>`),
  // kept between drives: a tree under the cache base on native; on the web, Chrome's profile for it and its page's
  // origin (agent-launch.mjs `webStore`; Firefox and WebKit open a fresh one each drive). EXACT_AGENT_STORAGE_FRESH
  // empties it.
  if (storage !== undefined && (!/^[A-Za-z0-9._-]+$/.test(storage) || ['.', '..'].includes(storage)))
    throw new Error("--storage: one name of letters, digits, '.', '-' or '_'");
  if (storage !== undefined && host !== 'web') env = { ...(env ?? {}), EXACT_AGENT_STORAGE: storage };
  // The CLI's `390x844` is accepted from a script too; anything else names the two forms.
  if (typeof size === 'string') {
    const m = size.match(/^(\d+)x(\d+)$/);
    if (!m) throw new Error(`size: [width, height] or "<width>x<height>", not ${JSON.stringify(size)}`);
    size = [Number(m[1]), Number(m[2])];
  }
  // @ref LLP 1133.024 D9 — a cold share: the web's location, or the native host's claim before its boot.
  if (url?.startsWith('share:')) {
    const target = appShareTarget(app),
      share = driverShare(url, process.cwd());
    if (host === 'web') url = webShareLocation(target, share);
    else {
      env = { ...(env ?? {}), EXACT_AGENT_SHARE: JSON.stringify({ target, share }) };
      url = undefined;
    }
  }
  if (url !== undefined && ['macos', 'mac', 'ios', 'linux', 'android', 'windows', 'host', 'host-ios'].includes(host)) {
    // @ref LLP 1038 D5/D11 — a native scheme/path is a launch location;
    // HTTP(S) keeps the existing development-plan locator form.
    if (/^https?:\/\//i.test(url)) {
      if (plan) throw new Error('a native session takes either a development --url or --plan, not both');
      env = developmentLaunchEnvironment(['--run', '--url', url], env ?? {});
    } else env = { ...(env ?? {}), EXACT_LAUNCH_URL: url };
  }
  const carrier = device
    ? await openStdio({ host: 'ios', plan, world, size, env, app, device, phone: pick, onProcess })
    : // `documents` are the Mac's command line, a terminal's route in (LLP
      // 1033 D3); each window's session then routes by its label (LLP 1069.010).
      host === 'macos' || host === 'mac'
      ? await openStdio({ host: 'macos', plan, size: size ?? VIEWPORT, env, app, session, documents, onProcess })
      : host === 'host'
        ? await openStdio({ host: 'host', plan, env, app, session, onProcess })
        : host === 'host-ios'
          ? await openIOS({ plan, app, env, session, hostFixture: true, touch, ax, onProcess })
          : host === 'linux' || host === 'android'
            ? await openStdio({ host, plan, size: size ?? VIEWPORT, env, app, reader, onProcess })
            : host === 'windows'
              ? await openStdio({ host: 'windows', plan, size: size ?? VIEWPORT, env, app, onProcess })
              : host === 'ios'
                ? await openIOS({ plan, env, app, size, touch, ax, onProcess })
                : await openWeb({
                    browser,
                    plan,
                    world,
                    size,
                    url,
                    app,
                    webDist,
                    onProcess,
                    reuse,
                    storage,
                    fresh: env?.EXACT_AGENT_STORAGE_FRESH === '1',
                    facts,
                    env,
                    mediaClock,
                    lineHeight,
                    port: webPort(port, { ...process.env, ...env }),
                  });
  const mapLocator =
    plan ??
    carrier.plan ??
    (url && /^https?:\/\//i.test(url) ? url : (env?.EXACT_DEV_PLAN ?? process.env.EXACT_DEV_PLAN)) ??
    // The dist the carrier serves — an app outside the repo's own (shop F2), never Caltrain's by default.
    (carrier.host === 'web' ? resolve(webDist ?? defaultWebDist(resolveApp(app)), 'app.plan') : null);
  // Without a plan of the drive's own, a native host runs its bake's: the maps a development bake left
  // (LLP 1012.001.000 D6).
  const baked = () => {
    const a = resolveApp(app);
    const bin =
      host === 'windows'
        ? (process.env.EXACT_WINDOWS_BIN ?? resolve(a.dir, 'dist-windows', `${executableName(a)}.exe`))
        : host === 'android'
          ? (process.env.EXACT_ANDROID_BIN ?? androidBinary(a, undefined, reader))
          : (process.env.EXACT_LINUX_BIN ?? linuxBinary(a));
    return bakedPlans(bin, bakeOutput(a));
  };
  const sourceMaps = sourceMapReaders(mapLocator ? [mapLocator] : carrier.host !== 'web' ? baked() : []);
  const s = {
    carrier,
    launchFacts: facts,
    host: carrier.host,
    /** The sample host's sessions by label, and which one the next request goes to (`s.session = "b"`). */
    sessions: carrier.sessions ?? null,
    get session() {
      return carrier.state?.session ?? null;
    },
    set session(label) {
      if (carrier.state) carrier.state.session = label;
    },
    /** Milliseconds from launch to the first frame. */
    boot: carrier.boot,
    /** The agent's clock, milliseconds: the last `clock` value (0 at boot). */
    now: 0,
    logCursor: 0,
    gpuMs: carrier.gpuMs,
    /** The host's wire: one request as the carrier sends it, views addressed by `id`. The methods (`tap`, `type`, …)
     * are the operations a person performs; a request the wire would answer by doing nothing is refused (habits F8). */
    async op(req) {
      if (!req || typeof req !== 'object' || typeof req.op !== 'string')
        throw new MalformedOperation('op: a request is {op: "<name>", …}');
      const input = req.op === 'tap' || req.op === 'type';
      if (input && req.target !== undefined)
        throw new MalformedOperation(
          `op: the wire's ${req.op} addresses a view by \`id\`, not \`target\`; s.${req.op}(${JSON.stringify(req.target)}, …) finds a target and delivers the input as a person would`,
        );
      const r = await operationReply(carrier, req);
      if (req.op === 'tree') s.pendingRequests = r.pending ?? [];
      // A web page's own tap or type acts only for an iframe guest, history, a list's row or a control's value:
      // the carrier's browser input is the rest, which only the methods reach.
      if (
        input &&
        carrier.host === 'web' &&
        req.resize === undefined &&
        ![
          'tapped',
          'typed',
          'handled',
          'history',
          'into',
          'resized',
          'guest',
          'value',
          'delivery',
          'phase',
          'aimed',
        ].some(k => r[k] != null && r[k] !== false)
      )
        throw Object.assign(
          new Error(
            `op: the web page's ${req.op} delivered nothing to view ${req.id}; s.${req.op}(…) delivers browser input`,
          ),
          { reply: r },
        );
      return r;
    },
    /** Every live node in preorder, or one target and its descendants; {shallow:true} reads only the target's
     * record, retaining its real child ids. An iframe also carries url, loading, and a reachable guest outline (@ref
     * LLP 1020 D4). A canvas whose row carries a world summary answers with its world instead: `tree <canvas> [under
     * <entity>]` is the world's outline (@ref llp/1046.001-agent-interface-to-a-game.rfc.md D2). */
    async tree(target, options = {}) {
      const under = typeof options === 'string' ? { under: options } : {};
      if (typeof target === 'string' && target.startsWith('world:'))
        return s.op({ op: 'tree', ...(await s.target(target)), world: true, ...under });
      const { shallow = false, ax = false } = typeof options === 'object' && options ? options : {};
      if (ax) return axTree(s, target, options); // LLP 1080.002: the platform's accessibility tree
      const req = { op: 'tree' };
      if (target != null)
        req.target = typeof target === 'number' || /^\d+$/.test(String(target)) ? Number(target) : target;
      if (shallow !== false) req.shallow = shallow;
      const reply = await s.op(req).catch(async e => {
        if (typeof req.target !== 'string' || !/no view matches/.test(e.message)) throw e;
        req.target = (await s.find(target)).id;
        return s.op(req);
      });
      const canvas =
        target != null && shallow === false
          ? reply.nodes?.find(n => n.id === req.target || n.props?.testId === req.target)
          : null;
      return canvas?.world ? s.op({ op: 'tree', id: canvas.id, world: true, ...under }) : reply;
    },
    world(name) {
      return worldView(this, name);
    },
    async state(target, under, pose = false, busy = false, page = {}) {
      // `page.reveal`: a password field's text unmasked, for `expect state` alone
      const reply = await s.op({
        op: 'state',
        ...page,
        ...(busy ? { busy: true } : {}),
        ...(pose ? { pose: true } : {}),
        ...(target != null ? await s.target(target, false, true) : {}),
        ...(under != null ? { under: String(under).replace(/^[^:]+:/, '') } : {}),
      });
      await sourceMaps.refresh();
      return identifyResources(reply, sourceMaps);
    },
    async declaration(name, state) {
      await sourceMaps.refresh();
      const digest =
        state?.planDigest ??
        (mapLocator && !/^https?:/.test(mapLocator) && existsSync(mapLocator)
          ? createHash('sha256').update(readFileSync(mapLocator)).digest('hex')
          : null);
      const map = sourceMaps.get(digest);
      return map?.slots?.[name] ?? map?.derives?.[name] ?? null;
    },
    /** What happened since the last read: the runner's journal (`lines`, from index `from` up to `next`) and the
     * host's own output (`host`). `dropped` counts lines the journal ring let go before this read caught up. */
    async logs({ all = false } = {}) {
      const since = all ? 0 : s.logCursor;
      let r;
      try {
        r = await s.op({ op: 'logs', since, ...(all ? { all: true } : {}) });
      } catch (e) {
        if ((carrier.host !== 'web' || !carrier.hostLines.length) && !e.message.startsWith(AWAY)) throw e;
        r = { lines: [], from: since, next: since, refused: e.reply?.refused, error: e.message };
      }
      r.world = takeWorldLogs(s, r.world, all);
      const dropped = Math.max(0, r.from - since);
      s.logCursor = r.next;
      // What the driver did that the journal could not keep (a test's `reload` waiting for storage, LLP 1097 D9).
      const notes = s.notes?.splice(0) ?? [];
      const host = carrier.hostLines.slice(all ? 0 : (s.hostLogCursor ?? 0));
      s.hostLogCursor = carrier.hostLines.length;
      const crash = carrier.crashes?.() ?? [];
      return {
        ...(r.error ? { error: r.error, refused: r.refused } : {}),
        lines: [...notes, ...r.lines],
        host,
        ...(crash.length ? { crash } : {}),
        from: r.from,
        next: r.next,
        dropped,
        ...(r.world ? { world: r.world } : {}),
      };
    },
    /** `perf [<target>]` (LLP 1079 D2): the plan sites under a view (every root's without one), each with its work
     * across its instances' lifetimes and its source line; `{frames:true}` is the host's presented frames (D4). */
    async perf(target, frames) {
      if (typeof target === 'string' && target.startsWith('world:')) {
        const surface = target.slice(6) || 'world';
        const r = await s.op({
          op: 'state',
          ...(await s.target(surface)),
          world: true,
          perf: true,
          ...(frames?.spanSince != null ? { span_since: frames.spanSince } : {}),
        });
        if (r.world.perf.tickSpans.error) throw Error(r.world.perf.tickSpans.error);
        return { target, tick: r.world.tick, restarted: r.world.restarted, ...r.world.perf.tickSpans };
      }
      if (frames?.live != null) {
        // A live window (LLP 1079 D4): the page's clock follows the wall for `live` ms, and the driver's follows the
        // page's.
        const r = await s.op({
          op: 'perf',
          frames: true,
          live: frames.live,
          ...(frames.late != null ? { late: frames.late } : {}),
        });
        if (r.virtual && !r.live) throw Error(`perf frames live: the ${s.host} host has no live observation`);
        if (Number.isFinite(r.live?.to)) s.now = r.live.to;
        return r;
      }
      if (frames) return s.op({ op: 'perf', frames: true, ...(frames.late != null ? { late: frames.late } : {}) });
      const r = await s.op({
        op: 'perf',
        ...(target != null
          ? { target: /^\d+$/.test(String(target)) ? Number(target) : (await s.find(target)).id }
          : {}),
      });
      if (await sourceMaps.refresh())
        for (const site of [...r.sites, ...(r.handlers ?? [])]) {
          const n = { planDigest: r.plan, site: site.site };
          sourceMaps.attach(n);
          site.source = n.sourceMap;
        }
      return { target: target ?? null, ...r };
    },
    /** The component names the development source map of plan `digest` declares, or null without one (LLP 1146 D3). */
    components(digest) {
      const map = sourceMaps.get(digest);
      return map ? new Set(map.nodes.map(n => n?.component).filter(Boolean)) : null;
    },
    /** Every on-screen view's box in the viewport (scroll folded in), with its testId and type from the tree. With a
     * target, `node` explains that one node (LLP 1035.002 D1): every row it sets or inherits with where the value
     * came from, its box in each coordinate space the host has, the scroll and clip chains above it, whether it is
     * hidden, inert, in the viewport or clipped away, and what the host mounted for it — observations of the
     * runner's memory and the host's view tree, never a second model. */
    async layout(target, at, inspect = {}) {
      // inspect: {native: {depth, limit}} | {agree: true, limit} (LLP 1080.001)
      if (inspect.agree)
        return s.op({ op: 'layout', agree: true, ...(inspect.limit != null ? { limit: inspect.limit } : {}) });
      if (typeof target === 'string' && target.startsWith('world:'))
        return s.op({ op: 'layout', ...(await s.target(target)), ...(at ? { world: true, x: at[0], y: at[1] } : {}) });
      // `layout <canvas> at <x> <y>` is the world's pick (@ref llp/1046.001-agent-interface-to-a-game.rfc.md D2).
      if (target != null && at)
        return s.op({ op: 'layout', id: (await s.find(target)).id, world: true, x: at[0], y: at[1] });
      const req = {
        op: 'layout',
        ...(inspect.native ? { native: inspect.native } : {}),
        ...(inspect.picture && carrier.host === 'web' ? { picture: true } : {}),
      };
      if (target != null) {
        const found = await s.find(target);
        req.id = found.id;
        if (await sourceMaps.refresh()) req.plan = true;
        const reply = await s.op(req); // a view found by label or text is checked by its testId, else its type
        identifyInspectedNode(reply, found.props.testId ?? found.id);
        if (reply.node && reply.node.type !== found.type)
          throw Error(`layout: target ${target} changed during inspection; retry`);
        // The JS target keeps no runner to name its plan: it is the plan this carrier serves.
        if (
          reply.node &&
          carrier.host === 'web' &&
          reply.node.site != null &&
          !reply.node.planDigest &&
          mapLocator &&
          existsSync(mapLocator)
        )
          reply.node.planDigest = createHash('sha256').update(readFileSync(mapLocator)).digest('hex');
        if (reply.node) sourceMaps.attach(reply.node);
        withRegions(reply.env);
        delete reply.nodes;
        // the answer is the target (its box: node.space.viewport); every view's box beside it let an assertion over
        // `nodes` pass whatever the target was (Depot)
        return reply;
      }
      const mapped = await sourceMaps.refresh();
      const [l, t] = await Promise.all([s.op(req), s.op({ op: 'tree', ...(mapped ? { plan: true } : {}) })]);
      identifyLayoutNodes(l, t, mapped ? sourceMaps : null);
      withRegions(l.env);
      return l;
    },
    async target(target, withHandlers = false, canvas = false) {
      // canvas: a world's view is its engine's to answer (`world: true`), as `tree` asks it
      const text = String(target),
        colon = text.indexOf(':'),
        nodes = (await s.op({ op: 'tree' })).nodes; // one tree for every try
      const node = await s.find(target, false, false, nodes);
      if (node)
        return {
          id: node.id,
          ...(withHandlers ? { handlers: node.handlers } : {}),
          ...(canvas && node.world ? { world: true } : {}),
        };
      // an entity before a label holding a colon
      const owner = colon < 0 ? null : await s.find(text.slice(0, colon), false, false, nodes);
      return owner
        ? { id: owner.id, entity: text.slice(colon + 1) }
        : (({ id, world }) => ({ id, ...(canvas && world ? { world: true } : {}) }))(
            await s.find(target, true, true, nodes),
          );
    },
    /** The node for a target: a testId (first in preorder on a selected route; a covered screen's copy only when no
     * active one carries it), a view id, else the one view whose label or text it is (`nodeNamed`). */
    async find(target, required = true, named = true, tree = null) {
      if (target == null) throw new MalformedOperation(`no view matches ${target}`);
      const byId = typeof target === 'number' || /^\d+$/.test(String(target));
      const t = tree
        ? { nodes: tree }
        : required
          ? await s.op({ op: 'tree', target: byId ? Number(target) : target, shallow: true }).catch(e => {
              if (byId || !/no view matches/.test(e.message)) throw e;
              return null;
            })
          : null;
      const nodes = t?.nodes ?? (await s.op({ op: 'tree' })).nodes;
      const matches = byId ? nodes.filter(n => n.id === Number(target)) : nodes.filter(n => n.props.testId === target);
      const node =
        matches.find(n => !n.inactive) ??
        matches[0] ??
        (byId || !named ? undefined : (nodeNamed(nodes, String(target)) ?? undefined));
      if (!node && required)
        throw new Error(
          `no view matches ${target} by testId, label or text${targetsIn(nodes)}${unopenedHint(nodes)}; tree lists live targets, and a virtualized list's row outside its window comes in by key: \`tap <list> into <key>\` (feed F10)${await s.inFlight()}${s.keptStore ? `; this drive kept the store \`${s.keptStore}\` from earlier drives, so the screen can show what they saved (a \`persist\` state): --reset-storage starts it empty` : ''}`,
        );
      return node;
    },
    async inFlight() {
      // Native misses use the tree's snapshot; another transport read could time out.
      const st = s.host === 'web' ? await s.op({ op: 'state' }).catch(() => null) : { pending: s.pendingRequests };
      return inFlightNote(st?.pending ?? []); // held asks named apart, with `clock held` (agent-held.mjs)
    },
    /** Move the clock: to an absolute millisecond, by '+N', or to 'settle' (or wait, the clock frozen, for 'held'
     * requests' replies, or 'until state <path> == <literal> within <ms>') — a fixed point at which nothing is in
     * flight (`settled: false` if timers keep starting motion), a task due a commit later (`after(1, …)`) included,
     * for 16 rounds. Timers fire on the way, each at its own time; motion is seeked, never played. The clock lands
     * where the runner says; a timer's refusal is the error. */
    async clock(spec = 'settle', round = 0, down = null) {
      // `held`, `until`: agent-held.mjs
      if (/^held(\s|$)/.test(String(spec))) return heldClock(s, String(spec).split(/\s+/).slice(1));
      if (/^until(\s|$)/.test(String(spec))) return untilClock(s, String(spec));
      const real = /^\+(\d+(?:\.\d+)?)\s+real(?:\s+until\s+(\S.*))?$/.exec(String(spec)),
        time = real ? real[1] : typeof spec === 'string' && spec.startsWith('+') ? spec.slice(1) : spec;
      // `+N real until <path>`: a test harness's signal (a file it controls): `+N real`, ended early once <path>
      // exists.
      const until = real?.[2] ? resolve(real[2]) : null;
      // (A scheme is two letters or more: `C:\…` is a Windows path.)
      if (/^state\s/.test(real?.[2] ?? ''))
        throw new MalformedOperation(
          `clock +N real until state: real time moves the clock here; \`clock until state <path> == <literal> within <ms>\` waits with it frozen`,
        );
      if (until && (/^[a-z][a-z0-9+.-]+:/i.test(real[2]) || /[*?[\]{}]/.test(real[2])))
        throw new MalformedOperation(
          `clock +N real until <path>: a file path on this machine, not a URL or a glob: ${real[2]}`,
        );
      if (until && !statSync(dirname(until), { throwIfNoEntry: false })?.isDirectory())
        throw new MalformedOperation(
          `clock +N real until ${real[2]}: its directory ${dirname(until)} does not exist (the harness that signals makes it first)`,
        );
      if (until && statSync(until, { throwIfNoEntry: false })?.isDirectory())
        throw new MalformedOperation(`clock +N real until ${real[2]}: a directory, not the file a harness writes`);
      const signalled = () => !!statSync(until, { throwIfNoEntry: false })?.isFile();
      if (!['settle', 'data'].includes(spec) && !Number.isFinite(Number(time)))
        throw new MalformedOperation(
          `clock: not a time: ${spec}; use clock +100, clock +100 real, clock settle, clock data or clock until state <path> == <literal> within <ms>; state shows the current clock`,
        );
      // @ref LLP 1080.000 §12 — an Apple host takes its clock over at the wall under platform timing (and reports a
      // clock it already has otherwise); a seek counts from where it stands. Settle needs no count.
      if (spec !== 'settle' && spec !== 'data' && ['ios', 'host-ios', 'macos', 'host'].includes(carrier.host))
        s.now = (await s.op({ op: 'clock', take: true })).clock;
      // `data`: the app's data lands (activation, every request in flight, each `then`), the clock unmoved and no timer
      // fired.
      if (spec === 'data') {
        const r = await s.op({ op: 'clock', data: true });
        if (r.settled === false)
          r.diagnostic =
            r.reason === 'data'
              ? "clock data gave up on the data module's activation at its bound (20 s); logs say why"
              : 'clock data gave up on requests still in flight at its bound (20 s); state shows them under pending';
        const soon = r.settled === false ? [] : await nextCommitTasks(s, r.clock);
        if (soon.length)
          r.diagnostic = `${soon.map(([name]) => `\`${name}\``).join(', ')} ${soon.length > 1 ? 'are' : 'is'} due a commit later (${soon[0][1]} ms: an \`after(1, …)\`), which a frozen clock reaches only when it moves: \`clock settle\` or \`clock +1\` runs ${soon.length > 1 ? 'them' : 'it'}`;
        return r;
      }
      const req = { op: 'clock' };
      if (spec === 'settle') req.settle = true;
      else if (real || (typeof spec === 'string' && spec.startsWith('+'))) req.to = s.now + Number(time);
      else req.to = Number(spec);
      if (!req.settle && !Number.isFinite(req.to))
        throw new MalformedOperation(
          `clock: not a time: ${spec}; use clock +100, clock +100 real, clock settle, clock data or clock until state <path> == <literal> within <ms>; state shows the current clock`,
        );
      // `+N real`: N ms of real time pass, the clock moving with them a step
      // at a time, so what runs on real time — a playing video (LLP 1042 §3),
      // a store, the network — goes on beside the app's timers (jukebox F14).
      if (real) {
        const from = s.now,
          end = req.to,
          t0 = performance.now();
        // (A file there already: met at once, the clock unmoved.)
        if (until && signalled()) return { clock: s.now, real: 0, until: 'met' };
        for (;;) {
          if (down && !(await down())) return { clock: s.now, cancelled: true };
          // Met: the clock moves by the real time gone, as a `+M real` with M that time would.
          const met = until !== null && signalled();
          const to = Math.max(s.now, Math.min(end, from + (performance.now() - t0))),
            // real time passes here: the host waits on no native motion
            r = await s.op({ op: 'clock', to, step: true });
          s.now = r.clock;
          if (met || to >= end)
            return {
              ...r,
              real: Math.round(performance.now() - t0),
              ...(until ? { until: met ? 'met' : 'bound' } : {}),
            };
          await new Promise(ok => setTimeout(ok, Math.min(REAL_STEP_MS, end - to)));
        }
      }
      // A long seek steps by wall clock (each operation ~CLOCK_BUDGET_MS of a web clock op's 60 s, from CLOCK_STEP_MS
      // of world time, at most 4x a step and CLOCK_SPAN_MS), so a cheap hour is a few presents, not 3,600; the final
      // reply is the seek's.
      let span = CLOCK_STEP_MS;
      // the host lets native motion run once, after the last step, for up to all of it
      const jump = req.settle ? 0 : req.to - s.now;
      for (let t0; !req.settle && req.to - s.now > span; span = clockSpan(span, performance.now() - t0)) {
        if (down && !(await down())) return { clock: s.now, cancelled: true };
        t0 = performance.now();
        s.now = (await s.op({ op: 'clock', to: s.now + span, step: true })).clock;
      }
      if (down && !(await down())) return { clock: s.now, cancelled: true };
      // an intermediate step's pending state is stale by now; a failed step threw
      const r = await s.op(req.to - s.now < jump ? { ...req, jump } : req);
      s.now = r.clock;
      // A store's or the network's reply comes on real time, which a jump does not wait for unless a timer fires first
      // (calendar F10).
      if (!req.settle && r.inflight)
        r.diagnostic = `${r.inflight} request${r.inflight === 1 ? '' : 's'} still in flight on real time (a store's, a worker's, the network's): \`clock +N\` moved the clock without waiting for ${r.inflight === 1 ? 'it' : 'them'}; \`clock data\` or \`clock settle\` lands them (then \`clock +N\`), and \`state\` lists them under pending. A native view's UIView.animate runs on for up to N ms (at most 1.5 s) of a \`clock +N\`, and \`clock settle\` waits up to 1.5 s for it; one a display link moves runs only with \`clock +N real\``;
      if (!req.settle && r.reason === 'motion')
        r.diagnostic = [
          r.diagnostic,
          "the app's own native animation (UIView.animate, Core Animation) still runs after this jump's wait (its length, at most 1.5 s of real time): `clock settle` waits again, `clock +N real` lets it run on",
        ]
          .filter(Boolean)
          .join('; ');
      if (req.settle && r.settled === false)
        r.diagnostic =
          r.reason === 'motion'
            ? "clock settle waited 1.5 s for the app's own native animation (UIView.animate, Core Animation) and it still runs: `clock +N real` lets it run on, `screenshot` shows where it is"
            : r.reason === 'scroll-event'
              ? "clock settle ran out of rounds (up to 8 frames each) with a list's reader's scroll event (a `scroll=` handler) still owed, or still running in the last round: `clock settle` again waits again"
              : r.reason === 'scroll'
                ? "clock settle waited 4 s for a scroll view a finger left moving (a fling's glide, a bounce; on the web a scroller whose scroll events go on) and it still moves: `clock settle` again waits on, `layout` shows where it is"
                : r.reason === 'device'
                  ? `clock settle stops at held device requests (${(r.tickets ?? []).map(t => '@' + t).join(' ')}); state shows them under pending; answer with tap @N <choice> or type @N <value>`
                  : r.reason === 'requests'
                    ? 'clock settle gave up on requests still in flight at its bound (20 s native); state shows them under pending, and logs a `request N` with no `fulfil N`'
                    : `clock settle did not reach quiescence: ${JSON.stringify(r.world ?? r)}; state world:* busy shows moving values and busy reasons; state shows held input; logs shows reload/refusals`;
      // an `after(1, …)` the settle's last commit armed
      const soon = req.settle && r.settled !== false ? await nextCommitTasks(s, r.clock) : [];
      if (soon.length && round < 16) {
        await this.clock(String(soon[soon.length - 1][1]));
        return this.clock('settle', round + 1);
      }
      if (soon.length) Object.assign(r, settleBound(soon));
      return req.settle ? worldTells(s, r) : r; // a drawing world's tells at rest (LLP 1127.000 D1)
    },
    /** The window to `w`x`h` points (reader: repagination): the browser's viewport, a macOS window, the Linux
     * presenter. The wire's
     * `tap` with `resize` (LLP 1041 §8: an input, not an operation); an iOS app's viewport is the device's, and it
     * says so. */
    async resize(w, h) {
      if (![w, h].every(Number.isFinite)) throw new MalformedOperation('resize: expected two finite dimensions');
      if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      return s.landed({ ...(await s.op({ op: 'tap', resize: [w, h] })), carrier: host });
    },
    /** The window's close button, as ⌘W or the red button press it (studio diary R17): the window asks its
     * `beforeunload`
     * first, and one a handler keeps stays (`closed: false`). macOS: `NSWindow.performClose`; the web: the page closed
     * with
     * its `beforeunload` run, the browser's "Leave site?" answered "Stay". iOS and Linux close no window, and say so.
     * The
     * wire's `tap` with `close`, an input as `resize` is. A closed window ends the drive's session (the app's last,
     * the app). */
    async closeWindow() {
      if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      const r = { ...(await s.op({ op: 'tap', close: true })), carrier: host };
      return r.closed ? r : s.landed(r);
    },
    /** Invoke the platform's Back handler, or tap UIKit's Back through XCTest with `native: true`. */
    async back({ native = false } = {}) {
      if (s.held) {
        throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      }
      if (native) {
        if (!carrier.touches || touch !== 'platform') {
          throw new Error('native Back needs iOS with --touch platform');
        }
        const container = (await carrier.ask({ op: 'state' })).navigation?.nativeBack;
        if (typeof container !== 'string' || !container.length) {
          throw new Error(
            'native Back needs a mounted session with a pushed route and a native bar; settle that route first',
          );
        }
        const r = await carrier.touches.ask({ op: 'tap', back: true, container }, 150000);
        if (r.error) {
          throw new Error(r.error);
        }
        return s.landed({ ...r, delivery: 'platform', carrier: host });
      }
      const r = await carrier.ask({ op: 'tap', back: true });
      if (r.error) {
        throw Object.assign(new Error(r.error), { reply: r });
      }
      return s.landed({ ...r, carrier: host });
    },
    /** The device facts by their web names (LLP 1061 D5; LLP 1069.000 D6), grouped on the wire as LLP 1069.007 D2
     * groups them. `media`: `{"prefers-reduced-motion": "reduce"}`, `"prefers-reduced-transparency"` likewise,
     * `"prefers-contrast": "more"|"less"|"custom"|"no-preference"`, `"prefers-color-scheme": "dark"|"light"` (the
     * system's; an app's `setScheme` still wins). `page`: `"visibility-state": "visible"|"hidden"`, `online`,
     * `can-share`, `can-share-files`, `can-share-pictures`, `can-open-files` and `has-focus` `"true"|"false"`,
     * `"root-font-size"` in px (what `rem` follows), `"bold-text"` `"true"|"false"` (Apple's Bold Text, refused by
     * name elsewhere). `fold` (LLP 1078 D7): `posture folded|continuous`, `segments <cols>x<rows>`, `gap <points>` —
     * a host without a fold splits its viewport evenly with the gap centred on each divider; a host with a real fold
     * refuses ("the device decides"). Unnamed facts stay. The reply is what the host now reports, by group. */
    async prefer(facts) {
      if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      const { media, page, fold, mode, midi, extension } = preferGroups(facts);
      // The midi and extension groups are the runner's (LLP 1125 §4, LLP 1124 D7), each asked on its own; alone, its
      // reply is the answer.
      const replies = {};
      for (const [k, g] of [
        ['midi', { midi }],
        ['extension', extension],
      ])
        if (Object.values(g).some(v => Object.keys(v).length)) replies[k] = await s.op({ op: 'prefer', ...g });
      if (Object.keys(replies).length && ![media, page, fold, mode].some(g => Object.keys(g).length))
        return replies.extension ?? replies.midi;
      if (carrier.prefer) return s.tagged(await carrier.prefer(media, page, fold, mode));
      return s.op(preferOp(media, page, fold, mode));
    },
    /** Pixels as PNG (second argument true includes the native window), or a canvas carry with `(path, target,
     * "save")`, or film: `(path, {over, every, window})` (LLP 1012.001.000 D2). */
    screenshot: async (path, target = false, form, film = false, of) => {
      // `screenshot shots/home.png` makes `shots/`, as a shell redirect would not but every agent expects.
      mkdirSync(dirname(resolve(path)), { recursive: true });
      if (target && typeof target === 'object') return s.film(path, target);
      if (form === 'save') {
        if (!['web', 'macos', 'ios', 'linux', 'windows'].includes(s.host))
          throw new Error(`world save unavailable on this host yet: ${s.host}`);
        const reply = await s.op({ op: 'screenshot', ...(await s.target(target)), world: true, form: 'save' });
        const { data, ...metadata } = reply;
        if (typeof data !== 'string')
          throw new Error(
            `canvas ${target} returned no save bytes; inspect state and state ${target}:* for the refusal reason`,
          );
        if (reply.bytes > WORLD_LIMIT || data.length > 4 * Math.ceil(WORLD_LIMIT / 3))
          throw new Error(
            'world carrier exceeds 256 MiB limit; inspect `state world:*` and reduce saved entities before `screenshot checkpoint.world world save`',
          );
        const bytes = Buffer.from(data, 'base64');
        if (bytes.length !== reply.bytes)
          throw new Error(
            `canvas ${target} returned a truncated save; inspect state and logs; retry screenshot checkpoint.world ${target} save`,
          );
        writeFileSync(resolve(path), bytes);
        return s.tagged({ ...metadata, screenshot: resolve(path) });
      }
      // `window status` (iOS): with the status bar as the device shows it, to compare with a `simctl io` reference
      // (euxy FRICTION 5).
      if (form === 'status' && !(target === true && ['ios', 'host-ios'].includes(host)))
        throw new MalformedOperation("screenshot <png> window status: an iOS simulator's window with its status bar");
      // `window app` (iOS): the app's own drawing alone, never the display's (smoke's canvas readback, whose reference
      // is one).
      if (form === 'app' && !(target === true && ['ios', 'host-ios'].includes(host)))
        throw new MalformedOperation("screenshot <png> window app: an iOS simulator's window as the app draws it");
      // `window pictureinpicture` (macOS): Exact's picture-in-picture window, outside the app's (LLP 1042 §9.1).
      if (form === 'pictureinpicture' && !(target === true && ['macos', 'mac'].includes(host)))
        throw new MalformedOperation(
          "screenshot <png> window pictureinpicture: Exact's picture-in-picture window, on macOS",
        );
      // With several windows floating, the image names which one (`window pictureinpicture <image>`).
      if (form === 'pictureinpicture') {
        const r = await carrier.ask({
          op: 'screenshot',
          path: resolve(path),
          window: 'pictureinpicture',
          ...(of !== undefined ? { id: (await s.target(of)).id } : {}),
        });
        if (r.error) throw new Error(r.error);
        return s.tagged(r);
      }
      if (form !== undefined && form !== 'status' && form !== 'app')
        throw new MalformedOperation(`screenshot: unknown form ${form}`);
      if (typeof target === 'string')
        return worldTells(
          s,
          await s.op({ op: 'screenshot', ...(await s.target(target)), world: true, path: resolve(path) }),
        );
      const r = await carrier.screenshot(resolve(path), target, film, form === 'status', form === 'app');
      if (scale === undefined) return worldTells(s, await s.tagged(r)); // a drawing world's tells (LLP 1127.000 D1)
      const { image, shrunk } = s.scaled(r);
      if (shrunk) writeFileSync(r.screenshot, encodePng(image));
      return worldTells(s, await s.tagged({ ...r, scale }));
    },
    scaled(r) {
      return scaledScreenshot(r, scale, host);
    },
    /** Film (LLP 1012.001.000 D2): `agent-film.mjs`. */
    film(path, opts) {
      return film(s, { carrier, timing, scale }, path, opts);
    },
    /** An input's end (LLP 1012 §2): the `then` of each answer the input settled lands before the reply, as a click
     * handler's state is there for a test's next line — the clock unmoved and no timer fired (trivia F3, kanban F19).
     * A reply still on real time (a store's, the network's) stays for a `clock` step. The reply's tags are read
     * after it. */
    async landed(r) {
      if (r == null || r.error != null || r.delivery === 'unsupported') return s.tagged(r);
      // The input closed the window (the app's `close()`, or `close`'s button): no session is left to land in.
      if (r.closed) return r;
      const l = await carrier.ask({ op: 'clock', land: true });
      // delivered: not `error`, so neither an agent nor a test sends it again as if refused
      if (l.error) return { ...r, landError: l.error };
      // A sheet the input (or its answer's `then`) opened or closed still moving past the iOS host's bound.
      if (l.settled === false) {
        r.settled = false;
        r.reason = l.reason;
      }
      delete r.epoch;
      delete r.incarnation;
      delete r.clock;
      // No scratch store: the input's reply says so, for an `open()` script as for the CLI, on every carrier
      // (LLP 1102 §3.17); the read is bounded, advice only.
      if (storage === undefined && !s.storageSaid && (s.storageProbes = (s.storageProbes ?? 0) + 1) <= 20) {
        const j = await Promise.race([
            peekLogs(s, s.storagePeek ?? 0).catch(() => null),
            new Promise(done => setTimeout(() => done(null), 2000)),
          ]),
          hit =
            Array.isArray(j?.lines) &&
            ((s.storagePeek = j.next),
            j.lines.some(x => STORAGE_REFUSED.test(typeof x === 'string' ? x : JSON.stringify(x))));
        if (hit) {
          s.storageSaid = true;
          r.note = [r.note, STORAGE_NOTE].filter(Boolean).join('; ');
        }
      }
      return s.tagged(r);
    },
    /** Every reply carries the runner's `epoch`, `incarnation` and `clock` (LLP 1035.002 D3). A host that answered
     * the operation itself stamps them; the web carrier's input and capture are the driver's own (CDP), so the driver
     * reads the tags after the operation and adds what the reply lacks. An error is left alone. */
    async tagged(r) {
      if (r == null || r.error != null || r.epoch != null) return r;
      const tags = await s.op({ op: 'tags' });
      for (const key of Object.keys(tags)) if (r[key] === undefined) r[key] = tags[key];
      return r;
    },
    close: carrier.close,
    springboard: carrier.springboard,
  };
  Object.assign(s, sessionInput(s, { carrier, host, timing, touch, app }));
  // Platform timing and Android's reader start on a live host clock; seed the driver's relative seeks where the host
  // handed time over (LLP 1080.000 §12; LLP 1107 D1). Headless Android starts at zero, so the same read keeps its seek
  // clock unchanged.
  if ((timing === 'platform' && carrier.host !== 'web') || carrier.host === 'android') {
    const { clock } = await s.op({ op: 'tags' });
    if (Number.isFinite(clock)) s.now = clock;
  }
  return s;
}
