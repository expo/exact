import { afterAll, beforeAll, expect, test } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

let server, suite;
beforeAll(async () => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/names.js') return new Response('export const sourceTypes = {};', {headers: {'content-type': 'text/javascript'}});
    const file = path.startsWith('/web/') ? `..${path}` : existsSync(new URL(`.${path}`, import.meta.url)) ? `.${path}` : `../web${path}`;
    return new Response(path.endsWith('.js') ? readFileSync(new URL(file, import.meta.url)) : '<!doctype html><main id="exact-root"></main>', {
      headers: {'content-type': path.endsWith('.js') ? 'text/javascript' : 'text/html'},
    });
  }});
  suite = await suiteBrowser();
}, 60_000);
afterAll(async () => { await suite?.close(); server?.stop(true); });

// LLP 1070 H2: a literal gap and a changing numeric cross-size keep eager geometry.
test('a gapped horizontal collection matches eager cards before and after resizing', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const root = document.getElementById('exact-root');
    const strip = () => {
      const el = document.createElement('div');
      el.style.cssText = 'display:flex;gap:14px;height:120px;width:400px;overflow-x:scroll;overflow-y:hidden';
      root.append(el); return el;
    };
    const virtual = strip(), eager = strip();
    const card = (parent, n) => {
      const el = document.createElement('div');
      el.dataset.n = String(n); el.style.cssText = 'width:60px;height:100px;flex:none';
      parent.append(el);
    };
    for (let n = 0; n < 300; n++) card(eager, n);
    const height = rt.sig(120);
    const own = rt.scope(() => {
      rt.S(virtual, 'height', 'px', () => height() + 0);
      list.vl(virtual, rt.sig(Array.from({length: 300}, (_, n) => n)), n => n(), (w, n) => card(w, n()),
        {x: true, est: 60, gap: 14, pw: [400], ph: [120]});
    });
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    const compare = () => {
      const origin = el => el.getBoundingClientRect().left - el.scrollLeft;
      const wrong = [...virtual.querySelectorAll('[data-n]')].filter(el => {
        const other = eager.querySelector(`[data-n="${el.dataset.n}"]`);
        return Math.abs((el.getBoundingClientRect().left - origin(virtual)) - (other.getBoundingClientRect().left - origin(eager))) > 0.01;
      }).map(el => el.dataset.n);
      return {left: virtual.scrollLeft, width: virtual.scrollWidth, eagerWidth: eager.scrollWidth, wrong};
    };
    const move = async left => {
      const moved = new Promise(resolve => virtual.addEventListener('scroll', resolve, {once: true}));
      virtual.scrollLeft = left; eager.scrollLeft = left;
      await moved; await exact.lists.settle(); return compare();
    };
    const middle = await move(3000);
    virtual.querySelector('[data-n="40"]').style.width = '110px';
    eager.querySelector('[data-n="40"]').style.width = '110px';
    rt.commit(() => rt.W(height, 180));
    rt.settled(); await exact.lists.settle();
    const resized = {left: virtual.scrollLeft, height: virtual.clientHeight, wrong: compare().wrong};
    const end = await move(30000);
    exact.lists.into(virtual.$list.view, 200, "start", "end");
    await exact.lists.settle();
    const indexed = virtual.scrollLeft;
    rt.end(own); rt.settled();
    return {middle, resized, end, indexed};
  });
  expect(result).toEqual({
    middle: {left: 3000, width: 22186, eagerWidth: 22186, wrong: []},
    resized: {left: 3000, height: 180, wrong: []},
    end: {left: 21836, width: 22236, eagerWidth: 22236, wrong: []},
    indexed: 14510,
  });
  await page.close();
}, 60_000);

