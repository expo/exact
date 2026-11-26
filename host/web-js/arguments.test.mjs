import { expect } from 'bun:test';
import { engineTests } from '../../scripts/coverage.mjs';
import { existsSync } from 'node:fs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

const { chromium: test } = await engineTests(['chromium']);

function runtimeServer(development = false) {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') { return new Response(null, { status: 204 }); }
    if (path.endsWith('/names.js')) { return new Response('export default [[], [], []]; export const types = [[], [], []], sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } }); }
    if (path === '/') { return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } }); }
    const local = new URL('../..' + path, import.meta.url), file = existsSync(local) ? local : new URL('../web/' + path.split('/').pop(), import.meta.url);
    const code = hostFixtureFile(file);
    return new Response(development && path.endsWith('/rt.js')
      ? String(code).replace('const DEVELOPMENT_STORE = null', 'const DEVELOPMENT_STORE = "fixture"') : code, { headers: { 'content-type': 'text/javascript' } });
  } });
}

// A Rust source's GET or HEAD read is marked `cancel` (rust-data.js, rust-readers.test.mjs); here the runtime's half.
test.each([1, 128])('a commit forgets and aborts %s Rust HTTP reads, and old replies land nowhere after remount', async lanes => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async lanes => {
      const rt = await import('/host/web-js/rt.js');
      const wave = rt.sig(0), refusing = rt.sig(false), requests = [], values = [];
      rt.clock.agent = true;
      rt.data.answer = (_source, [wave, lane]) => wave
        ? { req: { method: lane % 2 ? 'HEAD' : 'GET', url: `/hold/${wave}/${lane}`, cancel: true } }
        : { v: 'idle' };
      let parsed = 0;
      rt.data.parse = (_source, _args, outcome) => { parsed++; return { v: outcome.value }; };
      rt.data.fetch = (req, _message, controller) => new Promise(resolve => requests.push({ req, controller, resolve }));
      rt.mount(() => {
        for (let lane = 0; lane < lanes; lane++) {
          const value = rt.res(`lane-${lane}`, 'completion', () => [wave(), lane], 'idle', [0, lane], 's', '', true);
          values.push(value);
          rt.effect(() => value());
        }
        rt.memo(() => { if (refusing()) throw new rt.Refusal('refuse navigation'); });
      });
      rt.commit(() => rt.W(wave, 1));
      const pending = values.filter(v => v.p()).length;
      // A refused navigation keeps the requests alive.
      const refused = rt.commit(() => { rt.W(wave, 0); rt.W(refusing, true); });
      const kept = requests.every(r => r.controller && !r.controller.signal.aborted);
      rt.commit(() => rt.W(wave, 0));
      const away = { pending: values.filter(v => v.p()).length, inflight: rt.inflight.n,
        aborted: requests.filter(r => r.controller?.signal.aborted).length };
      rt.commit(() => rt.W(wave, 2));
      // Even a transport that delivers a reply after abort cannot land an old answer.
      for (const r of requests.slice(0, lanes)) r.resolve({ value: 'old' });
      await Promise.resolve();
      const remounted = { pending: values.filter(v => v.p()).length, parsed, values: [...new Set(values.map(v => v()))] };
      for (const r of requests.slice(lanes)) r.resolve({ value: 'fresh' });
      await Promise.resolve();
      return { pending, refused, kept, away, remounted,
        complete: { pending: values.filter(v => v.p()).length, parsed, inflight: rt.inflight.n,
          values: [...new Set(values.map(v => v()))] } };
    }, lanes);
    expect(result).toEqual({ pending: lanes, refused: false, kept: true,
      away: { pending: 0, inflight: 0, aborted: lanes },
      remounted: { pending: lanes, parsed: 0, values: ['idle'] },
      complete: { pending: 0, parsed: lanes, inflight: 0, values: ['fresh'] } });
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test('agent state distinguishes cancellable HTTP reads from Rust and TypeScript streams', async () => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js'), { install } = await import('/host/web-js/agent.js');
      rt.clock.agent = true;
      rt.data.answer = source => source === 'typescript' ? { stream: () => new Promise(() => {}) }
        : { req: { method: source === 'head' ? 'HEAD' : 'GET', url: '/held', stream: source === 'rust', cancel: source !== 'rust' } };
      rt.data.fetch = () => new Promise(() => {});
      let region;
      rt.mount(() => {
        region = rt.scope(() => {
          for (const source of ['get', 'head', 'rust', 'typescript']) {
            const value = rt.res(source, source, () => [], undefined, undefined, 's', 'idle');
            rt.effect(() => value());
          }
        });
      });
      const exact = globalThis.exact;
      Object.assign(exact, { clock: rt.clock, After: rt.After, views: rt.Views, viewId: rt.viewId,
        resources: rt.Resources, mutations: rt.Mutations, state: [[], [], []] });
      install(exact);
      const state = await exact.agentSettled({ op: 'state' });
      const cancellable = rt.Resources.every(r => r.ticket?.ctl instanceof AbortController);
      rt.commit(() => rt.end(region));
      const ended = await exact.agentSettled({ op: 'state' });
      return { cancellable, pending: state.pending.map(r => r.name), streams: state.streams.map(r => r.name),
        ended: { pending: ended.pending, streams: ended.streams, inflight: rt.inflight.n } };
    });
    expect(result).toEqual({ cancellable: true, pending: ['get', 'head', 'rust', 'typescript'],
      streams: ['rust', 'typescript'], ended: { pending: [], streams: [], inflight: 0 } });
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

