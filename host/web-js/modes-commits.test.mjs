// Browser mutation delivery and the work a modes pass does after a commit.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { EXE } from '../../scripts/paths.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { withoutDev } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
const sourceApp = `keyframes leave
  to opacity=0

component App
  state shown = true
  action drop
    shown = false
  view
    column color="ink"
      button "Drop" press=drop testId="drop"
      if shown
        column color-scheme="dark" testId="panel" -exact-exit-animation="leave 2000ms linear both"
          text "Panel"
`;
let server, browser, dir;
beforeAll(async () => {
  dir = mkdtempSync(resolve(root, 'target/modes-commits-'));
  writeFileSync(resolve(dir, 'app.contract'), sourceApp);
  writeFileSync(resolve(dir, 'app.forma'), 'forma t\n\ntheme\n  ink = light-dark(#222222, #eeeeee)\n');
  const built = spawnSync(resolve(root, `target/debug/exact-web-js${EXE}`), ['js', resolve(dir, 'app.contract'), '-o', dir], { encoding: 'utf8' });
  expect(built.status, built.stderr).toBe(0);
  // Read retention without adding a diagnostic to the shipped runtime.
  const source = readFileSync(new URL('./modes.js', import.meta.url), 'utf8') + '\nexport const retained = () => ({ records: Records.length, bounds: Bounds.size });';
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path.endsWith('/modes.js')) return new Response((path.startsWith('/release/') || path.startsWith('/built-release/')) ? withoutDev(source) : source, { headers: { 'content-type': 'text/javascript' } });
    if (/^\/(full|release)\/rt\.js$/.test(path)) return new Response('export const After = [], PropHooks = {}, inflight = { n: 0 }, say = console.error;', { headers: { 'content-type': 'text/javascript' } });
    if (/^\/built(-release)?\/$/.test(path)) return new Response('<link rel="stylesheet" href="app.css"><div id="exact-root"></div><script type="module">import app from "./app.js";app();</script>', { headers: { 'content-type': 'text/html' } });
    if (/^\/built(-release)?\//.test(path) || path.startsWith('/web/')) {
      const name = path.replace(/^\/built(-release)?\//, '');
      const file = path.startsWith('/web/') ? resolve(root, 'host', path.slice(1)) : [dir, import.meta.dir, resolve(root, 'host/web')].map(base => resolve(base, name)).find(existsSync);
      return file ? new Response(Bun.file(file)) : new Response(null, { status: 404 });
    }
    return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
  } });
  browser = await suiteBrowser();
});
afterAll(async () => { await browser?.close(); server?.stop(true); if (dir) rmSync(dir, { recursive: true, force: true }); });

async function fixture(copy) {
  const page = await browser.newPage();
  await page.goto(new URL(copy + '/', server.url).href);
  await page.evaluate(async () => {
    const { modes } = await import('./modes.js');
    const { After, inflight } = await import('./rt.js');
    const root = document.getElementById('exact-root');
    const classes = Object.fromEntries(Array.from({ length: 254 }, (_, i) => ['c' + i, ['color']]));
    classes.c1 = ['background-color'];
    window.install = (vars = classes) => modes([['density', ['regular', 'compact']]], { c0: { 0: 0 } }, ['color:var(--x-ink)'], vars, []);
    window.commit = async () => { for (const f of After) f(); while (inflight.n) await Promise.resolve(); };
    window.add = (parent = root, attrs = {}) => {
      const e = document.createElement('div');
      for (const [name, value] of Object.entries(attrs)) e.setAttribute(name, value);
      parent.append(e); return e;
    };
    const style = document.createElement('style');
    style.textContent = ':root,[data-x-modes="density=regular"]{--x-ink:#aa0000}[data-x-modes="density=compact"]{--x-ink:#00aa00}.c0{color:var(--x-ink)}';
    document.head.append(style);
    window.parentBox = add(root, { class: 'c0' });
    window.child = add(parentBox, { 'data-x-b': '0' });
    install(); await commit();
  });
  return page;
}

