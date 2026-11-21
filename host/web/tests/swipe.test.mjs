import { beforeAll, afterAll, expect } from 'bun:test';
import { chromium, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';

const { unavailable } = chromium();
const check = need('chrome', unavailable);
let server, browser;
const modules = {
  '/motion-glue.js': new URL('../motion-glue.js', import.meta.url),
  '/input-glue.js': new URL('../input-glue.js', import.meta.url),
};
const fixture = `<!doctype html>
<main id="root">
  <div id="row" style="position:absolute;left:100px;top:100px;width:300px;height:44px;touch-action:pan-y"></div>
  <div id="pan" style="position:absolute;left:100px;top:200px;width:300px;height:100px;touch-action:none"></div>
  <div id="clip" style="display:none;position:absolute;left:100px;top:100px;width:300px;height:100px;overflow:hidden">
    <div id="target" style="width:100%;height:100%;box-sizing:border-box">
      <div id="handle" style="position:relative;width:100%;height:100%;touch-action:none"></div>
    </div>
  </div>
</main>
<script type="module">
  import { motionController } from './motion-glue.js';
  import { createInputHandlers } from './input-glue.js';
  const row = document.getElementById('row'), pan = document.getElementById('pan');
  const handle = document.getElementById('handle'), target = document.getElementById('target'), clip = document.getElementById('clip');
  const views = new Map([[1, row], [2, pan], [3, handle], [4, target], [5, clip]]);
  window.calls = []; window.deltas = []; window.ends = [];
  let serial = 0;
  const held = new Set();
  const token = () => { const t = String(++serial); held.add(t); return t; };
  window.motion = motionController({ views, now: () => performance.now(), generation: () => 1,
    inert: () => false, applyBatch() {}, request(r) {
      if (r.op === 'gesture') return { slop: 4, knee: 64, resistance: 0.2 };
      calls.push(r);
      if (r.op === 'begin') return { token: token(), value: [r.x, r.y] };
      if (r.op === 'height-begin') return { token: token(), target: 4, value: [r.x, 0] };
      if (r.op === 'transform-geometry') return { accepted: true };
      if (r.op === 'transform-begin') return { accepted: true, runtime: r.runtime, geometrySequence: r.geometrySequence,
        translateToken: token(), scaleToken: token(), value: r.values.slice(0, 3) };
      if (r.op === 'transform-move' || r.op === 'transform-action') return { accepted: true, committed: true };
      if (!held.has(r.token)) return { accepted: false };
      if (['cancel', 'release', 'release-measured'].includes(r.op)) held.delete(r.token);
      return { accepted: true };
    }
  });
  row.exactHandlers = ['swiperight', 'swipeleft'];
  motion.attachSwipe(row, 1, (type, fn) => row.addEventListener(type, fn));
  const input = createInputHandlers({ root: document.getElementById('root'), views,
    retiredViews: new Set(), ready: () => true, inertAncestor: () => false,
    dispatch: (id, delta) => deltas.push(delta) });
  pan.addEventListener('pointerdown', input.pan(pan, 2, (type, fn) => pan.addEventListener(type, fn)));
  const ancestor = new URL(location.href).searchParams.get('ancestor');
  if (ancestor) {
    clip.style.display = 'block'; pan.style.display = 'none';
    row.style.cssText = 'position:absolute;left:0;bottom:0;width:300px;height:44px;touch-action:pan-y';
    handle.append(row);
    const on = (type, fn) => handle.addEventListener(type, fn);
    if (ancestor === 'pan') on('pointerdown', input.pan(handle, 3, on));
    if (ancestor === 'height') {
      motion.heightBinding({ id: 3, target: 4, handleKey: '3', targetKey: '4' });
      motion.attachHeightDrag(handle, 3, on);
    }
    if (ancestor === 'transform') {
      motion.transformBinding({ id: 3, target: 4, clip: 5, runtime: '1', handleKey: '3', targetKey: '4', clipKey: '5' });
      motion.attachTransformDrag(handle, 3, on);
    }
  }
  document.addEventListener('pointerup', e => ends.push(e.target.id || e.target.tagName), true);
</script>`;

beforeAll(async () => {
  if (unavailable) return;
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    return path === '/' ? new Response(fixture, { headers: { 'content-type': 'text/html' } })
      : new Response(Bun.file(modules[path] ?? new URL('..' + path, import.meta.url)));
  } });
  browser = await suiteBrowser();
});
afterAll(async () => {
  await browser?.close();
  server?.stop(true);
});

