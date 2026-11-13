import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';
import { test, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { open } from '../../scripts/agent.mjs';
import { perfDelta } from '../../scripts/agent-inspect.mjs';

function runtimeServer(development = false, names = [[], [], []], types = [[], [], []], map = null) {
  const bytes = JSON.stringify(map);
  return Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') { return new Response(null, { status: 204 }); }
    if (path === '/exact.json') return Response.json({ plan: { sha256: map?.digest }, dev: { sourceMap: {
      url: '/app.plan.map.json', bytes: Buffer.byteLength(bytes), sha256: createHash('sha256').update(bytes).digest('hex') } } });
    if (path === '/app.plan.map.json') return new Response(bytes);
    if (path.endsWith('/names.js')) { return new Response(`export default ${JSON.stringify(names)}; export const types = ${JSON.stringify(types)}, sourceTypes = {};`, { headers: { 'content-type': 'text/javascript' } }); }
    if (path === '/') { return new Response('<div id="exact-root" data-boot-ms="0"></div><script>globalThis.exact = {};</script>', { headers: { 'content-type': 'text/html' } }); }
    const local = new URL('../..' + path, import.meta.url), file = existsSync(local) ? local : new URL('../web/' + path.split('/').pop(), import.meta.url);
    if (path.endsWith('/rt.js') && development) { return new Response(readFileSync(file, 'utf8').replace('const DEVELOPMENT_STORE = null;', 'const DEVELOPMENT_STORE = "fixture";'), { headers: { 'content-type': 'text/javascript' } }); }
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
}

test('agent diagnostics distinguish list members through reordering, removal and recovery', async () => {
  const location = { file: 'app.contract', line: 7, col: 3, end_col: 8, component: 'App' };
  const map = { digest: 'ab'.repeat(32), nodes: [], resources: { items: location, single: { ...location, line: 8 } } };
  const server = runtimeServer(true, [[], [], ['items', 'single']], [[], [], [['[', 's'], 's']], map);
  let browser;
  try {
    browser = await open({ host: 'web', url: server.url.href });
    const { evaluate } = browser.carrier;
    await evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      const { install } = await import('/host/web-js/agent.js');
      const ids = rt.sig(['current', 'retained', 'failed', 'pending']), completions = new Map();
      rt.clock.agent = true;
      rt.data.answer = (source, args) => {
        const completion = Promise.withResolvers();
        completions.set(args[0] ?? source, completion);
        return { promise: completion.promise };
      };
      let items, single;
      rt.mount(root => {
        items = resEach('items', 'item', ids, x => x, x => [x], '[s', 's', '');
        single = rt.res('single', 'single', () => [], undefined, undefined, 's', '');
        rt.hd(root, { headTitle: () => items().join(',') + single() });
      });
      const exact = globalThis.exact;
      Object.assign(exact, { plan: 'ab'.repeat(32), clock: rt.clock, After: rt.After, views: rt.Views, viewId: rt.viewId,
        resources: rt.Resources, mutations: rt.Mutations, state: [[], [], [items, single]] });
      install(exact);
      window.fixture = { rt, ids, items, completions,
        change: value => rt.commit(() => rt.W(ids, value)),
        finish: (key, error) => {
          const completion = completions.get(key); completions.delete(key);
          if (error) completion.reject(new Error(error)); else completion.resolve(key + ' answer');
          return completion.promise.catch(() => {});
        },
        pending: () => rt.Resources.filter(r => r.ticket).map(r => r.asked[0] ?? r.name) };
    });
    const state = () => browser.state();
    const finish = async (key, error = null) => {
      await evaluate(`fixture.finish(${JSON.stringify(key)}, ${JSON.stringify(error)})`);
      expect(await evaluate(() => fixture.pending())).not.toContain(key);
    };
    await finish('current');
    await finish('retained');
    await evaluate(() => fixture.rt.commit(() => fixture.rt.R(fixture.items)));
    await finish('current');
    await finish('retained', 'retained error');
    await finish('failed', 'fallback error');
    await finish('single', 'singleton error');
    const before = await state(), statuses = Object.entries(before.resourceStatus);
    expect(statuses).toHaveLength(5);
    for (const [key, status] of statuses) expect(status.declaration).toEqual({
      status: 'compatible', digest: map.digest, ...(key === 'single' ? map.resources.single : location) });
    const keyFor = test => statuses.find(([, status]) => status.resource === 0 && test(status))?.[0];
    const current = keyFor(s => s.displayed === 'current'), retained = keyFor(s => s.displayed === 'retained');
    const failed = keyFor(s => s.displayed === 'fallback' && !s.pending), pending = keyFor(s => s.pending);
    expect(new Set([current, retained, failed, pending]).size).toBe(4);
    expect([current, retained, failed, pending].every(k => typeof k === 'string' && k.startsWith('items['))).toBe(true);
    expect(before.failed).toEqual({ [retained]: 'retained error', [failed]: 'fallback error', single: 'singleton error' });
    expect(before.failures).toEqual(Object.fromEntries([retained, failed, 'single'].map(k => [k, { count: 1, first: 0, last: 0 }])));
    expect(before.resourceStatus.single).toMatchObject({ resource: 1, source: 'single', displayed: 'fallback', pending: false });
    const perfBefore = await browser.perf();
    expect(perfBefore.resources.find(r => r.name === 'single')).toEqual({ name: 'single', asked: 1 });
    expect(perfBefore.resources.filter(r => r.name === 'items').map(r => r.member).sort()).toEqual([current, retained, failed, pending].sort());
    await evaluate(() => fixture.change(['pending', 'failed', 'retained', 'current', 'failed']));
    const reordered = await state();
    expect(reordered.resourceStatus).toEqual(before.resourceStatus);
    expect(reordered.failed).toEqual(before.failed);
    expect(reordered.failures).toEqual(before.failures);
    expect(perfDelta(perfBefore, await browser.perf()).resources).toEqual([]);
    await evaluate(() => fixture.change(['pending', 'retained', 'current', 'new']));
    const removed = await state();
    expect(removed.resourceStatus[failed]).toBeUndefined();
    expect(removed.failed[failed]).toBeUndefined();
    expect(removed.failures[failed]).toBeUndefined();
    for (const k of [current, retained, pending, 'single']) expect(removed.resourceStatus[k]).toEqual(before.resourceStatus[k]);
    expect(Object.keys(removed.resourceStatus)).toHaveLength(5);
    const perfRemoved = await browser.perf();
    expect(perfRemoved.resources.some(r => r.member === failed)).toBe(false);
    expect(perfDelta(perfBefore, perfRemoved).resources).toEqual([
      { name: 'items', member: Object.keys(removed.resourceStatus).find(k => !before.resourceStatus[k]), asked: 1 },
    ]);
    await finish('new', 'new member error');
    expect(Object.values((await state()).failed).sort()).toEqual(['new member error', 'retained error', 'singleton error']);
    await evaluate(() => fixture.rt.commit(() => fixture.rt.R(fixture.items)));
    await finish('retained');
    const recovered = await state();
    expect(recovered.resourceStatus[retained]).toMatchObject({ resource: 0, displayed: 'current', pending: false });
    expect(recovered.failed[retained]).toBeUndefined();
    expect(recovered.failures[retained]).toEqual(before.failures[retained]);
    const perfRecovered = await browser.perf();
    await evaluate(() => fixture.rt.commit(() => fixture.items.r.members().find(r => r.asked[0] === 'current').force()));
    await finish('current');
    expect(perfDelta(perfRecovered, await browser.perf()).resources).toEqual([{ name: 'items', member: current, asked: 1 }]);
    await evaluate(() => fixture.change([]));
    const empty = await state();
    expect(empty.resourceStatus).toEqual({ single: before.resourceStatus.single });
    expect(empty.failed).toEqual({ single: 'singleton error' });
    expect(empty.failures).toEqual({ single: before.failures.single });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['resource', 'svg'])('departed %s scopes release their memo closures', async kind => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async kind => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      const { resEach } = await import('/host/web-js/each.js');
      const { svgGeometry } = await import('/host/web-js/svg-transform.js');
      const ids = rt.sig([0]), width = rt.sig(4), refs = [];
      rt.clock.agent = true;
      rt.data.answer = (_, [id]) => ({ v: String(id).padEnd(1024, 'x') });
      let items;
      rt.mount(root => {
        if (kind === 'resource') items = resEach('items', 'item', ids, x => x, x => [x], '[s', 's', '');
        else rt.each(root, ids, id => id(), parent => {
          const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
          parent.append(svg);
          const held = { svg };
          refs.push(new WeakRef(held));
          svgGeometry(svg, 'x', () => (held.svg.localName === 'svg' ? width() : 0));
        });
      });
      const remember = () => { if (items) refs.push(new WeakRef(items.r.members()[0])); };
      remember();
      for (let i = 1; i <= 40; i++) {
        rt.commit(() => rt.W(ids, [i]));
        remember();
      }
      rt.commit(() => rt.W(width, 9));
      window.fixture = { rt, ids, refs, items };
    }, kind);
    expect(await page.evaluate(kind => kind === 'resource'
      ? fixture.items().map(v => v.slice(0, 2))
      : [...document.querySelectorAll('svg')].map(e => e.getAttribute('x')), kind))
      .toEqual(kind === 'resource' ? ['40'] : ['9']);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('HeapProfiler.collectGarbage');
    expect(await page.evaluate(() => fixture.refs.filter(r => r.deref()).length)).toBe(1);
    await page.evaluate(() => fixture.rt.commit(() => fixture.rt.W(fixture.ids, [])));
    // The next commit releases the previous commit's completion callbacks.
    await page.evaluate(() => fixture.rt.commit(() => {}));
    await cdp.send('HeapProfiler.collectGarbage');
    expect(await page.evaluate(() => fixture.refs.filter(r => r.deref()).length)).toBe(0);
  } finally { await browser.close(); server.stop(true); }
});