test('the shared web controller counts authored gaps as mounted coverage', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const limits = await page.evaluate(async () => {
    const {collectionController} = await import('/web/collection-glue.js');
    const root = document.getElementById('exact-root'), el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:300px;height:100px;overflow:auto';
    root.append(el);
    const views = new Map([[1, el]]), rows = [];
    for (let n = 0; n < 8; n++) {
      const row = document.createElement('div'); row.style.cssText = 'width:100px;height:100px;flex:none';
      row.dataset.view = String(n + 2); el.append(row); views.set(n + 2, row);
      rows.push({view: n + 2, root: n + 2, index: n, start: n * 114, size: 100, epoch: '1'});
    }
    let now = 1000;
    const frames = [], limits = [];
    const controller = collectionController({root, views, now: () => now,
      requestFrame(fn) { frames.push(fn); return frames.length; }, cancelFrame() {},
      report(bytes) { limits.push(new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(88, true)); },
    });
    controller.commit([{view: 1, axis: 'x', revision: '1', scrollSequence: '0', count: 100,
      totalExtent: 11386, rows, pending: false, correction: null}]);
    for (const left of [10, 20]) {
      now += 16; el.scrollLeft = left; el.dispatchEvent(new Event('scroll'));
      frames.splice(0).forEach(fn => fn());
    }
    controller.dispose(); return limits;
  });
  expect(limits.at(-1)).toBe(1);
  await page.close();
}, 60_000);

test('a collapsed gapped card regains its estimate on invalidation', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const width = rt.sig(0);
    const own = rt.scope(() => list.vl(el, rt.sig([0]), n => n(), w => {
      const card = document.createElement('div'); card.style.height = '100px'; w.append(card);
      rt.S(card, 'width', 'px', width);
    }, {x:true, est:100, gap:14, pw:[320], ph:[100], inputs:() => [width()]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    const collapsed = {extent:el.$list.index.total, mounted:el.$list.mounted.length};
    rt.commit(() => rt.W(width, 100)); rt.settled(); await exact.lists.settle();
    const grown = {extent:el.$list.index.total, mounted:el.$list.mounted.length};
    rt.end(own); rt.settled(); return {collapsed, grown};
  });
  expect(result).toEqual({collapsed:{extent:0, mounted:0}, grown:{extent:100, mounted:1}});
  await page.close();
}, 60_000);

test('gapped zero cards regain off-window estimates on a cross-size change', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const own = rt.scope(() => list.vl(el, rt.sig(Array.from({length:100}, (_, n) => n)), n => n(), w => {
      const card = document.createElement('div'); card.style.cssText = 'width:0;height:100px'; w.append(card);
    }, {x:true, est:100, gap:14, pw:[320], ph:[100]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    for (let p = 0; p < 100; p++) {
      const moved = new Promise(resolve => el.addEventListener('scroll', resolve, {once:true}));
      const next = Math.min(el.$list.index.prefix(p), el.scrollWidth - el.clientWidth);
      if (next !== el.scrollLeft) { el.scrollLeft = next; await moved; }
      await exact.lists.settle();
    }
    const c = el.$list, before = c.index.total;
    // Stop at the invalidating report, before fresh zero measurements replace estimates.
    const f = {...c.geometry, revision:c.revision, scroll_sequence:c.geometry.scroll_sequence + 1,
      cross:200, port_cross:200, measurements:[]};
    c.feedback(f, c.prepare(f), {});
    const after = {extent:c.index.total, far:c.index.prefix(80)};
    rt.end(own); rt.settled(); return {before, after};
  });
  expect(result).toEqual({before:1386, after:{extent:11386, far:9120}});
  await page.close();
}, 60_000);

test('a gapped cards distance excludes the trailing gap', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const own = rt.scope(() => list.vl(el, rt.sig([0,1,2,3,4]), n => n(), w => {
      const card = document.createElement('div'); card.style.cssText = 'width:100px;height:100px'; w.append(card);
    }, {x:true, est:100, gap:14, pw:[320], ph:[100]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    const distance = el.$list.distance(0, [101, 421]);
    rt.end(own); rt.settled(); return distance;
  });
  expect(result).toEqual([true, 1]);
  await page.close();
}, 60_000);

