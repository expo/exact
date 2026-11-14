// fragment.js, the web's scroll to a fragment (LLP 1153 D5), over stand-in elements and real browser scrollers.
// A scroll by the person since navigation cancels it though the module arms later; a hold while pictures load ends at the app's own scroll, a
// newer navigation, or an entry that no longer shows the fragment; a fragment is all after the first `#`; and Back
// puts back the scrollers above the route as well as those in it, again after the browser's own scroll to the id.
import { test, expect, afterAll } from 'bun:test';
import { hostFixtureFile, glueDeclarations } from './tests/glue-fixture.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

class El {
  constructor(attrs = {}, parent = null) {
    this.attrs = new Map(Object.entries(attrs)); this.parentElement = parent; this.children = []; parent?.children.push(this);
    this.scrollTop = 0; this.scrollLeft = 0; this.scrollHeight = 0; this.clientHeight = 0; this.scrollWidth = 0; this.clientWidth = 0;
    this.listeners = []; this.aligned = 0; this.isConnected = true;
  }
  get id() { return this.attrs.get('id') ?? ''; }
  getAttribute(k) { return this.attrs.get(k) ?? null; }
  hasAttribute(k) { return this.attrs.has(k); }
  setAttribute(k, v) { this.attrs.set(k, v); }
  removeAttribute(k) { this.attrs.delete(k); }
  all() { return this.children.flatMap(c => [c, ...c.all()]); }
  querySelectorAll(sel) {
    if (sel === '*') return this.all();
    if (sel === 'img') return this.all().filter(e => e.attrs.has('img'));
    const name = sel.slice(1, -1);
    return this.all().filter(e => e.attrs.has(name));
  }
  contains(e) { for (let x = e; x; x = x.parentElement) if (x === this) return true; return false; }
  closest() { return null; }
  matches() { return false; }
  addEventListener(type, f, o) { this.listeners.push({ type, f, signal: o?.signal }); }
  fire(type) { for (const l of this.listeners) if (l.type === type && !l.signal?.aborted) l.f({}); }
  scrollIntoView() { this.aligned++; }
  scrollTo({ top, left }) { this.scrollTop = top; this.scrollLeft = left; }
  focus() {}
}
const page = new El(), root = new El({}, page), port = new El({}, root);
Object.assign(port, { scrollHeight: 4000, clientHeight: 400 });
const route = new El({ navigationKey: '1' }, port), target = new El({ id: 'moves' }, route), picture = new El({ img: '' }, route);
picture.complete = false;
// Test files share one process: the stand-in document and history are put back after this file.
const had = Object.fromEntries(['document', 'history', 'requestAnimationFrame'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]));
afterAll(() => { for (const [k, d] of Object.entries(had)) if (d) Object.defineProperty(globalThis, k, d); else delete globalThis[k]; });
globalThis.document = { scrollingElement: page, activeElement: null };
globalThis.history = { state: { exact: 0, id: 1, url: '/item/7#moves' } };
const frames = [], frame = () => { for (const f of frames.splice(0)) f(); };
globalThis.requestAnimationFrame = f => frames.push(f);
const f = await import('./fragment.js');
const lines = [], log = l => lines.push(l), person = { personScrolls: 0 };

test('a fragment is everything after the first #', () => {
  expect(f.shows(1, 'a#b', { id: 1, url: '/item/7#a#b' })).toBe(true);
  expect(f.shows(1, 'a', { id: 1, url: '/item/7#a#b' })).toBe(false);
  expect(f.shows(1, '', { id: 1, url: '/item/7' })).toBe(true);
});

test("a scroll by the person since the navigation cancels its scroll, though the module armed after it", () => {
  person.personScrolls = 1;
  f.arm(root, { top: 1, fragment: 'moves', activation: 1, since: 0 }, person);
  f.scroll(root, 1, 'moves', log, 1);
  expect([target.aligned, lines.at(-1)]).toEqual([0, 'fragment #moves: the person scrolled first; the route stays where it is']);
});