test.each(['message', 'beforeunload', 'midimessage'])('%s registrations end with their scope', async kind => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async kind => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      rt.clock.agent = true;
      const { commitGuestOrigin } = await import('/host/web-js/navigation.js');
      const listeners = new Set(), add = window.addEventListener, remove = window.removeEventListener;
      window.addEventListener = function(type, f, options) {
        if (type === kind) listeners.add(f);
        return add.call(this, type, f, options);
      };
      window.removeEventListener = function(type, f, options) {
        if (type === kind) listeners.delete(f);
        return remove.call(this, type, f, options);
      };
      const bind = { message: rt.onMessage, beforeunload: rt.onUnload, midimessage: rt.onMIDI }[kind];
      const size = () => kind === 'midimessage' ? rt.Sounds.midi.size : listeners.size;
      let heard = 0, el, live;
      for (let i = 0; i < 41; i++) {
        if (live) rt.end(live);
        el = document.createElement(kind === 'message' ? 'iframe' : 'div');
        if (kind === 'message') el.setAttribute('sandbox', 'allow-scripts');
        document.body.append(el);
        if (kind === 'message') commitGuestOrigin(el);
        live = rt.scope(() => rt.on(el, kind, () => heard++, bind));
      }
      const mounted = size();
      if (kind === 'message') window.dispatchEvent(new MessageEvent(kind, { source: el.contentWindow, origin: 'null', data: 'hello' }));
      else if (kind === 'beforeunload') window.dispatchEvent(new Event(kind));
      else for (const [, f] of rt.Sounds.midi) f(['input', [144, 60, 127], 0, -1]);
      rt.end(live);
      // Exit presentation may keep the DOM connected after its scope ends.
      return { mounted, ended: size(), heard, connected: el.isConnected };
    }, kind);
    expect(result).toEqual({ mounted: 1, ended: 0, heard: 1, connected: true });
  } finally { await browser.close(); server.stop(true); }
});

