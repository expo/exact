import { afterAll, beforeAll, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests();

const WEB = resolve(import.meta.dir, '..');
const style = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') return new Response(null, { status: 404 });
    if (path === '/') return new Response(`<style>${style}</style><div id="exact-root"></div>
      <script type="module">
        import { createInputHandlers } from '/input-glue.js';
        createInputHandlers({ root: document.getElementById('exact-root'), views: new Map(), retiredViews: new Set(), agentMode: new URL(location.href).searchParams.has('agent'), ready: () => true, inertAncestor: el => el.closest('[inert]') !== null, dispatch() {} });
        window.ready = true;
      </script>`, { headers: { 'content-type': 'text/html' } });
    return new Response(Bun.file(resolve(WEB, path.slice(1))), { headers: { 'content-type': 'text/javascript' } });
  } });
});
afterAll(() => server?.stop(true));

for (const engine of ['chromium', 'webkit']) engineTest[engine](`${engine}: native active properties add no implicit animation`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    const seen = await page.evaluate(async () => {
      const root = document.getElementById('exact-root'), results = [], pixel = document.createElement('canvas').getContext('2d', { willReadFrequently: true });
      for (const held of [false, true]) {
        root.innerHTML = '<button data-native data-button-style="filled" data-exact-on="press" style="width:120px;height:44px;--exact-press:0.9;--exact-press-opacity-to:0.4;--exact-press-color:blue;--exact-press-background-color:red;scale:calc(1.25 * var(--exact-press-factor,1))"><span>Native</span></button>';
        const el = root.firstElementChild, box = el.getBoundingClientRect();
        const pointer = type => el.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerType: 'mouse', pointerId: 1, isPrimary: true, button: 0, buttons: type === 'pointerdown' ? 1 : 0, clientX: box.x + box.width / 2, clientY: box.y + box.height / 2 }));
        if (held) exact.hold(el, 'active'); else pointer('pointerdown');
        await new Promise(requestAnimationFrame);
        const tracks = el.getAnimations();
        const durations = tracks.filter(a => a.effect.getKeyframes().some(f => Object.keys(f).some(k => k.endsWith('-progress')))).map(a => a.effect.getTiming().duration);
        tracks.find(a => a.effect.getKeyframes().some(f => '--exact-button-press' in f)).finish();
        const style = getComputedStyle(el);
        pixel.fillStyle = style.color; pixel.fillRect(0, 0, 1, 1);
        const color = Array.from(pixel.getImageData(0, 0, 1, 1).data).slice(0, 3);
        results.push([durations, Number(style.scale), Number(style.opacity), color, Number(getComputedStyle(el.firstElementChild).opacity)]);
        if (held) exact.hold(el, null); else pointer('pointerup');
        await new Promise(requestAnimationFrame);
        results.push([Number(getComputedStyle(el).scale), Number(getComputedStyle(el).opacity)]);
      }
      return results;
    });
    expect(seen).toEqual([[[0, 0, 0, 0], 1.125, 0.4, [0, 0, 255], 0.75], [1.25, 1], [[0, 0, 0, 0], 1.125, 0.4, [0, 0, 255], 0.75], [1.25, 1]]);
  } finally { await browser.close(); }
}, 60_000);

