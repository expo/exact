import { expect, test } from 'bun:test';
import { parseOptions, readBench, readDrawPair, sampleBrowserWindow, tableRow, waitForBenchPage } from './bench.mjs';

const ring = (p95, count = 7, p50 = p95, mean = p95) => ({
  count, p50, p95, p99: p95, max: p95 ?? 0, mean: mean ?? 0,
});

// Terrain smoke proof state at tick 36 after perf_reset.
const recorded = {
  tick: 36, device: true, ready: true, readyReasons: [],
  quality: { rung: 'high', samples: 1, scale: 1 },
  perf: {
    clock: 'agent', pixels: [1280, 720],
    tickMs: ring(0.19999999925494194, 1),
    encodeMs: ring(0.6000000014901161, 7, 0.19999999925494194, 0.2714285712156977),
    frameMs: ring(16.7),
    catchUp: { overloadFrames: 0, clampFrames: 0, clampMs: 0, idleFrames: 0, idleMs: 0 },
  },
  renderHooks: {
    cpuMs: {
      prepare: ring(0),
      opaque: ring(0.19999999925494194, 7, 0.10000000149011612, 0.1428571417927742),
      background: ring(0.09999999776482582, 7, 0, 0.014285713966403688),
      surface: ring(null, 0),
    },
    budget: { allowance: 64, overFrames: 0, overMs: 0 },
    frame: { items: 'sections', uploadBytes: ring(0), draws: ring(823), itemsDrawn: ring(823) },
  },
  gpuMs: { 'forward + sky': ring(0.10474399999999999, 1) },
};

// The device-free workload state read after two overlapping crossings.
const recordedGame = {
  tick: 75, pending: 271, completed: 2449, kept: 0, drew: false, lightCells: 0, uploadBytes: 0,
  discarded: { stale: 0, stagedLeft: 0, queuedLeft: 0 },
  gpuBufferBytes: 0, peakGpuBufferBytes: 0, cpuBytes: 0, peakCpuBytes: 0,
  crossingIntervalMs: 800, timestampQueries: false, thermalState: null, thermalChanges: 0,
  refills: {
    started: 2, finishedCount: 0, finishedUs: [], unfinishedCount: 2, oldestUnfinishedMs: 28,
    unfinished: [
      { id: 1, ageMs: 28, pending: 135, dropped: 0 },
      { id: 2, ageMs: 16, pending: 136, dropped: 0 },
    ],
  },
};

const defaultArgs = {
  radius: 'smoke', move: 'stand', light: '0', edits: '0', inflight: 'discard',
  compare: 'off', vertex: '12', blend: 'off', budget: 'deadline', draw: 'on', pool: 'off', jobs: 'off',
  quality: 'high', fps: '0',
};

function double({
  first = 1, full = 2, intervals, timestamps = true, thermal = false, extraUnits = 0,
  extraUploads = 0, encodeMs = recorded.perf.encodeMs, ignoredSwitch, finishedCount = 1, bindsPerMs,
} = {}) {
  let elapsed = 0, window = 0;
  const calls = [];
  return {
    calls,
    now: () => elapsed,
    async wait(ms) { elapsed += ms; },
    async set(name, value) { calls.push([name, value]); },
    async play() { calls.push('play'); },
    async startBrowserWindow() { return async () => null; },
    async state(reset = false) {
      if (reset) window++;
      const pending = elapsed < full ? recordedGame.pending : 0;
      const afterFirstWindow = elapsed >= 12;
      return {
        ...recorded, ready: pending === 0,
        args: { ...defaultArgs, ...Object.fromEntries(calls.filter(call => Array.isArray(call) && call[0] !== ignoredSwitch)) },
        perf: { ...recorded.perf, encodeMs, ...(intervals ? { frameMs: ring(intervals[Math.max(0, window - 1)]) } : {}) },
        renderHooks: {
          ...recorded.renderHooks,
          game: {
            ...recordedGame,
            pending, drew: elapsed >= first, completed: recordedGame.completed + (7 + extraUnits) * elapsed,
            uploadBytes: (1024 + extraUploads) * elapsed,
            bindsTotal: bindsPerMs == null ? null : bindsPerMs * elapsed,
            timestampQueries: timestamps,
            thermalState: 'nominal', thermalChanges: thermal && afterFirstWindow ? 2 : 0,
            refills: afterFirstWindow
              ? { started: recordedGame.refills.started, finishedCount,
                finishedUs: Array(Math.min(128, finishedCount)).fill(28000),
                unfinishedCount: 1, oldestUnfinishedMs: elapsed + 4,
                unfinished: [{ ...recordedGame.refills.unfinished[1], ageMs: elapsed + 4 }] }
              : { started: 1, finishedCount: 0, finishedUs: [], unfinishedCount: 1,
                oldestUnfinishedMs: 28, unfinished: [recordedGame.refills.unfinished[0]] },
          },
        },
      };
    },
  };
}