test.each([true, false])('writes over list members retire independently (overlay: %s)', async overlay => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async overlay => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      const { answers } = await import('/host/web-js/checkpoint.js');
      const urls = rt.sig(['a', 'b']), saved = rt.sig(null), refusing = rt.sig(false), completions = [], overlaid = [];
      const options = { later: false, keep: false, showLanded: false };
      rt.clock.agent = true;
      rt.data.answer = (source, args) => source === 'look' && !options.later ? { v: 'server' }
        : { promise: new Promise(resolve => completions.push({ key: source === 'save' ? 'save' : args[0], resolve })) };
      rt.data.overlay = !overlay ? undefined : (_, args, answer, writes) => {
        overlaid.push(args[0]);
        const keep = writes.filter(w => options.keep && args[0] === 'b' && w.answered).map(w => w.id);
        return { value: options.showLanded || keep.length || writes.some(w => !w.answered) ? 'writing' : answer, keep };
      };
      let got, one, mutation;
      rt.mount(root => {
        got = resEach('got', 'look', urls, x => x, x => [x], '[s', 's', '');
        one = rt.res('one', 'look', () => ['z'], undefined, undefined, 's', '');
        mutation = rt.mut('saved', saved, [got, one], 'b');
        rt.memo(() => { if (refusing()) throw new rt.Refusal('refused removal'); });
        rt.hd(root, { headTitle: () => got().join(',') + one() });
      });
      window.fixture = { rt, got, options, overlaid, completions,
        snapshot: () => JSON.parse(answers([got, one], [['[', 's'], 's'], {})),
        save: () => rt.commit(() => rt.M(mutation, 'save', [])),
        change: value => rt.commit(() => rt.W(urls, value)),
        refuse: value => rt.commit(() => { rt.W(urls, value); rt.W(refusing, true); }),
        finish: key => {
          const ticket = key === 'save' ? mutation.ticket : rt.Resources.find(r => r.ticket?.args[0] === key).ticket;
          const i = completions.findLastIndex(c => c.key === key);
          completions.splice(i, 1)[0].resolve(key === 'save' ? true : 'fresh');
          return ticket.id;
        },
        state: () => ({ got: got(), one: one(), writes: rt.writeRecords().length }) };
    }, overlay);
    const finish = async key => {
      const ticket = await page.evaluate(key => fixture.finish(key), key);
      await page.waitForFunction(ticket => [...fixture.rt.Resources, ...fixture.rt.Mutations].every(r => r.ticket?.id !== ticket), ticket);
    };
    const checkpoint = [['got', 'look', [], ['server', 'server']], ['one', 'look', ['z'], 'server']];
    for (const showLanded of [false, true, false]) {
      await page.evaluate(showLanded => { fixture.options.showLanded = showLanded; fixture.overlaid.length = 0; fixture.save(); }, showLanded);
      const shown = overlay ? 'writing' : 'server';
      expect(await page.evaluate(() => fixture.state())).toEqual({ got: [shown, shown], one: shown, writes: 1 });
      expect(await page.evaluate(() => fixture.snapshot())).toEqual(checkpoint);
      expect(await page.evaluate(() => fixture.overlaid.sort())).toEqual(overlay ? ['a', 'b', 'z'] : []);
      expect(await page.evaluate(() => fixture.completions.map(c => c.key))).toEqual(['save']);
      await finish('save');
      const settled = overlay && showLanded ? 'writing' : 'server';
      expect(await page.evaluate(() => fixture.state())).toEqual({ got: [settled, settled], one: settled, writes: 0 });
      expect(await page.evaluate(() => fixture.snapshot())).toEqual(checkpoint);
    }
    if (!overlay) return;
    await page.evaluate(() => {
      fixture.options.later = fixture.options.keep = true;
      fixture.save(); fixture.change(['b', 'a', 'c', 'b']);
    });
    await finish('c');
    expect(await page.evaluate(() => fixture.snapshot())).toEqual([
      ['got', 'look', [], ['server', 'server', 'fresh', 'server']], ['one', 'look', ['z'], 'server'],
    ]);
    await finish('save');
    await finish('a');
    expect(await page.evaluate(() => fixture.state())).toEqual({ got: ['writing', 'fresh', 'writing', 'writing'], one: 'writing', writes: 1 });
    await finish('b');
    await finish('c');
    await finish('z');
    expect(await page.evaluate(() => fixture.state())).toEqual({ got: ['writing', 'fresh', 'fresh', 'writing'], one: 'fresh', writes: 1 });
    expect(await page.evaluate(() => fixture.refuse(['b', 'd']))).toBe(false);
    expect(await page.evaluate(() => fixture.state())).toEqual({ got: ['writing', 'fresh', 'fresh', 'writing'], one: 'fresh', writes: 1 });
    await page.evaluate(() => fixture.change(['b', 'd']));
    expect(await page.evaluate(() => fixture.state())).toEqual({ got: ['writing', 'writing'], one: 'fresh', writes: 1 });
    await finish('d');
    expect(await page.evaluate(() => fixture.refuse([]))).toBe(false);
    expect(await page.evaluate(() => fixture.state())).toEqual({ got: ['writing', 'fresh'], one: 'fresh', writes: 1 });
    await page.evaluate(() => fixture.change(['d']));
    expect(await page.evaluate(() => fixture.state())).toEqual({ got: ['fresh'], one: 'fresh', writes: 0 });
    await page.evaluate(() => { fixture.change([]); fixture.options.later = false; fixture.save(); });
    await finish('save');
    expect(await page.evaluate(() => fixture.state())).toEqual({ got: [], one: 'server', writes: 0 });
  } finally { await browser.close(); server.stop(true); }
}, 30000);

