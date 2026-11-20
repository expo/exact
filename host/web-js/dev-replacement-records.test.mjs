// Dev replacement records (host/web-js/checkpoint.js): a replacement restores the reader's
// selection, focus and scroll anchors, and gives each scroller back to its author's follower.
import { afterEach, expect, test } from 'bun:test';
import { stopAll } from '../../scripts/test-children.mjs';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));

// A dev loop a test started ends with the test, a timeout included.
afterEach(() => stopAll());

// Exercise document replacement independently of the dev loop's file watcher and reload admission.
function replacementFixture(dir) {
  const dist = join(dir, 'dist');
  let checkpoint;
  const build = () => {
    const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'replacement', '--out', dist, '--render', 'none', '--dev'], {
      cwd: ROOT, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8',
    });
    expect(result.status, result.stdout + result.stderr).toBe(0);
  };
  build();
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path.startsWith('/shared/')) return new Response(Bun.file(join(ROOT, 'host/web', path.slice('/shared/'.length))));
    if (path !== '/') return new Response(Bun.file(join(dist, path.slice(1))));
    const html = readFileSync(join(dist, 'index.html'), 'utf8');
    const script = checkpoint ? `<script type="application/vnd.exact.dev-checkpoint">${JSON.stringify(checkpoint).replaceAll('<', '\\u003c')}</script>` : '';
    return new Response(html.replace('<div id="exact-root"', script + '<div id="exact-root"'), { headers: { 'content-type': 'text/html' } });
  } });
  return { base: server.url.href, carry: cp => { checkpoint = cp; }, build, stop: () => server.stop(true) };
}

function replacementRecordsServer() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    const js = text => new Response(text, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/rt.js') return js('export const After = [], PropHooks = {}, Resources = [], inflight = { n: 1 }, clock = {}, data = {}, journal = []; export const settled = () => { for (const f of After) f(); }; export const carryRouter = null, late = null, launchLocation = null, res = null, sig = null;');
    if (path === '/navigation.js') return js(hostFixtureFile(new URL('../web/navigation.js', import.meta.url)));
    if (['/checkpoint.js', '/focus.js'].includes(path)) return js(Bun.file(new URL('.' + path, import.meta.url)));
    return new Response('<!doctype html><main id="exact-root" style="min-height:2000px"></main>', { headers: { 'content-type': 'text/html' } });
  } });
}