const options = (...args) => parseOptions(['web', '--device=Mac Studio', '--os=macOS 27.2.1',
  '--refresh-hz=60', '--warmup-ms=0', '--window-ms=10', '--timeout-ms=8', '--poll-ms=1', ...args], false);

test('waits for the page context before its first evaluate and bounds the wait', async () => {
  const probes = [];
  const client = {
    async call(method) {
      probes.push(method);
      if (method === 'DOM.getDocument')
        return { root: { nodeId: 1, documentURL: 'http://127.0.0.1/' } };
      if (probes.length < 4) throw new Error('Cannot find default execution context');
      return { object: { objectId: 'document' } };
    },
    async eval() {
      if (probes.length < 4) throw new Error('Cannot find default execution context');
      probes.push('evaluate');
      return true;
    },
  };
  const poll = async (read, ready, { bound } = {}) => {
    let attempt = 0, value;
    do {
      value = await read();
    } while (!ready(value) && ++attempt < (bound ?? 0));
    expect(bound).toBe(2);
    return value;
  };
  await waitForBenchPage(client, poll, 2);
  expect(probes).toEqual(['DOM.getDocument', 'DOM.resolveNode', 'DOM.getDocument', 'DOM.resolveNode',
    'Runtime.releaseObjectGroup', 'evaluate']);
  await expect(waitForBenchPage({ ...client, call: async () => ({ root: { documentURL: 'about:blank' } }) }, poll, 2))
    .rejects.toThrow('The terrain controls did not mount within 2 ms');
});

test('records the state rings and refill observations across three windows', async () => {
  const host = double();
  const row = await readBench(host, options('radius=40', 'move=fly', 'light=5', 'budget=deadline'), 'abc');
  expect(host.calls).toEqual([['radius', '40'], ['move', 'fly'], ['light', '5'], ['budget', 'deadline'], 'play']);
  expect(row).toMatchObject({
    commit: 'abc', host: 'web', profile: 'web', resolution: [1280, 720], pixels: [1280, 720], quality: 'high',
    device: 'Mac Studio', os: 'macOS 27.2.1', refreshHz: 60, calibration: 'uncalibrated',
    switches: { radius: '40', move: 'fly', light: '5', budget: 'deadline', draw: 'on' },
    firstPicture: { ms: 1 }, fullWindow: { ms: 2 }, mountSampling: 'poll', pollMs: 1, paired: false,
    observedSwitches: { radius: '40', move: 'fly', light: '5', budget: 'deadline', draw: 'on' },
    discarded: recordedGame.discarded, kept: recordedGame.kept, lightCells: recordedGame.lightCells,
    gpuBufferBytes: recordedGame.gpuBufferBytes, peakGpuBufferBytes: recordedGame.peakGpuBufferBytes,
    cpuBytes: recordedGame.cpuBytes, peakCpuBytes: recordedGame.peakCpuBytes,
    inconclusive: [],
  });
  expect(row.windows).toHaveLength(3);
  expect(row.windows[0]).toMatchObject({
    tickMs: recorded.perf.tickMs, prepareMs: recorded.renderHooks.cpuMs.prepare,
    encodeMs: recorded.perf.encodeMs, frameMs: recorded.perf.frameMs, catchUp: recorded.perf.catchUp,
    budget: recorded.renderHooks.budget, uploadBytes: recorded.renderHooks.frame.uploadBytes,
    itemsDrawn: recorded.renderHooks.frame.itemsDrawn, gpuMs: recorded.gpuMs, timestampQueries: true,
    work: { operationsCompleted: 70, uploadBytes: 10240 },
    refill: {
      started: 1, finished: 1, unfinished: 1, finishedMs: { count: 1, p50: 28, p95: 28, max: 28 },
      oldestUnfinishedMs: 16, crossingIntervalMs: 800, backlogStart: 0, backlogEnd: 0,
      unfinishedCrossings: [{ id: 2, ageMs: 16, pending: 136, dropped: 0 }],
    },
  });
  expect(row.windows[1].refill).toMatchObject({ started: 0, finished: 0, unfinished: 1, oldestUnfinishedMs: 26 });
  expect(tableRow(row)).toContain('| Mac Studio | macOS 27.2.1 | 60 |');
  expect(row).not.toHaveProperty('status');
});

