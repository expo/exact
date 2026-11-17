import { test, expect } from 'bun:test';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { faultRequest, open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

// Capture the timer's delay, so observations never depend on a remaining countdown.
function captureRetryWaits() {
  globalThis.retryWaits = [];
  let pending;
  const timeout = globalThis.setTimeout;
  globalThis.setTimeout = (fn, ms, ...args) => {
    if (pending) { retryWaits.push({ ...pending, wait: ms }); pending = null; }
    return timeout(fn, ms, ...args);
  };
  Object.defineProperty(AbortController.prototype, 'retrying', { configurable: true, set(value) {
    Object.defineProperty(this, 'retrying', { configurable: true, writable: true, value });
    pending = value;
  } });
}

async function until(read, predicate, what) {
  const deadline = performance.now() + 20000;
  for (;;) {
    const value = await read();
    if (predicate(value)) return value;
    if (performance.now() > deadline) throw new Error(`${what}: ${JSON.stringify(value)}`);
    await new Promise(r => setTimeout(r, 50));
  }
}

test('driver faults fail individual long-poll attempts and recovery resets their backoff in a browser', async () => {
  const root = resolve(import.meta.dir, '../..'), asked = [];
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/') return new Response(`<script type="module">
      import { readLongPoll } from '/host/web/http-body.js';
      import { faultOp, faultsJson } from '/host/web/faults.js';
      (${captureRetryWaits.toString()})();
      window.controller = new AbortController();
      window.sent = []; window.faultOp = faultOp; window.faultsJson = faultsJson;
      const url = location.origin + '/changes?since=0';
      faultOp({ fail: url, times: 2 });
      window.start = () => { window.done = readLongPoll({ url, longPoll: { cursor: 'since', from: 'seq', sameCursorEvent: false } },
        { method: 'GET' }, 64, m => sent.push(m), controller, () => true); };
    </script>`, { headers: { 'content-type': 'text/html' } });
    if (path === '/changes') {
      asked.push(new URL(req.url).search);
      return new Response('{"seq":1}', { headers: { 'content-type': 'application/json' } });
    }
    return new Response(Bun.file(resolve(root, path.slice(1))));
  } });
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.clock.install();
    await page.goto(server.url.href);
    await page.clock.pauseAt(await page.evaluate(() => Date.now()) + 100);
    await page.evaluate(() => { start(); });
    const first = await page.evaluate(() => ({ ...controller.retrying, left: controller.retrying.at - performance.now(), faults: faultsJson() }));
    expect(first.error).toContain('fetch failed (driver fault):');
    expect(first.left).toBeGreaterThanOrEqual(800);
    expect(first.left).toBeLessThanOrEqual(1200);
    expect(first.faults[0]).toMatchObject({ left: 1, hits: 1 });
    expect(asked).toEqual([]);
    await page.clock.runFor(first.left);
    const second = await page.evaluate(() => ({ left: controller.retrying.at - performance.now(), faults: faultsJson() }));
    expect(second.left).toBeGreaterThanOrEqual(1600);
    expect(second.left).toBeLessThanOrEqual(2400);
    expect(second.faults[0]).toMatchObject({ left: 0, hits: 2 });
    expect(asked).toEqual([]);
    // Match the cursor after recovery; the original prefix must not match this ask.
    await page.evaluate(() => faultOp({ fail: location.origin + '/changes?since=1', times: 1 }));
    await page.clock.runFor(second.left);
    const recovered = await until(
      () => page.evaluate(() => ({ sent, attempt: controller.retrying?.attempt, left: controller.retrying?.at - performance.now(), faults: faultsJson() })),
      value => value.sent.length === 1 && value.attempt === 1, 'recovery and backoff reset');
    expect(recovered.sent[0].id).toBe('1');
    expect(recovered.left).toBeGreaterThanOrEqual(800);
    expect(recovered.left).toBeLessThanOrEqual(1200);
    expect(recovered.faults.map(f => f.hits)).toEqual([2, 1]);
    expect(await page.evaluate(() => retryWaits.map(r => ({ attempt: r.attempt, wait: r.wait }))))
      .toEqual([{ attempt: 1, wait: first.left }, { attempt: 2, wait: second.left }, { attempt: 1, wait: recovered.left }]);
    expect(asked).toEqual(['?since=0']);
    await page.evaluate(async () => { faultOp({ fail: location.origin + '/changes?since=1' }); controller.abort(); window.end = await done; });
    expect(await page.evaluate(() => ({ kind: end.kind, retrying: controller.retrying, hits: faultsJson()[1].hits })))
      .toEqual({ kind: 4, retrying: undefined, hits: 0 });
    expect(asked).toEqual(['?since=0']);
  } finally { await page.close(); await browser.close(); server.stop(true); }
}, 20000);

// LLP 1016.000 D8, end to end (theme2's ruling of 2026-10-22): an app whose
// `live` resource is a long poll and whose `view` re-reads `with live`,
// against a stand-in that counts requests and serves an event, a heartbeat,
// a 503 with Retry-After, a second event, a same-cursor heartbeat, then holds.
// The view's re-reads track the events one for one, heartbeats cause none,
// the wait shows as `retrying`, and new arguments close the held request.
// Run on the web JS target, an app's web build (the wasm target is a game's
// and needs a crate of its own: its op is host/web/src/batch_tests.rs's).
for (const [carrier, build] of [['web-js', 'host/web-js/build.mjs']]) {
  test(`a long poll's events re-read a resource one for one, heartbeats none, on ${carrier}`, async () => {
    if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
    const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/longpoll-test-'));
    const answers = [
      { body: '{"seq":1}' }, { status: 204 }, { status: 503, headers: { 'retry-after': '2' } },
      { body: '{"seq":2}' }, { body: '{"seq":2}' },
    ];
    const polls = [], loads = [], held = [], release = [];
    const cors = { 'access-control-allow-origin': '*', 'access-control-expose-headers': 'retry-after' };
    const peer = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const url = new URL(req.url);
      if (url.pathname === '/load') { loads.push(url.searchParams.get('at')); return new Response('loaded ' + url.searchParams.get('at'), { headers: cors }); }
      polls.push(url.search);
      // gen 2 answers one heartbeat, then holds until the test answers it (`release`).
      const gen2 = url.searchParams.get('gen') === '2' && !polls.slice(0, -1).some(p => p.includes('gen=2'));
      const a = url.searchParams.get('gen') === '1' ? answers.shift() : gen2 ? { status: 204 } : null;
      if (!a) return new Promise((resolve) => { held.push(req); release.push(() => resolve(new Response('{"seq":7}', { headers: { ...cors, 'content-type': 'application/json' } }))); req.signal.addEventListener('abort', () => resolve(new Response(null, { status: 204, headers: cors }))); });
      return new Response(a.body ?? null, { status: a.status ?? 200, headers: { ...cors, 'content-type': 'application/json', ...a.headers } });
    } });
    const origin = `http://127.0.0.1:${peer.port}`, nextURL = `${origin}/changes?since=1&gen=1`;
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Long Poll Test', app: { id: 'test.longpoll', name: 'Long Poll Test' }, host: { web: {} }, rust: false }));
    writeFileSync(resolve(dir, 'app.contract'), `component LongPollTest
  state gen = 1
  resource live = changes(gen) as shape string else empty()
  resource view = load() with live as shape string else empty()
  action next
    gen = gen + 1
  view
    column
      text live testId="live"
      text view testId="view"
      button press=next testId="next"
        text "next"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.longpoll';
export const grants = ${JSON.stringify(`net.fetch ${origin}`)};
export function answer(name: string, args: any[]): any {
  if (name === 'changes') return fetch(${JSON.stringify(origin)} + '/changes?since=0&gen=' + args[0], {
    exactStream: (e: any) => e.type === 'error' ? 'ended ' + e.kind : e.lastEventId,
    exactLongPoll: { cursor: 'since', from: 'seq' },
  } as RequestInit);
  if (name === 'load') return fetch(${JSON.stringify(origin)} + '/load?at=' + args[0]).then((r: any) => r.text());
  return '';
}
`);
    let browser, server;
    try {
      const built = Bun.spawn([process.execPath, build, 'longpoll-test', '--out', resolve(dir, 'dist'), '--render', 'none'], {
        cwd: root, env: { ...process.env, EXACT_APP_DIR: dir, EXACT_WEB_DIST: resolve(dir, 'dist') }, stdout: 'pipe', stderr: 'pipe',
      });
      const [status, stdout, stderr] = await Promise.all([built.exited, new Response(built.stdout).text(), new Response(built.stderr).text()]);
      expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
      // A file the build did not write (`exact.json`, which only a bake has) is a 404.
      server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
        const path = new URL(req.url).pathname, file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
        if (!await file.exists()) return new Response('not found', { status: 404 });
        if (path === '/') return new Response(`<script>(${captureRetryWaits.toString()})();</script>` + await file.text(), { headers: { 'content-type': 'text/html' } });
        return new Response(file);
      } });
      browser = await open({ host: 'web', url: server.url.href, webDist: resolve(dir, 'dist'), failFetch: `${origin}/changes?since=0\t2\n${nextURL}` });
      const live = s => s.streams?.find(x => x.name === 'live');
      const stateUntil = (predicate, what) => until(() => browser.state(), predicate, what);
      const reset = await stateUntil(s => s.resources.live === '1' && s.faults?.[1]?.hits >= 1, 'a fault after recovery');
      expect(reset.faults[0]).toMatchObject({ hits: 2, left: 0 });
      expect(live(reset).retrying.error).toContain('fetch failed (driver fault):');
      expect(polls).toEqual(['?since=0&gen=1']);
      const waits = await browser.carrier.evaluate(() => retryWaits);
      expect(waits.slice(0, 3).map(r => r.attempt)).toEqual([1, 2, 1]);
      for (const [r, lower, upper] of [[waits[0], 800, 1200], [waits[1], 1600, 2400], [waits[2], 800, 1200]]) {
        expect(r.error).toContain('fetch failed (driver fault):');
        expect(r.wait).toBeGreaterThanOrEqual(lower);
        expect(r.wait).toBeLessThanOrEqual(upper);
      }
      expect(waits[2].error).toContain(nextURL);
      expect((await browser.logs()).lines.filter(l => l.includes('fetch failed (driver fault)')).length).toBeGreaterThanOrEqual(3);
      await browser.op(faultRequest('pass', ['fetch', nextURL]));
      await browser.clock('data');
      const done = await stateUntil(s => s.resources.view === 'loaded 2' && held.length === 1, 'the second event');
      expect(done.resources.live).toBe('2');
      expect(live(done).retrying).toBeUndefined();
      expect(await browser.carrier.evaluate(() => retryWaits.find(r => r.error === 'HTTP 503')?.attempt)).toBe(1);
      expect(polls).toEqual(['?since=0&gen=1', '?since=1&gen=1', '?since=1&gen=1', '?since=1&gen=1', '?since=2&gen=1', '?since=2&gen=1']);
      // One load for each value of live (its placeholder's, if the view asked before the first event), none for a heartbeat.
      expect(loads.filter(x => x !== '')).toEqual(['1', '2']);
      // New arguments let the long poll go: the request it held is aborted.
      await browser.tap('next');
      await stateUntil(() => held.length === 2 && held[0].signal.aborted, 'the held request aborted');
      expect(polls.slice(-2)).toEqual(['?since=0&gen=2', '?since=0&gen=2']);
      // Its first reply a heartbeat, the poll is held: `clock settle` waits on it no more, state names it held,
      // and `clock held live` returns on its next reply, an event here.
      const settled = await browser.clock('settle');
      expect(settled.settled).not.toBe(false);
      const named = await browser.state();
      expect(named.pending).toContainEqual(expect.objectContaining({ name: 'live', held: true }));
      setTimeout(() => release.at(-1)(), 300);
      const heldReply = await browser.clock('held live 10s');
      expect(heldReply.settled).not.toBe(false);
      expect((await stateUntil(s => s.resources.live === '7', 'the held reply')).resources.live).toBe('7');
    } finally {
      await browser?.close(); server?.stop(true); peer.stop(true); rmSync(dir, { recursive: true, force: true });
    }
  }, 240000);
}
