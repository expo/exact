import { test, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

function runtimeServer(overrides = {}) {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (Object.hasOwn(overrides, path)) { return new Response(overrides[path], { headers: { 'content-type': 'text/javascript' } }); }
    if (path === '/favicon.ico') { return new Response(null, { status: 204 }); }
    if (path.endsWith('/names.js')) { return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } }); }
    if (path === '/') { return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } }); }
    const local = new URL('../..' + path, import.meta.url), file = existsSync(local) ? local : new URL('../web/' + path.split('/').pop(), import.meta.url);
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
}

// A tool whose action's commit is refused ends `failed` `refused-commit` (the runner's `modules/calls.rs`), never
// `ok` over the state the refusal put back, through the agent's door and through WebMCP's.
test('a tool whose commit is refused does not report it applied', async () => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const registered = {};
      document.modelContext = { registerTool: tool => { registered[tool.name] = tool; return Promise.resolve(); } };
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      const n = rt.sig(0, 'n');
      rt.clock.agent = true;
      rt.mount(() => {});
      const face = { digest: 'd', doors: ['web'] }, row = action => [() => action, {}, 'writes', [false, false, true], 'A tool.', null,
        () => n(), null, face, 'n'];
      tools({
        add: row(rt.act(() => rt.W(n, n() + 1))),
        mistyped: row(rt.act(() => rt.W(n, 'one'))),
        throws: row(rt.act(() => { rt.W(n, 5); throw new rt.Refusal('a refusal in the body'); })),
      });
      for (let i = 0; i < 8; i++) await Promise.resolve();
      const agent = async tool => {
        const { call } = globalThis.exact.calls.call({ tool });
        for (let i = 0; i < 64 && globalThis.exact.calls.called(call).outcome == null; i++) await Promise.resolve();
        const { outcome, certainty, reason, value } = globalThis.exact.calls.called(call);
        return [outcome, certainty, reason, value, n()];
      };
      const web = async tool => {
        const { outcome, certainty, reason, value } = await registered[tool].execute({});
        return [outcome, certainty, reason, value, n()];
      };
      return [await agent('add'), await agent('mistyped'), await agent('throws'), await web('mistyped'), await web('throws'),
        await web('add')];
    });
    expect(result).toEqual([
      ['ok', 'applied', '', 1, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['ok', 'applied', '', 2, 2],
    ]);
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
}, 60000);

test('a replacing commit publishes supersession after its deferred view update', async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const side = rt.sig(false), slot = rt.sig('seed'), replies = [], output = {};
      let mutation;
      rt.data.answer = () => ({ promise: new Promise(resolve => replies.push(resolve)) });
      const panel = text => parent => {
        const el = document.createElement('div'); el.textContent = text; el.dataset.sharedElement = 'panel';
        el.style.setProperty('--exact-layout-transition', '20 0 linear'); parent.append(el); rt.pr(el);
      };
      rt.mount(root => {
        mutation = rt.mut('save', slot, [], 's');
        rt.when(root, side, panel('new'), panel('old'));
      });
      await rt.pieces();
      tools({ save: [() => rt.act(() => rt.M(mutation, 'save', [])), {}, 'writes', [false, false, true], 'Save.', null, () => slot()] });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const state = id => { const c = exact.calls.called(id); return c.outcome ? [c.outcome, c.certainty] : c.state; };
      const { call } = exact.calls.call({ tool: 'save' }); await flush();
      output.started = state(call);
      const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
      let transition;
      document.startViewTransition = update => transition = start(async () => { await gate.promise; return update(); });
      rt.commit(() => { rt.M(mutation, 'save', []); rt.W(side, true); });
      output.deferred = !!transition;
      for (const f of rt.Clocked) f(); await flush();
      output.during = state(call);
      gate.resolve(); await transition.updateCallbackDone; await transition.finished;
      output.after = state(call);
      for (const reply of replies) reply('done'); await flush();
      return output;
    });
    expect(result).toEqual({ started: 'running', deferred: true, during: 'running', after: ['superseded', 'uncertain'] });
  } finally { await browser.close(); server.stop(true); }
}, 60000);