test('mount timeouts carry their backlog and draw off has no first picture', async () => {
  const row = await readBench(double({ first: Infinity, full: Infinity }), options(), 'abc');
  expect(row.firstPicture).toEqual({ ms: null, timeoutMs: 8, backlog: recordedGame.pending });
  expect(row.fullWindow).toEqual(row.firstPicture);
  expect(row.inconclusive).toEqual([]);
  const off = await readBench(double(), options('draw=off'), 'abc');
  expect(off.firstPicture).toBeNull();
  expect(off.fullWindow).toEqual({ ms: 2 });
});

test('arena and indirect rows wait for a first picture and report mean binds beside draws', async () => {
  for (const [draw, bindsPerMs, mean] of [['arena', 14, 20], ['indirect', 63, 90]]) {
    const host = double({ first: 3, bindsPerMs });
    const row = await readBench(host, options('draw=' + draw), 'abc');
    expect(host.calls).toEqual([['draw', draw], 'play']);
    expect(row.observedSwitches.draw).toBe(draw);
    expect(row.firstPicture).toEqual({ ms: 3 });
    expect(row.inconclusive).toEqual([]);
    expect(tableRow(row)).toContain('draw=' + draw);
    for (const window of row.windows) {
      expect(window.binds).toBe(mean);
      expect(window.draws).toEqual(recorded.renderHooks.frame.draws);
    }
  }
});

test('reports browser cadence, long tasks and display periods for each web window', async () => {
  const host = double({ intervals: [50, 25, 50] });
  const sampled = [];
  host.startBrowserWindow = async duration => {
    const start = host.now();
    const index = sampled.push(duration);
    return async () => {
      expect(host.now() - start).toBe(duration);
      return { rafIntervals: [16 * index, 32 * index],
        longTasks: { count: index - 1, durationMs: 50 * (index - 1) } };
    };
  };
  const row = await readBench(host, options('--refresh-hz=80'), 'abc');
  expect(sampled).toEqual([10, 10, 10]);
  expect(row.windows.map(({ rafMs, longTasks, displayPeriodsPerFrame }) =>
    [rafMs, longTasks, displayPeriodsPerFrame])).toEqual([
    [{ p50: 16, p95: 32, count: 2 }, { count: 0, durationMs: 0 }, 4],
    [{ p50: 32, p95: 64, count: 2 }, { count: 1, durationMs: 50 }, 2],
    [{ p50: 48, p95: 96, count: 2 }, { count: 2, durationMs: 100 }, 4],
  ]);
  expect(tableRow(row)).toContain('| 16.00/32.00/2 / 32.00/64.00/2 / 48.00/96.00/2 | 0/0.00 / 1/50.00 / 2/100.00 | 4.00 / 2.00 / 4.00 |');
  const mac = await readBench(host, { ...options(), host: 'macos' }, 'abc');
  expect(sampled).toHaveLength(3);
  expect(mac.windows.map(({ rafMs, longTasks, displayPeriodsPerFrame }) =>
    [rafMs, longTasks, displayPeriodsPerFrame])).toEqual(Array(3).fill([null, null, null]));
  expect(tableRow(mac)).toContain('| — | — | — | uncalibrated |');
});

