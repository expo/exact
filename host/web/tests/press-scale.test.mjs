import { afterAll, beforeAll, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { withLook } from '../look.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';

const engineTest = await engineTests(['chromium', 'webkit']);

const WEB = resolve(import.meta.dir, '..');
const style = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response(`<style>${style}</style><div id="exact-root"></div>
      <script type="module">
        import { createInputHandlers } from '/input-glue.js';
        createInputHandlers({ root: document.getElementById('exact-root'), views: new Map(), retiredViews: new Set(), agentMode: new URL(location.href).searchParams.has('agent'), ready: () => true, inertAncestor: () => false, dispatch() {} });
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
        const durations = el.getAnimations().filter(a => a.effect.getKeyframes().some(f => '--exact-press-factor' in f || '--exact-press-paint' in f)).map(a => a.effect.getTiming().duration);
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
    expect(seen).toEqual([[[0, 0], 1.125, 0.4, [0, 0, 255], 0.75], [1.25, 1], [[0, 0], 1.125, 0.4, [0, 0, 255], 0.75], [1.25, 1]]);
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

for (const engine of ['chromium', 'webkit']) engineTest[engine](`${engine}: active translation shares the press curve, quick-tap hold and immediate agent clock`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    for (const agent of [false, true]) {
      await page.goto(`${server.url.href}${agent ? '?agent' : ''}`);
      await page.waitForFunction(() => window.ready);
      const seen = await page.evaluate(async agent => {
        const results = [], root = document.getElementById('exact-root');
        for (const scale of [1, .5]) {
          root.innerHTML = `<div data-exact-on="press" style="width:100px;height:80px;--exact-press:${scale};--exact-press-translate:40px 20px;--exact-press-x:40px;--exact-press-y:20px;--exact-translate-composes:1;scale:calc(var(--exact-press-factor,1))">Slab</div>`;
          const el = root.firstElementChild;
          const read = () => {
            const s = getComputedStyle(el), p = Number(s.getPropertyValue('--exact-pressed'));
            const [x, y = 0] = s.translate.split(' ').map(parseFloat);
            return [p, Number(s.scale), x, y];
          };
          exact.hold(el, 'active');
          const animation = el.getAnimations().find(a => a.effect.getKeyframes().some(f => '--exact-pressed' in f));
          if (agent) {
            results.push([scale, read()]);
            exact.hold(el, null);
            results.push([scale, read()]);
            continue;
          }
          animation.pause(); animation.currentTime = 30;
          const p = animation.effect.getComputedTiming().progress;
          results.push([scale, p, read()]);
          exact.hold(el, null);
          results.push([scale, p, read(), el.getAnimations().includes(animation)]);
          animation.finish(); await animation.finished; await new Promise(requestAnimationFrame);
          const release = el.getAnimations().find(a => a.effect.getKeyframes().some(f => '--exact-pressed' in f));
          release.pause(); release.currentTime = 30;
          results.push([scale, 1 - release.effect.getComputedTiming().progress, read()]);
          release.finish(); await release.finished; await new Promise(requestAnimationFrame);
          results.push([scale, 0, read()]);
        }
        return results;
      }, agent);
      if (agent) expect(seen).toEqual([[1, [1, 1, 40, 20]], [1, [0, 1, 0, 0]], [.5, [1, .5, 40, 20]], [.5, [0, 1, 0, 0]]]);
      else for (const [scale, progress, [offset, factor, x, y], same] of seen) {
        if (progress) { expect(progress).toBeGreaterThan(0); expect(progress).toBeLessThan(1); }
        expect(offset).toBeCloseTo(progress, 4);
        expect(factor).toBeCloseTo(1 + (scale - 1) * progress, 4);
        expect(x).toBeCloseTo(40 * progress, 2);
        expect(y).toBeCloseTo(20 * progress, 2);
        if (same != null) expect(same).toBe(true);
      }
    }
  } finally { await browser.close(); }
}, 60_000);
