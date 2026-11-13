// The iOS simulator carrier (scripts/agent.mjs `open`): the bundle `build.mjs --ios` assembled, installed and launched
// with the agent socket's path in its environment.
import { exclusiveIOS, simulatorDisplay, refuseBackendOrigin } from './agent-launch.mjs';
import { nativeBuild, planOverride, refuseBuild } from './agent-build.mjs';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { connect } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { openTouches, realTap, springboard, withAccessibilityRuntime } from '../host/apple/touches.mjs';
import { nativeKey } from './agent-keys.mjs';
import { jsonLines, hangup, waitAtMost, bootRefusal, simulatorConsole, REPLY_MS } from './agent-wire.mjs';
import { appleArtifacts, bundleId, install } from '../host/apple/build.mjs';
import {
  crashWatch,
  keepInstalledRelease,
  showSimulator,
  simulator,
  displayAsIs,
  withDisplay,
  withMaterials,
  withSoftwareKeyboard,
  forwardedEnv,
} from '../host/apple/devices.mjs';
import { appleExecutableName, resolveApp } from './app.mjs';
const sleep = ms => new Promise(r => setTimeout(r, ms));
/** The simulator carrier: the bundle `build.mjs --ios` assembled, installed and launched on a simulator with the
 * agent socket's path in its environment (simctl passes SIMCTL_CHILD_*); then the same JSON lines over that socket
 * (`AgentIOS.swift`). A `simctl launch --console` stays attached for the app's stdout and stderr (its
 * `--stdout=`/`--stderr=` files stay empty on Xcode 26). One app per bundle id per device: a session replaces a
 * running copy; closing hangs up the socket, which ends the app, and kills the pid the app reported if it lingers. */
export async function openIOS({
  plan,
  app,
  size,
  env: extra = {},
  session,
  hostFixture = false,
  touch = 'agent',
  ax = false,
  onProcess,
}) {
  const a = resolveApp(app);
  const built = nativeBuild('ios', a, { sample: hostFixture, checked: process.env.EXACT_BUILD_CHECKED }),
    bundle = built.built;
  if (!built.missing) refuseBackendOrigin(built, a);
  const id = hostFixture ? `${a.id}.host` : a.id;
  // An app's exact.mjs builds first instead, or bakes a plan-only edit to run on this build (agent-build.mjs,
  // LLP 1116.003).
  const override = plan || hostFixture ? null : planOverride('ios', a, built);
  if (override) plan = override.plan;
  else refuseBuild('ios', built, a, { sample: hostFixture });
  const dev = simulator(undefined, { app: id, dir: a.dir, hint: '--sim <udid|name> (or EXACT_SIM)' });
  return withSoftwareKeyboard(dev, () =>
    exclusiveIOS(dev.udid, id, () =>
      // An AX drive turns UIKit's accessibility runtime on for its launch (`withAccessibilityRuntime`).
      withAccessibilityRuntime(dev.udid, ax && touch === 'agent', () =>
        openIOSOwned({ a, bundle, id, dev, plan, size, extra, session, hostFixture, touch, onProcess }),
      ),
    ),
  );
}
/** Whether the app `simctl launch --console` started (`simulatorConsole`'s pid) still runs, and where a screenshot of
 * the simulator's display went. */
