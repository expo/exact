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
// The root's routes: a page, a sheet over it, a route pushed inside that sheet, and a second sheet, whose detents
// the test writes (as `data-navigationdetent` and `data-navigationinitialdetent`, the attributes both web targets
// write). The page covers the window's safe areas, so an inset the test emulates reaches `env()`.
const page = '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">' + style + `<div id="exact-root"><main id="nav" navigationKey="1" navigationBack="back" style="width:100%;height:100%;position:relative">
  <div navigationKey="1" id="home" style="${abs}"><header><h1>Home</h1></header></div>
  <div navigationKey="2" id="sheet" navigationPresentation="modal" style="${abs}"><header><h2>Sheet</h2></header><div id="rows" style="height:120px"></div><input id="field"></div>
  <div navigationKey="3" id="pushed" style="${abs}"><header><h2>Pushed</h2></header></div>
  <div navigationKey="4" id="over" navigationPresentation="modal" style="${abs}"><header><h2>Over</h2></header></div>
</main></div>
<script type="module">
  import { navigation, useSheets, sheetFrame } from './navigation.js';
  const root = document.getElementById('exact-root');
  useSheets(sheetFrame); // as both web hosts do for a plan that can present a sheet
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