test('a tool can report resource status during its first pending answer', async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      const { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const replies = [];
      rt.data.answer = () => ({ promise: new Promise(resolve => replies.push(resolve)) });
      let read, family;
      rt.mount(() => {
        read = rt.res('read', 'read', () => [], undefined, undefined, 's', 'placeholder');
        family = resEach('family', 'read', () => [1], x => x, x => [x], '[s', 's', 'placeholder');
      });
      const rows = {};
      for (const [name, resource] of [['read', read], ['family', family]]) {
        for (const status of ['p', 'f', 'e', 'c']) {
          const derived = rt.memo(() => resource[status]());
          rows[name + status] = [() => rt.act(() => {}), {}, 'reads', [false, false, true], 'Status.', null, () => derived()];
        }
      }
      tools(rows);
      const results = [];
      for (const tool of Object.keys(rows)) {
        const { call } = exact.calls.call({ tool });
        for (let i = 0; i < 32; i++) await Promise.resolve();
        const c = exact.calls.called(call); results.push([c.outcome, c.certainty, c.value]);
      }
      for (const reply of replies) reply('done');
      return results;
    });
    expect(result).toEqual([true, false, null, false, true, false, null, false].map(value => ['ok', 'applied', value]));
  } finally { await browser.close(); server.stop(true); }
}, 60000);

test('topic re-asks keep their last ask origin without joining its invocation', async () => {
  const source = readFileSync(new URL('./ts-data.js', import.meta.url), 'utf8')
    .replace('__APP_TS__', './tool-source.js').replace('__FETCH_MODULE__', './ts-fetch.js')
    .replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '');
  const server = runtimeServer({ '/host/web-js/ts-data.js': source, '/host/web-js/tool-source.js': 'export {};',
    '/host/web-js/admission-data.js': 'export const tsGrantSet = {};' });
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { modules } = await import('/host/web-js/modules.js');
      const { tools } = await import('/host/web-js/calls.js');
      await import('/host/web-js/ts-data.js');
      rt.clock.agent = true;
      modules({ guard: ['guard.read', false, { origin: 'app', default: 'allowed' }, 's'] });
      const requests = [], replies = [], guarded = [], parses = [], output = {};
      let count, wait;
      rt.data.answer = source => {
        if (source === 'count') {
          Promise.resolve(rt.data.reserved.guard('guard', [], 'guard')).then(v => guarded.push(v), e => guarded.push(e.message));
        }
        return { req: { method: 'GET', url: source, headers: [] }, watches: new Set(['tick']) };
      };
      rt.data.fetch = req => new Promise(resolve => { requests.push([req.url, req.origin]); replies.push(resolve); });
      rt.data.parse = (source, _args, o) => {
        parses.push([source, rt.Origin.now(), rt.Invocation.id]);
        return { v: o.value };
      };
      rt.mount(() => {
        count = rt.res('count', 'count', () => [], 'seed', [], 's', '', true);
        wait = rt.mut('wait', rt.sig('seed'), [], 's');
      });
      tools({ bump: [() => rt.act(() => { rt.R(count); rt.M(wait, 'wait', []); }), {}, 'writes', [false, false, true], 'Bump.'] });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const { call } = exact.calls.call({ tool: 'bump' }); await flush();
      replies[1]({ value: 'first count' }); await flush();
      output.before = exact.calls.called(call).state;
      exact.announceModule('tick'); await flush();
      output.owner = count.r.ticket.invocation;
      // A topic announced during this re-ask waits for its reply, then asks without an invocation again.
      exact.announceModule('tick'); await flush();
      replies[0]({ value: 'waited' }); await flush();
      output.completed = exact.calls.called(call).outcome;
      replies[2]({ value: 'second count' }); await flush();
      output.reaskedOwner = count.r.ticket.invocation;
      replies[3]({ value: 'third count' }); await flush();
      exact.announceModule('tick'); await flush();
      replies[4]({ value: 'fourth count' }); await flush();
      rt.commit(() => rt.R(count)); await flush();
      replies[5]({ value: 'person count' }); await flush();
      output.requests = requests; output.guarded = guarded; output.parses = parses;
      return output;
    });
    expect(result).toEqual({ before: 'running', owner: 0, reaskedOwner: 0, completed: 'ok',
      requests: [['wait', 'tool'], ['count', 'tool'], ['count', 'tool'], ['count', 'tool'], ['count', 'tool'], ['count', 'app']],
      parses: [['count', 'tool', 1], ['wait', 'tool', 1], ['count', 'tool', 0], ['count', 'tool', 0], ['count', 'tool', 0], ['count', 'app', 0]],
      guarded: [...Array(4).fill('guard.read: NotAllowedError'), 'allowed'] });
  } finally { await browser.close(); server.stop(true); }
}, 60000);

