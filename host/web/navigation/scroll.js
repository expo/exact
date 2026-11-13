// The eager scrollFollowEnd projection also belongs to this DOM controller.
export function scrollFollowers(positionContexts, schedule = (fn, ms) => setTimeout(fn, ms), cancel = (t) => clearTimeout(t)) {
  // An explicit chat/log policy, not CSS overflow anchoring: keep the end
  // visible across resizing only while the reader is already there.
  const followedScrolls = new Map();
  function rememberScroll(s) {
    s.top = s.el.scrollTop;
    s.height = s.el.scrollHeight;
    s.port = s.el.clientHeight;
    s.end = s.top >= s.height - s.port - 1;
  }
  // A scroll the app makes (`scrollIntoView`, a `scrollTop` write) owns the
  // offset until it settles, as UIKit's `setContentOffset(_:animated:)` runs
  // on to its target: the follow moves nothing meanwhile, `scrollFollowEnd`
  // turning true included, and where the scroll rests decides whether the
  // end is followed (LLP 1070.000 §15). It settles at its `scrollend`, or
  // 300 ms after its last move (§2.6's quiet interval), which covers a
  // browser without `scrollend` and a scroll that moved nothing.
  const QUIET_MS = 300;
  function quiet(s) {
    cancel(s.owned.quiet);
    s.owned.quiet = schedule(() => release(s), QUIET_MS);
  }
  function release(s) {
    if (!s.owned) {
      return;
    }
    cancel(s.owned.quiet);
    s.owned = null;
    rememberScroll(s);
  }
  /** The followed scrollers an authored scroll of `el` (a scroller, or an element in them) moves, now owned by it. */
  function own(el) {
    const owned = [];
    for (const s of followedScrolls.values()) {
      if (s.el === el || s.el.contains(el)) {
        s.owned ??= { quiet: null };
        quiet(s);
        owned.push(s);
      }
    }
    return owned;
  }
  function settleFollow(s) {
    if (!s.el.isConnected) {
      return;
    }
    // A reader above the end uses the browser's CSS scroll anchoring. Writing the remembered numeric offset here would
    // undo its adjustment when content above the visible message changes.
    if (s.end && !s.owned) {
      s.el.scrollTop = s.el.scrollHeight - s.el.clientHeight;
    }
    rememberScroll(s);
    const children = [...s.el.children];
    if (children.length !== s.children.length || children.some((el, i) => el !== s.children[i])) {
      s.observer.disconnect();
      s.observer.observe(s.el);
      for (const child of children) {
        s.observer.observe(child);
      }
      s.children = children;
    }
  }
  function followScroll(el, enabled) {
    const old = followedScrolls.get(el);
    if (old || !enabled) {
      if (old && !enabled) {
        old.observer.disconnect();
        el.removeEventListener("scroll", old.scrolled);
        el.removeEventListener("scrollend", old.ended);
        if (old.owned) {
          cancel(old.owned.quiet);
        }
        followedScrolls.delete(el);
      }
      return;
    }
    const s = {
      el,
      top: 0,
      height: 0,
      port: 0,
      end: true,
      owned: null,
      children: []
    };
    // Where the port is, before a commit changes the content and on a
    // `scroll`. ResizeObserver settles a changed geometry before a queued scroll
    // notification is allowed to change whether the reader follows the end.
    s.observed = () => {
      if (el.scrollHeight === s.height && el.clientHeight === s.port) {
        rememberScroll(s);
      }
    };
    // Only a move renews an authored scroll's quiet interval: a commit that
    // moves nothing does not hold the follow off.
    s.scrolled = () => {
      if (s.owned) {
        quiet(s);
      }
      s.observed();
    };
    s.ended = () => release(s);
    s.observer = new ResizeObserver(() => {
      settleFollow(s);
      positionContexts();
    });
    s.observer.observe(el);
    el.addEventListener("scroll", s.scrolled, { passive: true });
    el.addEventListener("scrollend", s.ended, { passive: true });
    followedScrolls.set(el, s);
  }
  return {
    followedScrolls,
    followScroll,
    settleFollow,
    rememberScroll,
    own,
    release
  };
}
export function scrollDrag(el, listen, began, ended) {
  const o = { passive: true };
  let down = 0, finger = null, dragging = false, from = [0, 0], seen = [];
  // `touches` is every finger on the surface; one outside `el` is no part of its drag, and its lift is never heard here.
  const own = (e) => [...e.touches].filter((t) => el.contains(t.target));
  const follow = (e) => {
    if (!down) {
      from = [el.scrollLeft, el.scrollTop];
      finger = null;
    }
    const touches = own(e);
    down = touches.length;
    if (!touches.some((t) => t.identifier === finger)) {
      finger = touches[0]?.identifier ?? null;
      seen = [];
    }
  };
  const at = (e, t) => {
    seen.push([
      e.timeStamp,
      t.clientX,
      t.clientY
    ]);
    while (e.timeStamp - seen[0][0] > 200) {
      seen.shift();
    }
  };
  listen("touchstart", follow, o);
  listen("touchmove", (e) => {
    const t = [...e.touches].find((t) => t.identifier === finger);
    if (t) {
      at(e, t);
    }
  }, o);
  listen("scroll", () => {
    if (down && !dragging) {
      dragging = true;
      began();
    }
  }, o);
  const lift = (e) => {
    if (own(e).length) {
      return follow(e);
    }
    down = 0;
    if (!dragging) {
      return;
    }
    dragging = false;
    const b = seen.at(-1), a = b && (seen.findLast((s) => b[0] - s[0] >= 100) ?? seen[0]), dt = b && e.timeStamp - b[0] < 100 ? b[0] - a[0] : 0, axis = (i, moved) => moved && dt > 0 ? (a[i] - b[i]) / dt : 0;
    seen = [];
    ended(axis(1, el.scrollLeft !== from[0]), axis(2, el.scrollTop !== from[1]));
  };
  listen("touchend", lift, o);
  listen("touchcancel", lift, o);
}
// A scroll drag (Charlie, 2026-11-10), the web's nearest to UIKit's `scrollViewWillBeginDragging` and `scrollViewWillEndDragging`: with a touch down on `el`, its offset moving begins one (`began()`, once); the last touch lifting ends it, `ended(vx, vy)`: the finger's velocity over the last 100 ms of its moves, as UIKit's is the pan's, in CSS px per ms, negated so it is positive as scrollLeft and scrollTop grow; 0 on an axis the drag did not scroll, or when the finger rested 100 ms before lifting. The finger's own touch events, one clock: `scroll` events come at frame times, which trail the touches by the page's input latency. A mouse, a wheel or a programmatic scroll is no drag, and the browser never says where a fling will rest. Passive: the browser's scrolling is untouched.
