// A bound `-exact-layout-transition` on the JS runtime (GitHub #809): the shape reduced motion takes, `none` while calm
// and a curve otherwise. A row holding the curve slides when a row above it goes; one whose value is `none` jumps; one
// that gains the curve in the commit that moves it slides from where it was, as presence-glue.js plays a style op that
// gains the row. Build exact-web-js once before running this test (docs/testing.md).
import { test, expect } from 'bun:test';
import { EXE } from '../../scripts/paths.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { withLook } from '../web/look.mjs';

const SOURCE = `component App
  state calm = false
  state shown = true
  action motion
    calm = not calm
  action drop
    shown = not shown
  action both
    calm = not calm
    shown = not shown
  view
    column
      button "Motion" press=motion testId="motion"
      button "Drop" press=drop testId="drop"
      button "Both" press=both testId="both"
      if shown
        text "First" testId="first" height=40
      text "Second" testId="second" height=40 -exact-layout-transition=(calm ? "none" : "2000ms linear")
`;

test.each(['item', 'index', 'match'].flatMap(region => [[region, false], [region, true]]))('a moving %s binding gains its curve from the new region value (computed: %s)', async (region, computed) => {
  const each = region !== 'match';
  const value = region === 'item' ? 'row.active' : region === 'index' ? 'index > 0' : 'active';
  const branch = each ? 'for row, index in rows key=row.id' : 'match first([active])\n        case some(active)';
  const indent = each ? '        ' : '          ';
  const app = await exitApp('chromium', `shape Row
  id: number
  active: bool

component App
  state rows = [Row(id=1, active=false), Row(id=2, active=false)]
  state active = false
  state ms = 2000
  state shown = true
  action move
    rows = [Row(id=2, active=false), Row(id=1, active=true)]
    active = true
    shown = ${each ? 'true' : 'false'}
  view
    column
      button "Move" press=move testId="move"
      if shown
        text "Before" height=40
      ${branch}
${indent}text "Row" testId=${each ? '`row-${row.id}`' : '"row-1"'} height=40 -exact-layout-transition=(${value} ? ${computed ? '`\${ms}ms  linear`' : '"2000ms linear"'} : "none")
${each ? '' : '        case none\n          text "Absent"'}
`);
  try {
    const result = await app.page.evaluate(async () => {
      const row = document.querySelector('[data-testid="row-1"]');
      const before = row.getBoundingClientRect().top;
      document.querySelector('[data-testid="move"]').click();
      const animations = row.getAnimations();
      for (const animation of animations) { animation.pause(); animation.currentTime = 0; }
      const start = row.getBoundingClientRect().top;
      await Promise.all(animations.map(animation => (animation.finish(), animation.finished)));
      return { same: row === document.querySelector('[data-testid="row-1"]'), css: row.style.getPropertyValue('--exact-layout-transition'), animations: animations.length, start, before, end: row.getBoundingClientRect().top };
    });
    expect(result.same).toBe(true);
    expect(result.css).toBe('2000 0 linear');
    expect(result.animations).toBeGreaterThan(0);
    expect(result.start).toBeCloseTo(result.before, 1);
    expect(Math.abs(result.end - result.before)).toBeGreaterThan(1);
  } finally { await app.close(); }
}, 30000);

test('constructed layout curves and whitespace variants animate', async () => {
  const source = SOURCE.replace('state calm = false', 'state calm = false\n  state ms = 2000')
    .replace('"2000ms linear"', '`\${ms}ms  linear`');
  const app = await exitApp('chromium', source);
  try {
    const result = await app.page.evaluate(async () => {
      const row = document.querySelector('[data-testid="second"]');
      document.querySelector('[data-testid="drop"]').click();
      const animations = row.getAnimations();
      const css = row.style.getPropertyValue('--exact-layout-transition');
      await Promise.all(animations.map(animation => (animation.finish(), animation.finished)));
      return { css, animations: animations.length };
    });
    expect(result.css).toBe('2000 0 linear');
    expect(result.animations).toBeGreaterThan(0);
  } finally { await app.close(); }
}, 30000);

