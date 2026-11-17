import { test, expect } from 'bun:test';
import { existsSync } from 'node:fs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

function serve() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') return new Response(null, { status: 204 });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes={};', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    const local = new URL('../..' + path, import.meta.url), file = existsSync(local) ? local : new URL('../web/' + path.split('/').pop(), import.meta.url);
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
}

test('change tasks read committed DOM, coalesce keys and cancel or roll back pending work at fixed time', async () => {
  const server = serve(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href + '?agent');
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const input = rt.sig(10), enabled = rt.sig(true), count = rt.sig(0), seen = [], commands = [];
      let box;
      rt.Hosts.mark = value => commands.push([value, box.getBoundingClientRect().width]);
      rt.gated(0, () => rt.commit(() => {
        seen.push([input(), box.getBoundingClientRect().width, rt.clock.now]);
        rt.W(count, count() + 1); rt.C('mark', [input()]);
      }), true, false, enabled, () => 1 / input(), 'sync', 0, 0, 0, true);
      rt.mount(root => {
        box = document.createElement('div'); root.append(box);
        rt.effect(() => { box.style.width = input() + 'px'; });
      });
      const before = seen.length;
      rt.advance(0, false, undefined, false);
      rt.commit(() => rt.W(input, 20));
      rt.commit(() => rt.W(input, 30));
      const coalesced = seen.length;
      rt.advance(0, false, undefined, false);
      rt.commit(() => rt.W(input, 40));
      rt.commit(() => rt.W(enabled, false));
      rt.advance(0, false, undefined, false);
      const cancelled = seen.length;
      rt.commit(() => rt.W(enabled, true));
      const refused = rt.commit(() => rt.W(input, 0));
      rt.advance(0, false, undefined, false);
      rt.advance(0, false, undefined, false);
      return { before, coalesced, cancelled, refused, seen, commands, count: count(), now: rt.clock.now, pending: rt.clock.timers.length };
    });
    expect(result).toEqual({ before: 0, coalesced: 1, cancelled: 2, refused: false,
      seen: [[10, 10, 0], [30, 30, 0], [40, 40, 0]], commands: [[10, 10], [30, 30], [40, 40]], count: 3, now: 0, pending: 0 });
  } finally { await page.close(); await browser.close(); server.stop(true); }
}, 60_000);

test('a live change action is queued after mount and reads the applied DOM', async () => {
  const server = serve(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const input = rt.sig(7), seen = globalThis.changeSeen = [];
      let box;
      rt.mount(root => { box = document.createElement('p'); root.append(box); rt.effect(() => { box.textContent = String(input()); }); });
      rt.gated(0, () => rt.commit(() => seen.push(box.textContent)), true, false, () => true, input, 'sync', 0, 0, 0, true);
      globalThis.changeSet = () => { rt.commit(() => rt.W(input, 8)); rt.commit(() => rt.W(input, 9)); };
      globalThis.changeAtMount = seen.length;
    });
    expect(await page.evaluate(() => changeAtMount)).toBe(0);
    await page.waitForFunction(() => changeSeen.length === 1);
    await page.evaluate(() => changeSet());
    await page.waitForFunction(() => changeSeen.length === 2);
    expect(await page.evaluate(() => changeSeen)).toEqual(['7', '9']);
  } finally { await page.close(); await browser.close(); server.stop(true); }
}, 60_000);

test('a change waits for deferred view-transition tails and their host commands', async () => {
  const server = serve(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href + '?agent');
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const key = rt.sig(0), side = rt.sig(false), seen = [], commands = [];
      let panel;
      const label = text => parent => {
        const el = document.createElement('div'); el.textContent = text; el.dataset.sharedElement = 'panel';
        el.style.setProperty('--exact-layout-transition', '20 0 linear'); parent.append(el); rt.pr(el);
      };
      rt.Hosts.mark = value => commands.push(value);
      rt.gated(0, () => rt.commit(() => seen.push([key(), panel.textContent, commands.slice(), rt.clock.now])), true, false, () => true, key, 'sync', 0, 0, 0, true);
      rt.mount(root => { panel = document.createElement('div'); root.append(panel); rt.when(panel, side, label('new'), label('old')); });
      await rt.pieces();
      rt.advance(0, false, undefined, false);
      const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
      let transition;
      document.startViewTransition = update => transition = start(async () => { await gate.promise; return update(); });
      rt.commit(() => { rt.W(side, true); rt.W(key, 1); rt.C('mark', [1]); });
      rt.commit(() => { rt.W(key, 2); rt.C('mark', [2]); });
      const deferred = !!transition;
      // A history traversal the browser animated applies synchronously while the earlier tail waits.
      rt.useTraversal(() => true); rt.commit(() => {}); rt.useTraversal(null);
      rt.advance(20, false, undefined, false);
      const waiting = seen.slice();
      gate.resolve();
      if (transition) { await transition.updateCallbackDone; await transition.finished; }
      rt.advance(20, false, undefined, false);
      return { deferred, waiting, seen, now: rt.clock.now };
    });
    expect(result).toEqual({ deferred: true, waiting: [[0, 'old', [], 0]], seen: [[0, 'old', [], 0], [2, 'new', [1, 2], 20]], now: 20 });
  } finally { await page.close(); await browser.close(); server.stop(true); }
}, 60_000);

test('an input runs after the change action and the timers due before it', async () => {
  const server = serve(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href + '?agent');
    const order = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const { on } = await import('/host/web-js/events.js');
      const { every } = await import('/host/web-js/clock.js');
      const key = rt.sig(0), order = [];
      every(0, () => rt.commit(() => order.push('timer')), true, 'timer');
      rt.gated(0, () => rt.commit(() => order.push('change')), true, false, () => true, key, 'sync', 0, 0, 0, true);
      every(0, () => rt.commit(() => order.push('after')), true, 'after');
      rt.mount(() => {});
      const button = document.createElement('button');
      document.body.append(button);
      on(button, 'click', () => order.push('input'));
      button.click();
      return order;
    });
    expect(order).toEqual(['timer', 'change', 'after', 'input']);
  } finally { await page.close(); await browser.close(); server.stop(true); }
}, 60_000);
