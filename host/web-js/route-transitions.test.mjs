// A route's own entrance and exit on the JS target (LLP 1013.001): the route a push adds plays its
// `::view-transition-new`, the route a pop removes its `::view-transition-old`, in a view transition that names
// that route alone. The route beneath stays shown and the arriving route inert until it ends (D2, D6); `none` and
// no part start none; reduce motion, a traversal the browser animated (D5) and a second change (D6) cut.
import { test, expect, beforeAll, afterAll } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';
import { engineTests } from '../../scripts/coverage.mjs';

const engineTest = await engineTests();

// Long parts, so that a test sees the transition while it runs.
const SOURCE = `routes nav
  home "/"
  item "/item"
  cut "/cut"
  plain "/plain"

keyframes enter
  from opacity=0 translate="10% 0"
keyframes leave
  to opacity=0 rotate="5deg"

style Slides
  ::view-transition-new
    animation="enter 2s linear both"
  ::view-transition-old
    animation="leave 2s linear both"

style Cuts
  ::view-transition-new
    animation="none"
  ::view-transition-old
    animation="none"

component App
  action go(to: string)
    nav = push(nav, to)
  action back
    nav = back(nav)
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "item"
          column navigationKey=\`\${e.id}\` class=Slides testId="route-item" position="absolute" top=0 bottom=0 left=0 right=0 background-color="white"
            Body(name=e.name, go=go, back=back)
        else if e.name == "cut"
          column navigationKey=\`\${e.id}\` class=Cuts testId="route-cut" position="absolute" top=0 bottom=0 left=0 right=0 background-color="white"
            Body(name=e.name, go=go, back=back)
        else
          column navigationKey=\`\${e.id}\` testId=\`route-\${e.name}\` position="absolute" top=0 bottom=0 left=0 right=0 background-color="white"
            Body(name=e.name, go=go, back=back)
      button press=back testId="page-back" position="absolute" bottom=0 left=0
        text "Page Back"

component Body
  props
    name: string
    go: action
    back: action
  view
    column
      button id="back" press=back testId=\`back-\${name}\`
        text "Back"
      button press=go(path("item")) testId=\`item-\${name}\`
        text "Item"
      button press=go(path("cut")) testId=\`cut-\${name}\`
        text "Cut"
      button press=go(path("plain")) testId=\`plain-\${name}\`
        text "Plain"
`;

let dir, server;
beforeAll(async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  dir = mkdtempSync(resolve(root, 'target/route-transitions-test-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Route Transitions', app: { id: 'test.route-transitions', name: 'Route Transitions' }, host: { web: {} } }));
  writeFileSync(resolve(dir, 'app.contract'), SOURCE);
  const dist = resolve(dir, 'dist');
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'route-transitions', '--out', dist, '--render', 'none'], {
    cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
  expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    const path = new URL(req.url).pathname, file = Bun.file(resolve(dist, path === '/' || !path.includes('.') ? 'index.html' : path.slice(1)));
    return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
  } });
}, 120_000);
afterAll(() => { server?.stop(true); if (dir) rmSync(dir, { recursive: true, force: true }); });

// Each view transition the page starts, and the transition itself, for the test to wait on.
const RECORD = () => {
  window.transitions = [];
  const start = Document.prototype.startViewTransition;
  Document.prototype.startViewTransition = function (update) {
    const t = start.call(this, update);
    transitions.push(t);
    return t;
  };
};

async function page({ reducedMotion = 'no-preference', uaTransition = false, engine = 'chromium' } = {}) {
  const browser = await suiteBrowser(engine), context = await browser.newContext({ viewport: { width: 390, height: 700 }, reducedMotion });
  const p = await context.newPage(), errors = [];
  p.on('pageerror', e => errors.push(e.message));
  await p.addInitScript(RECORD);
  if (uaTransition) await p.addInitScript(() => addEventListener('popstate', e => {
    if (e.hasUAVisualTransition) return;
    e.stopImmediatePropagation();
    dispatchEvent(new PopStateEvent('popstate', { state: e.state, hasUAVisualTransition: true }));
  }, true));
  await p.goto(server.url.href);
  // The piece that runs them is loaded after first paint (rt.js `pr`).
  await p.waitForFunction(() => typeof globalThis.exact?.afterCommit === 'function');
  /** The routes shown and inert, the started transitions and the parts their animations play, by name. */
  const state = () => p.evaluate(() => ({
    routes: [...document.querySelectorAll('[data-testid^=route-]')].map(r => ({
      name: r.dataset.testid.slice(6), hidden: r.style.visibility === 'hidden', inert: r.inert,
    })),
    started: transitions.length,
    parts: document.getAnimations().filter(a => a.effect?.pseudoElement?.startsWith('::view-transition'))
      .map(a => `${a.effect.pseudoElement.replace(/\(.*\)/, '')} ${a.animationName}`).sort(),
  }));
  const ready = () => p.evaluate(() => transitions.at(-1)?.ready.then(() => true, () => false));
  const finished = () => p.evaluate(() => Promise.all(transitions.map(t => t.finished.catch(() => {}))).then(() => true));
  return { p, errors, state, ready, finished, close: () => context.close() };
}

