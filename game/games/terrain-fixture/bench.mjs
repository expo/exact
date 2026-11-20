#!/usr/bin/env bun
import { cpus, loadavg } from 'node:os';
import { resolve } from 'node:path';

const resolution = [1280, 720];
const defaults = {
  radius: 'smoke', move: 'stand', light: '0', edits: '0', inflight: 'discard',
  compare: 'off', vertex: '12', blend: 'off', budget: 'deadline', draw: 'on', pool: 'off', jobs: 'off',
};
const choices = {
  radius: ['smoke', '8', '40'], move: ['stand', 'walk', 'fly'], inflight: ['discard', 'keep'],
  compare: ['off', 'on'], vertex: ['12', '64'], blend: ['off', 'window', 'background', 'surface'],
  draw: ['on', 'off'], pool: ['off', 'on'], jobs: ['off', 'on'],
};
const whole = number => /^\d+$/.test(number) && Number(number) <= 0xffffffff;
const valid = (key, value) => choices[key]?.includes(value) ??
  (key === 'budget'
    ? value === 'deadline' || (value?.startsWith('count:') && whole(value.slice(6)) && Number(value.slice(6)) > 0)
    : whole(value));

export function parseOptions([host, ...args], headless = process.env.BENCH_HEADLESS === '1') {
  if (!['web', 'macos', 'ios'].includes(host))
    throw new Error('Specify web (Chrome WebGPU), macos (Metal) or ios (a paired device, with --phone=…).');
  if (headless && host !== 'web')
    throw new Error('BENCH_HEADLESS=1 is available for web only.');
  const timings = { '--warmup-ms': 20000, '--window-ms': 60000, '--timeout-ms': 60000, '--poll-ms': 50 };
  const switches = { ...defaults };
  const set = new Map();
  let pair = null, device = null, os = null, refreshHz = null, phone = null;
  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg === '--pair' && args[index + 1] === 'draw' && pair == null) {
      pair = args[++index];
      continue;
    }
    const at = arg.indexOf('=');
    if (at < 0) throw new Error('Invalid bench option ' + arg);
    const key = arg.slice(0, at), value = arg.slice(at + 1);
    if (Object.hasOwn(timings, key) && whole(value) && (Number(value) > 0 || key === '--warmup-ms')) {
      timings[key] = Number(value);
    } else if (Object.hasOwn(switches, key) && valid(key, value)) {
      set.set(key, value);
      switches[key] = value;
    } else if (key === '--device' && value.trim()) {
      device = value;
    } else if (key === '--os' && value.trim()) {
      os = value;
    } else if (key === '--phone' && value.trim() && host === 'ios') {
      phone = value;
    } else if (key === '--refresh-hz' && /^\d+(\.\d+)?$/.test(value) && Number.isFinite(Number(value)) && Number(value) > 0) {
      refreshHz = Number(value);
    } else {
      throw new Error('Invalid bench option ' + arg);
    }
  }
  if (!headless && (!device || !os || refreshHz == null))
    throw new Error('A display row requires --device=…, --os=… and --refresh-hz=….');
  if (host === 'ios' && !phone) throw new Error('An ios row requires --phone=<device name or identifier>.');
  const { '--warmup-ms': warmupMs, '--window-ms': windowMs, '--timeout-ms': timeoutMs, '--poll-ms': pollMs } = timings;
  return { host, switches, set, warmupMs, windowMs, timeoutMs, pollMs, pair, headless, device, os, refreshHz, phone };
}

function game(world) {
  return world?.renderHooks?.game ?? null;
}

function distribution(values) {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const at = fraction => sorted[Math.ceil(sorted.length * fraction) - 1];
  return { count: sorted.length, p50: at(0.5), p95: at(0.95), max: sorted.at(-1) };
}

function change(before, end, key) {
  return Number.isFinite(before?.[key]) && Number.isFinite(end?.[key]) && end[key] >= before[key]
    ? end[key] - before[key] : null;
}

function refillWindow(before, after) {
  if (!before?.refills || !after?.refills) return null;
  const { finishedUs, unfinished, unfinishedCount, oldestUnfinishedMs } = after.refills;
  const finished = change(before.refills, after.refills, 'finishedCount');
  const samplesMissing = finished > finishedUs.length;
  return {
    started: change(before.refills, after.refills, 'started'),
    finished,
    unfinished: unfinishedCount,
    finishedMs: finished && !samplesMissing ? distribution(finishedUs.slice(-finished).map(us => us / 1000)) : null,
    ...(samplesMissing ? { samplesMissing: finished - finishedUs.length } : {}),
    oldestUnfinishedMs,
    unfinishedCrossings: unfinished,
    crossingIntervalMs: after.crossingIntervalMs,
    backlogStart: before.pending,
    backlogEnd: after.pending,
  };
}

