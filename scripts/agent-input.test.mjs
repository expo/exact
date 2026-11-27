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

// An Android keyboard that leaves between the reveal and the press gives the page its height back and the navigation
// bar its room, which can park the revealed target under the bar: the host refuses the press as covered.
const coveredOnce = ({ moves = true, refusals = 1 } = {}) => {
  const calls = [], box = { x: 16, y: 1038, w: 328, h: 52 }, viewport = { w: 360, h: 708 };
  const node = { id: 248, props: { testId: 'car-save' }, depth: 3, type: 'Pressable' };
  let refused = 0;
  const carrier = { host: 'android', browser: 'chrome', launchFacts: launchFacts({}), reset() {}, close() {},
    async ask(req) {
      if (req.op === 'layout') return { nodes: [{ id: 248, ...box }], viewport, ...(req.id === 248 ? { node: { ...node, space: { viewport: { ...box } } } } : {}) };
      return req.op === 'clock' ? { clock: 0 } : { nodes: [node] };
    },
    async input(id, kind, opts) {
      calls.push([kind, opts.at]);
      if (refused++ < refusals) throw new Error(`view ${id} is covered or not hit at its projected center`);
      return { at: [180, 584.67] };
    },
    async reveal() {
      calls.push(['reveal']);
      if (!moves && calls.length > 1) return { scrolled: false };
      const from = box.y + 26; box.y = 558.67; return { scrolled: true, from: [180, from], to: [180, box.y + 26] };
    },
  };
  return { calls, carrier };
};

test('a press refused as covered after the window moved its target is revealed again and pressed once more', async () => {
  const { calls, carrier } = coveredOnce();
  const s = await open({ reuse: carrier });
  s.landed = async r => r;
  const reply = await s.tap(248);
  assert.deepEqual(calls, [['reveal'], ['press', undefined], ['reveal'], ['press', undefined]]);
  assert.deepEqual(reply.at, [180, 584.67]);
  assert.ok(reply.revealedAgain, 'the reply says the target was revealed again');
});

test('a press that stays covered, with nothing to scroll it clear, is refused as before', async () => {
  const { calls, carrier } = coveredOnce({ moves: false, refusals: 2 });
  const s = await open({ reuse: carrier });
  s.landed = async r => r;
  await assert.rejects(() => s.tap(248), /covered or not hit at its projected center/);
  assert.deepEqual(calls, [['reveal'], ['press', undefined], ['reveal']]);
});

test('a press refused as covered twice is refused, revealed again only once', async () => {
  const { calls, carrier } = coveredOnce({ refusals: 2 });
  const s = await open({ reuse: carrier });
  s.landed = async r => r;
  await assert.rejects(() => s.tap(248), /covered or not hit at its projected center/);
  assert.deepEqual(calls, [['reveal'], ['press', undefined], ['reveal'], ['press', undefined]]);
});

test('a press at a named point is never pressed again: the point is the author\'s', async () => {
  const { calls, carrier } = coveredOnce();
  const s = await open({ reuse: carrier });
  s.landed = async r => r;
  await assert.rejects(() => s.tap(248, { at: [10, 10] }), /covered or not hit at its projected center/);
  assert.equal(calls.filter(([k]) => k === 'press').length, 1);
});
