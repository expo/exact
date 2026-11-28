import { expect, mock, test } from 'bun:test';

mock.module('./navigation.js', () => ({
  shownStacks: () => new Map(),
  holdRoutes: () => () => {},
  inertAncestor: () => false,
}));

// A ViewTransition whose update callback never runs (e.g. compositor/GPU loss skipping the
// transition) must still drain pending commit tails so later UI commits never wedge.
test('a rejected ViewTransition drains pending tails and lets later commits run', async () => {
  const prev = {
    document: globalThis.document,
    matchMedia: globalThis.matchMedia,
    exact: globalThis.exact,
  };
  try {
    const style = { textContent: '' };
    let rejectUpdate, rejectFinished, rejectReady;
    globalThis.document = {
      head: { appendChild: el => el },
      createElement: () => style,
      getElementById: () => null,
      querySelectorAll: () => [],
      startViewTransition() {
        const updateCallbackDone = new Promise((_, rej) => { rejectUpdate = rej; });
        const ready = new Promise((_, rej) => { rejectReady = rej; });
        const finished = new Promise((_, rej) => { rejectFinished = rej; });
        return { updateCallbackDone, ready, finished, skipTransition() {} };
      },
    };
    globalThis.matchMedia = () => ({ matches: false });
    globalThis.exact = {};

    const { rt } = await import('./rt-link.js');
    rt.Invocation ??= { id: 0, pending: new Set() };
    const { commit } = await import('./shared.js');

    const order = [];
    const inflight = { n: 0 };
    // Start a crossFade transition whose callback will never be invoked.
    expect(commit(() => { order.push('first'); return true; }, [], inflight, [], true)).toBe(true);
    // Queue a second commit and an afterCommit hook behind the stalled transition.
    expect(commit(() => { order.push('second'); return true; }, [], inflight, [], false)).toBe(true);
    globalThis.exact.afterCommit(() => order.push('after'));
    expect(order).toEqual([]);
    expect(inflight.n).toBe(1);

    // Reject the ViewTransition without running its update callback.
    rejectUpdate(new Error('skipped'));
    rejectReady(new Error('skipped'));
    rejectFinished(new Error('skipped'));
    await new Promise(r => setTimeout(r, 0));

    expect(order).toEqual(['first', 'second', 'after']);
    expect(inflight.n).toBe(0);

    // A subsequent commit runs immediately, proving `pending` is cleared.
    expect(commit(() => { order.push('third'); return true; }, [], inflight, [], false)).toBe(true);
    expect(order).toEqual(['first', 'second', 'after', 'third']);

    // A synchronous throw from startViewTransition must also restore inflight and drain pending tails
    // on both the crossFade path and the paired/routed transition path.
    globalThis.document.startViewTransition = () => { throw new Error('sync skip'); };
    expect(commit(() => { order.push('fourth'); return true; }, [], inflight, [], true)).toBe(true);
    expect(inflight.n).toBe(0);
    expect(order).toEqual(['first', 'second', 'after', 'third', 'fourth']);

    const sharedEl = {
      isConnected: true,
      style: { viewTransitionName: '' },
      getAttribute: name => (name === 'data-shared-element' ? 'card' : null),
      closest: () => null,
    };
    const root = { querySelectorAll: sel => (sel === '[data-shared-element]' ? [sharedEl] : []) };
    globalThis.document.getElementById = id => (id === 'exact-root' ? root : null);
    expect(commit(() => { order.push('fifth'); return true; }, [], inflight, [], false)).toBe(true);
    expect(inflight.n).toBe(0);
    expect(sharedEl.style.viewTransitionName).toBe('');
    expect(commit(() => { order.push('sixth'); return true; }, [], inflight, [], false)).toBe(true);
    expect(order).toEqual(['first', 'second', 'after', 'third', 'fourth', 'fifth', 'sixth']);
  } finally {
    if (prev.document === undefined) delete globalThis.document; else globalThis.document = prev.document;
    if (prev.matchMedia === undefined) delete globalThis.matchMedia; else globalThis.matchMedia = prev.matchMedia;
    if (prev.exact === undefined) delete globalThis.exact; else globalThis.exact = prev.exact;
  }
});
