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

// start.rs `end_list_anchor` and index.rs `capture_bottom_anchor` (LLP 1168.000 D1.2), as the runner's end_anchor.rs: a
// list that opens at its end keeps its last visible row off the port's bottom when an update puts rows before the
// window, at a zero offset too, past a leading row that stays first or a separator that moves with the page; an edit
// inside the window anchors as before, and so does a list that opens at its start (mail F8). end_anchor.rs's pins too.
test('a list that opens at its end keeps its last visible row when rows land before the window', async () => {
  const result = await evaluate(`(async () => {
    // \`start\`: a reachstart handler, which the reports at the top fire, leaving it unanswered when the change lands;
    // \`hook\`: run on the list before the change lands.
    const run = async (first, change, offset = 0, atEnd = true, start = undefined, hook = undefined) => {
      const el = document.createElement('div');
      el.style.cssText = 'height:320px;width:400px;overflow:auto';
      document.getElementById('exact-root').append(el);
      const rows = rt.sig(first);
      const own = rt.scope(() => list.vl(el, rows, i => i(), (w, i) => {
        const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
      }, {est:32, pw:[], ph:[320], atEnd, start}));
      rt.settled();
      await exact.lists.pending(); await exact.lists.settle();
      const c = [...exact.lists.internals.Lists.values()].find(c => c.el === el);
      const report = (offset, velocity) => {
        const f = { ...c.geometry, revision: c.revision, scroll_sequence: c.geometry.scroll_sequence + 1, offset,
          measurements: c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size: 32 })) };
        const byView = c.prepare(f);
        if (byView) c.feedback(f, byView, { velocity, limit: null });
      };
      report(offset + 1000, -4000); report(offset, -4000); report(offset, 0);
      hook?.(c);
      const before = c.correction?.offset ?? null;
      rt.commit(() => rt.W(rows, change(rows()))); rt.settled();
      const out = [before, c.correction?.offset ?? null];
      rt.end(own); el.remove(); rt.settled();
      return out;
    };
    const plain = Array.from({length:200}, (_, i) => i), page = Array.from({length:20}, (_, i) => i - 20);
    const days = r => ['day-before', ...page.slice(0, 10), 'today', ...page.slice(10), ...r.slice(1)];
    const insert = at => r => [...r.slice(0, at), 'extra', ...r.slice(at)], asked = [];
    return [
      await run(plain, r => [...page, ...r]),
      await run(['start', ...plain], r => [r[0], ...page, ...r.slice(1)]),
      await run(['today', ...plain], days),
      await run(plain, r => [...page, ...r], 645),
      await run(plain, insert(5)),
      await run(plain, insert(25), 640),
      await run(['start', ...plain], insert(3)),
      await run(plain, r => [...page, ...r], 0, false),
      // Pinned (LLP 1010's D1.2 note): a row put right after a leading row is taken for a page. With no reachstart
      // unanswered, a page after a leading run of two rows is not; with one, it is that page, and so is an insert,
      // but not once the start was refused or held, as \`edges\` re-arms it (end_anchor.rs
      // \`a_refused_start_leaves_nothing_unanswered\`); equal rows again, a row body's inputs changed (an opaque body's
      // commit), answer nothing (\`an_answer_with_the_same_rows_leaves_reachstart_unanswered\`); a row told apart as
      // \`eq\` tells it (signed zero) answers it (reach.rs \`rows_answer_by_key_or_by_value_as_eq\`); a list that opens
      // at its start keeps nothing asked.
      await run(['start', ...plain], insert(1)),
      await run(['start', 'today', ...plain], r => [r[0], r[1], ...page, ...r.slice(2)]),
      await run(['start', 'today', ...plain], r => [r[0], r[1], ...page, ...r.slice(2)], 0, true, () => {}),
      await run(plain, insert(5), 0, true, () => {}, c => asked.push(c.startAsked)),
      await run(plain, insert(5), 0, true, () => {}, c => c.rearm(0)),
      await run(plain, insert(5), 0, true, () => {}, c => c.update(c.items.map(x => x), [c.idents, c.dups], true)),
      await run(plain, insert(5), 0, true, () => {}, c => c.update(c.items.map(x => x === 0 ? -0 : x), [c.idents, c.dups])),
      await run(plain, insert(5), 0, false, () => {}, c => asked.push(c.startAsked)),
      // What the start asked, kept only by the list that opens at its end (reach.rs \`disarm_edge\`).
      asked,
    ];
  })()`);
  const N = null;
  expect(result).toEqual([[N, 640], [N, 640], [N, 672], [N, 1285], [N, N], [N, N], [N, N], [N, N], [N, 32], [N, N], [N, 640], [N, 32], [N, N], [N, 32], [N, N], [N, N], [true, false]]);
}, 60_000);