function reading(before, end, durationMs, browser, refreshHz) {
  const start = game(before), finish = game(end);
  const perf = end?.perf;
  const hooks = end?.renderHooks;
  const raf = distribution(browser?.rafIntervals ?? []);
  const frameP50 = perf?.frameMs?.p50;
  const bufferKeys = ['created', 'reused', 'destroyed', 'bytesWritten'];
  return {
    durationMs, clock: perf?.clock ?? null,
    tickMs: perf?.tickMs ?? null, prepareMs: hooks?.cpuMs?.prepare ?? null,
    encodeMs: perf?.encodeMs ?? null, frameMs: perf?.frameMs ?? null,
    rafMs: browser ? { p50: raf?.p50 ?? null, p95: raf?.p95 ?? null, count: raf?.count ?? 0 } : null,
    longTasks: browser?.longTasks ?? null,
    displayPeriodsPerFrame: refreshHz != null && Number.isFinite(frameP50) ? frameP50 / (1000 / refreshHz) : null,
    drawMs: hooks ? { opaque: hooks.cpuMs?.opaque, background: hooks.cpuMs?.background, surface: hooks.cpuMs?.surface } : null,
    budget: hooks?.budget ?? null, jobs: hooks?.jobs ?? null, catchUp: perf?.catchUp ?? null,
    uploadBytes: hooks?.frame?.uploadBytes ?? null, draws: hooks?.frame?.draws ?? null,
    itemsDrawn: hooks?.frame?.itemsDrawn ?? null,
    gpuMs: end?.gpuMs ?? null, timestampQueries: finish?.timestampQueries ?? null,
    thermalStateStart: start?.thermalState ?? null, thermalStateEnd: finish?.thermalState ?? null,
    thermalChanges: change(start, finish, 'thermalChanges'),
    work: { operationsCompleted: change(start, finish, 'completed'), uploadBytes: change(start, finish, 'uploadBytes') },
    sectionBuffers: finish?.sectionBuffers ? Object.fromEntries(bufferKeys.map(key =>
      [key, change(start?.sectionBuffers, finish.sectionBuffers, key)])) : null,
    refill: refillWindow(start, finish),
  };
}

function inconclusive(windows, headless) {
  const conditions = [];
  if (headless) conditions.push({ quantity: 'row', condition: 'headless' });
  for (const [index, window] of windows.entries()) {
    if (window.thermalChanges > 0) conditions.push({ quantity: 'thermalState', condition: 'changed', window: index + 1 });
    if (window.refill?.samplesMissing)
      conditions.push({ quantity: 'refill.finishedMs', condition: 'finished sample limit exceeded', window: index + 1 });
    if (window.timestampQueries === false)
      conditions.push({ quantity: 'gpuMs', condition: 'timestamp queries unavailable', window: index + 1 });
  }
  const quantities = new Map([
    ...['tickMs', 'prepareMs', 'encodeMs', 'frameMs', 'uploadBytes', 'draws', 'itemsDrawn']
      .map(key => [key, w => w[key]?.p95]),
    ['refill.finishedMs', w => w.refill?.finishedMs?.p95],
  ]);
  for (const window of windows) {
    for (const key of Object.keys(window.gpuMs ?? {}))
      quantities.set('gpuMs.' + key, w => w.timestampQueries ? w.gpuMs?.[key]?.p95 : null);
  }
  for (const [quantity, measure] of quantities) {
    const values = windows.map(measure);
    if (values.every(Number.isFinite) && Math.max(...values) > 1.1 * Math.min(...values))
      conditions.push({ quantity, statistic: 'p95', condition: 'windows differ by more than 10%' });
  }
  return conditions;
}