const tap = (p, id) => p.click(`[data-testid=${id}]`);
const shown = s => s.routes.filter(r => !r.hidden).map(r => r.name);

// Chrome, Firefox and Safari run same-document view transitions alike.
for (const engine of ['chromium', 'firefox', 'webkit']) engineTest[engine](`in ${engine} a push plays the entrance over the route beneath, which stays shown while the arriving route is inert`, async () => {
  const { p, errors, state, ready, finished, close } = await page({ engine });
  try {
    await tap(p, 'item-home');
    expect(await ready()).toBe(true);
    const during = await state();
    expect(during.started).toBe(1);
    expect(during.parts).toEqual(['::view-transition-new enter']);
    expect(shown(during)).toEqual(['home', 'item']);
    expect(during.routes.find(r => r.name === 'item').inert).toBe(true);
    await p.evaluate(() => transitions[0].skipTransition());
    await finished();
    const after = await state();
    expect(shown(after)).toEqual(['item']);
    expect(after.routes.find(r => r.name === 'item').inert).toBe(false);
    // The pop plays the exit of the route it removes, the route beneath the live page.
    await tap(p, 'back-item');
    expect(await ready()).toBe(true);
    const popping = await state();
    expect(popping.parts).toEqual(['::view-transition-old leave']);
    expect(popping.routes.map(r => r.name)).toEqual(['home']);
    await p.evaluate(() => transitions.at(-1).skipTransition());
    await finished();
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('`none` and no part start no transition: the author\'s cut, and the platform\'s, which is a cut here', async () => {
  const { p, errors, state, close } = await page();
  try {
    for (const [go, back] of [['cut-home', 'back-cut'], ['plain-home', 'back-plain']]) {
      await tap(p, go);
      await p.waitForTimeout(100);
      await tap(p, back);
      await p.waitForTimeout(100);
    }
    const s = await state();
    expect(s.started).toBe(0);
    expect(shown(s)).toEqual(['home']);
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('a second change ends the first at its end and plays its own', async () => {
  const { p, errors, state, ready, finished, close } = await page();
  try {
    await tap(p, 'item-home');
    expect(await ready()).toBe(true);
    // Back while the entrance plays, from the page: the arriving route takes no press.
    await tap(p, 'page-back');
    await p.waitForFunction(() => transitions.length === 2);
    expect(await ready()).toBe(true);
    const popping = await state();
    expect(popping.parts).toEqual(['::view-transition-old leave']);
    await p.evaluate(() => transitions.at(-1).skipTransition());
    await finished();
    const after = await state();
    expect(shown(after)).toEqual(['home']);
    expect(after.routes.find(r => r.name === 'home').inert).toBe(false);
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('reduce motion, and a traversal the browser animated, cut', async () => {
  for (const options of [{ reducedMotion: 'reduce' }, { uaTransition: true }]) {
    const { p, errors, state, ready, finished, close } = await page(options);
    try {
      if (options.uaTransition) {
        await tap(p, 'item-home');
        await ready();
        await p.evaluate(() => transitions[0].skipTransition());
        await finished();
        await p.evaluate(() => history.back());
        await p.waitForFunction(() => document.querySelectorAll('[data-testid^=route-]').length === 1);
        expect((await state()).started).toBe(1);
      } else {
        await tap(p, 'item-home');
        await p.waitForTimeout(100);
        await tap(p, 'back-item');
        await p.waitForTimeout(100);
        expect((await state()).started).toBe(0);
      }
      expect(shown(await state())).toEqual(['home']);
      expect(errors).toEqual([]);
    } finally { await close(); }
  }
}, 60_000);
