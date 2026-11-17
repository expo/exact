import { beforeAll, afterAll, test as bunTest, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { cdpBrowser, chromium, DESKTOP_INPUT } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';

const { unavailable } = chromium();
const test = need('chrome', unavailable, { test: bunTest });
let server, suite;
beforeAll(async () => {
  if (unavailable) return;
  server = Bun.serve({port:0, hostname:'127.0.0.1', fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/names.js') return new Response('export const sourceTypes = {};', {headers:{'content-type':'text/javascript'}});
    const file = path.startsWith('/web/') ? `..${path}` : existsSync(new URL(`.${path}`, import.meta.url)) ? `.${path}` : `../web${path}`;
    return new Response(path.endsWith('.js') ? readFileSync(new URL(file, import.meta.url)) : '<!doctype html><body><main id="exact-root"></main>', {
      headers:{'content-type':path.endsWith('.js') ? 'text/javascript' : 'text/html'},
    });
  }});
  suite = await cdpBrowser({args:[DESKTOP_INPUT]});
}, 60_000);
afterAll(async () => { await suite?.close(); server?.stop(true); }, 60_000);

for (const grouped of [false, true]) test(`a deferred drop keeps its own result through an unrelated refusal (grouped=${grouped})`, async () => {
  const {sessionId, targetId} = await suite.page();
  const call = (method, params) => suite.cdp.send(method, params, sessionId);
  await call('Page.navigate', {url:`http://127.0.0.1:${server.port}`});
  const evaluated = await call('Runtime.evaluate', {returnByValue:true, awaitPromise:true, expression:`(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js'), {install} = await import('/reorder.js');
    rt.clock.agent = true;
    const grouped = ${grouped};
    const port = document.createElement('div'); port.id = 'arrange';
    if (grouped) port.dataset.reordergroup = 'cards';
    port.style.cssText = 'height:100px;width:320px;overflow:auto';
    const status = document.createElement('div');
    document.getElementById('exact-root').append(port, status);
    const rows = rt.sig(Array.from({length:30}, (_, i) => String(i))), sent = rt.sig(false);
    const reply = Promise.withResolvers();
    let response;
    port.$reorderdrop = rt.act((item, before) => {
      rt.W(sent, true);
      response = reply.promise.then(() => rt.commit(() => {
        const next = rows().filter(k => k !== item);
        next.splice(next.indexOf(before), 0, item);
        rt.W(rows, next);
      }, 'move reply'));
    }, ['s', '?s']);
    const label = text => parent => {
      const el = document.createElement('span'); el.textContent = text;
      el.dataset.sharedElement = 'status'; el.style.setProperty('--exact-layout-transition', '20 0 linear');
      parent.append(el); rt.pr(el);
    };
    const own = rt.scope(() => {
      list.vl(port, rows, i => i(), (w, i) => {
        const row = document.createElement('div'); row.textContent = i(); row.style.height = '20px';
        row.dataset.reorderfor = 'arrange'; row.dataset.view = rt.viewId(row); w.append(row);
      }, {est:20, pw:[], ph:[100]});
      rt.when(status, sent, label('Waiting for move'), label('Ready'));
    });
    rt.settled(); await rt.pieces(); await exact.lists.pending(); await exact.lists.settle();
    const reorder = install(exact.lists, {}, rt.After);
    await new Promise(resolve => { port.addEventListener('scroll', resolve, {once:true}); port.scrollTop = 200; });
    await exact.lists.settle();
    const row = port.querySelector('[data-listitemkey="s:10"]').firstElementChild;
    row.dispatchEvent(new PointerEvent('pointerdown', {bubbles:true, pointerId:1}));
    await exact.lists.settle();
    const binding = reorder.binding(rt.viewId(row)), geometry = reorder.geometry(binding.list);
    const {token} = grouped ? reorder.beginGroup(binding, geometry, true) : reorder.begin(binding, geometry);
    reorder.preview(token, geometry, 300);
    const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
    let transition;
    document.startViewTransition = update => transition = start(async () => { await gate.promise; return update(); });
    const snapshot = () => ({pending:!!port.$list.pendingDrops?.size, phase:reorder.session()?.phase ?? null});
    const accepted = grouped ? rt.commit(() => reorder.dropGroup(token, geometry, 0)) : reorder.drop(token, geometry);
    const deferred = !!transition, before = snapshot();
    const refused = rt.commit(() => { throw new rt.Refusal('unrelated action'); });
    const during = snapshot();
    gate.resolve();
    if (transition) { await transition.updateCallbackDone; await transition.finished; }
    document.startViewTransition = start;
    const after = snapshot();
    reply.resolve(); await response;
    const offset = port.$list.geometry.offset;
    reorder.reconcileGroup(); reorder.finish(token);
    row.dispatchEvent(new PointerEvent('pointerup', {bubbles:true, pointerId:1}));
    rt.end(own); port.remove(); status.remove(); rt.settled();
    return {accepted, refused, deferred, before, during, after, offset};
  })()`});
  await suite.cdp.send('Target.closeTarget', {targetId});
  if (evaluated.exceptionDetails) throw Error(evaluated.exceptionDetails.exception?.description ?? evaluated.exceptionDetails.text);
  const held = {pending:true, phase:grouped ? 'holding' : null};
  expect(evaluated.result.value).toEqual({accepted:true, refused:false, deferred:true, before:held, during:held, after:held, offset:180});
}, 60_000);

