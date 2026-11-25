import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
import { expect } from 'bun:test';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';
import { glueDeclarations } from './glue-fixture.mjs';
import { runtimeSource, standIns } from './stand-ins.mjs';
import { decodePng } from '../../../scripts/png.mjs';
import { fixture as glueContext } from '../glue-fixture.mjs';

const { chromium: test } = await engineTests(['chromium']);
const host = glueDeclarations(readFileSync(new URL('../glue.js', import.meta.url), 'utf8'), ['apply', 'bootNow', 'reportColors']);
const runtime = ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'rt-link.js', 'held.js', 'font-family.js', 'observers.js', 'grid.js', 'overlay.js', 'roster.js', 'router.js', 'route-pattern.js', 'schedule.js', 'budget.js', 'elements.js', 'shape.js'];
const standins = standIns(runtime);
// What the wasm host's boot reads, as the shared glue fixture keeps it for the agent tests: a primitive as it is,
// anything else a stand-in that does nothing, and each wasm export a no-op. The page's own globals are its own.
const sharedContext = glueContext();
const shared = Object.entries(sharedContext).map(([name, value]) => [name, value === null || typeof value !== 'object' && typeof value !== 'function', value === null || typeof value !== 'object' && typeof value !== 'function' ? value : null]);
const sharedWasm = Object.keys(sharedContext.wasm);

// The data module supplies one valid list; both targets use the real browser replayer.
const draw = `export const drawer = {
  draw() {
    const bytes = new Uint8Array(48), v = new DataView(bytes.buffer);
    v.setUint32(0, 0x44324345, true); v.setUint32(4, 1, true);
    v.setUint32(8, 53, true); v.setUint32(12, 4, true);
    [0, 0, 20, 20].forEach((n, i) => v.setFloat64(16 + i * 8, n, true));
    return { lists: [bytes], frame: globalThis.animateCanvas ?? false };
  }, retire() {}
};`;

function serve() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/draw.js') return new Response(draw, { headers: { 'content-type': 'text/javascript' } });
    const files = {
      '/canvas2d.js': '../../web-js/canvas2d.js',
      '/canvas2d-glue.js': '../canvas2d-glue.js',
      '/navigation.js': '../navigation.js',
    };
    if (files[path]) return new Response(readFileSync(new URL(files[path], import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    if (runtime.includes(path.slice(1))) return new Response(runtimeSource(path.slice(1)), { headers: { 'content-type': 'text/javascript' } });
    if (standins.has(path.slice(1))) return new Response(standins.get(path.slice(1)), { headers: { 'content-type': 'text/javascript' } });
    return new Response('<main id="exact-root"></main>', { headers: { 'content-type': 'text/html' } });
  } });
}

for (const target of ['js', 'wasm']) test(`${target}: canvas fit and position follow the wrapper after style changes`, async () => {
  const server = serve(), browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ deviceScaleFactor: 1 });
    await page.goto(`${server.url}?agent=1`);
    await page.evaluate(async target => {
      globalThis.exact = {};
      await import('/canvas2d.js');
      const el = document.createElement('div'), surface = document.createElement('canvas');
      el.dataset.view = '1'; el.dataset.bitmapWidth = '40'; el.dataset.bitmapHeight = '20';
      el.id = 'actual';
      el.style.cssText = 'position:relative;isolation:isolate;width:160px;height:80px;font-size:20px;object-fit:none;object-position:7px 11px';
      surface.dataset.surface = '';
      surface.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;z-index:-1';
      el.append(surface); document.getElementById('exact-root').append(el);
      const oracle = document.createElement('canvas');
      oracle.id = 'oracle'; oracle.width = 40; oracle.height = 20;
      oracle.style.cssText = 'display:block;width:160px;height:80px;font-size:20px;object-fit:none;object-position:7px 11px';
      oracle.getContext('2d').fillRect(0, 0, 40, 20); el.after(oracle);
      if (target === 'js') {
        const { engine } = await import('/canvas2d.js');
        const canvases = engine({ data: {}, clock: { now: 0, agent: true }, inflight: { n: 0 }, journal: [], After: [], views: new Map([[1, el]]), viewId: () => 1 });
        canvases.args({ e: el, name: 'dot', args: [], types: [], names: [], mounted: 0 });
        globalThis.drawn = () => exact.canvas2dState()?.[0]?.draws > 0;
      } else {
        exact.send = () => {};
        const glue = exact.canvas2dGlue({ views: new Map([[1, el]]), now: () => 0 });
        exact.wasm = { exact_canvas_geometry() {
          glue.op({ id: 1, fresh: true, lifetime: 1, generation: 0, w: 40, h: 20, scale: 1, stretch: true, lists: [] });
          surface.getContext('2d').fillRect(0, 0, 20, 20);
        } };
        glue.op({ id: 1, watch: true }); glue.geometry(1);
        globalThis.drawn = () => surface.width === 40;
      }
      // The shared fixture draws a 20px square in a 40 × 20 bitmap.
      oracle.getContext('2d').clearRect(20, 0, 20, 20);
    }, target);
    await page.waitForFunction(() => drawn());
    for (const [fit, position] of [['none', '7px 11px'], ['contain', 'right bottom'], ['cover', '25% 75%'], ['fill', 'left top'], ['scale-down', '1em 0'], ['none', '10vw 0'], ['none', 'var(--missing, 7px 11px)']]) {
      await page.evaluate(([fit, position]) => {
        for (const id of ['actual', 'oracle']) Object.assign(document.getElementById(id).style, { objectFit: fit, objectPosition: position });
      }, [fit, position]);
      const actual = decodePng(await page.locator('#actual').screenshot()), oracle = decodePng(await page.locator('#oracle').screenshot());
      expect(Buffer.from(actual.data).equals(Buffer.from(oracle.data)), `${target}: ${fit}, ${position}`).toBe(true);
    }
  } finally { await browser.close(); server.stop(true); }
}, 30000);

