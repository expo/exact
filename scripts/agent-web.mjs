// The web carrier (scripts/agent.mjs `open`): Chrome on its own CDP pipe, over the app's checked web build or a
// drive's own plan. Firefox and WebKit are agent-playwright.mjs's.
import {
  Cdp,
  agentPage,
  browserDiagnosticNoise,
  launchLog,
  launchFailure,
  drivePagePort,
  closePage,
  backLayer,
  cdpEscape,
  copyCdpFailureContext,
  chromium,
  webNeeds,
  DESKTOP_INPUT,
  closeWindowsBrowser,
  quitBrowser,
  retainCleanupError,
  removeBrowserProfile,
  driveStore,
  listenOn,
  withEpoch,
  withFaults,
  refuseStale,
  webChanges,
  builtSettingLines,
  parityScript,
  netFlags,
} from './agent-launch.mjs';
import { worldFile } from './agent-inspect.mjs';
import { LAUNCH_MEDIA, preferWeb } from './agent-prefer.mjs';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { pageAim, pagePoint, pageOutside } from './agent-aim.mjs';
import { fileDrag, driverFiles, pageContact } from './agent-drag.mjs';
import {
  MalformedOperation,
  cdpKey,
  browserKey,
  withHeldModifiers,
  deliverClipboard,
  chordModifiers,
  withChordModifiers,
  heldForClick,
  cdpWheel,
} from './agent-keys.mjs';
import { waitAtMost } from './agent-wire.mjs';
import { builtAppMatches, jsTargetBuild, serveBuildTree, serveStatic } from '../host/web/serve.mjs';
import { bootedGeneration, leaseGeneration } from '../host/web/generations.mjs';
import { resolveApp, webDist as defaultWebDist } from './app.mjs';
const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const sleep = ms => new Promise(r => setTimeout(r, ms));
/** Every desktop carrier's viewport unless a drive names one (LLP 1012.001.000 D8, Charlie 2026-09-30: 900, the
 * page's and the conformance run's), so one drive gives one set of numbers on every host. A phone or simulator is
 * its device's size. */
export const VIEWPORT = [420, 900];
/** An app made by `exact new` builds itself: its own `exact.mjs web-build`, when the dist is its default one. */
const ownWebBuild = (app, dist) =>
  app.dir &&
  app.target &&
  existsSync(resolve(app.dir, 'exact.mjs')) &&
  resolve(dist) === resolve(app.target, 'web-dist')
    ? `(cd '${String(app.dir).replaceAll("'", "'\\''")}' && bun exact.mjs web-build)`
    : null;
/** Refuse to drive anything but a complete, authenticated build of the
 * selected app. The build marker binds every public runtime artifact. */
export async function assertWebDistApp(dist, app) {
  const shellQuote = value => "'" + String(value).replaceAll("'", "'\\''") + "'";
  if (!(await builtAppMatches(dist, app)))
    throw new Error(
      `web dist is not a complete build for selected app ${app.id}; stale receipt ${resolve(dist, '.exact-build.json')}; run ${ownWebBuild(app, dist) ?? `EXACT_APP_DIR=${shellQuote(app.dir)} EXACT_WEB_DIST=${shellQuote(resolve(dist))} bun host/web/build.mjs ${app.crate('web')}`}`,
    );
}
export const openWeb = async options => {
  const { browser, reuse, url, facts, mediaClock = 'wall', lineHeight = null } = options,
    selected = resolveApp(options.app);
  if (!url || url.startsWith('/'))
    await (
      await import('./agent-test.mjs')
    ).backendPreflight(selected, resolve(options.webDist ?? defaultWebDist(selected)));
  if (browser === 'chrome' && reuse) {
    if (
      reuse.browser === 'chrome' &&
      JSON.stringify(reuse.launchFacts) === JSON.stringify(facts) &&
      (reuse.mediaClock ?? 'wall') === mediaClock &&
      (reuse.lineHeight ?? null) === lineHeight
    ) {
      try {
        await reuse.reset({ session: true });
        return reuse;
      } catch (error) {
        await reuse.close();
        throw error;
      }
    }
    await reuse.close();
    options = { ...options, reuse: undefined };
  }
  return (await import('./agent-launch.mjs')).exclusiveWebStore(options, openWebUnlocked);
};
/** Two animation frames in the page, or false when they have not come within `ms`. The deadline is the page's own,
 * so a stalled requestAnimationFrame settles Runtime.evaluate instead of hanging it. */