test('an owned stream keeps a call running until its deadline', async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      let read, message, finish;
      rt.data.answer = () => ({ stream: deliver => { message = deliver; return new Promise(resolve => { finish = resolve; }); } });
      rt.mount(() => { read = rt.res('read', 'read', () => [], 'seed', [], 's', '', true); });
      tools({ watch: [() => rt.act(() => rt.R(read)), {}, 'reads', [false, false, true], 'Watch.', null, () => read()] });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const { call } = exact.calls.call({ tool: 'watch' }); await flush();
      const before = exact.calls.called(call).state;
      message({ v: 'message' }); await flush();
      const afterMessage = exact.calls.called(call).state;
      rt.advance(10000, false); await flush();
      const c = exact.calls.called(call);
      finish({ v: 'ended' }); await flush();
      return [before, afterMessage, c.outcome, c.certainty];
    });
    expect(result).toEqual(['running', 'running', 'timeout', 'uncertain']);
  } finally { await browser.close(); server.stop(true); }
}, 60000);

test('a tool owns sends from its then and queue, and reports a standing resource value', async () => {
  const server = runtimeServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const registered = {};
      document.modelContext = { registerTool: tool => { registered[tool.name] = tool; return Promise.resolve(); } };
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const requests = [], replies = [], output = {}, slot = rt.sig('seed'), other = rt.sig('seed'), choose = rt.sig(false);
      rt.data.answer = source => ({ req: { method: 'GET', url: source, headers: [] } });
      rt.data.fetch = req => new Promise(resolve => { requests.push([req.url, req.origin]); replies.push(resolve); });
      rt.data.parse = (_source, _args, o) => ({ v: o.value });
      let resource, first, second, queued;
      rt.mount(() => {
        resource = rt.res('read', 'read', () => [], 'seed', [], 's', '', true);
        first = rt.mut('first', slot, [], 's');
        second = rt.mut('second', other, [resource], 's');
        queued = rt.mut('queued', rt.sig('seed'), [], 's', 1);
      });
      rt.queues([[slot, other, choose], [], [resource]]);
      first.then = () => rt.M(second, 'second', []);
      const returned = rt.memo(() => choose() ? resource() : slot());
      const row = (action, returns = () => returned()) => [() => rt.act(action), {}, 'writes', [false, false, true], 'A tool.', null, returns, null,
        { digest: 'd', doors: ['web'] }, 's'];
      tools({
        chain: row(() => { rt.W(choose, true); rt.M(first, 'first', []); }),
        queue: row(() => { rt.M(queued, 'queued', [1]); rt.M(queued, 'queued', [2]); }, () => other()),
        wait: row(() => rt.M(queued, 'queued', [3])),
      });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const state = id => {
        const c = exact.calls.called(id);
        return c.outcome ? [c.outcome, c.certainty, c.value] : c.state;
      };
      const answer = async (i, value) => { replies[i]({ value }); await flush(); };
      await flush();
      const web = registered.chain.execute({});
      await flush();
      await answer(0, 'first');
      output.beforeThen = state(1);
      rt.advance(rt.clock.now, false);
      await flush();
      output.afterThen = state(1);
      await answer(1, 'second');
      output.afterSend = state(1);
      await answer(2, 'fresh');
      output.web = await web;

      let { call } = exact.calls.call({ tool: 'queue' });
      await flush();
      await answer(3, 'queued first');
      output.beforeNext = state(call);
      rt.advance(rt.clock.now, false);
      await flush();
      output.afterNext = state(call);
      await answer(4, 'queued second');
      output.queue = state(call);

      // An unrelated refresh does not delay reporting the resource's standing value.
      rt.commit(() => rt.W(choose, false));
      ({ call } = exact.calls.call({ tool: 'wait' }));
      await flush();
      rt.commit(() => { rt.R(resource); rt.W(choose, true); });
      await answer(5, 'sent');
      output.pendingCompletion = state(call);
      await answer(6, 'reported');
      output.completed = state(call);
      output.requests = requests;
      return output;
    });
    expect(result).toEqual({
      beforeThen: 'running', afterThen: 'running', afterSend: 'running',
      web: { outcome: 'ok', certainty: 'applied', reason: '', stale: false, skipped: [], value: 'fresh', message: '' },
      beforeNext: 'running', afterNext: 'running', queue: ['ok', 'applied', 'second'],
      pendingCompletion: ['ok', 'applied', 'fresh'], completed: ['ok', 'applied', 'fresh'],
      requests: [['first', 'tool'], ['second', 'tool'], ['read', 'tool'], ['queued', 'tool'], ['queued', 'tool'], ['queued', 'tool'], ['read', 'app']],
    });
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 60000);