// @ref runner/src/instance/collection/index_tests.rs `a_bottom_anchor_keeps_the_last_visible_row_off_the_ports_bottom`,
// case by case on the JS target's size index.
test('a bottom anchor keeps the last visible row off the port\'s bottom, as the runner\'s does', async () => {
  const result = await evaluate(`(async () => {
    const port = document.createElement('div');
    port.style.cssText = 'height:100px;width:200px;overflow:auto';
    document.getElementById('exact-root').append(port);
    const own = rt.scope(() => list.vl(port, () => [0, 1], i => i(), (w, i) => { const row = document.createElement('div'); row.style.height = '20px'; w.append(row); }, {est:20, pw:[], ph:[100]}));
    rt.settled();
    const C = port.$list.index.constructor;
    rt.end(own); port.remove(); rt.settled();
    const ix = new C(10), keys = n => Array.from({length:n}, (_, i) => 'k' + i);
    ix.replace(keys(10)); keys(10).forEach(k => ix.setMeasured(k, ix.token(k), 10));
    const top = ix.bottomAnchor(0, 20, false), inside = ix.bottomAnchor(5, 20, false), follow = ix.bottomAnchor(80, 20, true);
    const out = [top.order[top.row], inside.order[inside.row], follow.follows];
    ix.replace([...Array.from({length:5}, (_, i) => 'p' + i), ...keys(10)]);
    out.push(ix.restoreAnchor(top, 20), ix.keptRow(top, 20), ix.restoreAnchor(inside, 20), ix.restoreAnchor(follow, 20));
    ix.setMeasured('k1', ix.token('k1'), 30);
    out.push(ix.restoreAnchor(top, 20));
    ix.replace(ix.order.filter(k => k !== 'k1'));
    out.push(ix.restoreAnchor(top, 20));
    // index_tests.rs \`a_bottom_anchor_reads_the_padding_and_skips_a_zero_height_tail\`.
    const at = (hs, ks) => { const ix = new C(10); ix.replace(ks ?? keys(hs.length)); hs.forEach((h, i) => ix.setMeasured('k' + i, ix.token('k' + i), h)); return ix; };
    const pages = Array.from({length:5}, (_, i) => 'p' + i);
    const padded = at(Array(20).fill(32)); padded.leading = 20;
    const p = padded.bottomAnchor(-20, 320, false);
    padded.replace([...pages, ...keys(20)]);
    const tail = at([10, 10, 0]); tail.trailing = 20;
    const t = tail.bottomAnchor(0, 30, false);
    tail.replace([...pages, 'k0', 'k1', 'new', 'k2']);
    out.push(padded.restoreAnchor(p, 320), t.order[t.row], tail.restoreAnchor(t, 30));
    return out;
  })()`);
  expect(result).toEqual(['k1', 'k2', true, 50, true, 55, 130, 70, 50, 30, 'k1', 50]);
}, 60_000);

