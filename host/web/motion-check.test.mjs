// The motion checker on timelines built by hand (`motion-check.mjs`, LLP 1063 D10): where a box is shown comes from
// formulas written out here, not from the checker's reference, so that the reference is held to them; each way of
// breaking a promise is told apart.
import { test, expect } from 'bun:test';
import { checkMoves, eased } from './motion-check.mjs';

const FRAME = 1000 / 60, LINEAR = { ms: 300, delay: 0, easing: 'linear' };

test('easings are CSS’s', () => {
  expect(eased('ease', 0.5)).toBeCloseTo(0.8024033877, 6);
  expect(eased('cubic-bezier(0, 0, 1, 1)', 0.3)).toBeCloseTo(0.3, 6);
  expect([eased('linear', 0.3), eased('ease-in-out', 0.5), eased('ease-in', 0.5), eased('ease-out', 0.5)].map(x => Math.round(x * 1e4) / 1e4)).toEqual([0.3, 0.5, 0.3154, 0.6846]);
  // `linear()`: an input left out is spaced between its neighbours, and an output may pass 1
  expect(eased('linear(0, 0.25 75%, 1)', 0.5)).toBeCloseTo(0.25 * 0.5 / 0.75, 9);
  expect(eased('linear(0, 1.2, 1)', 0.75)).toBeCloseTo(1.1, 9);
  expect(eased('linear(0 0%, 0.5 20% 60%, 1 100%)', 0.4)).toBeCloseTo(0.5, 9);
  // Outside its stops, the line through the two nearest goes on
  expect([eased('linear(0 25%, 1 75%)', 0), eased('linear(0 25%, 1 75%)', 1)]).toEqual([-0.5, 1.5]);
  expect([eased('steps(4)', 0.5), eased('steps(1, jump-start)', 0)]).toEqual([null, null]);
  // A stop's percentage may come before its output; a stop it cannot read is no curve
  expect(eased('linear(0, 50% 0.5, 1)', 0.5)).toBeCloseTo(0.5, 9);
  expect([eased('linear(0, 0.5 50% 60% 70%, 1)', 0.5), eased('linear(0,,1)', 0.5), eased('cubic-bezier', 0.5), eased('linear(0, 1.)', 0.5), eased('cubic-bezier(2, 0, 1, 1)', 0.5), eased('linear(0)', 0.5), eased('linear(0, 20% .5 60%, 1)', 0.5)])
    .toEqual([null, null, null, null, null, null, null]);
  // Two first stops at one input, once the omitted inputs are filled in, are no curve a progress alone can say
  expect([eased('linear(0 0%, 1 0%, 1 100%)', 0.5), eased('linear(0, 0.5, 1 0%, 1 100%)', 0.5)]).toEqual([null, null]);
  // (Two at one input with one output are one point)
  expect(eased('linear(0 0%, 0 0%, 1 100%)', 0.5)).toBeCloseTo(0.5, 9);
  expect(eased('linear(0 0%, 0 0%, 1 0%, 1 100%)', 0.5)).toBeNull();
});

// A box laid at (0, 0) and moved by commits at `at` to each of `to` in turn, sampled each frame from its first commit
// until well after it settles; `show(t, k)` is where it is shown, `moving(t, k)` whether anything is left of its moves
function timeline({ curve = LINEAR, anchor = 'box', at = [50], to = [[100, 0, 40, 40]], show, moving, until = 600 }) {
  const commits = [{ t: 0, laid: { a: [0, 0, 40, 40] } }, ...at.map((t, i) => ({ t, laid: { a: to[i] } }))];
  const samples = [], sample = (k, t) => samples.push({ k, t, nodes: { a: { shown: show(t, k), moving: moving(t, k) } } });
  sample(1, 0);
  for (const [i, { t }] of commits.entries()) {
    if (i) sample(i, t);
    const next = commits[i + 1]?.t ?? until;
    for (let u = t; u < next; u += FRAME) sample(i + 1, u);
  }
  sample(commits.length, until);
  return { nodes: { a: { curve, anchor } }, commits, samples };
}

// The linear move from (0, 0) to (100, 0) at 50 ms, over 300 ms
const linear = t => Math.min(1, Math.max(0, (t - 50) / 300));
const moved = (extra = {}) => timeline({ show: (t, k) => k === 1 ? [0, 0, 40, 40] : [100 * linear(t), 0, 40, 40], moving: (t, k) => k === 2 && t < 350, ...extra });

