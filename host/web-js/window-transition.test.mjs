// `startViewTransition()` on the JS target (LLP 1013.003): the action's tree update runs inside the browser's view
// transition, the root's own cross-fade at the browser's 0.25 s; a push in the same commit plays nothing of its own,
// and reduce motion cuts.
import { expect, beforeAll, afterAll } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';
import { engineTests } from '../../scripts/coverage.mjs';

const engineTest = await engineTests();

const SOURCE = `routes nav
  home "/"
  item "/item"

keyframes enter
  from opacity=0

component App
  state game = "a"
  action pick(g: string)
    startViewTransition()
    game = g
    nav = push(nav, path("item"))
  action open
    nav = push(nav, path("item"))
  view
    main navigationKey=\`\${top(nav).id}\` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` testId=\`route-\${e.name}\` position="absolute" top=0 bottom=0 left=0 right=0 background-color="white"
          ::view-transition-new
            animation="enter 2s linear both"
          text game testId="game"
          button press=pick("b") testId=\`pick-\${e.name}\`
            text "Pick"
          button press=open testId=\`open-\${e.name}\`
            text "Open"
`;

let dir, server;
beforeAll(async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  dir = mkdtempSync(resolve(root, 'target/window-transition-test-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Window Transition', app: { id: 'test.window-transition', name: 'Window Transition' }, host: { web: {} } }));
  writeFileSync(resolve(dir, 'app.contract'), SOURCE);
  const dist = resolve(dir, 'dist');
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'window-transition', '--out', dist, '--render', 'none'], {
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

const RECORD = () => {
  window.transitions = [];
  const start = Document.prototype.startViewTransition;
  Document.prototype.startViewTransition = function (update) {
    const t = start.call(this, update);
    transitions.push(t);
    return t;
  };
};

async function page({ reducedMotion = 'no-preference', engine = 'chromium' } = {}) {
  const browser = await suiteBrowser(engine), context = await browser.newContext({ viewport: { width: 390, height: 700 }, reducedMotion });
  const p = await context.newPage(), errors = [];
  p.on('pageerror', e => errors.push(e.message));
  await p.addInitScript(RECORD);
  await p.goto(server.url.href);
  await p.waitForFunction(() => typeof globalThis.exact?.afterCommit === 'function');
  const state = () => p.evaluate(() => ({
    started: transitions.length,
    game: document.querySelector('[data-testid=game]')?.textContent,
    parts: document.getAnimations().filter(a => a.effect?.pseudoElement?.startsWith('::view-transition'))
      .map(a => `${a.effect.pseudoElement} ${a.effect.getTiming().duration}`).sort(),
  }));
  const ready = () => p.evaluate(() => transitions.at(-1)?.ready.then(() => true, () => false));
  return { p, errors, state, ready, close: () => context.close() };
}

for (const engine of ['chromium', 'webkit']) engineTest[engine](`in ${engine} the action's commit cross-fades the whole page`, async () => {
  const { p, errors, state, ready, close } = await page({ engine });
  try {
    await p.click('[data-testid=pick-home]');
    expect(await ready()).toBe(true);
    const during = await state();
    expect(during.started).toBe(1);
    expect(during.game).toBe('b');
    // The root's own pair alone, at the browser's duration: the pushed route's 2 s entrance does not play.
    expect(during.parts.every(part => /^::view-transition-[a-z-]+\(root\) 250$/.test(part))).toBe(true);
    expect(during.parts).toContain('::view-transition-new(root) 250');
    expect(during.parts).toContain('::view-transition-old(root) 250');
    await p.evaluate(() => transitions[0].skipTransition());
    expect(errors).toEqual([]);
  } finally {
    await close();
  }
}, 60_000);

// A route's own entrance leaves the root unnamed in the transitions' style element; the fade names it again.
engineTest.chromium("the commit cross-fades the whole page after a route's own entrance has played", async () => {
  const { p, errors, state, ready, close } = await page();
  try {
    await p.click('[data-testid=open-home]');
    expect(await ready()).toBe(true);
    expect((await state()).parts.some(part => part.endsWith(' 2000') && !part.includes('(root)'))).toBe(true);
    await p.evaluate(() => transitions[0].skipTransition());
    await p.click('[data-testid=pick-item]');
    expect(await ready()).toBe(true);
    const during = await state();
    expect(during.started).toBe(2);
    expect(during.parts).toContain('::view-transition-new(root) 250');
    expect(during.parts.every(part => /\(root\) 250$/.test(part))).toBe(true);
    await p.evaluate(() => transitions[1].skipTransition());
    expect(errors).toEqual([]);
  } finally {
    await close();
  }
}, 60_000);

engineTest.chromium('under reduce motion the commit cuts', async () => {
  const { p, errors, state, close } = await page({ reducedMotion: 'reduce' });
  try {
    await p.click('[data-testid=pick-home]');
    await p.waitForFunction(() => document.querySelector('[data-testid=game]')?.textContent === 'b');
    expect((await state()).started).toBe(0);
    expect(errors).toEqual([]);
  } finally {
    await close();
  }
}, 60_000);