test('hidden roots match eager layout in both collection axes', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const root = document.getElementById('exact-root'), results = [];
    for (const [x, hidden, zero] of [[false, 1, -1], [true, 1, -1], [true, 2, -1], [true, -1, 1], [true, 3, -1]]) {
      const strip = () => {
        const el = document.createElement('div');
        el.style.cssText = `width:320px;height:300px;overflow:auto;${x ? 'display:flex;gap:14px' : ''}`;
        root.append(el); return el;
      };
      const virtual = strip(), eager = strip();
      const card = (w, n) => {
        const el = document.createElement('div'); el.dataset.n = n;
        el.style.cssText = `width:${n === zero ? 0 : 100}px;height:100px;flex:none;display:${n === hidden || hidden === 3 ? 'none' : 'block'}`;
        w.append(el);
      };
      for (let n = 0; n < 3; n++) card(eager, n);
      const own = rt.scope(() => list.vl(virtual, rt.sig([0,1,2]), n => n(), (w, n) => card(w, n()),
        {x, est:100, gap:x ? 14 : 0, pw:[320], ph:[300]}));
      rt.settled(); await exact.lists.pending(); await exact.lists.settle();
      const start = el => {
        const a = el.getBoundingClientRect(), b = el.querySelector(`[data-n="${hidden === 2 ? 1 : 2}"]`).getBoundingClientRect();
        return x ? b.left - a.left : b.top - a.top;
      };
      results.push({virtual:hidden === 3 ? 0 : start(virtual), eager:hidden === 3 ? 0 : start(eager), extent:virtual.$list.index.total});
      rt.end(own); rt.settled();
    }
    return results;
  });
  expect(result).toEqual([{virtual:100, eager:100, extent:200}, {virtual:114, eager:114, extent:214}, {virtual:114, eager:114, extent:214}, {virtual:128, eager:128, extent:228}, {virtual:0, eager:0, extent:0}]);
  await page.close();
}, 60_000);

test('cross-size invalidation does not anchor remeasured card widths', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const own = rt.scope(() => list.vl(el, rt.sig(Array.from({length:100}, (_, n) => n)), n => n(), w => {
      const card = document.createElement('div'); card.style.cssText = 'width:100px;height:100px'; w.append(card);
    }, {x:true, est:100, gap:14, pw:[320], ph:[100]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    const moved = new Promise(resolve => el.addEventListener('scroll', resolve, {once:true}));
    el.scrollLeft = 500; await moved; await exact.lists.settle();
    const c = el.$list;
    const report = measurements => {
      const f = {...c.geometry, revision:c.revision, scroll_sequence:c.geometry.scroll_sequence + 1,
        offset:500, cross:200, port_cross:200, measurements};
      c.feedback(f, c.prepare(f), {});
    };
    report([]);
    report(c.mounted.map(m => ({view:m.view, epoch:m.epoch, size:150})));
    const offset = c.correction?.offset ?? 500;
    rt.end(own); rt.settled(); return offset;
  });
  expect(result).toBe(500);
  await page.close();
}, 60_000);

test('gap-inclusive measurement noise preserves zero-to-positive changes', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const own = rt.scope(() => list.vl(el, rt.sig([0,1]), n => n(), w => {
      const card = document.createElement('div'); card.style.cssText = 'width:100px;height:100px'; w.append(card);
    }, {x:true, est:100, gap:14, pw:[320], ph:[100]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    const c = el.$list, m = c.mounted[0], byView = new Map([[m.view, 0]]), sizes = [];
    for (const size of [0, 0.005, 0]) {
      c.measure(byView, {view:m.view, epoch:m.epoch, size});
      sizes.push([c.index.h[0], c.zeros.has(m.key)]);
    }
    const refused = [];
    for (const size of [-1, NaN, Infinity]) {
      try { c.measure(byView, {view:m.view, epoch:m.epoch, size}); refused.push(false); }
      catch { refused.push(true); }
    }
    rt.end(own); rt.settled(); return {sizes, refused};
  });
  expect(result).toEqual({sizes:[[14, true], [14.005, false], [14, true]], refused:[true, true, true]});
  await page.close();
}, 60_000);

test('scroll anchors inside gaps skip cards before the port', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const own = rt.scope(() => list.vl(el, rt.sig(Array.from({length:10}, (_, n) => n)), n => n(), w => {
      const card = document.createElement('div'); card.style.cssText = 'width:100px;height:100px'; w.append(card);
    }, {x:true, est:100, gap:14, pw:[320], ph:[100]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    el.scrollLeft = 105;
    const anchor = el.$list.scrollAnchor();
    const c = el.$list, m = c.mounted.find(m => m.position === 1);
    m.wrapper.firstElementChild.style.width = '0px';
    await exact.lists.settle();
    el.scrollLeft = 105;
    const zero = c.scrollAnchor();
    rt.end(own); rt.settled(); return {anchor, zero};
  });
  expect(result).toEqual({anchor:['1', -9], zero:['2', -23]});
  await page.close();
}, 60_000);

