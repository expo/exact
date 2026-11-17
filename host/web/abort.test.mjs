import { test, expect } from 'bun:test';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runTests } from '../../scripts/agent-test.mjs';
import { abortServer, TESTS, within } from '../../scripts/abort-fixture.mjs';
import { chromium, suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { hostFixtureFile } from './tests/glue-fixture.mjs';
import { modulePrelude } from '../web-js/glue.mjs';

const chromeTest = need('chrome', chromium().unavailable);

for (const wrappedAny of [false, true]) for (const placement of ['main', 'worker']) chromeTest(`a suppressed abort cancels a ${placement} realm fetch before its reply${wrappedAny ? ' with wrapped AbortSignal.any' : ''}`, async () => {
  const received = Promise.withResolvers(), closed = Promise.withResolvers(), reply = Promise.withResolvers();
  let released = false;
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, idleTimeout: 0, async fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/held') {
      received.resolve();
      req.signal.addEventListener('abort', () => closed.resolve());
      await reply.promise;
      return new Response('late reply');
    }
    if (path === '/') return new Response('<!doctype html><body></body>', { headers: { 'content-type': 'text/html' } });
    // The page loads the prelude's scripts as the web build joins them (host/web-js/glue.mjs `modulePrelude`).
    if (path === '/module-prelude.js') return new Response(modulePrelude(resolve(import.meta.dir, '../..')), { headers: { 'content-type': 'text/javascript' } });
    const source = path === '/module-binary.js' ? '../../js/src/binary.js' : path.slice(1);
    if (!existsSync(resolve(import.meta.dir, source))) return new Response('not found', { status: 404 });
    return new Response(hostFixtureFile(resolve(import.meta.dir, source)), { headers: { 'content-type': 'text/javascript' } });
  } });
  const browser = await suiteBrowser(), context = await browser.newContext(), page = await context.newPage();
  try {
    await page.goto(server.url.href);
    const held = await page.evaluate(async ({ placement, wrappedAny }) => {
      globalThis.exact = {};
      const { prepare, call, run } = await import('/module-glue.js');
      const { createGrantSet } = await import('/grant-admission.js');
      const grants = `net.fetch ${location.origin}`, url = new URL(location.origin);
      const set = { version: 1, entries: [[1, grants, ['fetch', url.protocol.slice(0, -1), url.hostname, Number(url.port)], null]], error: null };
      let seal = 14695981039346656037n;
      for (const b of new TextEncoder().encode(JSON.stringify(set))) seal = BigInt.asUintN(64, (seal ^ BigInt(b)) * 1099511628211n);
      const identity = { appId: 'test.suppressed-abort', grants, grantSet: createGrantSet({ ...set, seal: seal.toString(16).padStart(16, '0') }), placement };
      const script = new TextEncoder().encode(`
        let controller, result = 'pending', stopped = 0, added = 0, removed = 0;
        const add = AbortSignal.prototype.addEventListener, remove = AbortSignal.prototype.removeEventListener;
        AbortSignal.prototype.addEventListener = function(type, ...args) {
          if (type === 'abort' && this !== controller?.signal) added++;
          return add.call(this, type, ...args);
        };
        AbortSignal.prototype.removeEventListener = function(type, ...args) {
          if (type === 'abort' && this !== controller?.signal) removed++;
          return remove.call(this, type, ...args);
        };
        ${wrappedAny ? `
        const compose = AbortSignal.any;
        AbortSignal.any = function(signals) {
          const signal = compose.call(this, signals);
          add.call(signal, 'abort', event => event.stopImmediatePropagation());
          return signal;
        };
        ` : ''}
        globalThis.exact = { abi: 1, appId: 'test.suppressed-abort', grants: ${JSON.stringify(grants)}, answer(source) {
          if (source === 'hold') {
            controller = new AbortController();
            controller.signal.addEventListener('abort', event => { stopped++; event.stopImmediatePropagation(); });
            return fetch(${JSON.stringify(location.origin + '/held')}, { method: 'POST', body: 'held', signal: controller.signal })
              .then(() => result = 'fulfilled', error => result = error === controller.signal.reason ? error.message : 'wrong reason');
          }
          if (source === 'abort') { controller.abort(new Error('by the app')); return stopped; }
          return { result, added, removed };
        }};
      `);
      const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', script)), b => b.toString(16).padStart(2, '0')).join('');
      const receipt = new TextEncoder().encode(JSON.stringify({ version: 1, abi: 1, appId: identity.appId, grants,
        module: { sha256: 'a'.repeat(64) }, web: { file: 'app.js', bytes: script.length, sha256: digest } }));
      const realm = await prepare({ script, receipt }, identity);
      const invoke = async (source, outcome) => {
        let answer = call({ id: realm.id, op: outcome ? 'resume' : 'answer', source, args: [], store: [], grants: [], outcome });
        while (answer.continuation) {
          call({ op: 'dispatch', id: realm.id, token: answer.continuation, store: [], grants: [] });
          answer = await run(answer.continuation);
        }
        return answer;
      };
      const held = await invoke('hold'), wire = new AbortController();
      // Drive the loader's ticket seam directly, with a real HTTP reply held by the local server.
      const network = fetch(held.request.url, { method: held.request.method, body: held.request.body, signal: wire.signal }).catch(error => error.name);
      globalThis.probe = { realm, invoke, call, wire, network };
      return { ticket: held.ticket, url: held.request.url };
    }, { placement, wrappedAny });
    expect(held.url).toBe(server.url.origin + '/held');
    await received.promise;
    const aborted = await page.evaluate(async () => {
      const { realm, invoke, call, wire } = probe;
      let woke = false;
      const tickets = [];
      exact.abortWake = () => {
        woke = true;
        tickets.push(...call({ op: 'aborted', id: realm.id }).tickets);
        if (tickets.length) wire.abort();
      };
      const stopped = (await invoke('abort')).value;
      // A task checkpoint in either placement has drained the rejection before this answer reads it.
      const result = (await invoke('result')).value;
      return { stopped, result, tickets, woke };
    });
    expect(aborted).toEqual({ stopped: 1, result: { result: 'by the app', added: 1, removed: 1 }, tickets: [held.ticket], woke: true });
    expect(released).toBe(false);
    expect(await page.evaluate(() => probe.network)).toBe('AbortError');
    await closed.promise;
    expect(released).toBe(false);
    released = true;
    reply.resolve();
    expect(await page.evaluate(async () => {
      const late = await probe.invoke('hold', { response: { status: 200, body: 'late reply', headers: [] } });
      const tickets = probe.call({ op: 'aborted', id: probe.realm.id }).tickets;
      probe.realm.dispose();
      return { value: late.value, tickets };
    })).toEqual({ value: 'by the app', tickets: [] });
  } finally {
    reply.resolve();
    await context.close();
    server.stop(true);
  }
}, 30_000);