export async function readBench(session, config, commit) {
  const { host, switches, set, warmupMs, windowMs, timeoutMs, pollMs, headless, device, os, refreshHz } = config;
  for (const [name, value] of set) await session.set(name, value);

  let observedSwitches = null;
  const state = async (reset = false) => {
    const world = await session.state(reset);
    if (world?.args && observedSwitches == null) {
      for (const [name, expected] of Object.entries(switches)) {
        if (world.args[name] !== expected)
          throw new Error(`Canvas argument ${name} is ${world.args[name]}; expected ${expected}`);
      }
      observedSwitches = world.args;
    }
    return world;
  };

  const start = session.now();
  const limit = start + timeoutMs;
  const drawing = switches.draw === 'on';
  await session.play();
  let firstPicture, fullWindow, backlog;
  for (;;) {
    const world = await state();
    const observed = game(world);
    const ms = session.now() - start;
    backlog = observed?.pending ?? backlog;
    if (ms <= timeoutMs) {
      if (drawing && observed?.drew === true) firstPicture ??= { ms };
      if (world?.ready && observed?.pending === 0) fullWindow ??= { ms };
    }
    if (((!drawing || firstPicture) && fullWindow) || session.now() >= limit) break;
    await session.wait(Math.min(pollMs, limit - session.now()));
  }
  const timeout = { ms: null, timeoutMs, backlog: backlog ?? null };

  await session.wait(warmupMs);
  const windows = [];
  let last;
  for (let index = 0; index < 3; index++) {
    const before = await state(true);
    const windowStart = session.now();
    const finishBrowserWindow = host === 'web' ? await session.startBrowserWindow(windowMs) : null;
    await session.wait(windowMs);
    last = await state();
    const durationMs = session.now() - windowStart;
    const browser = await finishBrowserWindow?.();
    windows.push(reading(before, last, durationMs, browser, host === 'web' ? refreshHz : null));
  }
  const terrain = game(last);
  return {
    commit, host, switches, observedSwitches, profile: host === 'web' ? 'web' : 'host-dev',
    resolution, pixels: await session.pixels?.() ?? last?.perf?.pixels ?? null,
    device, os, refreshHz, headless, calibration: 'uncalibrated', warmupMs, windowMs,
    mountSampling: 'poll', pollMs, firstPicture: drawing ? firstPicture ?? timeout : null,
    fullWindow: fullWindow ?? timeout, paired: false,
    sectionsPending: terrain?.pending ?? null, operationsCompleted: terrain?.completed ?? null,
    discarded: terrain?.discarded ?? null, kept: terrain?.kept ?? null,
    gpuBufferBytes: terrain?.gpuBufferBytes ?? null, peakGpuBufferBytes: terrain?.peakGpuBufferBytes ?? null,
    sectionBuffers: terrain?.sectionBuffers ?? null,
    cpuBytes: terrain?.cpuBytes ?? null, peakCpuBytes: terrain?.peakCpuBytes ?? null,
    lightCells: terrain?.lightCells ?? null,
    lane: last?.renderHooks?.jobs?.lane ?? null,
    inconclusive: inconclusive(windows, headless), windows,
  };
}

const workMatches = (on, off) => Number.isFinite(on.operationsCompleted) && on.operationsCompleted === off.operationsCompleted
  && Number.isFinite(on.uploadBytes) && Number.isFinite(off.uploadBytes)
  && Math.abs(on.uploadBytes - off.uploadBytes) <= 0.02 * Math.min(on.uploadBytes, off.uploadBytes);

function encodeDifference(on, off) {
  if (!on?.count || !off?.count) return null;
  return Object.fromEntries(['p50', 'p95', 'p99', 'max', 'mean'].map(key => [
    key, Number.isFinite(on[key]) && Number.isFinite(off[key]) ? on[key] - off[key] : null,
  ]));
}

export async function readDrawPair(read, config) {
  const withDraw = draw => ({ ...config, switches: { ...config.switches, draw },
    set: new Map([...config.set, ['draw', draw]]) });
  const on = await read(withDraw('on'));
  const off = await read(withDraw('off'));
  const windows = on.windows.map((window, index) => {
    const other = off.windows[index];
    return {
      on: window.work, off: other.work, matched: workMatches(window.work, other.work),
      encodeSamples: Boolean(window.encodeMs?.count && other.encodeMs?.count),
    };
  });
  const matched = windows.every(window => window.matched && window.encodeSamples);
  return {
    on, off,
    drawPair: {
      result: matched ? 'MATCHED' : 'INCONCLUSIVE',
      windows,
      encodeDifferenceMs: matched ? on.windows.map((window, index) =>
        encodeDifference(window.encodeMs, off.windows[index].encodeMs)) : null,
    },
  };
}

