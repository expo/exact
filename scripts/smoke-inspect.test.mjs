import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { chromium, suiteBrowser } from './agent-launch.mjs';
import { need } from './coverage.mjs';
import { formControlsSmoke, isDeckURL, rebooted } from './smoke-inspect.mjs';

need('chrome', chromium().unavailable)('a reload waits for the replacement document to define and resolve runtime readiness', async () => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    if (new URL(req.url).pathname !== '/') return new Response('', { status: 404 });
    return new Response(`<div id="exact-root" data-boot-ms="1"></div><script>
      globalThis.defineRuntime = () => { globalThis.exact = { agentSettled() {}, ready: new Promise(done => {
        globalThis.finishRuntime = () => { globalThis.launchFacts = location.search; done(); };
      }) }; };
    </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  const browser = await suiteBrowser(), page = await browser.newPage();
  try {
    const evaluate = expression => page.evaluate(expression);
    await page.goto(`${server.url}?locale=fr-CA&seed=42`);
    await evaluate('defineRuntime(); finishRuntime()');
    for (let n = 0; n < 3; n++) {
      const before = await evaluate('performance.timeOrigin');
      let finished = false, observed, observedRuntime, first = true;
      const checked = new Promise(done => { observed = done; });
      const runtimeChecked = new Promise(done => { observedRuntime = done; });
      const carrier = { evaluate: async expression => {
        const result = await evaluate(expression);
        if (first) {
          first = false;
          // Commit the replacement between the old document's readiness read and its reply.
          await page.reload();
          observed();
        } else if (await evaluate('!!globalThis.exact?.ready')) observedRuntime();
        return result;
      } };
      const ready = rebooted(carrier, before).then(() => { finished = true; });
      await checked;
      await evaluate('true');
      assert.equal(await evaluate('typeof exact'), 'undefined');
      assert.equal(finished, false, 'the old document cannot establish readiness for the replacement');
      await evaluate('defineRuntime()');
      await runtimeChecked;
      await evaluate('true');
      assert.equal(finished, false, 'the replacement runtime has not resolved readiness');
      await evaluate('finishRuntime()');
      await ready;
      assert.equal(await evaluate('launchFacts'), '?locale=fr-CA&seed=42');
    }
  } finally {
    await page.close();
    await browser.close();
    await server.stop(true);
  }
}, 30_000);

test('Apple keyboard smokes use the resolved app name while drives keep the app selector', () => {
  const root = resolve(import.meta.dir, '..');
  for (const [host, selector, inherited, name] of [
    ['ios', 'apps/messages', undefined, 'messages'],
    ['macos', 'messages-apple', undefined, 'messages'],
    ['ios', undefined, 'apps/messages', 'messages'],
    ['ios', 'apps/messages-stress', undefined, 'messages-stress'],
    ['ios', 'apps/exact-live', undefined, 'exact-live'],
  ]) {
    // Module mocks stay in a child so other tests retain the real driver and smoke helpers.
    const script = `
      import assert from 'node:assert/strict';
      import { mock } from 'bun:test';
      import * as agent from './scripts/agent.mjs';
      import * as inspect from './scripts/smoke-inspect.mjs';
      const selector = ${JSON.stringify(selector) ?? 'undefined'}, seen = [], done = new Error('keyboard route checked');
      const session = {
        tree: async () => ({ roots: [1], nodes: [{ id: 1, props: {} }] }),
        layout: async () => ({ viewport: { w: 400, h: 800 }, nodes: [{ id: 1 }] }),
        close: async () => { throw done; },
      };
      mock.module('./scripts/agent.mjs', () => ({ ...agent,
        open: async (options) => { assert.equal(options.app, selector); return session; },
      }));
      mock.module('./scripts/smoke-inspect.mjs', () => ({ ...inspect,
        agree: async () => {},
        messageKeys: async (s, options) => { assert.equal(s, session); seen.push(options.app); throw done; },
      }));
      delete process.env.EXACT_APP_DIR;
      const inherited = ${JSON.stringify(inherited) ?? 'undefined'};
      if (inherited) process.env.EXACT_APP_DIR = inherited;
      process.argv = [process.execPath, './scripts/smoke.mjs', ${JSON.stringify(host)},
        ...(selector ? ['--app', selector] : []), '--app-only'];
      try { await import('./scripts/smoke.mjs'); assert.fail('did not reach keyboard route'); }
      catch (error) { assert.equal(error, done); }
      assert.deepEqual(seen, [${JSON.stringify(name)}]);
      assert.equal(process.env.EXACT_APP_DIR, inherited);
    `;
    const r = spawnSync(process.execPath, ['-e', script], { cwd: root, encoding: 'utf8', timeout: 20_000 });
    assert.equal(r.status, 0, `${host} ${selector ?? inherited}: ${r.stdout}${r.stderr}${r.error ?? ''}`);
  }
}, 120_000);

test('the deck smoke resolves frame URLs within the document base', () => {
  assert.equal(isDeckURL('/deck/index.html', null), true);
  assert.equal(isDeckURL('./deck/index.html', null), false);
  const origin = 'https://exact.invalid';
  for (const path of ['/', `/.exact/gen/${'a'.repeat(32)}/`,
    `/caltrain/.exact/gen/${'b'.repeat(32)}/`, `/.exact/root/web/releases/${'c'.repeat(64)}/`]) {
    const base = origin + path, deck = new URL('deck/index.html', base);
    for (const source of ['./deck/index.html', 'deck/index.html', deck.href, deck.pathname]) {
      assert.equal(isDeckURL(source, base), true, `${source} at ${base}`);
    }
    assert.equal(isDeckURL('/deck/index.html', base), path === '/', base);
    assert.equal(isDeckURL('../deck/index.html', base), path === '/', base);
    for (const source of [undefined, null, '', 'http://[', './other/index.html',
      './deck/index.html?other', './deck/index.html#other', '//other.invalid/deck/index.html',
      `https://other.invalid${deck.pathname}`, `${origin}/.exact/gen/${'d'.repeat(32)}/deck/index.html`]) {
      assert.equal(isDeckURL(source, base), false, `${source} at ${base}`);
    }
  }
});

test('the form-control smoke waits for a chooser that presents after the tap returns', async () => {
  const failures = [], pictures = [];
  await formControlsSmoke({ host: 'ios', check: (ok, why) => { if (!ok) failures.push(why); return ok; },
    open: async ({ touch }) => {
      const slots = {};
      let tapped = false, reads = 0;
      return {
        tap: async () => { tapped = true; return touch ? { delivery: 'platform' } : { delivery: 'unsupported', reason: 'use --touch platform' }; },
        type: async (target, value) => {
          if (value === 'unavailable') throw new Error('disabled');
          slots[target] = value;
        },
        state: async () => ({ slots }),
        screenshot: async () => {
          if (!tapped) return {};
          pictures.push(++reads);
          return reads < 3 ? {} : { presented: 'popover' };
        },
        close: async () => {},
      };
    } });
  assert.deepEqual(failures, []);
  assert.deepEqual(pictures, [1, 2, 3, 1, 2, 3]);
}, 600_000); // The fixture's compile builds the contract binary first on a fresh checkout.