test("the hold while pictures load ends at the app's own scroll, a newer navigation, or another entry shown", () => {
  const held = ends => {
    f.arm(root, { top: 1, fragment: 'moves', activation: 2, since: person.personScrolls }, person);
    f.scroll(root, 1, 'moves', log, 2);
    route.fire('load');
    expect(target.aligned).toBe(2); // the scroll, then the hold's own
    ends();
    route.fire('load');
    expect(target.aligned).toBe(2);
    target.aligned = 0; route.listeners = [];
  };
  held(() => f.cancel());
  held(() => f.arm(root, { top: 1, fragment: 'top', activation: 3, since: person.personScrolls }, person));
  held(() => { globalThis.history.state = { exact: 1, id: 1, url: '/item/7#top' }; });
  globalThis.history.state = { exact: 0, id: 1, url: '/item/7#moves' };
  held(() => { person.personScrolls++; });
});

test('Back puts back the scrollers above the route, as those in it', () => {
  picture.complete = true;
  port.scrollTop = 120;
  globalThis.history.state = { exact: 1, id: 1, url: '/item/7#other' };
  f.arm(root, { top: 1, fragment: 'other', activation: 4, pushed: true, since: person.personScrolls }, person); // entry 0 kept as it was
  port.scrollTop = 900;
  globalThis.history.state = { exact: 0, id: 1, url: '/item/7#moves' };
  f.arm(root, { top: 1, fragment: 'moves', activation: 5, traversal: true, since: person.personScrolls }, person);
  f.scroll(root, 1, 'moves', log, 5);
  expect(port.scrollTop).toBe(120);
  port.scrollTop = 900; // the browser's own, at its next frame's layout
  frame(); frame();
  expect(port.scrollTop).toBe(120);
});

test('a deferred traversal restoration yields to authored scrolling, navigation and a changed entry', () => {
  picture.complete = true;
  globalThis.history.state = { exact: 41, id: 1, url: '/item/7#moves' };
  port.scrollTop = 123;
  globalThis.history.state = { exact: 42, id: 1, url: '/item/7#other' };
  f.arm(root, { top: 1, fragment: 'other', activation: 40, pushed: true }, person);
  for (const end of [() => f.cancel(), () => f.arm(root, { top: 1, fragment: 'other', activation: 7 }, person),
    () => { globalThis.history.state = { exact: 8, id: 1, url: '/item/7#moves' }; },
    () => { person.personScrolls++; }]) {
    globalThis.history.state = { exact: 41, id: 1, url: '/item/7#moves' };
    port.scrollTop = 900;
    f.arm(root, { top: 1, fragment: 'moves', activation: 6, traversal: true }, person);
    f.scroll(root, 1, 'moves', log, 6);
    expect(port.scrollTop).toBe(123);
    port.scrollTop = 900;
    end();
    frame(); frame();
    expect(port.scrollTop).toBe(900);
  }
});

// Run the wasm commit's property path without booting a plan or loading its unrelated capabilities.
const wasmProperties = `
const root = document.getElementById('root'), host = {}, presence = {}, views = new Map([[1, document.getElementById('port')]]);
const retiredViews = new WeakSet(), followedScrolls = new Map(), pendingScrolls = new Map(), lists = new WeakSet();
const agentMode = false, agentKeepsStore = false, log = () => {};
const prepareContexts = log, syncMedia = log, refreshSymbols = log, syncLists = log, focusAutofocus = log, positionContexts = log, markScrollDocument = log;
const collections = { restyled: log }, navigation = { project: log };
const viewFor = (_, id) => views.get(id);
${glueDeclarations(hostFixtureFile(new URL('./wasm/views.js', import.meta.url), 'utf8'), ['applyProps'])}
${glueDeclarations(hostFixtureFile(new URL('./wasm/commit.js', import.meta.url), 'utf8'), ['apply'])}
export const set = (name, value) => apply({ ops: [{ op: 'props', id: 1, set: { [name]: String(value) } }] });
`;