export function tableRow(row) {
  const show = value => value ?? '—';
  const mount = value => value == null ? '—' : value.ms == null
    ? `timeout at ${value.timeoutMs} ms (${show(value.backlog)} pending)` : value.ms.toFixed(1);
  const perWindow = metric => row.windows.map(w => show(w[metric]?.p95)).join(' / ');
  const display = format => row.windows.some(w => format(w) != null)
    ? row.windows.map(w => show(format(w))).join(' / ') : '—';
  const fixed = value => value?.toFixed(2);
  const raf = display(w => w.rafMs && `${show(fixed(w.rafMs.p50))}/${show(fixed(w.rafMs.p95))}/${w.rafMs.count}`);
  const tasks = display(w => w.longTasks && `${w.longTasks.count}/${fixed(w.longTasks.durationMs)}`);
  const periods = display(w => fixed(w.displayPeriodsPerFrame));
  const switches = Object.entries(row.switches).map(([name, value]) => `${name}=${value}`).join(', ');
  const cells = [row.commit, row.host, row.profile, row.resolution.join('×'), show(row.device), show(row.os),
    show(row.refreshHz), switches, show(row.lane), row.pollMs, mount(row.firstPicture), mount(row.fullWindow),
    show(row.peakGpuBufferBytes), show(row.peakCpuBytes), perWindow('tickMs'), perWindow('prepareMs'),
    perWindow('encodeMs'), perWindow('frameMs'), raf, tasks, periods, row.calibration, row.headless ? 'headless' : 'display'];
  return `| ${cells.map(cell => String(cell).replaceAll('|', '\\|')).join(' | ')} |`;
}

async function run(config) {
  const [{ spawn }, { sleep }, { pollUntil }] = await Promise.all([
    import('node:child_process'), import('../../bench/exact.mjs'), import('../../../scripts/agent-launch.mjs'),
  ]);
  const app = import.meta.dir;
  const root = resolve(app, '../../..');
  const dist = resolve(app, 'target', 'bench-dist');
  const commit = (await Bun.$`git -C ${root} rev-parse HEAD`.quiet()).text().trim();
  const env = {
    ...process.env, EXACT_UPDATE_TRUST: 'development',
    EXACT_GAME_PROOF_PROFILE: config.host === 'web' ? 'release' : 'gpu-dev',
    EXACT_APP_DIR: app, EXACT_WEB_DIST: dist, EXACT_SCALE: '1',
  };
  const deviceFlags = config.host === 'ios' ? ['--device', '--phone', config.phone] : [];
  const build = spawn(process.execPath, [resolve(app, 'proof.mjs'), config.host, ...deviceFlags, '--build-only'],
    { cwd: root, env, stdio: ['ignore', 2, 2] });
  const status = await new Promise((done, fail) => {
    build.once('error', fail);
    build.once('exit', done);
  });
  // A build-only proof ends unverified, with status 3.
  if (status !== 3) throw new Error('Build exited ' + status);

  const read = async options => {
    const transport = config.host === 'web'
      ? await openWeb(dist, pollUntil, options)
      : await openNative(config.host, app, env, config.phone);
    // A loaded machine stretches the frames of a browser or an app it runs; a device's are its own.
    const loaded = () => config.host === 'ios' ? null : Math.round(loadavg()[0] * 10) / 10;
    const before = loaded();
    try {
      const row = await readBench({ ...transport, now: () => performance.now(), wait: sleep }, options, commit);
      row.browser = transport.browser ?? null;
      row.machineLoad = before == null ? null : { before, after: loaded(), cores: cpus().length };
      // With more runnable threads than cores, a frame waits for a core.
      if (row.machineLoad && Math.max(before, row.machineLoad.after) > row.machineLoad.cores)
        row.inconclusive.push({ quantity: 'row', condition: 'the machine had more load than cores' });
      return row;
    } finally {
      await transport.close();
    }
  };
  if (config.pair) {
    const pair = await readDrawPair(read, config);
    console.log(JSON.stringify(pair));
    console.log(tableRow(pair.on));
    console.log(tableRow(pair.off));
  } else {
    const row = await read(config);
    console.log(JSON.stringify(row));
    console.log(tableRow(row));
  }
}

async function openNative(host, app, env, phone) {
  const [{ open }, { displayReady, focus }] = await Promise.all([
    import('../../../scripts/agent-session.mjs'), import('../../bench/exact.mjs'),
  ]);
  const mac = host === 'macos';
  if (mac) displayReady();
  let pid, id, pixels;
  // A device draws at its own size; the Mac app takes the bench's.
  const session = await open({
    host, app, timing: 'platform', ...(mac ? { size: resolution } : { device: true, phone }),
    env: { ...env, EXACT_AGENT: 'live' }, onProcess: child => { pid = child.pid; },
  });
  try {
    if (mac && pid) focus(pid, true);
  } catch (error) {
    await session.close();
    throw error;
  }
  return {
    set: (name, value) => session.type(name, value),
    play: () => session.tap('play'),
    async state(reset) {
      id ??= (await session.find('world', false, false))?.id;
      if (id == null) return null;
      const reply = await session.op({ op: 'state', world: true, id, ...(reset ? { perf_reset: true } : {}) });
      pixels = reply.world?.perf?.pixels ?? pixels;
      return reply.world;
    },
    pixels: () => pixels ?? null,
    close: session.close,
  };
}