for (const [scenario, delay] of [
  ['late', 600], ['late', 999], ['late', 1000], ['late', 1001],
  ['cancel', 600], ['cancel_finished', 600], ['refused', 600], ['same_slot', 600],
  ['deleted', 600], ['deleted_target', 600],
  ['next_preview', 700], ['next_cancel', 700], ['next_group_cancel', 700],
  ['next_preview', 1000], ['next_cancel', 1000], ['next_group_cancel', 1000],
  ...['next_same_slot', 'next_group_same_slot', 'next_drop', 'next_drop_cancel', 'next_refused', 'next_group_refused', 'next_completed', 'next_group_completed']
    .flatMap(scenario => [[scenario, 700], [scenario, 1000]]),
]) test(`a late drop excludes its row only while eligible (${scenario}, delay=${delay})`, async () => {
  const {sessionId, targetId} = await suite.page();
  const call = (method, params) => suite.cdp.send(method, params, sessionId);
  await call('Page.navigate', {url:`http://127.0.0.1:${server.port}`});
  const evaluated = await call('Runtime.evaluate', {returnByValue:true, awaitPromise:true, expression:`(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js'), {install} = await import('/reorder.js');
    rt.clock.agent = true;
    const scenario = ${JSON.stringify(scenario)};
    const port = document.createElement('div'); port.id = 'arrange';
    port.style.cssText = 'height:100px;width:320px;overflow:auto';
    if (scenario.startsWith('next_group_')) port.dataset.reordergroup = 'cards';
    document.getElementById('exact-root').append(port);
    const rows = rt.sig(Array.from({length:30}, (_, i) => String(i)));
    const reply = Promise.withResolvers();
    let response;
    port.$reorderdrop = rt.act((item, before) => {
      if (item === '11') {
        if (scenario.endsWith('refused')) throw new rt.Refusal('second drop refused');
        if (scenario.endsWith('completed')) {
          const next = rows().filter(k => k !== item);
          next.splice(next.indexOf(before), 0, item);
          rt.W(rows, next);
        }
        return;
      }
      response = reply.promise.then(() => rt.commit(() => {
        const next = rows().filter(k => k !== item);
        next.splice(next.indexOf('15'), 0, item);
        rt.W(rows, next);
      }, 'move reply'));
      if (scenario === 'refused') throw new rt.Refusal('drop refused');
    }, ['s', '?s']);
    const own = rt.scope(() => list.vl(port, rows, i => i(), (w, i) => {
      const row = document.createElement('div'); row.textContent = i(); row.style.height = '20px';
      row.dataset.reorderfor = 'arrange'; row.dataset.view = rt.viewId(row); w.append(row);
    }, {est:20, pw:[], ph:[100]}));
    rt.settled(); await rt.pieces(); await exact.lists.pending(); await exact.lists.settle();
    const reorder = install(exact.lists, {}, rt.After);
    await new Promise(resolve => { port.addEventListener('scroll', resolve, {once:true}); port.scrollTop = 207; });
    await exact.lists.settle();
    const row = port.querySelector('[data-listitemkey="s:10"]').firstElementChild;
    const successorTop = () => port.querySelector('[data-listitemkey="s:' + (scenario.endsWith('completed') ? '12' : '11') + '"]').getBoundingClientRect().top - port.getBoundingClientRect().top;
    const before = successorTop();
    row.dispatchEvent(new PointerEvent('pointerdown', {bubbles:true, pointerId:1}));
    await exact.lists.settle();
    const binding = reorder.binding(rt.viewId(row)), geometry = reorder.geometry(binding.list);
    const {token} = reorder.begin(binding, geometry);
    reorder.preview(token, geometry, scenario === 'same_slot' ? 207 : 300);
    const accepted = reorder.drop(token, geometry, 3000);
    const deadline = port.$list.pendingDrops?.get(token)?.deadline ?? null;
    if (scenario === 'cancel') reorder.cancel(token);
    rt.advance(3600);
    reorder.finish(token);
    const finished = reorder.frame(token) === null;
    if (scenario === 'cancel_finished') reorder.cancel(token);
    if (scenario.startsWith('next_')) {
      rt.advance(3650);
      row.dispatchEvent(new PointerEvent('pointerup', {bubbles:true, pointerId:1}));
      const nextRow = port.querySelector('[data-listitemkey="s:11"]').firstElementChild;
      nextRow.dispatchEvent(new PointerEvent('pointerdown', {bubbles:true, pointerId:2}));
      await exact.lists.settle();
      const binding = reorder.binding(rt.viewId(nextRow)), geometry = reorder.geometry(binding.list);
      const grouped = scenario.startsWith('next_group_');
      const next = grouped ? reorder.beginGroup(binding, geometry, false) : reorder.begin(binding, geometry);
      if (!['next_preview', 'next_cancel', 'next_group_cancel'].includes(scenario)) {
        reorder.preview(next.token, geometry, scenario.endsWith('same_slot') ? 220 : 300);
        if (grouped) rt.commit(() => reorder.dropGroup(next.token, geometry, 3650));
        else reorder.drop(next.token, geometry, 3650);
        reorder.reconcileGroup();
      }
      if (scenario.endsWith('cancel')) reorder.cancel(next.token);
      if (scenario !== 'next_preview') reorder.finish(next.token);
      nextRow.dispatchEvent(new PointerEvent('pointerup', {bubbles:true, pointerId:2}));
    }
    if (scenario === 'deleted' || scenario === 'deleted_target') {
      const saved = rows();
      const removed = scenario === 'deleted' ? '10' : '15';
      rt.commit(() => rt.W(rows, saved.filter(k => k !== removed)), 'delete row');
      rt.commit(() => rt.W(rows, saved), 'restore row');
      await exact.lists.settle();
      if (port.scrollTop !== 207) await new Promise(resolve => {
        port.addEventListener('scroll', resolve, {once:true}); port.scrollTop = 207;
      });
      await exact.lists.settle();
    }
    rt.commit(() => {
      const next = [...rows()]; [next[0], next[1]] = [next[1], next[0]];
      rt.W(rows, next);
    }, 'unrelated rows');
    const intervening = port.$list.geometry.offset;
    const pending = port.$list.pendingDrops?.has(token) ?? false;
    rt.advance(3000 + ${delay});
    reply.resolve(); await response;
    await exact.lists.settle();
    const offset = port.$list.geometry.offset, after = successorTop(), consumed = !port.$list.pendingDrops?.has(token);
    const remaining = [...port.$list.pendingDrops?.values() ?? []].map(p => p.deadline);
    let secondOffset = null;
    if (scenario === 'next_drop') {
      await new Promise(resolve => { port.addEventListener('scroll', resolve, {once:true}); port.scrollTop = 207; });
      await exact.lists.settle();
      rt.advance(${delay} === 700 ? 4100 : 4650);
      rt.commit(() => {
        const next = rows().filter(k => k !== '11');
        next.splice(next.indexOf('15'), 0, '11');
        rt.W(rows, next);
      }, 'second move reply');
      await exact.lists.settle();
      secondOffset = port.$list.geometry.offset;
    }
    row.dispatchEvent(new PointerEvent('pointerup', {bubbles:true, pointerId:1}));
    rt.end(own); port.remove(); rt.settled();
    return {accepted, finished, offset, before, after, intervening, pending, deadline, consumed, remaining, secondOffset};
  })()`});
  await suite.cdp.send('Target.closeTarget', {targetId});
  if (evaluated.exceptionDetails) throw Error(evaluated.exceptionDetails.exception?.description ?? evaluated.exceptionDetails.text);
  const pending = scenario === 'late' || scenario.startsWith('next_'), excluded = pending && delay < 1000;
  expect(evaluated.result.value).toEqual({accepted:true, finished:true, offset:excluded ? 187 : 287,
    before:scenario.endsWith('completed') ? 33 : 13, after:excluded ? 13 : -87, intervening:207, pending,
    deadline:['same_slot', 'refused'].includes(scenario) ? null : 4000, consumed:true, remaining:scenario === 'next_drop' ? [4650] : [],
    secondOffset:scenario === 'next_drop' ? excluded ? 187 : 287 : null});
}, 60_000);

