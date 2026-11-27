import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// LLP 1115.000 D4.8 (amended 2026-11-12) in headless Chrome: on a phone-width window a sheet opens at the detent iOS
// opens it at (`navigationInitialDetent`, else the first of `navigationDetent`), its foot on the window's; `large`,
// no detent, a window of UIKit's compact height and a wide window keep the tallest sheet or the card. The shell's
// stylesheet and the navigation glue's projection (navigation.js, which both web targets run), over nodes as the
// hosts write them.
import { expect } from 'bun:test';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cdpBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { withLook } from '../look.mjs';
import { readyPage } from './page.mjs';

const WEB = resolve(fileURLToPath(new URL('..', import.meta.url)));
const { unavailable } = chromium();
const check = need('chrome', unavailable);

const style = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>[\s\S]*?<\/style>/)[0];
const abs = 'position:absolute;inset:0;display:flex;flex-direction:column';
// The root's routes: a page, a sheet over it, a route pushed inside that sheet, a second sheet, whose detents the
// test writes (as `data-navigationdetent` and `data-navigationinitialdetent`, the attributes both web targets
// write), then a full-screen route that paints its own ground and a sheet over it. The page covers the window's safe
// areas, so an inset the test emulates reaches `env()`.
const page = '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">' + style + `<div id="exact-root"><main id="nav" navigationKey="1" navigationBack="back" style="width:100%;height:100%;position:relative">
  <div navigationKey="1" id="home" style="${abs}"><header><h1>Home</h1></header></div>
  <div navigationKey="2" id="sheet" navigationPresentation="modal" style="${abs}"><header><h2>Sheet</h2></header><div id="rows" style="height:120px"></div><input id="field"></div>
  <div navigationKey="3" id="pushed" style="${abs}"><header><h2>Pushed</h2></header></div>
  <div navigationKey="4" id="over" navigationPresentation="modal" style="${abs}"><header><h2>Over</h2></header></div>
  <div navigationKey="5" id="full" navigationPresentation="fullscreen" style="${abs};background:black"><header><h2>Full</h2></header></div>
  <div navigationKey="6" id="top" navigationPresentation="modal" style="${abs}"><header><h2>Top</h2></header></div>
</main></div>
<script type="module">
  import { navigation } from './navigation.js';
  const root = document.getElementById('exact-root');
  navigation.connect(root, () => false, () => {}, () => {}, null, () => false, false);
  window.select = key => { document.getElementById('nav').setAttribute('navigationKey', key); navigation.project(root, () => {}); };
  // A route's detents as a host writes them, and the sheet laid out where it stands once it has slid up.
  window.detents = (id, said, initial) => {
    const e = document.getElementById(id);
    said == null ? e.removeAttribute('data-navigationdetent') : e.setAttribute('data-navigationdetent', said);
    initial == null ? e.removeAttribute('data-navigationinitialdetent') : e.setAttribute('data-navigationinitialdetent', initial);
  };
  window.shown = id => getComputedStyle(document.getElementById(id)).visibility === 'visible';
  window.box = id => { const e = document.getElementById(id); e.getAnimations().forEach(a => a.finish()); const b = e.getBoundingClientRect(); return [b.y, b.height].map(v => Math.round(v * 10) / 10); };
  select('1');
  window.ready = true;
</script>`;