for (const copy of ['full', 'release']) {
  test(`${copy}: unrelated commits do no document-wide discovery, including a large anchor tree`, async () => {
    const page = await fixture(copy);
    try {
      const result = await page.evaluate(async () => {
        const root = document.getElementById('exact-root');
        const fragment = document.createDocumentFragment();
        for (let i = 0; i < 1416; i++) fragment.append(document.createElement('div'));
        for (let i = 0; i < 48820; i++) fragment.append(document.createComment('anchor'));
        root.append(fragment); await commit();
        const queries = { classes: 0, document: 0, root: 0 };
        const classes = Document.prototype.getElementsByClassName;
        Document.prototype.getElementsByClassName = function(...args) { queries.classes++; return classes.apply(this, args); };
        for (const proto of [Document.prototype, Element.prototype]) {
          const query = proto.querySelectorAll;
          proto.querySelectorAll = function(...args) { if (this === document) queries.document++; if (this === root) queries.root++; return query.apply(this, args); };
        }
        // The issue's invalidation: add and remove an unrelated element before each pass.
        for (let i = 0; i < 3; i++) { document.body.appendChild(document.createElement('div')).remove(); await commit(); }
        return queries;
      });
      console.log(`${copy}: three unrelated commits, 254 token classes, 48,820 anchors: ${JSON.stringify(result)}`);
      expect(result).toEqual({ classes: 0, document: 0, root: 0 });
    } finally { await page.close(); }
  });

  for (const delivered of [false, true]) test(`${copy}: ${delivered ? 'delivered' : 'queued'} records preserve moved boundaries and adoption`, async () => {
    const page = await fixture(copy);
    try {
      const result = await page.evaluate(async delivered => {
        const flush = async () => { if (delivered) await Promise.resolve(); exact.modes.set([]); };
        // An existing child precedes its new parent in the cached order.
        const parent = add(parentBox, { 'data-x-b': '0' });
        await commit();
        parent.append(child); parent.setAttribute('data-x-pin', 'density=compact');
        await flush();
        const moved = [child.getAttribute('data-x-modes'), getComputedStyle(child).color];
        const wrap = add(parent);
        wrap.innerHTML = '<div data-x-b="0"><div data-x-b="0"></div></div>';
        await flush();
        const nested = [...wrap.querySelectorAll('[data-x-b]')].map(e => [e.getAttribute('data-x-modes'), getComputedStyle(e).color]);
        wrap.remove(); await flush();
        parentBox.append(wrap); await flush();
        const remounted = [...wrap.querySelectorAll('[data-x-b]')].map(e => e.getAttribute('data-x-modes'));
        const adopted = add(parentBox, { style: 'color:rgb(1, 2, 3);background-color:rgb(4, 5, 6)' });
        await flush();
        adopted.className = 'c1 c0'; await flush();
        const cleared = [adopted.style.color, adopted.style.backgroundColor];
        // Installing another table must discover a class that was previously irrelevant.
        const later = add(parentBox, { class: 'later', style: 'color:rgb(1, 2, 3)' });
        await flush(); install({ later: ['color'] }); await flush();
        return { moved, nested, remounted, cleared, later: later.style.color };
      }, delivered);
      expect(result).toEqual({ moved: ['density=compact', 'rgb(0, 170, 0)'], nested: Array(2).fill(['density=compact', 'rgb(0, 170, 0)']), remounted: Array(2).fill('density=regular'), cleared: ['', 'rgb(4, 5, 6)'], later: '' });
    } finally { await page.close(); }
  });

  test(`${copy}: boundary attributes and root membership update the retained set`, async () => {
    const page = await fixture(copy);
    try {
      const result = await page.evaluate(async () => {
        parentBox.setAttribute('data-x-b', '1');
        parentBox.setAttribute('data-x-pin', 'density=compact');
        await commit();
        const added = child.getAttribute('data-x-modes');
        parentBox.removeAttribute('data-x-b'); await commit();
        const removed = child.getAttribute('data-x-modes');
        document.body.append(child);
        exact.modes.set([['density', 'compact']]);
        const outside = child.getAttribute('data-x-modes');
        parentBox.append(child); await commit();
        return [added, removed, outside, child.getAttribute('data-x-modes')];
      });
      expect(result).toEqual(['density=compact', 'density=regular', 'density=regular', 'density=compact']);
    } finally { await page.close(); }
  });
}


