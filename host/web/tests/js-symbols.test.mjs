import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// Test symbol updates with a minimal DOM, then verify their computed styles in Chrome.
import { afterAll, test, expect } from 'bun:test';
import { copyFileSync, existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';
import { fileURLToPath } from 'node:url';

// symbols.js beside stand-ins for the runtime and the picker glue it loads.
const dir = mkdtempSync(resolve(tmpdir(), 'exact-js-symbols-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
copyFileSync(resolve(fileURLToPath(new URL('../../web-js/symbols.js', import.meta.url))), resolve(dir, 'symbols.js'));
writeFileSync(resolve(dir, 'rt.js'), 'export const { After, PropHooks, Hooks, inflight, journal, clock, data, onEnd } = globalThis.rtStandIn;\n');
writeFileSync(resolve(dir, 'picker-glue.js'), '');
const After = [], PropHooks = {}, inflight = { n: 0 };
globalThis.rtStandIn = { After, PropHooks, Hooks: {}, inflight, journal: [], clock: { now: 0 }, data: { appId: 'com.example' }, onEnd() {} };

class Img {
  constructor() { this.localName = 'img'; this.attrs = new Map(); this.props = new Map(); this.classList = [];
    this.style = { setProperty: (k, v) => this.props.set(k, v), removeProperty: (k) => this.props.delete(k), getPropertyValue: (k) => this.props.get(k) ?? '' }; }
  setAttribute(k, v) { this.attrs.set(k, String(v)); }
  getAttribute(k) { return this.attrs.get(k) ?? null; }
  hasAttribute(k) { return this.attrs.has(k); }
  removeAttribute(k) { this.attrs.delete(k); }
  toggleAttribute(k, on) { if (on) this.attrs.set(k, ''); else this.attrs.delete(k); }
  set src(v) { this.setAttribute('src', v); }
  get alt() { return this.getAttribute('alt') ?? ''; }
  set alt(v) { this.setAttribute('alt', v); }
  get isConnected() { return true; }
  get clientWidth() { return 24; } get clientHeight() { return 24; }
}
const el = new Img();
globalThis.document = {
  head: { append() {} }, createElement: () => ({}), styleSheets: [],
  getElementById: () => ({}),
};
globalThis.window = {};
globalThis.addEventListener = () => {};
globalThis.matchMedia = () => ({ addEventListener() {} });
globalThis.MutationObserver = class { observe() {} };
globalThis.getComputedStyle = () => ({ fontSize: '17px', fontWeight: '400', objectFit: 'contain', paddingLeft: '0', paddingRight: '0', paddingTop: '0', paddingBottom: '0', getPropertyValue: () => '' });
globalThis.exact = { appURL: async (path) => `blob:test/${path}` };

async function refresh() {
  for (const f of After) f();
  await Promise.resolve();
}

function symbolServer(content, boot) {
  const root = resolve(import.meta.dir, '../../..');
  const html = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8'))
    .replace('<link rel="preload" href="./app.wasm" as="fetch" crossorigin>', '')
    .replace('<div id="exact-root"></div>', `<div id="exact-root">${content}</div>`)
    // Register static images as rt.js does when it adopts a page.
    .replace('<script type="module" src="./glue.js"></script>', `<script type="module">
      import { Hooks } from '/rt.js';
      for (const e of document.querySelectorAll('#exact-root img[data-symbol-source]')) Hooks.image(e);
      ${boot}</script>`);
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname.slice(1);
    if (!path) return new Response(html, { headers: { 'content-type': 'text/html' } });
    // Unbundled rt.js re-exports perf.js, which reads this generated table.
    if (path === 'names.js') return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    const match = /^(web\/)?([A-Za-z0-9_.-]+\.js)$/.exec(path);
    if (!match) return new Response('Not found', { status: 404 });
    const files = match[1] ? [resolve(root, 'host/web', match[2])]
      : [resolve(root, 'host/web-js', path), resolve(root, 'host/web', path)];
    const file = files.find(existsSync);
    return file ? new Response(readFileSync(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('Not found', { status: 404 });
  } });
}

async function symbolReady(page, url, report = () => {}) {
  const errors = [];
  const error = (kind, value) => { errors.push([kind, value]); report(kind, value); };
  page.setDefaultTimeout(5000);
  page.on('pageerror', value => error('pageerror', String(value)));
  page.on('requestfailed', request => error('requestfailed', [request.url(), request.failure()]));
  page.on('response', response => { if (response.status() >= 400) error('http', [response.url(), response.status()]); });
  await page.goto(url);
  await page.waitForFunction(() => globalThis.ready);
  expect(errors).toEqual([]);
  return errors;
}

test('an image that leaves a symbol for an app:/ file keeps the file', async () => {
  const { symbols } = await import(resolve(dir, 'symbols.js'));
  symbols({ photo: ['M0 0L24 24', false] });
  expect(PropHooks.src(el, 'symbol:photo')).toBe(true);
  await refresh();
  expect(el.getAttribute('data-symbol-path')).toBe('M0 0L24 24');
  expect(el.getAttribute('src')).toStartWith('data:image/svg+xml');
  expect(PropHooks.src(el, 'app:/data/photo.png')).toBe(true);
  // The symbol is gone before the file lands.
  for (const a of ['data-symbol-path', 'data-symbol-source', 'data-symbol-fill']) expect(el.hasAttribute(a)).toBe(false);
  expect(el.style.getPropertyValue('--exact-symbol-mask')).toBe('');
  while (inflight.n) await Bun.sleep(1);
  expect(el.getAttribute('src')).toBe('blob:test/app:/data/photo.png');
  // A later commit's refresh leaves it there.
  await refresh();
  expect(el.getAttribute('src')).toBe('blob:test/app:/data/photo.png');
});

test('a symbol is decorative unless its author named it', async () => {
  const { symbols } = await import(resolve(dir, 'symbols.js'));
  symbols({ photo: ['M0 0L24 24', false] });
  const unnamed = new Img(), named = new Img();
  named.setAttribute('alt', 'Add a stop'); // the template's alt, from `alt` or `aria-label`
  expect(PropHooks.src(unnamed, 'symbol:photo')).toBe(true);
  expect(PropHooks.src(named, 'symbol:photo')).toBe(true);
  expect(unnamed.getAttribute('alt')).toBe('');
  expect(named.getAttribute('alt')).toBe('Add a stop');
});

// LLP 1011 §4: under `fill`, CSS's initial `object-fit`, a symbol keeps its
// aspect in a box of another shape, as on the Apple hosts; `none` draws it at
// its size.
test('a symbol keeps its aspect under object-fit: fill', async () => {
  const { symbols } = await import(resolve(dir, 'symbols.js'));
  symbols({ chevron: ['M8 6l6 6-6 6', false] });
  const was = globalThis.getComputedStyle;
  try {
    for (const [fit, size] of [['fill', 'contain'], ['cover', 'cover'], ['none', '17px 17px']]) {
      globalThis.getComputedStyle = () => ({ fontSize: '17px', fontWeight: '400', objectFit: fit, paddingLeft: '0', paddingRight: '0', paddingTop: '0', paddingBottom: '0', getPropertyValue: () => '' });
      const wide = new Img();
      expect(PropHooks.src(wide, 'symbol:chevron')).toBe(true);
      await refresh();
      expect(wide.style.getPropertyValue('--exact-symbol-fit')).toBe(size);
    }
  } finally { globalThis.getComputedStyle = was; }
});

// The emitted entry installs symbols before mounting, then projects navigation.
// Use the real consumers and stylesheet: no exactViewport resource commits on resize.
test('symbol axes follow the current projected title and root font without another action', async () => {
  const axes = { auto: '', fixed: '--exact-symbol-size-authored:1;--exact-symbol-weight-authored:1;font-size:23px;font-weight:800;',
    size: '--exact-symbol-size-authored:1;font-size:23px;', weight: '--exact-symbol-weight-authored:1;font-weight:800;' };
  const tabs = Object.entries(axes).map(([id, style]) => `<button id="tab-${id}" role="tab" aria-controls="panel-${id}" aria-selected="${id === 'auto'}" data-button-style="bordered" style="display:grid;flex:1">
    <img id="${id}" data-symbol-source="symbol:book" data-symbol-path="M0 0L24 24" style="${style}"><span id="title-${id}" data-exact-text>Library</span></button>`).join('');
  const panels = Object.keys(axes).map(id => `<div id="panel-${id}" role="tabpanel"><div navigationkey="${id}">Panel</div></div>`).join('');
  const content = `<div style="height:400px;display:flex;flex-direction:column" navigationback="back" navigationkey="auto" id="nav">
    <div style="flex:1">${panels}</div><div role="tablist" style="display:flex;flex-direction:row">${tabs}</div></div>
    <button id="larger">Root 125%</button><button id="reset">Root medium</button><button id="title">Title 30/600</button><button id="inherit">Inherited title</button>`;
  const boot = `import { symbols } from '/symbols.js';
    import { After, commit, C, sig, W, S, useCommands } from '/rt.js';
    import { setRootFontSize } from '/commands.js';
    import { projectRoots } from '/document.js';
    import { navigation } from '/navigation.js';
    useCommands({ setRootFontSize });
    const size = sig(null), weight = sig(null);
    for (const e of document.querySelectorAll('[data-exact-text]')) {
      S(e, 'font-size', 'px', () => size()); S(e, 'font-weight', '', () => weight());
    }
    symbols({ book: ['M0 0L24 24', false] });
    commit(() => {});
    projectRoots(navigation, () => false, console.error, After);
    document.getElementById('larger').onclick = () => commit(() => C('setRootFontSize', ['125%']));
    document.getElementById('reset').onclick = () => commit(() => C('setRootFontSize', ['medium']));
    document.getElementById('title').onclick = () => commit(() => { W(size, 30); W(weight, 600); });
    document.getElementById('inherit').onclick = () => commit(() => { W(size, null); W(weight, null); });
    globalThis.ready = true;`;
  const server = symbolServer(content, boot);
  let browser, stage = 'launch';
  const report = (kind, value) => console.log('symbol diagnostic', JSON.stringify({ stage, kind, value }));
  try {
    browser = await suiteBrowser('chromium', { timeout: 10000 });
    stage = 'page'; report('stage', stage);
    const page = await browser.newPage({ viewport: { width: 1280, height: 700 } });
    const observed = [], expected = [];
    stage = 'navigation'; report('stage', stage);
    const errors = await symbolReady(page, server.url.href, report);
    async function sample(name, titleSize, titleWeight, sidebar) {
      const value = await page.evaluate(() => ({
        sidebar: document.getElementById('nav').hasAttribute('data-exact-sidebar'),
        images: ['auto', 'fixed', 'size', 'weight'].map(id => {
          const e = document.getElementById(id), title = document.getElementById('title-' + id);
          const css = getComputedStyle(e), text = getComputedStyle(title);
          const svg = new DOMParser().parseFromString(decodeURIComponent(e.getAttribute('src').split(',')[1]), 'image/svg+xml').documentElement;
          return [id, parseFloat(css.fontSize), Number(css.fontWeight), Number(svg.getAttribute('width')), parseFloat(text.fontSize), Number(text.fontWeight)];
        }),
      }));
      observed.push({ name, ...value });
      report('sample', observed.at(-1));
      expected.push({ name, sidebar, images: Object.keys(axes).map(id => [id,
        ['fixed', 'size'].includes(id) ? 23 : titleSize, ['fixed', 'weight'].includes(id) ? 800 : titleWeight,
        ['fixed', 'size'].includes(id) ? 23 : titleSize, titleSize, titleWeight]) });
    }
    async function click(id) { stage = id; report('stage', stage); await page.locator('#' + id).click(); }
    stage = 'initial-wide';
    await sample('initial-wide', 16, 500, true);
    await click('larger'); await sample('root125', 20, 500, true);
    stage = 'resize899'; report('stage', stage);
    await page.setViewportSize({ width: 899, height: 700 });
    await page.waitForFunction(() => !document.getElementById('nav').hasAttribute('data-exact-sidebar'));
    await sample('narrow125', 15, 500, false);
    stage = 'resize900'; report('stage', stage);
    await page.setViewportSize({ width: 900, height: 700 });
    await page.waitForFunction(() => document.getElementById('nav').hasAttribute('data-exact-sidebar'));
    await sample('wide125', 20, 500, true);
    await click('title'); await sample('own-title', 30, 600, true);
    await click('reset'); await sample('root-medium-own-title', 30, 600, true);
    await click('inherit'); await sample('inherited-again', 16, 500, true);
    await click('larger'); await sample('root125-again', 20, 500, true);
    await click('reset'); await sample('root-medium', 16, 500, true);
    expect(errors).toEqual([]);
    expect(observed).toEqual(expected);
  } catch (error) { report('failure', String(error)); throw error;
  } finally {
    stage = 'close'; report('stage', stage);
    try { await browser?.close(); } finally { server.stop(true); }
  }
}, 30000);


// LLP 1081 stage 2: a conditional colour that goes leaves the colour past it;
// one that comes back takes it again.
test('a symbol under a bound colour follows whether it has a value', async () => {
  const img = id => `<img id="${id}" data-symbol-source="symbol:book" data-symbol-path="M0 0L24 24" style="width:16px;height:16px">`;
  const content = `<div style="color:rgb(10, 20, 30);--exact-accent:rgb(102, 82, 197)">
    <div id="bound">${img('alone')}</div>
    <div style="color:rgb(0, 128, 0)"><div id="inner">${img('over')}</div></div>
    <div id="implied">${img('unwritten')}</div></div>
    <button id="off">Off</button><button id="on">On</button>`;
  const boot = `import { symbols } from '/symbols.js';
    import { commit, sig, W, S } from '/rt.js';
    const c = sig('rgb(255, 0, 0)');
    for (const id of ['bound', 'inner', 'implied']) S(document.getElementById(id), 'color', '', () => c());
    symbols({ book: ['M0 0L24 24', false] });
    commit(() => {});
    document.getElementById('off').onclick = () => commit(() => W(c, null));
    document.getElementById('on').onclick = () => commit(() => W(c, 'rgb(255, 0, 0)'));
    globalThis.ready = true;`;
  const server = symbolServer(content, boot);
  let browser;
  try {
    browser = await suiteBrowser('chromium', { timeout: 10000 });
    const page = await browser.newPage();
    const errors = await symbolReady(page, server.url.href);
    const ink = () => page.evaluate(() => new Promise(done => requestAnimationFrame(() => done(Object.fromEntries(['alone', 'over', 'unwritten']
      .map(id => [id, getComputedStyle(document.getElementById(id)).backgroundColor]))))));
    const red = 'rgb(255, 0, 0)', green = 'rgb(0, 128, 0)', base = 'rgb(10, 20, 30)';
    expect(await ink()).toEqual({ alone: red, over: red, unwritten: red });
    await page.locator('#off').click();
    expect(await ink()).toEqual({ alone: base, over: green, unwritten: base });
    await page.locator('#on').click();
    expect(await ink()).toEqual({ alone: red, over: red, unwritten: red });
    expect(errors).toEqual([]);
  } finally {
    try { await browser?.close(); } finally { server.stop(true); }
  }
}, 30000);

// A symbol on a button's face draws in its title's colour, which the colour
// scheme changes without a commit: it follows, as on the web host (glue.js
// `mediaChanged`).
test('a symbol on a button follows its title across a colour scheme change', async () => {
  const content = `<button type="button" data-native data-button-style="bordered" style="display:grid;color:light-dark(rgb(1, 2, 3), rgb(4, 5, 6))">
    <img id="symbol" data-symbol-source="symbol:book" data-symbol-path="M0 0L24 24"><span data-exact-text>Library</span></button>`;
  const boot = `import { symbols } from '/symbols.js';
    import { commit } from '/rt.js';
    symbols({ book: ['M0 0L24 24', false] });
    commit(() => {});
    globalThis.ready = true;`;
  const server = symbolServer(content, boot);
  let browser;
  try {
    browser = await suiteBrowser('chromium', { timeout: 10000 });
    const page = await browser.newPage({ colorScheme: 'light' });
    const errors = await symbolReady(page, server.url.href);
    const ink = () => page.evaluate(() => new Promise(done => requestAnimationFrame(() => done(getComputedStyle(document.getElementById('symbol')).backgroundColor))));
    expect(await ink()).toBe('rgb(1, 2, 3)');
    await page.emulateMedia({ colorScheme: 'dark' });
    // The media query's `change` comes after the emulation's reply.
    await page.waitForFunction(() => getComputedStyle(document.getElementById('symbol')).backgroundColor === 'rgb(4, 5, 6)', null, { timeout: 2000 });
    expect(errors).toEqual([]);
  } finally {
    try { await browser?.close(); } finally { server.stop(true); }
  }
}, 30000);

// The same with the app's modes on (modes.js): a mode boundary (`data-x-b`) under a
// token colour writes it again as its implied row, no author's, and a symbol under it
// draws in that colour (LLP 1081 stage 2), as does one under a boundary whose own bound
// colour clears (the implied row stands there, as the runner installs it). The parent's
// own colour differs from the mode's row, so inheritance alone would show blue.
// With no token above, a clear or refusal leaves the inherited colour.
test('a symbol under a mode boundary follows its implied row and its binding', async () => {
  const img = id => `<img id="${id}" data-symbol-source="symbol:book" data-symbol-path="M0 0L24 24" style="width:16px;height:16px">`;
  const content = `<div style="color:rgb(10, 20, 30);--exact-accent:rgb(102, 82, 197)">
    <div class="c1" style="color:rgb(0, 0, 255)">${img('beside')}
      <div id="pin" data-x-b="0">${img('under')}</div>
      <div id="bound" data-x-b="0">${img('cleared')}</div></div>
    <div id="bare" data-x-b="0">${img('alone')}</div></div>
    <button id="none">None</button><button id="red">Red</button><button id="bad">Bad</button><button id="paint">Paint</button>`;
  const boot = `import { symbols } from '/symbols.js';
    import { modes, Sx } from '/modes.js';
    import { commit, sig, W, S } from '/rt.js';
    modes([], { c1: [0] }, ['color:rgb(0, 128, 0)'], {}, [], {});
    const c = sig('inherit');
    for (const id of ['bound', 'bare']) Sx(S, document.getElementById(id), 'color', '', () => c(), 0);
    symbols({ book: ['M0 0L24 24', false] });
    commit(() => {});
    document.getElementById('none').onclick = () => commit(() => W(c, null));
    document.getElementById('red').onclick = () => commit(() => W(c, 'rgb(255, 0, 0)'));
    document.getElementById('bad').onclick = () => commit(() => W(c, 'not a colour'));
    document.getElementById('paint').onclick = () => commit(() => W(c, document.getElementById('paint').dataset.v));
    globalThis.ready = true;`;
  const server = symbolServer(content, boot);
  let browser;
  try {
    browser = await suiteBrowser('chromium', { timeout: 10000 });
    const page = await browser.newPage();
    const errors = await symbolReady(page, server.url.href);
    const ink = () => page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(() => done(Object.fromEntries(['beside', 'under', 'cleared', 'alone']
      .map(id => [id, getComputedStyle(document.getElementById(id)).backgroundColor])))))));
    const green = 'rgb(0, 128, 0)', red = 'rgb(255, 0, 0)', base = 'rgb(10, 20, 30)';
    const colours = (cleared = green, alone = base) => ({ beside: 'rgb(0, 0, 255)', under: green, cleared, alone });
    expect(await ink()).toEqual(colours());
    await page.locator('#none').click();
    expect(await ink()).toEqual(colours());
    await page.locator('#red').click();
    expect(await ink()).toEqual(colours(red, red));
    // A value the row refuses is unset as a clear is: the implied row stands.
    await page.locator('#bad').click();
    expect(await ink()).toEqual(colours());
    // One clear or refusal after another keeps the implied row.
    for (const [v, want] of [['inherit', green], ['unset', green], ['currentcolor', green], ['nope', green], ['nada', green], ['rgb(255, 0, 0)', 'rgb(255, 0, 0)'], ['inherit', green]]) {
      await page.evaluate(v => { document.getElementById('paint').dataset.v = v; }, v);
      await page.locator('#paint').click();
      expect([v, await ink()]).toEqual([v, colours(want, want === red ? red : base)]);
    }
    expect(errors).toEqual([]);
  } finally {
    try { await browser?.close(); } finally { server.stop(true); }
  }
}, 30000);