for (const engine of ['chromium', 'webkit']) engineTest[engine](`${engine}: stationary physical and held presses follow computed scale targets`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    const seen = await page.evaluate(async () => {
      const root = document.getElementById('exact-root'), results = [];
      for (const held of [false, true]) for (const initial of [null, '0.97', 'var(--scale-token)']) {
        root.innerHTML = '<div data-exact-on="press" style="width:100px;height:100px;scale:calc(1.25 * var(--exact-press-factor, 1))">Scale</div>';
        const el = root.firstElementChild, token = document.createElement('style');
        token.textContent = ':root { --scale-token: 0.97 }'; document.head.append(token);
        if (initial != null) el.style.setProperty('--exact-press', initial);
        const box = el.getBoundingClientRect();
        const pointer = type => el.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerType: 'mouse', pointerId: 1, isPrimary: true, button: 0, buttons: type === 'pointerdown' ? 1 : 0, clientX: box.x + box.width / 2, clientY: box.y + box.height / 2 }));
        const settle = async () => {
          await new Promise(requestAnimationFrame);
          el.getAnimations().forEach(a => a.finish());
          await new Promise(requestAnimationFrame);
        };
        const scale = () => Math.round(Number(getComputedStyle(el).scale) / 1.25 * 1000) / 1000;
        if (held) exact.hold(el, 'active'); else pointer('pointerdown');
        await settle();
        const before = scale();
        if (initial?.startsWith('var')) {
          token.sheet.cssRules[0].style.setProperty('--scale-token', '0.92');
          exact.repaintPress(); // CSSOM edits use the runtime's refresh hook
        } else el.style.setProperty('--exact-press', initial == null ? '0.8' : '0.92');
        await settle();
        const changed = scale();
        el.style.removeProperty('--exact-press');
        await settle();
        const cleared = scale();
        el.style.setProperty('--exact-press', 'var(--scale-token)');
        await settle();
        token.textContent = ':root { --scale-token: 0.88 }';
        await settle();
        const again = scale();
        if (held) exact.hold(el, null); else pointer('pointerup');
        await settle();
        results.push([held, initial, before, changed, cleared, again, scale()]);
        token.remove();
      }
      return results;
    });
    expect(seen).toEqual([false, true].flatMap(held => [null, '0.97', 'var(--scale-token)'].map(initial =>
      [held, initial, initial == null ? 1 : 0.97, initial == null ? 0.8 : 0.92, 1, 0.88, 1])));
  } finally {
    await browser.close();
  }
}, 60_000);

for (const engine of ['chromium', 'webkit']) engineTest[engine](`${engine}: a composed motion sample clears static z and keeps the press offset`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    const seen = await page.evaluate(async () => {
      const { motionController } = await import('/motion-glue.js');
      const root = document.getElementById('exact-root');
      root.innerHTML = '<div data-exact-on="press" style="width:100px;height:80px;--exact-press-translate:4px 4px;--exact-translate-composes:1;--exact-translate-x:0px;--exact-translate-y:0px;--exact-translate-z:30px;--exact-press-x:4px;--exact-press-y:4px">Slab</div>';
      const el = root.firstElementChild;
      const motion = motionController({ views: new Map([[1, el]]), now: () => 0, generation: () => 1, request() {}, applyBatch() {}, inert: () => false });
      const base = el.getBoundingClientRect();
      motion.animate({ id: 1, property: 'translate', values: [[0, 0, 10, 0], [100, 60, 10, 0]], delay: 0, duration: 1000 });
      const animation = el.getAnimations()[0]; animation.pause(); animation.currentTime = 500;
      const read = () => { const b = el.getBoundingClientRect(); return [b.x - base.x, b.y - base.y, parseFloat(getComputedStyle(el).getPropertyValue('--exact-translate-z'))]; };
      const before = read();
      exact.hold(el, 'active');
      el.getAnimations().filter(a => a !== animation).forEach(a => a.finish());
      const during = read();
      await new Promise(requestAnimationFrame);
      exact.hold(el, null);
      el.getAnimations().filter(a => a !== animation).forEach(a => a.finish());
      const after = read();
      el.style.removeProperty("--exact-press-translate");
      const cleared = read();
      motion.reset();
      return [before, during, after, cleared];
    });
    expect(seen).toEqual([[60, 30, 0], [64, 34, 0], [60, 30, 0], [60, 30, 0]]);
  } finally { await browser.close(); }
}, 30_000);

engineTest.chromium('a translated touch lift outside the painted box cannot click', async () => {
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage({ hasTouch: true });
    await page.goto(new URL('?agent', server.url).href);
    await page.waitForFunction(() => window.ready);
    await page.evaluate(() => {
      const root = document.getElementById('exact-root');
      root.innerHTML = '<button type="button" data-exact-on="press" style="position:absolute;left:20px;top:20px;width:60px;height:30px;--exact-press-translate:100px;--exact-press-x:100px;--exact-translate-composes:1">Move</button>';
      window.clicks = 0;
      root.addEventListener('click', () => clicks++);
    });
    const cdp = await page.context().newCDPSession(page);
    const { x, y, width, height } = await page.locator('button').boundingBox();
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: x + width / 2, y: y + height / 2 }] });
    expect((await page.locator('button').boundingBox()).x).toBe(x + 100);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    expect(await page.evaluate(() => clicks)).toBe(0);
  } finally { await browser.close(); }
}, 30_000);