test.each([false, true])('list members ask independently of the aggregate checkpoint (arguments: %s)', async withArgs => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const boot = await page.evaluate(async withArgs => {
      globalThis.__exactRender = true;
      const cp = document.createElement('script');
      cp.type = 'application/vnd.exact.checkpoint';
      cp.textContent = JSON.stringify({ answers: [
        ['items', 'item', [], [{ r: ['rendered 1'] }, { r: ['rendered 2'] }]],
        ['title', 'title', [], 'Hello'],
      ], pending: [] });
      document.head.append(cp);
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      const { answers } = await import('/host/web-js/checkpoint.js');
      const requests = [], completions = [];
      rt.data.answer = (source, args) => {
        requests.push([source, args]);
        return { promise: new Promise(resolve => completions.push(resolve)) };
      };
      let items, title;
      rt.mount(root => {
        items = resEach('items', 'item', () => [1, 2], x => x, x => withArgs ? [x] : [], '[{s}', '{s}', ['']);
        title = rt.res('title', 'title', () => [], undefined, undefined, 's', '');
        rt.hd(root, { headTitle: () => title() + items().map(row => row[0]).join(',') });
      });
      const snapshot = () => JSON.parse(answers([items, title], [['[', { text: 's' }], 's'], {}));
      window.fixture = { rt, items, title, completions, snapshot };
      return { requests, items: items(), pending: items.p(), title: title(), answers: snapshot() };
    }, withArgs);
    expect(boot).toEqual({ requests: [['item', withArgs ? [1] : []], ['item', withArgs ? [2] : []]],
      items: [[''], ['']], pending: true, title: 'Hello', answers: [['title', 'title', [], 'Hello']] });
    await page.evaluate(() => fixture.completions.forEach((resolve, i) => resolve([`fresh ${i + 1}`])));
    await page.waitForFunction(() => fixture.rt.inflight.n === 0);
    expect(await page.evaluate(() => [fixture.items(), fixture.items.p(), fixture.title()]))
      .toEqual([[['fresh 1'], ['fresh 2']], false, 'Hello']);
    expect(await page.evaluate(() => fixture.snapshot())).toEqual([
      ['items', 'item', [], [{ r: ['fresh 1'] }, { r: ['fresh 2'] }]], ['title', 'title', [], 'Hello'],
    ]);
  } finally { await browser.close(); server.stop(true); }
}, 30000);