test('hidden horizontal cards reappear after content changes', async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const hidden = rt.sig(1);
    const own = rt.scope(() => list.vl(el, rt.sig([0,1,2]), n => n(), (w, n) => {
      const card = document.createElement('div'); card.style.cssText = 'width:100px;height:100px'; w.append(card);
      rt.S(card, 'display', 'str', () => n() === hidden() ? 'none' : 'block');
    }, {x:true, est:100, gap:14, pw:[320], ph:[100], inputs:() => [hidden()]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    const extents = [el.$list.index.total];
    for (const n of [-1, 2, 0, -1]) {
      rt.commit(() => rt.W(hidden, n)); rt.settled(); await exact.lists.settle();
      extents.push(el.$list.index.total);
    }
    rt.end(own); rt.settled(); return extents;
  });
  expect(result).toEqual([214, 328, 214, 214, 328]);
  await page.close();
}, 60_000);

for (const mode of ['zero invalidation', 'hidden invalidation', 'first hidden', 'gap anchor', 'hidden spread']) test(`horizontal geometry: ${mode}`, async () => {
  const page = await suite.newPage();
  await page.goto(`http://127.0.0.1:${server.port}`);
  const result = await page.evaluate(async mode => {
    const rt = await import('/rt.js'), list = await import('/list.js');
    rt.clock.agent = true;
    const el = document.createElement('div');
    el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
    document.getElementById('exact-root').append(el);
    const color = rt.sig(0);
    const own = rt.scope(() => list.vl(el, rt.sig(Array.from({length:100}, (_, n) => n)), n => n(), (w, n) => {
      const card = document.createElement('div');
      card.style.cssText = `width:${mode === 'zero invalidation' && n() === 3 ? 0 : 100}px;height:100px;display:${n() === (mode === 'first hidden' ? 10 : mode.startsWith('hidden') ? 3 : -1) ? 'none' : 'block'}`;
      w.append(card); rt.S(card, 'background-color', 'str', () => color() ? 'blue' : 'red');
    }, {x:true, est:100, gap:14, pw:[320], ph:[100], inputs:() => [color()]}));
    rt.settled(); await exact.lists.pending(); await exact.lists.settle();
    const c = el.$list;
    let actual;
    if (mode === 'hidden spread') {
      c.index.invalidateAll(); c.index.spread(0, 5, -90); actual = c.index.h[3];
    } else if (mode === 'gap anchor') {
      const a = c.index.anchor(105, 320, false);
      c.index.setMeasured(c.index.order[0], c.index.token(c.index.order[0]), 64);
      actual = c.index.restoreAnchor(a, 320);
    } else {
      const offset = mode === 'zero invalidation' ? 500 : mode === 'first hidden' ? 1300 : 2000;
      const moved = new Promise(resolve => el.addEventListener('scroll', resolve, {once:true}));
      el.scrollLeft = offset; await moved; await exact.lists.settle();
      if (mode !== 'first hidden') {
        rt.commit(() => rt.W(color, 1)); rt.settled(); await exact.lists.settle();
      }
      actual = el.scrollLeft;
    }
    rt.end(own); rt.settled(); return actual;
  }, mode);
  expect(result).toBe({'zero invalidation':500, 'hidden invalidation':2114, 'first hidden':1186, 'gap anchor':55, 'hidden spread':0}[mode]);
  await page.close();
}, 60_000);