test('samples page callbacks within the window and drains pending long tasks', async () => {
  let animate, notify, end;
  const calls = [];
  const task = (startTime, duration) => ({ startTime, duration });
  const pending = [task(180, 90), task(200, 60)];
  class Observer {
    static supportedEntryTypes = ['longtask'];
    constructor(callback) { notify = callback; }
    observe(options) { expect(options).toEqual({ type: 'longtask' }); }
    takeRecords() { return pending; }
    disconnect() { calls.push('disconnect'); }
  }
  const page = {
    performance: { now: () => 100 },
    requestAnimationFrame(callback) { animate = callback; return 5; },
    cancelAnimationFrame(id) { calls.push(id); },
    setTimeout(callback, ms) { expect(ms).toBe(100); end = callback; },
    PerformanceObserver: Observer,
  };
  const sample = sampleBrowserWindow(100, page);
  for (const stamp of [99, 110, 125, 155, 210]) animate(stamp);
  notify({ getEntries: () => [task(99, 70), task(111, 50)] });
  end();
  expect(await sample).toEqual({ rafIntervals: [15, 30], longTasks: { count: 2, durationMs: 140 } });
  expect(calls).toEqual([5, 'disconnect']);

  const unsupported = sampleBrowserWindow(100, { ...page, PerformanceObserver: undefined });
  end();
  expect(await unsupported).toEqual({ rafIntervals: [], longTasks: null });
});

test('passes the pool switch through to the canvas and reports buffer work', async () => {
  expect(options().switches.pool).toBe('off');
  expect(() => options('pool=yes')).toThrow('pool');
  const host = double();
  const state = host.state;
  host.state = async reset => {
    const world = await state(reset);
    const units = host.now();
    world.renderHooks.game.sectionBuffers = {
      created: 9, reused: units, destroyed: 0, bytesWritten: units * 60,
    };
    return world;
  };
  const row = await readBench(host, options('pool=on'), 'abc');
  expect(row.observedSwitches.pool).toBe('on');
  expect(row.sectionBuffers).toEqual({ created: 9, reused: 32, destroyed: 0, bytesWritten: 1920 });
  expect(row.windows[0].sectionBuffers).toEqual({ created: 0, reused: 10, destroyed: 0, bytesWritten: 600 });
});

test('keeps refill counts when a window exceeds the finished sample limit', async () => {
  const row = await readBench(double({ finishedCount: 129 }), options(), 'abc');
  expect(row.windows[0].refill).toMatchObject({ finished: 129, finishedMs: null, samplesMissing: 1 });
  expect(row.inconclusive).toContainEqual({
    quantity: 'refill.finishedMs', condition: 'finished sample limit exceeded', window: 1,
  });
});

test('headless and section 6 conditions name the quantities they affect', async () => {
  const config = parseOptions(['web', '--warmup-ms=0', '--window-ms=10', '--poll-ms=1'], true);
  const row = await readBench(double({ intervals: [16, 20, 16], timestamps: false, thermal: true }), config, 'abc');
  expect(row.headless).toBe(true);
  expect(row.device).toBeNull();
  expect(row.windows[0].gpuMs).toEqual(recorded.gpuMs);
  expect(row.windows[0].timestampQueries).toBe(false);
  expect(row.inconclusive).toEqual([
    { quantity: 'row', condition: 'headless' },
    { quantity: 'thermalState', condition: 'changed', window: 1 },
    ...[1, 2, 3].map(window => ({ quantity: 'gpuMs', condition: 'timestamp queries unavailable', window })),
    { quantity: 'frameMs', statistic: 'p95', condition: 'windows differ by more than 10%' },
  ]);
  expect(row).not.toHaveProperty('status');
});

