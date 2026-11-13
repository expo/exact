import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { withLook } from '../web/look.mjs';
import { webCompiler } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
const baseStyle = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
const source = `keyframes Draw
  from opacity=0.2
  to opacity=1

component App
  state trigger = "view"
  state shown = true
  action remove
    shown = false
  action jump
    scrollIntoView("port", 30, behavior="smooth")
  action unhold
    trigger = "none"
  view
    column
      button "Unhold" press=unhold testId="unhold"
      button "Remove" press=remove testId="remove"
      button "Jump" press=jump testId="jump"
      list virtualized=true height=120 width=240 estimated-item-height=40 id="port" testId="port"
        for item in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39] key=item
          row height=40
            if shown
              box testId=\`exit-\${item}\` width=1 height=1 -exact-exit-animation="Draw 1s linear both"
            box testId=\`box-\${item}\` width=20 height=20 animation="Draw 60s linear both"
            box testId=\`none-\${item}\` width=20 height=20 animation="Draw 60s linear both" -exact-animation-trigger="none"
            box testId=\`bound-\${item}\` width=20 height=20 animation="Draw 60s linear both" -exact-animation-trigger=trigger
            box testId=\`paused-\${item}\` width=20 height=20 animation="Draw 60s linear -30s both paused"
            svg width=20 height=20 viewBox="0 0 20 20"
              circle testId=\`svg-\${item}\` cx=10 cy=10 r=8 animation="Draw 60s linear both"
            list virtualized=true height=20 width=20 estimated-item-height=10 testId=\`inner-\${item}\`
              for inner in [0, 1, 2, 3, 4] key=inner
                box testId=\`nested-\${item}-\${inner}\` height=10 animation="Draw 60s linear both"
`;

