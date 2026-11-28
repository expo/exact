// App-owned virtual-list restoration through the browser runtime and real DOM geometry.
import { beforeAll, afterAll, test as bunTest, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { cdpBrowser, chromium, DESKTOP_INPUT } from '../../scripts/agent-launch.mjs';
import { webAx } from '../../scripts/agent-ax.mjs';
import { glueDeclarations } from '../web/tests/glue-fixture.mjs';
import { need } from '../../scripts/coverage.mjs';

const { unavailable } = chromium();
const test = need('chrome', unavailable, { test: bunTest });
let server, cdp, evaluate, call;
// Register the shared browser's shutdown hook before beforeAll runs.
const suite = unavailable ? null : await cdpBrowser({ args: [DESKTOP_INPUT] });
beforeAll(async () => {
  if (unavailable) return;
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/names.js') return new Response('export default [[], [], []]; export const types = [[], [], []]; export const sourceTypes = {};', {headers: {'content-type':'text/javascript'}});
    // A module that re-exports the host's own reaches it through `../web/`.
    const file = path.startsWith('/web/') ? `..${path}` : existsSync(new URL(`.${path}`, import.meta.url)) ? `.${path}` : `../web${path}`;
    return new Response(path.endsWith('.js') ? readFileSync(new URL(file, import.meta.url)) : '<!doctype html><body><main id="exact-root"></main>', {
      headers: { 'content-type': path.endsWith('.js') ? 'text/javascript' : 'text/html' },
    });
  }});
  ({ cdp } = suite);
  const { sessionId } = await suite.page();
  call = (method, params) => cdp.send(method, params, sessionId);
  evaluate = async expression => {
    const result = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (result.exceptionDetails) throw Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    return result.result.value;
  };
  await call('Page.navigate', { url: `http://127.0.0.1:${server.port}` });
  await evaluate(`Promise.all([import('/rt.js'), import('/list.js')]).then(([rt, list]) => { globalThis.rt = rt; globalThis.list = list; rt.clock.agent = true; })`);
}, 60_000); // hang bounds: Chrome's launch and exit are awaited as events, and take longer than 5 s under load
afterAll(async () => {
  await suite?.close();
  server?.stop(true);
}, 60_000);

test('an animated final row cannot remove the fixed trail across its window boundary', async () => {
  const result = await evaluate(`(() => {
    const port = document.createElement('div'); document.querySelector('main').append(port);
    const own = rt.scope(() => list.vl(port, () => [], i => i(), () => {}, {
      est:40, pw:[200], ph:[200], fixed:[() => {}, el => { const footer=document.createElement('div'); footer.textContent='Footer'; el.append(footer); }]
    }));
    const c=port.$list, leaving=document.createElement('div'); leaving.setAttribute('data-exiting','');
    port.insertBefore(leaving,c.fixed.b); c.children=[leaving]; c.emit();
    const kept=port.lastChild.textContent; rt.end(own); port.remove(); return kept;
  })()`);
  expect(result).toBe('Footer');
});

test('nested restoration keeps data below the lead and positions wholly inside fixed content', async () => {
  const result = await evaluate(`(() => {
    const port=document.createElement('div');document.querySelector('main').append(port);
    const own=rt.scope(()=>list.vl(port,()=>[1,2,3],i=>i(),()=>{}, {est:100,pw:[300],ph:[200],fixed:[()=>{},()=>{}]}));
    const c=port.$list; c.index.fixed=[300,800];
    const offsets=[];
    for(const at of [150,850]) {
      c.geometry={offset:at,port_main:200};const kept=c.position();
      c.geometry=null;c.index.fixed=[0,0];c.restorePosition(kept);
      offsets.push(c.correction.offset);c.index.fixed=[300,800];
    }
    rt.end(own);port.remove();return offsets;
  })()`);
  expect(result).toEqual([150, 850]);
});

