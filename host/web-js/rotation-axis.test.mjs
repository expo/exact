import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { withLook } from '../web/look.mjs';
import { webCompiler, buildMotion } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
const baseStyle = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
const source = `keyframes fold
  to rotate="-90deg x"
keyframes angle
  to rotate=-90
keyframes planar
  to rotate="60deg z"
keyframes negative
  to rotate=60
component App
  state svgExplicit = "none"
  state svgInherited = "none"
  state svgImplicit = "none"
  action refuseSVG
    svgExplicit = "fold 1s linear both"
    svgInherited = "angle 1s linear both"
    svgImplicit = "planar 1s linear both"
  state negative = "0 0 -1 0deg"
  state spring = false
  action springTurn
    spring = true
  state rotation = "0deg x"
  action turn
    rotation = "0deg y"
  view
    column gap=20 padding=20
      button "SVG" testId="svg-refuse" press=refuseSVG
      svg width=100 height=100
        rect testId="svg-explicit" width=10 height=10 animation=svgExplicit
        rect testId="svg-inherited" width=10 height=10 rotate="0deg x" animation=svgInherited
        rect testId="svg-implicit" width=10 height=10 rotate="10deg x" animation=svgImplicit
      button "Spring" testId="spring-turn" press=springTurn
      view testId="spring" width=120 height=60 rotate=(spring ? "90deg y" : "90deg x") transition="rotate -exact-spring(100, 10, 1)"
      view testId="negative-literal" rotate="0 0 -1 0deg" animation="negative 1s linear both"
      view testId="negative-bound" rotate=negative animation="negative 1s linear both"
      button "Axis" testId="axis" press=turn
      view testId="explicit" width=120 height=60 background-color="red" animation="fold 1s linear both"
      view testId="implicit" width=120 height=60 background-color="blue" rotate="0deg x" animation="angle 1s linear both"
      view testId="dynamic" width=120 height=60 background-color="green" rotate=rotation animation="angle 1s linear both"
      view rotate="0deg x"
        view testId="plane" width=120 height=60 background-color="black" animation="angle 1s linear both"
`;

function usePlan(plan) {
  const instantiate = WebAssembly.instantiateStreaming;
  WebAssembly.instantiateStreaming = async (...args) => {
    const result = await instantiate(...args), wasm = result.instance.exports;
    if (!wasm.exact_boot) return result;
    let input;
    return { ...result, instance: { exports: { ...wasm,
      exact_in(n) { input = wasm.exact_in(n); return input; },
      exact_boot(width, height, n, preferences) {
        const launch = new Uint8Array(wasm.memory.buffer, input, n).slice();
        const ptr = wasm.exact_in(plan.length + n), bytes = new Uint8Array(wasm.memory.buffer, ptr, plan.length + n);
        bytes.set(plan); bytes.set(launch, plan.length);
        return wasm.exact_boot_plan(plan.length, width, height, n, preferences);
      },
    } } };
  };
}