export async function waitForBenchPage(client, poll, timeoutMs, pollMs = 50) {
  let hasContext = false;
  const ready = await poll(async () => {
    try {
      if (!hasContext) {
        const { root } = await client.call('DOM.getDocument', { depth: 0 });
        if (!root?.documentURL || root.documentURL === 'about:blank') return false;
        const { object } = await client.call('DOM.resolveNode', { nodeId: root.nodeId, objectGroup: 'bench-start' });
        if (!object?.objectId) return false;
        await client.call('Runtime.releaseObjectGroup', { objectGroup: 'bench-start' });
        hasContext = true;
      }
      return await client.eval('!!document.querySelector(\'[data-testid="play"]\')');
    } catch (error) {
      if (!/execution context|find context|find node with given id|No document/i.test(error.message)) throw error;
      hasContext = false;
      return false;
    }
  }, Boolean, { bound: timeoutMs, interval: pollMs });
  if (!ready) throw new Error(`The terrain controls did not mount within ${timeoutMs} ms`);
}

export function sampleBrowserWindow(durationMs, page = globalThis) {
  const start = page.performance.now();
  const end = start + durationMs;
  const rafIntervals = [];
  let previous, frame;
  const sample = stamp => {
    if (stamp >= end) return;
    if (stamp >= start) {
      if (previous !== undefined) rafIntervals.push(stamp - previous);
      previous = stamp;
    }
    frame = page.requestAnimationFrame(sample);
  };
  frame = page.requestAnimationFrame(sample);

  const longTasks = page.PerformanceObserver?.supportedEntryTypes.includes('longtask')
    ? { count: 0, durationMs: 0 } : null;
  const record = entries => {
    for (const entry of entries) {
      if (entry.startTime >= start && entry.startTime < end) {
        longTasks.count++;
        longTasks.durationMs += entry.duration;
      }
    }
  };
  const observer = longTasks && new page.PerformanceObserver(list => record(list.getEntries()));
  observer?.observe({ type: 'longtask' });

  return new Promise(done => page.setTimeout(() => {
    page.cancelAnimationFrame(frame);
    if (observer) {
      record(observer.takeRecords());
      observer.disconnect();
    }
    done({ rafIntervals, longTasks });
  }, durationMs));
}

