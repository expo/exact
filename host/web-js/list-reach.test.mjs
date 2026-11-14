// A virtual list's edges, reachstart and reachend, through the browser runtime and real DOM geometry
// (reach.rs; split from list.test.mjs, whose harness this repeats).
import { beforeAll, afterAll, test as bunTest, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { cdpBrowser, chromium, DESKTOP_INPUT } from '../../scripts/agent-launch.mjs';
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
    if (path === '/names.js') return new Response('export const sourceTypes = {};', {headers: {'content-type':'text/javascript'}});
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

// @ref LLP 1145 D4 — `reach-margin` sets how far ahead of the last row `reachend` fires, as the runner's reach.rs.
test('reach-margin moves reachend earlier or later; the default is one port', async () => {
  const result = await evaluate(`(async () => {
    const lists = [undefined, '300%', '0PX 960px', '0px'].map(reach => {
      const el = document.createElement('div');
      el.style.cssText = 'height:320px;width:400px;overflow:auto';
      document.getElementById('exact-root').append(el);
      const l = { el, ends: 0, at: null };
      l.own = rt.scope(() => list.vl(el, () => Array.from({length:200}, (_, i) => i), i => i(), (w, i) => {
        const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
      }, {est:32, pw:[], ph:[320], end: () => { l.ends++; }, reach}));
      return l;
    });
    rt.settled();
    await exact.lists.pending(); await exact.lists.settle();
    // The last row starts at 6368; the port ends 320 past its offset.
    for (const top of [1000, 5128, 5768, 6080]) {
      await Promise.all(lists.map(({el}) => new Promise(resolve => { el.addEventListener('scroll', resolve, {once:true}); el.scrollTop = top; })));
      await exact.lists.settle();
      for (let f = 0; f < 3; f++) await new Promise(r => requestAnimationFrame(() => setTimeout(r, 0)));
      for (const l of lists) if (l.ends && l.at === null) l.at = top;
    }
    for (const {el, own} of lists) { rt.end(own); el.remove(); }
    rt.settled();
    return lists.map(l => l.at);
  })()`);
  expect(result).toEqual([5768, 5128, 5128, 6080]);
}, 60_000);

test('replacement lists fire both edges at their reach-margin thresholds', async () => {
  const result = await evaluate(`(async () => {
    const results = [];
    for (const reach of [undefined, '0px', '300%', '0PX 960px', undefined]) {
      const el = document.createElement('div');
      el.id = 'feed'; el.style.cssText = 'height:320px;width:400px;overflow:auto';
      document.getElementById('exact-root').append(el);
      const starts = [], ends = [];
      const own = rt.scope(() => list.vl(el, () => Array.from({length:200}, (_, i) => i), i => i(), (w, i) => {
        const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
      }, {est:32, pw:[], ph:[320], start: () => { starts.push(el.scrollTop); }, end: () => { ends.push(el.scrollTop); }, reach}));
      // Only the first mount is also published when the controller loads.
      rt.settled(); await exact.lists.pending(); await exact.lists.settle();
      const scroll = async top => {
        await new Promise(resolve => { el.addEventListener('scroll', resolve, {once:true}); el.scrollTop = top; });
        await exact.lists.settle();
      };
      // Each boundary only touches an edge row; the next pixel overlaps it.
      for (const top of [1000, 5088, 5089, 5728, 5729, 6048, 6049, 6080]) await scroll(top);
      for (const top of [1000, 992, 991, 352, 351, 32, 31, 0]) await scroll(top);
      await scroll(6080);
      results.push({starts, ends});
      rt.end(own); el.remove(); rt.settled();
      if (exact.lists.internals.Lists.size) throw Error('a retired list is still registered');
    }
    return results;
  })()`);
  expect(result).toEqual([
    {starts:[0, 351], ends:[5729, 6080]},
    {starts:[0, 31], ends:[6049, 6080]},
    {starts:[0, 991], ends:[5089, 6080]},
    {starts:[0, 31], ends:[5089, 6080]},
    {starts:[0, 351], ends:[5729, 6080]},
  ]);
}, 60_000);

// @ref LLP 1027.004 D3 — the edge band leads a fling as the window does (reach.rs `Margin::band`). The
// agent's reports carry no velocity, so these are made here: one at rest, then one moving at -4,000 px/s.
test('a fling toward the start fires reachstart as far ahead as the window leads', async () => {
  const result = await evaluate(`(async () => {
    const lists = [undefined, '0px', '200%'].map(reach => {
      const el = document.createElement('div');
      el.style.cssText = 'height:320px;width:400px;overflow:auto';
      document.getElementById('exact-root').append(el);
      const l = { el, starts: 0 };
      l.own = rt.scope(() => list.vl(el, () => Array.from({length:200}, (_, i) => i), i => i(), (w, i) => {
        const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
      }, {est:32, pw:[], ph:[320], start: () => { l.starts++; }, reach}));
      l.c = [...exact.lists.internals.Lists.values()].find(c => c.el === el);
      return l;
    });
    rt.settled();
    await exact.lists.pending(); await exact.lists.settle();
    await Promise.all(lists.map(({el}) => new Promise(resolve => { el.addEventListener('scroll', resolve, {once:true}); el.scrollTop = 1000; })));
    await exact.lists.settle();
    // Row 0 ends at 32; the band before a port at rest is the margin, and
    // moving toward the start a quarter second of travel more, up to two ports.
    const report = (c, offset, velocity) => {
      const f = { ...c.geometry, revision: c.revision, scroll_sequence: c.geometry.scroll_sequence + 1, offset, measurements: [] };
      const byView = c.prepare(f);
      return byView ? c.feedback(f, byView, { velocity, limit: null })[1]?.first ?? null : 'refused';
    };
    const out = lists.map(({c}) => [
      [960, 0], [960, -4000], [991, -4000], [992, -4000], [640, 0], [640, -4000], [671, 0], [1311, -4000], [1312, -4000],
    ].map(([offset, velocity]) => { const edge = report(c, offset, velocity); if (edge === 0) c.edgeArmed[0] = true; return edge; }));
    for (const {el, own} of lists) { rt.end(own); el.remove(); }
    rt.settled();
    return out;
  })()`);
  const N = null;
  expect(result).toEqual([
    [N, 0, 0, N, N, 0, N, N, N],
    [N, N, N, N, N, 0, N, N, N],
    [N, 0, 0, 0, 0, 0, 0, 0, N],
  ]);
}, 60_000);

// reach.rs `geometric_edges`: a page smaller than a fling's band re-arms its edge by the band at rest.
test('a page that lands within a fling\'s band re-arms reachstart', async () => {
  const result = await evaluate(`(async () => {
    const el = document.createElement('div');
    el.style.cssText = 'height:320px;width:400px;overflow:auto';
    document.getElementById('exact-root').append(el);
    let starts = 0;
    const rows = rt.sig(Array.from({length:200}, (_, i) => i));
    const own = rt.scope(() => list.vl(el, rows, i => i(), (w, i) => {
      const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
    }, {est:32, pw:[], ph:[320], start: () => { starts++; }}));
    rt.settled();
    await exact.lists.pending(); await exact.lists.settle();
    await new Promise(resolve => { el.addEventListener('scroll', resolve, {once:true}); el.scrollTop = 1000; });
    await exact.lists.settle();
    const c = [...exact.lists.internals.Lists.values()].find(c => c.el === el);
    const report = (offset, velocity) => {
      const f = { ...c.geometry, revision: c.revision, scroll_sequence: c.geometry.scroll_sequence + 1, offset,
        measurements: c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size: 32 })) };
      const byView = c.prepare(f);
      if (!byView) return 'refused';
      const edge = c.feedback(f, byView, { velocity, limit: null })[1];
      if (edge?.first === 0) starts++;
      return edge?.first ?? null;
    };
    const out = [report(960, -4000), report(900, -4000)];
    // A message at the end re-arms nothing: the first row is the one it fired at.
    rt.commit(() => rt.W(rows, [...rows(), ...Array.from({length:20}, (_, i) => 200 + i)]));
    rt.settled();
    out.push(report(900, -4000), report(900, -4000), report(100, -4000));
    rt.commit(() => rt.W(rows, [...Array.from({length:20}, (_, i) => i - 20), ...rows()]));
    rt.settled();
    // The page's new overscan is mounted by the first report and measured by the next
    out.push(c.correction?.offset, report(740, -4000), report(740, -4000));
    rt.end(own); el.remove(); rt.settled();
    return out;
  })()`);
  expect(result).toEqual([0, null, null, null, null, 740, null, 0]);
}, 60_000);

// reach.rs `page_at_start` (LLP 1168.000 D1.1): a page that lands before the first row of a list at its top, where
// nothing anchors, or after a leading row that stays first, re-arms the start once the window is measured; an empty
// update, an arrival at the end, a reorder that moves the first row down and a window that slid re-arm nothing.
test('a page that lands at the top re-arms reachstart once measured', async () => {
  const result = await evaluate(`(async () => {
    const run = async (change, reach, length = 200) => {
      const el = document.createElement('div');
      el.style.cssText = 'height:320px;width:400px;overflow:auto';
      document.getElementById('exact-root').append(el);
      const rows = rt.sig(Array.from({length}, (_, i) => i));
      const own = rt.scope(() => list.vl(el, rows, i => i(), (w, i) => {
        const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
      }, {est:32, pw:[], ph:[320], start: () => {}, reach}));
      rt.settled();
      await exact.lists.pending(); await exact.lists.settle();
      if (length > 10) await new Promise(resolve => { el.addEventListener('scroll', resolve, {once:true}); el.scrollTop = 1000; });
      await exact.lists.settle();
      const c = [...exact.lists.internals.Lists.values()].find(c => c.el === el);
      const report = (offset, velocity, measured = true) => {
        const f = { ...c.geometry, revision: c.revision, scroll_sequence: c.geometry.scroll_sequence + 1, offset,
          measurements: measured ? c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size: 32 })) : [] };
        const byView = c.prepare(f);
        if (!byView) return 'refused';
        return c.feedback(f, byView, { velocity, limit: null })[1]?.first ?? null;
      };
      const out = [report(0, -4000), report(0, 0)];
      rt.commit(() => rt.W(rows, change(rows())));
      rt.settled();
      out.push(c.correction?.offset ?? null, report(0, 0, false), report(0, 0), report(0, 0));
      rt.end(own); el.remove(); rt.settled();
      return out;
    };
    return [
      await run(r => [...Array.from({length:20}, (_, i) => i - 20), ...r]),
      await run(r => [...r]),
      await run(r => [...r, ...Array.from({length:20}, (_, i) => 200 + i)]),
      await run(r => [r[1], r[0], ...r.slice(2)]),
      // A leading row that stays first, with a page put after it; a window that slid under it.
      await run(r => [r[0], ...Array.from({length:20}, (_, i) => i - 20), ...r.slice(1)]),
      await run(r => [r[0], ...r.slice(1).map(k => k + 1000)]),
      // The page with a band of five ports; a leading row alone that is given rows after it.
      await run(r => [...Array.from({length:20}, (_, i) => i - 20), ...r], '500%'),
      await run(r => [r[0], 1, 2], undefined, 1),
    ];
  })()`);
  const N = null;
  expect(result).toEqual([
    [0, N, N, N, 0, N],
    [0, N, N, N, N, N],
    [0, N, N, N, N, N],
    [0, N, N, N, N, N],
    [0, N, N, N, 0, N],
    [0, N, N, N, N, N],
    // These fired their start as they mounted, the margin or the one row reaching it.
    [N, N, N, N, 0, N],
    [N, N, N, N, 0, N],
  ]);
}, 60_000);


// reach.rs `disarm_edge`: a start owed a page is spent by the start firing, and an emptied list owes none.
test('a page owed to the start is spent by the start firing or an emptied list', async () => {
  const result = await evaluate(`(async () => {
    const el = document.createElement('div');
    el.style.cssText = 'height:320px;width:400px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const rows = rt.sig(Array.from({length:200}, (_, i) => i));
    const own = rt.scope(() => list.vl(el, rows, i => i(), (w, i) => {
      const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
    }, {est:32, pw:[], ph:[320], start: () => {}}));
    rt.settled();
    await exact.lists.pending(); await exact.lists.settle();
    await new Promise(resolve => { el.addEventListener('scroll', resolve, {once:true}); el.scrollTop = 1000; });
    await exact.lists.settle();
    const c = [...exact.lists.internals.Lists.values()].find(c => c.el === el);
    const report = (offset, measured = true) => {
      const f = { ...c.geometry, revision: c.revision, scroll_sequence: c.geometry.scroll_sequence + 1, offset,
        measurements: measured ? c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size: 32 })) : [] };
      const byView = c.prepare(f);
      if (!byView) return 'refused';
      return c.feedback(f, byView, { velocity: 0, limit: null })[1]?.first ?? null;
    };
    const set = list => { rt.commit(() => rt.W(rows, list)); rt.settled(); };
    // The start never fired: a page lands, an unmeasured report at the top fires it, and the measured ones do not again.
    const out = [report(1000)];
    set([...Array.from({length:20}, (_, i) => i - 20), ...rows()]);
    out.push(report(0, false), report(0), report(0));
    // An emptied list, refilled, re-arms by its emptiness and owes no page from before it.
    set([...Array.from({length:20}, (_, i) => i - 40), ...rows()]);
    set([]);
    set(Array.from({length:220}, (_, i) => 1000 + i));
    out.push(report(0, false), report(0), report(0));
    rt.end(own); el.remove(); rt.settled();
    return out;
  })()`);
  const N = null;
  expect(result).toEqual([N, 0, N, N, 0, N, N]);
}, 60_000);