test('resource observations keep request provenance through switching, failure, rollback and recovery', async () => {
  for (const development of [true, false]) {
    const server = runtimeServer(development);
    const browser = await suiteBrowser(), page = await browser.newPage();
    try {
      await page.goto(server.url.href);
      await page.evaluate(async () => {
        const rt = await import('/host/web-js/rt.js'), owner = rt.sig('A'), completions = [];
        rt.clock.agent = true;
        rt.data.answer = () => ({ promise: new Promise(resolve => completions.push(resolve)) });
        const feed = rt.res('feed', 'feed', () => [owner()], undefined, undefined, 's', 'empty');
        window.fixture = { rt, owner, feed, completions,
          inspect: () => feed.r.observation?.(0),
          pick: v => rt.commit(() => rt.W(owner, v)),
          refresh: () => rt.commit(() => rt.R(feed)),
          finish: (i, value) => completions[i](value) };
        rt.commit(() => {});
      });
      const inspect = () => page.evaluate(() => fixture.inspect());
      const finish = async (index, value) => {
        await page.evaluate(([i, v]) => fixture.finish(i, v), [index, value]);
        await page.waitForFunction(() => fixture.rt.inflight.n === 0);
      };
      if (development) expect(await inspect()).toMatchObject({ displayed: 'fallback', pending: true, argumentsEqual: null });
      else expect(await inspect()).toBeUndefined();
      await finish(0, 'A rows');
      const first = await inspect();
      if (development) expect(first).toMatchObject({ displayed: 'current', pending: false, argumentsEqual: true });
      await page.evaluate(() => fixture.pick('B'));
      if (development) expect(await inspect()).toMatchObject({ displayed: 'retained', pending: true, argumentsEqual: false, displayedGeneration: first.displayedGeneration });
      await finish(1, 42);
      if (development) {
        expect(await inspect()).toMatchObject({ displayed: 'retained', pending: false, argumentsEqual: false });
        expect(await page.evaluate(() => fixture.feed.r.failures.count)).toBe(1);
      }
      await page.evaluate(() => fixture.pick('A'));
      if (development) expect(await inspect()).toMatchObject({ displayed: 'retained', pending: false, argumentsEqual: true });
      await page.evaluate(() => fixture.refresh());
      await finish(2, 'A fresh rows');
      const recovered = await inspect();
      if (development) expect(recovered.displayed).toBe('current');
      await page.evaluate(() => fixture.refresh());
      await finish(3, 42);
      if (development) {
        expect(await inspect()).toMatchObject({ displayed: 'retained', argumentsEqual: true, lastSuccessfulGeneration: recovered.displayedGeneration });
        expect(await page.evaluate(() => fixture.feed.r.failures.count)).toBe(2);
      }
      const before = await inspect();
      await page.evaluate(() => fixture.rt.commit(() => { fixture.rt.W(fixture.owner, 'C'); throw new fixture.rt.Refusal('refused'); }));
      expect(await inspect()).toEqual(before);
      expect(await page.evaluate(() => fixture.feed())).toBe('A fresh rows');
      await page.evaluate(() => { fixture.pick('B'); fixture.pick('C'); fixture.finish(4, 42); fixture.finish(5, 'C rows'); });
      await page.waitForFunction(() => fixture.rt.inflight.n === 0);
      if (development) {
        expect(await inspect()).toMatchObject({ displayed: 'current', argumentsEqual: true });
        expect(await page.evaluate(() => fixture.feed.r.failures.count)).toBe(2);
      }
      expect(await page.evaluate(() => fixture.feed())).toBe('C rows');
    } finally { await browser.close(); server.stop(true); }
  }
}, 30000);

test.each(['reply', 'message'])('a resource %s refused by a derive fails without losing its last value', async kind => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async kind => {
      const rt = await import('/host/web-js/rt.js'), answers = [];
      rt.clock.agent = true;
      rt.data.answer = () => kind === 'reply'
        ? { promise: new Promise(resolve => answers.push({ finish: resolve })) }
        : { stream: (deliver, controller) => {
          answers.push({ finish: v => deliver({ v }), controller });
          return new Promise(() => {});
        } };
      let count, share;
      rt.mount(root => {
        count = rt.res('count', 'count', () => [], undefined, undefined, 'n', 0);
        share = rt.memo(() => 100 / (count() - 5), 'n', 'share');
        rt.hd(root, { headTitle: () => `${share()} ${count.p() ? 'pending' : 'settled'}` });
      });
      window.fixture = { rt, count, answers,
        finish: (i, value) => answers[i].finish(value),
        refresh: () => rt.commit(() => rt.R(count)),
        state: () => ({ value: count(), pending: count.p(), failure: count.e(),
          ticket: !!count.r.ticket, title: document.title, asks: answers.length }) };
    }, kind);
    const finish = async (i, value) => {
      await page.evaluate(([i, value]) => fixture.finish(i, value), [i, value]);
      await page.waitForFunction(() => fixture.rt.inflight.n === 0);
    };
    await finish(0, 4);
    expect(await page.evaluate(() => fixture.state())).toMatchObject({ value: 4, pending: false, title: '-100 settled' });
    await page.evaluate(() => fixture.refresh());
    await finish(1, 5);
    expect(await page.evaluate(() => fixture.state())).toEqual({
      value: 4, pending: false, ticket: false, title: '-100 settled', asks: 2,
      failure: ['error', 'derive "share": value does not conform to its type', ''],
    });
    if (kind === 'message') expect(await page.evaluate(() => fixture.answers[1].controller.signal.aborted)).toBe(true);
    await page.evaluate(() => fixture.rt.commit(() => {}));
    expect(await page.evaluate(() => fixture.answers.length)).toBe(2);
    await page.evaluate(() => fixture.refresh());
    await finish(2, 10);
    expect(await page.evaluate(() => fixture.state())).toMatchObject({ value: 10, pending: false, failure: null, title: '20 settled', asks: 3 });
  } finally { await browser.close(); server.stop(true); }
}, 30000);

