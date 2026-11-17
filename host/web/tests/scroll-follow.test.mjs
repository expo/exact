// An authored scroll owns a followed scroller until it settles (navigation.js
// `scrollFollowers`; LLP 1070.000 §15, expo/exact#370): the end-follow moves
// nothing meanwhile, `scrollFollowEnd` turning true included, and where the
// scroll rests decides whether the end is followed. Run against stand-in
// elements, a stand-in ResizeObserver and a hand-driven timer.
import { expect, test } from 'bun:test';

import { scrollFollowers } from '../navigation/scroll.js';

const observers = [];
globalThis.ResizeObserver = class {
  constructor(fn) { this.fn = fn; observers.push(this); }
  observe() {}
  disconnect() {}
};

/** A scroller `port` tall over `height` of content, at `top`, with a follower; the timer is advanced by hand. */
const setup = ({ height = 1000, port = 100, top = 900 } = {}) => {
  const heard = {}, timers = new Map();
  let next = 0;
  const child = {};
  const el = {
    scrollTop: top, scrollHeight: height, clientHeight: port, isConnected: true, children: [],
    contains: (e) => e === child,
    addEventListener: (type, fn) => { heard[type] = fn; },
    removeEventListener: (type) => { delete heard[type]; },
  };
  const f = scrollFollowers(() => {}, (fn) => { timers.set(++next, fn); return next; }, (t) => timers.delete(t));
  const enable = () => {
    observers.length = 0;
    f.followScroll(el, true);
    return f.followedScrolls.get(el);
  };
  // The content grows: the browser's ResizeObserver, then the follow.
  const grow = (by) => { el.scrollHeight += by; for (const o of observers) o.fn(); };
  // The scroll moves the port (a frame of an animation, or its last).
  const scroll = (to) => { el.scrollTop = to; heard.scroll(); };
  const quiet = () => { for (const [t, fn] of [...timers]) { timers.delete(t); fn(); } };
  return { el, child, f, enable, grow, scroll, heard, quiet };
};

test('a follower at its end follows content that grows', () => {
  const s = setup();
  const followed = s.enable();
  s.f.rememberScroll(followed);
  s.grow(100);
  expect(s.el.scrollTop).toBe(1000);
});

test('an authored scroll owns the scroller: growth mid-flight is not followed, and it rests where it lands', () => {
  const s = setup();
  const followed = s.enable();
  s.f.rememberScroll(followed);
  // A smooth reveal of an element in it, away from the end.
  expect(s.f.own(s.child)).toEqual([followed]);
  s.grow(100);
  expect(s.el.scrollTop).toBe(900);
  s.scroll(600);
  s.grow(100);
  s.scroll(300);
  expect(s.el.scrollTop).toBe(300);
  s.heard.scrollend();
  expect(followed.owned).toBe(null);
  expect(followed.end).toBe(false);
  s.grow(100);
  expect(s.el.scrollTop).toBe(300);
});

test('a scroll that rests at the end leaves it followed, and turning the follow on with it does not jump first', () => {
  const s = setup({ top: 200 });
  // `scrollFollowEnd` turns true in the batch that asks for the scroll: a
  // new follower stands at the end, and an authored scroll owns it before
  // the follow settles, so the port is not put at the end ahead of it.
  const followed = s.enable();
  s.f.own(s.el);
  s.f.settleFollow(followed);
  expect(s.el.scrollTop).toBe(200);
  s.scroll(500);
  s.grow(50);
  expect(s.el.scrollTop).toBe(500);
  s.scroll(950);
  s.heard.scrollend();
  expect(followed.end).toBe(true);
  s.grow(100);
  expect(s.el.scrollTop).toBe(1050);
});

test('without a scrollend, the scroll settles once quiet', () => {
  const s = setup();
  const followed = s.enable();
  s.f.rememberScroll(followed);
  s.f.own(s.child);
  s.scroll(900);
  expect(followed.owned).not.toBe(null);
  s.quiet();
  expect(followed.owned).toBe(null);
  expect(followed.end).toBe(true);
  s.grow(100);
  expect(s.el.scrollTop).toBe(1000);
});
