import { test, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { browserMissing } from '../../../scripts/agent-playwright.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { glueCase, hostFixtureFile } from './glue-fixture.mjs';

const root = resolve(import.meta.dir, '../../..');
const wasmState = glueCase(readFileSync(resolve(root, 'host/web/wasm/agent.js'), 'utf8'), 'state');
const check = need('chrome', await browserMissing('chromium'));
const record = { value: '😀 @alice', spans: [
  { name: 'mention', start: 3, end: 9 }, { name: 'unknown', start: 0, end: 2 },
] };
const styles = { mention: 'color:rgb(16,131,254)' };

async function page(drive, unavailable = false) {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path.endsWith('/names.js')) return new Response('export default [[],[],[]]; export const types = [[],[],[]], sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/') return new Response('<div id="exact-root"><textarea id="field"></textarea></div>', { headers: { 'content-type': 'text/html' } });
    const local = resolve(root, path.slice(1)), file = existsSync(local) ? local : resolve(root, 'host/web', path.split('/').pop());
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
  // Older Chromium builds still gate OpaqueRange behind this flag.
  const browser = await suiteBrowser('chromium', { args: ['--enable-experimental-web-platform-features'] }), p = await browser.newPage(), errors = [];
  p.on('pageerror', error => errors.push(error.message));
  try {
    await p.goto(server.url.href);
    await p.evaluate(async ({ record, styles, unavailable, wasmState }) => {
      if (unavailable) Object.defineProperty(HTMLTextAreaElement.prototype, 'createValueRange', { value: undefined });
      globalThis.exact = {};
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/agent.js');
      const field = document.getElementById('field');
      field.value = record.value;
      field.setAttribute('data-decorations', JSON.stringify(record));
      field.setAttribute('data-decoration-styles', JSON.stringify(styles));
      rt.clock.agent = true;
      Object.assign(exact, { clock: rt.clock, After: rt.After, views: rt.Views, viewId: rt.viewId,
        resources: rt.Resources, mutations: rt.Mutations, state: [[], [], []] });
      install(exact);
      // Run the wasm agent's state bridge over the same DOM; its runner has no app state here.
      window.wasmState = new Function('request', `const views = exact.views, ask = () => ({}),
        presence = {}, forgettable = new Map(), host = {}, navigation = { observation: () => ({}) },
        root = document.getElementById('exact-root');
        switch (request.op) { ${wasmState} }`);
      window.installDecorations = async () => {
        const { install } = await import('/host/web/decorations.js');
        window.logs = [];
        install(field, line => logs.push(line));
      };
    }, { record, styles, unavailable, wasmState });
    await drive(p);
    expect(errors).toEqual([]);
  } finally { await p.close(); await browser.close(); server.stop(true); }
}

check('the JS tree keeps the authored decoration record through edits and removal', async () => {
  await page(async p => {
    const prop = () => p.evaluate(async () => (await exact.agentSettled({ op: 'tree' })).nodes[0].props.decorations);
    expect(await prop()).toBe(JSON.stringify(record));
    await p.locator('#field').fill('new text');
    expect(JSON.parse(await prop())).toEqual(record);
    await p.evaluate(() => document.getElementById('field').removeAttribute('data-decorations'));
    expect(await prop()).toBeUndefined();
  });
}, 30000);

check('JS decoration state finds unfocused fields before tree inspection and after mounting', async () => {
  await page(async p => {
    const state = () => p.evaluate(async () => (await exact.agentSettled({ op: 'state' })).decorations);
    await p.evaluate(() => installDecorations());
    expect(await p.evaluate(() => exact.views.size)).toBe(0);
    const first = await state();
    expect(first).toHaveLength(1);
    expect(first[0].applied).toEqual({ source: record.value, recordValue: record.value,
      provisional: false, spans: [record.spans[0]] });
    await p.evaluate(() => {
      const field = document.getElementById('field'), next = field.cloneNode();
      next.id = 'next-field';
      next.value = field.value;
      document.getElementById('exact-root').append(next);
      exact.installDecorations(next);
    });
    const mounted = await state();
    expect(mounted).toHaveLength(2);
    expect(mounted[0]).toEqual(first[0]);
    expect(mounted[1].applied).toEqual(first[0].applied);
    const tree = await p.evaluate(() => exact.agentSettled({ op: 'tree' }));
    expect(mounted.map(field => field.id)).toEqual(tree.nodes.map(node => node.id));
    await p.evaluate(() => document.getElementById('next-field').remove());
    expect(await state()).toEqual(first);
  });
}, 30000);

