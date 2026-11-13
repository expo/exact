import { test, expect } from 'bun:test';
import { existsSync } from 'node:fs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { glueDeclarations, hostFixtureFile } from './glue-fixture.mjs';

function runtimeServer() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/host/web-js/shared.js') return new Response('export const commit = tail => { globalThis.heldTail = tail; return true; };', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/favicon.ico') return new Response(null, { status: 204 });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/' || path.startsWith('/item/')) return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    const local = new URL('../../..' + path, import.meta.url);
    const file = existsSync(local) ? local : new URL('../' + path.split('/').pop(), import.meta.url);
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
}

for (const launch of ['/', '/#top', '/item/a%7Cb', '/item/a%7Cb?view=one#top']) for (const deferred of [false, true]) test(`a link pop is journaled once, while pushes and Back buttons are quiet (launch: ${launch}, deferred: ${deferred})`, async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  const home = launch.startsWith('/item/') ? launch.split('#')[0].replace('%7C', '|') : '/';
  const detail = launch.startsWith('/item/') ? '/item/c' : '/detail';
  try {
    await page.goto(new URL(launch, server.url).href);
    await page.evaluate(async ({ launch, deferred, home, detail }) => {
      const rt = await import('/host/web-js/rt.js'), { navigation } = await import('/navigation.js');
      rt.clock.agent = true;
      rt.routes([['home', '/', null, false, false], ['detail', '/detail', 0, false, false], ['item', '/item/:id', 0, false, false]]);
      const router = rt.sig(rt.launch(launch));
      rt.mount(root => {
        rt.effect(() => {
          const stack = router()[1][0][1];
          root.innerHTML = `<main navigationBack="back" navigationKey="${stack.at(-1)[0]}">${stack.map(([id, , url]) =>
            `<section navigationKey="${id}"><a href="${url === detail ? home : detail}">${url === detail ? 'Home' : 'Detail'}</a>
              <button id="back">Back</button></section>`).join('')}</main>`;
          root.querySelectorAll('button').forEach(b => b.onclick = () => rt.commit(() => rt.W(router, rt.x_back(router()))));
        });
        rt.router(router, navigation);
      });
      window.fixture = { rt, router, navigation, lines: () => rt.journal.filter(l => l.includes('popped the stack')) };
      if (deferred) { rt.pr(document.createElement('div')); await rt.pieces(); }
    }, { launch, deferred, home, detail });
    const release = () => page.evaluate(() => { const tail = globalThis.heldTail; globalThis.heldTail = null; tail?.(); });
    const click = async text => {
      await page.getByRole('link', { name: text, exact: true }).click();
      await release();
      await page.waitForFunction(() => history.state.id === fixture.router()[1][0][1].at(-1)[0]);
    };
    await click('Detail');
    expect(await page.evaluate(() => fixture.lines())).toEqual([]);
    await page.getByRole('button', { name: 'Back', exact: true }).click();
    await release();
    await page.waitForFunction(launch => history.state.url === launch, launch);
    expect(await page.evaluate(() => fixture.lines())).toEqual([]);
    await click('Detail');
    await click('Home');
    expect(await page.evaluate(() => history.state.url)).toBe(launch);
    const linkURL = await page.evaluate(home => { const url = new URL(home, location.href); return url.pathname + url.search + url.hash; }, home);
    expect(await page.evaluate(() => fixture.lines().map(l => l.replace(/^t=\S+ /, ''))))
      .toEqual([`navigation: link ${JSON.stringify(linkURL)} popped the stack to a retained visit`]);
    await click('Detail');
    await click('Home');
    expect(await page.evaluate(() => fixture.lines())).toHaveLength(1);
  } finally { await browser.close(); server.stop(true); }
});

// Chromium writes a link's `|` as `%7C`; a URL serializer that follows the standard (Bun's) keeps it, and the router's
// visit is still `/item/a%7Cb` (route-pattern.js `canonical`). The line compares the router's form and names the link.
test('a link pop is journaled when the URL serializer leaves the link\'s `|` unescaped', async () => {
  const { navigation } = await import('../navigation/routes.js'), { canonical } = await import('../../web-js/route-pattern.js');
  const names = ['document', 'location', 'history'], had = names.map(n => Object.getOwnPropertyDescriptor(globalThis, n));
  const clicks = [], lines = [];
  try {
    globalThis.document = { addEventListener(type, f) { if (type === 'click') clicks.push(f); } };
    globalThis.location = new URL('http://app.test/item/a%7Cb');
    globalThis.history = { state: null, go() {}, pushState(state) { this.state = state; }, replaceState(state) { this.state = state; } };
    expect(new URL('/item/a|b', location.href).pathname).toBe('/item/a|b');
    const root = { contains: () => true, querySelector: () => null, querySelectorAll: () => [] };
    navigation.connect(root, () => false, line => lines.push(line), null, null, () => true, false, canonical);
    const home = { op: 'router', top: 1, url: '/item/a%7Cb', removed: [], visits: [[1, '/item/a%7Cb']] };
    navigation.apply(home);
    navigation.apply({ op: 'router', top: 2, url: '/item/c', removed: [], visits: [[1, '/item/a%7Cb'], [2, '/item/c']] });
    const link = { href: 'http://app.test/item/a|b#top', target: '', closest: () => link, hasAttribute: () => false };
    for (const capture of clicks) capture({ target: link, button: 0, eventPhase: 1, defaultPrevented: false });
    navigation.apply({ ...home, removed: [2] });
    expect(lines).toEqual(['navigation: link "/item/a|b#top" popped the stack to a retained visit']);
  } finally {
    navigation.reset();
    names.forEach((n, i) => had[i] ? Object.defineProperty(globalThis, n, had[i]) : delete globalThis[n]);
  }
});

