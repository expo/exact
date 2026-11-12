import { test, expect } from 'bun:test';
import { existsSync } from 'node:fs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

function runtimeServer() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') { return new Response(null, { status: 204 }); }
    if (path.endsWith('/names.js')) { return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } }); }
    if (path === '/') { return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } }); }
    const local = new URL('../..' + path, import.meta.url), file = existsSync(local) ? local : new URL('../web/' + path.split('/').pop(), import.meta.url);
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
}

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