test.each(['reply', 'message'])('returning to settled arguments forgets an intervening resource %s', async kind => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async kind => {
      const rt = await import('/host/web-js/rt.js'), key = rt.sig('A'), answers = [];
      rt.clock.agent = true;
      rt.data.answer = (_, [key]) => kind === 'reply'
        ? { promise: new Promise(resolve => answers.push({ key, finish: resolve })) }
        : { stream: (deliver, controller) => {
          answers.push({ key, finish: v => deliver({ v }), controller });
          return new Promise(() => {});
        } };
      let resource;
      rt.mount(root => {
        resource = rt.res('r', 'src', () => [key()], undefined, undefined, 's', '');
        rt.hd(root, { headTitle: () => `${resource()} ${resource.p() ? 'pending' : 'settled'}` });
      });
      window.fixture = { rt, resource, answers,
        pick: value => rt.commit(() => rt.W(key, value)),
        finish: (i, value) => answers[i].finish(value),
        state: () => ({ value: resource(), pending: resource.p(), title: document.title,
          ticketArgs: resource.r.ticket?.args, asks: answers.map(a => a.key) }) };
    }, kind);
    await page.evaluate(() => fixture.finish(0, 'vA'));
    await page.waitForFunction(() => fixture.rt.inflight.n === 0);
    await page.evaluate(() => { fixture.pick('B'); fixture.pick('A'); });
    const waiting = { value: 'vA', pending: true, title: 'vA pending', ticketArgs: ['A'], asks: ['A', 'B', 'A'] };
    expect(await page.evaluate(() => fixture.state())).toEqual(waiting);
    await page.evaluate(async () => { fixture.finish(1, 'vB'); await Promise.resolve(); });
    expect(await page.evaluate(() => fixture.state())).toEqual(waiting);
    if (kind === 'message') expect(await page.evaluate(() => fixture.answers[1].controller.signal.aborted)).toBe(true);
    await page.evaluate(() => fixture.rt.commit(() => {}));
    expect(await page.evaluate(() => fixture.state())).toEqual(waiting);
    await page.evaluate(() => fixture.finish(2, 'vA fresh'));
    await page.waitForFunction(() => fixture.rt.inflight.n === 0);
    expect(await page.evaluate(() => fixture.state())).toMatchObject({ value: 'vA fresh', pending: false, title: 'vA fresh settled', asks: ['A', 'B', 'A'] });
  } finally { await browser.close(); server.stop(true); }
}, 30000);

test.each([['not pending', false], ['pending', true]])('resource pending state rolls back with its request (initially %s)', async (_, initiallyPending) => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const before = await page.evaluate(async initiallyPending => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), answer = Promise.withResolvers();
      let trigger, later, bad, controller;
      rt.data.answer = (name, [value]) => {
        if (name === 'trigger') { return { promise: answer.promise }; }
        if (name === 'later' && (value === 0) === initiallyPending) {
          return { stream: (_deliver, owner) => { controller = owner; return new Promise(() => {}); } };
        }
        if (name === 'bad' && value === 1) { throw Object.assign(new Error('source refused'), { refuse: true }); }
        return { v: 0 };
      };
      rt.mount(root => {
        trigger = rt.res('trigger', 'trigger', () => [], undefined, undefined, 'n', 0);
        later = rt.res('later', 'later', () => [trigger()], undefined, undefined, 'n', 0);
        bad = rt.res('bad', 'bad', () => [trigger()], undefined, undefined, 'n', 0);
        rt.hd(root, { headTitle: () => [trigger(), later(), bad()].join(' ') });
      });
      // A heartbeat ends the transport wait while the resource still needs a value.
      controller?.replied();
      const state = () => ({ pending: later.p(), ticket: !!later.r.ticket, title: document.title });
      window.fixture = { trigger, answer, state };
      return state();
    }, initiallyPending);
    expect(before).toEqual({ pending: initiallyPending, ticket: initiallyPending, title: '0 0 0' });
    await page.evaluate(() => fixture.answer.resolve(1));
    await page.waitForFunction(() => fixture.trigger.f());
    expect(await page.evaluate(() => fixture.state())).toEqual(before);
  } finally { await browser.close(); server.stop(true); }
}, 30000);

test.each([
  ['derive', false], ['derive', true], ['source', false], ['source', true],
])('a list element first asked in a refused commit is asked again (refusal: %s, existing pending: %s)', async (refuseAt, initiallyPending) => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async refuseAt => {
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      rt.clock.agent = true;
      const ids = rt.sig([1]), bad = rt.sig(false), asked = [], answers = [];
      rt.data.answer = (_source, [n]) => {
        asked.push(n);
        if (n === 2 && bad() && refuseAt === 'source') { throw new rt.Refusal('source refused'); }
        const answer = Promise.withResolvers();
        answers.push(() => answer.resolve(n * 10));
        return { promise: answer.promise };
      };
      const items = resEach('items', 'item', ids, n => n, n => [n], '[n', 'n', 0);
      const share = rt.memo(() => 100 / (items().length + 1 - (bad() ? items().length + 1 : 0)), 'n', 'share');
      rt.commit(() => {});
      rt.effect(() => { document.getElementById('exact-root').textContent = `${items().join(',')} ${items.p() ? 'pending' : 'settled'} ${share()}`; });
      window.fixture = { rt, ids, bad, asked, answers,
        state: () => ({ shown: items(), pending: items.p(), ids: ids(), bad: bad(), share: share(), asked: [...asked] }),
        add: refuse => rt.commit(() => { rt.W(ids, [1, 2]); rt.W(bad, refuse); }) };
    }, refuseAt);
    if (!initiallyPending) {
      await page.evaluate(() => fixture.answers[0]());
      await page.waitForFunction(() => fixture.rt.inflight.n === 0);
    }
    const before = await page.evaluate(() => fixture.state());
    expect(before).toEqual({ shown: [initiallyPending ? 0 : 10], pending: initiallyPending, ids: [1], bad: false, share: 50, asked: [1] });
    expect(await page.evaluate(() => fixture.add(true))).toBe(false);
    expect(await page.evaluate(() => fixture.state())).toEqual({ ...before, asked: [1, 2] });
    expect(await page.evaluate(() => fixture.rt.Resources.length)).toBe(1);
    expect(await page.evaluate(() => fixture.add(false))).toBe(true);
    await page.evaluate(() => fixture.answers.forEach(finish => finish()));
    await page.waitForFunction(() => fixture.rt.inflight.n === 0);
    expect(await page.evaluate(() => fixture.state())).toEqual({
      shown: [10, 20], pending: false, ids: [1, 2], bad: false, share: 100 / 3, asked: [1, 2, 2],
    });
    expect(await page.locator('#exact-root').textContent()).toBe(`10,20 settled ${100 / 3}`);
  } finally { await browser.close(); server.stop(true); }
}, 30000);