// A list that opens at its end, whose fixed footer is taller than its port and the rows around it, mounts no data
// row there: at rest at its end, nothing pending, it has opened.
test('an end-start list whose footer fills its window opens at its end with no row mounted', async () => {
  const result = await evaluate(`(() => {
    const port=document.createElement('div');document.querySelector('main').append(port);
    const own=rt.scope(()=>list.vl(port,()=>[1,2,3],i=>i(),()=>{}, {est:60,pw:[130],ph:[320],fixed:[()=>{},()=>{}]}));
    const c=port.$list; c.index.fixed=[0,3000];
    c.atEnd=true; c.mounted=[]; c.pending=false; c.animating=false; c.correction=null;
    c.geometry={offset:c.index.maxOffset(320),port_main:320};
    c.settleStart();
    const opened=!c.atEnd;
    rt.end(own);port.remove();return opened;
  })()`);
  expect(result).toBe(true);
});

test('initial follow measures fixed geometry before choosing an end anchor', async () => {
  const result = await evaluate(`(() => {
    const offsets=[];
    for(const items of [[1],[]]) for(const start of ['start','end']) {
      const port=document.createElement('div');document.querySelector('main').append(port);
      const own=rt.scope(()=>list.vl(port,()=>items,i=>i(),()=>{}, {est:100,pw:[300],ph:[200],atEnd:start==='end',fixed:[()=>{},()=>{}]}));
      const c=port.$list; c.followEnd=true; const byView=new Map(c.mounted.map((m,i)=>[m.view,i]));
      const measurements=c.fixedItems().map((item,i)=>({...item,size:[600,800][i]}));
      c.feedback({offset:0,port_main:200,port_cross:300,cross:300,padding:[0,0],measurements},byView,{});
      offsets.push(c.correction?.offset??0);rt.end(own);port.remove();
    }
    return offsets;
  })()`);
  expect(result).toEqual([0,1300,0,1200]);
});

test('fling preloading does not expose the trail before the geometric window reaches it', async () => {
  const result=await evaluate(`(() => {
    const port=document.createElement('div');document.querySelector('main').append(port);
    const own=rt.scope(()=>list.vl(port,()=>[1,2,3],i=>i(),()=>{}, {est:100,pw:[300],ph:[200],fixed:[()=>{},()=>{}]}));
    const c=port.$list;c.index.fixed=[300,800];c.geometry={offset:0,port_main:200,cross:300};
    c.realize(false,{velocity:4000});const fling=c.fixed.hidden;
    c.geometry.offset=101;c.realize(false,{});const near=c.fixed.hidden;
    rt.end(own);port.remove();return [fling,near];
  })()`);
  expect(result).toEqual([true,false]);
});

