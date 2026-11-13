// A scroll drag on the web (navigation.js `scrollDrag`; Charlie, 2026-11-10):
// a touch down and the scroller's offset moving begin one, the last touch
// lifting ends it with UIKit's velocity, CSS px per ms and positive as the
// offset grows. The function is read from the source and run against stand-in
// listeners and events, each with the timeStamp the browser would give it.
import { expect, test } from 'bun:test';

import { scrollDrag } from '../navigation/scroll.js';

const OUTSIDE = {};
const scroller = () => {
  // A touch's `target` is `OUTSIDE` when it began outside the scroller.
  const el = { scrollLeft: 0, scrollTop: 0, contains: target => target !== OUTSIDE }, heard = {}, said = [], options = [];
  scrollDrag(el, (type, fn, o) => { heard[type] = fn; options.push(o); }, () => said.push(['start']), (vx, vy) => said.push(['end', vx, vy]));
  // `touches`: the fingers down after the event, as [identifier, x, y, target];
  // `changed`: those it is about (a lift's are no longer in `touches`).
  const finger = ([identifier, clientX, clientY, target = el]) => ({ identifier, clientX, clientY, target });
  const touch = (type, t, touches, changed = touches) => heard[type]({ timeStamp: t, touches: touches.map(finger), changedTouches: changed.map(finger) });
  const scroll = (left, top, t) => { el.scrollLeft = left; el.scrollTop = top; heard.scroll({ timeStamp: t }); };
  return { said, options, touch, scroll };
};

test('the release velocity is the finger\'s, on its own clock, however late the page\'s scroll events come', () => {
  const s = scroller();
  // A finger moving up the screen at 1 px per ms for 200 ms: the content
  // scrolls down, so scrollTop grows and the velocity is positive.
  s.touch('touchstart', 0, [[1, 100, 350]]);
  for (let t = 16; t <= 192; t += 16) {
    s.touch('touchmove', t, [[1, 100, 350 - t]]);
    // The frame that shows the move comes later, as a page under load (or a
    // driver's real-time steps) delivers it: `scroll`'s timeStamp is no
    // measure of the finger.
    s.scroll(0, t - 15, 200 + 2.7 * t);
  }
  s.touch('touchend', 200, [], [[1, 100, 150]]);
  expect(s.said).toEqual([['start'], ['end', 0, 1]]);
  // Momentum after the lift is no drag.
  s.scroll(0, 400, 900);
  expect(s.said.length).toBe(2);
  expect(s.options.every(o => o.passive === true)).toBe(true);
});

test('a drag back toward the top is negative, a second finger holds the drag, and a finger that rested before lifting is 0', () => {
  const s = scroller();
  s.scroll(300, 0, 0);
  s.touch('touchstart', 0, [[1, 50, 100]]);
  for (let t = 10; t <= 110; t += 10) { s.touch('touchmove', t, [[1, 50 + 2 * t, 100]]); s.scroll(300 - 2 * t, 0, t + 40); }
  s.touch('touchstart', 115, [[1, 270, 100], [2, 10, 10]], [[2, 10, 10]]);
  s.touch('touchend', 116, [[2, 10, 10]], [[1, 270, 100]]);
  expect(s.said).toEqual([['start']]);
  s.touch('touchend', 120, [], [[2, 10, 10]]);
  // The second finger, followed from its own down, never moved.
  expect(s.said).toEqual([['start'], ['end', 0, 0]]);
  const r = scroller();
  r.touch('touchstart', 0, [[1, 50, 100]]);
  for (let t = 10; t <= 110; t += 10) { r.touch('touchmove', t, [[1, 50 + 2 * t, 100]]); r.scroll(300 - 2 * t, 0, t + 40); }
  r.touch('touchend', 120, [], [[1, 290, 100]]);
  // Rightward at 2 px per ms: scrollLeft shrinks.
  expect(r.said).toEqual([['start'], ['end', -2, 0]]);
  r.touch('touchstart', 1000, [[1, 50, 300]]);
  r.touch('touchmove', 1010, [[1, 50, 250]]); r.scroll(80, 50, 1012);
  r.touch('touchmove', 1030, [[1, 50, 210]]); r.scroll(80, 90, 1032);
  r.touch('touchcancel', 1500, [], [[1, 50, 210]]);
  expect(r.said.slice(2)).toEqual([['start'], ['end', 0, 0]]);
});

test('an axis the drag did not scroll is 0, and a scroll with no touch down or a touch that never scrolls sends nothing', () => {
  const s = scroller();
  s.scroll(0, 100, 10);
  s.touch('touchstart', 20, [[1, 0, 0]]);
  s.touch('touchend', 40, [], [[1, 0, 0]]);
  expect(s.said).toEqual([]);
  s.touch('touchstart', 100, [[1, 100, 300]]);
  for (let t = 116; t <= 200; t += 16) { s.touch('touchmove', t, [[1, 100 + (t - 100) / 2, 300 - (t - 100)]]); s.scroll(0, t, t + 5); }
  s.touch('touchend', 200, [], [[1, 150, 200]]);
  // The finger drifted sideways, but the scroller moved only down the page.
  expect(s.said).toEqual([['start'], ['end', 0, 1]]);
});

// `TouchEvent.touches` is every finger on the surface (Touch Events, `touches`), and the scroller hears only its own
// fingers' events: a finger resting outside it must neither hold its drag open nor begin one at a later scroll.
test('a finger outside the scroller neither holds its drag open nor makes a later scroll a drag', () => {
  const s = scroller();
  s.touch('touchstart', 0, [[1, 100, 300]]);
  for (let t = 16; t <= 96; t += 16) { s.touch('touchmove', t, [[1, 100, 300 - t]]); s.scroll(0, t, t + 5); }
  // A finger lands on the header (the scroller hears nothing of it), then the scroller's lifts.
  s.touch('touchend', 100, [[2, 10, 10, OUTSIDE]], [[1, 100, 204]]);
  expect(s.said).toEqual([['start'], ['end', 0, 1]]);
  s.touch('touchstart', 500, [[3, 100, 300]]);
  for (let t = 16; t <= 96; t += 16) { s.touch('touchmove', 500 + t, [[3, 100, 300 - t]]); s.scroll(0, 100 + t, 505 + t); }
  s.touch('touchend', 600, [], [[3, 100, 204]]);
  expect(s.said.slice(2)).toEqual([['start'], ['end', 0, 1]]);
  // A tap while a finger rests outside, then a programmatic scroll with no finger down.
  const r = scroller();
  r.touch('touchstart', 0, [[2, 10, 10, OUTSIDE], [1, 100, 300]], [[1, 100, 300]]);
  r.touch('touchend', 50, [[2, 10, 10, OUTSIDE]], [[1, 100, 300]]);
  r.scroll(0, 100, 600);
  expect(r.said).toEqual([]);
});