test("refresh carries a Rust request's forced cache mode through runtime dispatch", async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const observed = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const sent = [];
      globalThis.fetch = async (_input, init) => { sent.push(init.cache); return new Response('reply'); };
      const rt = await import('/host/web-js/rt.js');
      const { createRequestExecutor, setAppGrantSet, appGrantSet } = await import('/host/web-js/admission.js');
      setAppGrantSet();
      rt.clock.agent = true;
      rt.data.fetch = createRequestExecutor('test.refresh', appGrantSet());
      rt.data.parse = () => ({ v: 'reply' });
      const modes = ['force-cache', 'no-store'];
      rt.data.answer = name => ({ req: { method: 'GET', url: '/assets/cache.txt', headers: [], cache: modes[name === 'a' ? 0 : 1] } });
      let a, b;
      rt.mount(root => {
        a = rt.res('a', 'a', () => [], undefined, undefined, 's', '');
        b = rt.res('b', 'b', () => [], undefined, undefined, 's', '');
        rt.hd(root, { headTitle: () => a() + b() });
      });
      while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0));
      rt.commit(() => { rt.R(a); rt.R(b); });
      while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0));
      return sent;
    });
    expect(observed).toEqual(['force-cache', 'no-store', 'reload', 'no-store']);
  } finally { await browser.close(); server.stop(true); }
});

test("the ask made when the source becomes ready is not a refresh, and a refresh asked while waiting is", async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const observed = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const sent = [], callbacks = [];
      globalThis.fetch = async (_input, init) => { sent.push(init.cache ?? 'default'); return new Response('reply'); };
      const rt = await import('/host/web-js/rt.js');
      const { createRequestExecutor, setAppGrantSet, appGrantSet } = await import('/host/web-js/admission.js');
      setAppGrantSet();
      rt.clock.agent = true;
      rt.data.fetch = createRequestExecutor('test.launch', appGrantSet());
      rt.data.parse = () => ({ v: 'reply' });
      let ready = false;
      rt.data.answer = () => ready ? { req: { method: 'GET', url: '/assets/cache.txt', headers: [] } } : null;
      rt.data.ready = f => callbacks.push(f);
      let a, b;
      rt.mount(root => {
        a = rt.res('a', 'a', () => [], undefined, undefined, 's', '');
        b = rt.res('b', 'b', () => [], undefined, undefined, 's', '');
        rt.hd(root, { headTitle: () => a() + b() });
      });
      rt.commit(() => rt.R(b)); // asked while its source is not ready
      ready = true;
      for (const f of callbacks) f();
      while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0));
      const launch = sent.splice(0).sort();
      rt.commit(() => rt.R(a));
      while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0));
      return { launch, later: sent };
    });
    expect(observed).toEqual({ launch: ['default', 'reload'], later: ['reload'] });
  } finally { await browser.close(); server.stop(true); }
});

test("a refused refresh while waiting leaves the launch's ask ordinary, and a failed launch ask leaves a later refresh one", async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const observed = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const sent = [], callbacks = [];
      globalThis.fetch = async (_input, init) => { sent.push(init.cache ?? 'default'); return new Response('reply'); };
      const rt = await import('/host/web-js/rt.js');
      const { createRequestExecutor, setAppGrantSet, appGrantSet } = await import('/host/web-js/admission.js');
      setAppGrantSet();
      rt.clock.agent = true;
      rt.data.fetch = createRequestExecutor('test.launch', appGrantSet());
      rt.data.parse = () => ({ v: 'reply' });
      let ready = false, down = true;
      rt.data.answer = name => {
        if (!ready) return null;
        if (name === 'f' && down) { down = false; throw new Error('down'); }
        return { req: { method: 'GET', url: '/assets/cache.txt', headers: [] } };
      };
      rt.data.ready = f => callbacks.push(f);
      const b = rt.sig(1);
      let a, c, f;
      rt.mount(root => {
        a = rt.res('a', 'a', () => [], undefined, undefined, 's', '');
        c = rt.res('c', 'c', () => [1 / b()], undefined, undefined, 's', '');
        f = rt.res('f', 'f', () => [], undefined, undefined, 's', '');
        rt.hd(root, { headTitle: () => a() + c() + f() });
      });
      const committed = rt.commit(() => { rt.R(a); rt.W(b, 0); }); // refused: `c`'s argument is not finite
      ready = true;
      for (const g of callbacks) g();
      while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0));
      const launch = sent.splice(0);
      rt.commit(() => rt.R(f));
      while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0));
      return { committed, launch, failed: sent };
    });
    expect(observed).toEqual({ committed: false, launch: ['default', 'default'], failed: ['reload'] });
  } finally { await browser.close(); server.stop(true); }
});