check('JS decoration state waits for the first field to load its decoration module', async () => {
  await page(async p => {
    const requested = Promise.withResolvers(), release = Promise.withResolvers();
    await p.route('**/host/web-js/decorations.js', async route => {
      requested.resolve();
      await release.promise;
      await route.continue();
    });
    try {
      await p.evaluate(async () => {
        const rt = await import('/host/web-js/rt.js');
        rt.decorate(document.getElementById('field'));
        window.firstDecorationState = exact.agentSettled({ op: 'state' });
      });
      await requested.promise;
      expect(await p.evaluate(() => !!document.getElementById('field').exactDecorations)).toBe(false);
      release.resolve();
      const state = await p.evaluate(async () => (await firstDecorationState).decorations);
      expect(state).toHaveLength(1);
      expect(state[0].applied).toEqual({ source: record.value, recordValue: record.value,
        provisional: false, spans: [record.spans[0]] });
    } finally { release.resolve(); }
  });
}, 30000);

for (const unavailable of [false, true]) check(`both web agents inspect decorations with value ranges ${unavailable ? 'unavailable' : 'available'}`, async () => {
  await page(async p => {
    const states = () => p.evaluate(async () => [
      (await exact.agentSettled({ op: 'state' })).decorations, wasmState({ op: 'state' }).decorations,
    ]);
    expect(await states()).toEqual([[], []]);
    await p.evaluate(() => installDecorations());
    const id = await p.evaluate(() => exact.viewId(document.getElementById('field')));
    const painted = !unavailable;
    const expected = applied => [{ id, painted, applied }];
    const initial = { source: record.value, recordValue: record.value, provisional: false, spans: [record.spans[0]] };
    expect(await states()).toEqual([expected(initial), expected(initial)]);
    expect(await p.evaluate(() => CSS.highlights.size)).toBe(painted ? 1 : 0);
    expect(await p.evaluate(() => logs)).toEqual(['decorations: no decoration "unknown"']);
    await p.locator('#field').focus();
    await p.evaluate(() => document.getElementById('field').setSelectionRange(0, 0));
    await p.keyboard.type('!');
    const edited = { source: '!' + record.value, recordValue: record.value, provisional: true,
      spans: [{ name: 'mention', start: 4, end: 10 }] };
    expect(await states()).toEqual([expected(edited), expected(edited)]);
    const next = { value: 'composition', spans: [{ name: 'mention', start: 0, end: 4 }] };
    await p.evaluate(next => {
      const el = document.getElementById('field');
      el.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
      el.value = next.value;
      el.dispatchEvent(new InputEvent('input', { bubbles: true, isComposing: true }));
      el.setAttribute('data-decorations', JSON.stringify(next));
    }, next);
    expect(await states()).toEqual([expected(edited), expected(edited)]);
    await p.evaluate(() => document.getElementById('field').dispatchEvent(new CompositionEvent('compositionend', { bubbles: true })));
    const committed = { source: next.value, recordValue: next.value, provisional: false, spans: next.spans };
    expect(await states()).toEqual([expected(committed), expected(committed)]);
    await p.evaluate(() => document.getElementById('field').setAttribute('data-decoration-styles', '{}'));
    const unstyled = { ...committed, spans: [] };
    expect(await states()).toEqual([expected(unstyled), expected(unstyled)]);
    expect(await p.evaluate(() => logs)).toEqual(['decorations: no decoration "unknown"', 'decorations: no decoration "mention"']);
    await p.evaluate(() => document.getElementById('field').removeAttribute('data-decorations'));
    expect(await states()).toEqual([[], []]);
    await p.evaluate(({ record, styles }) => {
      const el = document.getElementById('field');
      el.setAttribute('data-decorations', JSON.stringify(record));
      el.setAttribute('data-decoration-styles', JSON.stringify(styles));
    }, { record, styles });
    expect((await states())[0]).toHaveLength(1);
    await p.evaluate(() => { window.detachedField = document.getElementById('field'); detachedField.remove(); });
    expect(await states()).toEqual([[], []]);
    await p.evaluate(() => {
      const el = detachedField;
      document.getElementById('exact-root').append(el);
      exact.viewId(el);
    });
    expect((await states()).map(fields => fields.length)).toEqual([1, 1]);
    await p.evaluate(() => detachedField.exactDecorations.destroy());
    expect(await states()).toEqual([[], []]);
  }, unavailable);
}, 30000);
