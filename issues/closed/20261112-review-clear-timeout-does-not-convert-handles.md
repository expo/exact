# clearTimeout leaves timers armed when their IDs arrive as strings

**Status:** Closed
**Resolution:** A data source's `clearTimeout`, natively (js/src/time.js) and on the web JS target (ts-fetch.js), converts its argument to a `long` as Web IDL does before looking the timer up, so a handle's string or a boxed number clears it.
**Systems:** JavaScript data sources, js/src/binary.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `js/src/binary.js:402-407` uses the argument to
`clearTimeout` directly as a `Map` key. Timer IDs are numeric, so a string
or boxed number representing the same ID does not cancel the timer. The
callback still runs, and a parked answer keeps its old deadline.

For example, cancellation after an ID passes through a string-valued
field fails:

```js
const id = setTimeout(() => { /* work that should be canceled */ }, 10);
clearTimeout(String(id));
```

A bounded probe ran the unchanged `timers` function from `binary.js` with
a controlled answer owner and clock. Numeric cancellation removed the
timer and ran no callback. Both `String(id)` and `new Number(id)` left it
armed; advancing to 10 ran the callback once. The probe is
`/tmp/exact-review-20261112/timer-cancel-coercion.mjs`; it does not launch
a host or wait on wall-clock time.

Apply the timer API's integer argument conversion before looking up or
deleting the handle, including the rejection-report queue. Preserve the
existing `call.ran` invalidation for the converted ID. Validate numeric,
string and boxed IDs, no-op unknown IDs, and cancellation of an already
parked deadline.

The [HTML timer specification](https://html.spec.whatwg.org/multipage/timers-and-user-prompts.html#timers)
defines cancellation after Web IDL argument conversion. The timer setter
in this same implementation already applies Web IDL conversion to its
delay.

Checked open and closed filesystem issues, `QUEUE.md`, and all 275 cached
GitHub issues. The closed TypeScript failure-parity issue establishes
answer-owned timers; it does not cover cancellation handle conversion.