async function inBrowser(run, { html = '<!doctype html><main id="root"></main>', path = '/', fetch, needs = [] } = {}) {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
    const path = new URL(request.url).pathname;
    if (fetch && path === '/slow.png') return fetch(request);
    if (path === '/fragment.js' || path === '/navigation.js') {
      return new Response(hostFixtureFile(new URL(`.${path}`, import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    }
    if (path === '/wasm-properties.js') return new Response(wasmProperties, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/host/web-js/names.js') return new Response('export const sourceTypes = [];', { headers: { 'content-type': 'text/javascript' } });
    if (path.startsWith('/host/')) {
      const local = Bun.file(new URL(`../..${path}`, import.meta.url));
      const file = await local.exists() ? local : Bun.file(new URL(`../..${path.replace('/web-js/', '/web/')}`, import.meta.url));
      return await file.exists() ? new Response(hostFixtureFile(file.name), { headers: { 'content-type': 'text/javascript' } }) : new Response(null, { status: 404 });
    }
    return new Response(html, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser('chromium', { needs });
    const page = await browser.newPage();
    await page.goto(new URL(path, server.url).href, { waitUntil: 'domcontentloaded' });
    await run(page);
  } finally { await browser?.close(); server.stop(true); }
}

for (const target of ['wasm', 'js']) test(`${target}: authored offsets end traversal restoration while pictures load`, async () => {
  let finish;
  const picture = new Promise(resolve => { finish = resolve; });
  try {
    await inBrowser(async page => {
      const result = await page.evaluate(async target => {
        const f = await import('/fragment.js'), root = document.getElementById('root'), port = document.getElementById('port');
        const person = { personScrolls: 0 }, frames = async () => { for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame); };
        let set;
        if (target === 'wasm') ({ set } = await import('/wasm-properties.js'));
        else {
          const rt = await import('/host/web-js/rt.js'), values = new Map();
          for (const name of ['scrollTop', 'scrollLeft']) {
            const value = rt.sig(null); values.set(name, value);
            rt.scope(() => rt.P(port, name, value));
          }
          set = (name, value) => rt.commit(() => rt.write(values.get(name).n, value));
        }
        history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
        port.scrollTo(80, 120); await frames();
        history.replaceState({ exact: 2, id: 1, url: '/item#other' }, '');
        f.arm(root, { top: 1, fragment: 'other', pushed: true }, person);
        const offsets = [];
        for (const [name, value] of [['scrollTop', 900], ['scrollLeft', 500]]) {
          history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
          f.arm(root, { top: 1, fragment: 'moves', traversal: true }, person);
          f.scroll(root, 1, 'moves', () => {});
          await frames();
          offsets.push([port.scrollTop, port.scrollLeft]);
          set(name, value); await frames();
          offsets.push(port[name]);
        }
        // An offset ends a hold, but leaves a fragment that has not yet scrolled armed.
        f.arm(root, { top: 1, fragment: 'moves', activation: 4 }, person);
        set('scrollTop', 300);
        f.scroll(root, 1, 'moves', () => {}, 4);
        await frames();
        return { offsets, aligned: port.scrollTop, loading: document.readyState !== 'complete' };
      }, target);
      expect(result).toEqual({ offsets: [[120, 80], 900, [900, 80], 500], aligned: 700, loading: true });
    }, {
      html: '<!doctype html><main id="root"><div id="port" style="width:200px;height:200px;overflow:auto;overflow-anchor:none"><section navigationKey="1" style="width:2000px"><img src="/slow.png" style="width:10px;height:10px;display:block"><div style="height:690px"></div><div id="moves">Moves</div><div style="height:2000px"></div></section></div></main>',
      fetch: () => picture,
    });
  } finally { finish(new Response(null, { status: 404 })); }
}, 30_000);

for (const target of ['wasm', 'js']) test(`${target}: a commit that mirrors the current offsets keeps the fragment's hold and restoration`, async () => {
  let finish;
  const picture = new Promise(resolve => { finish = resolve; });
  try {
    await inBrowser(async page => {
      const result = await page.evaluate(async target => {
        const f = await import('/fragment.js'), root = document.getElementById('root'), port = document.getElementById('port');
        const person = { personScrolls: 0 }, frames = async () => { for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame); };
        let set;
        if (target === 'wasm') ({ set } = await import('/wasm-properties.js'));
        else {
          const rt = await import('/host/web-js/rt.js'), values = new Map();
          for (const name of ['scrollTop', 'scrollLeft']) {
            const value = rt.sig(null); values.set(name, value);
            rt.scope(() => rt.P(port, name, value));
          }
          set = (name, value) => rt.commit(() => rt.write(values.get(name).n, value));
        }
        const mirror = async () => { set('scrollTop', port.scrollTop); set('scrollLeft', port.scrollLeft); await frames(); };
        const img = document.querySelector('img'), out = {};
        // The picture hold: the picture's layout moves the target after the mirror.
        history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
        f.arm(root, { top: 1, fragment: 'moves', activation: 1 }, person);
        f.scroll(root, 1, 'moves', () => {}, 1);
        await mirror();
        img.style.height = '110px';
        img.dispatchEvent(new Event('load'));
        out.held = port.scrollTop;
        // Traversal restoration: the browser's own scroll after the mirror is put back.
        port.scrollTo(0, 120); await frames();
        history.replaceState({ exact: 2, id: 1, url: '/item#other' }, '');
        f.arm(root, { top: 1, fragment: 'other', pushed: true }, person);
        history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
        f.arm(root, { top: 1, fragment: 'moves', traversal: true }, person);
        f.scroll(root, 1, 'moves', () => {});
        await frames();
        await mirror();
        port.scrollTop = 900; await frames();
        out.restored = port.scrollTop;
        return out;
      }, target);
      expect(result).toEqual({ held: 800, restored: 120 });
    }, {
      html: '<!doctype html><main id="root"><div id="port" style="width:200px;height:200px;overflow:auto;overflow-anchor:none"><section navigationKey="1"><img src="/slow.png" style="width:10px;height:10px;display:block"><div style="height:690px"></div><div id="moves">Moves</div><div style="height:2000px"></div></section></div></main>',
      fetch: () => picture,
    });
  } finally { finish(new Response(null, { status: 404 })); }
}, 30_000);

test('Back and Forward keep their saved place while the document and its pictures load', async () => {
  let finish;
  const picture = new Promise(resolve => { finish = resolve; });
  await inBrowser(async page => {
    await page.evaluate(async () => {
      const f = await import('/fragment.js'), { navigation } = await import('/navigation.js');
      const root = document.getElementById('root'), port = document.getElementById('port');
      let activation = 0, shown;
      const apply = url => {
        if (shown === url) return;
        shown = url;
        const op = { top: 1, url, removed: [], fragment: url.split('#')[1] ?? '', activation: ++activation, since: navigation.personScrolls };
        navigation.apply(op);
        f.arm(root, op, navigation);
        f.scroll(root, 1, op.fragment, () => {}, op.activation);
      };
      navigation.connect(root, apply, () => {});
      apply('/item/7#moves');
      const move = top => new Promise(resolve => {
        port.addEventListener('scroll', resolve, { once: true });
        port.scrollTop = top;
      });
      await move(120);
      apply('/item/7#other');
      await move(500);
      apply('/item/7#moves');
      await move(700);
      navigation.personScrolls++;
      window.travel = async delta => {
        await navigation.travel(null, delta);
        // Pass the restoration's two frames and Chrome's next layout while load is still pending.
        for (let i = 0; i < 5; i++) await new Promise(requestAnimationFrame);
      };
      await window.travel(-2);
      document.querySelector('img').style.height = '300px';
      for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame);
    });
    expect(await page.locator('#port').evaluate(el => el.scrollTop)).toBe(120);
    await page.evaluate(() => window.travel(2));
    expect(await page.locator('#port').evaluate(el => el.scrollTop)).toBe(700);
    finish(new Response(new Uint8Array([0]), { headers: { 'content-type': 'image/png' } }));
    await page.waitForLoadState('load');
    await page.evaluate(async () => { for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame); });
    expect(await page.locator('#port').evaluate(el => el.scrollTop)).toBe(700);
  }, {
    path: '/item/7#moves',
    needs: ['perf'],
    html: '<!doctype html><main id="root"><div id="port" style="height:200px;overflow:auto;overflow-anchor:none"><section navigationKey="1"><img src="/slow.png" style="width:10px;height:10px"><div style="height:400px"></div><div id="moves">Moves</div><div style="height:800px"></div><div id="other">Other</div><div style="height:2000px"></div></section></div></main>',
    fetch: () => picture,
  });
}, 30_000);