function stuckLaunch(udid, pid) {
  let running = false;
  try {
    if (pid) {
      process.kill(Number(pid), 0);
      running = true;
    }
  } catch {}
  const shot = resolve(tmpdir(), `exact-ios-launch-${Date.now()}.png`);
  const io = spawnSync('xcrun', ['simctl', 'io', udid, 'screenshot', shot], { encoding: 'utf8', timeout: 10000 });
  return [
    pid ? `its process ${pid} ${running ? 'is still running' : 'has exited'}` : 'simctl named no process for it',
    io.status === 0 ? `the simulator's display is in ${shot}` : "the simulator's display could not be captured",
  ].join('; ');
}
async function openIOSOwned({ a, bundle, id, dev, plan, size, extra, session, hostFixture, touch, onProcess }) {
  showSimulator(dev, true); // a person watching sees what is driven, and keeps the focus
  // Real touches (LLP 1080.000, `--touch platform`; `drag`, an authored test's drag alone): the runner starts first, so
  // its own launch never backgrounds the app.
  const touches =
    touch !== 'agent'
      ? await openTouches({
          udid: dev.udid,
          appId: id ?? bundleId(a.crate('apple')),
          foldable: dev.type === 'iPhone-Duo',
        })
      : null;
  // A Release build installed under the bundle id is copied aside first, and the note says how to put it back
  // (`keepInstalledRelease`).
  let kept = null;
  try {
    kept = keepInstalledRelease(dev.udid, id, bundle, hostFixture ? 'ExactHostIOS' : appleExecutableName(a, true));
    if (kept.install) install(dev, bundle, a, hostFixture, true);
  } catch (e) {
    await touches?.close();
    throw e;
  }
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-ios-'));
  const sock = resolve(dir, 'agent.sock');
  const env = {
    EXACT_ASSETS: appleArtifacts(a, { destination: 'ios-simulator', host: hostFixture }).capture,
    ...forwardedEnv(),
    EXACT_AGENT: '1',
    EXACT_AGENT_SOCKET: sock,
    ...(plan ? { EXACT_PLAN: plan } : {}),
    ...(size ? { EXACT_WINDOW_WIDTH: String(size[0]), EXACT_WINDOW_HEIGHT: String(size[1]) } : {}),
    ...extra,
  };
  const childEnv = { ...process.env };
  for (const [k, v] of Object.entries(env)) childEnv[`SIMCTL_CHILD_${k}`] = v;
  const crashName = hostFixture ? 'ExactHostIOS' : appleExecutableName(a, true);
  let closing = false;
  const crashes = crashWatch(crashName, bundle, { bundleId: id, udid: dev.udid }),
    // `logs`, a failed launch: this build's crash here (ReportCrash writes it seconds after the exit: LLP 1156.001 D4)
    crashed = (why, died) => new Error([why, ...crashes(died ? 10000 : 0)].join('\n'));
  const hostLines = [];
  let launch,
    pid = null,
    socket = null;
  // The launch brings the app to the front: not while another drive reads this simulator's display. The caller
  // holds the display lock (`simulatorDisplay`) through the accessibility setup and the launch.
  try {
    launch = simulatorConsole(
      () =>
        spawn(
          'xcrun',
          ['simctl', 'launch', '--console', '--terminate-running-process', dev.udid, id ?? bundleId(a.crate('apple'))],
          { env: childEnv, stdio: ['ignore', 'pipe', 'pipe'] },
        ),
      hostLines,
      onProcess,
    );
    // The app binds the socket once its first frame is applied: connect when it appears.
    const t = Date.now();
    while (!socket) {
      if (launch.done && !launch.retry()) {
        rmSync(dir, { recursive: true, force: true });
        await touches?.close();
        throw crashed('the simulator launch exited before opening its agent socket; ' + hostLines.join('\n'), true);
      }
      if (Date.now() - t > 20000) {
        // What was on the simulator, seen rather than guessed (LLP 1143 D6): a boot logo, a crash alert, a blank
        // window.
        const seen = stuckLaunch(dev.udid, launch.pid);
        try {
          launch.process.kill('SIGKILL');
        } catch {}
        await launch.exited;
        rmSync(dir, { recursive: true, force: true });
        await touches?.close();
        throw crashed(`the app never opened its agent socket; ${seen}\n` + hostLines.join('\n'));
      }
      socket = await new Promise(ok => {
        const s = connect(sock);
        s.once('connect', () => ok(s));
        s.once('error', () => {
          s.destroy();
          ok(null);
        });
      });
      if (!socket) await sleep(50);
    }
  } catch (e) {
    if (!launch) {
      rmSync(dir, { recursive: true, force: true });
      await touches?.close();
    }
    throw e;
  }
  socket.on('error', () => {});
  const lines = jsonLines(socket, socket, hostLines);
  const exited = new Promise(r =>
    socket.on('close', async () => {
      const exit = closing ? null : await waitAtMost(launch.exited, 1000);
      lines.fail(
        closing
          ? 'the app was closed'
          : crashed(
              hangup({ what: exit != null ? 'the app ended' : 'the app hung up', pid, exit, hostLines }),
              exit != null,
            ).message,
      );
      r(); // its report lands seconds after the exit
    }),
  );
  const close = async () => {
    closing = true;
    try {
      socket.end();
    } catch {}
    const hungUp = await waitAtMost(
      exited.then(() => true),
      2000,
    );
    let touchFailure = null;
    try {
      await touches?.close();
    } catch (e) {
      touchFailure = e;
    }
    if (pid) {
      try {
        process.kill(pid, 'SIGKILL');
      } catch {}
    }
    // the socket closed as the app ended: `simctl launch` hears of it ~0.5 s later, with nothing to add
    if (!hungUp) await waitAtMost(launch.exited, 1000);
    if (!launch.done) {
      try {
        launch.process.kill('SIGKILL');
      } catch {}
      await launch.exited;
    }
    rmSync(dir, { recursive: true, force: true });
    if (touchFailure) throw touchFailure;
  };
  try {
    const ready = await waitAtMost(lines.next(), 20000, () => {
      throw new Error('the app never became ready; ' + hostLines.join('\n'));
    });
    if (!ready.ready) throw new Error('unexpected first line: ' + JSON.stringify(ready));
    if (ready.error) throw new Error(bootRefusal(ready.error));
    pid = ready.pid ?? null;
    // The sample host routes by label, as the macOS one does over stdio.
    const state = { session: session ?? null };
    const ask = (req, session = state.session) =>
      waitAtMost(
        lines.ask(session ? { ...req, session } : req),
        req.op === 'perf' && req.live > 0 ? req.live + REPLY_MS : REPLY_MS,
        () => {
          const why = hangup({ what: `${req.op} did not answer within ${REPLY_MS / 1000} s`, pid, hostLines });
          lines.fail(why);
          throw new Error(why);
        },
      );
    let canvasContact = false;
    return {
      host: hostFixture ? 'host-ios' : 'ios',
      boot: ready.boot,
      hostLines,
      gpuMs: () => null,
      sessions: ready.sessions ?? null,
      state,
      plan,
      touches,
      touch,
      crashes,
      udid: dev.udid,
      installNote: kept?.note,
      springboard: request =>
        springboard(touches, request, name => {
          if (socket.destroyed || launch.done)
            throw crashed(`${name}: the app's agent socket is gone; the archive comparison lost its app session`);
        }),
      ask,
      reveal: id => ask({ op: 'reveal', id }),
      async input(id, kind, opts) {
        if (kind === 'key') {
          const session = state.session;
          return nativeKey({ id, opts, ask: req => ask(req, session) });
        }
        const guest = {
          selector: opts.selector,
          x: opts.x,
          y: opts.y,
          entity: opts.entity,
          world: opts.world,
          under: opts.under,
          phase: opts.phase,
        };
        if (['down', 'move', 'hold', 'up', 'cancel'].includes(kind)) {
          if (kind === 'down' || canvasContact) {
            // `pan`: a pan's or a canvas's contact alone, for a drag. The recognized finger: under --touch platform the
            // session plays a held contact as one real touch instead (agent-drag.mjs `heldPhase`), with a world
            // entity's tap the one contact still sent here.
            const r = await ask({
              op: 'tap',
              phase: kind,
              ...(id != null ? { id } : {}),
              x: opts.x,
              y: opts.y,
              dx: opts.dx,
              dy: opts.dy,
              ms: opts.ms,
              ...(opts.virtual ? { virtual: true } : {}),
              ...(opts.pan ? { pan: true } : {}),
            });
            // A move refused partway answers with where its delivered samples left the contact (`pointer` keeps it).
            if (r.error && !(kind === 'move' && Array.isArray(r.at))) throw new Error(r.error);
            if (r.delivery !== 'unsupported' || canvasContact) {
              canvasContact = !['up', 'cancel'].includes(kind);
              return r;
            }
            if (r.reason) return r; // the host's own why (a contact only UIKit could arbitrate), no contact held
          }
          // A held contact needs a touch that stays down across requests;
          // XCTest's public touches each press and lift (LLP 1080.000 D3).
          return {
            phase: kind,
            delivery: 'unsupported',
            reason: 'no held contact across requests on iOS (LLP 1080.000 P3)',
          };
        }
        // Under `--touch platform` a press is a real touch or nothing: a guest
        // (iframe) or world press would be dispatched by the host and must
        // never be reported as platform input (LLP 1080.000 D8).
        // One whole real gesture (LLP 1080.000 §11): only the runner makes one.
        if (kind === 'drag')
          return realTap({ ask, touches, id, at: opts.at, drag: opts.drag, abandon: why => lines.fail(why) });
        if (touch === 'platform' && kind === 'press') {
          if (Object.values(guest).some(v => v != null))
            throw new Error(
              'unsupported under --touch platform: a press into an iframe guest or a world entity reaches no real touch yet (LLP 1080.000 stage 1)',
            );
          return realTap({ ask, touches, id, rel: opts.at, abandon: why => lines.fail(why) });
        }
        const r =
          kind === 'contextmenu' || kind === 'dblclick'
            ? await ask({ op: 'tap', id, [kind]: true, ...(opts.at ? { at: opts.at } : {}) })
            : kind === 'pinch'
              ? await ask({ op: 'tap', id, pinch: opts.pinch, at: opts.at })
              : kind === 'wheel'
                ? await ask({
                    op: 'tap',
                    id,
                    wheel: opts.wheel,
                    ...(opts.gesture ? { gesture: true } : {}),
                    ...(opts.modifiers ? { modifiers: opts.modifiers } : {}),
                  })
                : kind === 'drop'
                  ? await ask({ op: 'tap', id, drop: opts.drop })
                  : kind === 'dragenter' || kind === 'dragleave'
                    ? await ask({ op: 'tap', id, [kind]: opts[kind] })
                    : kind === 'hover'
                      ? await ask({ op: 'tap', id, hover: true })
                      : kind === 'press'
                        ? await ask({
                            op: 'tap',
                            id,
                            ...guest,
                            ...(opts.at !== undefined ? { at: opts.at } : {}),
                            ...(opts.modifiers ? { modifiers: opts.modifiers } : {}),
                          })
                        : kind === 'mediasession'
                          ? await ask({
                              op: 'tap',
                              id,
                              mediaSession: opts.mediaSession,
                              ...(opts.seconds != null ? { seconds: opts.seconds } : {}),
                            })
                          : await ask({
                              op: 'type',
                              id,
                              text: opts.text,
                              ...(opts.clipboard ? { clipboard: opts.clipboard } : {}),
                              ...(opts.files ? { files: opts.files } : {}),
                              ...(opts.image ? { image: opts.image } : {}),
                              ...guest,
                            });
        if (r.error) throw new Error(r.error);
        return r;
      },
      // `own`: the app's drawing alone, never the display's (smoke's canvas readback); `window === 'display'`: the
      // display as it is (`displayAsIs`).
      async screenshot(path, window = false, film = false, status = false, own = false) {
        const take = async () => {
          const r = await ask({
            op: 'screenshot',
            path,
            window: window === 'display' || window,
            ...(film ? { film } : {}),
          });
          if (r.error) throw new Error(r.error);
          if (window === 'display' && !film) return displayAsIs(r, path, dev.udid);
          const bar = r.statusBar;
          if (!status) delete r.statusBar;
          // The host draws the window (with an alert or sheet up, or asked); its pixels are then the display's, `simctl
          // io` captures (about 1 s), which draw glass as the device does and hold the keyboard; the status bar's only
          // when asked. `probe` asks the app, after the display is read, whether it was still in front.
          const probe = () => ask({ op: 'screenshot', probe: true });
          if (film) return r; // a page picture over glass takes the window's display pixels, cut to it:
          if (!(window || r.window === true))
            return own
              ? (delete r.materials, r)
              : withMaterials(r, path, async p => {
                  const w = await ask({ op: 'screenshot', path: p, window: true });
                  if (w.error) throw new Error(w.error);
                  return withDisplay(w, p, dev.udid, w.statusBar ?? 0, { probe });
                });
          return withDisplay(r, path, dev.udid, status ? 0 : (bar ?? 0), { own, probe });
        };
        // Films take no display: their frames are the app's alone.
        return film ? take() : simulatorDisplay(dev.udid, take);
      },
      close,
    };
  } catch (e) {
    const died = launch.done; // before close ends the launch
    try {
      await close();
    } catch (failure) {
      e.message += `; ${failure.message}`;
    }
    throw /crash report:/.test(e.message) ? e : Object.assign(e, { message: crashed(e.message, died).message });
  }
}