for (const copy of ['built', 'built-release']) test(`${copy}: an exit completion releases cached boundaries and removal records without another commit`, async () => {
  const page = await browser.newPage();
  try {
    await page.goto(new URL(copy + '/', server.url).href);
    await page.evaluate(async () => { const { pieces } = await import('./rt.js'); await pieces(); });
    await page.waitForFunction(() => globalThis.exact?.presenceLive);
    const result = await page.evaluate(async () => {
      const { After, inflight } = await import('./rt.js');
      const { retained } = await import('./modes.js');
      const panel = document.querySelector('[data-testid="panel"]');
      exact.modes.set([]);
      const before = retained();
      let commits = 0;
      After.push(() => commits++);
      document.querySelector('[data-testid="drop"]').click();
      while (inflight.n) await Promise.resolve();
      const atRemoval = commits;
      const animations = panel.getAnimations();
      const during = { connected: panel.isConnected, bounds: retained().bounds, animations: animations.length };
      await Promise.all(animations.map(a => (a.finish(), a.finished)));
      // The removal promise and observer delivery finish at the next task; no app commit follows.
      await new Promise(resolve => setTimeout(resolve, 0));
      return { before, during, after: retained(), connected: panel.isConnected, tuple: panel.$xd ?? null, furtherCommits: commits - atRemoval };
    });
    expect(result.before).toEqual({ records: 0, bounds: 1 });
    expect(result.during).toEqual({ connected: true, bounds: 1, animations: 1 });
    expect(result.connected).toBe(false);
    expect(result.furtherCommits).toBe(0);
    expect(result.after).toEqual({ records: 0, bounds: 0 });
    expect(result.tuple).toBeNull();
  } finally { await page.close(); }
});

test.each([false, true])('the agent drive boots and commits with modes (token classes: %s)', tokens => {
  const appDir = mkdtempSync(resolve(dir, 'drive-'));
  writeFileSync(resolve(appDir, 'app.forma'), 'forma t\n\naxes\n  density = regular | compact\n\ntheme\n  ink = #222222\n    when density=compact: #eeeeee\n');
  writeFileSync(resolve(appDir, 'app.contract'), `component App
  state n = 0
  action count
    n = n + 1
  view
    column${tokens ? ' color="ink"' : ''}
      button "Count" press=count testId="count"
      column -exact-mode="density=compact"
        text \`\${n}\` testId="n"
`);
  const batch = resolve(appDir, 'batch.json');
  writeFileSync(batch, JSON.stringify({ compiler: resolve(root, `target/debug/exact-web-js${EXE}`), work: resolve(appDir, 'runtime'), cases: [{ file: resolve(appDir, 'app.contract'), answers: [], facts: {}, hostSources: [], events: [{ tap: 'count' }, { tap: 'count' }] }] }));
  const driven = spawnSync(process.execPath, ['semantics/difftest/js/drive.mjs', batch], { cwd: root, encoding: 'utf8' });
  expect(driven.status, driven.stderr).toBe(0);
  const result = JSON.parse(driven.stdout);
  expect(result.error ?? result.outside).toBeUndefined();
  expect(result.lines.filter(line => line.startsWith('outcome '))).toEqual(['outcome ok', 'outcome ok', 'outcome ok']);
  expect(result.lines.filter(line => line.startsWith('view "n"'))).toEqual(['view "n" "0"', 'view "n" "1"', 'view "n" "2"']);
});
