import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { fixture } from './surface.mjs';

test('async wake schedules a live frame without spinning under a frozen clock', async () => {
  let now = null, dirty = false, woke = false, renders = 0;
  const f = await fixture({
    gpu: {
      gpu_dirty: () => dirty || woke,
      gpu_dirty_no_wake: () => dirty,
      gpu_render: () => {
        renders += 1;
        dirty = false;
        woke = false;
        return 0;
      },
    },
  });
  f.exact.now = () => now;
  // Live mode (`exact.now` unset): `wake()` schedules a frame via microtask.
  delete f.exact.now;
  f.create(1);
  await f.exact.gpu.settled();
  renders = 0;
  woke = true;
  f.exact.gpu.wake();
  await Promise.resolve();
  f.frame();
  assert.equal(renders, 1, 'live wake schedules and draws one frame');
  assert.equal(woke, false);

  // Frozen agent clock: `wake()` stays latched; `settled(false)` does not render it.
  now = 100;
  f.exact.now = () => now;
  await f.exact.gpu.settled(false);
  renders = 0;
  woke = true;
  f.exact.gpu.wake();
  await Promise.resolve();
  assert.equal(renders, 0, 'frozen clock wake does not schedule wall-clock renders');
  assert.deepEqual(await f.exact.gpu.settled(false), []);
  assert.equal(renders, 0, 'non-clock inspection leaves pure wake latched');
  assert.equal(woke, true);

  // Explicit clock step (`settled(true)`) renders a woken surface once, even if it re-arms.
  f.gpu.gpu_render = () => {
    renders += 1;
    dirty = false;
    woke = true; // Re-arm during render: must still terminate in one render.
    return 0;
  };
  assert.deepEqual(await f.exact.gpu.settled(true), []);
  assert.equal(renders, 1, 'clock settle renders re-arming wake once without spinning');

  // Non-wake dirty state still settles on `settled(false)`.
  renders = 0;
  dirty = true;
  woke = false;
  f.gpu.gpu_render = () => {
    renders += 1;
    dirty = false;
    return 0;
  };
  assert.deepEqual(await f.exact.gpu.settled(false), []);
  assert.equal(renders, 1, 'non-wake dirty state renders during regular settle');
});