test('draw pairing runs on then off and refuses mismatched units or bytes', async () => {
  const config = options('radius=8', '--pair', 'draw');
  expect(config.pair).toBe('draw');
  const order = [];
  const run = change => readDrawPair(row => {
    order.push(row.switches.draw);
    expect(row.set.get('draw')).toBe(row.switches.draw);
    expect(row.switches.radius).toBe('8');
    const on = row.switches.draw === 'on';
    return readBench(double(on ? {} : change), row, 'abc');
  }, config);
  for (const change of [{ extraUnits: 1 }, { extraUploads: 30 }]) {
    const pair = await run(change);
    expect(pair.drawPair.result).toBe('INCONCLUSIVE');
    expect(pair.drawPair.encodeDifferenceMs).toBeNull();
    expect(pair.off.firstPicture).toBeNull();
  }
  const pair = await run({ extraUploads: 10, encodeMs: ring(0.25) });
  expect(pair.drawPair.result).toBe('MATCHED');
  expect(pair.drawPair.encodeDifferenceMs[0].p95).toBeCloseTo(0.35);
  expect(order).toEqual(['on', 'off', 'on', 'off', 'on', 'off']);
});

test('a run that measured nothing fails instead of printing a row', async () => {
  const idle = double();
  const state = idle.state;
  idle.state = async reset => ({ ...await state(reset), perf: { ...recorded.perf, frameMs: ring(null, 0) } });
  await expect(readBench(idle, options('radius=40'), 'abc')).rejects.toThrow('Window 1 drew no frame');
  const blind = double();
  blind.state = async () => null;
  await expect(readBench(blind, options('radius=40'), 'abc'))
    .rejects.toThrow('Canvas argument radius at the start of window 1 is missing, expected 40');
});

test('validates hosts, metadata and switch values before opening a host', async () => {
  expect(() => options('radius=12')).toThrow('radius');
  expect(() => options('light=-1')).toThrow('light');
  expect(() => options('budget=count:0')).toThrow('budget');
  expect(() => parseOptions(['linux'], true)).toThrow('or web-android (Chrome on it)');
  expect(() => parseOptions(['ios', '--device=d', '--os=o', '--refresh-hz=120'], false)).toThrow('--phone');
  expect(parseOptions(['ios', '--device=d', '--os=o', '--refresh-hz=120', '--phone=pad'], false).phone).toBe('pad');
  for (const host of ['android', 'web-android'])
    expect(parseOptions([host, '--device=d', '--os=o', '--refresh-hz=90'], false).host).toBe(host);
  expect(() => parseOptions(['android'], true)).toThrow('web only');
  expect(() => parseOptions(['macos'], true)).toThrow('web only');
  expect(() => parseOptions(['web'], false)).toThrow('--refresh-hz');
  await expect(readBench(double({ ignoredSwitch: 'radius' }), options('radius=8'), 'abc'))
    .rejects.toThrow('Canvas argument radius at the start of window 1 is smoke, expected 8');
  const mac = parseOptions(['macos', '--device=Mac Studio', '--os=macOS', '--refresh-hz=59.94',
    '--warmup-ms=0', '--window-ms=10', '--poll-ms=1'], false);
  expect(await readBench(double(), mac, 'abc')).toMatchObject({ profile: 'host-dev', refreshHz: 59.94 });
});

test('records the jobs switch, counters and lane', async () => {
  expect(options().switches.jobs).toBe('off');
  expect(() => options('jobs=yes')).toThrow('jobs');
  const host = double();
  const read = host.state;
  const jobs = { queued: 3, queuedBytes: 32, running: 1, done: 7, stale: 2, failed: 0, lane: 'pool' };
  let resetAt = 0, window = 0;
  host.state = async reset => {
    const world = await read(reset);
    if (reset) {
      resetAt = host.now();
      window++;
    }
    world.renderHooks.jobs = { ...jobs, done: (host.now() - resetAt) * window,
      stale: Number(host.now() > resetAt) * window };
    return world;
  };
  const row = await readBench(host, options('jobs=on'), 'abc');
  expect(row.observedSwitches.jobs).toBe('on');
  expect(row.windows.map(window => window.jobs)).toEqual([1, 2, 3].map(window => ({
    ...jobs, done: window * 10, stale: window,
  })));
  expect(row).not.toHaveProperty('renderHooks');
  expect(row.lane).toBe('pool');
  expect(tableRow(row)).toContain('| pool |');
  const off = await readBench(double(), options(), 'abc');
  expect(off.windows.map(window => window.jobs)).toEqual([null, null, null]);
  expect(off.lane).toBeNull();
});