async function openWeb(dist, poll, { headless, timeoutMs, pollMs }) {
  const [{ spawn }, { mkdtemp, rm }, { tmpdir }, { extname }, { chromium }, { cdp, displayReady, stop, track }] =
    await Promise.all([import('node:child_process'), import('node:fs/promises'), import('node:os'),
      import('node:path'), import('../../../scripts/agent-launch.mjs'), import('../../bench/exact.mjs')]);
  // BENCH_HEADLESS=1 checks the script without a display: its row is never a reading.
  if (!headless) displayReady();
  const { executable, unavailable } = chromium({ needs: ['gpu', 'perf'] });
  if (unavailable) throw new Error(unavailable);
  const profile = await mkdtemp(resolve(tmpdir(), 'terrain-bench-'));
  const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm' };
  let server, client, child;
  const close = async () => {
    client?.close();
    server?.stop(true);
    await stop(child);
    await rm(profile, { recursive: true, force: true });
  };
  try {
    server = Bun.serve({
      hostname: '127.0.0.1', port: 0,
      async fetch(request) {
        const name = new URL(request.url).pathname;
        const path = resolve(dist, '.' + (name === '/' ? '/index.html' : decodeURIComponent(name)));
        const file = Bun.file(path);
        if (!path.startsWith(dist + '/') || !await file.exists()) return new Response(null, { status: 404 });
        return new Response(file, { headers: {
          'Content-Type': types[extname(path)] ?? 'application/octet-stream',
          'Cross-Origin-Opener-Policy': 'same-origin',
          'Cross-Origin-Embedder-Policy': 'require-corp',
        } });
      },
    });
    child = track(spawn(executable, ['--user-data-dir=' + profile, '--no-first-run', '--no-default-browser-check',
      '--remote-debugging-port=0', '--window-size=1280,720', '--enable-unsafe-webgpu',
      // Chrome for Testing turns on every experiment in Chromium's testing configuration. One of them
      // sends this page a main frame on one display frame in two to four while it uploads sections:
      // a 53 ms frame at 75 Hz that is 13 ms without it. A release browser starts with none of them.
      '--disable-field-trial-config',
      ...(headless ? ['--headless=new'] : []), server.url.href],
    { stdio: 'ignore' }));
    let launchError;
    child.once('error', error => { launchError = error; });
    const page = await poll(async () => {
      if (launchError) throw launchError;
      if (child.exitCode != null) throw new Error('Chrome exited ' + child.exitCode);
      const file = await Bun.file(resolve(profile, 'DevToolsActivePort')).text().catch(() => null);
      if (file) {
        const tabs = await fetch('http://127.0.0.1:' + file.split('\n')[0] + '/json/list').then(r => r.json()).catch(() => []);
        return tabs.find(tab => tab.type === 'page' && tab.url.startsWith(server.url.origin));
      }
    }, Boolean, { bound: timeoutMs, interval: pollMs });
    if (!page) throw new Error(`Chrome did not open the bench page within ${timeoutMs} ms`);
    client = await cdp(page.webSocketDebuggerUrl);
    const { product } = await client.call('Browser.getVersion');
    await client.call('Page.bringToFront');
    await client.call('Emulation.setDeviceMetricsOverride', { width: resolution[0], height: resolution[1], deviceScaleFactor: 1, mobile: false });
    const evaluate = (fn, ...args) => client.eval(`(${fn})(${args.map(value => JSON.stringify(value)).join(',')})`);
    await waitForBenchPage(client, poll, timeoutMs, pollMs);
    return {
      browser: product,
      async set(name, value) {
        const set = await evaluate((key, text) => {
          const input = document.querySelector('[data-testid="' + key + '"]');
          if (!input) return false;
          input.value = text;
          input.dispatchEvent(new Event('input', { bubbles: true }));
          // The view takes a typed value at its next commit: play before it and the canvas mounts with the old one.
          return new Promise(taken => requestAnimationFrame(() => requestAnimationFrame(() => taken(true))));
        }, name, value);
        if (!set) throw new Error('Could not set ' + name);
      },
      async play() { await evaluate(() => document.querySelector('[data-testid="play"]').click()); },
      async startBrowserWindow(durationMs) {
        // Start the page's timer without holding a CDP request open for the full window.
        const { result, exceptionDetails } = await client.call('Runtime.evaluate', {
          expression: `(${sampleBrowserWindow})(${durationMs})`,
        });
        if (exceptionDetails || !result?.objectId)
          throw new Error('Could not start browser timing: ' + JSON.stringify(exceptionDetails ?? result));
        const promiseObjectId = result.objectId;
        return async () => {
          try {
            const response = await client.call('Runtime.awaitPromise', { promiseObjectId, returnByValue: true });
            if (response.exceptionDetails)
              throw new Error('Could not read browser timing: ' + JSON.stringify(response.exceptionDetails));
            return response.result.value;
          } finally {
            await client.call('Runtime.releaseObject', { objectId: promiseObjectId });
          }
        };
      },
      state: reset => evaluate(arm => {
        const canvas = document.querySelector('[data-testid="world"]');
        if (!canvas) return null;
        return globalThis.exact?.gpu?.agent(Number(canvas.dataset.view),
          { op: 'state', ...(arm ? { perf_reset: true } : {}) })?.world ?? null;
      }, reset),
      pixels: () => evaluate(() => {
        const canvas = document.querySelector('canvas');
        return canvas ? [canvas.width, canvas.height] : null;
      }),
      close,
    };
  } catch (error) {
    await close();
    throw error;
  }
}

if (import.meta.main) {
  if (process.argv.includes('--help')) {
    console.log('bun game/games/terrain-fixture/bench.mjs web|macos|ios --device=… --os=… --refresh-hz=… [--phone=…] [switch=value ...] [--pair draw] [--warmup-ms=20000] [--window-ms=60000] [--timeout-ms=60000] [--poll-ms=50]');
    console.log('BENCH_HEADLESS=1 allows a web sanity run without display metadata.');
  } else {
    await run(parseOptions(process.argv.slice(2))).catch(error => {
      console.error(error.message);
      process.exit(1);
    });
  }
}
