// @ref LLP 1123 D9, D17 — each reader's own safe-area observation on the web, in Chrome with a phone's safe area, and in
// WebKit and Firefox with that band given: a
// reader at the top takes the status bar's depth and one below it none; a reader its own input moves settles over a few
// rounds; one that moves back and forth is unsettled at D9's bound; a reader out of the document takes `initial`; and
// an ancestor's value never reaches a reader, which has its own. The properties are unregistered, as activation leaves
// them, so `initial` is the guaranteed-invalid value and a reader's fallback applies.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

let server;
beforeAll(() => {
  server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/observe.js') {
      return new Response(Bun.file(resolve(import.meta.dir, '../navigation/observe.js')), { headers: { 'content-type': 'text/javascript' } });
    }
    if (path === '/transitions') return new Response(TRANSITIONS, { headers: { 'content-type': 'text/html' } });
    return new Response(`<!doctype html><meta name="viewport" content="width=device-width, viewport-fit=cover">
      <style>body { margin: 0 } div { box-sizing: border-box }</style>
      <div id="top" style="position:absolute;top:0;left:0;width:390px;height:200px;padding-top:var(--exact-safe-area-inset-top, 5px)"></div>
      <div id="low" style="position:absolute;top:300px;left:0;width:100px;height:100px;padding-top:var(--exact-safe-area-inset-top, 5px)"></div>
      <div id="settles" style="position:absolute;left:100px;width:50px;height:50px;top:calc(60px - var(--exact-safe-area-inset-top, 60px))"></div>
      <div id="swings" style="position:absolute;left:200px;width:50px;height:50px;top:calc(2 * var(--exact-safe-area-inset-top, 0px))"></div>
      <div style="position:absolute;top:0;left:300px;--exact-safe-area-inset-top:30px"><div id="inside" style="width:20px;height:400px;padding-top:var(--exact-safe-area-inset-top, 5px)"></div></div>
      <script type="module">
        import { observe, documentBand } from '/observe.js';
        const ids = ['top', 'low', 'settles', 'swings', 'inside'], gone = document.createElement('div');
        const readers = [...ids.map((id) => document.getElementById(id)), gone];
        // Chrome's override is the browser's env(); another engine takes the same band from the query
        const given = new URL(location.href).searchParams.get('band');
        const band = given ? given.split(',').map(Number) : documentBand(), { rounds, unsettled } = observe(readers, band);
        const inset = (el) => ['top', 'right', 'bottom', 'left'].map((e) => el.style.getPropertyValue('--exact-safe-area-inset-' + e));
        window.result = { band, rounds, unsettled: unsettled.map((el) => el.id || 'gone'), gone: inset(gone),
          inputs: Object.fromEntries(ids.map((id) => [id, inset(document.getElementById(id))])),
          padding: Object.fromEntries(['top', 'low', 'inside'].map((id) => [id, parseFloat(getComputedStyle(document.getElementById(id)).paddingTop)])),
          tops: Object.fromEntries(['settles', 'swings'].map((id) => [id, document.getElementById(id).getBoundingClientRect().top])) };
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
});
afterAll(() => server?.stop(true));

// Readers whose inputs transition: one at the top padding by its input over a second, one its input moves over a
// second, beside a box whose opacity is already transitioning, the rounds 100ms in
const TRANSITIONS = `<!doctype html><style>body { margin: 0 } div { box-sizing: border-box; position: absolute; width: 50px; height: 50px }</style>
  <div id="pads" style="top:0;left:0;height:200px;padding-top:var(--exact-safe-area-inset-top, 5px);transition:padding-top 1s linear"></div>
  <div id="moves" style="left:100px;top:calc(60px - var(--exact-safe-area-inset-top, 60px));transition:top 1s linear"></div>
  <div id="other" style="left:200px;top:300px;opacity:0;transition:opacity 1s linear"></div>
  <script type="module">
    import { observe } from '/observe.js';
    const other = document.getElementById('other');
    other.getBoundingClientRect();
    other.style.opacity = '1';
    getComputedStyle(other).opacity;
    setTimeout(() => {
      const running = Math.round(other.getAnimations()[0].currentTime);
      const readers = ['pads', 'moves'].map((id) => document.getElementById(id)), { rounds } = observe(readers, [47, 0, 34, 0]);
      const transitions = (el) => el.getAnimations().map((a) => [a.transitionProperty, a.effect.getKeyframes()[0][a.transitionProperty === 'top' ? 'top' : 'paddingTop'], a.effect.getKeyframes().at(-1)[a.transitionProperty === 'top' ? 'top' : 'paddingTop']]);
      window.result = { rounds, pads: transitions(readers[0]), moves: transitions(readers[1]), other: other.getAnimations().map((a) => Math.round(a.currentTime) - running),
        rule: document.documentElement.classList.contains('exact-rounds') };
    }, 100);
  </script>`;

for (const engine of ['chromium', 'webkit', 'firefox']) test(`a round's values start no transition, and the settled inputs one each, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    await page.goto(`${server.url.href}transitions`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    // The rounds read the layout, not a transition's first frame: the mover writes 47, 34, 21, 8 and 0, holding at round 5
    expect(r.rounds).toBe(5);
    expect(r.pads).toEqual([['padding-top', '5px', '47px']]);
    expect(r.moves).toEqual([['top', '0px', '60px']]);
    expect(r.other).toEqual([0]);
    expect(r.rule).toBe(false);
  } finally {
    await browser.close();
  }
});

for (const engine of ['chromium', 'webkit', 'firefox']) test(`each reader takes its own observation, in rounds until it holds or the bound, in ${engine}`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage({ viewport: { width: 390, height: 800 } });
    if (engine === 'chromium') {
      const cdp = await page.context().newCDPSession(page);
      await cdp.send('Emulation.setSafeAreaInsetsOverride', { insets: { top: 47, bottom: 34, left: 0, right: 0 } });
    }
    await page.goto(engine === 'chromium' ? server.url.href : `${server.url.href}?band=47,0,34,0`);
    await page.waitForFunction(() => window.result);
    const r = await page.evaluate(() => window.result);
    expect(r.band).toEqual([47, 0, 34, 0]);
    expect(r.inputs.top).toEqual(['47px', '0px', '0px', '0px']);
    expect(r.padding.top).toBe(47);
    expect(r.inputs.low).toEqual(['0px', '0px', '0px', '0px']);
    expect(r.padding.low).toBe(0);
    // 60 less its own depth under the bar: 13, then 26, 39, 52, past the bar, where it rests
    expect(r.tops.settles).toBe(60);
    expect(r.inputs.settles[0]).toBe('0px');
    // Twice its depth: 94 clears the bar, 0 takes it again, at the bound still swinging
    expect(r.rounds).toBe(24);
    expect(r.unsettled).toEqual(['swings']);
    // Its own, the bar's 47 from its top, not the 30 an ancestor says
    expect(r.inputs.inside[0]).toBe('47px');
    expect(r.padding.inside).toBe(47);
    expect(r.gone).toEqual(['initial', 'initial', 'initial', 'initial']);
  } finally {
    await browser.close();
  }
});