for (const mode of ['boundaries', 'tail', 'off-window reveal', 'collapsed reveal']) test(`hidden row model: ${mode}`, async () => {
  const page = await suite.newPage();
  try {
    await page.goto(`http://127.0.0.1:${server.port}`);
    const result = await page.evaluate(async mode => {
      const rt = await import('/rt.js'), list = await import('/list.js');
      rt.clock.agent = true;
      const el = document.createElement('div');
      el.style.cssText = 'display:flex;gap:14px;width:320px;height:100px;overflow:auto';
      document.getElementById('exact-root').append(el);
      const shown = rt.sig(false), events = [0, 0], count = mode === 'boundaries' ? 5 : 1000;
      const own = rt.scope(() => list.vl(el, rt.sig(Array.from({length:count}, (_, n) => n)), n => n(), (w, n) => {
        const card = document.createElement('div');
        card.style.cssText = 'width:100px;height:100px;scroll-snap-align:start'; w.append(card);
        rt.S(card, 'display', 'str', () => (mode === 'boundaries' ? n() === 0 || n() === 4 : mode === 'tail' ? n() >= 3 : mode === 'collapsed reveal' ? !shown() : n() === 80 && !shown()) ? 'none' : 'block');
      }, {x:true, est:100, gap:14, pw:[320], ph:[100], inputs:() => [shown()], start:() => { events[0]++; }, end:() => { events[1]++; }}));
      rt.settled(); await exact.lists.pending(); await exact.lists.settle();
      const c = el.$list;
      for (let pass = 0; c.pending && pass < 1000; pass++) await exact.lists.settle();
      if (mode === 'collapsed reveal') {
        const before = c.index.total;
        rt.commit(() => rt.W(shown, true)); rt.settled(); await exact.lists.settle();
        exact.lists.into(c.view, 500, 'start', 'start'); await exact.lists.settle();
        const result = {before, extent:c.index.total, far:el.scrollLeft};
        rt.end(own); rt.settled(); return result;
      }
      if (mode === 'off-window reveal') {
        const move = async left => {
          const moved = new Promise(resolve => el.addEventListener('scroll', resolve, {once:true}));
          el.scrollLeft = left; await moved; await exact.lists.settle();
        };
        await move(9000); await move(0);
        const before = c.index.h[80];
        rt.commit(() => rt.W(shown, true)); rt.settled(); await exact.lists.settle();
        const result = {before, after:c.index.h[80], extent:c.index.total, mounted:c.mounted.some(m => m.position === 80)};
        rt.end(own); rt.settled(); return result;
      }
      const result = {extent:c.index.total, pending:c.pending, ...(mode === 'boundaries' ? {events, anchor:c.index.anchor(105, 100, false).row} : {})};
      rt.end(own); rt.settled(); return result;
    }, mode);
    expect(result).toEqual(mode === 'boundaries' ? {extent:328, pending:false, events:[1,1], anchor:2}
      : mode === 'tail' ? {extent:328, pending:false}
      : mode === 'collapsed reveal' ? {before:0, extent:113986, far:57000}
      : {before:0, after:114, extent:113986, mounted:false});
  } finally { await page.close(); }
}, 60_000);

test('hidden rows are absent from browser snap areas and the presented window', async () => {
  const page = await suite.newPage();
  try {
    await page.goto(`http://127.0.0.1:${server.port}`);
    await page.evaluate(async () => {
      const rt = await import('/rt.js'), list = await import('/list.js');
      rt.clock.agent = true;
      const el = document.createElement('div'); el.id = 'snapping';
      el.style.cssText = 'display:flex;gap:14px;width:100px;height:100px;overflow:auto;scroll-snap-type:x mandatory';
      document.getElementById('exact-root').append(el);
      rt.scope(() => list.vl(el, rt.sig([0,1,2,3]), n => n(), (w, n) => {
        const card = document.createElement('div');
        card.style.cssText = `width:100px;height:100px;scroll-snap-align:start;display:${n() === 1 ? 'none' : 'block'}`;
        w.append(card);
      }, {x:true, est:100, gap:14, pw:[100], ph:[100]}));
      rt.settled(); await exact.lists.pending(); await exact.lists.settle();
      el.scrollLeft = 105;
    });
    await page.waitForFunction(() => document.getElementById('snapping').scrollLeft === 114);
    expect(await page.evaluate(async () => {
      await exact.lists.settle();
      const c = document.getElementById('snapping').$list;
      return {anchor:c.scrollAnchor(), rows:c.snapshot().rows.map(r => r.index), hiddenBox:c.mounted.find(m => m.position === 1)?.wrapper.getClientRects().length ?? 0};
    })).toEqual({anchor:['2', 0], rows:[0,2,3], hiddenBox:0});
  } finally { await page.close(); }
}, 60_000);