test('a wasm batch held for presence retains its link after click dispatch ends', async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  const commit = glueDeclarations(hostFixtureFile(new URL('../glue.js', import.meta.url), 'utf8'), ['heldLinks', 'applyBatch']);
  try {
    await page.goto(server.url.href);
    await page.evaluate(async commit => {
      const { navigation, presenceLoader } = await import('/navigation.js');
      globalThis.exact = {};
      const root = document.getElementById('exact-root'), lines = [], applied = [];
      root.innerHTML = '<a href="/">Home</a>';
      const log = line => lines.push(line), host = {}, agentMode = false;
      const motion = { commit() {} }, arrange = { commit() {} }, clocks = { sync() {} };
      const letGo = () => {}, refitTexts = () => {}, flowBatch = () => {};
      // Exercise the wasm batch's hold and release with only its router op applied.
      const apply = batch => {
        for (const op of batch.ops) if (op.op === 'router') { navigation.apply(op); applied.push(op.top); }
        return batch.timers;
      };
      let ready;
      const loading = new Promise(resolve => { ready = resolve; });
      const presence = presenceLoader(() => loading, root, batch => applyBatch(batch), log);
      const bindings = { navigation, host, agentMode, motion, arrange, clocks, letGo, refitTexts, flowBatch, presence, apply };
      const applyBatch = new Function(...Object.keys(bindings), `${commit}; return applyBatch;`)(...Object.values(bindings));
      navigation.connect(root, () => false, log, null, null, () => true);
      const boot = { op: 'router', top: 1, url: '/', removed: [], visits: [[1, '/']] };
      const detail = { op: 'router', top: 2, url: '/detail', removed: [], visits: [[1, '/'], [2, '/detail']] };
      const home = { ...boot, removed: [2] };
      applyBatch({ ops: [boot] });
      applyBatch({ ops: [detail] });
      let click;
      root.querySelector('a').onclick = event => {
        click = event;
        event.preventDefault();
        applyBatch({ ops: [{ op: 'exit', id: 2 }, home] });
      };
      window.fixture = { applied, lines, navigation,
        phase: () => click.eventPhase,
        later: () => applyBatch({ ops: [detail] }),
        release: async () => { ready(() => ({})); await presence.pending(); },
        back: () => applyBatch({ ops: [home] }) };
    }, commit);
    await page.getByRole('link', { name: 'Home', exact: true }).click();
    expect(await page.evaluate(() => fixture.phase())).toBe(0);
    expect(await page.evaluate(() => fixture.applied)).toEqual([1, 2]);
    expect(await page.evaluate(() => fixture.lines)).toEqual([]);
    // A later batch waits behind the exit without inheriting its link.
    await page.evaluate(() => fixture.later());
    await page.evaluate(() => fixture.release());
    await page.waitForFunction(() => history.state.id === 2);
    expect(await page.evaluate(() => fixture.applied)).toEqual([1, 2, 1, 2]);
    expect(await page.evaluate(() => fixture.lines))
      .toEqual(['navigation: link "/" popped the stack to a retained visit']);
    await page.evaluate(() => fixture.back());
    await page.waitForFunction(() => history.state.id === 1);
    expect(await page.evaluate(() => fixture.lines)).toHaveLength(1);
  } finally { await browser.close(); server.stop(true); }
});

test('duplicate testIds across covered routes are inspected for only 32 projections per root', async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const { navigation } = await import('/navigation.js'), root = document.getElementById('exact-root'), lines = [];
      root.innerHTML = `<main navigationBack="back" navigationKey="2">
        <section navigationKey="1"><p data-testid="repeated">Previous</p><p data-testid="same-route"></p><p data-testid="same-route"></p></section>
        <section navigationKey="2" navigationPresentation="modal"><p data-testid="repeated">Current</p><p data-testid="other"></p></section>
        <section navigationKey="3" data-exiting><p data-testid="other"></p></section>
      </main>`;
      navigation.connect(root, () => false, l => lines.push(l), null, null, () => false, true);
      const query = root.querySelectorAll.bind(root); let scans = 0;
      root.querySelectorAll = selector => { if (selector === '[data-testid]') scans++; return query(selector); };
      // Inspection must neither read nor expire on the wall clock.
      const now = performance.now.bind(performance);
      performance.now = () => { throw new Error('projection read the wall clock'); };
      try {
        for (let i = 0; i < 100; i++) navigation.project(root, l => lines.push(l));
        const beforeActivation = scans;
        navigation.activate(root, l => lines.push(l));
        const first = lines.slice();
        for (let i = 1; i < 32; i++) navigation.project(root, l => lines.push(l));
        root.querySelector('section').insertAdjacentHTML('beforeend', '<p data-testid="late"></p>');
        root.querySelector('section[navigationKey="2"]').insertAdjacentHTML('beforeend', '<p data-testid="late"></p>');
        for (let i = 0; i < 100; i++) navigation.project(root, l => lines.push(l));
        const other = document.createElement('div');
        other.innerHTML = root.innerHTML;
        document.body.append(other);
        navigation.project(other, l => lines.push(l));
        return { first, lines, scans, beforeActivation };
      } finally { performance.now = now; }
    });
    const message = 'navigation: testId "repeated" occurs on multiple routes; covered routes remain mounted';
    expect(result.beforeActivation).toBe(0);
    expect(result.first).toEqual([message]);
    expect(result.scans).toBe(32);
    expect(result.lines).toEqual([message, message, 'navigation: testId "late" occurs on multiple routes; covered routes remain mounted']);
  } finally { await browser.close(); server.stop(true); }
});
