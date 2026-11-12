import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { formControlsSmoke, isDeckURL } from './smoke-inspect.mjs';

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