async function open(ancestor = '') {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.port}/?ancestor=${ancestor}`);
  return page;
}
const contacts = page => page.evaluate(() => [...exact.contacts]);
const operations = page => page.evaluate(() => calls.map(r => r.op));
const frame = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));

for (const ancestor of ['height', 'transform']) for (const end of ['release', 'capture loss', 'cancellation']) {
  check(`a deferred ${ancestor} drag keeps the touch until its own ${end}`, async () => {
    const page = await open(ancestor), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    try {
      await page.evaluate(() => {
        window.captureLosses = [];
        document.getElementById('handle').addEventListener('lostpointercapture', e => captureLosses.push(e.target.id));
      });
      const cdp = await page.context().newCDPSession(page);
      const touch = (type, y) => cdp.send('Input.dispatchTouchEvent', {
        type, touchPoints: y === undefined ? [] : [{ x: 250, y }],
      });
      await touch('touchStart', 180);
      const pending = await contacts(page);
      expect(pending.map(([, state]) => state)).toEqual(['pending']);
      const pointer = pending[0][0];
      for (const y of [160, 140, 120, 100, 80]) await touch('touchMove', y);
      expect(await page.evaluate(() => captureLosses)).toEqual(['row']);
      expect(await operations(page)).not.toContain('cancel');
      const shown = await page.evaluate(ancestor => {
        const style = getComputedStyle(document.getElementById('target'));
        return Number.parseFloat(ancestor === 'height' ? style.height : style.translate.split(' ')[1]);
      }, ancestor);
      if (ancestor === 'height') expect(shown).toBeGreaterThan(100);
      else expect(shown).toBeLessThan(0);
      if (end === 'capture loss') {
        await page.evaluate(pointer => document.getElementById('handle').releasePointerCapture(pointer), pointer);
        await touch('touchMove', 60);
      } else if (end === 'cancellation') await touch('touchCancel');
      if (end !== 'cancellation') await touch('touchEnd');
      const ops = await operations(page);
      expect(ops.filter(op => op.endsWith('begin'))).toEqual([ancestor + '-begin']);
      expect(ops.filter(op => op === (ancestor === 'height' ? 'move' : 'transform-move')).length).toBeGreaterThan(1);
      expect(ops.filter(op => op.endsWith('action'))).toEqual(end === 'release' ? [ancestor + '-action'] : []);
      expect(ops.filter(op => op === 'cancel')).toHaveLength(end === 'release' ? 0 : ancestor === 'height' ? 1 : 2);
      expect(await page.evaluate(pointer => document.getElementById('handle').hasPointerCapture(pointer), pointer)).toBe(false);
      expect(await contacts(page)).toEqual([]);
      expect(errors).toEqual([]);
    } finally { await page.context().close(); }
  });
}

for (const ancestor of ['pan', 'height', 'transform']) {
  for (const end of ['outside release', 'outside cancellation', 'buttonless move']) {
    check(`a ${ancestor} waiting for a swipe ends on ${end}`, async () => {
      const page = await open(ancestor), errors = [];
      page.on('pageerror', error => errors.push(error.message));
      try {
        await page.mouse.move(250, 197);
        await page.mouse.down();
        await page.mouse.move(250, 227, { steps: 15 });
        expect(await contacts(page)).toEqual([[1, 'pending']]);
        if (end === 'outside release') await page.mouse.up();
        else await page.evaluate(end => {
          const target = end === 'buttonless move' ? document.getElementById('row') : document.body;
          target.dispatchEvent(new PointerEvent(end === 'buttonless move' ? 'pointermove' : 'pointercancel', {
            bubbles: true, pointerId: 1, pointerType: 'mouse', buttons: 0, clientX: 250, clientY: 170,
          }));
        }, end);
        expect(await contacts(page)).toEqual([]);
        expect(await page.evaluate(() => document.getElementById('handle').hasPointerCapture(1))).toBe(false);
        if (end !== 'outside release') await page.mouse.up();
        await page.mouse.move(250, 170);
        await page.mouse.move(250, 120);
        await frame(page);
        expect((await operations(page)).filter(op => op.endsWith('begin'))).toEqual([]);
        expect(await page.evaluate(() => deltas)).toEqual([]);
        expect(await page.evaluate(() => document.getElementById('handle').hasPointerCapture(1))).toBe(false);
        // With the button held, a vertical refusal by the child still lets the ancestor begin.
        await page.mouse.move(250, 180);
        await page.mouse.down();
        await page.mouse.move(250, 170);
        await page.mouse.move(250, 160);
        await frame(page);
        if (ancestor === 'pan') expect(await page.evaluate(() => deltas.length)).toBeGreaterThan(0);
        else expect(await operations(page)).toContain(ancestor + '-begin');
        await page.mouse.up();
        expect(errors).toEqual([]);
      } finally { await page.context().close(); }
    });
  }
  check(`a claimed swipe keeps precedence over its ${ancestor} ancestor through release`, async () => {
    const page = await open(ancestor);
    try {
      await page.mouse.move(150, 180);
      await page.mouse.down();
      await page.mouse.move(160, 180);
      await page.mouse.move(250, 180);
      expect(await contacts(page)).toEqual([[1, 'claimed']]);
      await page.mouse.up();
      expect(await contacts(page)).toEqual([]);
      expect((await operations(page)).filter(op => op.endsWith('begin'))).toEqual(['begin']);
      expect((await operations(page)).filter(op => op === 'action')).toHaveLength(1);
      expect(await page.evaluate(() => deltas)).toEqual([]);
    } finally { await page.context().close(); }
  });
}

check('an outside lift frees a deferred pan before the next press', async () => {
  const page = await open('pan');
  try {
    await page.mouse.move(250, 197);
    await page.mouse.down();
    await page.mouse.move(250, 227, { steps: 15 });
    await page.mouse.up();
    // Send the next down without a hover move that could clear the stale contact.
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: 250, y: 120, button: 'left', buttons: 1, clickCount: 1 });
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: 270, y: 120, button: 'left', buttons: 1 });
    await frame(page);
    expect(await page.evaluate(() => deltas)).toEqual(['20,0']);
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: 270, y: 120, button: 'left', buttons: 0, clickCount: 1 });
  } finally { await page.context().close(); }
});

for (const [name, start, end, steps] of [['edge', 141, 171, 15], ['jump', 122, 146, 1]]) {
  check(`a pending swipe lifted outside after the ${name} exit leaves hover and pan free`, async () => {
    const page = await open();
    try {
      await page.mouse.move(250, start);
      await page.mouse.down();
      await page.mouse.move(250, end, { steps });
      expect(await contacts(page)).toEqual([[1, 'pending']]);
      await page.mouse.up();
      expect(await page.evaluate(() => ends)).toEqual(['HTML']);
      expect(await contacts(page)).toEqual([]);

      await page.mouse.move(350, 122, { steps: 10 });
      await page.mouse.click(350, 122, { button: 'right' });
      expect(await operations(page)).toEqual([]);
      await page.mouse.move(150, 250);
      await page.mouse.down();
      for (const x of [170, 190, 210, 230]) {
        await page.mouse.move(x, 250);
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(resolve)));
      }
      expect(await page.evaluate(() => deltas)).toEqual(['20,0', '20,0', '20,0', '20,0']);
      await page.mouse.up();
    } finally { await page.context().close(); }
  });
}

check('a pending swipe hears its cancellation outside in capture phase', async () => {
  const page = await open();
  try {
    await page.mouse.move(250, 141);
    await page.mouse.down();
    await page.mouse.move(250, 171, { steps: 15 });
    await page.evaluate(() => {
      document.body.addEventListener('pointercancel', e => e.stopPropagation());
      document.body.dispatchEvent(new PointerEvent('pointercancel', { bubbles: true, pointerId: 2 }));
    });
    expect(await contacts(page)).toEqual([[1, 'pending']]);
    await page.evaluate(() => document.body.dispatchEvent(new PointerEvent('pointercancel', { bubbles: true, pointerId: 1 })));
    expect(await contacts(page)).toEqual([]);
    await page.mouse.up();
    await page.mouse.move(350, 122);
    expect(await operations(page)).toEqual([]);
  } finally { await page.context().close(); }
});

for (const active of [false, true]) {
  check(`a mouse move with no button cancels a ${active ? 'claimed' : 'pending'} swipe`, async () => {
    const page = await open();
    try {
      await page.mouse.move(150, 122);
      await page.mouse.down();
      if (active) {
        await page.mouse.move(160, 122);
        await page.mouse.move(250, 122);
      }
      expect(await contacts(page)).toEqual([[1, active ? 'claimed' : 'pending']]);
      // A lift outside the browser supplies no terminal event; its next move reports no button.
      await page.evaluate(() => document.getElementById('row').dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true, pointerId: 1, pointerType: 'mouse', buttons: 0, clientX: 300, clientY: 122,
      })));
      expect(await contacts(page)).toEqual([]);
      await page.mouse.up();
      await page.mouse.move(350, 122);
      const ops = await operations(page);
      expect(ops.filter(op => op === 'begin')).toHaveLength(active ? 1 : 0);
      expect(ops.filter(op => op === 'cancel')).toHaveLength(active ? 1 : 0);
      expect(ops).not.toContain('action');
    } finally { await page.context().close(); }
  });
}

check('a claimed mouse swipe still commits once on its button-up sample', async () => {
  const page = await open();
  try {
    await page.mouse.move(150, 122);
    await page.mouse.down();
    await page.mouse.move(160, 122);
    await page.mouse.move(250, 122);
    expect(await contacts(page)).toEqual([[1, 'claimed']]);
    await page.mouse.up();
    expect(await contacts(page)).toEqual([]);
    const ops = await operations(page);
    expect(ops.filter(op => op === 'action')).toHaveLength(1);
    expect(ops.filter(op => op === 'release-measured')).toHaveLength(1);
    expect(ops).not.toContain('cancel');
  } finally { await page.context().close(); }
});

// LLP 1094.000 D8: a swipe beside the browser's own drag. The browser starts a draggable node's drag after a few
// pixels, before the swipe decides: a sideways contact in a direction the swipe hears cancels the drag, and the
// pointer stream goes on to the swipe; an upright one lets the drag go.
check('a draggable swipe row keeps a sideways mouse drag and lets an upright one drag out', async () => {
  const page = await open();
  try {
    await page.evaluate(() => {
      document.getElementById('row').draggable = true;
      window.drags = [];
      window.addEventListener('dragstart', e => drags.push(e.defaultPrevented));
    });
    await page.mouse.move(150, 122);
    await page.mouse.down();
    await page.mouse.move(220, 124, { steps: 15 });
    await page.mouse.up();
    expect(await page.evaluate(() => drags)).toEqual([true]);
    expect(await operations(page)).toContain('begin');
    await page.evaluate(() => { calls.length = 0; drags.length = 0; });
    await page.mouse.move(150, 122);
    await page.mouse.down();
    await page.mouse.move(152, 170, { steps: 15 });
    await page.mouse.up();
    expect(await page.evaluate(() => drags)).toEqual([false]);
    expect(await operations(page)).not.toContain('begin');
  } finally { await page.context().close(); }
});