test('fixed content anchors stay out of Views, tree --ax joins fixed and row controls, and axStamp skips non-element views', async () => {
  const setup = await evaluate(`(async () => {
    const { install } = await import('/agent.js');
    const port = document.createElement('div'); document.querySelector('main').append(port);
    let headerBtn, rowBtn, footerBtn;
    const own = rt.scope(() => list.vl(port, () => [1, 2], i => i(), (row, item) => {
      const btn = document.createElement('button'); btn.textContent = 'Row ' + item(); row.append(btn);
      if (item() === 1) rowBtn = btn;
    }, {
      est: 40, pw: [200], ph: [200],
      fixed: [
        el => { headerBtn = document.createElement('button'); headerBtn.textContent = 'Header Action'; el.append(headerBtn); },
        el => { footerBtn = document.createElement('button'); footerBtn.textContent = 'Footer Action'; el.append(footerBtn); },
      ],
    }));
    const c = port.$list;
    const fixed = c.fixedItems();
    const fixedViews = fixed.map(item => item.view);
    const exact = (globalThis.exact = { views: rt.Views, viewId: rt.viewId, root: document.querySelector('main'), clock: rt.clock, After: rt.After, Mutations: rt.Mutations, journal: rt.journal, data: rt.data, rev: rt.rev, inflight: rt.inflight, settled: rt.settled });
    install(exact);
    globalThis.__cleanupFixedAx = () => { rt.end(own); port.remove(); };
    return {
      hasNonElement: [...rt.Views.values()].some(v => v.nodeType !== 1),
      reservedAbsent: fixedViews.every(v => !rt.Views.has(v)),
      distinctPositive: fixedViews.every(v => Number.isInteger(v) && v > 0) && fixedViews[0] !== fixedViews[1],
      sides: fixedViews.map(v => c.fixedSide(v)),
      ids: { header: rt.viewId(headerBtn), row: rt.viewId(rowBtn), footer: rt.viewId(footerBtn) },
    };
  })()`);
  try {
    expect(setup.hasNonElement).toBe(false);
    expect(setup.reservedAbsent).toBe(true);
    expect(setup.distinctPositive).toBe(true);
    expect(setup.sides).toEqual([0, 1]);
    // Run the real CDP tree --ax collector so AX joins must resolve data-agent-view on both fixed and row controls.
    const carrier = { browser: 'chrome', call, evaluate, ask: req => evaluate(`exact.agentSettled(${JSON.stringify(req)})`) };
    const { reply, plain } = await webAx(carrier, {}, () => carrier.ask({ op: 'tree' }));
    expect(reply.error).toBeUndefined();
    const joined = new Map(reply.ax.elements.filter(e => e.id != null).map(e => [e.name, e.id]));
    expect(joined.get('Header Action')).toBe(setup.ids.header);
    expect(joined.get('Row 1')).toBe(setup.ids.row);
    expect(joined.get('Footer Action')).toBe(setup.ids.footer);
    expect(plain.nodes.some(n => n.id === setup.ids.header)).toBe(true);
    expect(plain.nodes.some(n => n.id === setup.ids.row)).toBe(true);
    expect(plain.nodes.some(n => n.id === setup.ids.footer)).toBe(true);
    // Defense in depth: non-element entries in Views are skipped in both axStamp and hatchRegions loops.
    const stray = await evaluate(`(async () => {
      const comment = document.createComment('stray');
      document.querySelector('main').append(comment);
      const strayId = rt.reserveViewId();
      rt.Views.set(strayId, comment);
      let hatchSawNonElement = false;
      exact.hatchRegions = { of: el => { if (el.nodeType !== 1) hatchSawNonElement = true; return null; } };
      const res = await exact.agentSettled({ op: 'axStamp' });
      delete exact.hatchRegions;
      rt.Views.delete(strayId);
      comment.remove();
      return { ok: !res.error && typeof res.nonce === 'number', hatchSawNonElement };
    })()`);
    expect(stray).toEqual({ ok: true, hatchSawNonElement: false });
  } finally {
    await evaluate('globalThis.__cleanupFixedAx?.(); delete globalThis.__cleanupFixedAx;');
  }
  // Wasm host axStamp guard: skips non-element views while still stamping real elements.
  const wasmAgent = readFileSync(new URL('../web/wasm/agent.js', import.meta.url), 'utf8');
  const stamped = new Map();
  const realEl = { nodeType: 1, isConnected: true, getAttribute: k => stamped.get(k) ?? null, setAttribute: (k, v) => stamped.set(k, String(v)) };
  const commentNode = { nodeType: 8, isConnected: true };
  const bindings = {
    views: new Map([[1, commentNode], [2, realEl]]),
    ask: () => ({}), shownValue: () => '', host: { wasm: {}, agentClock: 0 }, INHERITED_CSS: [], now: () => 0, root: {},
    iframeLoading: new Map(), waiting: new Map(), httpHelpers: {}, loadGpuIfNeeded: () => {}, heldNamed: x => x,
    loadAfterPaint: () => {}, stageLoaded: () => true, loadStage: () => {}, agentMode: true, gpuInPlay: () => false,
    register: () => {}, seek: () => {}, postSurface: () => {}, deferFulfill: () => {}, documentsGlue: () => {},
    picker: () => {}, t0: 0, log: () => {}, readOut: () => '', presence: {}, byTicket: new Map(), writeIn: () => 0,
    pageFacts: {}, pageChanged: () => {}, foldChanged: () => {}, followOffset: () => {}, pieces: {}, moduleReady: Promise.resolve(),
    SETTLE_DEADLINE_MS: 1000, mediaAttached: () => false, GLIDE_BOUND_MS: 1000, collections: {}, settleCandidate: () => {},
    tagged: x => x, performance: { timeOrigin: 123 },
  };
  const wasmReply = new Function(...Object.keys(bindings), glueDeclarations(wasmAgent, ['agentReply']) + '; return agentReply;')(...Object.values(bindings));
  expect(wasmReply({ op: 'axStamp' })).toEqual({ nonce: 123 });
  expect(stamped.get('data-agent-view')).toBe('2');
});