test('tool calls own their requests and can report retained resource values', async () => {
  const server = runtimeServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const requests = [], replies = [], output = {};
      rt.data.answer = (source, args) => ({ req: { method: 'GET', url: source, headers: [], body: JSON.stringify(args) } });
      rt.data.fetch = req => new Promise(resolve => { requests.push(req); replies.push(resolve); });
      rt.data.parse = (_source, _args, outcome) => outcome.next ? { req: { method: 'GET', url: 'second', headers: [] } } : { v: outcome.value };
      const slot = rt.sig('seed'), query = rt.sig(0), unrelatedQuery = rt.sig(0);
      let resource, unrelated, mutation;
      rt.mount(() => {
        resource = rt.res('read', 'read', () => [query()], 'seed', [0], 's', '', true);
        unrelated = rt.res('other', 'other', () => [unrelatedQuery()], 'seed', [0], 's', '', true);
        mutation = rt.mut('send', slot, [], 's');
      });
      const report = rt.memo(() => resource()), failed = rt.memo(() => report() === 'failed');
      rt.memo(() => { if (query() === 1) throw new rt.Refusal('refused replacement'); return false; });
      let ran = 0;
      const row = (action, returns = () => report(), fails = null) => [() => rt.act(action), {}, 'writes', [false, false, true], 'A tool.', fails, returns, null, null, 's'];
      tools({
        refresh: row(() => rt.R(resource)),
        read: row(() => { ran++; }),
        fails: row(() => { ran++; }, () => 'answer', () => failed()),
        send: row(() => rt.M(mutation, 'send', []), () => slot()),
      });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const call = async tool => { const { call } = exact.calls.call({ tool }); await flush(); return call; };
      const state = id => {
        const c = exact.calls.called(id);
        return c.outcome ? [c.outcome, c.certainty, c.reason, c.value] : c.state;
      };
      const answer = async (i, value, next = false) => { replies[i]({ value, next }); await flush(); };

      // A person's refresh replaces a call's ask. Refused commits restore both the ticket and its owner.
      let id = await call('refresh');
      output.started = state(id);
      rt.commit(() => { rt.R(resource); rt.W(query, 1); });
      await flush();
      output.rollback = state(id);
      rt.commit(() => rt.R(resource));
      await flush();
      output.refresh = state(id);
      // Release both requests so a missing supersession cannot hold the next call in the old runtime.
      await answer(0, 'old');
      await answer(1, 'person');

      // Both clauses read through derives, including a cached derive whose value has not changed.
      rt.commit(() => rt.R(resource));
      output.pendingReturns = state(await call('read'));
      output.pendingFails = state(await call('fails'));
      output.ran = ran;
      await answer(2, 'ready');

      id = await call('send');
      rt.commit(() => rt.M(mutation, 'send', ['person']));
      await flush();
      output.send = state(id);
      await answer(3, 'old send');
      await answer(4, 'person send');

      // A request made by a reply's parse keeps the invocation through the asynchronous continuation.
      id = await call('send');
      await answer(5, null, true);
      output.chained = state(id);
      rt.commit(() => rt.W(slot, 'person assignment'));
      await flush();
      output.chainedSuperseded = state(id);
      await answer(6, 'late');

      id = await call('send');
      // Another request starts in the turn in which the owned reply settles.
      replies[7]({ value: 'done' });
      queueMicrotask(() => rt.commit(() => rt.W(unrelatedQuery, 1)));
      await flush();
      output.unrelated = state(id);
      output.otherPending = unrelated.p();
      await answer(8, 'other');
      output.ordinary = state(await call('read'));

      // New arguments keeping the same HTTP request preserve its invocation.
      rt.data.answer = source => ({ req: { method: 'GET', url: source, headers: [] } });
      id = await call('refresh');
      rt.commit(() => rt.W(query, 1));
      await flush();
      output.keptRollback = state(id);
      rt.commit(() => rt.W(query, 2));
      await flush();
      output.keptReplacement = state(id);
      output.keptRequests = requests.length;
      await answer(9, 'moved');
      output.keptCompletion = state(id);
      output.keptValue = report();
      output.requestOrigins = requests.map(r => r.origin);
      return output;
    });
    expect(result).toEqual({
      started: 'running', rollback: 'running', refresh: ['superseded', 'applied', '', null],
      pendingReturns: ['ok', 'applied', '', 'person'], pendingFails: ['ok', 'applied', '', 'answer'], ran: 2,
      send: ['superseded', 'uncertain', '', null], chained: 'running', chainedSuperseded: ['superseded', 'uncertain', '', null],
      unrelated: ['ok', 'applied', '', 'done'], otherPending: true, ordinary: ['ok', 'applied', '', 'ready'],
      keptRollback: 'running', keptReplacement: 'running', keptRequests: 10, keptValue: 'moved',
      keptCompletion: ['ok', 'applied', '', 'moved'],
      requestOrigins: ['tool', 'app', 'app', 'tool', 'app', 'tool', 'tool', 'tool', 'app', 'tool'],
    });
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 60000);