// end_anchor.rs `a_reader_in_the_leading_padding_keeps_the_last_visible_row` (LLP 1168.000 D1.2): a reader 32 px into
// the padding above the first row is kept there; a row put first moves the port to 0, from -32.
test('a reader in the leading padding keeps the last visible row', async () => {
  const result = await evaluate(`(async () => {
    const el = document.createElement('div');
    el.style.cssText = 'height:320px;width:400px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const rows = rt.sig(Array.from({length:200}, (_, i) => i));
    const own = rt.scope(() => list.vl(el, rows, i => i(), (w, i) => {
      const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
    }, {est:32, pw:[], ph:[320], atEnd: true}));
    rt.settled();
    await exact.lists.pending(); await exact.lists.settle();
    const c = [...exact.lists.internals.Lists.values()].find(c => c.el === el);
    const report = (offset, velocity) => {
      const f = { ...c.geometry, padding: [32, 0], revision: c.revision, scroll_sequence: c.geometry.scroll_sequence + 1, offset,
        measurements: c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size: 32 })) };
      const byView = c.prepare(f);
      if (byView) c.feedback(f, byView, { velocity, limit: null });
    };
    for (let i = 0; i < 3; i++) report(c.correction?.offset ?? 200 * 32 + 32 - 320, 0);
    report(968, -4000); report(-32, -4000); report(-32, 0);
    const out = [c.correction?.offset ?? null];
    rt.commit(() => rt.W(rows, ['extra', ...rows()])); rt.settled();
    out.push(c.correction?.offset ?? null, c.correction?.from ?? null);
    rt.end(own); el.remove(); rt.settled();
    return out;
  })()`);
  expect(result).toEqual([null, 0, -32]);
}, 60_000);

// end_anchor.rs `a_held_reader_330_px_below_a_page_is_offered_the_next` and
// `the_start_waits_for_the_reader_unless_the_band_reaches_the_page` (LLP 1168.000 D1.1 with D1.2): a reader at the top
// of a list that opens at its end is held a page down; the start fires again only where the page's first row is in the
// band, 22 px into a one-port band below a 330 px page, not below a 352 px one, and below 640 px with a 500% margin.
test('a held reader below a page is offered the next only where the band reaches it', async () => {
  const result = await evaluate(`(async () => {
    const run = async (page, small, reach) => {
      const el = document.createElement('div');
      el.style.cssText = 'height:320px;width:400px;overflow:auto';
      document.getElementById('exact-root').append(el);
      const rows = rt.sig(Array.from({length:200}, (_, i) => i));
      let starts = 0;
      const own = rt.scope(() => list.vl(el, rows, i => i(), (w, i) => {
        const row = document.createElement('div'); row.textContent = String(i()); row.style.height = '32px'; w.append(row);
      }, {est:32, pw:[], ph:[320], atEnd: true, start: () => {}, reach}));
      rt.settled();
      await exact.lists.pending(); await exact.lists.settle();
      const c = [...exact.lists.internals.Lists.values()].find(c => c.el === el);
      const report = (offset, velocity, size = () => 32) => {
        const f = { ...c.geometry, revision: c.revision, scroll_sequence: c.geometry.scroll_sequence + 1, offset,
          measurements: c.mounted.map(m => ({ view: m.view, epoch: m.epoch, size: size(m.position) })) };
        const byView = c.prepare(f);
        if (byView && c.feedback(f, byView, { velocity, limit: null })[1]?.first === 0) starts++;
      };
      report(1000, -4000); report(0, -4000); report(0, 0);
      const fired = starts;
      rt.commit(() => rt.W(rows, [...Array.from({length:page}, (_, i) => i - page), ...rows()])); rt.settled();
      const estimated = c.correction?.offset ?? null;
      const size = i => i === 0 ? 32 : i < page ? small : 32;
      // The page's rows mount on the first report at the estimate and are measured on the second (the runner's window
      // has them mounted already).
      report(estimated, 0, size); report(estimated, 0, size);
      const held = c.correction?.offset ?? estimated;
      report(held, 0, size); report(held, 0, size);
      const out = [fired, estimated, Math.round(held * 100) / 100, starts];
      rt.end(own); el.remove(); rt.settled();
      return out;
    };
    return [await run(11, 29.8), await run(11, 32), await run(20, 32), await run(20, 32, '500%')];
  })()`);
  expect(result).toEqual([[1, 352, 330, 2], [1, 352, 352, 1], [1, 640, 640, 1], [1, 640, 640, 2]]);
}, 60_000);
