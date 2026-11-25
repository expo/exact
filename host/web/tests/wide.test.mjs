import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// LLP 1116.004 D1 in a real browser at a phone's width (390) and a desktop's (1280): the shell's stylesheet and the
// navigation glue's projection (navigation.js, which both web targets run) over nodes as the hosts write them. A root
// tab bar whose placement is unsaid is a sidebar on a wide window and a bar on a phone; an arrangement the app wrote
// stays its own. A stack whose root route asks for the split (LLP 1127 D1: "split", a width or a placeholder box of its
// own) shows that route beside its top on a wide window (the list live, its disclosure gone, the row whose press pushed
// the detail marked, no Back in the detail, and while nothing is selected the platform's placeholder in the pane or the
// author's own) and pushes on a phone; "none" or no ask pushes; a sheet stays a sheet; a resize changes nothing else. At a
// desktop width `check` advises (D1.3) on a tab bar left along the bottom, and holds its advice for a list (LLP 1127 D1).
import { test, expect } from 'bun:test';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { cdpBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { readyPage } from './page.mjs';
import { fileURLToPath } from 'node:url';

const WEB = resolve(fileURLToPath(new URL('..', import.meta.url)));
const { unavailable } = chromium();
const check = need('chrome', unavailable);

const style = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>[\s\S]*?<\/style>/)[0];
const chevron = '<img data-symbol-source="symbol:forward-chevron" alt="" style="width:17px;height:17px">';
const abs = 'position:absolute;inset:0;display:flex;flex-direction:column';
// A route of rows (`rows` true: each row writes the router and draws the disclosure), as the hosts write a `list` of `button`s; `inner` after it.
// It asks for the split (LLP 1127 D1) where its rows push, unless `extra` says its own `navigationSplit`.
const asks = (rows, extra) => rows && !/navigationSplit=/.test(extra) ? `navigationSplit="split" ${extra}` : extra;
const list = (id, key, rows = true, extra = '', inner = '') => `<div navigationKey="${key}" id="${id}" style="${abs}" ${asks(rows, extra)}><header><h1>People</h1></header>
  <div data-exact-list role="list" id="${id}-list">${['ada', 'bo'].map(n => `<button id="${id}-${n}" data-exact-on="press" data-exact-router-press="${rows}" style="display:flex"><span data-exact-text>${n}</span>${rows ? chevron : ''}</button>`).join('')}</div>${inner}</div>`;
const detail = (id, key, extra = '') => `<div navigationKey="${key}" id="${id}" style="${abs}" ${extra}><header><h2>${id}</h2></header><div>${id}</div></div>`;
const tabs = (bar) => `<main id="nav" navigationKey="2" navigationBack="back" style="width:100%;height:100%;display:flex;flex-direction:column;position:relative">
    <div id="panels" style="flex:1 1 0%;min-height:0;position:relative">
      <div role="tabpanel" id="p1" style="${abs}">${list('r1', 1)}${detail('r2', 2)}${detail('r3', 3, 'navigationPresentation="modal"')}</div>
      <div role="tabpanel" id="p2" style="${abs}">${list('t1', 10, false)}${detail('t2', 11)}</div>
    </div>
    ${bar}
  </main>`;
const BAR = `<div role="tablist" id="bar" style="display:flex;flex-direction:row"><button role="tab" id="tab1" aria-controls="p1" aria-selected="true" data-button-style="bordered" data-native style="flex:1 1 0%"><img alt=""><span data-exact-text>People</span></button><button role="tab" id="tab2" aria-controls="p2" aria-selected="false" data-button-style="bordered" data-native style="flex:1 1 0%"><img alt=""><span data-exact-text>Saved</span></button></div>`;
const page = (body, routePaths = [], waitForActivation = false) => style + `<div id="exact-root">${body}</div><span id="secondary" style="position:absolute;color:var(--exact-secondary-label)"></span>
<script type="module">
  import { navigation, unselected } from './navigation.js';
  const root = document.getElementById('exact-root');
  window.calls = [];
  navigation.connect(root, () => false, line => calls.push(line), id => calls.push('host back ' + id), null, href => ${JSON.stringify(routePaths)}.includes(href), ${waitForActivation});
  window.activate = () => navigation.activate(root, line => calls.push(line));
  window.select = key => { document.getElementById('nav').setAttribute('navigationKey', key); navigation.project(root, () => {}); };
  const locations = new Map();
  window.push = (key, url, visits) => {
    locations.set(Number(key), url);
    navigation.apply({ top: Number(key), url, removed: [], visits: visits ?? [...locations] }); select(key);
  };
  window.box = id => { const b = document.getElementById(id).getBoundingClientRect(); return [b.x, b.y, b.width, b.height].map(Math.round); };
  window.shown = id => { const e = document.getElementById(id); return getComputedStyle(e).visibility === 'visible' && !e.closest('[inert]'); };
  window.off = () => unselected(document.getElementById('nav')).map(e => e.id);
  window.back = id => { const b = document.querySelector('#' + id + ' > header')?.shadowRoot?.querySelector('button'); return !!b && !b.hidden; };
  // The platform's placeholder in a stack's detail pane: its words, its left edge and colour (null where none shows).
  window.placeholder = id => { const e = document.getElementById(id), p = getComputedStyle(e, '::before'); return e.hasAttribute('data-exact-split-empty') && p.content !== 'none' ? [p.content, Math.round(parseFloat(p.left)), p.color] : null; };
  select(document.getElementById('nav').getAttribute('navigationKey'));
  window.ready = true;
</script>`;

/** The page (`html`) in headless Chrome at `w`×800: `test(evaluate, size, click, call, reopen)`, `size(w)` resizing the
 * viewport and `reopen()` loading the page again. */