test('replacement records keep Shelf search selection and reading anchors', async () => {
  mkdirSync(join(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(join(ROOT, 'target/replacement-records-'));
  const source = join(dir, 'app.contract');
  const contract = height => `component App
  state query = "Piranesi"
  action edit(value)
    query = value
  view
    column
      input value=query input=edit testId="search"
      column height=120 width=300 overflow="auto" scrollFollowEnd=true testId="plain"
        column height=${height} flex-shrink=0
        for n in range(30) key=n
          text \`Paragraph \${n}\` height=40 flex-shrink=0 testId=\`plain-\${n}\`
      list height=120 width=300 overflow="auto" testId="book-detail"
        column height=${height}
        for n in range(30) key=n
          text \`Book \${n}\` height=40 testId=\`book-\${n}\`
      list virtualized=true estimated-item-height=40 height=120 width=300 testId="virtual"
        for n in range(30) key=n
          text \`Virtual \${n}\` height=(n < 5 ? ${height} : 40) testId=\`virtual-\${n}\`
      list virtualized=true appearance="none" estimated-item-height=40 height=100 width=300 testId="fixed"
        column height=200
        for n in range(30) key=n
          text \`Fixed \${n}\` height=40
        column height=200
      list virtualized=true appearance="none" estimated-item-height=40 height=100 width=300 testId="empty"
        column height=200
        for n in range(0) key=n
          text \`Empty \${n}\` height=40
      column height=${height}
      for n in range(50) key=n
        text \`Page \${n}\` height=40 testId=\`page-\${n}\`
      text "${height}" testId="label"
`;
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Reading', app: { id: 'test.reading', name: 'Reading' } }));
  writeFileSync(source, contract(40));
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.ready);
    await page.waitForFunction(() => globalThis.exact?.lists?.internals && [...exact.lists.internals.Lists.values()].every(c => c.geometry));
    await page.evaluate(() => {
      const field = document.querySelector('[data-testid="search"]');
      field.focus(); field.setSelectionRange(2, 5, 'backward');
      for (const id of ['plain', 'book-detail', 'virtual']) document.querySelector(`[data-testid="${id}"]`).scrollTop = 253;
      for (const id of ['fixed', 'empty']) document.querySelector(`[data-testid="${id}"]`).scrollTop = 50;
      window.scrollTo(0, 733);
    });
    await page.waitForFunction(() => [...exact.lists.internals.Lists.values()].every(c => c.geometry?.offset === (['fixed', 'empty'].includes(c.el.dataset.testid) ? 50 : 253)));
    const before = await page.evaluate(() =>
      [['plain-5', 'plain'], ['book-5', 'book-detail'], ['virtual-6', 'virtual'], ['page-10', null]].map(([id, port]) =>
        document.querySelector(`[data-testid="${id}"]`).getBoundingClientRect().top - (port ? document.querySelector(`[data-testid="${port}"]`).getBoundingClientRect().top : 0)));
    fixture.carry(await page.evaluate(() => ({ generation: exact.clock.generation, time: exact.clock.now, answers: [], carryAnswers: false,
      slots: exact.state[0].map(s => [s.n.devName, s.n.devType, s()]), ...globalThis.__exactDevRecords() })));
    writeFileSync(source, contract(100));
    fixture.build();
    await page.goto(fixture.base);
    await page.waitForFunction(() => document.querySelector('[data-testid="label"]')?.textContent === '100');
    await page.waitForFunction(() => globalThis.exact?.lists?.internals && [...exact.lists.internals.Lists.values()].every(c => c.geometry && !c.target && !c.correction));
    await page.evaluate(() => new Promise(ok => requestAnimationFrame(ok)));
    const after = await page.evaluate(() => {
      const field = document.querySelector('[data-testid="search"]');
      return { focused: document.activeElement === field, selection: [field.selectionStart, field.selectionEnd, field.selectionDirection],
        anchors: [['plain-5', 'plain'], ['book-5', 'book-detail'], ['virtual-6', 'virtual'], ['page-10', null]].map(([id, port]) =>
          document.querySelector(`[data-testid="${id}"]`).getBoundingClientRect().top - (port ? document.querySelector(`[data-testid="${port}"]`).getBoundingClientRect().top : 0)) };
    });
    expect({ focused: after.focused, selection: after.selection, anchored: after.anchors.map((top, i) => Math.abs(top - before[i]) <= 1) })
      .toEqual({ focused: true, selection: [2, 5, 'backward'], anchored: [true, true, true, true] });
    for (const id of ['fixed', 'empty']) expect(await page.getByTestId(id).evaluate(el => el.scrollTop)).toBe(50);
    await page.keyboard.insertText('x');
    expect(await page.getByTestId('search').inputValue()).toBe('Pixesi');
    await page.getByTestId('fixed').evaluate(el => { el.scrollTop = 1450; });
    await page.waitForFunction(() => [...exact.lists.internals.Lists.values()].find(c => c.el.dataset.testid === 'fixed')?.geometry?.offset === 1450);
    fixture.carry(await page.evaluate(() => ({ answers: [], carryAnswers: false, ...globalThis.__exactDevRecords() })));
    await page.goto(fixture.base);
    await page.waitForFunction(() => document.querySelector('[data-testid="fixed"]')?.scrollTop === 1450);
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test.each(['focus', 'autofocus', 'vertical', 'horizontal'])('replacement records restore a settled list window before focus and both offsets (%s)', async mode => {
  const dir = mkdtempSync(join(ROOT, 'target/replacement-list-window-'));
  const horizontal = mode === 'horizontal';
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Window', app: { id: 'test.window', name: 'Window' } }));
  writeFileSync(join(dir, 'app.contract'), `component App
  view
    list virtualized=true appearance="none" ${horizontal ? 'display="flex" estimated-item-width=40' : 'estimated-item-height=40'} height=240 width=240 overflow="auto" testId="list"
      for n in range(80) key=n
        column width=${horizontal ? 40 : 600} height=${horizontal ? 600 : 40} flex-shrink=0
          input value="Piranesi" ${mode === 'autofocus' ? 'autofocus=(n == 27)' : ''} testId=\`field-\${n}\`
`);
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const settle = () => page.evaluate(async () => {
      await exact.lists.settle();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      await exact.lists.settle();
    });
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.lists?.internals?.controller());
    await settle();
    await page.getByTestId('list').evaluate((el, horizontal) => el.scrollTo({
      left: horizontal ? 1000 : 75, top: horizontal ? 75 : 1000, behavior: 'instant',
    }), horizontal);
    await settle();
    if (['focus', 'autofocus'].includes(mode)) await page.getByTestId('field-27').evaluate(el => {
      el.focus({ preventScroll: true }); el.setSelectionRange(2, 5, 'backward');
    });
    const offsets = () => page.getByTestId('list').evaluate(el => [el.scrollLeft, el.scrollTop]);
    const before = await offsets();
    expect(before).toEqual(horizontal ? [1000, 75] : [75, 1000]);
    fixture.carry(await page.evaluate(() => ({ answers: [], carryAnswers: false, ...globalThis.__exactDevRecords() })));
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.lists?.internals?.controller());
    await settle();
    if (['focus', 'autofocus'].includes(mode)) {
      expect(await page.evaluate(() => document.activeElement?.dataset.testid)).toBe('field-27');
      expect(await page.getByTestId('field-27').evaluate(el => [el.selectionStart, el.selectionEnd, el.selectionDirection])).toEqual([2, 5, 'backward']);
      await page.keyboard.insertText('x');
      expect(await page.getByTestId('field-27').inputValue()).toBe('Pixesi');
    } else expect(await offsets()).toEqual(before);
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test.each(['vertical', 'horizontal', 'rows', 'padding'])('replacement records retain both list offsets while a resource changes their range (%s)', async mode => {
  const dir = mkdtempSync(join(ROOT, 'target/replacement-list-range-'));
  const horizontal = mode === 'horizontal', modules = join(dir, 'modules/web');
  mkdirSync(modules, { recursive: true });
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'List range', app: { id: 'test.list.range', name: 'List range' } }));
  writeFileSync(join(dir, 'app.contract'), `shape Extent
  cross: number
  count: number
component App
  resource extent = extent() as shape Extent else empty(cross=240, count=${mode === 'rows' ? 5 : 80})
  view
    column height=120 width=300 overflow="auto" testId="outer"
      column height=100 flex-shrink=0
      list virtualized=true appearance="none" ${horizontal ? 'display="flex" estimated-item-width=40' : 'estimated-item-height=40'} height=240 width=240 flex-shrink=0 padding=${mode === 'padding' ? 20 : 0} overflow="auto" testId="list"
        for n in range(extent.count) key=n
          column width=${horizontal ? '40' : 'extent.cross'} height=${horizontal ? 'extent.cross' : '40'} flex-shrink=0
            text \`Row \${n}\`
      column height=1000 flex-shrink=0
`);
  writeFileSync(join(dir, 'app.ts'), `export const grants = '';
export const answer = (_source: string, _args: unknown[], _store: unknown, _storage: unknown, native: any) => native.later(1);
`);
  writeFileSync(join(modules, 'index.js'), 'export const later = () => new Promise(ok => { globalThis.answerExtent = () => ok({ cross: 600, count: 80 }); });\n');
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const settle = () => page.evaluate(async () => {
      await exact.lists.settle();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      await exact.lists.settle();
    });
    const offsets = () => page.getByTestId('list').evaluate(el => [el.scrollLeft, el.scrollTop]);
    await page.goto(fixture.base);
    await page.waitForFunction(() => typeof globalThis.answerExtent === 'function');
    await page.evaluate(() => answerExtent());
    await page.waitForFunction(() => exact.resources.every(r => !r.pending()));
    await settle();
    const before = horizontal ? [1000, 75] : [mode === 'rows' ? 0 : 75, 1000];
    await page.getByTestId('list').evaluate((el, [left, top]) => el.scrollTo({ left, top, behavior: 'instant' }), before);
    await page.getByTestId('outer').evaluate(el => { el.scrollTop = 150; });
    await settle();
    expect(await offsets()).toEqual(before);
    expect(await page.getByTestId('outer').evaluate(el => el.scrollTop)).toBe(150);
    fixture.carry(await page.evaluate(() => ({ answers: [], carryAnswers: false, ...globalThis.__exactDevRecords() })));
    await page.goto(fixture.base);
    await page.waitForFunction(() => typeof globalThis.answerExtent === 'function');
    await settle();
    expect(await offsets()).toEqual(mode === 'rows' ? [0, 0] : horizontal ? [1000, 0] : [0, 1000]);
    expect(await page.getByTestId('outer').evaluate(el => el.scrollTop)).toBe(150);
    const pendingRestoration = await page.evaluate(() => history.scrollRestoration);
    await page.evaluate(() => answerExtent());
    await page.waitForFunction(() => exact.resources.every(r => !r.pending()));
    await settle();
    expect({ pendingRestoration, offsets: await offsets(), restoration: await page.evaluate(() => history.scrollRestoration) })
      .toEqual({ pendingRestoration: 'manual', offsets: before, restoration: 'auto' });
    expect(await page.getByTestId('outer').evaluate(el => el.scrollTop)).toBe(150);
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test.each(['reader', 'end', 'start', 'gone'])('replacement records restore a list reader and its followed end (%s)', async mode => {
  const dir = mkdtempSync(join(ROOT, 'target/replacement-end-'));
  const source = join(dir, 'app.contract');
  const contract = height => `component App
  view
    list virtualized=true appearance="none" estimated-item-height=40 ${mode !== 'end' ? 'scroll-start="end"' : ''} scrollFollowEnd=true height=300 width=300 testId="list"
      column height=${height}
      for n in range(30) key=${mode === 'gone' && height === 60 ? 'n + 30' : 'n'}
        text \`Row \${n}\` height=40
      column height=100
`;
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'End', app: { id: 'test.end', name: 'End' } }));
  writeFileSync(source, contract(0));
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.lists?.internals?.controller());
    await page.evaluate(() => exact.lists.settle());
    const before = await page.getByTestId('list').evaluate((el, mode) => {
      el.scrollTop = mode === 'start' ? 0 : mode === 'end' ? el.scrollHeight - el.clientHeight : 150;
      return el.scrollTop;
    }, mode);
    await page.evaluate(() => exact.lists.settle());
    const cp = await page.evaluate(() => ({ answers: [], carryAnswers: false, ...globalThis.__exactDevRecords() }));
    expect(cp.scrolls[0].axes.top.anchor.list.follows).toBe(mode === 'end');
    fixture.carry(cp);
    writeFileSync(source, contract(60));
    fixture.build();
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.lists?.internals?.controller());
    await page.evaluate(async () => {
      await exact.lists.settle();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      await exact.lists.settle();
    });
    const after = await page.getByTestId('list').evaluate(el => ({ offset: el.scrollTop, end: el.scrollHeight - el.clientHeight }));
    expect(after.offset).toBe(['start', 'gone'].includes(mode) ? 0 : mode === 'end' ? after.end : before + 60);
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test('replacement records give a hidden scroller back to its end follower', async () => {
  const dir = mkdtempSync(join(ROOT, 'target/replacement-hidden-follow-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Hidden', app: { id: 'test.hidden', name: 'Hidden' } }));
  writeFileSync(join(dir, 'app.contract'), `component App
  state shown = true
  state rows = 20
  action reveal
    shown = true
  action grow
    rows = rows + 10
  view
    column
      button press=reveal testId="reveal"
        text "Reveal"
      button press=grow testId="grow"
        text "Grow"
      column display=(shown ? "flex" : "none") height=100 width=300 overflow="auto" scrollFollowEnd=true testId="log"
        for n in range(rows) key=n
          text \`Line \${n}\` height=50 flex-shrink=0
`);
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const offset = () => page.getByTestId('log').evaluate(el => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(() => ok(el.scrollTop)))));
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.ready);
    await page.getByTestId('log').evaluate(el => { el.scrollTop = el.scrollHeight; });
    const cp = await page.evaluate(() => ({ answers: [], carryAnswers: false,
      slots: exact.state[0].map(s => [s.n.devName, s.n.devType, s()]), ...globalThis.__exactDevRecords() }));
    expect(cp.scrolls.map(s => s.axes.top.anchor)).toEqual([{ end: true }]);
    // The reload mounts the recorded scroller hidden. A commit with nothing pending drops its record unrestored.
    cp.slots.find(([name]) => name === 'shown')[2] = false;
    fixture.carry(cp);
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.ready);
    await page.getByTestId('grow').click();
    expect(await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(() => ok(globalThis.__exactDevRecords().restoreUntil)))))).toBeUndefined();
    await page.getByTestId('reveal').click();
    const shown = await offset();
    await page.getByTestId('grow').click();
    expect([shown, await offset()]).toEqual([1400, 1900]);
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test.each(['plain', 'list'])('replacement records restore nested scrollers from inside out (%s)', async mode => {
  const dir = mkdtempSync(join(ROOT, 'target/replacement-nested-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Nested', app: { id: 'test.nested', name: 'Nested' } }));
  writeFileSync(join(dir, 'app.contract'), `component App
  view
    column height=120 width=300 overflow="auto" testId="outer"
      ${mode === 'list' ? 'list virtualized=true appearance="none" estimated-item-height=40' : 'column overflow="auto"'} height=300 flex-shrink=0 testId="inner"
        for n in range(30) key=n
          text \`Row \${n}\` height=40 flex-shrink=0 testId=\`row-\${n}\`
      column height=1000 flex-shrink=0
`);
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const settle = () => page.evaluate(async () => {
      if (exact.lists) await exact.lists.settle();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      if (exact.lists) await exact.lists.settle();
    });
    const positions = () => page.evaluate(() => ({
      outer: document.querySelector('[data-testid="outer"]').scrollTop,
      inner: document.querySelector('[data-testid="inner"]').scrollTop,
      anchor: document.querySelector('[data-testid="row-8"]').getBoundingClientRect().top,
    }));
    await page.goto(fixture.base);
    await page.waitForFunction(mode => globalThis.exact?.ready && (mode !== 'list' || exact.lists?.internals?.controller()), mode);
    await settle();
    await page.getByTestId('inner').evaluate(el => { el.scrollTop = 100; });
    await page.getByTestId('outer').evaluate(el => { el.scrollTop = 200; });
    await settle();
    const before = await positions();
    expect(before).toMatchObject({ inner: 100, outer: 200 });
    fixture.carry(await page.evaluate(() => ({ answers: [], carryAnswers: false, ...globalThis.__exactDevRecords() })));
    await page.goto(fixture.base);
    await page.waitForFunction(mode => globalThis.exact?.ready && (mode !== 'list' || exact.lists?.internals?.controller()), mode);
    await settle();
    expect(await positions()).toEqual(before);
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test.each(['plain', 'list'])('replacement records restore containing list windows before descendant scrollers (%s)', async mode => {
  const dir = mkdtempSync(join(ROOT, 'target/replacement-row-scroller-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Row scrollers', app: { id: 'test.row.scrollers', name: 'Row scrollers' } }));
  writeFileSync(join(dir, 'app.contract'), `component App
  view
    list virtualized=true appearance="none" estimated-item-height=200 height=300 width=300 testId="outer"
      for n in range(80) key=n
        column height=200
          ${mode === 'list' ? 'list virtualized=true appearance="none" estimated-item-height=40' : 'column overflow="auto"'} height=200 flex-shrink=0 testId=\`inner-\${n}\`
            for m in range(30) key=m
              text \`Row \${n}:\${m}\` height=40 flex-shrink=0
`);
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const settle = () => page.evaluate(async () => {
      await exact.lists.settle();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      await exact.lists.settle();
    });
    const offsets = () => page.evaluate(() => ({
      outer: document.querySelector('[data-testid="outer"]').scrollTop,
      inner: document.querySelector('[data-testid="inner-20"]').scrollTop,
      others: [...document.querySelectorAll('[data-testid^="inner-"]')].filter(el => el.dataset.testid !== 'inner-20').map(el => el.scrollTop),
    }));
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.lists?.internals?.controller());
    await settle();
    await page.getByTestId('outer').evaluate(el => { el.scrollTop = 4000; });
    await settle();
    await page.getByTestId('inner-20').evaluate(el => { el.scrollTop = 130; });
    await settle();
    const before = await offsets();
    expect(before).toMatchObject({ outer: 4000, inner: 130 });
    expect(before.others.every(offset => offset === 0)).toBe(true);
    fixture.carry(await page.evaluate(() => ({ answers: [], carryAnswers: false, ...globalThis.__exactDevRecords() })));
    await page.goto(fixture.base);
    await page.waitForFunction(() => globalThis.exact?.lists?.internals?.controller());
    await page.waitForFunction(() => document.querySelector('[data-testid="inner-20"]')?.scrollTop === 130, null, { timeout: 5000 });
    await settle();
    expect(await offsets()).toEqual(before);
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test.each(['plain', 'focused', 'smooth'])('replacement records keep nested scroll boundaries and restore instantly (%s)', async mode => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    const js = text => new Response(text, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/rt.js') return js('export const After = [], PropHooks = {}, Resources = [], inflight = { n: 0 }, clock = {}, data = {}, journal = []; export const settled = () => {}; export const carryRouter = null, late = null, launchLocation = null, res = null, sig = null;');
    if (path === '/navigation.js') return js(hostFixtureFile(new URL('../web/navigation.js', import.meta.url)));
    if (['/checkpoint.js', '/focus.js'].includes(path)) return js(Bun.file(new URL('.' + path, import.meta.url)));
    return new Response('<main id="exact-root"></main>', { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async mode => {
      const { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      const content = height => `<div data-carry-type="Column" id="outer" style="height:120px;width:200px;overflow:auto;${mode === 'smooth' ? 'scroll-behavior:smooth' : ''}">
        <div data-carry-type="Column" id="inner" style="height:300px;width:400px;overflow:auto;${mode === 'smooth' ? 'scroll-behavior:smooth' : ''}">
          <div data-carry-type="Column" style="height:${height}px;width:900px"></div>
          <textarea data-carry-type="TextArea" style="display:block;height:100px;width:900px;padding:0;border:0">Anchor</textarea>
          <div data-carry-type="Column" style="height:1000px;width:900px"></div>
        </div>
        <div data-carry-type="Column" style="height:1000px;width:400px"></div>
      </div>`;
      const positions = () => ['inner', 'outer'].map(id => {
        const el = document.getElementById(id);
        return [el.scrollLeft, el.scrollTop];
      });
      root.innerHTML = content(100);
      prepareDev()();
      if (mode === 'focused') root.querySelector('textarea').focus({ preventScroll: true });
      document.getElementById('inner').scrollTo({ left: mode === 'smooth' ? 70 : 0, top: mode === 'smooth' ? 100 : 0, behavior: 'instant' });
      document.getElementById('outer').scrollTo({ left: mode === 'smooth' ? 40 : 0, top: mode === 'smooth' ? 200 : 150, behavior: 'instant' });
      const before = positions(), cp = globalThis.__exactDevRecords();
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(mode === 'smooth' ? 100 : 160);
      finish();
      const immediate = positions();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      return { before, immediate, after: positions() };
    }, mode);
    expect(result.immediate).toEqual(result.before);
    expect(result.after).toEqual(result.before);
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each([false, true])('replacement focus waits for Shelf notes to mount and yields to a later focus (autofocus: %s)', async auto => {
  mkdirSync(join(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(join(ROOT, 'target/replacement-notes-'));
  const modules = join(dir, 'modules/web');
  mkdirSync(modules, { recursive: true });
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Notes', app: { id: 'test.notes', name: 'Notes' } }));
  writeFileSync(join(dir, 'app.contract'), `shape Book
  id: string
component App
  state draft = "hello world"
  resource books = books() as shape Book[] else empty()
  action edit(value)
    draft = value
  view
    column
      input testId="search"
      list
        for book in books key=book.id
${auto ? '          input autofocus=true testId="late-search"\n' : ''}          input value=draft input=edit testId="notes"
`);
  writeFileSync(join(dir, 'app.ts'), `export const grants = '';
export const answer = (_source: string, _args: unknown[], _store: unknown, _storage: unknown, native: any) => native.later(1);
`);
  writeFileSync(join(modules, 'index.js'), 'export const later = () => new Promise(ok => { globalThis.answerBooks = () => ok([{ id: "book" }]); });\n');
  let fixture, browser;
  try {
    fixture = replacementFixture(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage(), messages = [];
    page.on('console', m => { if (m.type() === 'error' || m.text().startsWith('exact dev reload:')) messages.push(m.text()); });
    page.on('pageerror', e => messages.push(String(e)));
    await page.goto(fixture.base);
    await page.waitForFunction(() => typeof globalThis.answerBooks === 'function');
    await page.evaluate(() => answerBooks());
    await page.getByTestId('notes').waitFor();
    for (const cancel of [false, true]) {
      await page.getByTestId('notes').evaluate(field => { field.focus(); field.setSelectionRange(2, 7, 'backward'); });
      const checkpoint = await page.evaluate(() => ({ generation: exact.clock.generation, epoch: exact.clock.epoch, time: exact.clock.now,
        slots: exact.state[0].map(s => [s.n.devName, s.n.devType, s()]), answers: [], carryAnswers: false, ...globalThis.__exactDevRecords() }));
      fixture.carry(checkpoint);
      await page.goto(fixture.base);
      try {
        await page.waitForFunction(() => typeof globalThis.answerBooks === 'function');
      } catch (error) { throw new Error(`${error}\n${messages.join('\n')}\n${await page.locator('pre').allTextContents()}`); }
      expect(await page.getByTestId('notes').count()).toBe(0);
      if (cancel) await page.getByTestId('search').focus();
      await page.evaluate(() => answerBooks());
      await page.getByTestId('notes').waitFor();
      await page.waitForFunction(id => document.activeElement === document.querySelector(`[data-testid="${id}"]`), cancel ? 'search' : 'notes', { timeout: 5000 });
      const result = await page.getByTestId('notes').evaluate(field => ({ focused: document.activeElement === field,
        selection: [field.selectionStart, field.selectionEnd, field.selectionDirection] }));
      expect(result.focused).toBe(!cancel);
      if (!cancel) expect(result.selection).toEqual([2, 7, 'backward']);
    }
  } finally { await browser?.close(); await fixture?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 300000);

test.each(['plain', 'page'])('replacement records keep the block start with an inline offset (%s)', async mode => {
  const server = replacementRecordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.clock.install();
    await page.goto(server.url.href);
    const result = await page.evaluate(async mode => {
      const { prepareDev } = await import('/checkpoint.js'), rt = await import('/rt.js');
      const root = document.getElementById('exact-root');
      document.body.style.margin = '0';
      const content = height => `<div data-carry-type="Column" style="height:${height}px;width:1600px"></div><div data-carry-type="Text" style="height:1000px;width:1600px">Top</div>`;
      if (mode === 'plain') root.innerHTML = '<div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto"></div>';
      const container = mode === 'page' ? root : document.getElementById('port');
      const port = mode === 'page' ? document.scrollingElement : container;
      container.innerHTML = content(0);
      prepareDev()();
      port.scrollLeft = 40;
      const cp = globalThis.__exactDevRecords();
      cp.focus = { path: [99], type: 'Input' };
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      container.innerHTML = content(60);
      port.scrollLeft = 0;
      finish();
      return { offsets: [port.scrollLeft, port.scrollTop], anchors: cp.scrolls.flatMap(s => Object.values(s.axes).flatMap(a => a.anchor?.nodes ?? [])),
        restoration: history.scrollRestoration, waiting: rt.inflight.n };
    }, mode);
    expect(result).toEqual({ offsets: [40, 0], anchors: [], restoration: 'manual', waiting: 1 });
    await page.clock.runFor(10000);
    expect(await page.evaluate(() => [scrollX, document.getElementById('port')?.scrollLeft ?? scrollX])).toEqual(mode === 'page' ? [40, 40] : [0, 40]);
    expect(await page.evaluate(async () => ({ restoration: history.scrollRestoration,
      waiting: (await import('/rt.js')).inflight.n, pending: (await import('/rt.js')).After.length }))).toEqual({ restoration: 'auto', waiting: 1, pending: 0 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['plain', 'page'])('replacement records retain a clamped anchor until its resource tail arrives (%s)', async mode => {
  const server = replacementRecordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.setViewportSize({ width: 600, height: 200 });
    await page.goto(server.url.href);
    const before = await page.evaluate(async mode => {
      const { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      root.style.minHeight = '0';
      document.body.style.margin = '0';
      const content = tail => `<div data-carry-type="Column" style="height:100px"></div><div data-carry-type="Text" style="height:100px">Anchor</div><div id="tail" style="height:${tail}px"></div>`;
      const frame = tail => mode === 'page' ? content(tail)
        : `<div data-carry-type="Column" id="port" style="height:200px;overflow:auto">${content(tail)}</div>`;
      root.innerHTML = frame(1000);
      prepareDev()();
      const port = mode === 'page' ? document.scrollingElement : document.getElementById('port');
      port.scrollTop = 130;
      const cp = globalThis.__exactDevRecords();
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = frame(0);
      finish();
      const current = mode === 'page' ? document.scrollingElement : document.getElementById('port');
      return { offset: current.scrollTop, restoration: history.scrollRestoration };
    }, mode);
    expect(before).toEqual({ offset: 0, restoration: 'manual' });
    await page.evaluate(async () => {
      document.getElementById('tail').style.height = '1000px';
      const rt = await import('/rt.js');
      rt.inflight.n = 0; rt.settled();
    });
    await page.waitForFunction(() => history.scrollRestoration === 'auto');
    expect(await page.evaluate(mode => (mode === 'page' ? document.scrollingElement : document.getElementById('port')).scrollTop, mode)).toBe(130);
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['rtl', 'row-reverse', 'column-reverse'])('replacement records yield to reader scrolling in signed ranges (%s)', async mode => {
  const server = replacementRecordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.evaluate(async mode => {
      const { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      root.innerHTML = `<div data-carry-type="Column" id="port" style="${mode === 'rtl' ? 'direction:rtl' : `display:flex;flex-direction:${mode}`};height:100px;width:200px;overflow:auto;overflow-anchor:none"><div style="flex-shrink:0;height:1000px;width:900px"></div></div>`;
      prepareDev()();
      const port = document.getElementById('port');
      const vertical = mode === 'column-reverse';
      port.scrollLeft = vertical ? 130 : -130;
      port.scrollTop = vertical ? -130 : 130;
      const cp = globalThis.__exactDevRecords();
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      port.firstElementChild.style[vertical ? 'width' : 'height'] = vertical ? '200px' : '100px';
      port[vertical ? 'scrollTop' : 'scrollLeft'] = 0;
      finish();
    }, mode);
    const vertical = mode === 'column-reverse';
    await page.waitForFunction(vertical => document.getElementById('port')[vertical ? 'scrollTop' : 'scrollLeft'] === -130, vertical);
    expect(await page.evaluate(vertical => ({ offset: document.getElementById('port')[vertical ? 'scrollLeft' : 'scrollTop'],
      restoration: history.scrollRestoration }), vertical)).toEqual({ offset: 0, restoration: 'manual' });
    await page.locator('#port').hover();
    await page.mouse.wheel(vertical ? 0 : -55, vertical ? -55 : 0);
    await page.waitForFunction(vertical => document.getElementById('port')[vertical ? 'scrollTop' : 'scrollLeft'] === -185, vertical);
    await page.evaluate(async vertical => {
      document.getElementById('port').firstElementChild.style[vertical ? 'width' : 'height'] = vertical ? '900px' : '1000px';
      const rt = await import('/rt.js');
      rt.inflight.n = 0; rt.settled();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
    }, vertical);
    expect(await page.locator('#port').evaluate(el => [el.scrollLeft, el.scrollTop])).toEqual(vertical ? [130, -185] : [-185, 130]);
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['wheel', 'touch', 'key', 'prevented', 'letter', 'zero'])('replacement records retain offsets when input scrolls nothing (%s)', async mode => {
  const server = replacementRecordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async mode => {
      const { prepareDev } = await import('/checkpoint.js'), rt = await import('/rt.js');
      const root = document.getElementById('exact-root');
      const content = width => ['reader', 'other'].map(id => `<div data-carry-type="Column" id="${id}" style="width:200px;height:100px;overflow:auto;overflow-anchor:none"><div style="width:${width}px;height:${width === 200 ? 100 : 1000}px"></div></div>`).join('');
      root.innerHTML = content(900);
      prepareDev()();
      for (const el of root.children) { el.scrollLeft = 130; el.scrollTop = 130; }
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(globalThis.__exactDevRecords()); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(200);
      finish();
      const port = document.getElementById('reader'), frame = () => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      if (mode === 'prevented') port.addEventListener('wheel', e => e.preventDefault(), { passive: false });
      const options = { bubbles: true, cancelable: true };
      const event = ['key', 'letter'].includes(mode) ? new KeyboardEvent('keydown', { ...options, key: mode === 'key' ? 'ArrowDown' : 'a' })
        : new WheelEvent('wheel', { ...options, deltaY: mode === 'zero' ? 0 : 55 });
      if (mode === 'touch') {
        const touch = y => new Touch({ identifier: 1, target: port, clientX: 10, clientY: y });
        port.dispatchEvent(new TouchEvent('touchstart', { ...options, touches: [touch(80)] }));
        port.dispatchEvent(new TouchEvent('touchmove', { ...options, touches: [touch(25)] }));
      } else port.dispatchEvent(event);
      await frame();
      const pending = { offsets: [port.scrollLeft, port.scrollTop], waiting: rt.inflight.n, restoration: history.scrollRestoration };
      for (const el of root.children) { el.firstElementChild.style.width = '900px'; el.firstElementChild.style.height = '1000px'; }
      rt.inflight.n = 0; rt.settled();
      await frame();
      return { pending, offsets: [...root.children].map(el => [el.scrollLeft, el.scrollTop]), restoration: history.scrollRestoration };
    }, mode);
    expect(result.pending).toEqual({ offsets: [0, 0], waiting: 1, restoration: 'manual' });
    expect(result.offsets).toEqual([[130, 130], [130, 130]]);
    expect(result.restoration).toBe('auto');
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['empty-auto', 'empty-manual', 'complete', 'cancel', 'expire'])('replacement records restore globals when their phase ends (%s)', async mode => {
  const server = replacementRecordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    if (mode === 'expire') await page.clock.install();
    await page.goto(server.url.href);
    const before = await page.evaluate(async mode => {
      const rt = await import('/rt.js'), af = await import('/focus.js');
      const root = document.getElementById('exact-root');
      history.scrollRestoration = mode === 'empty-manual' ? 'manual' : 'auto';
      const original = HTMLElement.prototype.focus;
      const cp = {
        focus: mode.startsWith('empty') ? null : { path: [2], type: 'Input' },
        scrolls: mode.startsWith('empty') ? [] : [{ path: [0], type: 'Column', axes: {
          left: { target: 0 }, top: { target: 130, anchor: { coordinate: 'y', nodes: [{ path: [0, 0], type: 'Text', y: -30 }] } },
        } }],
      };
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const { prepareDev } = await import('/checkpoint.js'), finish = prepareDev();
      root.innerHTML = '<div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto"><div style="height:1000px"></div></div><input data-carry-type="Input" id="other">';
      af.autofocusProp(document.getElementById('other'), true);
      finish();
      if (HTMLElement.prototype.focus !== original) throw new Error('focus was not released after mounting');
      const restoration = history.scrollRestoration;
      if (mode === 'complete') {
        document.getElementById('port').innerHTML = '<div data-carry-type="Text" style="height:1000px">Anchor</div>';
        root.insertAdjacentHTML('beforeend', '<input data-carry-type="Input" id="notes">');
        rt.inflight.n = 0; rt.settled();
      } else if (mode === 'cancel') {
        document.getElementById('other').focus();
        const port = document.getElementById('port');
        port.dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: 55 }));
        port.scrollTop = 55; port.dispatchEvent(new Event('scroll'));
      }
      return restoration;
    }, mode);
    expect(before).toBe(mode === 'empty-auto' ? 'auto' : 'manual');
    if (mode === 'expire') await page.clock.runFor(10000);
    else await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
    const result = await page.evaluate(async () => {
      const af = await import('/focus.js'), rt = await import('/rt.js');
      document.activeElement?.blur();
      af.mayFocus(); af.autofocus();
      const offered = document.activeElement === document.body;
      const field = document.createElement('input');
      document.getElementById('exact-root').append(field);
      af.autofocusProp(field, true); af.autofocus();
      return { restoration: history.scrollRestoration, offered, autofocus: document.activeElement === field, pending: rt.After.length };
    });
    expect(result).toEqual({ restoration: mode === 'empty-manual' ? 'manual' : 'auto', offered: true, autofocus: true, pending: 0 });
    if (mode === 'complete') {
      await page.evaluate(() => window.scrollTo(0, 500));
      await page.waitForFunction(() => scrollY === 500);
      await page.goto(new URL('away', server.url).href);
      await page.goBack();
      await page.waitForFunction(() => scrollY === 500, null, { timeout: 5000 });
    }
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test('replacement records end when the document leaves during restoration', async () => {
  const server = replacementRecordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.evaluate(async () => {
      window.scrollTo(0, 500);
      const cp = { scrolls: [{ path: [0], type: 'Column', axes: { left: { target: 0 }, top: { target: 130 } } }] };
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const { prepareDev } = await import('/checkpoint.js');
      prepareDev()();
      const rt = await import('/rt.js');
      addEventListener('pagehide', () => sessionStorage.setItem('departed', JSON.stringify({
        restoration: history.scrollRestoration, pending: rt.After.length,
      })), { once: true });
    });
    expect(await page.evaluate(() => ({ restoration: history.scrollRestoration, offset: scrollY }))).toEqual({ restoration: 'manual', offset: 500 });
    await page.goto(new URL('away', server.url).href);
    expect(await page.evaluate(() => JSON.parse(sessionStorage.getItem('departed')))).toEqual({ restoration: 'auto', pending: 0 });
    await page.goBack();
    expect(await page.evaluate(() => history.scrollRestoration)).toBe('auto');
    await page.waitForFunction(() => scrollY === 500, null, { timeout: 5000 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each([
  ['self', 'html', 'scale(2)'], ['ancestor', 'html', 'scale(2)'], ['self', 'svg', 'scale(2)'],
  ['self', 'html', 'scale(0)'], ['ancestor', 'html', 'scale(0)'],
  ['self', 'html', 'scaleY(-1)'], ['self', 'html', 'rotate(180deg)'],
  ['ancestor', 'html', 'scaleY(-1)'], ['ancestor', 'html', 'rotate(180deg)'],
  ['self', 'svg', 'scaleY(-1)'], ['self', 'svg', 'rotate(180deg)'],
  ['anchor', 'svg', 'scale(2)'], ['anchor', 'svg', 'scaleY(-1)'], ['anchor', 'svg', 'rotate(180deg)'],
  ['anchor-origin', 'svg', 'translate(20px,10px) scale(2)'], ['anchor', 'svg-viewbox', 'scale(2)'],
  ['anchor', 'svg', 'scale(0)'], ['anchor', 'svg', 'scaleX(0)'],
  ['ancestor-scale', 'svg', 'scale:2'], ['ancestor-rotate', 'svg', 'rotate:180deg'],
  ...['self', 'ancestor'].flatMap(port => ['html', 'svg'].flatMap(kind => [
    [`${port}-translate`, kind, 'translate:0px 0px 1px'], [`${port}-rotate`, kind, 'rotate:x 30deg'],
  ])),
].map(args => [...args, 'chromium']).concat([
  ['ancestor-rotate', 'svg', 'rotate:180deg', 'firefox'],
]))('replacement records match browser anchoring through a transformed %s with an %s anchor (%s, %s)', async (mode, kind, transform, engine) => {
  const server = replacementRecordsServer();
  let browser;
  try {
    browser = await suiteBrowser(engine);
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async ({ mode, kind, transform }) => {
      const rt = await import('/rt.js'), { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      const content = height => `<div data-carry-type="Column" style="${mode.startsWith('ancestor') ? `${mode === 'ancestor' ? `transform:${transform}` : transform};transform-origin:top left` : ''}">
        <div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto;${mode.startsWith('self') ? `${mode === 'self' ? `transform:${transform}` : transform};transform-origin:top left` : ''}">
          <div data-carry-type="Column" id="above" style="height:${height}px"></div>
          ${kind.startsWith('svg') ? `<svg data-carry-type="Svg" width="200" height="100" ${kind === 'svg-viewbox' ? 'viewBox="0 0 100 50"' : ''} style="display:block;${mode.startsWith('anchor') ? `transform:${transform};transform-origin:${mode === 'anchor-origin' ? '20px 10px' : 'top left'}` : ''}"><rect width="200" height="100"/></svg>` : '<div data-carry-type="Column" style="height:100px">Anchor</div>'}
          <div data-carry-type="Column" style="height:1000px"></div>
        </div></div>`;
      const frame = () => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      root.innerHTML = content(100);
      prepareDev()();
      let port = document.getElementById('port');
      port.scrollTop = 130;
      await frame();
      const cp = globalThis.__exactDevRecords();
      document.getElementById('above').style.height = '160px';
      await frame();
      const ordinary = port.scrollTop;
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(160);
      rt.inflight.n = 0;
      finish();
      port = document.getElementById('port');
      await frame();
      const restored = port.scrollTop;
      document.getElementById('above').style.height = '220px';
      await frame();
      return { ordinary, restored, grown: port.scrollTop, pending: rt.After.length };
    }, { mode, kind, transform });
    expect(result.ordinary).toBe(190);
    expect(result.restored).toBe(result.ordinary);
    expect(result.grown).toBe(250);
    expect(result.pending).toBe(0);
  } finally { await browser?.close(); server.stop(true); }
}, 30000);