// A region's arm owns its resources (mount.js `when`): a refused commit keeps the arm, its read and its reply.
test.each([false, true])("a region's HTTP read follows the committed region (refused: %s)", async refused => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async refused => {
      const rt = await import('/host/web-js/rt.js');
      const mounted = rt.sig(true), refusing = rt.sig(false), requests = [], values = [], parsed = [];
      rt.clock.agent = true;
      rt.data.answer = () => ({ req: { method: 'GET', url: '/held', cancel: true } });
      rt.data.parse = (_source, _args, outcome, store) => {
        parsed.push(outcome);
        store.set('answer', 'parsed');
        return { v: 'parsed' };
      };
      rt.data.fetch = (_req, _message, controller) => new Promise(resolve => {
        requests.push({ controller, resolve });
        controller.signal.addEventListener('abort', () => resolve({ failed: 1, message: 'aborted' }));
      });
      rt.mount(root => {
        const region = document.createElement('div');
        root.append(region);
        rt.when(region, () => mounted(), () => {
          const value = rt.res('value', 'read', () => [], undefined, undefined, 's', 'idle');
          values.push(value);
          rt.effect(() => value());
        });
        rt.memo(() => { if (refusing()) throw new rt.Refusal('refuse navigation'); });
      });
      const committed = rt.commit(() => { rt.W(mounted, false); rt.W(refusing, refused); });
      const ended = !!values[0].r.gone, aborted = requests[0].controller.signal.aborted, inflight = rt.inflight.n;
      requests[0].resolve({ value: 'late' });
      await Promise.resolve();
      await Promise.resolve();
      return { committed, ended, aborted, inflight, parsed: parsed.length, value: values[0].r.value,
        stored: rt.Store.map.has('answer'), closed: rt.journal.some(line => line.endsWith(': its ticket was let go')),
        arms: values.length, requests: requests.length };
    }, refused);
    expect(result).toEqual({ committed: !refused, ended: !refused, aborted: !refused, inflight: refused ? 1 : 0,
      parsed: refused ? 1 : 0, value: refused ? 'parsed' : 'idle', stored: refused, closed: !refused,
      arms: 1, requests: 1 });
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test.each(['startActivity', 'updateActivity', 'endActivity'])('an unlinked %s send counts only finite arguments', async source => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async source => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      const result = rt.sig(null), changed = rt.sig(false);
      let mutation;
      rt.clock.agent = true;
      rt.data.answer = () => { throw new Error('an activity reached the app source'); };
      rt.mount(() => { mutation = rt.mut('result', result, [], 's'); });
      return [Infinity, 1].map(n => {
        const committed = rt.commit(() => { rt.W(changed, true); rt.M(mutation, source, ['key', n]); });
        return { committed, changed: changed(), result: result(), requests: mutation.requests, error: rt.refused()?.message };
      });
    }, source);
    expect(result).toEqual([
      { committed: false, changed: false, result: null, requests: 0, error: 'result: argument 2 contains a non-finite number' },
      { committed: false, changed: false, result: null, requests: 1, error: `${source} is not linked into this artifact` },
    ]);
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test.each([false, true])('a non-finite context cannot reuse an upload (completed: %s)', async completed => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async completed => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      rt.data.uploads = (await import('/host/web-js/uploads.js')).ledger;
      const b = rt.sig(1), result = rt.sig(null), count = rt.sig(0);
      let calls = 0, value, mutation, finish;
      rt.clock.agent = true;
      rt.data.answer = source => {
        if (source !== 'up') return new Promise(() => {});
        calls++;
        return { upload: 'up', stream: (_progress, ctl) => {
          ctl.sent = true;
          return new Promise(done => { finish = () => done({ v: 'answer' }); });
        } };
      };
      rt.mount(root => {
        const args = () => ['key', 1 / b()]; args.context = 1;
        value = rt.res('value', 'up', args, undefined, undefined, 's', '');
        mutation = rt.mut('result', result, [value], 's');
        rt.hd(root, { headTitle: () => value() });
      });
      if (completed) { finish(); await Promise.resolve(); }
      const committed = rt.commit(() => rt.W(b, 0));
      const refused = { committed, b: b(), calls, pending: value.p(), failed: value.f(), error: rt.refused()?.message,
        context: value.r.ticket?.args[1] ?? value.r.settled?.[1] };
      rt.commit(() => rt.M(mutation, 'saveLater', []));
      const cancelled = rt.commit(() => { rt.W(b, 0); rt.W(result, null); });
      const reconciled = { committed: cancelled !== false, b: b(), calls, pending: value.p(), failed: value.f(),
        error: value.e()?.[1], mutationPending: mutation.p(), hasTicket: value.r.ticket != null };
      // A refresh at the non-finite context checks it again, whether or not the upload ended in failure.
      const refreshed = rt.commit(() => { rt.W(count, count() + 1); rt.R(value); });
      const refresh = { committed: refreshed !== false, count: count(), error: rt.refused()?.message, failed: value.f() };
      if (!completed) { finish(); await Promise.resolve(); }
      return { refused, reconciled, refresh, late: { value: value(), failed: value.f(), pending: value.p() } };
    }, completed);
    expect(result).toEqual({
      refused: { committed: false, b: 1, calls: 1, pending: !completed, failed: false,
        error: 'value: argument 2 contains a non-finite number', context: 1 },
      reconciled: { committed: true, b: 0, calls: 1, pending: false, failed: true,
        error: 'value: argument 2 contains a non-finite number', mutationPending: false, hasTicket: false },
      refresh: { committed: false, count: 0, error: 'value: argument 2 contains a non-finite number', failed: true },
      late: { value: completed ? 'answer' : '', failed: true, pending: false },
    });
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