test('delayed drops landing in one update anchor a row neither moved', async () => {
  const {sessionId, targetId} = await suite.page();
  const call = (method, params) => suite.cdp.send(method, params, sessionId);
  await call('Page.navigate', {url:`http://127.0.0.1:${server.port}`});
  const evaluated = await call('Runtime.evaluate', {returnByValue:true, awaitPromise:true, expression:`(async () => {
    const rt = await import('/rt.js'), list = await import('/list.js'), {install} = await import('/reorder.js');
    rt.clock.agent = true;
    const port = document.createElement('div'); port.id = 'arrange';
    port.style.cssText = 'height:100px;width:320px;overflow:auto';
    document.getElementById('exact-root').append(port);
    const rows = rt.sig(Array.from({length:30}, (_, i) => String(i))), drops = [];
    port.$reorderdrop = rt.act((item, before) => { drops.push([item, before]); }, ['s', '?s']);
    const own = rt.scope(() => list.vl(port, rows, i => i(), (w, i) => {
      const row = document.createElement('div'); row.textContent = i(); row.style.height = '20px';
      row.dataset.reorderfor = 'arrange'; row.dataset.view = rt.viewId(row); w.append(row);
    }, {est:20, pw:[], ph:[100]}));
    rt.settled(); await rt.pieces(); await exact.lists.pending(); await exact.lists.settle();
    const reorder = install(exact.lists, {}, rt.After);
    await new Promise(resolve => { port.addEventListener('scroll', resolve, {once:true}); port.scrollTop = 207; });
    await exact.lists.settle();
    const top = key => port.querySelector('[data-listitemkey="s:' + key + '"]').getBoundingClientRect().top - port.getBoundingClientRect().top;
    const before = top(12);
    const drag = async (key, pointerId, y, at, finishAt) => {
      const row = port.querySelector('[data-listitemkey="s:' + key + '"]').firstElementChild;
      row.dispatchEvent(new PointerEvent('pointerdown', {bubbles:true, pointerId}));
      await exact.lists.settle();
      const binding = reorder.binding(rt.viewId(row)), geometry = reorder.geometry(binding.list);
      const {token} = reorder.begin(binding, geometry);
      reorder.preview(token, geometry, y);
      reorder.drop(token, geometry, at);
      rt.advance(finishAt);
      reorder.finish(token);
      row.dispatchEvent(new PointerEvent('pointerup', {bubbles:true, pointerId}));
    };
    await drag('10', 1, 300, 3000, 3600);
    await drag('11', 2, 280, 3650, 3650);
    rt.advance(3700);
    rt.commit(() => {
      let next = rows();
      for (const [item, before] of drops) { next = next.filter(k => k !== item); next.splice(next.indexOf(before), 0, item); }
      rt.W(rows, next);
    }, 'both moves');
    await exact.lists.settle();
    const result = {drops, order:rows().slice(10, 15), offset:port.$list.geometry.offset, before, after:top(12)};
    rt.end(own); port.remove(); rt.settled();
    return result;
  })()`});
  await suite.cdp.send('Target.closeTarget', {targetId});
  if (evaluated.exceptionDetails) throw Error(evaluated.exceptionDetails.exception?.description ?? evaluated.exceptionDetails.text);
  // Row 12 keeps its place in the port.
  expect(evaluated.result.value).toEqual({drops:[['10', '15'], ['11', '14']], order:['12', '13', '11', '14', '10'], offset:167, before:33, after:33});
}, 60_000);