// Replace the carrier's plan before boot, so no carrier-app resource runs.
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
  check(`${target}: prefetched box and SVG animations start on first reveal`, async () => {
    const { cmd } = webCompiler();
    if (cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
    const dir = mkdtempSync(resolve(root, 'target/animation-trigger-'));
    let browser, server;
    try {
      writeFileSync(resolve(dir, 'app.contract'), source);
      const compile = spawnSync(cmd, ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
      expect(compile.status, compile.stderr).toBe(0);
      const dist = target === 'js' ? dir : resolve(process.env.EXACT_WEB_DIST);
      server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
        const path = new URL(request.url).pathname.slice(1);
        if (!path && target === 'js') return new Response(`<style>${baseStyle}</style><link rel="stylesheet" href="/app.css"><div id="exact-root"></div><script type="module">import app from "/app.js"; app();window.ready=true;</script>`, { headers: { 'content-type': 'text/html' } });
        const bases = target === 'wasm' ? [dist] : path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
        const file = bases.map(base => resolve(base, path || 'index.html')).find(existsSync);
        return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
      } });
      browser = await suiteBrowser();
      const page = await browser.newPage();
      const errors = [];
      page.on('pageerror', e => errors.push(e.message));
      if (target === 'wasm') await page.addInitScript(usePlan, [...readFileSync(resolve(dir, 'app.plan'))]);
      await page.goto(server.url.href);
      await page.waitForFunction(() => document.querySelector('[data-testid="box-5"]'));
      // Use the agent's animation clock so elapsed-time checks do not depend on scheduling.
      await page.evaluate(async () => {
        const { animationClock } = await import('/navigation.js');
        window.time = 0;
        window.animationTime = animationClock(() => time, () => null, () => {});
        window.tick = to => { animationTime.register(time); time = to; animationTime.seek(time); };
        tick(500);
      });
      const initial = await page.evaluate(() => {
        const el = id => document.querySelector(`[data-testid="${id}"]`);
        window.plays = ['box-5', 'svg-5', 'nested-5-0'].map(id => el(id).getAnimations()[0]);
        const p = el('port').getBoundingClientRect(), b = el('box-5').getBoundingClientRect();
        return {
          outside: b.top >= p.bottom,
          plays: plays.map(a => [a.playState, a.currentTime]),
          paused: el('paused-5').getAnimations()[0].playState,
          inserted: el('none-5').getAnimations()[0].currentTime,
        };
      });
      expect(initial.outside).toBe(true);
      expect(initial.plays).toEqual([['paused', 0], ['paused', 0], ['paused', 0]]);
      expect(initial.paused).toBe('paused');
      expect(initial.inserted).toBe(500);
      await page.evaluate(() => new Promise(resolve => {
        const observer = new MutationObserver(() => {
          if (!document.querySelector('[data-exiting]')) return;
          tick(time);
          observer.disconnect();
          resolve();
        });
        observer.observe(document.querySelector('[data-testid="port"]'), { childList: true, subtree: true, attributes: true });
        document.querySelector('[data-testid="remove"]').click();
      }));
      expect(await page.evaluate(() => [...document.querySelectorAll('[data-exiting]')].every(el =>
        getComputedStyle(el).animationPlayState.split(',').every(s => s.trim() === 'running')))).toBe(true);
      await page.evaluate(() => tick(1500));
      await page.waitForFunction(() => !document.querySelector('[data-exiting]'));
      await page.evaluate(() => document.querySelector('[data-testid="unhold"]').click());
      await page.waitForFunction(() => document.querySelector('[data-testid="bound-5"]').getAnimations()[0].playState === 'running');
      await page.evaluate(() => { document.querySelector('[data-testid="port"]').scrollTop = 160; });
      await page.waitForFunction(() => plays.every(a => a.playState === 'running'));
      const revealed = await page.evaluate(() => {
        tick(time);
        const el = id => document.querySelector(`[data-testid="${id}"]`);
        return {
          times: plays.map(a => a.currentTime),
          same: ['box-5', 'svg-5', 'nested-5-0'].map((id, i) => el(id).getAnimations()[0] === plays[i]),
          paused: el('paused-5').getAnimations()[0].playState,
          nested: el('nested-5-2').getAnimations()[0].playState,
        };
      });
      expect(revealed.same).toEqual([true, true, true]);
      expect(revealed.times).toEqual([0, 0, 0]);
      expect(revealed.paused).toBe('paused');
      expect(revealed.nested).toBe('paused');
      await page.evaluate(() => { document.querySelector('[data-testid="inner-5"]').scrollTop = 20; });
      await page.waitForFunction(() => document.querySelector('[data-testid="nested-5-2"]').getAnimations()[0].playState === 'running');
      await page.evaluate(() => tick(time + 300));
      expect(await page.evaluate(() => plays.map(a => a.currentTime))).toEqual([300, 300, 300]);
      await page.evaluate(() => { document.querySelector('[data-testid="port"]').scrollTop = 0; });
      await page.waitForFunction(() => document.querySelector('[data-testid="port"]').scrollTop === 0);
      expect(await page.evaluate(() => plays.every(a => animationTime.runs(a) && a.currentTime === 300))).toBe(true);
      await page.evaluate(() => { document.querySelector('[data-testid="port"]').scrollTop = 1400; });
      await page.waitForFunction(() => !document.querySelector('[data-testid="box-5"]'));
      await page.evaluate(() => { document.querySelector('[data-testid="port"]').scrollTop = 0; });
      await page.waitForFunction(() => document.querySelector('[data-testid="box-5"]'));
      expect(await page.evaluate(() => ['box', 'svg'].map((kind, i) => {
        const a = document.querySelector(`[data-testid="${kind}-5"]`).getAnimations()[0];
        return a !== plays[i] && a.playState === 'paused' && a.currentTime === 0;
      }))).toEqual([true, true]);
      await page.evaluate(() => {
        window.approach = [];
        const port = document.querySelector('[data-testid="port"]');
        window.sample = () => {
          const box = document.querySelector('[data-testid="box-30"]');
          if (!box) return;
          const p = port.getBoundingClientRect(), b = box.getBoundingClientRect();
          if (b.top >= p.bottom || b.bottom <= p.top) approach.push(['box', 'svg'].map(kind =>
            getComputedStyle(document.querySelector(`[data-testid="${kind}-30"]`)).animationPlayState));
        };
        window.observer = new MutationObserver(sample);
        observer.observe(port, { childList: true, subtree: true, attributes: true, attributeFilter: ['style'] });
        port.addEventListener('scroll', sample);
        document.querySelector('[data-testid="jump"]').click();
        sample();
      });
      await page.waitForFunction(() => {
        const port = document.querySelector('[data-testid="port"]'), box = document.querySelector('[data-testid="box-30"]');
        if (!box) return false;
        const p = port.getBoundingClientRect(), b = box.getBoundingClientRect();
        return b.top < p.bottom && b.bottom > p.top && getComputedStyle(box).animationPlayState === 'running';
      });
      const approach = await page.evaluate(() => { observer.disconnect(); return window.approach; });
      expect(approach.length).toBeGreaterThan(0);
      expect(approach.every(states => states.every(s => s === 'paused'))).toBe(true);
      expect(errors).toEqual([]);
    } finally {
      await browser?.close(); server?.stop(true);
      rmSync(dir, { recursive: true, force: true });
    }
  }, 120000);
}