// The same scalar and container arguments as runner/tests/it/arguments.rs.
test.each(['resource', 'send'])('a non-finite %s argument refuses before dispatch and rolls back', async target => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const results = await page.evaluate(async target => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      const b = rt.sig(1), result = rt.sig(null), changed = rt.sig(false);
      const expressions = [n => 1 / n, n => -1 / n, n => 0 / n, n => [1 / n], n => 1 / n, n => [[1 / n, null]]];
      let expression = expressions[0], calls = 0, value, mutation;
      rt.clock.agent = true;
      rt.data.answer = () => { calls++; return { v: 'answer' }; };
      rt.mount(root => {
        if (target === 'resource') value = rt.res('value', 'read', () => ['key', expression(b())], undefined, undefined, 's', '');
        else mutation = rt.mut('result', result, [], 's');
        rt.hd(root, { headTitle: () => value ? value() : 'arguments' });
      });
      return expressions.map(expr => {
        expression = expr;
        const before = calls, requests = mutation?.requests ?? 0;
        const committed = rt.commit(() => {
          if (target === 'resource') rt.W(b, 0);
          else { rt.W(changed, true); rt.M(mutation, 'save', ['key', expression(0)]); }
        });
        const refused = { committed, calls: calls - before, requests: (mutation?.requests ?? 0) - requests,
          b: b(), changed: changed(), result: result(), why: rt.refused()?.message };
        const accepted = rt.commit(() => {
          if (target === 'resource') rt.W(b, b() + 1);
          else rt.M(mutation, 'save', ['key', expression(1)]);
        });
        const finite = { committed: accepted !== false, calls: calls - before, requests: (mutation?.requests ?? 0) - requests,
          value: value ? value() : result() };
        rt.commit(() => { rt.W(b, 1); rt.W(result, null); });
        return { refused, finite };
      });
    }, target);
    for (const { refused, finite } of results) {
      expect(refused).toEqual({ committed: false, calls: 0, requests: 0, b: 1, changed: false, result: null,
        why: `${target === 'resource' ? 'value' : 'result'}: argument 2 contains a non-finite number` });
      expect(finite).toEqual({ committed: true, calls: 1, requests: target === 'send' ? 1 : 0, value: 'answer' });
    }
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test.each([[1, 'scalar'], [0, 'scalar'], [0, 'list'], [0, 'record']])('a non-finite resource argument keeps its failure after cancellation (numerator: %s, %s)', async (numerator, container) => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async ({ numerator, container }) => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      const b = rt.sig(1), result = rt.sig(null), count = rt.sig(0);
      let calls = 0, value, mutation, resolve;
      rt.data.answer = source => {
        calls++;
        return source === 'read' ? { v: 'answer' } : new Promise(done => { resolve = done; });
      };
      rt.mount(root => {
        value = rt.res('value', 'read', () => {
          const arg = numerator / b() + count() * 0;
          return ['key', container === 'scalar' ? arg : container === 'list' ? [arg] : [[arg, null]]];
        }, undefined, undefined, 's', '');
        mutation = rt.mut('result', result, [value], 's');
        rt.hd(root, { headTitle: () => value() });
      });
      rt.commit(() => rt.M(mutation, 'saveLater', []));
      const before = { calls, pending: mutation.p() };
      const committed = rt.commit(() => { rt.W(b, 0); rt.W(result, null); });
      const cancelled = { committed: committed !== false, calls, b: b(), result: result(), pending: mutation.p(),
        failed: value.f(), value: value(), error: value.e()?.[1] };
      const accepted = rt.commit(() => rt.W(count, count() + 1));
      const unrelated = { committed: accepted !== false, count: count(), calls, failed: value.f() };
      resolve('late');
      await Promise.resolve();
      const late = { result: result(), failed: value.f() };
      rt.commit(() => { rt.W(b, 2); rt.R(value.r); });
      return { before, cancelled, unrelated, late, recovered: { calls, failed: value.f(), value: value() } };
    }, { numerator, container });
    expect(result).toEqual({
      before: { calls: 2, pending: true },
      cancelled: { committed: true, calls: 2, b: 0, result: null, pending: false, failed: true,
        value: 'answer', error: 'value: argument 2 contains a non-finite number' },
      unrelated: { committed: true, count: 1, calls: 2, failed: true },
      late: { result: null, failed: true },
      recovered: { calls: 3, failed: false, value: 'answer' },
    });
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test.each(['scalar', 'list', 'record'])('signed zero keeps a reconciliation failure (%s)', async container => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async container => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      const b = rt.sig(0), result = rt.sig(null);
      let calls = 0, fail = false, value, mutation;
      rt.clock.agent = true;
      rt.data.answer = source => {
        if (source !== 'read') return new Promise(() => {});
        calls++;
        if (fail) throw new Error('reconciliation failed');
        return { v: 'answer' };
      };
      rt.mount(root => {
        value = rt.res('value', 'read', () => [container === 'scalar' ? b() : container === 'list' ? [b()] : [[b(), null]]],
          undefined, undefined, 's', '');
        mutation = rt.mut('result', result, [value], 's');
        rt.hd(root, { headTitle: () => value() });
      });
      rt.commit(() => rt.M(mutation, 'saveLater', []));
      fail = true;
      rt.commit(() => rt.W(result, null));
      const cancelled = { calls, failed: value.f(), error: value.e()?.[1] };
      fail = false;
      rt.commit(() => rt.W(b, -b()));
      const flipped = { calls, failed: value.f(), error: value.e()?.[1] };
      rt.commit(() => rt.W(b, 1));
      return { cancelled, flipped, recovered: { calls, failed: value.f() } };
    }, container);
    expect(result).toEqual({
      cancelled: { calls: 2, failed: true, error: 'reconciliation failed' },
      flipped: { calls: 2, failed: true, error: 'reconciliation failed' },
      recovered: { calls: 3, failed: false },
    });
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test.each([false, true])('a non-finite resource argument refuses before activation (baked: %s)', async baked => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async baked => {
      globalThis.__exactRender = true;
      const rt = await import('/host/web-js/rt.js');
      const b = rt.sig(1), callbacks = [];
      let ready = false, calls = 0, value;
      rt.data.answer = () => { if (!ready) return null; calls++; return { v: 'answer' }; };
      rt.data.ready = f => callbacks.push(f);
      rt.mount(root => {
        value = rt.res('value', 'read', () => ['key', 1 / b()], baked ? 'answer' : undefined,
          baked ? ['key', 1] : undefined, 's', '');
        rt.hd(root, { headTitle: () => value() });
      });
      const committed = rt.commit(() => rt.W(b, 0));
      const refused = { committed, b: b(), calls, error: rt.refused()?.message, failed: value.f() };
      ready = true;
      for (const f of callbacks) f();
      return { refused, activated: { calls, value: value(), pending: value.p(), failed: value.f() } };
    }, baked);
    expect(result).toEqual({
      refused: { committed: false, b: 1, calls: 0, error: 'value: argument 2 contains a non-finite number', failed: false },
      activated: { calls: 1, value: 'answer', pending: false, failed: false },
    });
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test.each(['one', 'mixed', 'burst', 'continuous', 'held', 'empty', 'continuation', 'identity', 'failure', 'refusal', 'topic-refusal', 'store', 'topic-store', 'binary', 'idle', 'unmount'])(
  'a read kept during a change of arguments: %s', async scenario => {
    const server = runtimeServer();
    let browser, page;
    try {
      browser = await suiteBrowser();
      page = await browser.newPage();
      await page.goto(server.url.href);
      const result = await page.evaluate(async scenario => {
        const rt = await import('/host/web-js/rt.js');
        const seq = rt.sig(0), topic = rt.sig('a'), active = rt.sig(true), reads = [], frames = [];
        const args = Object.assign(() => [topic(), seq()], { context: 1, gate: active });
        const store = ['store', 'topic-store'].includes(scenario);
        let value, region, state = 0;
        rt.clock.agent = true;
        let refuse = false, continued = false;
        rt.data.answer = () => {
          if (refuse) {
            rt.Store.set('attempted', 'yes');
            throw Object.assign(Error('offline'), { refuse: true });
          }
          if (scenario === 'binary') {
            const raw = Uint8Array.of(255 - state);
            return { req: { method: 'POST', url: '/state', raw, body: new TextDecoder().decode(raw) } };
          }
          const req = { method: 'GET', url: scenario === 'empty' ? '' : '/state', cancel: true };
          if (scenario === 'held') Object.assign(req, { held: true, http: 'ordered held' });
          return { req, store };
        };
        rt.data.fetch = req => new Promise(resolve => reads.push({ resolve, snapshot: state, forced: req.forced, origin: req.origin, url: req.url }));
        rt.data.parse = (_, [topic, seq], reply) => {
          if (reply.failed) throw Error('offline');
          if (scenario === 'continuation' && !continued) {
            continued = true;
            return { req: { storage: 'read' } };
          }
          if (store) rt.Store.set('seen', 'yes');
          return { v: `${reply.snapshot}@${topic}/${seq}` };
        };
        rt.mount(() => {
          region = rt.scope(() => {
            value = rt.res('view', 'load', args, undefined, undefined, 's', 'waiting');
            rt.effect(() => { frames.push([value(), value.c(), value.p(), value.f()]); });
          });
        });
        const event = (mixed = false) => {
          state++;
          rt.Origin.run('action', () => rt.commit(() => { rt.W(seq, state); if (mixed) rt.W(topic, 'b'); }));
        };
        if (scenario === 'identity') rt.commit(() => rt.W(topic, 'b'));
        else event(scenario === 'mixed');
        if (scenario === 'burst') { event(); event(); }
        if (scenario.startsWith('topic-')) rt.commit(() => rt.sourceChanged(value.r, 'topic'));
        refuse = ['refusal', 'topic-refusal'].includes(scenario);
        if (scenario === 'idle') rt.commit(() => rt.W(active, false));
        if (scenario === 'unmount') rt.commit(() => rt.end(region));
        // The transport returns its snapshot even if an idle or removed view let it go.
        for (let i = 0; i < reads.length && i < 5; i++) {
          // Two events per 200 ms snapshot, in delivery order, through the second read.
          if (scenario === 'continuous' && i < 2) {
            if (i === 1) event();
            event();
          }
          if (reads[i].url === undefined) reads[i].snapshot = reads[0].snapshot;
          reads[i].resolve(scenario === 'failure' && i === 0 ? { failed: 1 } : { snapshot: reads[i].snapshot });
          await Promise.resolve();
          await Promise.resolve();
        }
        return { frames, snapshots: reads.filter(r => r.url !== undefined).map(r => r.snapshot), forced: reads.some(r => r.forced), origin: reads.at(-1)?.origin, attempted: rt.Store.get('attempted'),
          follows: rt.journal.filter(l => l.includes('with') && l.includes('asked again')).length };
      }, scenario);
      const inactive = ['idle', 'unmount'].includes(scenario), identity = scenario === 'identity';
      const snapshots = scenario === 'continuous' ? [0, 2, 4] : scenario === 'burst' ? [0, 3]
        : inactive || identity || scenario.endsWith('refusal') ? [0] : [0, 1];
      expect(result.snapshots).toEqual(snapshots);
      expect(result.attempted).toBeUndefined();
      expect(result.forced).toBe(scenario === 'topic-store');
      if (snapshots.length > 1) expect(result.origin).toBe('action');
      const replaced = scenario === 'binary';
      expect(result.follows).toBe(replaced ? 0 : scenario.endsWith('refusal') ? 1 : snapshots.length - 1);
      if (inactive) {
        expect(result.frames.every(([text]) => text === 'waiting')).toBe(true);
      } else if (scenario.endsWith('refusal')) {
        expect(result.frames.at(-1)).toEqual(['0@a/1', true, false, true]);
      } else {
        const latest = snapshots.at(-1), topic = identity || scenario === 'mixed' ? 'b' : 'a';
        expect(result.frames.at(-1)).toEqual([`${latest}@${topic}/${latest}`, true, false, false]);
        if (snapshots.length > 1 && !replaced && scenario !== 'failure') {
          expect(result.frames).toContainEqual([`0@${topic}/${snapshots[1]}`, true, true, false]);
          expect(result.frames.slice(result.frames.findIndex(f => f[1])).every(f => f[1])).toBe(true);
        }
      }
    } finally {
      await page?.close();
      await browser?.close();
      server.stop(true);
    }
  });

