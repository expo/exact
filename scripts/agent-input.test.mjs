// Geometry used to address input, including nodes the presenter does not mirror.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { open } from './agent.mjs';
import { launchFacts } from './agent-launch.mjs';

test('a point on a viewless iOS node uses its layout and is revealed before input', async () => {
  const calls = [], box = { x: 10, y: 1000, w: 44, h: 44 }, viewport = { w: 420, h: 900 };
  let layoutOnly = true;
  const node = { id: 1, props: {}, depth: 0, type: 'View' };
  const carrier = { host: 'ios', browser: 'chrome', launchFacts: launchFacts({}), reset() {},
    async ask(req) {
      if (req.op === 'layout') return { nodes: [], viewport,
        ...(req.id === 1 ? { node: { ...node, space: { viewport: { ...box } }, native: { layoutOnly } } } : {}) };
      return req.op === 'clock' ? { clock: 0 } : { nodes: [node] };
    },
    async input(id, kind, opts) { calls.push([kind, opts.at]); return { at: [32, 406] }; },
    async reveal() { calls.push(['reveal']); const from = box.y; box.y = 400; return { scrolled: true, from, to: box.y }; },
  };
  const s = await open({ reuse: carrier });
  s.landed = async r => r;
  const reply = await s.tap(1, { at: [22, 6] });
  assert.deepEqual(reply.scrolled, { from: 1000, to: 400 });
  assert.deepEqual(calls, [['reveal'], ['press', [22, 6]]]);
  calls.length = 0;
  await assert.rejects(() => s.tap(1, { at: [44, 0] }), /inside the target box/);
  assert.deepEqual(calls, []);
  layoutOnly = false;
  await assert.rejects(() => s.tap(1, { at: [22, 6] }), /inside the target box/);
  assert.deepEqual(calls, []);
});
