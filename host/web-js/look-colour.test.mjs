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

for (const scheme of ['light', 'dark']) {
  test(`a hovered link's colour reaches its text through a scheme boundary (${scheme})`, async () => {
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
    // The boundary's colour, written again once the app has committed.
    await evaluate(`new Promise(ok => { const t = setInterval(() => { if (${at('dark')}?.style.color) { clearInterval(t); ok(); } }, 20); })`);
    const colours = () => evaluate(`Object.fromEntries(['nav', 'plain', 'under-dark', 'own', 'under-literal'].map(id => [id, getComputedStyle(document.querySelector('[data-testid="' + id + '"]')).color]))`);
    const move = async id => {
      const [x, y] = await evaluate(`(() => { const r = ${at(id)}.getBoundingClientRect(); return [r.x + 5, r.y + r.height / 2]; })()`);
      await call('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
    };
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