// A fetch the app aborts, on the wasm target (scripts/abort-fixture.mjs): the realm reports it (door 23), the
// runner answers it aborted in the commit, and glue.js cancels the page's request, so the server sees its
// connection close; the aborted answer goes on, and the next request goes out at once. Against a built dist:
// `bun scripts/abort-fixture.mjs native <dir> [port]`, then its wasm build (the line that prints), then
// EXACT_ABORT_WASM_DIST=<dist> EXACT_ABORT_PORT=<port> (47713; one run at a time per port).
test.skipIf(!process.env.EXACT_ABORT_WASM_DIST)('an aborted fetch is cancelled on the wire, on the wasm target', async () => {
  const dist = resolve(process.env.EXACT_ABORT_WASM_DIST);
  let server, page, dir;
  try {
    server = abortServer({ port: Number(process.env.EXACT_ABORT_PORT ?? 47713) });
    page = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      const file = Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    dir = mkdtempSync(resolve(import.meta.dir, '../../target/abort-wasm-'));
    const file = resolve(dir, 'app.test.contract');
    writeFileSync(file, TESTS);
    const ran = await runTests({ host: 'web', file, url: page.url.href, webDist: dist });
    expect(ran.results.map(r => [r.name, r.failures])).toEqual(ran.results.map(r => [r.name, []]));
    expect(ran.results.length).toBe(1);
    expect(server.seen).toEqual(['/held', '/next']);
    expect(await within(2000, () => server.closed.includes('/held'))).toBe(true);
  } finally {
    page?.stop(true);
    server?.stop();
    if (dir) rmSync(dir, { recursive: true, force: true });
  }
}, 300_000);