engineTest.chromium('inertness and a native style change clear the appropriate live press tracks', async () => {
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    const states = await page.evaluate(async () => {
      const root = document.getElementById('exact-root');
      root.innerHTML = '<button type="button" data-native data-button-style="plain">Text</button>';
      const b = root.firstElementChild;
      const state = () => [b.hasAttribute('data-pressed'), b.hasAttribute('data-exact-button-press'), b.getAnimations().length];
      b.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, isPrimary: true, button: 0, buttons: 1 }));
      const before = state();
      b.removeAttribute('data-button-style');
      await Promise.resolve();
      const restyled = state();
      root.inert = true;
      await Promise.resolve();
      return [before, restyled, state()];
    });
    expect(states).toEqual([[true, true, 1], [true, false, 0], [false, false, 0]]);
  } finally { await browser.close(); }
}, 30_000);

for (const engine of ['chromium', 'firefox', 'webkit']) engineTest[engine](`${engine}: a native button tapped within the scroll delay shows its highlight for a frame`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ hasTouch: true });
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    const quick = await page.evaluate(async () => {
      const root = document.getElementById('exact-root');
      root.innerHTML = '<div style="height:36px;overflow:auto"><button type="button" data-native data-button-style="plain"><span>Text</span></button><div style="height:100px"></div></div>';
      const b = root.querySelector('button'), face = b.firstElementChild;
      const { x, y } = b.getBoundingClientRect();
      const pointer = (type, buttons) => b.dispatchEvent(new PointerEvent(type, {
        bubbles: true, pointerType: 'touch', pointerId: 1, isPrimary: true, button: 0, buttons, clientX: x + 5, clientY: y + 5,
      }));
      const read = () => [b.hasAttribute('data-exact-button-press'), Number(getComputedStyle(face).opacity)];
      const frame = () => new Promise(requestAnimationFrame);
      // Lift in the same turn so both edges occur inside the scroll delay.
      pointer('pointerdown', 1);
      const down = read();
      pointer('pointerup', 0);
      await frame();
      const first = read();
      await frame();
      const [release] = b.getAnimations();
      return [down, first, release?.effect.getTiming().duration ?? null];
    });
    expect(quick).toEqual([[false, 1], [true, .75], 120]);
  } finally { await browser.close(); }
}, 30_000);

for (const engine of ['chromium', 'webkit']) engineTest[engine](`${engine}: ordinary touch release refreshes the control's box`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    expect(await page.evaluate(() => {
      const root = document.getElementById('exact-root');
      root.innerHTML = '<button data-exact-on="press" style="position:absolute;left:40px;top:40px;width:100px;height:80px">Press</button>';
      const el = root.firstElementChild;
      const pointer = type => el.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerType: 'touch', pointerId: 1, isPrimary: true, button: 0, clientX: 90, clientY: 80 }));
      pointer('pointerdown');
      el.style.left = '240px';
      pointer('pointerup');
      return exact.strayClick({ isTrusted: true, detail: 1 });
    })).toBe(true);
  } finally { await browser.close(); }
}, 30_000);

for (const engine of ['chromium', 'webkit']) engineTest[engine](`${engine}: quick taps show the pressed endpoints for a frame`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    for (const agent of [false, true]) {
      await page.goto(`${server.url.href}${agent ? '?agent' : ''}`);
      await page.waitForFunction(() => window.ready);
      const seen = await page.evaluate(async () => {
        const root = document.getElementById('exact-root');
        root.innerHTML = `<div data-exact-on="press" style="width:100px;height:80px;--exact-press:0.5;
          --exact-press-translate:40px 20px;--exact-press-x:40px;--exact-press-y:20px;--exact-translate-composes:1;
          --exact-press-opacity:0.5;scale:calc(var(--exact-press-factor,1))">Slab</div>`;
        const el = root.firstElementChild, frame = () => new Promise(requestAnimationFrame);
        const pointer = (type, buttons) => el.dispatchEvent(new PointerEvent(type, {
          bubbles: true, pointerType: 'mouse', pointerId: 1, isPrimary: true, button: 0, buttons, clientX: 40, clientY: 30,
        }));
        const read = () => {
          const s = getComputedStyle(el);
          return [el.hasAttribute('data-pressed'), Number(s.scale), parseFloat(s.translate), Number(s.opacity)];
        };
        pointer('pointerdown', 1);
        const down = read();
        pointer('pointerup', 0);
        await frame();
        const first = read();
        await frame();
        const tracks = el.getAnimations();
        const release = tracks.map(a => a.effect.getTiming().duration);
        tracks.forEach(a => a.finish());
        const rest = read();
        pointer('pointerdown', 1);
        pointer('pointerup', 0);
        pointer('pointerdown', 1);
        await frame(); await frame();
        const repressed = el.hasAttribute('data-pressed');
        el.setAttribute('disabled', '');
        await Promise.resolve();
        return { down, first, release, rest, repressed, disabled: read() };
      });
      expect(seen).toEqual(agent ? {
        down: [true, .5, 40, .5], first: [false, 1, 0, 1], release: [], rest: [false, 1, 0, 1], repressed: true, disabled: [false, 1, 0, 1],
      } : {
        down: [true, 1, 0, 1], first: [true, .5, 40, .5], release: [120, 120, 120], rest: [false, 1, 0, 1], repressed: true, disabled: [false, 1, 0, 1],
      });
    }
  } finally { await browser.close(); }
}, 30_000);