async function inChrome(html, test, w = 1280) {
  const server = createServer((req, res) => {
    if (req.url === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(html); return; }
    res.writeHead(200, { 'content-type': 'text/javascript' }); res.end(readFileSync(req.url === '/check.js' ? resolve(WEB, '../web-js/check.js') : resolve(WEB, 'navigation.js')));
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const suite = await cdpBrowser({ args: ['--window-size=1400,900'] }), { cdp } = suite;
  try {
    const { sessionId } = await suite.page();
    const call = (method, params) => cdp.send(method, params, sessionId);
    const evaluate = async (expression) => {
      const reply = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
      if (reply.exceptionDetails) throw new Error(reply.exceptionDetails.exception?.description ?? reply.exceptionDetails.text);
      return reply.result.value;
    };
    const size = async (width) => {
      await call('Emulation.setDeviceMetricsOverride', { width, height: 800, deviceScaleFactor: 1, mobile: false });
      await evaluate('new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))');
    };
    const click = async (id) => {
      const [x, y] = await evaluate(`(e => { const b = e.getBoundingClientRect(); return [b.x + b.width / 2, b.y + b.height / 2]; })(document.getElementById(${JSON.stringify(id)}))`);
      await call('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
      await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
    };
    const url = `http://127.0.0.1:${server.address().port}/`, reopen = () => readyPage(cdp, sessionId, url);
    await size(w);
    await reopen();
    await test(evaluate, size, click, call, reopen);
  } finally {
    await suite.close();
    server.close();
  }
}

check('a root tab bar left unsaid is a sidebar on a wide window and a bar on a phone, across a resize', () => inChrome(page(tabs(BAR)), async (evaluate, size) => {
  // Wide: the root's column turns; the bar is a 240-wide column at the leading edge, the panels beside it, the tabs rows.
  expect(await evaluate(`[document.getElementById('nav').hasAttribute('data-exact-sidebar'), box('bar'), box('panels'), box('tab1')[3] < 60, box('tab2')[1] > box('tab1')[1]]`))
    .toEqual([true, [0, 0, 240, 800], [240, 0, 1040, 800], true, true]);
  // The selected tab is in the accent's tint; the tablist and its tabs keep their roles.
  expect(await evaluate(`[getComputedStyle(document.getElementById('tab1')).backgroundColor !== getComputedStyle(document.getElementById('tab2')).backgroundColor, document.getElementById('bar').getAttribute('role')]`)).toEqual([true, 'tablist']);
  await size(390);
  expect(await evaluate(`[document.getElementById('nav').hasAttribute('data-exact-sidebar'), box('bar')[0], box('bar')[2], box('bar')[1] + box('bar')[3], box('panels')[2]]`)).toEqual([false, 0, 390, 800, 390]);
  await size(1280);
  expect(await evaluate(`[document.getElementById('nav').hasAttribute('data-exact-sidebar'), box('bar')]`)).toEqual([true, [0, 0, 240, 800]]);
  // Keep the authored content aligned with the viewport segments when each half is narrow
  await evaluate(`Object.defineProperty(window, 'viewport', { configurable: true, value: { segments: [new DOMRect(0, 0, 620, 800), new DOMRect(660, 0, 620, 800)] } }); select(document.getElementById('nav').getAttribute('navigationKey'));`);
  expect(await evaluate(`[document.getElementById('nav').hasAttribute('data-exact-sidebar'), box('panels')[0], box('panels')[2]]`)).toEqual([false, 0, 1280]);
  await evaluate(`delete window.viewport; select(document.getElementById('nav').getAttribute('navigationKey'));`);
  expect(await evaluate(`document.getElementById('nav').hasAttribute('data-exact-sidebar')`)).toBe(true);
}), 60000);

// LLP 1123 D9: the box that ends at the root's bar along the bottom has no bottom inset, as the bar keeps the home
// indicator clear; a box laid over the window keeps the window's, and a box in the flow after the bar takes the bottom
check('the root bar along the bottom clears the home indicator for the box above it, not for an overlay', () => {
  const overlay = '<div id="overlay" style="position:fixed;inset:0;pointer-events:none"></div>';
  return inChrome(page(tabs(overlay + BAR)), async (evaluate, size, click, call) => {
    await call('Emulation.setSafeAreaInsetsOverride', { insets: { top: 0, left: 0, right: 0, bottom: 34 } });
    await evaluate(`window.bottoms = ids => ids.map(id => getComputedStyle(document.getElementById(id))).map((style, i) => i ? style.getPropertyValue('--exact-safe-area-inset-bottom').trim() : style.paddingBottom); select(document.getElementById('nav').getAttribute('navigationKey'))`);
    expect(await evaluate(`bottoms(['bar', 'panels', 'overlay'])`)).toEqual(['34px', '0px', '34px']);
    // A sheet, fixed to the window, reaches its bottom; a full-screen route stays in the panels' box
    expect(await evaluate(`{ const route = document.getElementById('r2'); route.setAttribute('navigationPresentation', 'fullscreen'); const seen = bottoms(['bar', 'r2', 'r3']); route.removeAttribute('navigationPresentation'); seen }`)).toEqual(['34px', '0px', '34px']);
    // A box in the flow that takes the overlay's place ends at the bar instead of the panels
    expect(await evaluate(`document.getElementById('overlay').replaceWith(Object.assign(document.createElement('div'), { id: 'strip', textContent: 'Strip' })); select(2); bottoms(['bar', 'panels', 'strip'])`)).toEqual(['34px', '34px', '0px']);
    // A box after the bar laid over the window leaves the bar the bottom, until it joins the flow
    expect(await evaluate(`document.getElementById('nav').append(Object.assign(document.createElement('footer'), { id: 'after', textContent: 'After', style: 'position:fixed' })); select(2); bottoms(['bar', 'strip', 'after'])`)).toEqual(['34px', '0px', '34px']);
    expect(await evaluate(`document.getElementById('after').style.position = 'static'; select(2); bottoms(['bar', 'strip', 'after'])`)).toEqual(['0px', '34px', '34px']);
    // A box laid out nowhere neither takes the bottom after the bar nor ends at it
    expect(await evaluate(`document.getElementById('after').style.display = 'none'; document.getElementById('bar').before(Object.assign(document.createElement('div'), { id: 'gone', style: 'display:none' })); select(2); bottoms(['bar', 'strip', 'gone'])`)).toEqual(['34px', '0px', '34px']);
    // Nor is a bar an `order` moves off the bottom
    expect(await evaluate(`{ const strip = document.getElementById('strip'); strip.style.order = '1'; select(2); const seen = bottoms(['bar', 'strip', 'panels']); strip.style.order = ''; select(2); seen }`)).toEqual(['0px', '34px', '34px']);
    // A bar already clear of the bottom inset takes no more, and a bar an `order` keeps last is still along the bottom
    expect(await evaluate(`{ const nav = document.getElementById('nav'); nav.style.boxSizing = 'border-box'; nav.style.paddingBottom = '34px'; select(2); const seen = bottoms(['bar', 'strip']); nav.style.boxSizing = nav.style.paddingBottom = ''; select(2); seen }`)).toEqual(['0px', '34px']);
    expect(await evaluate(`{ const bar = document.getElementById('bar'); bar.style.order = '1'; select(2); const seen = bottoms(['bar', 'strip']); bar.style.order = ''; select(2); seen }`)).toEqual(['34px', '0px']);
    // A bar whose nav lies out of the root's flow is no bar along its bottom
    expect(await evaluate(`{ const bar = document.getElementById('bar'), nav = Object.assign(document.createElement('nav'), { id: 'rail', style: 'position:absolute;left:0;right:0;bottom:0' }); bar.before(nav); nav.append(bar, Object.assign(document.createElement('footer'), { id: 'foot' })); select(2); bottoms(['bar', 'strip', 'panels']) }`)).toEqual(['0px', '34px', '34px']);
  }, 390);
}, 60000);

// LLP 1123 D9: the marks follow the edges they read, and a box in the top layer reads the window's inset
check('the root bar along the bottom follows its edges and leaves the window its inset over the panel', () => {
  return inChrome(page(tabs(BAR + '<footer id="strip"><div id="grow" style="height:0"></div></footer>')), async (evaluate, size, click, call) => {
    await call('Emulation.setSafeAreaInsetsOverride', { insets: { top: 0, left: 0, right: 0, bottom: 34 } });
    await evaluate(`window.bottoms = ids => ids.map(id => getComputedStyle(document.getElementById(id))).map((style, i) => i ? style.getPropertyValue('--exact-safe-area-inset-bottom').trim() : style.paddingBottom); window.frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); select(document.getElementById('nav').getAttribute('navigationKey'))`);
    expect(await evaluate(`bottoms(['bar', 'panels'])`)).toEqual(['34px', '0px']);
    // A box after the bar that grows with no commit lifts the bar off the bottom, and the panel has its inset back
    expect(await evaluate(`document.getElementById('grow').style.height = '64px'; frames().then(() => bottoms(['bar', 'panels']))`)).toEqual(['0px', '34px']);
    expect(await evaluate(`document.getElementById('grow').style.height = '0px'; frames().then(() => bottoms(['bar', 'panels']))`)).toEqual(['34px', '0px']);
    // A dialog in the top layer from within the panel reads the window's inset
    expect(await evaluate(`{ const d = Object.assign(document.createElement('dialog'), { id: 'dialog' }); document.getElementById('panels').append(d); d.showModal(); const seen = bottoms(['bar', 'dialog']); d.close(); d.remove(); seen }`)).toEqual(['34px', '34px']);
    // A bar laid out wholly below the window takes nothing from the panel before it
    expect(await evaluate(`document.getElementById('panels').style.flex = '0 0 800px'; frames().then(() => bottoms(['bar', 'panels']))`)).toEqual(['0px', '34px']);
  }, 390);
}, 60000);

check('a root nav keeps its footer below the scrolling sidebar and hides it in the bottom bar', () => {
  const foot = '<footer id="foot"><button id="account" onclick="calls.push(this.id)">Account</button></footer>';
  const bar = BAR.replace('</div>', '<button role="tab">More</button>'.repeat(30) + '</div>');
  return inChrome(page(tabs('<nav id="rail">' + bar + foot + '</nav>')), async (read, size, click) => {
    const geometry = `[box('panels')[0], box('rail')[2], box('foot')[1] + box('foot')[3], box('bar')[3] <= box('foot')[1]]`;
    expect(await read(geometry)).toEqual([240, 240, 800, true]);
    expect(await read(`{ const bar = document.getElementById('bar'); bar.scrollTop = 100; [bar.scrollTop, box('foot')[1] + box('foot')[3]] }`)).toEqual([100, 800]);
    await click('account');
    expect(await read('calls')).toContain('account');
    await size(390);
    expect(await read(`[box('foot')[3], box('bar')[2], box('bar')[1] + box('bar')[3]]`)).toEqual([0, 390, 800]);
    expect(await read(`document.getElementById('account').focus(); document.activeElement.id`)).not.toBe('account');
    await size(1280);
    expect(await read(geometry)).toEqual([240, 240, 800, true]);
    // Author placement wins. Moving the footer out of the nav also gives it back to the page.
    expect(await read(`document.getElementById('bar').style.appearance = 'none'; select(2); box('panels')[0]`)).toBe(0);
    await size(390);
    expect(await read(`box('foot')[3]`)).toBeGreaterThan(0);
    expect(await read(`document.getElementById('bar').style.appearance = ''; select(2); box('foot')[3]`)).toBe(0);
    expect(await read(`document.getElementById('nav').append(document.getElementById('foot')); select(2); box('foot')[3]`)).toBeGreaterThan(0);
  });
}, 60000);

check('fold-only media changes reproject navigation without a viewport resource', () => inChrome(page(tabs(BAR)), async (evaluate, size, click, call) => {
  const sidebar = wanted => evaluate(`new Promise((resolve, reject) => {
    const root = document.getElementById('nav');
    const observer = new MutationObserver(check);
    const timer = setTimeout(() => { observer.disconnect(); reject(new Error('sidebar projection did not settle')); }, 10000);
    function check() { if (root.hasAttribute('data-exact-sidebar') === ${wanted}) { clearTimeout(timer); observer.disconnect(); resolve(); } }
    observer.observe(root, { attributes: true, attributeFilter: ['data-exact-sidebar'] }); check();
  })`);
  await evaluate(`Object.defineProperty(navigator, 'devicePosture', { configurable: true, value: undefined }); Object.defineProperty(window, 'viewport', { configurable: true, value: undefined }); window.resizes = 0; addEventListener('resize', () => resizes++);`);
  expect(await evaluate(`document.getElementById('nav').hasAttribute('data-exact-sidebar')`)).toBe(true);
  await call('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false,
    displayFeature: { orientation: 'vertical', offset: 620, maskLength: 40 } });
  await sidebar(false);
  expect(await evaluate(`[resizes, box('panels')[0], box('panels')[2]]`)).toEqual([0, 0, 1280]);
  await call('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
  await sidebar(true);
  expect(await evaluate(`resizes`)).toBe(0);
}), 60000);

check('an arrangement the app wrote stays its own on a wide window', async () => {
  // A vertical tablist (a rail), a bar it positions itself, and a root that is no flex column: none is turned.
  const vertical = BAR.replace('role="tablist"', 'role="tablist" aria-orientation="vertical"');
  const positioned = BAR.replace('style="display:flex;flex-direction:row"', 'style="display:flex;flex-direction:row;position:absolute;left:0;right:0;bottom:0"');
  for (const html of [tabs(vertical), tabs(positioned), tabs(BAR).replace('display:flex;flex-direction:column;position:relative', 'display:block;position:relative')]) {
    await inChrome(page(html), async (evaluate) => {
      expect(await evaluate(`document.getElementById('nav').hasAttribute('data-exact-sidebar')`)).toBe(false);
    });
  }
}, 60000);

check('appearance none keeps authored root tabs across selection and restores the sidebar with auto', () => inChrome(page(tabs(BAR.replace('id="bar"', 'id="bar" class="authored-tabs"'))) + '<style>.authored-tabs { appearance: none; }</style>', async (evaluate) => {
  await evaluate(`select(10)`);
  expect(await evaluate(`[document.getElementById('nav').hasAttribute('data-exact-sidebar'), box('bar')[2], shown('t1'), shown('r2')]`))
    .toEqual([false, 1280, true, false]);
  await evaluate(`document.getElementById('bar').style.appearance = 'auto'; select(2)`);
  expect(await evaluate(`[document.getElementById('nav').hasAttribute('data-exact-sidebar'), shown('r2')]`)).toEqual([true, true]);
  await evaluate(`document.getElementById('bar').style.appearance = 'none'; select(10)`);
  expect(await evaluate(`document.getElementById('nav').hasAttribute('data-exact-sidebar')`)).toBe(false);
}), 60000);

check('a projection with nothing to change writes nothing', () => inChrome(page(tabs(BAR)), async (evaluate) => {
  // What projections write, but for the selection and a style the test itself sets on the tab bar.
  await evaluate(`window.writes = (change) => new Promise(done => { const seen = [], watch = new MutationObserver(records => seen.push(...records.filter(r => r.attributeName !== 'navigationkey' && !(r.target.id === 'bar' && r.attributeName === 'style')).map(r => (r.target.id || r.target.localName) + ' ' + r.attributeName))); watch.observe(document.getElementById('exact-root'), { attributes: true, subtree: true }); change(); setTimeout(() => { watch.disconnect(); done(seen); }); })`);
  // A split showing the detail a row's press pushed, then a bound style that does not place the bar; then a sheet
  // over it, the tab bar inert beneath.
  await evaluate(`select('1'); { const row = document.getElementById('r1-bo'); row.addEventListener('click', () => push('2', '/people/bo'), {once:true}); row.click(); }`);
  // The tab bar's resize observer hears once when it starts watching, in the next rendering update, and rechecks its
  // placement then; that recheck is no projection's, so the test watches only once two frames have passed
  await evaluate(`new Promise((settled) => requestAnimationFrame(() => requestAnimationFrame(settled)))`);
  expect(await evaluate(`writes(() => { select(2); document.getElementById('bar').style.backgroundColor = 'red'; select(2); }).then(seen => [document.getElementById('nav').hasAttribute('data-exact-sidebar'), document.getElementById('r1-bo').hasAttribute('data-exact-split-row'), seen])`)).toEqual([true, true, []]);
  await evaluate(`select(3)`);
  expect(await evaluate(`writes(() => { select(3); select(3); }).then(seen => [document.getElementById('bar').inert, seen])`)).toEqual([true, []]);
}), 60000);

check('retained root tabs yield to changed authored placement and restore without losing selection', () => inChrome(page(tabs(BAR)), async (evaluate, size) => {
  // Each change to what places the bar, then its undoing: by an inline style, a class or the bar's orientation.
  await evaluate(`document.head.append(Object.assign(document.createElement('style'), { textContent: '#exact-root .placed { position: absolute !important; }' }))`);
  const style = (id, property, value) => `document.getElementById('${id}').style.${property} = '${value}'`, bar = "document.getElementById('bar')";
  const samples = [];
  for (const change of [
    [style('nav', 'display', 'block'), style('nav', 'display', 'flex')],
    [style('bar', 'position', 'absolute'), style('bar', 'position', 'relative')],
    [style('nav', 'flexDirection', 'row'), style('nav', 'flexDirection', 'column')],
    [style('bar', 'flexDirection', 'column'), style('bar', 'flexDirection', 'row')],
    [`${bar}.classList.add('placed')`, `${bar}.classList.remove('placed')`],
    [`${bar}.setAttribute('aria-orientation', 'vertical')`, `${bar}.removeAttribute('aria-orientation')`],
  ]) {
    for (const step of change) samples.push(await evaluate(`${step}; select(2); [document.getElementById('nav').hasAttribute('data-exact-sidebar'), shown('r2'), shown('t1')]`));
  }
  expect(samples).toEqual(Array.from({ length: 6 }, () => [[false, true, false], [true, true, false]]).flat());
  await size(390);
  expect(await evaluate(`document.getElementById('nav').hasAttribute('data-exact-sidebar')`)).toBe(false);
  await size(1280);
  expect(await evaluate(`[document.getElementById('nav').hasAttribute('data-exact-sidebar'), box('bar'), shown('r2')]`)).toEqual([true, [0, 0, 240, 800], true]);
}), 60000);

check('a list whose rows push is beside its detail on a wide window, and pushes on a phone', () => inChrome(page(tabs(BAR)), async (evaluate, size) => {
  // Wide, nothing chosen: the list is a 320-wide column, its disclosure gone; the rest of the stack's width is the
  // platform's placeholder (LLP 1116.006 D2), one line in the secondary label's colour (the look's, which its sheet carries).
  await evaluate(`select('1')`);
  const secondary = await evaluate(`getComputedStyle(document.getElementById('secondary')).color`);
  expect(secondary).not.toBe(await evaluate(`getComputedStyle(document.body).color`));
  expect(await evaluate(`[document.getElementById('r1').getAttribute('data-exact-split'), box('r1'), getComputedStyle(document.querySelector('#r1 img')).display, shown('r1'), placeholder('p1')]`))
    .toEqual(['list', [240, 0, 320, 800], 'none', true, ['"Select an item"', 320, secondary]]);
  // A row's press pushes its detail: both show, the list live and the row marked; the detail has no Back.
  await evaluate(`{ const row = document.getElementById('r1-bo'); row.addEventListener('click', () => push('2', '/people/bo'), {once:true}); row.click(); }`);
  expect(await evaluate(`[shown('r1'), shown('r2'), box('r2'), document.getElementById('r2').getAttribute('data-exact-split'), document.getElementById('r1-bo').hasAttribute('data-exact-split-row'), back('r2'), off(), placeholder('p1')]`))
    .toEqual([true, true, [560, 0, 720, 800], 'detail', true, false, ['p2', 'r3'], null]);
  // A sheet stays a sheet over both, the list and the detail inert beneath it. In a wide window the look draws it as a
  // card (LLP 1115.000 D4.8): 560 wide, a sheet's height (45rem), in the window's middle once it has slid up.
  await evaluate(`push('3', '/people/bo/edit')`);
  const sheet = await evaluate(`document.getElementById('r3').getAnimations().forEach(a => a.finish()); box('r3')`);
  expect([sheet[0], sheet[2], sheet[3], Math.abs(sheet[1] * 2 + sheet[3] - 800) <= 1]).toEqual([360, 560, 720, true]);
  expect(await evaluate(`[document.getElementById('r3').hasAttribute('data-exact-split'), shown('r1'), shown('r2'), document.getElementById('r1').hasAttribute('data-exact-covered'), off()]`))
    .toEqual([false, false, false, true, ['p2', 'r1', 'r2']]);
  // A phone: the pushed stack, the list hidden and inert, the detail the whole width with Back, the row's disclosure
  // still not drawn (the web draws none at any width); then wide again.
  await evaluate(`select('2')`);
  await size(390);
  expect(await evaluate(`[shown('r1'), shown('r2'), box('r2')[2], box('r2')[3] + box('bar')[3], document.getElementById('r1').hasAttribute('data-exact-split'), back('r2'), getComputedStyle(document.querySelector('#r1 img')).display, off()]`))
    .toEqual([false, true, 390, 800, false, true, 'none', ['p2', 'r1', 'r3']]);
  // A phone's list, nothing chosen, is the whole width: no placeholder.
  await evaluate(`select('1')`);
  expect(await evaluate(`[shown('r1'), box('r1')[2], placeholder('p1')]`)).toEqual([true, 390, null]);
  await evaluate(`select('2')`);
  await size(1280);
  expect(await evaluate(`[shown('r1'), shown('r2'), box('r1')[2], document.getElementById('r1-bo').hasAttribute('data-exact-split-row')]`)).toEqual([true, true, 320, true]);
  // The other tab's route does not ask: the stack stays unsplit, with no placeholder.
  expect(await evaluate(`[document.getElementById('t1').hasAttribute('data-exact-split'), placeholder('p2')]`)).toEqual([false, null]);
  // A row that is a link (LLP 1115.000 D4.4) is followed from the document's capture phase (input-glue.js, router.js),
  // so its commit lands within the click, before a listener nearer the row would hear it: its row is marked all the same.
  await evaluate(`select('1'); document.addEventListener('click', e => { if (e.target.closest('#r1-ada')) push('2', '/people/ada'); }, true); document.getElementById('r1-ada').click()`);
  expect(await evaluate(`[shown('r2'), document.getElementById('r1-ada').hasAttribute('data-exact-split-row'), document.getElementById('r1-bo').hasAttribute('data-exact-split-row')]`)).toEqual([true, true, false]);
}), 60000);

check('the author\'s placeholder in the root route takes the detail pane while nothing is selected, and shows nowhere else', () => {
  // As the lowerer writes it: `display: none` but where the split shows it (LLP 1116.006 D2).
  const own = '<div navigationSplit="placeholder" id="own" style="display:none;gap:8px"><span id="own-text">Pick a person</span><button id="own-add">Add</button></div>';
  return inChrome(page(`<main id="nav" navigationKey="1" navigationBack="back" style="width:100%;height:100%;position:relative">${list('r1', 1, true, '', own)}${detail('r2', 2)}</main>`), async (evaluate, size) => {
    const hit = `(e => document.elementFromPoint(box(e)[0] + box(e)[2] / 2, box(e)[1] + box(e)[3] / 2)?.id)`;
    // Wide, nothing chosen: the author's box is the pane beside the list, its content centred and pressable; no host line.
    // OWED (QUEUE.md, the neutral look): the pane starts at 319, not 320, and so ends a pixel short of the window. The
    // shell places it at the list column's `100%`, and the neutral look draws that column's divider as a 1 px border
    // where LLP 1116 D1's drew a shadow, so the column's own box is a pixel narrower. A detail is placed by the stack.
    expect(await evaluate(`[box('r1'), box('own'), shown('own'), Math.round(box('own-text')[0] + box('own-text')[2] / 2), ${hit}('own-add'), placeholder('nav'), document.getElementById('nav').hasAttribute('data-exact-split-empty')]`))
      .toEqual([[0, 0, 320, 800], [319, 0, 960, 800], true, 799, 'own-add', null, false]);
    // A narrower wide window: the pane follows.
    await size(1000);
    expect(await evaluate(`box('own')`)).toEqual([319, 0, 680, 800]);
    // A detail shown: the author's box is gone; back at the list, it returns.
    await evaluate(`{ const row = document.getElementById('r1-bo'); row.addEventListener('click', () => push('2', '/people/bo'), {once:true}); row.click(); }`);
    expect(await evaluate(`[box('own'), getComputedStyle(document.getElementById('own')).display, box('r2')]`)).toEqual([[0, 0, 0, 0], 'none', [320, 0, 680, 800]]);
    await evaluate(`push('1', '/')`);
    expect(await evaluate(`box('own')`)).toEqual([319, 0, 680, 800]);
    // A phone: the list alone, the author's box nowhere.
    await size(390);
    expect(await evaluate(`[box('r1'), getComputedStyle(document.getElementById('own')).display, placeholder('nav')]`)).toEqual([[0, 0, 390, 800], 'none', null]);
    // Wide again, the box moved out of the route by a page module is the author's placeholder no more, wherever it went
    await size(1280);
    await evaluate(`{ const wrap = document.createElement('div'); document.getElementById('nav').after(wrap); wrap.append(document.getElementById('own')); push('1', '/'); }`);
    expect(await evaluate(`[document.getElementById('own').hasAttribute('data-exact-split-placeholder'), getComputedStyle(document.getElementById('own')).display]`)).toEqual([false, 'none']);
  });
}, 60000);

check('a placeholder a page module moves to another tab\'s root is shown by that root, whichever projects first', () => {
  const own = '<div navigationSplit="placeholder" id="own" style="display:none"><span>Pick</span></div>';
  const html = tabs(BAR).replace(list('t1', 10, false), list('t1', 10, false, 'navigationSplit="split"', own));
  const shown = `[document.getElementById('own').hasAttribute('data-exact-split-placeholder'), getComputedStyle(document.getElementById('own')).display]`;
  return inChrome(page(html), async (evaluate) => {
    await evaluate(`select('10')`);
    expect(await evaluate(shown)).toEqual([true, 'flex']);
    // Into the first tab's root, which shows it while nothing is selected there: that stack projects before the one it left
    await evaluate(`document.getElementById('r1').append(document.getElementById('own')); select('1')`);
    expect(await evaluate(shown)).toEqual([true, 'flex']);
    // And back, the root it leaves projecting first
    await evaluate(`document.getElementById('t1').append(document.getElementById('own')); select('10')`);
    expect(await evaluate(shown)).toEqual([true, 'flex']);
    // Out of every root: the root it left clears its mark
    await evaluate(`document.getElementById('t1').after(document.getElementById('own')); select('10')`);
    expect(await evaluate(shown)).toEqual([false, 'none']);
  });
}, 60000);

check('navigationSplit opts a stack out, or names the width of a list the rule does not find', async () => {
  const stack = (extra, rows) => `<main id="nav" navigationKey="2" navigationBack="back" style="width:100%;height:100%;position:relative">${list('r1', 1, rows, extra)}${detail('r2', 2)}</main>`;
  await inChrome(page(stack('navigationSplit="none"', true)), async (evaluate) => {
    expect(await evaluate(`[shown('r1'), document.getElementById('r1').hasAttribute('data-exact-split'), box('r2')]`)).toEqual([false, false, [0, 0, 1280, 800]]);
  });
  await inChrome(page(stack('navigationSplit="280"', false)), async (evaluate) => {
    expect(await evaluate(`[shown('r1'), box('r1'), box('r2'), back('r2')]`)).toEqual([true, [0, 0, 280, 800], [280, 0, 1000, 800], false]);
  });
}, 60000);

check('a list and its detail split at the fold, whatever the window\'s width or the list\'s', async () => {
  // LLP 1078 D6: Chromium's own fold (a display feature) makes two viewport segments, here the iPhone Duo's at 130°
  // (LLP 1078: 0–455.5 and 495.5–951, a 40-point band), in whole pixels
  const stack = ({ extra = '', dir = 'ltr', inner = '', rows = true, nav = 'width:100%' } = {}) => `<main id="nav" navigationKey="1" navigationBack="back" dir="${dir}" style="${nav};height:100%;position:relative">${list('r1', 1, rows, extra, inner)}${detail('r2', 2)}</main>`;
  const folded = (call, width, offset, maskLength, orientation = 'vertical') => call('Emulation.setDeviceMetricsOverride', { width, height: 800, deviceScaleFactor: 1, mobile: false, displayFeature: { orientation, offset, maskLength } })
    .then(() => call('Runtime.evaluate', { expression: 'new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))', awaitPromise: true }));
  const pick = `{ const row = document.getElementById('r1-bo'); row.addEventListener('click', () => push('2', '/people/bo'), {once:true}); row.click(); }`;
  const split = `document.getElementById('r1').getAttribute('data-exact-split')`;
  await inChrome(page(stack()), async (evaluate, size, click, call, reopen) => {
    // The list is the leading segment, and the platform's placeholder and then the detail are the trailing one
    await folded(call, 951, 456, 40);
    expect(await evaluate(`[${split}, box('r1'), placeholder('nav')?.[1]]`)).toEqual(['list', [0, 0, 456, 800], 496]);
    await evaluate(pick);
    expect(await evaluate(`[shown('r1'), shown('r2'), box('r2'), back('r2')]`)).toEqual([true, true, [496, 0, 455, 800], false]);
    // Narrower than the wide rule's 900, a window across a fold still splits there
    await folded(call, 700, 330, 40);
    expect(await evaluate(`[box('r1'), box('r2')]`)).toEqual([[0, 0, 330, 800], [370, 0, 330, 800]]);
    // A horizontal fold keeps the wide rule, as UIKit's detail spans the Duo's tall inner display
    await folded(call, 951, 380, 40, 'horizontal');
    expect(await evaluate(`[box('r1'), box('r2')]`)).toEqual([[0, 0, 320, 800], [320, 0, 631, 800]]);
    // The fold gone: the wide rule again, then a phone's pushed stack
    await size(1280);
    expect(await evaluate(`[box('r1'), box('r2')]`)).toEqual([[0, 0, 320, 800], [320, 0, 960, 800]]);
    await size(390);
    expect(await evaluate(`[shown('r1'), box('r2')]`)).toEqual([false, [0, 0, 390, 800]]);
    // A page that opens folded hears the unfold, which keeps the window's size and so fires no resize
    await folded(call, 951, 456, 40);
    await reopen();
    expect(await evaluate(`window.resizes = 0; addEventListener('resize', () => resizes++); [innerWidth, box('r1')]`)).toEqual([951, [0, 0, 456, 800]]);
    await call('Emulation.setDeviceMetricsOverride', { width: 951, height: 800, deviceScaleFactor: 1, mobile: false });
    expect(await evaluate(`new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('the unfold was not projected: ' + box('r1'))), 10000);
      (function check() { if (box('r1')[2] === 320) { clearTimeout(timer); resolve(resizes); } else requestAnimationFrame(check); })();
    })`)).toBe(0);
  });
  // The fold wins over a named width, which also splits rows the rule does not find; `none` still opts out
  await inChrome(page(stack({ extra: 'navigationSplit="280"', rows: false })), async (evaluate, size, click, call) => {
    await folded(call, 951, 456, 40);
    expect(await evaluate(`[${split}, box('r1')]`)).toEqual(['list', [0, 0, 456, 800]]);
  });
  await inChrome(page(stack({ extra: 'navigationSplit="none"' })), async (evaluate, size, click, call) => {
    await folded(call, 951, 456, 40);
    expect(await evaluate(`[${split}, placeholder('nav')]`)).toEqual([null, null]);
  });
  // A stack beside a rail, or within one segment, keeps the wide rule, which pushes across this fold, rather than
  // split into a sliver; a stack inset by a border still splits at the fold
  for (const nav of ['width:auto;margin-left:240px', 'width:400px']) {
    await inChrome(page(stack({ nav })), async (evaluate, size, click, call) => {
      await folded(call, 951, 456, 40);
      expect(await evaluate(`[${split}, placeholder('nav')]`)).toEqual([null, null]);
    });
  }
  await inChrome(page(stack({ nav: 'width:100%;box-sizing:border-box;border:1px solid' })), async (evaluate, size, click, call) => {
    await folded(call, 951, 456, 40);
    await evaluate(pick);
    expect(await evaluate(`[box('r1'), box('r2')]`)).toEqual([[1, 1, 455, 798], [496, 1, 454, 798]]);
  });
  // Right to left, the list is the segment at the inline start, on the right
  await inChrome(page(stack({ dir: 'rtl' })), async (evaluate, size, click, call) => {
    await folded(call, 951, 456, 40);
    await evaluate(pick);
    expect(await evaluate(`[box('r1'), box('r2')]`)).toEqual([[496, 0, 455, 800], [0, 0, 456, 800]]);
  });
  // The agent's substitute, set from another copy of the module as the JS target's agent is, moves the split when the
  // agent announces it, and only then
  await inChrome(page(stack()), async (evaluate) => {
    const prefer = (fold) => evaluate(`import('./agent-navigation.js').then(({ preferFold, FOLD_EVENT }) => { preferFold(${JSON.stringify(fold)}); return FOLD_EVENT; })`);
    const event = await prefer({ cols: 2, rows: 1, gap: 40 });
    expect(await evaluate(`box('r1')`)).toEqual([0, 0, 320, 800]);
    await evaluate(`dispatchEvent(new Event(${JSON.stringify(event)}))`);
    expect(await evaluate(`box('r1')`)).toEqual([0, 0, 456, 800]);
    await prefer(null);
    await evaluate(`dispatchEvent(new Event(${JSON.stringify(event)}))`);
    expect(await evaluate(`box('r1')`)).toEqual([0, 0, 320, 800]);
  }, 951);
  // The author's placeholder is the trailing segment, a pixel early, inside the band, as the previous test's OWED note says
  const own = '<div navigationSplit="placeholder" id="own" style="display:none"><span>Pick a person</span></div>';
  await inChrome(page(stack({ inner: own })), async (evaluate, size, click, call) => {
    await folded(call, 951, 456, 40);
    expect(await evaluate(`box('own')`)).toEqual([495, 0, 455, 800]);
  });
}, 120000);

// Stage 6d's projection inputs. These DOM fixtures test navigation.js, not
// whether either compiler emits the facts; compiled-plan coverage is separate.
const ruleStack = body => `<main id="nav" navigationKey="2" navigationBack="back" style="width:100%;height:100%;position:relative">
  <div id="r1" navigationKey="1" navigationSplit="split" style="${abs}"><header><h1>People</h1></header>${body}</div>${detail('r2', 2)}</main>`;

check('a served document stays unsplit across resize until its target adopts it', () => {
  const body = '<div data-exact-list><button data-exact-router-press="true">Open</button></div>';
  const stack = ruleStack(body).replace('navigationSplit="split"', 'navigationSplit="280"');
  return inChrome(page(stack, [], true), async (evaluate, size) => {
    const state = `[document.getElementById('r1').getAttribute('data-exact-split'), document.getElementById('r2').getAttribute('data-exact-split'), document.getElementById('r1').style.getPropertyValue('--exact-split'), document.getElementById('r1').inert, document.getElementById('r2').inert]`;
    expect(await evaluate(state)).toEqual([null, null, '', false, false]);
    await size(390); await size(1280);
    expect(await evaluate(state)).toEqual([null, null, '', false, false]);
    await evaluate('activate()');
    expect(await evaluate(state)).toEqual(['list', 'detail', '280px', false, false]);
  });
}, 60000);

// The list the rule finds in route r1, by its id ("" for none), or null where it finds none
const splitList = `import('/navigation.js').then(({ pushingList }) => pushingList(document.getElementById('r1'))?.list.id ?? null)`;

check('the split\'s list is found from list and outer press facts, not roles or decorative chevrons', () => {
  const link = '<a href="/item/a">A</a>';
  const cases = [
    ['link without a chevron', `<div data-exact-list role="presentation">${link}</div>`, true],
    ['link beside decoration', `<div data-exact-list>${link}<button data-exact-on="press" data-exact-router-press="false">Decorative${chevron}</button></div>`, true],
    ['decorative list', `<div data-exact-list><button data-exact-on="press" data-exact-router-press="false">Decorative${chevron}</button></div>`, false],
    ['press outside a list', `<button data-exact-on="press" data-exact-router-press="true">Open${chevron}</button>`, false],
    ['handler beats href', `<div data-exact-list><a href="/item/a" data-exact-on="press" data-exact-router-press="false">Pick${chevron}</a></div>`, false],
    ['outer press beats inner press', '<div data-exact-list><div data-exact-on="press" data-exact-router-press="false"><button data-exact-on="press" data-exact-router-press="true">Open</button></div></div>', false],
    ['another route before a list', `<div navigationKey="nested"><div data-exact-list>${link}</div></div>`, false],
    ['route key inside list rows', `<div data-exact-list><div navigationKey="nested">${link}</div></div>`, true],
    ['sideways outer list', `<div data-exact-list style="display:flex;flex-direction:row"><div data-exact-list>${link}</div></div>`, true],
    ['a list in the placeholder', `<div navigationSplit="placeholder"><div data-exact-list>${link}</div></div>`, false],
  ];
  return inChrome(page(ruleStack(''), ['/item/a']), async evaluate => {
    const observed = [];
    for (const [name, body] of cases) {
      // A new root identity isolates each case from the list retained for the one before.
      const route = `<div id="r1" navigationKey="1" navigationSplit="split" style="${abs}">${body}</div>`;
      observed.push([name, await evaluate(`document.getElementById('r1').outerHTML = ${JSON.stringify(route)}; select(2); ${splitList}`) !== null]);
    }
    expect(observed).toEqual(cases.map(([name, , found]) => [name, found]));
  });
}, 60000);

check('the web splits only a stack whose root route asks: "split", a width or a placeholder box of its own', () => {
  const rows = `<div data-exact-list><button data-exact-on="press" data-exact-router-press="true">Open${chevron}</button></div>`;
  const own = '<div navigationSplit="placeholder" style="display:none">Pick one</div>';
  const cases = [
    ['a list whose rows push, unasked', '', rows, false],
    ['"split" with no list', 'navigationSplit="split"', '', true],
    ['a width with no list', 'navigationSplit="280"', '', true],
    ['its own placeholder box', '', own, true],
    ['a placeholder in a box of the route', '', `<div>${own}</div>`, false],
    ['"none"', 'navigationSplit="none"', rows, false],
    ['a fraction of a px', 'navigationSplit="280.5"', rows, false],
    ['a word no host reads', 'navigationSplit="auto"', rows, false],
    ['"none" over its own placeholder', 'navigationSplit="none"', own, false],
    ['its own placeholder animating out, gone at once as on the native hosts', '', own.replace('<div ', '<div data-exiting '), false],
  ];
  return inChrome(page(ruleStack('')), async evaluate => {
    const observed = [];
    for (const [name, said, body] of cases) {
      const route = `<div id="r1" navigationKey="1" ${said} style="${abs}"><header><h1>People</h1></header>${body}</div>`;
      observed.push([name, await evaluate(`document.getElementById('r1').outerHTML = ${JSON.stringify(route)}; select(2); document.getElementById('r1').hasAttribute('data-exact-split')`)]);
    }
    expect(observed).toEqual(cases.map(([name, , , splits]) => [name, splits]));
  });
}, 60000);

check('a later pushing list wins over a retained empty list, whose identity alone keeps it the split\'s list', () => {
  const body = `<div data-exact-list id="first"><button data-exact-on="press" data-exact-router-press="true">Open${chevron}</button></div>
    <div data-exact-list id="later" style="display:none"><a id="later-row" href="/item/b">B</a></div>`;
  return inChrome(page(ruleStack(body), ['/item/b']), async evaluate => {
    expect(await evaluate(`document.getElementById('r1').hasAttribute('data-exact-split')`)).toBe(true);
    await evaluate(`document.getElementById('first').textContent = 'No Results'; document.getElementById('later').style.display = 'block'; push(2, '/item/b', [[1, '/'], [2, '/item/b']])`);
    expect(await evaluate(`document.getElementById('later-row').hasAttribute('data-exact-split-row')`)).toBe(true);
    // The route asks, so the split stays, and the emptied list is still the one its identity retains
    await evaluate(`document.getElementById('later').textContent = 'No Results'; select(2)`);
    expect(await evaluate(`Promise.all([document.getElementById('r1').hasAttribute('data-exact-split'), ${splitList}])`)).toEqual([true, 'later']);
    // The first empty list was not the retained candidate after the later one pushed
    await evaluate(`document.getElementById('later').remove(); select(2)`);
    expect(await evaluate(`Promise.all([document.getElementById('r1').hasAttribute('data-exact-split'), ${splitList}])`)).toEqual([true, null]);
  });
}, 60000);

check('a bare plan rechecks the split\'s list on a resize that stays above the wide threshold, and the ask holds the split', () => {
  const body = `<div data-exact-list id="resize-list"><button data-exact-on="press" data-exact-router-press="true">Open${chevron}</button></div>`;
  const html = page(ruleStack(body)) + '<style>@media (min-width:1100px) { #resize-list { display:flex;flex-direction:row } }</style>';
  // No exactViewport runtime and no authored or visible default placeholder.
  return inChrome(html, async (evaluate, size) => {
    expect(await evaluate(`[document.querySelector('[navigationSplit="placeholder"]') === null, document.querySelector('[data-exact-split-empty]') === null, document.getElementById('r1').hasAttribute('data-exact-split')]`)).toEqual([true, true, true]);
    await size(1200);
    expect(await evaluate(`Promise.all([getComputedStyle(document.getElementById('resize-list')).flexDirection, document.getElementById('r1').hasAttribute('data-exact-split'), back('r2'), ${splitList}])`)).toEqual(['row', true, false, null]);
    await size(1000);
    expect(await evaluate(`Promise.all([document.getElementById('r1').hasAttribute('data-exact-split'), back('r2'), ${splitList}])`)).toEqual([true, false, 'resize-list']);
  }, 1000);
}, 60000);

check('turning a split off clears its selected mark and stale row ownership cannot hide deeper Back', () => {
  const body = `<div data-exact-list><button id="owned-row" data-exact-on="press" data-exact-router-press="true">Open${chevron}</button></div>`;
  const html = ruleStack(body).replace('</main>', `${detail('r3', 3)}</main>`);
  return inChrome(page(html), async evaluate => {
    await evaluate(`push(2, '/item/a'); { const row = document.getElementById('owned-row'); row.addEventListener('click', () => push(3, '/item/b'), {once:true}); row.click(); }`);
    expect(await evaluate(`[document.getElementById('owned-row').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([true, false]);
    await evaluate(`document.getElementById('r1').setAttribute('navigationSplit', 'none'); select(3)`);
    expect(await evaluate(`[document.getElementById('owned-row').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([false, true]);
    // Keep an explicit split width, but revoke the old row's router-writing fact.
    await evaluate(`document.getElementById('r1').setAttribute('navigationSplit', '280'); document.getElementById('owned-row').setAttribute('data-exact-router-press', 'false'); select(3)`);
    expect(await evaluate(`[box('r1')[2], document.getElementById('owned-row').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([280, false, true]);
  });
}, 60000);

check('a detail no row pushed marks the row the app says is current', () => {
  const body = `<div data-exact-list><button id="row-a" data-exact-on="press" data-exact-router-press="true">A${chevron}</button>
    <button id="row-b" data-exact-on="press" data-exact-router-press="true" aria-current="page">B${chevron}</button></div>`;
  return inChrome(page(ruleStack(body)), async evaluate => {
    const marks = `[...document.querySelectorAll('[data-exact-split-row]')].map(e => e.id)`;
    expect(await evaluate(`push(2, '/item/b'); ${marks}`)).toEqual(['row-b']);
    // A row's press names its detail's row, whatever the app says is current.
    expect(await evaluate(`{ const row = document.getElementById('row-a'); row.addEventListener('click', () => push(2, '/item/a', [[1, '/'], [2, '/item/a']]), {once:true}); row.click(); } ${marks}`)).toEqual(['row-a']);
    // With no detail shown no row is marked.
    expect(await evaluate(`select(1); ${marks}`)).toEqual([]);
  });
}, 60000);

check('check advises on a bottom tab bar at a desktop width, holds its advice for a list, and advises on neither at a phone\'s', () => {
  const positioned = BAR.replace('style="display:flex;flex-direction:row"', 'style="display:flex;flex-direction:row;position:absolute;left:0;right:0;bottom:0"');
  const html = page(tabs(positioned).replaceAll(chevron, '').replace('id="r1" style="position:absolute', 'id="r1" style="position:relative')) + `<script type="module">
    import { observe } from './check.js';
    window.advice = () => { const views = new Map([...document.querySelectorAll('#exact-root *')].map(e => [e.id || e.localName, e])); return (observe(views).advice ?? []).map(a => a.kind + ' ' + a.id); };
  </script>`;
  return inChrome(html, async (evaluate, size) => {
    await evaluate(`select('1')`);
    expect(await evaluate('advice()')).toEqual(['tabbar bar']);
    await evaluate(`select('2')`);
    expect(await evaluate('advice()')).toEqual(['tabbar bar']);
    await size(390);
    expect(await evaluate('advice()')).toEqual([]);
  });
}, 60000);

check('the split and sidebar leave check nothing to advise', () => inChrome(page(tabs(BAR)) + `<script type="module">
    import { observe } from './check.js';
    window.advice = () => { const views = new Map([...document.querySelectorAll('#exact-root *')].map(e => [e.id || e.localName, e])); return observe(views).advice ?? []; };
  </script>`, async (evaluate) => {
  expect(await evaluate(`select('2'); advice()`)).toEqual([]);
}), 60000);

// LLP 1116.005 D3: each route keeps its own scroll (LLP 1038 §7: mounted rows keep theirs), on a phone and through a
// split. A route of `n` tall rows over its own scroller; `rows` writes the router, and its stack asks to split.
const tall = (id, key, n, rows = false) => `<div navigationKey="${key}" id="${id}" style="${abs}" ${asks(rows, '')}><header><h2>${id}</h2></header><div id="${id}-scroll" ${rows ? 'data-exact-list' : ''} style="flex:1 1 0%;min-height:0;overflow:auto">${
  Array.from({ length: n }, (_, i) => rows ? `<button data-exact-on="press" data-exact-router-press="true" style="display:flex;height:50px"><span data-exact-text>${i}</span>${chevron}</button>` : `<div style="height:50px">${i}</div>`).join('')}</div></div>`;

check('a pushed route opens at its top, and Back finds the list where it was', () => inChrome(page(`<main id="nav" navigationKey="1" navigationBack="back" style="width:100%;height:100%;position:relative">${tall('r1', 1, 60, true)}</main>`), async (evaluate, size) => {
  // The renderer's rows: a push adds the route's row, a pop removes it, a `replace` keeps it (its key is the visit's id).
  const enter = (key, url) => `document.getElementById('nav').insertAdjacentHTML('beforeend', ${JSON.stringify(tall(`r${key}`, key, 40))}); push('${key}', '${url}')`;
  const pop = `document.getElementById('r2').remove(); push('1', '/')`;
  const tops = `[...document.querySelectorAll('[id$="-scroll"]')].map(e => Math.round(e.scrollTop))`;
  await size(390);
  // A phone: the list scrolled, a detail pushed over it opens at its top; Back shows the list at its offset.
  await evaluate(`document.getElementById('r1-scroll').scrollTop = 1500`);
  await evaluate(enter(2, '/item/30'));
  expect(await evaluate(`[shown('r1'), shown('r2'), ${tops}]`)).toEqual([false, true, [1500, 0]]);
  await evaluate(`document.getElementById('r2-scroll').scrollTop = 600; ${pop}`);
  expect(await evaluate(`[shown('r1'), ${tops}]`)).toEqual([true, [1500]]);
  // Wide: the list column keeps its offset beside a detail that opens at its top.
  await size(1280);
  await evaluate(enter(2, '/item/30'));
  expect(await evaluate(`[shown('r1'), shown('r2'), document.getElementById('r1').getAttribute('data-exact-split'), ${tops}]`)).toEqual([true, true, 'list', [1500, 0]]);
  await evaluate(pop);
  expect(await evaluate(`[shown('r1'), ${tops}]`)).toEqual([true, [1500]]);
}), 60000);

check('retained visit URLs restore href selection after adoption, replace and same-top reset', () => {
  const body = `<div data-exact-list role="presentation" id="rows"><div data-listitemkey="a"><a id="row-a" href="/item/a">A<button data-exact-router-press="false">Star</button></a></div><a id="row-b" href="/item/b">B</a></div>`;
  const html = ruleStack(body).replace('</main>', `${detail('r3', 3)}</main>`);
  return inChrome(page(html, ['/item/a', '/item/b']), async evaluate => {
    const marks = `[...document.querySelectorAll('[data-exact-split-row]')].map(e => e.id)`;
    // A restored deeper route inherits its underlying row's selection, but keeps Back.
    await evaluate(`push(3, '/edit', [[1, '/'], [2, '/item/a'], [3, '/edit']])`);
    expect(await evaluate(`[${marks}, back('r2'), back('r3')]`)).toEqual([['row-a'], false, true]);
    await evaluate(`push(3, '/edit', [[1, '/'], [2, '/item/b'], [3, '/edit']])`);
    expect(await evaluate(marks)).toEqual(['row-b']);
    // A same-top reset must replace the snapshot before its history fast path returns.
    await evaluate(`(async () => { const {navigation} = await import('/navigation.js'); navigation.reset({top:3,url:'/edit',visits:[[1,'/'],[2,'/unknown'],[3,'/edit']]}); select(3); })()`);
    expect(await evaluate(`[${marks}, back('r3')]`)).toEqual([[], true]);
    await evaluate(`push(3, '/edit', [[1, '/'], [2, '/item/a'], [3, '/edit']]); document.getElementById('row-a').setAttribute('data-exact-router-press', 'false'); select(3)`);
    expect(await evaluate(marks)).toEqual([]);
    await evaluate(`document.getElementById('row-a').removeAttribute('data-exact-router-press'); select(3)`);
    expect(await evaluate(marks)).toEqual(['row-a']);
    // A hidden list marks no row, and the route's ask holds the split
    await evaluate(`document.getElementById('rows').style.display = 'none'; select(3)`);
    expect(await evaluate(`[${marks}, document.getElementById('r1').hasAttribute('data-exact-split')]`)).toEqual([[], true]);
    await evaluate(`document.getElementById('rows').style.display = ''; select(3)`);
    expect(await evaluate(marks)).toEqual(['row-a']);
  });
}, 60000);

check('only an ordinary outer press associates its row with navigation', () => {
  const body = '<div data-exact-list><div id="owner" data-exact-router-press="true"><button id="star" data-exact-router-press="false">Star</button></div><a id="action-link" href="/item/b" data-exact-router-press="true">Linked action</a></div>';
  const html = ruleStack(body).replace('</main>', `${detail('r3', 3)}</main>`);
  return inChrome(page(html), async (evaluate, _size, click) => {
    // A refused action owns no later timer/resource/keyboard navigation after this click dispatch ends.
    // The first detail has no Back in a split whoever owns it, so ownership shows on a push inside it.
    await evaluate(`push(2, '/item/a'); document.getElementById('owner').click(); push(3, '/item/b')`);
    expect(await evaluate(`[document.getElementById('owner').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([false, true]);
    // The host leaves these link clicks to the browser, so a later router change owns no row.
    for (const modifier of ['ctrlKey', 'metaKey', 'shiftKey', 'altKey']) {
      await evaluate(`push(2, '/item/a'); { const link = document.getElementById('action-link'); link.addEventListener('click', e => e.preventDefault(), {once:true}); link.dispatchEvent(new MouseEvent('click', {bubbles:true,cancelable:true,${modifier}:true})); } push(3, '/edit')`);
      expect(await evaluate(`[document.getElementById('action-link').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([false, true]);
    }
    await evaluate(`push(2, '/item/a'); document.getElementById('star').click(); push(3, '/edit')`);
    expect(await evaluate(`[document.getElementById('owner').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([false, true]);
    await evaluate(`document.getElementById('owner').addEventListener('click', () => push(3, '/item/b'), {once:true})`);
    await click('owner');
    expect(await evaluate(`[document.getElementById('owner').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([true, false]);
    await evaluate(`(async () => { const {navigation} = await import('/navigation.js'); navigation.reset({top:3,url:'/item/b',visits:[[3,'/item/b']]}); select(3); })()`);
    expect(await evaluate(`[document.getElementById('owner').hasAttribute('data-exact-split-row'), back('r3')]`)).toEqual([false, true]);
  });
}, 60000);