/** The page in headless Chrome: `test(evaluate, size, call)`, `size(w, h)` resizing the viewport. */
async function inChrome(test) {
  const server = createServer((req, res) => {
    if (req.url === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(page); return; }
    res.writeHead(200, { 'content-type': 'text/javascript' }); res.end(readFileSync(resolve(WEB, 'navigation.js')));
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
    const size = async (width, height) => {
      await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
      await evaluate('new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))');
    };
    await size(390, 844);
    await readyPage(cdp, sessionId, `http://127.0.0.1:${server.address().port}/`);
    await test(evaluate, size, call);
  } finally {
    await suite.close();
    server.close();
  }
}

// At 390 × 844 the tallest sheet starts 12 px below the top: 832 tall, so `medium` is 416.
const open = (evaluate, said, initial) => evaluate(`select('1'); detents('sheet', ${JSON.stringify(said)}, ${JSON.stringify(initial)}); select('2'); box('sheet')`);

check('a narrow sheet opens at its first detent, or the one navigationInitialDetent names', () => inChrome(async (evaluate) => {
  expect(await open(evaluate, 'medium large')).toEqual([428, 416]);
  expect(await open(evaluate, 'medium large', 'large')).toEqual([12, 832]);
  expect(await open(evaluate, '300 large')).toEqual([544, 300]);
  expect(await open(evaluate, '45%')).toEqual([469.6, 374.4]);
  expect(await open(evaluate, '2000 large')).toEqual([12, 832]);
  // `fit-content` is the route's content: its header and rows.
  const [top, height] = await open(evaluate, 'fit-content');
  expect(await evaluate(`[document.getElementById('sheet').scrollHeight, ${top + height}]`)).toEqual([height, 844]);
  expect(height).toBeLessThan(416);
  // Unsaid, empty or `large`: the tallest sheet, as before.
  for (const said of [null, '', 'large']) expect(await open(evaluate, said)).toEqual([12, 832]);
  // A word the detents do not hold is no initial detent (the compiler refuses a literal one); the first stands.
  expect(await open(evaluate, 'medium large', 'fit-content')).toEqual([428, 416]);
  // Terminal `fit-content` ceiling (LLP 1075.003 §9.11, expo/exact#2023): short content shrinks to fit,
  // tall content caps at the initial stop without JS measurement.
  const [, fitHeight] = await open(evaluate, 'fit-content');
  expect(await open(evaluate, 'medium fit-content')).toEqual([844 - fitHeight, fitHeight]);
  expect(await evaluate(`document.getElementById('sheet').getAttribute('data-exact-detent')`)).toBe('fit-capped');
  expect(await open(evaluate, '100 fit-content')).toEqual([744, 100]);
  expect(await open(evaluate, 'medium fit-content', 'fit-content')).toEqual([844 - fitHeight, fitHeight]);
}), 60000);

check('the detent is read as the sheet is presented and when its detents change, and each sheet keeps its own', () => inChrome(async (evaluate) => {
  expect(await open(evaluate, 'medium large')).toEqual([428, 416]);
  // The initial detent alone changing does not move the open sheet; new detents do.
  expect(await evaluate(`detents('sheet', 'medium large', 'large'); select('2'); box('sheet')`)).toEqual([428, 416]);
  expect(await evaluate(`detents('sheet', '300 large', 'large'); select('2'); box('sheet')`)).toEqual([12, 832]);
  expect(await evaluate(`detents('sheet', '300'); select('2'); box('sheet')`)).toEqual([544, 300]);
  // Presented again, it reads them again.
  expect(await open(evaluate, '300 large', 'large')).toEqual([12, 832]);
  // A sheet over a sheet: each at its own height, the one beneath where it stood.
  expect(await evaluate(`detents('over', 'medium large'); select('4'); [box('over'), box('sheet')]`)).toEqual([[428, 416], [12, 832]]);
}), 60000);

check('a short window, a wide one and a resize between them keep the platform shape', () => inChrome(async (evaluate, size) => {
  expect(await open(evaluate, 'medium large')).toEqual([428, 416]);
  // UIKit's compact height (a phone on its side): always the tallest sheet.
  await size(800, 400);
  expect(await evaluate(`box('sheet')`)).toEqual([12, 388]);
  // A wide window: the card, as tall as iPad's form sheet (45rem), in the window's middle.
  await size(1280, 900);
  expect(await evaluate(`box('sheet')`)).toEqual([90, 720]);
  // Back at a phone's size and a taller phone: the detent again, half the tallest sheet.
  await size(390, 844);
  expect(await evaluate(`box('sheet')`)).toEqual([428, 416]);
  await size(430, 932);
  expect(await evaluate(`box('sheet')`)).toEqual([472, 460]);
}), 60000);

check('on a touch screen a field with the focus raises the sheet to the tallest, as the keyboard does on iOS', () => inChrome(async (evaluate, size, call) => {
  expect(await open(evaluate, 'medium large')).toEqual([428, 416]);
  // A mouse's focus leaves it where it is.
  expect(await evaluate(`document.getElementById('field').focus(); box('sheet')`)).toEqual([428, 416]);
  await evaluate(`document.getElementById('field').blur()`);
  await call('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 1 });
  await size(390, 844);
  expect(await evaluate(`matchMedia('(pointer: coarse)').matches`)).toBe(true);
  expect(await evaluate(`box('sheet')`)).toEqual([428, 416]);
  expect(await evaluate(`document.getElementById('field').focus(); box('sheet')`)).toEqual([12, 832]);
}), 60000);

check('a bound value keeps the detents UIKit keeps: a word it cannot read or one out of order is dropped', () => inChrome(async (evaluate) => {
  expect(await open(evaluate, '0% large')).toEqual([12, 832]);
  expect(await open(evaluate, 'tall 300 large')).toEqual([544, 300]);
  expect(await open(evaluate, '300 200 large')).toEqual([544, 300]);
  expect(await open(evaluate, 'large 300')).toEqual([12, 832]);
  expect(await open(evaluate, '45% 300 large')).toEqual([469.6, 374.4]);
  expect(await open(evaluate, '100% 300')).toEqual([12, 832]);
  // The initial detent names a kept word: a dropped one is none, and the first kept stands.
  expect(await open(evaluate, '300 200 large', '200')).toEqual([544, 300]);
  // Terminal `fit-content` rejects a following `large`, and `large` rejects a following `fit-content`.
  expect(await open(evaluate, 'medium fit-content large', 'large')).toEqual([844 - (await open(evaluate, 'fit-content'))[1], (await open(evaluate, 'fit-content'))[1]]);
  expect(await open(evaluate, 'large fit-content', 'fit-content')).toEqual([12, 832]);
}), 60000);

check("an update that replaces a sheet's inline style keeps its height", () => inChrome(async (evaluate) => {
  expect(await open(evaluate, 'medium large')).toEqual([428, 416]);
  // The wasm target's style op (motion-glue.js `style`) replaces the route's whole inline style; its commit then
  // projects the navigation (commit.js).
  const restyle = `document.getElementById('sheet').style.cssText = ${JSON.stringify(abs + ';padding:4px')}`;
  expect(await evaluate(`${restyle}; select('2'); [box('sheet'), getComputedStyle(document.getElementById('sheet')).paddingTop]`)).toEqual([[428, 416], '4px']);
}), 60000);

check('a route pushed inside a sheet stays in it at its height, over the page the sheet covers', () => inChrome(async (evaluate, size) => {
  expect(await open(evaluate, 'medium large')).toEqual([428, 416]);
  expect(await evaluate(`select('3'); [box('pushed'), shown('pushed'), shown('home'), shown('sheet'), document.getElementById('home').hasAttribute('data-exact-covered')]`))
    .toEqual([[428, 416], true, true, false, true]);
  // Back to the sheet: at the height it had.
  expect(await evaluate(`select('2'); [box('sheet'), shown('pushed')]`)).toEqual([[428, 416], false]);
  // A wide window: the push is the sheet's card.
  await evaluate(`select('3')`);
  await size(1280, 900);
  expect(await evaluate(`box('pushed')`)).toEqual([90, 720]);
}), 60000);

check('the bottom safe area is added below every detent, a content-high one included', () => inChrome(async (evaluate, size, call) => {
  const [, content] = await open(evaluate, 'fit-content');
  await call('Emulation.setSafeAreaInsetsOverride', { insets: { bottom: 34 } });
  await size(390, 844);
  // The tallest sheet is 844 - 12 - 34 = 798 above the band: `medium` is 399 of it, 433 with the band.
  expect(await open(evaluate, 'medium large')).toEqual([411, 433]);
  expect(await open(evaluate, 'fit-content')).toEqual([844 - content - 34, content + 34]);
}), 60000);

// The sheet's content as an app writes a scrolled sheet: a header, then a scroller filling the rest whose column is at
// least its height, `rows` px of rows in it and the scroller's own bottom padding `pad` or class. What the scroller
// shows at its end: the sheet's frame, the last row's distance from the window's foot, the scroller's bottom padding
// and border, and whether it scrolls.
const scrolled = (evaluate, rows, pad = '', className = '') => evaluate(`{
  const sheet = document.getElementById('sheet');
  sheet.innerHTML = '<header><h2>Sheet</h2></header><div id="scroller" class="${className}" style="flex:1;min-height:0;overflow:auto;${pad && `padding-bottom:${pad}`}"><div style="min-height:100%;display:flex;flex-direction:column"><div style="height:${rows}px;flex-shrink:0"></div><div id="last" style="height:20px;flex-shrink:0"></div></div></div>';
  select('1'); select('2');
  const frame = box('sheet');
  [frame, ...end()];
}`);
const end = (evaluate) => evaluate(`end()`);
const endScript = `window.end = () => {
  const scroller = document.getElementById('scroller'), s = getComputedStyle(scroller);
  scroller.scrollTop = scroller.scrollHeight;
  return [Math.round(innerHeight - document.getElementById('last').getBoundingClientRect().bottom), s.paddingBottom, s.borderBottomWidth, scroller.scrollHeight > scroller.clientHeight];
}`;
const headerHeight = (evaluate) => evaluate(`document.querySelector('#sheet > header').getBoundingClientRect().height`);

check("a narrow sheet's bottom-edge scroller keeps its content above the bottom safe area once, short or tall, at any detent", () => inChrome(async (evaluate, size, call) => {
  await call('Emulation.setSafeAreaInsetsOverride', { insets: { bottom: 34 } });
  await size(390, 844);
  await evaluate(`${endScript}; detents('sheet', 'fit-content')`);
  // Short: the sheet is its content and the inset once, which the scroller's border holds; nothing scrolls.
  const short = await scrolled(evaluate, 200), header = await headerHeight(evaluate);
  expect(short).toEqual([[844 - header - 220 - 34, header + 220 + 34], 34, '0px', '34px', false]);
  // Tall: the tallest sheet, its last row ending above the inset.
  expect(await scrolled(evaluate, 2000)).toEqual([[12, 832], 34, '0px', '34px', true]);
  // An author's bottom padding, inline or from a class rule, is left alone and adds to the inset.
  expect(await scrolled(evaluate, 200, '10px')).toEqual([[844 - header - 220 - 44, header + 220 + 44], 44, '10px', '34px', false]);
  expect(await scrolled(evaluate, 2000, '10px')).toEqual([[12, 832], 44, '10px', '34px', true]);
  await evaluate(`document.head.insertAdjacentHTML('beforeend', '<style>.padded { padding-bottom: 10px } .more { padding-bottom: 30px } .tall { padding-bottom: 2vh }</style>')`);
  expect(await scrolled(evaluate, 200, '', 'padded')).toEqual([[844 - header - 220 - 44, header + 220 + 44], 44, '10px', '34px', false]);
  // A class switched with no projection, as the JS target writes an enumerated binding, and a length relative to the
  // window after a resize, still count.
  expect(await scrolled(evaluate, 2000, '', 'padded')).toEqual([[12, 832], 44, '10px', '34px', true]);
  await evaluate(`document.getElementById('scroller').className = 'more'`);
  expect(await end(evaluate)).toEqual([64, '30px', '34px', true]);
  await evaluate(`document.getElementById('scroller').className = 'tall'`);
  await size(390, 744);
  expect(await end(evaluate)).toEqual([49, '14.88px', '34px', true]);
  await size(390, 844);
  // A detent and the tallest sheet keep their heights, their scroller ending above the inset.
  await evaluate(`detents('sheet', 'medium large')`);
  expect(await scrolled(evaluate, 2000)).toEqual([[411, 433], 34, '0px', '34px', true]);
  await evaluate(`detents('sheet', null)`);
  expect(await scrolled(evaluate, 2000)).toEqual([[12, 832], 34, '0px', '34px', true]);
  // A wide window's card is clear of the window's edge.
  await size(1280, 900);
  expect(await scrolled(evaluate, 2000)).toEqual([[90, 720], 90, '0px', '0px', true]);
  // With no inset, nothing is added.
  await call('Emulation.setSafeAreaInsetsOverride', { insets: { top: 0, bottom: 0, left: 0, right: 0 } });
  await size(390, 844);
  await evaluate(`detents('sheet', 'fit-content')`);
  expect(await scrolled(evaluate, 200)).toEqual([[844 - header - 220, header + 220], 0, '0px', '0px', false]);
}), 60000);

check("an author's bottom border on a sheet's bottom-edge scroller replaces the inset", () => inChrome(async (evaluate, size, call) => {
  await call('Emulation.setSafeAreaInsetsOverride', { insets: { bottom: 34 } });
  await size(390, 844);
  await evaluate(`${endScript}; document.head.insertAdjacentHTML('beforeend', '<style>.lined { border-bottom: 1px solid black }</style>'); detents('sheet', 'fit-content')`);
  expect((await scrolled(evaluate, 2000, '', 'lined')).slice(2)).toEqual(['0px', '1px', true]);
}), 60000);

check('without calc-size() a content-high sheet whose scroller takes the inset adds no band of its own', () => inChrome(async (evaluate, size, call) => {
  await call('Emulation.setSafeAreaInsetsOverride', { insets: { bottom: 34 } });
  await size(390, 844);
  // The `@supports not (calc-size)` rules laid over the page, as a browser without calc-size() reads them.
  await evaluate(`{
    ${endScript};
    const rules = [], walk = list => { for (const r of list) r instanceof CSSSupportsRule && r.conditionText.includes('calc-size') ? rules.push(...[...r.cssRules].map(c => c.cssText)) : r.cssRules && walk(r.cssRules); };
    for (const s of document.styleSheets) walk(s.cssRules);
    const style = document.createElement('style');
    style.textContent = rules.join('\\n');
    document.head.append(style);
    detents('sheet', 'fit-content');
  }`);
  const short = await scrolled(evaluate, 200), header = await headerHeight(evaluate);
  expect(short).toEqual([[844 - header - 220 - 34, header + 220 + 34], 34, '0px', '34px', false]);
  expect(await evaluate(`getComputedStyle(document.getElementById('sheet')).borderBottomWidth`)).toBe('0px');
  expect(await scrolled(evaluate, 2000)).toEqual([[12, 832], 34, '0px', '34px', true]);
}), 60000);

check('a sheet whose last child in flow is not a scroller keeps the band below its content', () => inChrome(async (evaluate, size, call) => {
  await call('Emulation.setSafeAreaInsetsOverride', { insets: { bottom: 34 } });
  await size(390, 844);
  const [, banded] = await open(evaluate, 'fit-content');
  await evaluate(`{
    const scroller = document.createElement('div');
    scroller.id = 'scroller';
    scroller.style.cssText = 'height:40px;flex-shrink:0;overflow:auto';
    document.getElementById('sheet').prepend(scroller);
  }`);
  expect(await open(evaluate, 'fit-content')).toEqual([844 - banded - 40, banded + 40]);
  expect(await evaluate(`[getComputedStyle(document.getElementById('scroller')).borderBottomWidth, document.getElementById('sheet').hasAttribute('data-exact-bottom-scroller')]`)).toEqual(['0px', false]);
  // A text area last is a field, not the sheet's content scroller.
  await evaluate(`document.getElementById('sheet').append(Object.assign(document.createElement('textarea'), { id: 'notes' }))`);
  await open(evaluate, 'fit-content');
  expect(await evaluate(`[document.getElementById('notes').hasAttribute('data-exact-bottom-inset'), document.getElementById('sheet').hasAttribute('data-exact-bottom-scroller')]`)).toEqual([false, false]);
}), 60000);

check('each presented route is drawn above every route beneath it, sheet or full screen', () => inChrome(async (evaluate, size) => {
  // The route painted at a point, with every slide ended: hit testing skips an inert route, so the covered ones are
  // made hittable for the read.
  const at = (x, y) => evaluate(`{
    const routes = [...document.querySelectorAll('[navigationKey]:not(main)')], inert = routes.filter(e => e.inert);
    routes.forEach(e => { e.getAnimations().forEach(a => a.finish()); e.inert = false; });
    const id = document.elementFromPoint(${x}, ${y}).closest('[navigationKey]:not(main)')?.id;
    inert.forEach(e => { e.inert = true; });
    id;
  }`);
  for (const [width, height] of [[390, 844], [1200, 800]]) {
    await size(width, height);
    await evaluate(`select('1'); select('4')`);
    expect([width, await at(width / 2, height / 2)]).toEqual([width, 'over']);
    // A full-screen route over the sheets covers them, as UIKit presents it above the sheet it is presented from.
    await evaluate(`select('5')`);
    expect([width, await at(width / 2, height / 2), await evaluate(`shown('over')`)]).toEqual([width, 'full', true]);
    // A sheet over the full-screen route is above it in turn.
    await evaluate(`select('6')`);
    expect([width, await at(width / 2, height - 100)]).toEqual([width, 'top']);
  }
}), 60000);

check('a content-high wide sheet keeps its heading and actions visible while its list scrolls', () => inChrome(async (evaluate, size) => {
  await evaluate(`{
    const sheet = document.getElementById('sheet');
    sheet.innerHTML = '<header style="display:flex;align-items:center;justify-content:space-between;padding:8px 16px"><button id="cancel" aria-label="Cancel">Cancel</button><h2>Form</h2><button id="save" aria-label="Save">Save</button></header><div id="form" data-liststyle="inset-grouped" style="flex:1;min-height:0;overflow:auto"><section><div>' + Array.from({ length: 20 }, (_, i) => '<div style="height:54px">Field ' + (i + 1) + '</div>').join('') + '</div></section></div>';
    for (const button of sheet.querySelectorAll('button')) button.onclick = () => select('1');
  }`);
  for (const [width, height] of [[980, 545], [899, 400], [900, 400], [1200, 300], [1200, 1000], [650, 400]]) {
    await size(width, height);
    await open(evaluate, null);
    const geometry = await evaluate(`{
      box('sheet');
      const sheet = document.getElementById('sheet'), list = document.getElementById('form');
      const b = sheet.getBoundingClientRect();
      const visible = e => {
        const r = e.getBoundingClientRect();
        return r.top >= Math.max(0, b.top) && r.bottom <= Math.min(innerHeight, b.bottom) && r.left >= Math.max(0, b.left) && r.right <= Math.min(innerWidth, b.right);
      };
      list.scrollTop = list.scrollHeight;
      [b.y, b.height, list.scrollHeight > list.clientHeight, list.scrollTop > 0,
        [...sheet.querySelectorAll('h2, button')].every(visible),
        getComputedStyle(sheet).maxHeight];
    }`);
    const wide = width >= 900, sheetHeight = wide ? Math.min(720, height - 64) : height - 12;
    const top = wide ? (height - sheetHeight) / 2 : 12;
    expect([width, height, geometry]).toEqual([width, height,
      [top, sheetHeight, true, true, true, wide ? sheetHeight + 'px' : 'none']]);
    await evaluate(`document.getElementById('save').click()`);
    expect(await evaluate(`shown('home') && !shown('sheet')`)).toBe(true);
  }
}), 60000);