test('a bound layout transition slides while it holds a curve, jumps at none, and slides from where it was when gained', async () => {
  const root = resolve(import.meta.dir, '../..');
  const target = resolve(root, 'target');
  mkdirSync(target, { recursive: true });
  const dir = mkdtempSync(resolve(target, 'layout-transition-'));
  const style = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), SOURCE);
    const compiled = spawnSync(resolve(target, `debug/exact-web-js${EXE}`), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(compiled.status, compiled.stderr).toBe(0);
    server = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) return new Response(`<style>${style}</style><link rel="stylesheet" href="/app.css">
        <div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`,
      { headers: { 'content-type': 'text/html' } });
      // A module that re-exports the host's own reaches it through `../web/`.
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    browser = await suiteBrowser();
    const page = await (await browser.newContext({ viewport: { width: 400, height: 700 } })).newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => globalThis.exact?.presenceLive);
    // The ops presence hears, as wasm's batch would carry them: one only for a view gaining (before) or losing or gaining (after) the row.
    await page.evaluate(() => {
      const p = globalThis.exact.presenceLive, heard = globalThis.heard = [];
      for (const k of ['before', 'after']) { const f = p[k].bind(p); p[k] = (batch, views) => { heard.push([k, (batch.ops ?? []).length]); return f(batch, views); }; }
    });
    const heard = () => page.evaluate(() => globalThis.heard.splice(0));
    // Each press: the row's declaration, then where it shows now against where it lands, and whether it plays.
    const press = (id) => page.evaluate(async (id) => {
      const second = document.querySelector('[data-testid="second"]');
      document.querySelector(`[data-testid="${id}"]`).click();
      await new Promise((r) => setTimeout(r));
      const shown = second.getBoundingClientRect().top, playing = second.getAnimations().length > 0;
      await Promise.all(second.getAnimations().map((a) => (a.finish(), a.finished)));
      return { css: second.style.getPropertyValue('--exact-layout-transition'), moving: playing && Math.abs(shown - second.getBoundingClientRect().top) > 1 };
    }, id);
    expect(await press('drop')).toEqual({ css: '2000 0 linear', moving: true });
    expect(await heard()).toEqual([['before', 0], ['after', 0]]);
    expect(await press('motion')).toEqual({ css: '', moving: false });
    expect(await heard()).toEqual([['before', 0], ['after', 1]]);
    // Calm: nothing to measure (no op), and it jumps.
    expect(await press('drop')).toEqual({ css: '', moving: false });
    expect(await heard()).toEqual([['before', 0], ['after', 0]]);
    // Gained in the commit that moves it (First goes again).
    expect(await press('both')).toEqual({ css: '2000 0 linear', moving: true });
    expect(await heard()).toEqual([['before', 1], ['after', 1]]);
    // Lost in the commit that moves it: it jumps, and nothing plays it after.
    expect(await press('both')).toEqual({ css: '', moving: false });
    expect(await heard()).toEqual([['before', 0], ['after', 1]]);
    expect(await page.evaluate(() => globalThis.exact.presenceLive.tracking())).toBe(false);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