test.each(['argument', 'unmount', 'refusal', 'persist'])("a parser's store write settles before its kept read follows up (%s)", async mode => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async mode => {
      const rt = await import('/host/web-js/rt.js');
      const seq = rt.sig(0), asks = [], reads = [], writes = [];
      let revision, child, snapshot;
      rt.clock.agent = true;
      rt.data.answer = (source, args, store) => {
        if (source === 'revision') return { v: Number(store.get('revision') ?? 0), store: true };
        if (source === 'persist') return args[0] === 'waiting' ? { v: 0 } : { req: { storage: args[0] } };
        if (source === 'guard') {
          if (args[0] === 'refused') throw Object.assign(Error('dependent refused'), { refuse: true });
          return { v: 0 };
        }
        asks.push(args);
        if (mode === 'persist' && store.get('revision')) return { v: 'new' };
        if (mode === 'refusal' && store.get('revision')) return { v: 'refused' };
        return { req: { method: 'GET', url: '/snapshot' } };
      };
      rt.data.fetch = req => {
        if (req.storage) { writes.push(req.storage); return Promise.resolve({}); }
        return new Promise(resolve => reads.push(resolve));
      };
      rt.data.parse = source => {
        if (source === 'persist') return { v: 0 };
        rt.Store.set('revision', '2');
        return { v: 'snapshot' };
      };
      rt.mount(() => {
        revision = rt.res('revision', 'revision', () => [], undefined, undefined, 'n', 0);
        child = rt.scope(() => {
          const args = Object.assign(() => [seq(), mode === 'argument' ? revision() : 0], { context: 2 });
          snapshot = rt.res('snapshot', 'load', args, undefined, undefined, 's', 'waiting');
          rt.effect(() => snapshot());
          if (mode === 'persist') {
            const saved = rt.res('saved', 'persist', () => [snapshot()], undefined, undefined, 'n', 0);
            rt.effect(() => saved());
          }
          if (mode === 'refusal') {
            const guard = rt.res('guard', 'guard', () => [snapshot()], undefined, undefined, 'n', 0);
            rt.effect(() => guard());
          }
        });
        if (mode === 'unmount') rt.effect(() => { if (revision()) rt.end(child); });
      });
      rt.commit(() => rt.W(seq, 1));
      for (let i = 0; i < reads.length && i < 4; i++) {
        reads[i]({});
        await Promise.resolve();
        await Promise.resolve();
      }
      rt.commit(() => {});
      return { asks, writes, reads: reads.length, value: snapshot.r.value, failure: snapshot.r.error };
    }, mode);
    expect(result.reads).toBe(mode === 'argument' ? 2 : 1);
    const asks = [[0, 0], [1, 0]];
    if (mode !== 'unmount') asks.push([1, mode === 'argument' ? 2 : 0]);
    expect(result.asks).toEqual(asks);
    if (mode === 'persist') {
      expect(result.writes).toEqual(['snapshot', 'new']);
      expect(result.value).toBe(result.writes.at(-1));
    }
    if (mode === 'refusal') {
      expect(result.value).toBe('snapshot');
      expect(result.failure).toContain('dependent refused');
    }
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
});