for (const target of ['js', 'wasm', 'wasm-reload']) for (const agent of [false, true]) test(`${target}, agent=${agent}: removed Canvas 2D wrappers and bitmaps can be collected`, async () => {
  const server = serve(), browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ deviceScaleFactor: 2 });
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
    await page.goto(`${server.url}${agent ? '?agent=1' : ''}`);
    await page.evaluate(async ({ target, agent, host, shared, sharedWasm }) => {
      globalThis.exact = {};
      // Outside agent mode, wait for the first-geometry fallbacks to finish before collecting their closures.
      const timers = [], schedule = window.setTimeout.bind(window);
      window.setTimeout = (fn, ms) => {
        let done;
        timers.push(new Promise(resolve => { done = resolve; }));
        return schedule(() => { try { fn(); } finally { done(); } }, ms);
      };
      globalThis.timersDone = () => Promise.all(timers);
      await import('/canvas2d.js');
      const { drawer } = await import('/draw.js');
      const views = new Map(), refs = [], root = document.getElementById('exact-root');
      let mount, remove, ready;
      if (target === 'js') {
        const rt = await import('/rt.js');
        rt.clock.agent = agent;
        mount = (id, el) => rt.scope(() => rt.c2(el, 'dot', () => [], [], []));
        remove = (id, scope) => { views.get(id).remove(); views.delete(id); rt.end(scope); };
        ready = () => rt.inflight.n === 0;
      } else {
        const { afterPaintPieces } = await import('/navigation.js');
        const pieces = afterPaintPieces(async () => exact.canvas2dGlue, { views, now: () => 0, agent: () => agent });
        exact.send = () => {};
        exact.wasm = {
          memory: { buffer: drawer.draw().lists[0].buffer },
          exact_canvas_fonts: () => 0,
          exact_canvas_geometry(id, width, height, scale) {
            pieces.canvas2d({ id, fresh: true, lifetime: id, generation: 0, w: width * scale, h: height * scale, scale, lists: [[0, 48]] });
            return 0;
          },
          exact_in: () => 0, exact_plan_fonts: () => '[]', exact_boot_plan: () => '{"ops":[]}',
        };
        // Run the host's actual dispatcher and reload; unrelated services do nothing.
        const noop = () => {}, services = {
          views, root, pieces, retiredViews: new WeakSet(), presence: {}, listSelection: null,
          prepareContexts: noop, followedScrolls: new Map(), arrange: { destroy: noop, reset: noop }, motion: { destroy: noop, reset: noop },
          forgetList: noop, followScroll: noop, messageFrames: new Set(), sheetLeaves: () => false,
          messageViews: new Set(), dropSurface: noop, agentKeepsStore: false,
          navigation: { project: noop, reset: noop }, log: noop, refreshSymbols: noop, pendingScrolls: new Map(),
          collections: { restyled: noop, reset: noop }, syncLists: noop, focusAutofocus: noop,
          inputHandlers: null, positionContexts: noop, markScrollDocument: noop,
          bootAttempt: 0, incarnation: 0, devAssets: null, wasm: exact.wasm,
          memory: { buffer: new ArrayBuffer(128) }, readOut: x => x, prepareFonts: async () => [],
          encoder: new TextEncoder(), launchLocation: () => '/', preferences: () => 0, foldBits: () => 0,
          frameSampler: null, ticker: null, textflow: null, flowLoading: null, flowContexts: [], flowDue: null,
          flowFrames: false, lists: new Map(), storageRequests: null, grants: [], grantSet: null, unparsed: '',
          controllers: new Set(), forgettable: new Map(), inflight: new Map(), heldAsks: new Map(),
          mediaAttaching: new Map(), sizes: new Map(), page: null, commitFonts: noop,
          focus: { restart: (_, apply) => apply() }, applyBatch: batch => dispatch.apply(batch),
          toldOffset: null, followOffset: noop, now: () => 0, appRootFontSize: noop, inputReady: false,
          startClock: noop, afterPaint: noop, loadGpuIfNeeded: noop, agentMode: agent,
        };
        // (an empty list where iterated, and never a promise)
        const idle = new Proxy(function () {}, { get: (_, k) => k === Symbol.iterator ? function* () {} : k === 'then' || typeof k === 'symbol' ? undefined : idle, apply: () => idle });
        for (const [name, plain, value] of shared) if (!(name in services) && !(name in globalThis)) services[name] = plain ? value : idle;
        for (const name of sharedWasm) exact.wasm[name] ??= () => 0;
        services.host = services;
        const dispatch = new Function(...Object.keys(services), `${host}; return { apply, bootNow };`)(...Object.values(services));
        mount = id => dispatch.apply({ ops: [{ op: 'canvas2d', id, watch: true }] });
        remove = target === 'wasm-reload'
          ? () => dispatch.bootNow(new Uint8Array([1]), null, () => true, null, true)
          : id => dispatch.apply({ ops: [{ op: 'destroy', id }] });
        ready = () => pieces.pending();
      }
      let current = null, next = 1;
      globalThis.cycle = async () => {
        if (current) {
          await remove(current.id, current.c); current = null;
          if (target === 'wasm-reload') next = 1;
          return;
        }
        const id = next++, el = document.createElement('div'), surface = document.createElement('canvas');
        el.dataset.view = id; el.style.cssText = 'width:300px;height:150px;position:relative';
        surface.dataset.surface = ''; surface.style.position = 'absolute';
        el.append(surface); root.append(el); views.set(id, el);
        refs.push(new WeakRef(el), new WeakRef(surface));
        current = { id, c: mount(id, el) };
        await ready();
      };
      globalThis.drawn = () => root.querySelector('canvas')?.width === 600 && root.querySelector('canvas')?.getContext('2d').getImageData(0, 0, 1, 1).data[3] === 255;
      globalThis.retained = () => refs.filter(r => r.deref()).length;
    }, { target, agent, host, shared, sharedWasm });
    for (let i = 0; i < 41; i++) {
      await page.evaluate(() => cycle());
      await page.waitForFunction(() => drawn());
      if (i < 40) await page.evaluate(() => cycle());
    }
    await page.evaluate(() => timersDone());
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('HeapProfiler.collectGarbage');
    expect(errors).toEqual([]);
    expect(await page.evaluate(() => retained())).toBe(2);
    await page.evaluate(() => cycle());
    await cdp.send('HeapProfiler.collectGarbage');
    expect(await page.evaluate(() => retained())).toBe(0);
  } finally { await browser.close(); server.stop(true); }
}, 30000);