for (const target of ['js', 'wasm']) {
  const check = target === 'wasm' ? need('fixture', process.env.EXACT_WEB_DIST ? null : 'EXACT_WEB_DIST must name an all-capabilities wasm build') : test;
  check(`${target}: the agent clock folds around explicit and declared axes`, async () => {
    const { cmd, env } = webCompiler();
    if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
    const dir = mkdtempSync(resolve(root, 'target/rotation-axis-'));
    let browser, server;
    try {
      writeFileSync(resolve(dir, 'app.contract'), source);
      const compile = spawnSync(cmd, ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8', env });
      expect(compile.status, compile.stderr).toBe(0);
      const motion = target === 'js' ? await buildMotion() : null;
      const dist = target === 'js' ? dir : resolve(process.env.EXACT_WEB_DIST);
      server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
        const path = new URL(request.url).pathname.slice(1);
        if (path === 'motion.wasm' && motion) return new Response(Bun.file(motion));
        if (!path && target === 'js') return new Response(`<style>${baseStyle}</style><link rel="stylesheet" href="/app.css"><div id="exact-root"></div><script type="module">import app from "/app.js"; app();</script>`, { headers: { 'content-type': 'text/html' } });
        const bases = target === 'wasm' ? [dist] : path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
        const file = bases.map(base => resolve(base, path || 'index.html')).find(existsSync);
        return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
      } });
      browser = await suiteBrowser();
      const page = await browser.newPage();
      if (target === 'wasm') await page.addInitScript(usePlan, [...readFileSync(resolve(dir, 'app.plan'))]);
      await page.goto(server.url.href);
      await page.waitForSelector('[data-testid="dynamic"]');
      await page.waitForFunction(() => typeof globalThis.exact?.settleAt === 'function' || globalThis.exact?.root?.dataset.moduleReady === 'true');
      await page.evaluate(async () => {
        const { animationClock } = await import('/navigation.js');
        window.time = 0;
        const clock = animationClock(() => time, () => null, () => {});
        window.tick = to => { clock.register(time); time = to; clock.seek(time); };
        tick(500);
      });
      const size = id => page.locator(`[data-testid="${id}"]`).evaluate(el => {
        const r = el.getBoundingClientRect(); return [r.width, r.height];
      });
      for (const id of ['explicit', 'implicit', 'dynamic']) {
        const [w, h] = await size(id);
        expect(w).toBeCloseTo(120, 3);
        expect(h).toBeCloseTo(60 * Math.SQRT1_2, 3);
      }
      const [w, h] = await size('plane');
      expect(w).toBeCloseTo(180 * Math.SQRT1_2, 3);
      expect(h).toBeCloseTo(w, 3);
      await page.locator('[data-testid="axis"]').click();
      const [dw, dh] = await size('dynamic');
      expect(dw).toBeCloseTo(120 * Math.SQRT1_2, 3);
      expect(dh).toBeCloseTo(60, 3);
      await page.evaluate(() => tick(1000));
      for (const id of ['explicit', 'implicit']) expect((await size(id))[1]).toBeLessThan(0.01);
      for (const id of ['negative-literal', 'negative-bound']) {
        const angle = await page.locator(`[data-testid="${id}"]`).evaluate(el => {
          const parts = getComputedStyle(el).rotate.split(' ').map(parseFloat);
          return parts.length === 1 ? parts[0] : parts.at(-1) * Math.sign(parts[2]);
        });
        expect(angle).toBeCloseTo(-60, 3);
      }
      await page.locator('[data-testid="spring-turn"]').click();
      await page.waitForFunction(() => document.querySelector('[data-testid="spring"]').getAnimations().some(a => a instanceof Animation && !(a instanceof CSSAnimation)));
      const spring = await page.locator('[data-testid="spring"]').evaluate(async el => {
        const a = el.getAnimations().find(a => !(a instanceof CSSAnimation));
        await a.ready;
        a.pause(); a.currentTime = 50;
        const frames = a.effect.getKeyframes();
        return { rotate: getComputedStyle(el).rotate, frames: frames.map(f => f.rotate) };
      });
      expect(spring.frames.length).toBeGreaterThan(2);
      const components = spring.rotate.split(' ').map(parseFloat);
      expect(components.length).toBe(4);
      expect(components[0]).toBeGreaterThan(0);
      expect(components[1]).toBeGreaterThan(0);
      expect(components[2]).toBeCloseTo(0, 5);
      expect(components[3]).toBeGreaterThan(60);
      await page.locator('[data-testid="svg-refuse"]').click();
      for (const id of ['svg-explicit', 'svg-inherited', 'svg-implicit']) {
        expect(await page.locator(`[data-testid="${id}"]`).evaluate(el => el.getAnimations().length)).toBe(0);
      }
    } finally {
      await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
    }
  }, 60000);
}