for (const mode of ['without fragments', 'scroll events', 'fragment snapshots']) {
  test(`removed routes can be collected after ${mode}`, () => inBrowser(async page => {
    expect(await page.evaluate(async mode => {
      const fragment = mode === 'without fragments' ? null : await import('./fragment.js');
      const root = document.getElementById('root');
      history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
      window.removedRoutes = [];
      for (let i = 0; i < 40; i++) {
        const route = document.createElement('section'), port = document.createElement('div');
        route.setAttribute('navigationKey', '1');
        port.style.cssText = 'height:100px;overflow:auto';
        port.innerHTML = '<div style="height:20px">Row</div>'.repeat(200);
        route.append(port); root.append(route);
        if (mode === 'fragment snapshots') {
          history.replaceState({ exact: i + 2, id: 1, url: '/item#moves' }, '');
          fragment.arm(root, { top: 1, fragment: 'moves', pushed: true }, { personScrolls: 0 });
        } else {
          await new Promise(resolve => {
            port.addEventListener('scroll', () => resolve(), { once: true });
            port.scrollTop = 100;
          });
        }
        window.removedRoutes.push(new WeakRef(route));
        route.remove();
      }
      return window.removedRoutes.length;
    }, mode)).toBe(40);
    const cdp = await page.context().newCDPSession(page);
    for (let i = 0; i < 3; i++) await cdp.send('HeapProfiler.collectGarbage');
    expect(await page.evaluate(() => window.removedRoutes.filter(ref => ref.deref()).length)).toBe(0);
    if (mode !== 'without fragments') await page.evaluate(async () => {
      const fragment = await import('./fragment.js'), root = document.getElementById('root');
      root.innerHTML = '<section navigationKey="1"></section>';
      history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
      fragment.arm(root, { top: 1, fragment: 'moves', traversal: true }, { personScrolls: 0 });
      fragment.scroll(root, 1, 'moves', () => {});
    });
  }), 30_000);
}