test('a move that keeps every promise passes', () => {
  expect(checkMoves(moved())).toEqual([]);
});

test('a move after a delay holds the box where it was until the delay ends', () => {
  const curve = { ...LINEAR, delay: 100 }, held = t => Math.min(1, Math.max(0, (t - 150) / 300));
  const keep = timeline({ curve, show: (t, k) => k === 1 ? [0, 0, 40, 40] : [100 * held(t), 0, 40, 40], moving: (t, k) => k === 2 && t < 450 });
  expect(checkMoves(keep)).toEqual([]);
  const eager = timeline({ curve, show: (t, k) => k === 1 ? [0, 0, 40, 40] : [100 * linear(t), 0, 40, 40], moving: (t, k) => k === 2 && t < 450 });
  expect(checkMoves(eager)).toContainEqual(expect.stringMatching(/where its moves put it/));
});

test('a spring’s interruption adds the new move to the one under way, and an easing’s restarts from where it is shown', () => {
  // Retargeted at 200 ms to (200, 0), 50 points along. Added, the first move goes on to its end at 350 ms while the
  // second plays the change of 100 points until 500 ms; restarted, as CSS restarts a transition, one move plays the 150
  // points left from 200 ms to 500 ms
  const second = t => Math.min(1, Math.max(0, (t - 200) / 300)), at = [50, 200], to = [[100, 0, 40, 40], [200, 0, 40, 40]];
  const added = (t, k) => k === 1 ? [0, 0, 40, 40] : k === 2 ? [100 * linear(t), 0, 40, 40] : [100 * linear(t) + 100 * second(t), 0, 40, 40];
  const restarted = (t, k) => k < 3 ? added(t, k) : [50 + 150 * second(t), 0, 40, 40];
  const moving = (t, k) => k > 1 && t < 500, spring = { ...LINEAR, spring: true };
  expect(checkMoves(timeline({ curve: spring, at, to, show: added, moving }))).toEqual([]);
  expect(checkMoves(timeline({ at, to, show: restarted, moving }))).toEqual([]);
  const off = /^a: at 2\d\d(\.\d+)? ms is shown at .* where its moves put it/;
  expect(checkMoves(timeline({ curve: spring, at, to, show: restarted, moving }))).toContainEqual(expect.stringMatching(off));
  expect(checkMoves(timeline({ at, to, show: added, moving }))).toContainEqual(expect.stringMatching(off));
});