test('removing the last animated canvas stops the frame loop on its next callback', async () => {
  const server = serve(), browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.goto(String(server.url));
    const got = await page.evaluate(async () => {
      const frames = [], ops = [];
      window.requestAnimationFrame = fn => frames.push(fn);
      globalThis.exact = {};
      globalThis.animateCanvas = true;
      const { engine } = await import('/canvas2d.js');
      const make = exact.canvas2dGlue;
      exact.canvas2dGlue = options => {
        const glue = make(options);
        return { ...glue, op(op) { if (op.frames !== undefined) ops.push(op.frames); glue.op(op); } };
      };
      const el = document.createElement('div'), surface = document.createElement('canvas');
      surface.dataset.surface = ''; el.append(surface); document.body.append(el);
      const clock = { now: 0 }, rt = {
        clock, data: {}, inflight: { n: 0 }, journal: [], After: [], views: new Map([[1, el]]),
        viewId: () => 1, advance: time => { clock.now = time; }, wall: () => 1,
      };
      const canvases = engine(rt);
      await Promise.resolve();
      const c = { e: el, name: 'dot', args: [], types: [], names: [], mounted: 0 };
      canvases.args(c);
      exact.wasm.exact_canvas_geometry(1, 300, 150, 1);
      const mounted = [ops.slice(), frames.length];
      el.remove(); rt.views.delete(1); canvases.gone(c);
      frames.shift()();
      return { mounted, removed: [ops, frames.length, exact.canvas2dState()] };
    });
    expect(got).toEqual({ mounted: [[true], 1], removed: [[true, false], 0, []] });
  } finally { await browser.close(); server.stop(true); }
});
