// A history traversal flies its shared element on the JS target (LLP 1013.000 D7, "History
// traversals"): Back and Forward start a view transition that pairs the name, while the history
// mirror judges the traversal at once, from the router's settled value (LLP 1007 §4). The browser
// animating the traversal itself (`hasUAVisualTransition`), or reduced motion, cuts.
import { test, expect, beforeAll, afterAll } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

// Each route's content is mounted only while it is on top, so a pair leaves and arrives in one commit.
const SOURCE = `routes nav
  home "/"
    item "/item/:id"

style Fly
  -exact-layout-transition="200ms linear"

component App
  state locked = false
  derive current = top(nav)
  action back
    nav = back(nav)
  action follow(url: string)
    nav = go(nav, url)
  action lock
    locked = true

  view
    main navigationKey=\`\${current.id}\` navigationBack="back" navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` position="absolute" top=0 bottom=0 left=0 right=0
          if e.id == current.id
            if e.name == "home"
              header display="flex"
                text "Home" role="heading" aria-level=1
              link href=path("item", "a") testId="open"
                box sharedElement="photo" class=Fly width=40 height=40 background-color="red" testId="thumb"
            else
              header display="flex"
                button id="back" press=back disabled=locked testId="back"
                  text "Home"
                text \`Item \${e.params.id}\` role="heading" aria-level=2 testId="title"
              link href=path("item", "b") testId="next"
                text "Next"
              button press=lock testId="lock"
                text "Lock"
              box sharedElement="photo" class=Fly width=200 height=200 background-color="blue" testId="big"
`;