test('a curve the check does not model is named, and a curve that starts partway is no jump', () => {
  expect(checkMoves(moved({ curve: { ...LINEAR, easing: 'steps(4)' } }))).toEqual([expect.stringMatching(/^a: its curve steps\(4\) is not one the check models/)]);
  // Retargeted to where it is shown, on a curve it does not model, a box that rests at once is not taken to rest early
  const steps = { ...LINEAR, easing: 'steps(2, jump-end)' }, at = [50, 200], to = [[100, 0, 40, 40], [50, 0, 40, 40]];
  const stepped = timeline({ curve: steps, at, to, show: (t, k) => k === 3 ? [50, 0, 40, 40] : [t < 200 ? 0 : 50, 0, 40, 40], moving: (t, k) => k === 2 });
  expect(checkMoves(stepped)).toEqual([expect.stringMatching(/is not one the check models/)]);
  // Each commit's latest end bounds its own stretch: something left from 150 ms, after a 100 ms move from 50 ms, is named
  // though the box moves again at 450 ms
  const quick = { ...steps, ms: 100 }, again = [[100, 0, 40, 40], [200, 0, 40, 40]];
  const leaked = timeline({ curve: quick, at: [50, 450], to: again, show: (t, k) => k === 1 ? [0, 0, 40, 40] : k === 2 ? [t < 100 ? 0 : 100, 0, 40, 40] : [t < 500 ? 100 : 200, 0, 40, 40],
    moving: (t, k) => k === 2 ? t < 300 : k === 3 && t < 550 });
  expect(checkMoves(leaked)).toEqual([expect.stringMatching(/is not one the check models/), expect.stringMatching(/^a: at 150 ms, past its last move's end at 150 ms.* something of a move is left/)]);
  const partway = { ...LINEAR, easing: 'linear(0.5, 1)' }, half = t => 0.5 + 0.5 * linear(t);
  expect(checkMoves(moved({ curve: partway, show: (t, k) => k === 1 ? [0, 0, 40, 40] : [100 * half(t), 0, 40, 40] }))).toEqual([]);
  // After a delay too: the curve's start fills the delay
  const late = { ...partway, delay: 100 }, after = t => 0.5 + 0.5 * Math.min(1, Math.max(0, (t - 150) / 300));
  expect(checkMoves(timeline({ curve: late, show: (t, k) => k === 1 ? [0, 0, 40, 40] : [100 * after(t), 0, 40, 40], moving: (t, k) => k === 2 && t < 450 }))).toEqual([]);
});

test('an easing retargeted to where the box is shown has nothing to play', () => {
  // Halfway at 200 ms, 50 points along, retargeted to 50; and at 260 ms, 70 points along, where the share left is a
  // float that leaves no exact zero
  for (const [t, x] of [[200, 50], [260, 70], [349.97, 99.99]]) {
    const at = [50, t], to = [[100, 0, 40, 40], [x, 0, 40, 40]], there = (u, k) => k === 3 ? [x, 0, 40, 40] : [100 * linear(u), 0, 40, 40];
    expect(checkMoves(timeline({ at, to, show: there, moving: (u, k) => k === 2 }))).toEqual([]);
    expect(checkMoves(timeline({ at, to, show: there, moving: (u, k) => k > 1 && u < t + 300 }))).toContainEqual(expect.stringMatching(/something of a move is left/));
  }
});

test('an easing restarts with what was left to the last bit the rounding of its sums allows', () => {
  // From 0 to 128 over 1000 ms on a curve that is 2^-48 past halfway at 50 percent, retargeted there to 64: 2^-41 points
  // are left, which at 87.5 percent of the restart, 2^48 along, are 128 points back
  const curve = { ms: 1000, delay: 0, easing: `linear(0 0%, ${2 ** -47} 25%, 1 75%, ${2 ** 48} 87.5%, 1 100%)` };
  const E = t => eased(curve.easing, Math.min(1, Math.max(0, t / 1000))), left = 2 ** -41;
  const show = (t, k) => [k === 1 ? 0 : k === 2 ? 128 * E(t - 50) : 64 + left * (1 - E(t - 550)), 0, 40, 40];
  const back = timeline({ curve, at: [50, 550], to: [[128, 0, 40, 40], [64, 0, 40, 40]], show, moving: (t, k) => k === 2 || k === 3 && t < 1550, until: 1700 });
  expect(checkMoves(back)).toEqual([]);
  expect(show(1425, 3)[0]).toBeCloseTo(-64, 3);
});

test('a sample without its lifetime, or with content it cannot read, is refused', () => {
  const unread = moved();
  delete unread.samples[3].nodes.a.moving;
  expect(checkMoves(unread)).toEqual([expect.stringMatching(/^a: a commit or a sample has no box, lifetime or content/)]);
  for (const content of [[], Array(4)]) {
    const empty = moved();
    empty.samples[3].nodes.a.content = content;
    expect(checkMoves(empty)).toEqual([expect.stringMatching(/^a: a commit or a sample has no box, lifetime or content/)]);
  }
  const none = moved();
  delete none.samples[3].nodes;
  expect(checkMoves(none)).toEqual([expect.stringMatching(/^a: a commit or a sample has no box, lifetime or content/)]);
  // A box with no declaration, even one only its commits name, or a curve or floor it cannot read, is named
  expect(checkMoves({ ...moved(), nodes: {} })).toEqual([expect.stringMatching(/^a: the timeline records it, and declares no curve/)]);
  const laidOnly = moved();
  laidOnly.nodes = {};
  for (const s of laidOnly.samples) delete s.nodes.a;
  expect(checkMoves(laidOnly)).toEqual([expect.stringMatching(/^a: the timeline records it, and declares no curve/)]);
  expect(checkMoves({ ...moved(), nodes: { a: { curve: { ms: 300 } } } })).toEqual([expect.stringMatching(/^a: its curve is neither null/)]);
  for (const floor of [[0], [, 0]]) expect(checkMoves({ ...moved(), nodes: { a: { curve: LINEAR, floor } } })).toEqual([expect.stringMatching(/^a: its floor is not two sizes/)]);
});

test('a box’s content is at its shown corner at its laid size', () => {
  const content = (t, k) => [k === 1 ? 0 : 100 * linear(t), 0, 40, 40];
  const kept = moved();
  for (const s of kept.samples) s.nodes.a.content = content(s.t, s.k);
  expect(checkMoves(kept)).toEqual([]);
  for (const s of kept.samples) s.nodes.a.content = [0, 0, 40, 40];
  expect(checkMoves(kept)).toContainEqual(expect.stringMatching(/^a: at .* shows its content at 0,0,40,40, not at its shown corner/));
});

test('a jump, a box held still, and a box moved with no row are named', () => {
  expect(checkMoves(moved({ show: (t, k) => k === 1 ? [0, 0, 40, 40] : [100, 0, 40, 40] }))).toContainEqual(expect.stringMatching(/^a: jumps at the commit at 50 ms/));
  // Held where it was until the end, then put at its place: no jump at the commit, yet off its curve all along
  expect(checkMoves(moved({ show: (t, k) => k === 1 || t < 350 ? [0, 0, 40, 40] : [100, 0, 40, 40] }))).toContainEqual(expect.stringMatching(/is shown at 0,0,40,40, where its moves put it at/));
  expect(checkMoves(moved({ curve: null, moving: () => false }))).toContainEqual(expect.stringMatching(/where its layout puts it/));
});

test('a move that falls short, leaves something behind, or rests early is named', () => {
  expect(checkMoves(moved({ show: (t, k) => k === 1 ? [0, 0, 40, 40] : [Math.min(99, 100 * linear(t)), 0, 40, 40] })))
    .toContainEqual(expect.stringMatching(/past its last move's end .* is shown at 99,0,40,40 laid at 100,0,40,40/));
  expect(checkMoves(moved({ moving: (t, k) => k === 2 }))).toContainEqual(expect.stringMatching(/something of a move is left/));
  expect(checkMoves(moved({ moving: (t, k) => k === 2 && t < 300 }))).toContainEqual(expect.stringMatching(/rests before its last move ends/));
});

test('a size the move would take below zero is shown at zero', () => {
  // Grown from 0 to 40 wide on a curve that starts by going back: its share left passes 1, and its width 0
  const curve = { ms: 300, delay: 0, easing: 'linear(0, -0.5 50%, 1)' }, share = t => eased(curve.easing, linear(t));
  const grown = timeline({ curve, to: [[0, 0, 40, 40]], show: (t, k) => [0, 0, k === 1 ? 0 : Math.max(0, 40 * share(t)), 40], moving: (t, k) => k === 2 && t < 350 });
  grown.commits[0].laid.a = [0, 0, 0, 40];
  expect(checkMoves(grown)).toEqual([]);
});

test('a native button takes its new size at once, and moves by its centre', () => {
  // From a 40-wide box at 0 to a 60-wide one at 90: its centre goes from 20 to 120
  const centre = t => 20 + 100 * linear(t), to = [[90, 0, 60, 40]];
  const button = timeline({ anchor: 'centre', to, show: (t, k) => k === 1 ? [0, 0, 40, 40] : [centre(t) - 30, 0, 60, 40], moving: (t, k) => k === 2 && t < 350 });
  expect(checkMoves(button)).toEqual([]);
  const growing = timeline({ anchor: 'centre', to, show: (t, k) => k === 1 ? [0, 0, 40, 40] : [centre(t) - 25, 0, 50, 40], moving: (t, k) => k === 2 && t < 350 });
  expect(checkMoves(growing)).toContainEqual(expect.stringMatching(/not its laid size 60,40/));
});

test('each box is checked though another broke a promise, and a timeline without the evidence is refused', () => {
  const both = moved({ show: (t, k) => k === 1 ? [0, 0, 40, 40] : [100, 0, 40, 40] });
  both.nodes.b = both.nodes.a;
  for (const c of both.commits) c.laid.b = c.laid.a;
  for (const s of both.samples) s.nodes.b = s.nodes.a;
  const errors = checkMoves(both);
  expect(['a', 'b'].map(id => errors.some(e => e.startsWith(`${id}: jumps`)))).toEqual([true, true]);
  expect(checkMoves({ ...moved(), samples: [] })).toEqual([expect.stringMatching(/nothing to check/)]);
  const blind = moved();
  blind.samples = blind.samples.filter(s => !(s.k === 1 && s.t === 50));
  expect(checkMoves(blind)).toEqual([expect.stringMatching(/no frame on both sides of the commit at 50 ms/)]);
  const short = moved();
  short.samples = short.samples.filter(s => s.t < 300);
  expect(checkMoves(short)).toContainEqual(expect.stringMatching(/the recording ends at .* before its last move ends/));
});