export const webFrame = (evaluate, ms = 250) =>
  evaluate(`new Promise((resolve) => {
  let second, settled = false;
  const finish = (value) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    cancelAnimationFrame(first);
    if (second !== undefined) cancelAnimationFrame(second);
    resolve(value);
  };
  const timer = setTimeout(() => finish(false), ${ms});
  const first = requestAnimationFrame(() => { second = requestAnimationFrame(() => finish(true)); });
})`);
async function openWebUnlocked({
  browser = 'chrome',
  plan,
  world,
  size = VIEWPORT,
  url: pageURL,
  app,
  webDist,
  beforeSpawn,
  onProcess,
  reuse,
  storage,
  fresh = false,
  facts,
  env,
  mediaClock = 'wall',
  lineHeight = null,
  port: fixed = null,
}) {
  env = { ...process.env, ...env };
  const selected = resolveApp(app),
    dist = resolve(webDist ?? defaultWebDist(selected));
  // A launch location (`--url /share?text=hi`, LLP 1038 D5) is a path on the page this carrier serves.
  const location = pageURL?.startsWith('/') ? pageURL : null;
  if (location) pageURL = undefined;
  if (location && browser !== 'chrome')
    throw new Error(`--url ${location}: a launch location on the web is Chrome's drive`);
  if (browser !== 'chrome') {
    const { openPlaywrightWeb } = await import('./agent-playwright.mjs');
    return openPlaywrightWeb({
      browser,
      plan,
      world,
      size,
      url: pageURL,
      app,
      webDist,
      onProcess,
      reuse,
      storage,
      facts,
      env,
      port: fixed,
      parity: parityScript({ mediaClock, lineHeight }),
    });
  }
  if (!pageURL) {
    await assertWebDistApp(dist, selected);
    const js = jsTargetBuild(dist),
      env =
        selected.outside || webDist || process.env.EXACT_WEB_DIST
          ? `EXACT_APP_DIR=${selected.dir} EXACT_WEB_DIST=${dist} `
          : '';
    const command =
        (js && ownWebBuild(selected, dist)) ||
        `${env}bun host/web/build.mjs ${selected.crate('web')}${js ? '' : ' --wasm'}`,
      changed = webChanges(dist, selected);
    refuseStale('web', resolve(dist, '.exact-build.json'), changed.all, command);
  }
  // A JS-target build (LLP 1071) is served as its tree. It compiles one plan
  // ahead of time, so `--plan` is a JS build of that plan (host/web-js/build.mjs
  // --plan, over the app's data sources: its Rust module from the dist, when
  // it has one), served instead of the app's; on the wasm target the page
  // boots the app and swaps the plan in (`exact.reload`).
  const js = !pageURL && jsTargetBuild(dist);
  let served = dist,
    planBuild = null;
  if (js && plan) {
    planBuild = mkdtempSync(resolve(tmpdir(), 'exact-agent-plan-'));
    const b = spawnSync(
      process.execPath,
      [
        resolve(ROOT, 'host/web-js/build.mjs'),
        selected.name,
        '--plan',
        resolve(plan),
        '--out',
        planBuild,
        '--render',
        'none',
        ...(existsSync(resolve(dist, 'rust/wasm/app.module.wasm')) ? ['--data', dist] : []),
      ],
      { cwd: ROOT, env: { ...process.env, EXACT_APP_DIR: selected.dir }, encoding: 'utf8', maxBuffer: 64 << 20 },
    );
    if (b.status !== 0) {
      rmSync(planBuild, { recursive: true, force: true });
      throw new Error(
        `--plan ${plan}: its JS build failed: ${(b.stderr ?? '').trim().split('\n').slice(-3).join(' ')}`,
      );
    }
    served = planBuild;
  }
  // The drive's build: the generation it boots, leased while it runs, so another build put in place meanwhile (a dev
  // loop's, another `test web`'s) never answers this page (generations.mjs). One kept no more (a lease a crash lost)
  // ends the drive: it never moves to another build on its own.
  const pinned = js ? await bootedGeneration(served) : null,
    release = pinned ? leaseGeneration(served, pinned) : () => {};
  let gone = null;
  const server = createServer((req, res) => {
    if (req.url === '/__plan' && plan) {
      res.writeHead(200, { 'content-type': 'application/octet-stream' });
      res.end(readFileSync(plan));
      return;
    }
    if (req.url === '/favicon.ico') {
      res.writeHead(204);
      res.end();
      return;
    }
    if (js) {
      res.once('finish', () => {
        if (res.statusCode === 410) gone ??= pinned;
      });
      return serveBuildTree(served, req, res, { pinned });
    }
    serveStatic(dist, req, res);
  });
  // A named store's profile and origin are kept between drives, as a native scratch store is; `fresh` empties it first
  // (a test's).
  const kept = storage === undefined ? null : driveStore(selected, storage, env);
  if (kept && fresh) await removeBrowserProfile(kept.profile);
  const pagePort = drivePagePort(fixed, pageURL ? null : kept, env?.EXACT_AGENT_EPHEMERAL_PORT === '1', env);
  await listenOn(server, pagePort, '127.0.0.1', undefined, !!fixed, env).catch(e => {
    throw e.code !== 'EADDRINUSE' || fixed || pagePort !== kept?.port
      ? e
      : new Error(
          `--storage ${storage}: its page's port ${kept.port} is in use: another drive of this store is open (a store is one drive's at a time), or another program holds the port: try another store name, or --port <n>`,
        );
  });
  const port = server.address().port;
  if (!openWeb.said)
    console.error(
      `${(openWeb.said = `web: ${selected.name} served at http://127.0.0.1:${port}/`)}${fixed ? '' : ' (--port <n> or EXACT_PORT picks the port)'}`,
    );
  const chrome = chromium({ needs: webNeeds(selected) }).executable,
    // LLP 1115.000 D10.3: EXACT_SCALE, the Linux host's variable, launches Chrome at that device scale factor too, its
    // own and not CDP's emulated one (which leaves a 0.5px border 1px): an override's 0 keeps it
    scale = Number(env?.EXACT_SCALE ?? process.env.EXACT_SCALE) || 0;
  const profile = kept
    ? (mkdirSync(kept.profile, { recursive: true }), kept.profile)
    : mkdtempSync(resolve(tmpdir(), 'exact-agent-'));
  let child;
  try {
    beforeSpawn?.();
    child = spawn(
      chrome,
      [
        '--headless=new',
        DESKTOP_INPUT,
        '--remote-debugging-pipe',
        `--window-size=${size[0]},${size[1]}`,
        '--hide-scrollbars',
        ...(scale ? [`--force-device-scale-factor=${scale}`] : []),
        '--enable-unsafe-webgpu',
        '--disable-smooth-scrolling',
        `--user-data-dir=${profile}`,
        '--no-sandbox',
        '--disable-extensions',
        '--disable-background-networking',
        '--disable-component-update',
        '--no-first-run',
        '--no-default-browser-check',
        // The profile is a throwaway: a mock keychain and a plain password
        // store keep Chrome from asking the login keychain, which a shell
        // without keychain access refuses (errSecInteractionNotAllowed).
        '--use-mock-keychain',
        '--password-store=basic',
        ...netFlags({ ...process.env, ...env }),
        'about:blank',
      ],
      { detached: true, stdio: ['ignore', 'ignore', 'pipe', 'pipe', 'pipe'], windowsHide: true },
    );
    // Bun exposes null stdio on a failed spawn; wait before constructing CDP.
    // A pid is a started child: Bun 1.4.2 can drop the 'spawn' event of the
    // first child with extra pipes a test process starts, so only a child
    // without one waits for the event.
    if (!child.pid)
      await new Promise((ok, fail) => {
        child.once('spawn', ok);
        child.once('error', fail);
      });
  } catch (error) {
    server.close();
    if (!kept) rmSync(profile, { recursive: true, force: true });
    throw new Error(
      `web carrier unavailable: ${chrome}: ${error.code}; install Playwright's Chromium (docs/testing.md, Browsers), or set CHROME`,
    );
  }
  onProcess?.(child);
  const hostLines = pageURL ? [] : builtSettingLines(dist),
    launchTail = launchLog();
  let partial = ''; // a line split across stderr chunks
  const line = l => {
    if (!l) return;
    if (!browserDiagnosticNoise(l)) hostLines.push('chrome: ' + l);
    launchTail.add(l); // unfiltered, for a launch that fails
  };
  child.stderr.on('data', d => {
    const parts = (partial + d).split('\n');
    partial = parts.pop();
    parts.forEach(line);
  });
  child.stderr.on('end', () => {
    line(partial);
    partial = '';
  });
  const stdioClosed = new Promise(r => child.on('close', r)); // after 'exit', once stderr is drained
  const cdp = new Cdp(child.stdio[3], child.stdio[4]);
  const exited = new Promise(r => {
    child.on('exit', (code, signal) => {
      cdp.fail(`Chrome exited (${code ?? signal})`);
      r();
    });
    child.on('error', error => {
      cdp.fail(
        `web carrier unavailable: ${chrome}: ${error.code}; install Playwright's Chromium (docs/testing.md, Browsers), or set CHROME`,
      );
      r();
    });
  });
  const close = async () => {
    try {
      if (process.platform === 'win32') await closeWindowsBrowser(child, cdp, exited, profile);
      else {
        // A kill loses a named store's `localStorage`, which Chrome writes only at shutdown (app farm,
        // set-variants-0771).
        await quitBrowser(child, cdp, exited);
        await waitAtMost(exited, 2000);
      }
    } finally {
      server.close();
    }
    if (child.exitCode === null && child.signalCode === null)
      throw new Error(`Chrome ${child.pid} did not exit; owned profile retained at ${profile}`);
    if (!kept) await removeBrowserProfile(profile);
    if (planBuild) rmSync(planBuild, { recursive: true, force: true });
    release();
  };
  try {
    // A Chrome that dies at launch says why only on its stderr; the pipe just closes.
    const { targetInfos } = await cdp.send('Target.getTargets').catch(async error => {
      await waitAtMost(stdioClosed, 1000);
      if (partial) launchTail.add(partial); // and an unterminated last line
      throw copyCdpFailureContext(
        error,
        launchFailure(error, launchTail, { alive: child.exitCode === null && child.signalCode === null, chrome }),
      );
    });
    const target =
      targetInfos.find(t => t.type === 'page') ?? (await cdp.send('Target.createTarget', { url: 'about:blank' }));
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId: target.targetId, flatten: true });
    const heldKeys = new Map();
    let documentLoader = null;
    const call = async (method, params, timeoutMs) => {
      const reply = await cdp.send(method, withHeldModifiers(method, params, heldKeys), sessionId, timeoutMs);
      if (method === 'Input.dispatchKeyEvent') {
        if (params.type === 'keyUp') heldKeys.delete(params.code);
        else if (params.type === 'keyDown' || params.type === 'rawKeyDown') heldKeys.set(params.code, params);
      }
      return reply;
    };
    cdp.listeners.push(msg => {
      if (msg.sessionId !== sessionId) return;
      if (msg.method === 'Page.frameNavigated' && !msg.params.frame.parentId)
        documentLoader = msg.params.frame.loaderId;
      if (msg.method === 'Runtime.consoleAPICalled')
        hostLines.push(
          `console.${msg.params.type}: ` + msg.params.args.map(a => a.value ?? a.description ?? a.type).join(' '),
        );
      else if (msg.method === 'Runtime.exceptionThrown')
        hostLines.push(
          'exception: ' + (msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text),
        );
      else if (msg.method === 'Log.entryAdded' && msg.params.entry.level !== 'verbose')
        hostLines.push(`${msg.params.entry.level}: ${msg.params.entry.text}`);
    });
    await call('Runtime.enable');
    await call('Log.enable');
    await call('Page.enable');
    // Timestamp the actual browser input before a lazy surface module exists.
    // Its first-frame latency belongs in state.world.perf, outside the clock/hash.
    await call('Page.addScriptToEvaluateOnNewDocument', {
      source: `
      addEventListener('click', event => {
        if (event.isTrusted) { performance.clearMarks('exact-agent-input'); performance.mark('exact-agent-input', {startTime: event.timeStamp}); }
      }, true);
      // A wheel past the page's edge bounces (macOS elastic overscroll) on the
      // compositor's own clock, not the drive's: a screenshot after it caught
      // the page 1-10 px low in some runs. Off, on every target.
      addEventListener('DOMContentLoaded', () => { document.documentElement.style.overscrollBehavior = 'none'; });
    `,
    });
    // Conformance's parity (agent-launch.mjs `parityScript`): a frozen media clock, a fixed line height.
    if (parityScript({ mediaClock, lineHeight }))
      await call('Page.addScriptToEvaluateOnNewDocument', { source: parityScript({ mediaClock, lineHeight }) });
    // The page has the focus, as a person's page does and as Playwright makes it: an unfocused page's `focus()`
    // (the document's autofocus at boot) moved the focus and fired no `focus` event, which iOS fires (splitter rough
    // 13).
    await call('Emulation.setFocusEmulationEnabled', { enabled: true });
    // The viewport exactly: Chrome will not make a window narrower than 500.
    await call('Emulation.setDeviceMetricsOverride', {
      width: size[0],
      height: size[1],
      deviceScaleFactor: scale ? 0 : 1,
      mobile: false,
    });
    const evaluate = async (expression, timeoutMs) => {
      // A function of no arguments is called in the page, as Playwright's carriers do (a method's shorthand too).
      if (typeof expression === 'function') {
        let text = `${expression}`;
        try {
          new Function(`(${text})`);
        } catch {
          text = text.startsWith('async ') ? `async function ${text.slice(6)}` : `function ${text}`;
        }
        expression = `(${text})()`;
      }
      const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, timeoutMs);
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
      return r.result.value;
    };
    if (world && !plan) {
      const encoded = worldFile(world).toString('base64');
      await call('Page.addScriptToEvaluateOnNewDocument', {
        source: `globalThis.exactWorldCarry = Uint8Array.from(atob(${JSON.stringify(encoded)}), c => c.charCodeAt(0));`,
      });
    }
    // The page: this carrier's own server over dist/, or a URL the caller
    // named — the dev server, so a drive can watch an edit arrive.
    const page = agentPage(pageURL ? new URL(pageURL) : new URL(location ?? '/', `http://127.0.0.1:${port}/`), {
      facts,
      storage,
      env,
    });
    // Display preferences are the agent's from launch, never the machine's (LLP 1069.007 D2); `prefer` changes them.
    const emulated = { ...LAUNCH_MEDIA };
    await call('Emulation.setEmulatedMedia', {
      features: Object.entries(emulated).map(([name, value]) => ({ name, value })),
    });
    // Reset before the history mirror records the new session's browser index.
    await call('Page.resetNavigationHistory');
    await evaluate(`location.replace(${JSON.stringify(page.href)})`);
    // The first frame: the glue stamps the root when it is in the DOM. A fresh profile's first launch can be slow.
    const t = Date.now();
    let boot = null;
    while (boot == null) {
      if (Date.now() - t > 30000) throw new Error('the page never booted; ' + hostLines.join('\n'));
      await sleep(15);
      boot = await evaluate("document.getElementById('exact-root')?.dataset.bootMs ?? null").catch(() => null);
    }
    await evaluate('exact.ready'); // First pixel precedes deferred module readiness.
    if (plan && !js) {
      const carry = world
        ? `exact.worldCarry = Uint8Array.from(atob(${JSON.stringify(worldFile(world).toString('base64'))}), c => c.charCodeAt(0));`
        : '';
      // Another plan is a new document: its autofocus runs (LLP 1035.000 D9).
      await evaluate(
        `fetch('/__plan').then((r) => r.arrayBuffer()).then((b) => { ${carry} return exact.reload(new Uint8Array(b), true); })`,
      );
    }
    const frame = () => webFrame(evaluate);
    // The one contact this carrier may hold (LLP 1035.003 D1).
    let contact = null;
    let dragged = null; // a file drag `tap … dragenter` holds over the page: its point and its DragData
    const rawAsk = async req => {
      if (gone)
        throw new Error(`the build this drive booted (generation ${gone}) is no longer kept: run the drive again`);
      // The existing resize input uses Chrome's real viewport and resize event.
      if (req.op === 'tap' && req.resize !== undefined) {
        const pair = req.resize;
        if (
          Object.keys(req).some(k => !['op', 'resize'].includes(k)) ||
          !Array.isArray(pair) ||
          pair.length !== 2 ||
          !pair.every(n => Number.isInteger(n) && n >= 64 && n <= 4096) ||
          pair[0] * pair[1] > 8388608
        )
          return {
            malformed: true,
            error:
              'tap resize needs exactly two integer dimensions in 64...4096, area <= 8388608, and no other input fields',
          };
        await call('Emulation.setDeviceMetricsOverride', {
          width: pair[0],
          height: pair[1],
          deviceScaleFactor: scale ? 0 : 1,
          mobile: false,
        });
        size = [...pair]; // a later screenshot reports this size, not the launch's
        await frame();
        return { resized: pair, viewport: await evaluate('[innerWidth, innerHeight]'), delivery: 'browser-viewport' };
      }
      if (req.op === 'tap' && req.close !== undefined) return closePage(req, { cdp, sessionId, call, frame });
      if (req.op === 'tap' && req.back !== undefined) {
        const r = await backLayer(req, { evaluate, escape: cdpEscape(call), frame });
        if (r) return r;
      } // an open dialog's Escape, else the page's Back (LLP 1116.006 D4)
      // A clock op waits up to the page's one 20 s request deadline (a jump, for a request in flight before a due
      // timer; a settle or `data`, across its rounds); 60 s is headroom past it and CDP's default 15 s (ios21 t3: one
      // never answered).
      const timeout = req.op === 'clock' ? 60000 : req.op === 'perf' && req.live > 0 ? req.live + 30000 : undefined;
      return JSON.parse(
        await evaluate(
          `(typeof globalThis.exact?.agentSettled === 'function' ? exact.agentSettled(${JSON.stringify(req)}) : Promise.reject(new Error('the page has no agent adapter: open a development build with ?agent=1'))).then((r) => JSON.stringify(r))`,
          timeout,
        ),
      );
    };
    const ask = req => rawAsk(req).then(MalformedOperation.reply);
    return {
      host: 'web',
      browser: 'chrome',
      boot: Number(boot),
      hostLines,
      evaluate,
      call,
      launchFacts: facts,
      mediaClock,
      lineHeight,
      port, // `port`: the page's origin, which keeps what it stored
      async gpuMs() {
        const ms = await evaluate("document.getElementById('exact-root')?.dataset.gpuMs ?? null");
        return ms == null ? null : Number(ms);
      },
      /** A fresh test clears storage and history; `keep` reloads the current entry with its history and store
       * (LLP 1038 D7). A new `session` on a reused carrier starts from the launch display preferences and pointer,
       * never the last session's `prefer` (LLP 1069.007 D2; agent-prefer.mjs). */
      async reset({ keep = false, failFetch, epoch, session = false } = {}) {
        if (session) {
          for (const name of Object.keys(emulated)) delete emulated[name];
          Object.assign(emulated, LAUNCH_MEDIA);
          await call('Emulation.setEmulatedMedia', {
            features: Object.entries(emulated).map(([name, value]) => ({ name, value })),
          });
          await call('Emulation.setTouchEmulationEnabled', { enabled: false });
        }
        // Release browser-owned input while its original document still exists.
        for (const key of heldKeys.values())
          await call('Input.dispatchKeyEvent', { ...key, type: 'keyUp', text: undefined });
        if (contact?.mouse)
          await call('Input.dispatchMouseEvent', {
            type: 'mouseReleased',
            x: contact.x,
            y: contact.y,
            button: 'left',
            buttons: 0,
            clickCount: 1,
          });
        else if (contact) await call('Input.dispatchTouchEvent', { type: 'touchCancel', touchPoints: [] });
        await frame();
        contact = null;
        await evaluate('sessionStorage.clear()');
        if (!keep) await call('Storage.clearDataForOrigin', { origin: page.origin, storageTypes: 'all' });
        const destination = withEpoch(withFaults(keep ? await evaluate('location.href') : page.href, failFetch), epoch);
        const previousLoader = documentLoader,
          deadline = Date.now() + 30000;
        hostLines.length = 0;
        if (!keep) await call('Page.resetNavigationHistory');
        await evaluate(`history.replaceState(${keep ? 'history.state' : 'null'}, '', ${JSON.stringify(destination)})`);
        await call('Page.reload');
        let boot;
        while (
          documentLoader === previousLoader ||
          (boot = await evaluate("document.getElementById('exact-root')?.dataset.bootMs ?? null").catch(() => null)) ==
            null
        ) {
          if (Date.now() > deadline) throw new Error('the reused page never booted');
          await sleep(15);
        }
        await evaluate('exact.ready');
        this.boot = Number(boot);
      },
      ask,
      async reveal(id) {
        const r = await ask({ op: 'reveal', id });
        if (r.scrolled) await frame();
        return r;
      },
      prefer: (media, page, fold, mode) =>
        // the browser's emulation through CDP, the glue's substitute where it offers none (agent-prefer.mjs)
        preferWeb({ media, page, fold, mode, emulated, call, evaluate, frame, ask, scale }),
      // A click with modifiers named holds them as a keyboard does (`heldForClick`; Charlie, 2026-10-07).
      input(id, kind, opts) {
        return heldForClick(call, kind, opts, () => this.deliverInput(id, kind, opts));
      },
      async deliverInput(id, kind, opts) {
        if (kind === 'key') cdpKey(opts.key); // Refuse before focus or browser input.
        // @ref LLP 1038 D11 — history.go delivers popstate in the page.
        if (kind === 'history') {
          const reply = await ask({ op: 'tap', id, history: opts.history });
          if (reply.error) throw new Error(reply.error);
          await frame();
          return reply;
        }
        // @ref LLP 1098 D10 — the handler the browser would call, through the media glue's `act`.
        if (kind === 'mediasession') {
          const r = await evaluate(
            `(globalThis.exact?.mediaSession?.act(${id}, ${JSON.stringify(opts.mediaSession)}, ${opts.seconds ?? null}) ?? { error: 'mediasession: this page publishes no media session (no media glue)' })`,
          );
          MalformedOperation.reply(r);
          if (r.error) throw new Error(r.error);
          await frame();
          return r;
        }
        if (
          kind === 'key' &&
          (opts.phase != null ||
            (await evaluate(
              `exact.gpu?.wantsInput(${id}) || exact.views.get(${id})?.matches('button, a[href], [role="button"], [role="link"]') || false`,
            )))
        ) {
          return browserKey({ id, opts, evaluate, ask, call, frame });
        }
        const r = id == null ? null : (await ask({ op: 'layout' })).nodes.find(n => n.id === id);
        if (id != null && (!r || (r.w === 0 && r.h === 0))) throw new Error(`view ${id} has no box on screen`);
        const point = ['contextmenu', 'mouse', 'auxclick', 'clicks', 'dblclick', 'wheel', 'press'].includes(kind)
          ? opts.at
          : null;
        // CDP's bits for the modifiers held: Alt 1, Control 2, Meta 4, Shift 8.
        const modifiers = String(opts.modifiers ?? '')
          .split('+')
          .reduce((m, k) => m | ({ Alt: 1, Control: 2, Meta: 4, Shift: 8 }[k] ?? 0), 0);
        const line =
          r &&
          !point &&
          (['press', 'contextmenu', 'dblclick', 'auxclick', 'clicks', 'hover'].includes(kind) ||
            (kind === 'wheel' && opts.hover))
            ? await evaluate(`(${pagePoint})({ id: ${id} })`)
            : null;
        let x = line ? line[0] : r ? r.x + (point?.[0] ?? r.w / 2) : contact?.x,
          y = line ? line[1] : r ? r.y + (point?.[1] ?? r.h / 2) : contact?.y;
        // A plain press presses what it names (LLP 1012 §1): beside a control its middle holds, or refused
        // (agent-aim.mjs).
        let avoided;
        if (kind === 'press' && id != null && !point && opts.x == null && opts.y == null && opts.selector == null) {
          const aim = await evaluate(`(${pageAim})(${JSON.stringify({ id, x, y })})`);
          if (aim?.error) throw new Error(aim.error);
          if (aim?.at) {
            [x, y] = aim.at;
            avoided = aim.avoided;
          }
        }
        if (kind === 'press' || kind === 'key' || kind === 'type') {
          const request =
            kind === 'press'
              ? { op: 'tap', id, selector: opts.selector, x: opts.x, y: opts.y }
              : {
                  op: 'type',
                  id,
                  selector: opts.selector,
                  ...(kind === 'key' ? { key: opts.key } : { text: opts.text }),
                };
          const guest = await ask(request);
          if (guest.guest === true || guest.handled === true) {
            if (guest.error) throw new Error(guest.error);
            await frame();
            return { ...guest, at: [x, y] };
          }
        }
        if (id != null && ['press', 'contextmenu', 'dblclick', 'auxclick', 'clicks', 'mouse'].includes(kind)) {
          // An ancestor under the point takes the press itself, never the target's handlers: a miss, unless the target
          // passes its presses through (`pointer-events: none`), as a person's would be (reader: a tap between a run's
          // lines reported success).
          const why = await evaluate(
            `(() => { const view = exact.views.get(${id}), el = exact.backButton?.(view) ?? view, hit = (el?.getRootNode() ?? document).elementFromPoint(${x}, ${y}), name = hit && (hit.dataset?.view ? 'node #' + hit.dataset.view : hit.tagName.toLowerCase()); return !el ? null : !hit ? (${pageOutside})({ id: ${id} }) : el === hit || el.contains(hit) || hit.contains(el) && getComputedStyle(el).pointerEvents === 'none' ? null : hit.contains(el) ? name + ', which holds it, is under the point and would take the press: no box the target draws is there' : name + ' covers its middle'; })()`,
          );
          if (why) throw new Error(`tap #${id} at (${x}, ${y}): ${why}`);
        }
        if (
          kind === 'mouse' &&
          !(await evaluate(
            `exact.views.get(${id})?.localName === 'canvas' || !!exact.views.get(${id})?.querySelector('canvas')`,
          ))
        )
          throw new Error('mouse requires a canvas target');
        if (kind === 'down' || kind === 'move' || kind === 'hold' || kind === 'up' || kind === 'cancel') {
          // A held contact (LLP 1035.003 D1) is a finger here: CDP touch
          // events. Chrome recognizes, scrolls and flings from them exactly
          // as it would from a hand; a timed move is delivered as steps on
          // real time so its velocity is real too. Each event carries the
          // contact's own timestamp (LLP 1057 §10.6): a lift follows the last
          // move by one frame, as a finger's does, however long the driver
          // takes between ops; `tap hold <ms>` is how a pause is said.
          // `mouse` (a drag's, files diary F10): the left button instead, so a
          // desktop path is what runs — and nothing scrolls by the contact.
          const mouse = kind === 'down' ? !!opts.mouse : !!contact?.mouse;
          // The modifiers held through the contact (#107): its down's, until a move or an up names others.
          const held = kind === 'down' || opts.modifiers !== undefined ? modifiers : (contact?.modifiers ?? 0);
          // A finger is injected as touch input without Chrome's touch emulation, which would flip
          // the page's `(pointer)` to coarse and `(hover)` to none for the rest of the drive: a
          // simulated finger does not change the device (Charlie, 2026-10-07; a real touch device,
          // `--touch platform` or iOS, stays coarse).
          const button = (type, at, t, buttons) =>
            call('Input.dispatchMouseEvent', {
              type,
              x: at.x,
              y: at.y,
              button: 'left',
              buttons,
              clickCount: type === 'mouseMoved' ? 0 : 1,
              timestamp: t,
              modifiers: held,
            });
          const finger = (type, touchPoints, timestamp) =>
            call('Input.dispatchTouchEvent', { type, touchPoints, timestamp, modifiers: held });
          if (kind === 'down') {
            if (contact) throw new Error('a contact is already down; use `tap up` first');
            const px = opts.x ?? x,
              py = opts.y ?? y;
            // CDP wants epoch seconds; performance.timeOrigin plus the
            // runner's clock makes the DOM event's timeStamp that clock,
            // independent of how long the carrier took between operations.
            const t = ((await evaluate('performance.timeOrigin')) + (await ask({ op: 'tags' })).clock) / 1000;
            if (!mouse) await evaluate(`(${pageContact})('start', ${JSON.stringify({ x: px, y: py })})`);
            if (mouse) {
              await button('mouseMoved', { x: px, y: py }, t, 0);
              await button('mousePressed', { x: px, y: py }, t, 1);
            } else await finger('touchStart', [{ x: px, y: py }], t);
            contact = { x: px, y: py, t, mouse, modifiers: held };
            await frame();
            return {
              contact: id,
              phase: 'down',
              at: [px, py],
              delivery: 'platform',
              ...(mouse ? { pointer: 'mouse' } : {}),
            };
          }
          if (!contact) throw new Error('no contact is down');
          if (kind === 'move') {
            const to = { x: opts.x ?? contact.x + (opts.dx ?? 0), y: opts.y ?? contact.y + (opts.dy ?? 0) };
            const ms = Math.max(0, opts.ms ?? 0);
            const steps = Math.max(1, Math.round(ms / 16));
            for (let i = 1; i <= steps; i++) {
              const t = i / steps;
              contact.t += (ms || 16) / steps / 1000;
              const at = { x: contact.x + (to.x - contact.x) * t, y: contact.y + (to.y - contact.y) * t };
              if (mouse) await button('mouseMoved', at, contact.t, 1);
              else await finger('touchMove', [at], contact.t);
              if (ms) await sleep(ms / steps);
            }
            contact = { ...to, t: contact.t, mouse, modifiers: held };
            await frame();
            return { phase: 'move', at: [to.x, to.y], delivery: 'platform' };
          }
          if (kind === 'hold') {
            if (opts.ms) {
              if (!opts.virtual) await sleep(opts.ms);
              contact.t += opts.ms / 1000;
            }
            return { phase: 'hold', at: [contact.x, contact.y], delivery: 'platform' };
          }
          // A mouse has no cancel: its button comes up where it is.
          if (mouse) await button('mouseReleased', contact, contact.t + 0.008, 0);
          else await finger(kind === 'up' ? 'touchEnd' : 'touchCancel', [], contact.t + 0.008);
          const at = [contact.x, contact.y];
          contact = null;
          await frame();
          const note = mouse ? null : await evaluate(`(${pageContact})('finish')`);
          return { phase: kind, at, delivery: 'platform', ...(note ? { note } : {}) };
        }
        if (kind === 'pinch') {
          // two fingers spread from d to d·scale about the middle (LLP 1057.001 §5)
          const [cx, cy] = opts.at ? [r.x + opts.at[0], r.y + opts.at[1]] : [x, y],
            d = Math.max(8, Math.min(r.w, r.h) * 0.3),
            fingers = k => [0, 1].map(i => ({ x: cx + ((i ? 1 : -1) * d * k) / 2, y: cy, id: i }));
          await call('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: fingers(1) });
          for (let i = 1; i <= 8; i++) {
            await call('Input.dispatchTouchEvent', {
              type: 'touchMove',
              touchPoints: fingers(1 + ((opts.pinch - 1) * i) / 8),
            });
            await sleep(16);
          }
          await call('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
          await frame();
          return { pinch: opts.pinch, at: [cx, cy], delivery: 'platform' };
        }
        if (kind === 'wheel') await cdpWheel(call, frame, { ...opts, x, y, modifiers });
        else if (kind === 'drop' || kind === 'dragenter' || kind === 'dragleave')
          // Chrome's own drag (agent-drag.mjs)
          dragged = await fileDrag({ kind, opts, id, x, y, dragged, call, evaluate });
        else if (kind === 'hover') await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
        else if (['contextmenu', 'dblclick', 'mouse', 'auxclick', 'clicks'].includes(kind)) {
          await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, modifiers });
          // The right button, the middle (`auxclick`), or the left n times, each press counting the run so far.
          const button = kind === 'contextmenu' ? 'right' : kind === 'auxclick' ? 'middle' : 'left';
          for (
            let clickCount = 1;
            clickCount <= (kind === 'dblclick' ? 2 : kind === 'clicks' ? opts.clicks : 1);
            clickCount++
          ) {
            // `buttons` is what PointerEvent.buttons reports; CDP leaves it 0 unless told.
            await call('Input.dispatchMouseEvent', {
              type: 'mousePressed',
              x,
              y,
              button,
              buttons: { left: 1, right: 2, middle: 4 }[button],
              clickCount,
              modifiers,
            });
            await call('Input.dispatchMouseEvent', {
              type: 'mouseReleased',
              x,
              y,
              button,
              buttons: 0,
              clickCount,
              modifiers,
            });
          }
        } else if (kind === 'clipboard')
          await deliverClipboard({
            id,
            opts: {
              ...opts,
              ...(opts.files || opts.image
                ? {
                    pasted: opts.image
                      ? [{ name: 'image.png', bytes: readFileSync(opts.image).toString('base64') }]
                      : driverFiles(opts.files),
                  }
                : {}),
            },
            evaluate,
            call,
            ask,
          });
        else if (kind === 'key') {
          const f = await ask({ op: 'focus', id, select: false });
          if (f.error) throw new Error(f.error);
          // A chord's modifiers go down before its key and up after, as a keyboard's
          // do (`modifierEdges`); Enter types a newline in a textarea.
          const { key, code, vk, modifiers, text, location } = cdpKey(opts.key);
          await withChordModifiers(call, chordModifiers(opts.key), async () => {
            await call('Input.dispatchKeyEvent', {
              type: 'keyDown',
              key,
              code,
              windowsVirtualKeyCode: vk,
              modifiers,
              location,
              ...(text ? { text } : {}),
            });
            await call('Input.dispatchKeyEvent', {
              type: 'keyUp',
              key,
              code,
              windowsVirtualKeyCode: vk,
              modifiers,
              location,
            });
          });
        } else if (
          kind === 'press' &&
          (await evaluate(
            `(() => { const hit = document.elementFromPoint(${x}, ${y}), host = hit?.closest('[data-gpu-input]'), el = ${id == null ? 'null' : `exact.views.get(${id})`}; return !!host && (hit === host || hit.localName === 'canvas') && (!el || el === host || el.contains(host) || host.contains(el)); })()`,
          ))
        ) {
          // A tap on a world's canvas is a finger, as a held contact is here and
          // every tap is on iOS and Linux, so a proof leaves one world everywhere.
          await call('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
          await call('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
        } else if (kind === 'press') {
          // The page can move between the aim above and the press (a reply lands on real time while the
          // agent's clock stands still): record what the press reached and refuse a tap that missed.
          // Judged at the press itself: the tap's own action may replace the node right after.
          // A fresh listener each tap, removed in `finally`, so a failed dispatch leaves none behind.
          const arm = `(() => { if (window.__exactPress) removeEventListener('pointerdown', window.__exactPress, true); window.__exactPressed = null; window.__exactPress = (e) => { if (window.__exactPressed) return; const view = exact.views.get(${id}), back = exact.agentInput?.backButton(view), el = back ?? view, hit = back ? e.composedPath()[0] : e.target; window.__exactPressed = !el ? 'nothing: the target was gone at the press' : el === hit || el.contains(hit) || hit.contains(el) && getComputedStyle(el).pointerEvents === 'none' ? 'ok' : hit.closest?.('[data-testid]') ? '\"' + hit.closest('[data-testid]').dataset.testid + '\"' : hit.closest?.('[data-view]') ? 'node #' + hit.closest('[data-view]').dataset.view : hit.localName; }; addEventListener('pointerdown', window.__exactPress, true); })()`;
          const disarm = `(() => { removeEventListener('pointerdown', window.__exactPress, true); window.__exactPress = null; const r = window.__exactPressed; window.__exactPressed = null; return r; })()`;
          if (id != null) await evaluate(arm);
          let reached = 'ok';
          try {
            await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y, modifiers });
            await call('Input.dispatchMouseEvent', {
              type: 'mousePressed',
              x,
              y,
              button: 'left',
              buttons: 1,
              clickCount: 1,
              modifiers,
            });
            await call('Input.dispatchMouseEvent', {
              type: 'mouseReleased',
              x,
              y,
              button: 'left',
              buttons: 0,
              clickCount: 1,
              modifiers,
            });
          } finally {
            if (id != null) reached = await evaluate(disarm).catch(() => 'ok');
          }
          // null: armed but never pressed — the press reached no element of this page (an iframe moved over
          // the target, say); undefined: the press loaded a new document, which is the tap's own doing.
          const missed = reached === 'ok' || reached === undefined ? null : (reached ?? 'no element of this page');
          if (missed)
            throw new Error(
              `tap #${id} at (${x}, ${y}) landed on ${missed}: the page moved between aiming and the press; \`clock settle\` first, then tap again`,
            );
        } else if (kind === 'type') {
          const f = await ask({ op: 'focus', id });
          if (f.error) throw new Error(f.error);
          await call('Input.insertText', { text: opts.text });
        }
        await frame();
        return { at: [x, y], ...(avoided ? { avoided } : {}) };
      },
      async screenshot(path, window = false, film = false) {
        // A picture holds transitions at their end; a film follows the clock (LLP 1116.003 D8).
        // The wasm target has no `picture`.
        const readPicture = () =>
          ask({ op: film ? 'tags' : 'picture' }).then(r =>
            r.error === 'unknown op: picture' && !film ? ask({ op: 'tags' }) : r,
          );
        const tags = await readPicture();
        if (tags.error) throw new Error(tags.error);
        delete tags.moving;
        await frame();
        // Images in the viewport finish loading first, up to 3 s: a picture
        // of grey placeholders is not the page (LLP 1054.000 R9). `clock
        // settle` stays the app's clock alone; this wait is the picture's.
        const pending = await waitAtMost(
          evaluate(`(async () => {
          const on = (i) => { const b = i.getBoundingClientRect(); return b.bottom > 0 && b.right > 0 && b.top < innerHeight && b.left < innerWidth; };
          const left = () => [...document.images].filter((i) => !i.complete && on(i));
          const end = performance.now() + 3000;
          while (left().length && performance.now() < end) await new Promise((r) => setTimeout(r, 25)); await globalThis.exact.imageFrames?.();
          return left().length;
        })()`),
          3500,
        );
        await frame();
        // Frame and image callbacks can start GPU work after the first read.
        const settled = await readPicture();
        if (settled.error) throw new Error(settled.error);
        const { data } = await call('Page.captureScreenshot', { format: 'png' });
        writeFileSync(path, Buffer.from(data, 'base64'));
        return { ...tags, screenshot: path, w: size[0], h: size[1], ...(pending ? { imagesPending: pending } : {}) };
      },
      close,
    };
  } catch (e) {
    try {
      await close();
    } catch (failure) {
      retainCleanupError(e, failure);
    }
    throw e;
  }
}
