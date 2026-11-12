// A hold adopts the `translate` the page shows (motion-glue.js `sample`), in px: an axis
// that is a length, a percentage, or the `calc()` of both that the glue's own spring
// writes from a px offset to a % rest. A percentage is of the untransformed reference
// box (CSS Transforms 2 §5, `transform-box`), not the element's transformed bounds.
import { beforeAll, afterAll, expect } from 'bun:test';
import { chromium, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';

const { unavailable } = chromium();
const check = need('chrome', unavailable);
let server, browser;
const fixture = `<!doctype html>
<style>body{margin:0}</style>
<div id="box" style="position:absolute;left:300px;top:20px;width:180px;height:80px;padding:10px;border:0 solid"></div>
<div id="scaled" style="position:absolute;left:300px;top:200px;scale:0.5"><div id="inner" style="width:200px;height:100px"></div></div>
<div id="row" style="position:absolute;left:300px;top:400px;width:200px;height:44px;touch-action:pan-y"></div>
<div id="sized" style="position:absolute;left:600px;top:20px;width:200px;height:100px;padding:10px 20px;border:5px solid;box-sizing:border-box"></div>
<svg id="art" viewBox="0 0 400 200" style="position:absolute;left:0;top:500px;width:200px;height:100px">
  <rect id="rect" x="10" y="10" width="40" height="20" stroke="black" stroke-width="4"/>
</svg>
<script type="module">
  import { motionController } from './motion-glue.js';
  const byId = id => document.getElementById(id);
  const views = new Map([[1, byId('box')], [2, byId('inner')], [3, byId('row')], [4, byId('rect')], [5, byId('sized')]]);
  window.calls = [];
  let serial = 0;
  window.motion = motionController({ views, now: () => performance.now(), generation: () => 1,
    inert: () => false, applyBatch() {}, request(r) {
      if (r.op === 'gesture') return { slop: 4, knee: 64, resistance: 0.2 };
      calls.push(r);
      if (r.op === 'begin') return { token: String(++serial), value: [r.x, r.y] };
      return { accepted: true };
    } });
  byId('row').exactHandlers = ['swiperight', 'swipeleft'];
  motion.attachSwipe(byId('row'), 3, (type, fn) => byId('row').addEventListener(type, fn));
  // The value a hold on view \`id\` begins at, with \`style\` set on it first.
  window.caught = (id, style) => {
    const el = views.get(id);
    el.style.cssText += style;
    calls.length = 0;
    const h = motion.begin(id, 'translate');
    if (h) motion.end(h, [0, 0], true);
    el.style.removeProperty('translate'); el.style.removeProperty('scale'); el.style.removeProperty('rotate');
    el.style.removeProperty('transform-box');
    const begin = calls.find(c => c.op === 'begin');
    return begin ? [begin.x, begin.y].map(v => Math.round(v * 1000) / 1000) : null;
  };
</script>`;

beforeAll(async () => {
  if (unavailable) return;
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    return path === '/' ? new Response(fixture, { headers: { 'content-type': 'text/html' } })
      : new Response(Bun.file(new URL('..' + path, import.meta.url)));
  } });
  browser = await suiteBrowser();
});
afterAll(async () => {
  await browser?.close();
  server?.stop(true);
});

