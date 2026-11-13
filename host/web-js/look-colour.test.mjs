// A state look's `color` in a real browser, on a built app (LLP 1121 D8): a
// hovered link's colour reaches its text, through a `color-scheme` boundary
// that writes the link's token colour again on itself (modes.js), where the
// look's token resolves in the boundary's scheme; a text with its own colour
// keeps it, and the pointer leaving gives the base back. Under a literal base
// colour, which the boundary does not write again, the look's token resolves
// in the boundary's scheme too.
import { afterAll, beforeAll, expect, test as bunTest } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cdpBrowser, chromium } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { webCompiler } from './module.mjs';

const { unavailable } = chromium();
const test = need('chrome', unavailable, { test: bunTest });
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const design = 'tamber t\n\ntheme\n  ink = light-dark(#222222, #e2e2e2)\n  ink.nav = light-dark(#666666, #ffffff)\n';
const view = `component App
  state n = 0
  action go
    n = n + 1
  view
    column padding=40
      link href="/a" testId="nav" color="ink.nav"
        :hover
          color = "ink"
        text "Plain" testId="plain"
        column color-scheme="dark" testId="dark"
          text "Under dark" testId="under-dark"
        text "Own" testId="own" color="#0000ff"
      link href="/b" testId="literal" color="#666666"
        :hover
          color = "ink"
        column color-scheme="dark"
          text "Literal, under dark" testId="under-literal"
      link href="/r" testId="red-first"
        :hover
          color = "#cc0000"
        text "Red first"
      column testId="card" color="ink.nav" padding=20
        :hover
          color = "#0000cc"
        link href="/n" testId="nested"
          :hover
            color = "#cc0000"
          column color-scheme="dark"
            text "Nested, under dark" testId="nested-under"
          text "Nested" testId="nested-text"
      link href="/o" color="ink.nav"
        :hover
          color = "ink"
        column color-scheme="dark"
          :hover
            color = "#00aa00"
          text "Its own look" testId="own-look"
      column color="ink.nav" press=go
        :active
          color = "#cc00cc"
        column color-scheme="dark"
          text "Pressed, under dark" testId="press-under"
        text "Pressed" testId="press-plain"
      text "Away" testId="away" padding=40
`;
let dir, server, suite;
beforeAll(async () => {
  if (unavailable) return;
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-look-colour-')));
  const app = resolve(dir, 'app'), out = resolve(dir, 'out');
  mkdirSync(app);
  writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Looks', app: { id: 'test.looks', name: 'Looks' } }));
  writeFileSync(resolve(app, 'app.tamber'), design);
  writeFileSync(resolve(app, 'app.contract'), view);
  const built = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
  if (built.status !== 0) throw new Error(built.stdout + built.stderr);
  const { serveBuildTree } = await import('../web/serve.mjs');
  server = createServer((req, res) => serveBuildTree(out, req, res));
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  suite = await cdpBrowser({ args: ['--window-size=600,900'] });
}, 120_000);
afterAll(async () => {
  await suite?.close();
  server?.close();
  if (dir) rmSync(dir, { recursive: true, force: true });
}, 60_000);

/** A page of the built app in `scheme`, once the app has committed (the
 * boundary's colour is written again), and the calls the tests make on it. */
async function open(scheme = 'light') {
  const { cdp } = suite, { sessionId } = await suite.page();
  const call = (method, params) => cdp.send(method, params, sessionId);
  const evaluate = async expression => {
    const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  };
  await call('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: scheme }] });
  await call('Page.navigate', { url: `http://127.0.0.1:${server.address().port}/` });
  const at = id => `document.querySelector('[data-testid="${id}"]')`;
  await evaluate(`new Promise(ok => { const t = setInterval(() => { if (${at('dark')}?.style.color) { clearInterval(t); ok(); } }, 20); })`);
  // The pointer moved over `id`: its left edge, at its middle or (`corner`) its top.
  const move = async (id, corner = false) => {
    const [x, y] = await evaluate(`(() => { const r = ${at(id)}.getBoundingClientRect(); return [r.x + 5, ${corner ? 'r.y + 5' : 'r.y + r.height / 2'}]; })()`);
    await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
    return [x, y];
  };
  return { call, evaluate, at, move };
}

for (const scheme of ['light', 'dark']) {
  test(`a hovered link's colour reaches its text through a scheme boundary (${scheme})`, async () => {
    const { evaluate, move } = await open(scheme);
    const colours = () => evaluate(`Object.fromEntries(['nav', 'plain', 'under-dark', 'own', 'under-literal'].map(id => [id, getComputedStyle(document.querySelector('[data-testid="' + id + '"]')).color]))`);
    const [nav, ink] = scheme === 'light' ? ['rgb(102, 102, 102)', 'rgb(34, 34, 34)'] : ['rgb(255, 255, 255)', 'rgb(226, 226, 226)'];
    const rest = { nav, plain: nav, 'under-dark': 'rgb(255, 255, 255)', own: 'rgb(0, 0, 255)', 'under-literal': 'rgb(102, 102, 102)' };
    expect(await colours()).toEqual(rest);
    await move('plain');
    // The boundary's text takes `ink` in its own scheme, dark: #e2e2e2.
    expect(await colours()).toEqual({ ...rest, nav: ink, plain: ink, 'under-dark': 'rgb(226, 226, 226)' });
    await move('under-literal');
    expect(await colours()).toEqual({ ...rest, 'under-literal': 'rgb(226, 226, 226)' });
    await move('away');
    expect(await colours()).toEqual(rest);
  }, 60_000);
}

// Under a boundary, as beside it: the nearest look that holds gives the colour
// (the card's look set comes after the link's, which a link before it named
// first), a boundary's own look stands over the one around it, and a press's
// colour reaches it and goes with the release.
test('the nearest look, its own look and a press reach text under a boundary', async () => {
  const { call, evaluate, at, move } = await open();
  const colour = id => evaluate(`getComputedStyle(${at(id)}).color`);
  // The colour `id` settles on (the press eases): polled until it is `want`, or a second has passed.
  const settles = async (id, want) => expect(await evaluate(`new Promise(ok => { const end = performance.now() + 1000; const t = setInterval(() => { const c = getComputedStyle(${at(id)}).color; if (c === ${JSON.stringify(want)} || performance.now() > end) { clearInterval(t); ok(c); } }, 20); })`)).toBe(want);
  await move('nested-text');
  expect(await colour('nested-under')).toBe('rgb(204, 0, 0)');
  await move('card', true);
  expect(await colour('nested-under')).toBe('rgb(0, 0, 204)');
  await move('own-look');
  expect(await colour('own-look')).toBe('rgb(0, 170, 0)');
  const [x, y] = await move('press-plain');
  await call('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
  await settles('press-plain', 'rgb(204, 0, 204)');
  await settles('press-under', 'color(srgb 0.8 0 0.8)');
  await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
  await move('away');
  await settles('press-under', 'rgb(255, 255, 255)');
}, 60_000);