for (const engine of ['chromium', 'firefox', 'webkit']) engineTest[engine](`${engine}: press tracks use the lowered timing for each property and shorten repeated reversals`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => window.ready);
    const seen = await page.evaluate(async () => {
      const root = document.getElementById('exact-root');
      root.innerHTML = `<div data-exact-on="press" style="
        --exact-press:0.5;--exact-press-translate:40px 20px;--exact-press-x:40px;--exact-press-y:20px;
        --exact-translate-composes:1;scale:calc(var(--exact-press-factor,1));
        --exact-press-background-color:blue;--exact-press-color:red;--exact-press-opacity:0.5;
        --exact-press-timing:{&quot;scale&quot;:[400.0,0.0,&quot;linear&quot;],&quot;translate&quot;:[800.0,-80.0,&quot;linear&quot;],&quot;color&quot;:[100.0,40.0,&quot;steps(2,jump-end)&quot;],&quot;opacity&quot;:[200.0,0.0,&quot;linear&quot;]};
        background-color:white;color:black;
        transition:scale 0s, --exact-scale 400ms linear, translate 0s, --exact-translate-x 800ms linear -80ms, color 100ms steps(2) 40ms, opacity 200ms linear">
        Press
      </div>`;
      const el = root.firstElementChild;
      const track = name => el.getAnimations().find(a => a.effect.getKeyframes().some(k => name in k));
      const seek = (a, time) => { a.pause(); a.currentTime = time; return a.effect.getTiming(); };
      const sample = (time, name) => {
        const a = track(name);
        if (!a) return null;
        const { duration, delay } = seek(a, time);
        return [duration, delay];
      };
      const read = () => {
        const cs = getComputedStyle(el), paint = exact.pressShown(el) ?? cs;
        return [Number(cs.scale), parseFloat(cs.translate), Number(paint.opacity), paint.color, paint.backgroundColor];
      };
      exact.hold(el, 'active');
      await new Promise(requestAnimationFrame);
      const timings = [
        sample(100, '--exact-press-scale-progress'), sample(120, '--exact-pressed'),
        sample(50, '--exact-press-opacity-progress'), sample(100, '--exact-press-color-progress'),
      ];
      const held = read();
      exact.hold(el, null);
      const reversing = sample(25, '--exact-press-scale-progress');
      const offsetBack = sample(0, '--exact-pressed');
      exact.hold(el, 'active');
      const repeated = sample(0, '--exact-press-scale-progress');
      const transitions = el.getAnimations().filter(a => a.transitionProperty).length;
      await new Promise(requestAnimationFrame);
      el.style.setProperty('--exact-press-timing', '{}');
      exact.hold(el, null);
      const rest = read();
      exact.hold(el, 'active');
      return { timings, held, reversing, offsetBack, repeated, transitions, rest, immediate: read() };
    });
    expect(seen.timings).toEqual([[400, 0], [800, -80], [200, 0], [100, 40]]);
    expect(seen.held).toEqual([0.875, 10, 0.875, 'rgb(128, 0, 0)', 'rgb(0, 0, 255)']);
    expect(seen.reversing).toEqual([100, 0]);
    expect(seen.offsetBack).toEqual([200, -20]);
    expect(seen.repeated).toEqual([325, 0]);
    expect(seen.transitions).toBe(0);
    expect(seen.rest).toEqual([1, 0, 1, 'rgb(0, 0, 0)', 'rgb(255, 255, 255)']);
    expect(seen.immediate).toEqual([0.5, 40, 0.5, 'rgb(255, 0, 0)', 'rgb(0, 0, 255)']);
  } finally {
    await browser.close();
  }
}, 30_000);