async function open() {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.port}/`);
  await page.waitForFunction(() => window.caught);
  return { page, errors };
}

check('a hold begins at a translate of px, %, or calc() of both', async () => {
  const { page, errors } = await open();
  try {
    // #box: a 200×100 border box (180×80 content and 10px padding).
    const cases = [
      ['translate: 30px', [30, 0]],
      ['translate: 50%', [100, 0]],
      ['translate: calc(50% + 10px)', [110, 0]],
      ['translate: calc(10px + -50%) calc(25% - 5px)', [-90, 20]],
      ['translate: calc(-50% - 60px)', [-160, 0]],
      ['translate: 1.5e1px 0.5e1%', [15, 5]],
    ];
    for (const [style, want] of cases) expect([style, await page.evaluate(([id, s]) => caught(id, s), [1, style])]).toEqual([style, want]);
    expect(errors).toEqual([]);
  } finally { await page.context().close(); }
});

check('a percentage is of the untransformed box, under its own and its ancestors\' transforms', async () => {
  const { page, errors } = await open();
  try {
    for (const style of ['translate: 50%', 'translate: 50%; scale: 2', 'translate: 50%; rotate: 90deg'])
      expect([style, await page.evaluate(s => caught(1, s), style)]).toEqual([style, [100, 0]]);
    // A box's `transform-box` (CSS Transforms 1 §5): content-box and fill-box are its content box; border-box,
    // stroke-box and view-box its border box. #box sizes its content (180×80, 10px padding); #sized its border box
    // (200×100, 10px and 20px padding, 5px border: a 150×70 content box).
    for (const [box, wants] of [['content-box', [[90, 40], [75, 35]]], ['fill-box', [[90, 40], [75, 35]]], ['border-box', [[100, 50], [100, 50]]],
      ['stroke-box', [[100, 50], [100, 50]]], ['view-box', [[100, 50], [100, 50]]]]) {
      const style = `translate: 50% 50%; transform-box: ${box}`;
      expect([style, await page.evaluate(s => [caught(1, s), caught(5, s)], style)]).toEqual([style, wants]);
    }
    // #inner is 200×100 inside an ancestor drawn at half size.
    expect(await page.evaluate(() => caught(2, 'translate: 50% 50%'))).toEqual([100, 50]);
    // An SVG element's box is its `transform-box`: the view box by default, else its bounding box (CSS Transforms 1 §4).
    expect(await page.evaluate(() => caught(4, 'translate: 50% 50%'))).toEqual([200, 100]);
    expect(await page.evaluate(() => caught(4, 'translate: 50% 50%; transform-box: fill-box'))).toEqual([20, 10]);
    expect(await page.evaluate(() => caught(4, 'translate: 50% 50%; transform-box: stroke-box'))).toEqual([22, 12]);
    expect(errors).toEqual([]);
  } finally { await page.context().close(); }
});

check('the glue\'s own spring from a px offset to a % rest is caught where it shows', async () => {
  const { page, errors } = await open();
  try {
    const shown = await page.evaluate(() => {
      // Keyframes as the engine lowers them: [x, y, x%, y%].
      motion.animate({ id: 1, property: 'translate', delay: 0, duration: 1000,
        values: [[-120, 0, 0, 0], [-60, 0, -50, 0], [0, 0, -100, 0]] });
      const [animation] = document.getElementById('box').getAnimations();
      animation.pause(); animation.currentTime = 500;
      return getComputedStyle(document.getElementById('box')).translate;
    });
    expect(shown).toMatch(/^calc\(/);
    expect(await page.evaluate(() => caught(1, ''))).toEqual([-160, 0]);
    expect(errors).toEqual([]);
  } finally { await page.context().close(); }
});

check('a swipe claims a row whose translate is calc() of px and %', async () => {
  const { page, errors } = await open();
  try {
    // A 200px row shown at -50% - 60px (x 140 to 340): a swipe right moves it back toward rest.
    await page.evaluate(() => { document.getElementById('row').style.translate = 'calc(-50% - 60px)'; calls.length = 0; });
    await page.mouse.move(200, 422);
    await page.mouse.down();
    await page.mouse.move(210, 422);
    await page.mouse.move(250, 422);
    const begin = await page.evaluate(() => calls.find(c => c.op === 'begin'));
    expect(begin && [begin.x, begin.y]).toEqual([-160, 0]);
    expect(await page.evaluate(() => calls.map(c => c.op))).not.toContain('cancel');
    await page.mouse.up();
    expect(errors).toEqual([]);
  } finally { await page.context().close(); }
});

// Presence's own transform (presence-glue.js `own`) reads the same computed translate.
check('presence measures a translate of calc() in its reference box', async () => {
  const { page, errors } = await open();
  try {
    const moved = await page.evaluate(async () => {
      const { own } = await import('./presence-glue.js');
      const el = document.getElementById('sized'), at = [];
      for (const style of ['translate: calc(50% + 10px) calc(-25% - 5px)', 'translate: 50% 50%; transform-box: content-box']) {
        el.style.cssText += style;
        const m = own(el, getComputedStyle(el));
        at.push([m.e, m.f].map(v => Math.round(v * 1000) / 1000));
        el.style.removeProperty('translate'); el.style.removeProperty('transform-box');
      }
      return at;
    });
    expect(moved).toEqual([[110, -30], [75, 35]]);
    expect(errors).toEqual([]);
  } finally { await page.context().close(); }
});