test('an ordinary history push discards the scroll places of the forward entries it replaces', () => inBrowser(async page => {
  expect(await page.evaluate(async () => {
    const fragment = await import('./fragment.js'), { navigation } = await import('./navigation.js');
    const root = document.getElementById('root');
    root.innerHTML = '<div style="height:100px;overflow:auto"><section navigationKey="1" style="height:4000px"></section></div>';
    const port = root.firstElementChild;
    const apply = (url, top = 1) => navigation.apply({ top, url, removed: [] });
    navigation.connect(root, url => apply(url), () => {});
    const move = top => new Promise(resolve => {
      port.addEventListener('scroll', () => resolve(), { once: true });
      port.scrollTop = top;
    });
    apply('/item');
    apply('/item#old'); await move(100);
    apply('/item#later'); await move(200);
    await navigation.travel(null, -2);
    const back = history.state.exact;
    await move(300);
    apply('/other', 2);
    port.firstElementChild.setAttribute('navigationKey', '2');
    fragment.arm(root, { top: 2, fragment: '', traversal: true }, navigation);
    fragment.scroll(root, 2, '', () => {});
    return [back, history.state.exact, port.scrollTop];
  })).toEqual([0, 1, 300]);
}), 30_000);

test('traversal restores the latest scroll event on both axes while the route is retained', () => inBrowser(async page => {
  expect(await page.evaluate(async () => {
    const fragment = await import('./fragment.js'), root = document.getElementById('root');
    root.innerHTML = '<section navigationKey="1" style="height:100px;width:100px;overflow:auto"><div style="height:4000px;width:4000px"></div></section>';
    const port = root.firstElementChild;
    const move = (top, left) => new Promise(resolve => {
      port.addEventListener('scroll', () => resolve(), { once: true });
      port.scrollTo({ top, left, behavior: 'instant' });
    });
    history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
    await move(100, 50); await move(200, 80);
    history.replaceState({ exact: 2, id: 1, url: '/item#other' }, '');
    await move(500, 160);
    history.replaceState({ exact: 1, id: 1, url: '/item#moves' }, '');
    fragment.arm(root, { top: 1, fragment: 'moves', traversal: true }, { personScrolls: 0 });
    fragment.scroll(root, 1, 'moves', () => {});
    return [port.scrollTop, port.scrollLeft];
  })).toEqual([200, 80]);
}), 30_000);