test('adoption draws the static icon selected by the client viewport', async () => {
  const wide = 'M2 2H8V22H2Z', narrow = 'M16 2H22V22H16Z';
  const server = symbolServer(`<img id="icon" data-symbol-source="symbol:icon|star|0 0 24 24|${wide}|icons/wide.svg" data-symbol-path="${wide}" data-symbol-fill data-symbol-nonzero data-symbol-viewbox="0 0 24 24">`, `
    import { symbols } from '/symbols.js';
    import { h, adoptRow, commit } from '/rt.js';
    symbols({});
    const root = document.getElementById('exact-root'), original = root.firstElementChild;
    const path = innerWidth < 600 ? '${narrow}' : '${wide}';
    commit(() => adoptRow(root, () => h(root, 'img', 0, {
      id: 'icon', 'data-symbol-source': 'symbol:icon|star|0 0 24 24|' + path + '|icons/client.svg',
      'data-symbol-fill': '', 'data-symbol-nonzero': '', 'data-symbol-viewbox': '0 0 24 24'
    }, 0)));
    globalThis.reused = original === root.firstElementChild;
    globalThis.ready = true;`);
  const browser = await suiteBrowser('chromium', { timeout: 10000 });
  try {
    const page = await browser.newPage({ viewport: { width: 400, height: 300 } });
    await symbolReady(page, server.url.href);
    const result = await page.evaluate(() => {
      const e = document.getElementById('icon');
      return [globalThis.reused, e.getAttribute('data-symbol-path'), decodeURIComponent(e.symbolMask)];
    });
    expect(result.slice(0, 2)).toEqual([true, narrow]);
    expect(result[2]).toContain('d="' + narrow + '"');
  } finally { await browser.close(); server.stop(true); }
}, 15000);

test('a static icon without a viewBox keeps its nonzero fill rule', async () => {
  const path = 'M2 2H22V22H2Z M8 8H16V16H8Z';
  const server = symbolServer('', `
    import { symbols } from '/symbols.js';
    import { h, commit } from '/rt.js';
    symbols({});
    commit(() => h(document.getElementById('exact-root'), 'img', 0, {
      id: 'icon', 'data-symbol-source': 'symbol:icon|star||${path}|icons/a.svg',
      'data-symbol-fill': '', 'data-symbol-nonzero': ''
    }, 0));
    globalThis.ready = true;`);
  const browser = await suiteBrowser('chromium', { timeout: 10000 });
  try {
    const page = await browser.newPage();
    await symbolReady(page, server.url.href);
    expect(await page.evaluate(() => {
      const e = document.getElementById('icon');
      return [e.hasAttribute('data-symbol-nonzero'), e.hasAttribute('data-symbol-evenodd'),
        decodeURIComponent(e.symbolMask).match(/fill-rule="([a-z]+)"/)[1]];
    })).toEqual([true, false, 'nonzero']);
  } finally { await browser.close(); server.stop(true); }
}, 15000);