test("a refresh of a resource that slept through its source becoming ready is a refresh", async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const observed = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const sent = [], callbacks = [];
      globalThis.fetch = async (_input, init) => { sent.push(init.cache ?? 'default'); return new Response('reply'); };
      const rt = await import('/host/web-js/rt.js');
      const { createRequestExecutor, setAppGrantSet, appGrantSet } = await import('/host/web-js/admission.js');
      setAppGrantSet();
      rt.clock.agent = true;
      rt.data.fetch = createRequestExecutor('test.launch', appGrantSet());
      rt.data.parse = () => ({ v: 'reply' });
      let ready = false;
      rt.data.answer = () => ready ? { req: { method: 'GET', url: '/assets/cache.txt', headers: [] } } : null;
      rt.data.ready = f => callbacks.push(f);
      const open = rt.sig(true);
      let g;
      rt.mount(root => {
        g = rt.res('g', 'g', Object.assign(() => [], { gate: () => open() }), undefined, undefined, 's', '');
        rt.hd(root, { headTitle: () => g() });
      });
      rt.commit(() => rt.W(open, false)); // asleep before its source is ready
      ready = true;
      for (const f of callbacks) f();
      const asleep = sent.length;
      rt.commit(() => { rt.W(open, true); rt.R(g); });
      while (rt.inflight.n) await new Promise(resolve => setTimeout(resolve, 0));
      return { asleep, sent };
    });
    expect(observed).toEqual({ asleep: 0, sent: ['reload'] });
  } finally { await browser.close(); server.stop(true); }
});

test.each(['promise', 'stream', 'build', 'ready'])('a %s answer retained for previous arguments is journaled once per resource', async kind => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async kind => {
      const rt = await import('/host/web-js/rt.js'), argument = rt.sig('A'), replies = [];
      rt.clock.agent = true;
      let ready = kind !== 'ready';
      rt.data.answer = () => {
        if (!ready) return null;
        if (kind === 'stream') return { stream: deliver => new Promise(end => replies.push({ deliver, end })) };
        return { promise: new Promise(resolve => replies.push({ deliver: v => resolve(v.v) })) };
      };
      const initial = ['build', 'ready'].includes(kind);
      const answer = rt.res('answer', 'read', () => [argument()], initial ? 'A answer' : undefined,
        initial ? ['A'] : undefined, 's', 'loading');
      rt.commit(() => {});
      window.fixture = { rt, argument, answer, replies,
        pick: a => rt.commit(() => rt.W(argument, a)),
        lines: () => rt.journal.filter(l => l.includes('shows its answer for previous arguments')),
        ready: () => { ready = true; for (const f of rt.data.q.splice(0)) f(); } };
    }, kind);
    expect(await page.evaluate(() => fixture.lines())).toEqual([]);
    if (kind === 'promise' || kind === 'stream') {
      await page.evaluate(() => fixture.pick('B'));
      expect(await page.evaluate(() => fixture.lines())).toEqual([]); // a placeholder answers no previous arguments
      await page.evaluate(() => fixture.replies.at(-1).deliver({ v: 'B answer' }));
      await page.waitForFunction(() => fixture.answer() === 'B answer');
    }
    // A refusal after settlement must neither report nor spend the once-only diagnostic.
    await page.evaluate(() => {
      const { rt, argument, answer } = fixture;
      const guard = rt.memo(() => { answer(); if (argument() === 'refused') throw new rt.Refusal('refused'); return 0; });
      rt.commit(() => rt.W(argument, 'refused'));
      rt.end(guard.n);
    });
    expect(await page.evaluate(() => fixture.lines())).toEqual([]);
    await page.evaluate(() => fixture.pick('C'));
    expect(await page.evaluate(() => [fixture.answer(), fixture.answer.p()]))
      .toEqual([kind === 'promise' || kind === 'stream' ? 'B answer' : 'A answer', true]);
    expect(await page.evaluate(() => fixture.lines().map(l => l.replace(/^t=\S+ /, ''))))
      .toEqual(['resource answer shows its answer for previous arguments (LLP 1016 D3)']);
    if (kind === 'ready') await page.evaluate(() => fixture.ready());
    await page.evaluate(() => { fixture.pick('D'); fixture.rt.commit(() => {}); });
    expect(await page.evaluate(() => fixture.lines())).toHaveLength(1);
    await page.evaluate(() => fixture.replies.at(-1).deliver({ v: 'D answer' }));
    await page.waitForFunction(() => fixture.answer() === 'D answer');
    await page.evaluate(() => { fixture.pick('E'); fixture.rt.commit(() => {}); });
    expect(await page.evaluate(() => fixture.lines())).toHaveLength(1);
  } finally { await browser.close(); server.stop(true); }
});
