import { afterAll, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests(['chromium']);
const server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(req) {
  const path = new URL(req.url).pathname;
  return new Response(path.endsWith('.js') ? readFileSync(new URL('..' + path, import.meta.url)) : `<!doctype html>
    <style>body { margin: 0 } #port { width:300px; height:300px; overflow:auto } .row { height:100px } #grip { height:100px }</style>
    <div id="root"><div id="port" data-view="1" data-reordergroup="g">
    <div class="row"></div><div class="row" data-view="2"><div id="grip" data-view="3">Hold to move</div></div><div style="height:1500px"></div></div></div>`,
    { headers: { 'content-type': path.endsWith('.js') ? 'text/javascript' : 'text/html' } });
} });
afterAll(() => server.stop(true));

async function fixture(page, grouped) {
  await page.goto(`http://127.0.0.1:${server.port}`);
  await page.evaluate(async grouped => {
    const { arrangeController } = await import('/motion-glue.js');
    const { groupController } = await import('/group-glue.js');
    const root = document.getElementById('root'), port = document.getElementById('port'), grip = document.getElementById('grip');
    const views = new Map([...root.querySelectorAll('[data-view]')].map(el => [+el.dataset.view, el]));
    const calls = [], samples = [], pins = new Set();
    const collections = {
      reorderMapping: () => ({ port, portTop: 0, raw: port.scrollTop, scrollTop: port.scrollTop, contentY: 150 + port.scrollTop,
        revision: 1, scrollSequence: 1, portWidth: 300, portHeight: 300, rowWidth: 300, totalExtent: 1700 }),
      reorderContact(el, id) { pins.add(id); }, retainInteraction: () => 1,
      releaseInteraction(id) { pins.delete(id); }, releaseRetainedInteraction() { pins.clear(); }, reporting: () => false,
    };
    const motion = {
      captureReorder: () => [{ key: '2', view: 2, hold: '1', value: [0, 0] }], adoptReorder() {}, raiseReorder() {},
      presentReorder(view, hold, value) { samples.push(value); }, rebaseReorder: rows => rows, releaseReorder() {}, settleReorder(view, done) { done(); },
    };
    const arrange = arrangeController({ views, collections, motion, grouped: grouped ? groupController : null,
      root, now: () => performance.now(), generation: () => 1, ready: () => true, inert: () => false,
      request(req) {
        calls.push(req.op);
        return { accepted: true, token: '1', frame: [], phase: req.op === 'reorder-cancel' ? 'cancelling' : req.op === 'reorder-terminal' ? 'settling' : 'active' };
      }, applyBatch() {} });
    if (grouped) arrange.group({ id: 3, group: 'g', keys: true });
    arrange.binding({ id: 3, list: 1, wrapper: 2, runtime: '1', handleKey: '3', listKey: '1', wrapperKey: '2', rootKey: '2', rowEpoch: '1' });
    globalThis.f = { arrange, calls, samples, pins, grip, port };
  }, grouped);
}

for (const grouped of [false, true]) engineTest.chromium(`${grouped ? 'grouped' : 'in-list'} reorder yields a quick swipe and keeps a held finger`, async () => {
  const browser = await suiteBrowser(), page = await browser.newPage({ viewport: { width: 400, height: 600 }, hasTouch: true });
  try {
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, y) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints: type === 'touchEnd' || type === 'touchCancel' ? [] : [{ x: 100, y }] });
    await fixture(page, grouped);
    await touch('touchStart', 150);
    for (const y of [140, 120, 90, 60]) await touch('touchMove', y);
    await touch('touchEnd');
    expect(await page.evaluate(() => ({ calls: f.calls, pins: f.pins.size }))).toEqual({ calls: [], pins: 0 });
    await page.waitForFunction(() => f.port.scrollTop > 0);
    expect(await page.locator('#grip').evaluate(el => getComputedStyle(el).touchAction)).toBe('auto');
    await page.evaluate(() => f.arrange.reset());

    await fixture(page, grouped);
    await touch('touchStart', 150);
    await page.waitForFunction(() => f.calls.includes('reorder-begin'));
    await touch('touchMove', 130);
    await touch('touchMove', 100);
    await touch('touchMove', 60);
    expect(await page.evaluate(() => ({ top: f.port.scrollTop, begins: f.calls.filter(op => op === 'reorder-begin').length,
      cancelled: f.calls.includes('reorder-cancel') }))).toEqual({ top: 0, begins: 1, cancelled: false });
    await touch('touchEnd');
    await page.waitForFunction(() => f.calls.includes('reorder-terminal'));
    expect(await page.evaluate(() => f.calls.filter(op => op === 'reorder-terminal').length)).toBe(1);
    await page.evaluate(() => f.arrange.reset());
  } finally { await page.close(); }
}, 60000);