test('a tool can report a gathered family value while a member is pending', async () => {
  const server = runtimeServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      const { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const ids = rt.sig([]), replies = [], output = {};
      let rows, ran = 0;
      rt.data.answer = () => {
        const answer = Promise.withResolvers();
        replies.push(answer.resolve);
        return { promise: answer.promise };
      };
      rt.mount(() => { rows = resEach('rows', 'row', ids, x => x, x => [x], '[s', 's', ''); });
      const count = rt.memo(() => rows().length), failed = rt.memo(() => rows().includes('failed'));
      const row = (returns, fails = null) => [() => rt.act(() => { ran++; }), {}, 'reads', [false, false, true], 'A tool.', fails, returns];
      tools({ direct: row(() => rows().length), derived: row(() => count()), fails: row(() => 'ready', () => failed()) });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const call = async tool => {
        const { call } = exact.calls.call({ tool }); await flush();
        const c = exact.calls.called(call);
        return [c.outcome, c.certainty, c.reason, c.value];
      };
      output.empty = await call('direct');
      rt.commit(() => rt.W(ids, [1, 2]));
      replies[0]({ v: 'first' }); await flush();
      output.pending = [await call('direct'), await call('derived'), await call('fails')];
      output.ranWhilePending = ran;
      replies[1]({ v: 'second' }); await flush();
      output.settled = [await call('direct'), await call('derived'), await call('fails')];
      output.ran = ran;
      return output;
    });
    expect(result).toEqual({
      empty: ['ok', 'applied', '', 0],
      pending: [['ok', 'applied', '', 2], ['ok', 'applied', '', 2], ['ok', 'applied', '', 'ready']], ranWhilePending: 4,
      settled: [['ok', 'applied', '', 2], ['ok', 'applied', '', 2], ['ok', 'applied', '', 'ready']], ran: 7,
    });
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 60000);