test('parses quality rungs and benchmark frame caps', () => {
  expect(options().switches).toMatchObject({ quality: 'high', fps: '0' });
  for (const quality of ['low', 'medium', 'high']) {
    const config = options(`quality=${quality}`);
    expect(config.switches.quality).toBe(quality);
    expect(config.set.get('quality')).toBe(quality);
  }
  for (const fps of ['0', '30', '45', '60']) {
    const config = options(`fps=${fps}`);
    expect(config.switches.fps).toBe(fps);
    expect(config.set.get('fps')).toBe(fps);
  }
  expect(() => options('quality=mid')).toThrow('quality=mid');
  expect(() => options('fps=5')).toThrow('fps=5');
  expect(() => options('fps=junk')).toThrow('fps=junk');
});

test.each([['low', 'low', 0.5], ['medium', 'mid', 0.75]])('records the %s rung and the frame intervals', async (quality, rung, scale) => {
  const host = double({ intervals: [22.2, 22.3, 22.2] });
  const state = host.state;
  host.state = async reset => ({ ...await state(reset), quality: { rung, samples: 1, scale } });
  const row = await readBench(host, options(`quality=${quality}`, 'fps=45'), 'abc');
  expect(host.calls).toEqual([['quality', quality], ['fps', '45'], 'play']);
  expect(row).toMatchObject({
    switches: { quality, fps: '45' },
    observedSwitches: { quality, fps: '45' },
    quality: rung,
  });
  expect(row.windows.map(window => window.frameMs.p50)).toEqual([22.2, 22.3, 22.2]);
});

test('rejects a quality argument the canvas ignored', async () => {
  await expect(readBench(double({ ignoredSwitch: 'quality' }), options('quality=low'), 'abc'))
    .rejects.toThrow('Canvas argument quality at the start of window 1 is high, expected low');
});

const boundaries = [
  ['start of window 1', 2, true],
  ['end of window 1', 12, false],
  ['end of window 2', 22, false],
  ['end of window 3', 32, false],
];

test.each(boundaries)('rejects a stale rung at the %s', async (boundary, at, resetAtBoundary) => {
  const host = double();
  const state = host.state;
  host.state = async reset => {
    const world = await state(reset);
    if (host.now() !== at || reset !== resetAtBoundary) world.quality = { rung: 'mid', samples: 1, scale: 0.75 };
    return world;
  };
  await expect(readBench(host, options('quality=medium'), 'abc'))
    .rejects.toThrow(`Quality rung at the ${boundary} is high, expected mid`);
});

for (const [name, requested, changed] of [['fps', '45', '0'], ['move', 'fly', 'stand']]) {
  test.each(boundaries)(`rejects a changed canvas argument ${name} at the %s`, async (boundary, at, resetAtBoundary) => {
    const host = double();
    const state = host.state;
    host.state = async reset => {
      const world = await state(reset);
      if (host.now() === at && reset === resetAtBoundary) world.args[name] = changed;
      return world;
    };
    await expect(readBench(host, options(`${name}=${requested}`), 'abc'))
      .rejects.toThrow(`Canvas argument ${name} at the ${boundary} is ${changed}, expected ${requested}`);
  });
}

test('rejects a missing canvas argument at the end of the last window', async () => {
  const host = double();
  const state = host.state;
  host.state = async reset => {
    const world = await state(reset);
    if (host.now() === 32) delete world.args.fps;
    return world;
  };
  await expect(readBench(host, options('fps=45'), 'abc'))
    .rejects.toThrow('Canvas argument fps at the end of window 3 is missing, expected 45');
});

test('records the canvas arguments from the last boundary', async () => {
  const host = double();
  const state = host.state;
  host.state = async reset => {
    const world = await state(reset);
    return { ...world, args: { ...world.args, marker: String(host.now()) } };
  };
  const row = await readBench(host, options(), 'abc');
  expect(row.observedSwitches).toEqual({ ...defaultArgs, marker: '32' });
});

test('rejects a missing rung when the first window starts', async () => {
  const host = double();
  const state = host.state;
  host.state = async reset => ({ ...await state(reset), quality: undefined });
  await expect(readBench(host, options(), 'abc'))
    .rejects.toThrow('Quality rung at the start of window 1 is missing, expected high');
});