test('removed motion views leave the registry after their exit and keep identity when reattached', async () => {
  const root = resolve(import.meta.dir, '../..');
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname.slice(1);
    if (!path) return new Response('<body><div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path === 'names.js') return new Response('export const sourceTypes={};', { headers: { 'content-type': 'text/javascript' } });
    const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [resolve(root, 'host/web-js'), resolve(root, 'host/web')];
    const file = bases.map(base => resolve(base, path)).find(existsSync);
    return file ? new Response(Bun.file(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('', { status: 404 });
  } });
  let browser;
  try {
    browser = await suiteBrowser(); const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/rt.js');
      const root = document.getElementById('exact-root');
      for (let i = 0; i < 100; i++) {
        const node = document.createElement('section'); root.append(node);
        const scope = rt.scope(() => rt.mo(node, () => [null, null, null, 1, null]));
        node.remove(); rt.end(scope);
      }
      await Promise.resolve(); await Promise.resolve();
      const retired = rt.Views.size;
      const node = document.createElement('section'); root.append(node);
      const scope = rt.scope(() => rt.mo(node, () => [null, null, null, 1, null]));
      const id = rt.viewId(node); rt.end(scope);
      await Promise.resolve();
      const exitKept = rt.Views.get(id) === node; // still connected while its exit plays
      node.remove(); await Promise.resolve(); await Promise.resolve();
      const exitGone = !rt.Views.has(id);
      root.append(node);
      const reattached = rt.viewId(node);
      node.remove(); await Promise.resolve(); await Promise.resolve();
      return { retired, exitKept, exitGone, same: id === reattached, remaining: rt.Views.size };
    });
    expect(result).toEqual({ retired: 0, exitKept: true, exitGone: true, same: true, remaining: 0 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['chromium', 'webkit'])('computed exit animations follow reduced motion beside fixed and computed press scales on %s', async engine => {
  const app = await exitApp(engine, `shape Media
  prefersReducedMotion: bool

keyframes leave
  to opacity=0 scale=0.75

component App
  resource media = exactViewport() as shape Media
  state shown = true
  action toggle
    shown = not shown
  view
    column
      text (media.prefersReducedMotion ? "reduced" : "full") testId="mode"
      button "Toggle" press=toggle testId="toggle"
      if shown
        box testId="fixed" width=40 height=40 background-color="#334455" press=toggle -exact-press-scale=0.97 -exact-exit-animation=(media.prefersReducedMotion ? "none" : "leave 2000ms linear both")
        box testId="computed" width=40 height=40 background-color="#334455" press=toggle -exact-press-scale=(media.prefersReducedMotion ? 1 : 0.97) -exact-exit-animation=(media.prefersReducedMotion ? "none" : "leave 2000ms linear both")
`, { reducedMotion: 'reduce' });
  try {
    const { page, drop } = app;
    await page.waitForFunction(() => document.querySelector('[data-testid="mode"]')?.textContent === 'reduced');
    expect(await drop('toggle')).toEqual({ ghosts: 0, scales: [], durations: [], remaining: 0 });
    await page.locator('[data-testid="toggle"]').click();
    await page.locator('[data-testid="computed"]').waitFor();
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.waitForFunction(() => document.querySelector('[data-testid="mode"]')?.textContent === 'full');
    expect(await drop('toggle')).toEqual({ ghosts: 2, scales: [0.875, 0.875], durations: [2000, 2000], remaining: 0 });
    await page.locator('[data-testid="toggle"]').click();
    await page.locator('[data-testid="computed"]').waitFor();
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.waitForFunction(() => document.querySelector('[data-testid="mode"]')?.textContent === 'reduced');
    expect(await drop('toggle')).toEqual({ ghosts: 0, scales: [], durations: [], remaining: 0 });
  } finally { await app.close(); }
}, 60000);

// The runner ends an arm without updating its bindings, so a removed node plays the exit it had when the commit began
test.each(['chromium', 'webkit'])('a removal plays the exit its commit began with, whichever write comes first, on %s', async engine => {
  const app = await exitApp(engine, `keyframes leave
  to opacity=0

component App
  state calm = true
  state shown = true
  action calmFirst
    calm = not calm
    shown = false
  action dropFirst
    shown = false
    calm = not calm
  action show
    shown = true
  view
    column
      button "Calm first" press=calmFirst testId="calm-first"
      button "Drop first" press=dropFirst testId="drop-first"
      button "Show" press=show testId="show"
      if shown
        text "Card" testId="card" -exact-exit-animation=(calm ? "none" : "leave 2000ms linear both")
`);
  try {
    const leaving = async button => {
      const { ghosts } = await app.drop(button);
      await app.page.locator('[data-testid="show"]').click();
      await app.page.locator('[data-testid="card"]').waitFor();
      return ghosts;
    };
    // Each action flips `calm`, so the removals alternate between starting calm and starting with the exit
    expect([await leaving('calm-first'), await leaving('calm-first'), await leaving('drop-first'), await leaving('drop-first')]).toEqual([0, 1, 0, 1]);
  } finally { await app.close(); }
}, 60000);

test('an exit ghost does not run its resize action when its fractional box is rounded', async () => {
  const app = await exitApp('chromium', `keyframes leave
  to opacity=0

component App
  state shown = true
  state calls = 0
  state seen = "none"
  action hide
    shown = false
  action fit(w: number, h: number)
    calls = calls + 1
    seen = \`\${w}x\${h}\`
  view
    column
      button "hide" press=hide testId="hide"
      text \`\${calls} \${seen} \${shown}\` testId="seen"
      if shown
        box testId="panel" width=200.4 height=80.4 resize=fit -exact-exit-animation="leave 600ms linear both"
`);
  try {
    const { page } = app;
    await page.waitForFunction(() => document.querySelector('[data-testid="seen"]')?.textContent.startsWith('1 '));
    const result = await page.evaluate(async () => {
      const panel = document.querySelector('[data-testid="panel"]');
      const seen = document.querySelector('[data-testid="seen"]');
      const before = seen.textContent;
      // Wait for the browser to report the ghost's changed box before checking that no action ran.
      const resized = new Promise(resolve => {
        const observer = new ResizeObserver(entries => {
          const rect = entries.find(entry => entry.target === panel)?.contentRect;
          if (panel.hasAttribute('data-exiting') && rect?.width === 200 && rect.height === 80) {
            observer.disconnect();
            resolve();
          }
        });
        observer.observe(panel);
      });
      const exiting = new Promise(resolve => {
        const observer = new MutationObserver(() => {
          if (!panel.hasAttribute('data-exiting')) return;
          observer.disconnect();
          for (const animation of panel.getAnimations()) animation.pause();
          resolve();
        });
        observer.observe(panel, { attributes: true, attributeFilter: ['data-exiting'] });
      });
      document.querySelector('[data-testid="hide"]').click();
      await exiting;
      await resized;
      await new Promise(requestAnimationFrame);
      const after = seen.textContent, connected = panel.isConnected;
      await Promise.all(panel.getAnimations().map(animation => (animation.finish(), animation.finished)));
      return { before, after, connected, removed: !panel.isConnected };
    });
    expect(result.before).toMatch(/^1 200\.\d+x80\.\d+ true$/);
    expect(result.connected).toBe(true);
    expect(result.removed).toBe(true);
    expect(result.after).toBe(result.before.replace(' true', ' false'));
  } finally { await app.close(); }
}, 30000);

test.each(['finished', 'skipped'].flatMap(outcome => ['arm', 'row', 'match', 'list'].map(region => [outcome, region])))('a bound exit keeps the preceding commit value behind a %s view transition (%s)', async (outcome, region) => {
  const rows = region === 'row' || region === 'list';
  const value = rows ? 'row.k' : region === 'match' ? 'value' : 'k';
  const loop = 'for row in rows key=(broken ? row.id / (row.id - row.id) : row.id)';
  const branch = region === 'list' ? `list virtualized=true height=100 estimated-item-height=40\n        ${loop}` : region === 'row' ? loop : region === 'match' ? 'match first(shown ? [k] : [])\n        case some(value)' : 'if shown';
  const indent = region === 'match' || region === 'list' ? '          ' : '        ';
  const app = await exitApp('chromium', `keyframes leave
  to opacity=0

shape Row
  id: number
  k: number

component App
  state rows = [Row(id=1, k=0)]
  state broken = false
  state swapped = false
  state shown = true
  state k = 0
  action swap
    swapped = not swapped
  action corrupt
    broken = true
  action next
    broken = false
    rows = shown ? [Row(id=1, k=(k + 1) % 3)] : []
    k = (k + 1) % 3
  action drop
    shown = false
    rows = []
    k = (k + 1) % 3
  action idle
    shown = shown
  action show
    k = 0
    rows = [Row(id=1, k=0)]
    shown = true
  view
    column
      button "Swap" press=swap testId="swap"
      button "Next" press=next testId="next"
      button "Corrupt" press=corrupt testId="corrupt"
      button "Drop" press=drop testId="drop"
      button "Show" press=show testId="show"
      button "Idle" press=idle testId="idle"
      if swapped
        text "After" sharedElement="marker" -exact-layout-transition="20ms linear"
      else
        text "Before" sharedElement="marker" -exact-layout-transition="20ms linear"
      ${branch}
${indent}text "Card" testId="card" -exact-exit-animation=(${value} == 0 ? "none" : ${value} == 1 ? "leave 2000ms linear both" : "leave 4000ms linear both")
${region === 'match' ? '        case none\n          text "Gone"' : ''}
`);
  try {
    for (const [changes, later, corrupt] of [[2, 0], [3, 0], [4, 0], [2, 1], [2, 3], [2, 'idle'], [2, 0, true]]) {
      const observed = await app.page.evaluate(async ({ changes, later, corrupt, outcome }) => {
        const press = id => document.querySelector(`[data-testid="${id}"]`).click();
        const card = document.querySelector('[data-testid="card"]');
        const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
        let transition;
        document.startViewTransition = update => {
          transition = start(async () => { await gate.promise; return update(); });
          transition.ready.catch(() => {});
          return transition;
        };
        try {
          press('swap');
          if (corrupt) press('corrupt');
          for (let i = 0; i < changes; i++) press('next');
          press('drop');
          if (later === 'idle') press('idle');
          else for (let i = 0; i < later; i++) press('next');
          const deferred = !!transition, before = card.hasAttribute('data-exiting');
          const css = card.style.getPropertyValue('--exact-exit-animation');
          if (outcome === 'skipped') transition.skipTransition();
          gate.resolve();
          await transition.updateCallbackDone;
          await transition.ready.catch(() => {});
          const ghosts = [...document.querySelectorAll('[data-exiting]')];
          const animations = ghosts.flatMap(node => node.getAnimations());
          const durations = animations.map(animation => animation.effect.getTiming().duration);
          await Promise.all(document.getAnimations().map(animation => (animation.finish(), animation.finished)));
          await transition.finished;
          return { deferred, before, css, ghosts: ghosts.length, durations, removed: !card.isConnected };
        } finally { document.startViewTransition = start; }
      }, { changes, later, corrupt, outcome });
      const durations = changes === 3 ? [] : [changes === 2 ? 4000 : 2000];
      expect(observed).toEqual({ deferred: true, before: false, css: '', ghosts: durations.length, durations, removed: true });
      await app.page.locator('[data-testid="show"]').click();
      await app.page.locator('[data-testid="card"]').waitFor();
    }
  } finally { await app.close(); }
}, 60000);

test('presence binding probes read the committed clock and release their dependencies', async () => {
  const app = await exitApp('chromium', SOURCE);
  try {
    const observed = await app.page.evaluate(async () => {
      const rt = await import('/rt.js'), bindings = await import('/presence-bindings.js');
      rt.clock.agent = true; rt.clock.now = 0; rt.commit(() => {});
      const root = document.getElementById('exact-root'), exit = document.createElement('div'), layout = document.createElement('div');
      root.append(exit, layout);
      const signal = rt.sig(0), value = () => { signal(); return rt.x_performanceNow() < 1000 ? 'none' : 'curve'; };
      const own = rt.scope(() => {
        rt.px(exit); rt.S(exit, '--exact-exit-animation', '', value);
        rt.lt(layout, v => ({ css: v === 'curve' ? '20 0 linear' : null }), value);
      });
      const listeners = signal.n.obs.size;
      // Moving the clock alone does not publish a new committed time.
      rt.clock.now = 2000;
      for (let i = 0; i < 3; i++) bindings.captureExits();
      const result = { exit: rt.exitOf(exit), gains: bindings.gains().length, listeners: signal.n.obs.size - listeners };
      rt.end(own); exit.remove(); layout.remove();
      return result;
    });
    expect(observed).toEqual({ exit: 'none', gains: 0, listeners: 0 });
  } finally { await app.close(); }
}, 30000);

test('presence row maps stay cached until their subject or key dependencies change', async () => {
  const app = await exitApp('chromium', SOURCE);
  try {
    const observed = await app.page.evaluate(async () => {
      const rt = await import('/rt.js'), bindings = await import('/presence-bindings.js');
      const items = Array.from({ length: 100_000 }, (_, i) => [i, 1000]);
      const subject = rt.sig(items), salt = rt.sig(0), unrelated = rt.sig(0);
      const root = document.getElementById('exact-root'), exit = document.createElement('div');
      root.append(exit);
      let keys = 0;
      const own = rt.scope(() => {
        const offset = rt.memo(() => salt() % 2);
        const row = bindings.presenceRows((p, item) => {
          rt.px(exit);
          rt.S(exit, '--exact-exit-animation', '', () => `leave ${item()[1]}ms linear both`);
        }, subject, item => { keys++; return item()[0] + offset(); });
        rt.scope(() => row(root, rt.sig(items[1]), rt.sig(1), 'number:1'));
      });
      const capture = () => {
        bindings.captureExits();
        const result = [keys, rt.exitOf(exit)];
        keys = 0;
        return result;
      };
      const uninspected = keys, first = capture(), idle = [];
      for (let i = 0; i < 3; i++) {
        rt.commit(() => rt.W(unrelated, unrelated() + 1));
        idle.push(capture());
      }
      rt.commit(() => rt.W(subject, items.with(1, [1, 7000])));
      const changed = capture();
      rt.commit(() => rt.W(salt, 2));
      const equalKeyInput = capture();
      rt.commit(() => rt.W(salt, 1));
      const rekeyed = capture();
      rt.commit(() => rt.W(subject, []));
      const removed = capture();
      rt.end(own); exit.remove();
      return { uninspected, first, idle, changed, equalKeyInput, rekeyed, removed,
        listeners: subject.n.obs.size + salt.n.obs.size };
    });
    expect(observed).toEqual({ uninspected: 0, first: [100_000, 'leave 1000ms linear both'],
      idle: Array.from({ length: 3 }, () => [0, 'leave 1000ms linear both']),
      changed: [100_000, 'leave 7000ms linear both'], equalKeyInput: [0, 'leave 7000ms linear both'],
      rekeyed: [100_000, 'leave 1000ms linear both'], removed: [0, 'leave 1000ms linear both'], listeners: 0 });
  } finally { await app.close(); }
}, 30000);

// runner/collection.rs: a report with both edges armed dispatches reachend in that report after a reachstart that
// changes nothing; after one that changes something, reachend waits for a later report. A row or arm whose value an
// earlier commit changed behind a view transition has its exit read when reachstart commits; publishing that value to
// the row or arm is no change of reachstart's.
test.each(['row', 'match'])('a reachstart that writes nothing offers reachend in its report while a changed %s waits behind a view transition', async region => {
  const branch = region === 'row' ? 'for row in rows key=row.id' : 'match first(rows)\n        case some(row)';
  const indent = region === 'row' ? '        ' : '          ';
  const app = await exitApp('chromium', `keyframes leave
  to opacity=0

shape Row
  id: number
  k: number

component App
  state rows = [Row(id=1, k=0)]
  state swapped = false
  state ends = 0
  action swap
    swapped = not swapped
  action next
    rows = [Row(id=1, k=1)]
  action earlier
    let n = 1
  action later
    ends = ends + 1
  view
    column
      button "Swap" press=swap testId="swap"
      button "Next" press=next testId="next"
      text \`\${ends}\` testId="ends"
      if swapped
        text "After" sharedElement="marker" -exact-layout-transition="20ms linear"
      else
        text "Before" sharedElement="marker" -exact-layout-transition="20ms linear"
      ${branch}
${indent}text "Card" testId="card" -exact-exit-animation=(row.k == 0 ? "none" : "leave 2000ms linear both")
${region === 'match' ? '        case none\n          text "Gone"\n' : ''}      list virtualized=true height=100 estimated-item-height=40 reachstart=earlier reachend=later
        for x in [1] key=x
          text "Row"
`);
  try {
    // The first report reaches both edges: reachstart writes nothing, so reachend follows it.
    await app.page.waitForFunction(() => document.querySelector('[data-testid="ends"]')?.textContent === '1');
    const observed = await app.page.evaluate(async () => {
      const ends = () => document.querySelector('[data-testid="ends"]').textContent;
      const press = id => document.querySelector(`[data-testid="${id}"]`).click();
      const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
      let transition;
      document.startViewTransition = update => {
        transition = start(async () => { await gate.promise; return update(); });
        transition.ready.catch(() => {});
        return transition;
      };
      try {
        press('swap');
        press('next');
        // Both edges armed again, and a report asked for: the list's rows are as they were. A microtask the
        // reachstart handler queues runs after the report that called it.
        const c = [...exact.lists.internals.Lists.values()][0], heard = [], [earlier, later] = c.edges;
        c.edges = [() => { queueMicrotask(() => heard.push('report done')); heard.push('reachstart'); return earlier(); },
          () => { heard.push('reachend'); return later(); }];
        c.edgeArmed = [true, true];
        for (const m of c.mounted) m.epoch = ++c.nextEpoch;
        c.revision++;
        exact.lists.internals.settled();
        await exact.lists.settle();
        const deferred = !!transition, waiting = ends();
        gate.resolve();
        await transition.finished.catch(() => {});
        return { deferred, waiting, after: ends(), heard };
      } finally { document.startViewTransition = start; }
    });
    expect(observed).toEqual({ deferred: true, waiting: '1', after: '2', heard: ['reachstart', 'reachend', 'report done'] });
  } finally { await app.close(); }
}, 30000);

// The stand-in that paints a box while its size moves carries the box's own translate as the box resolves it: 50% of
// its `transform-box`, here its content box (200px less 20px padding and 5px border a side: 150px), so 75px, not 50%
// of the stand-in's own border box.
test('a resizing box\'s stand-in paints at its translate, resolved in the box\'s transform-box', async () => {
  const app = await exitApp('chromium', `component App
  state tall = false
  action grow
    tall = not tall
  view
    column
      button "grow" press=grow testId="grow"
      box testId="card" width=200 height=(tall ? 160 : 100) box-sizing="border-box" padding="10px 20px" border="5px solid #333" background-color="#4a90d9" transform-box="content-box" translate="50% 0" -exact-layout-transition="2000ms linear"
`);
  try {
    const { page } = app;
    const at = await page.evaluate(async () => {
      const card = document.querySelector('[data-testid="card"]');
      const rest = card.getBoundingClientRect().left;
      document.querySelector('[data-testid="grow"]').click();
      await new Promise(resolve => setTimeout(resolve));
      const stand = card.previousElementSibling?.hasAttribute('data-exiting') ? card.previousElementSibling.firstElementChild : null;
      const animations = [card, ...(stand ? [stand.parentElement, stand] : [])].flatMap(node => node.getAnimations());
      await Promise.all(animations.map(async animation => { animation.pause(); await animation.ready; animation.currentTime = 1000; }));
      const result = { rest, card: card.getBoundingClientRect().left, surface: stand?.getBoundingClientRect().left ?? null,
        height: stand?.getBoundingClientRect().height ?? null };
      await Promise.all(animations.map(animation => (animation.finish(), animation.finished)));
      return result;
    });
    // Mid-transition the card is where it rests, and its stand-in, half grown, paints over it.
    expect(at.card).toBeCloseTo(at.rest, 1);
    expect(at.height).toBeCloseTo(130, 1);
    expect(at.surface).toBeCloseTo(at.card, 1);
  } finally { await app.close(); }
}, 30000);

/** `source` compiled to the JS target and opened in `engine` with presence loaded. `drop(button)` presses `button`,
 * reports the exit animations that start, then finishes them; `close()` removes the page, server and app. */
async function exitApp(engine, source, pageOptions) {
  const root = resolve(import.meta.dir, '../..'), target = resolve(root, 'target');
  const dir = mkdtempSync(resolve(target, 'exit-animation-'));
  const style = withLook(readFileSync(resolve(root, 'host/web/index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/)[1];
  let server, browser;
  const close = async () => { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); };
  try {
    writeFileSync(resolve(dir, 'app.contract'), source);
    const compiled = spawnSync(resolve(target, `debug/exact-web-js${EXE}`), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
    expect(compiled.status, compiled.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname.slice(1);
      if (!path) { return new Response(`<style>${style}</style><link rel="stylesheet" href="/app.css"><div id="exact-root"></div><script type="module">import app from '/app.js';app();</script>`, { headers: { 'content-type': 'text/html' } }); }
      const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [dir, import.meta.dir, resolve(root, 'host/web')];
      const file = bases.map(base => resolve(base, path)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response(null, { status: 404 });
    } });
    browser = await suiteBrowser(engine);
    const page = await browser.newPage(pageOptions);
    await page.goto(server.url.href);
    await page.evaluate(async () => { const { pieces } = await import('/rt.js'); await pieces(); });
    await page.waitForFunction(() => globalThis.exact?.presenceLive);
    const drop = button => page.evaluate(async button => {
      document.querySelector(`[data-testid="${button}"]`).click();
      await new Promise(resolve => setTimeout(resolve));
      const ghosts = [...document.querySelectorAll('[data-exiting]')];
      const animations = ghosts.flatMap(node => node.getAnimations({ subtree: true }));
      await Promise.all(animations.map(async animation => { animation.pause(); await animation.ready; animation.currentTime = 1000; }));
      const scales = ghosts.map(ghost => parseFloat(getComputedStyle(ghost).scale));
      const result = { ghosts: ghosts.length, scales, durations: animations.map(animation => animation.effect.getTiming().duration) };
      await Promise.all(animations.map(animation => (animation.finish(), animation.finished)));
      return { ...result, remaining: document.querySelectorAll('[data-exiting]').length };
    }, button);
    return { page, drop, close };
  } catch (e) {
    await close();
    throw e;
  }
}


test('text selection scopes release removed nodes and the document listener has one owner', async () => {
  const root = resolve(import.meta.dir, '../..');
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname.slice(1);
    if (!path) return new Response('<body><div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path === 'names.js') return new Response('export const sourceTypes={};', { headers: { 'content-type': 'text/javascript' } });
    const bases = path.startsWith('web/') ? [resolve(root, 'host')] : [resolve(root, 'host/web-js'), resolve(root, 'host/web')];
    const file = bases.map(base => resolve(base, path)).find(existsSync);
    return file ? new Response(Bun.file(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('', { status: 404 });
  } });
  let browser;
  try {
    browser = await suiteBrowser(); const page = await browser.newPage(); await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const listeners = new Set(), add = document.addEventListener.bind(document), remove = document.removeEventListener.bind(document);
      document.addEventListener = (name, fn, ...rest) => { if (name === 'selectionchange') listeners.add(fn); add(name, fn, ...rest); };
      document.removeEventListener = (name, fn, ...rest) => { if (name === 'selectionchange') listeners.delete(fn); remove(name, fn, ...rest); };
      const rt = await import('/rt.js'), nav = await import('/web/navigation/selection.js');
      const root = document.getElementById('exact-root');
      for (let i = 0; i < 100; i++) {
        const node = document.createElement('p'); node.textContent = 'row ' + i; root.append(node);
        const scope = rt.scope(() => rt.onSelectionChange(node, 'selectionchange', () => {}, () => {}));
        node.remove(); rt.end(scope);
      }
      const retired = nav.selectedTexts.size, idle = listeners.size;
      const node = document.createElement('p'); node.textContent = 'hello'; root.append(node);
      let heard = 0;
      const old = nav.onSelection(node, () => { heard += 100; }), stop = nav.onSelection(node, () => heard++); old();
      const range = document.createRange(); range.selectNodeContents(node); getSelection().removeAllRanges(); getSelection().addRange(range);
      document.dispatchEvent(new Event('selectionchange'));
      const active = listeners.size; stop(); stop(); node.remove();
      return { retired, idle, heard, active, remaining: nav.selectedTexts.size, listeners: listeners.size };
    });
    expect(result).toEqual({ retired: 0, idle: 0, heard: 1, active: 1, remaining: 0, listeners: 0 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);