test.each(['refresh', 'topic', 'store', 'handoff'])('the refresh policy survives a deferred kept reply (%s)', async mode => {
  const server = runtimeServer(true), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    const { asks, transports } = await page.evaluate(async mode => {
      const rt = await import('/host/web-js/rt.js');
      const seq = rt.sig(0), asks = [], transports = [], deferred = [];
      let read, view, update, frozen = false;
      rt.clock.agent = true;
      rt.data.answer = (_, args, store, name, mutation, forced) => {
        asks.push([args[0], forced, rt.Origin.now()]);
        return { req: { method: 'GET', url: '/state', cancel: true }, store: mode === 'store' };
      };
      rt.data.fetch = req => new Promise(resolve => { read = resolve; transports.push(req.forced); });
      rt.data.parse = () => ({ v: 5 });
      rt.mount(() => {
        view = rt.res('view', 'load', Object.assign(() => [seq()], { context: 1 }), undefined, undefined, 'n', 0);
        rt.effect(() => view());
      });
      const change = () => rt.commit(() => rt.W(seq, 1));
      if (mode !== 'handoff') {
        change();
        if (mode === 'topic') rt.commit(() => rt.sourceChanged(view.r, 'topic'));
        if (mode === 'store') rt.Store.peer('key', 'new');
        document.startViewTransition = cb => { update = cb; return { ready: new Promise(() => {}), finished: new Promise(() => {}) }; };
        rt.pr(); await rt.pieces(); rt.useCommands({ startViewTransition: () => () => {} });
        rt.commit(() => rt.C('startViewTransition', []));
      } else {
        globalThis.__exactDevReplacement = { defer(f) { if (frozen) deferred.push(f); return frozen; } };
        frozen = true;
        deferred.push(change);
      }
      read({});
      await Promise.resolve();
      if (mode !== 'handoff') {
        if (mode === 'refresh') {
          for (const origin of ['action', 'tool']) rt.Origin.run(origin, () => rt.commit(() => rt.R(view)));
        } else rt.commit(() => rt.W(seq, 2));
        update();
      } else {
        frozen = false;
        for (const f of deferred) f();
      }
      return { asks, transports };
    }, mode);
    if (mode === 'topic' || mode === 'store') {
      expect(transports).toEqual([false, true]);
      expect(asks.at(-1).slice(0, 2)).toEqual([2, true]);
    } else expect(asks).toEqual([[0, false, 'app'], [1, false, 'app'], ...(mode === 'refresh'
      ? [[1, true, 'action'], [1, true, 'tool']] : [[1, false, 'app']])]);
  } finally {
    await page.close();
    await browser.close();
    server.stop(true);
  }
});