test('removing an owned family member supersedes a call only when the commit stands', async () => {
  const server = runtimeServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      const { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const ids = rt.sig([]), bad = rt.sig(false), output = {};
      let rows, finish;
      rt.data.answer = () => ({ promise: new Promise(resolve => { finish = resolve; }) });
      rt.mount(() => { rows = resEach('rows', 'row', ids, x => x, x => [x], '[s', 's', ''); });
      rt.memo(() => { if (bad()) throw new rt.Refusal('refused removal'); return false; });
      tools({ add: [() => rt.act(() => rt.W(ids, [1])), {}, 'writes', [false, false, true], 'A tool.', null, () => ids().length] });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const { call } = exact.calls.call({ tool: 'add' }); await flush();
      output.before = exact.calls.called(call).state;
      output.refused = rt.commit(() => { rt.W(ids, []); rt.W(bad, true); }); await flush();
      output.rollback = [exact.calls.called(call).state, rows.p(), ids()];
      output.removed = rt.commit(() => rt.W(ids, [])); await flush();
      const c = exact.calls.called(call);
      output.ended = [c.outcome, c.certainty, c.value];
      finish({ v: 'late' }); await flush();
      output.rows = rows();
      return output;
    });
    expect(result).toEqual({ before: 'running', refused: false, rollback: ['running', true, [1]], removed: true,
      ended: ['superseded', 'applied', null], rows: [] });
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 60000);

test('a superseded call keeps its continuation origin without skipping its commands', async () => {
  const server = runtimeServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const input = document.createElement('input'); input.id = 'n'; document.body.append(input);
      rt.useCommands({ focus: () => id => document.getElementById(id).focus() });
      const requests = [], replies = [], output = [];
      rt.data.answer = source => ({ req: { method: 'GET', url: source, headers: [] } });
      rt.data.fetch = req => new Promise(resolve => { requests.push([req.url, req.origin]); replies.push(resolve); });
      rt.data.parse = (_source, _args, o) => ({ v: o.value });
      let count, save, next, wait;
      rt.mount(() => {
        count = rt.res('count', 'count', () => [], 'seed', [], 's', '', true);
        save = rt.mut('save', rt.sig('seed'), [], 's');
        next = rt.mut('next', rt.sig('seed'), [], 's');
        wait = rt.mut('wait', rt.sig('seed'), [], 's');
      });
      save.then = () => { rt.C('focus', ['n']); rt.M(next, 'next', []); };
      const row = action => [() => rt.act(action), {}, 'writes', [false, false, true], 'A tool.'];
      tools({ save: row(() => { rt.C('focus', ['n']); rt.M(save, 'save', []); rt.R(count); }), wait: row(() => rt.M(wait, 'wait', [])) });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const call = async tool => { const { call } = exact.calls.call({ tool }); await flush(); return call; };
      for (const anotherCall of [false, true]) {
        const start = replies.length, id = await call('save');
        const before = document.activeElement.id;
        replies[start]({ value: 'saved' }); await flush();
        rt.commit(() => rt.R(count)); await flush();
        const other = anotherCall ? await call('wait') : null;
        rt.advance(rt.clock.now, false); await flush();
        const c = exact.calls.called(id);
        output.push([before, document.activeElement.id, c.outcome, c.certainty, c.skipped,
          other && exact.calls.called(other).state, requests.at(-1)]);
        for (let i = start + 1; i < replies.length; i++) replies[i]({ value: 'done' });
        await flush();
        if (other) output.push(exact.calls.called(other).skipped);
        input.blur();
      }
      return output;
    });
    const ended = ['', 'n', 'superseded', 'applied', ['focus']];
    expect(result).toEqual([[...ended, null, ['next', 'tool']], [...ended, 'running', ['next', 'tool']], []]);
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 60000);

test('a call stays running until its deferred tree update, which skips its commands', async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const input = document.createElement('input'); input.id = 'n'; document.body.append(input);
      rt.useCommands({ focus: () => id => document.getElementById(id).focus() });
      const side = rt.sig(false), output = {};
      const panel = text => parent => {
        const el = document.createElement('div'); el.textContent = text; el.dataset.sharedElement = 'panel';
        el.style.setProperty('--exact-layout-transition', '20 0 linear'); parent.append(el); rt.pr(el);
      };
      rt.mount(root => rt.when(root, side, panel('new'), panel('old')));
      await rt.pieces();
      tools({ open: [() => rt.act(() => { rt.W(side, true); rt.C('focus', ['n']); }), {}, 'writes', [false, false, true], 'Open.'] });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
      let transition;
      document.startViewTransition = update => transition = start(async () => { await gate.promise; return update(); });
      const { call } = exact.calls.call({ tool: 'open' }); await flush();
      output.deferred = !!transition;
      for (const f of rt.Clocked) f(); await flush();
      output.during = exact.calls.called(call).state;
      gate.resolve(); await transition.updateCallbackDone; await transition.finished; await flush();
      const c = exact.calls.called(call);
      output.ended = [c.outcome, c.certainty, c.skipped];
      output.focused = document.activeElement.id;
      return output;
    });
    expect(result).toEqual({ deferred: true, during: 'running', ended: ['ok', 'applied', ['focus']], focused: '' });
  } finally { await browser.close(); server.stop(true); }
}, 60000);

