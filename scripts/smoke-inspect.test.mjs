import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { formControlsSmoke, isDeckURL } from './smoke-inspect.mjs';

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