engineTest.chromium('reorder grips keep selection disabled through style replacement and release it when unbound', async () => {
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    for (const grouped of [false, true]) for (const ending of ['unbind', 'reset']) {
      await fixture(page, grouped);
      const result = await page.evaluate(async ending => {
        const { motionController } = await import('/motion-glue.js');
        const motion = motionController({ views: new Map([[3, f.grip]]), now: () => 0, generation: () => 1,
          request() {}, applyBatch() {}, inert: () => false });
        const before = getComputedStyle(f.grip).userSelect;
        motion.style(3, 'color: red');
        const after = getComputedStyle(f.grip).userSelect;
        if (ending === 'unbind') f.arrange.binding({ id: 3, list: null });
        else f.arrange.reset();
        return { before, after, released: getComputedStyle(f.grip).userSelect,
          bound: f.grip.hasAttribute('data-exact-reorder-grip'), inline: f.grip.style.userSelect };
      }, ending);
      expect(result).toEqual({ before: 'none', after: 'none', released: 'auto', bound: false, inline: '' });
      await page.evaluate(() => f.arrange.reset());
    }
  } finally { await page.close(); }
}, 60000);

engineTest.chromium('a pending reorder hold ends on release, cancel, blur and reset; mouse still lifts on movement', async () => {
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    for (const grouped of [false, true]) {
      for (const type of ['touch', 'pen']) {
        await fixture(page, grouped);
        const result = await page.evaluate(async type => {
          // Run the recognizer's timer deterministically, including timers cleared by an ending.
          const timers = new Map(), originalSet = window.setTimeout, originalClear = window.clearTimeout;
          let serial = 0;
          window.setTimeout = fn => { timers.set(++serial, fn); return serial; };
          window.clearTimeout = id => timers.delete(id);
          const results = [];
          try {
            for (const ending of ['pointerup', 'pointercancel', 'blur', 'move', 'reset']) {
              const event = (name, y = 150) => new PointerEvent(name, { pointerType: type, pointerId: 9, clientX: 100, clientY: y,
                bubbles: true, cancelable: true, isPrimary: true, button: 0 });
              f.grip.dispatchEvent(event('pointerdown'));
              if (ending === 'blur') window.dispatchEvent(new Event('blur'));
              else if (ending === 'reset') f.arrange.reset();
              else document.dispatchEvent(event(ending === 'move' ? 'pointermove' : ending, ending === 'move' ? 120 : 150));
              for (const fn of [...timers.values()]) fn();
              results.push({ type, ending, calls: [...f.calls], pins: f.pins.size });
            }
          } finally { window.setTimeout = originalSet; window.clearTimeout = originalClear; }
          return results;
        }, type);
        for (const r of result) { expect(r.calls).toEqual([]); expect(r.pins).toBe(0); }
      }
      await fixture(page, grouped);
      await page.mouse.move(100, 150); await page.mouse.down(); await page.mouse.move(100, 120);
      expect(await page.evaluate(() => f.calls.filter(op => op === 'reorder-begin').length)).toBe(1);
      await page.mouse.up(); await page.evaluate(() => f.arrange.reset());
    }
  } finally { await page.close(); }
}, 60000);

for (const grouped of [false, true]) engineTest.chromium(`${grouped ? 'grouped' : 'in-list'} reorder recognizes the platform hold and a reserved vertical axis`, async () => {
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    for (const type of ['touch', 'pen']) for (const mode of ['contextmenu', 'timer', 'none', 'pan-x', 'pan-y']) {
      await fixture(page, grouped);
      const result = await page.evaluate(({ type, mode }) => {
        const timers = new Map(), originalSet = window.setTimeout, originalClear = window.clearTimeout;
        let serial = 0;
        window.setTimeout = fn => { timers.set(++serial, fn); return serial; };
        window.clearTimeout = id => timers.delete(id);
        // Synthetic contacts exercise recognition; the real-touch case above covers capture.
        f.grip.setPointerCapture = () => {};
        f.grip.style.touchAction = ['none', 'pan-x', 'pan-y'].includes(mode) ? mode : 'auto';
        const event = (name, y) => new PointerEvent(name, { pointerType: type, pointerId: 9, clientX: 100, clientY: y,
          bubbles: true, cancelable: true, isPrimary: true, button: 0 });
        try {
          f.grip.dispatchEvent(event('pointerdown', 150));
          document.dispatchEvent(event('pointermove', mode === 'contextmenu' || mode === 'timer' ? 144 : 130));
          const menu = new PointerEvent('contextmenu', { pointerType: type, pointerId: 9, bubbles: true, cancelable: true });
          if (mode === 'contextmenu') f.grip.dispatchEvent(menu);
          if (mode === 'timer') for (const fn of [...timers.values()]) fn();
          const cs = getComputedStyle(f.grip);
          const out = { begins: f.calls.filter(op => op === 'reorder-begin').length, prevented: menu.defaultPrevented,
            timers: timers.size, select: cs.userSelect, sample: f.samples.at(-1), ghost: document.querySelector('[data-exact-ghost]')?.style.translate };
          document.dispatchEvent(event('pointerup', mode === 'contextmenu' || mode === 'timer' ? 144 : 130));
          f.arrange.reset();
          return out;
        } finally { window.setTimeout = originalSet; window.clearTimeout = originalClear; }
      }, { type, mode });
      expect(result.begins).toBe(mode === 'pan-y' ? 0 : 1);
      expect(result.prevented).toBe(mode === 'contextmenu');
      expect(result.timers).toBe(0);
      expect(result.select).toBe('none');
      if (mode === 'contextmenu' || mode === 'timer') {
        if (grouped) expect(result.ghost).toBe('0px');
        else expect(result.sample).toEqual([0, 0]);
      }
    }
  } finally { await page.close(); }
}, 60000);