test('a family member removed in a deferred tree update supersedes its call past a refused commit', async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js'), { resEach } = await import('/host/web-js/each.js');
      const { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const ids = rt.sig([]), side = rt.sig(false), bad = rt.sig(false), output = {};
      let rows, finish;
      rt.data.answer = () => ({ promise: new Promise(resolve => { finish = resolve; }) });
      const panel = text => parent => {
        const el = document.createElement('div'); el.textContent = text; el.dataset.sharedElement = 'panel';
        el.style.setProperty('--exact-layout-transition', '20 0 linear'); parent.append(el); rt.pr(el);
      };
      rt.mount(root => {
        rows = resEach('rows', 'row', ids, x => x, x => [x], '[s', 's', '');
        rt.when(root, side, panel('new'), panel('old'));
      });
      rt.memo(() => { if (bad()) throw new rt.Refusal('refused'); return false; });
      await rt.pieces();
      tools({ add: [() => rt.act(() => rt.W(ids, [1])), {}, 'writes', [false, false, true], 'Add.', null, () => ids().length] });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      const { call } = exact.calls.call({ tool: 'add' }); await flush();
      output.before = exact.calls.called(call).state;
      const gate = Promise.withResolvers(), start = document.startViewTransition.bind(document);
      let transition;
      document.startViewTransition = update => transition = start(async () => { await gate.promise; return update(); });
      rt.commit(() => { rt.W(ids, []); rt.W(side, true); });
      output.deferred = !!transition;
      output.refused = rt.commit(() => rt.W(bad, true));
      gate.resolve(); await transition.updateCallbackDone; await transition.finished; await flush();
      const c = exact.calls.called(call);
      output.ended = c.outcome ? [c.outcome, c.certainty] : c.state;
      finish({ v: 'late' }); await flush();
      return output;
    });
    expect(result).toEqual({ before: 'running', deferred: true, refused: false, ended: ['superseded', 'applied'] });
  } finally { await browser.close(); server.stop(true); }
}, 60000);

test("a device re-ask that answers at once supersedes the call's ask", async () => {
  const server = runtimeServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      rt.clock.agent = true;
      const replies = [], output = [];
      let read, wait, now = 'ask';
      rt.data.answer = source => {
        if (source === 'read' && now === 'value') return { v: 'device' };
        if (source === 'read' && now === 'throw') throw new Error('offline');
        return { req: { method: 'GET', url: source, headers: [] } };
      };
      rt.data.fetch = () => new Promise(resolve => replies.push(resolve));
      // What modules.js `reask` does when a module's key changes while the call's reply is answered.
      rt.data.parse = (source, _args, o) => { if (source === 'wait') { now = o.value; rt.R(read, true); } return { v: o.value }; };
      rt.mount(() => {
        read = rt.res('read', 'read', () => [], 'seed', [], 's', '', true);
        wait = rt.mut('wait', rt.sig('seed'), [], 's');
      });
      tools({ bump: [() => rt.act(() => { rt.R(read); rt.M(wait, 'wait', []); }), {}, 'writes', [false, false, true], 'Bump.'] });
      const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };
      for (const answer of ['value', 'throw']) {
        const start = replies.length;
        now = 'ask';
        const { call } = exact.calls.call({ tool: 'bump' }); await flush();
        // replies[start] is the mutation's, replies[start + 1] the resource's
        replies[start]({ value: answer }); await flush();
        const c = exact.calls.called(call);
        output.push(c.outcome ? [c.outcome, c.certainty] : c.state);
        replies[start + 1]({ value: 'late' }); await flush();
      }
      return output;
    });
    expect(result).toEqual([['superseded', 'applied'], ['superseded', 'applied']]);
  } finally { await browser.close(); server.stop(true); }
}, 60000);