let dir, server;
beforeAll(async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  dir = mkdtempSync(resolve(root, 'target/shared-traversal-test-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Shared Traversal', app: { id: 'test.shared-traversal', name: 'Shared Traversal' }, host: { web: {} } }));
  writeFileSync(resolve(dir, 'app.contract'), SOURCE);
  const dist = resolve(dir, 'dist');
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'shared-traversal', '--out', dist, '--render', 'none'], {
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

// Each view transition the page starts, with the shared elements named as the browser captures the old state and once
// its update has run (a name there on an arriver is a pair that flies).
const RECORD = () => {
  window.flights = [];
  const named = () => [...document.querySelectorAll('[data-shared-element]')].filter(e => e.style.viewTransitionName).map(e => e.dataset.testid);
  const start = Document.prototype.startViewTransition;
  Document.prototype.startViewTransition = function (update) {
    const flight = { before: named(), after: null };
    flights.push(flight);
    return start.call(this, () => { const r = update(); flight.after = named(); return r; });
  };
};

/** The app as the page shows it and the address bar: the route on top, its title, the location. */
const shown = page => page.evaluate(() => ({
  top: document.querySelector('[data-testid=title]')?.textContent ?? (document.querySelector('[data-testid=thumb]') ? 'Home' : null),
  path: location.pathname,
}));

async function page({ reducedMotion = 'no-preference', uaTransition = false } = {}) {
  const browser = await suiteBrowser(), context = await browser.newContext({ viewport: { width: 390, height: 700 }, reducedMotion });
  const p = await context.newPage(), errors = [];
  p.on('pageerror', e => errors.push(e.message));
  await p.addInitScript(RECORD);
  // A traversal the browser animated itself: each popstate seen as one whose `hasUAVisualTransition` is true.
  if (uaTransition) await p.addInitScript(() => addEventListener('popstate', e => {
    if (e.hasUAVisualTransition) return;
    e.stopImmediatePropagation();
    dispatchEvent(new PopStateEvent('popstate', { state: e.state, hasUAVisualTransition: true }));
  }, true));
  await p.goto(server.url.href);
  // The shared-element piece is loaded after first paint (rt.js `pr`).
  await p.waitForFunction(() => typeof globalThis.exact?.afterCommit === 'function');
  const settle = () => p.waitForFunction(() => !document.querySelector('[data-exiting]') && !document.getAnimations().some(a => a.playState === 'running'));
  const until = async want => { await p.waitForFunction(w => {
    const top = document.querySelector('[data-testid=title]')?.textContent ?? (document.querySelector('[data-testid=thumb]') ? 'Home' : null);
    return top === w.top && location.pathname === w.path;
  }, want); await settle(); };
  const flights = () => p.evaluate(() => flights.splice(0));
  return { p, errors, until, flights, close: () => context.close() };
}

test('Back and Forward fly the shared element, and the address bar and the app agree', async () => {
  const { p, errors, until, flights, close } = await page();
  try {
    await p.click('[data-testid=open]');
    await until({ top: 'Item a', path: '/item/a' });
    expect(await flights()).toEqual([{ before: ['thumb'], after: ['big'] }]);
    // Back presses the route's Back control (routes.js `popped`).
    await p.evaluate(() => history.back());
    await until({ top: 'Home', path: '/' });
    expect(await flights()).toEqual([{ before: ['big'], after: ['thumb'] }]);
    // Forward goes to the root's `navigate`.
    await p.evaluate(() => history.forward());
    await until({ top: 'Item a', path: '/item/a' });
    expect(await flights()).toEqual([{ before: ['thumb'], after: ['big'] }]);
    await p.evaluate(() => history.back());
    await until({ top: 'Home', path: '/' });
    expect(await shown(p)).toEqual({ top: 'Home', path: '/' });
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('a Back during a Back, both judged before either flight\'s update, lands where the address bar does', async () => {
  const { p, errors, until, flights, close } = await page();
  try {
    await p.click('[data-testid=open]');
    await until({ top: 'Item a', path: '/item/a' });
    await p.click('[data-testid=next]');
    await until({ top: 'Item b', path: '/item/b' });
    await flights();
    // The second Back is asked for as the first's popstate is heard, before its view transition's update has run.
    await p.evaluate(() => { addEventListener('popstate', () => history.back(), { once: true }); history.back(); });
    await until({ top: 'Home', path: '/' });
    expect((await flights()).length).toBeGreaterThan(0);
    // The mirror is still the stack: Forward goes to item a.
    await p.evaluate(() => history.forward());
    await until({ top: 'Item a', path: '/item/a' });
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('a refused Back restores the entry', async () => {
  const { p, errors, until, flights, close } = await page();
  try {
    await p.click('[data-testid=open]');
    await until({ top: 'Item a', path: '/item/a' });
    await p.click('[data-testid=lock]');
    await flights();
    await p.evaluate(() => history.back());
    // The entry is put back: the address bar returns to the item the app kept.
    await p.waitForFunction(() => location.pathname === '/item/a' && history.state?.url === '/item/a');
    await p.waitForTimeout(200);
    expect(await shown(p)).toEqual({ top: 'Item a', path: '/item/a' });
    expect(await flights()).toEqual([]);
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('a traversal the browser animated, or under reduced motion, cuts', async () => {
  for (const options of [{ uaTransition: true }, { reducedMotion: 'reduce' }]) {
    const { p, errors, until, flights, close } = await page(options);
    try {
      await p.click('[data-testid=open]');
      await until({ top: 'Item a', path: '/item/a' });
      const forward = await flights();
      await p.evaluate(() => history.back());
      await until({ top: 'Home', path: '/' });
      expect(await flights()).toEqual([]);
      await p.evaluate(() => history.forward());
      await until({ top: 'Item a', path: '/item/a' });
      expect(await flights()).toEqual([]);
      // A link is no traversal: it flies unless motion is reduced.
      expect(forward, JSON.stringify(options)).toEqual(options.reducedMotion ? [] : [{ before: ['thumb'], after: ['big'] }]);
      expect(errors).toEqual([]);
    } finally { await close(); }
  }
}, 60_000);

test('the agent\'s back resolves once the flight\'s update has run', async () => {
  const browser = await open({ host: 'web', url: server.url.href, size: [390, 700] });
  try {
    await browser.tap('open');
    await browser.clock('settle');
    const back = await browser.back();
    expect(back.refused).toBeUndefined();
    expect(await browser.carrier.evaluate('[location.pathname, !!document.querySelector("[data-testid=thumb]"), !!globalThis.exact.viewTransition]')).toEqual(['/', true, true]);
  } finally { await browser.close(); }
}, 60_000);
