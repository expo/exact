# On the web, a persisted number state whose stored text is "constructor" stops the app from booting

**Status:** Closed
**Resolution:** `persist.js` revives only an own key of `NONFINITE` (`Object.hasOwn`), so a stored `"constructor"` leaves the initial value standing; the web JS target's other lookups of outside names in plain objects do the same.
**Systems:** host/web-js/persist.js
**Severity:** P3
**Author:** Claude (Opus 5.5), from GPT-6 Astra's and Grok 4.7's whole-project reviews (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

`revive` reads a stored string as a non-finite number with `v in NONFINITE`
(`host/web-js/persist.js:13`, `:19`). `in` also finds the properties `NONFINITE` inherits
from `Object.prototype`, so `"constructor"`, `"toString"`, `"valueOf"`,
`"hasOwnProperty"` and `"__proto__"` revive as a function or an object, not `undefined`.
`stored` (`persist.js:36-45`) then returns that value in place of the initial one, the
emitted code passes it to `sig` (`host/web-js/src/regions.rs:210-224`), and `sig` throws
"a slot's initial value does not conform to its type" (`host/web-js/rt.js:88-89`). The
page does not boot until the stored key is cleared.

A property that script running before the slot is made adds to `Object.prototype` is
found the same way: a stored name of one that holds a finite number revives as that number
and boots, where the runner keeps the initial value.

Any other stored value that does not fit is dropped with a journal line and the initial
value stands (`persist.js:42-44`). The runner reads only `"NaN"`, `"Infinity"` and
`"-Infinity"` (`runner/src/runner/persist.rs:175-181`).

## To see it

Confirmed by reading the code, and by evaluating the lookup in Bun:
`"constructor" in { NaN: NaN, Infinity: Infinity, '-Infinity': -Infinity }` is `true`, and
`conforms` (`host/web-js/shape.js:10`) refuses what `"toString"`, `"constructor"` and
`"__proto__"` revive as, so `sig` throws rather than holding a function.
An app ships `state q: string = "" persist` and a person types `constructor`; the next
version declares `state q: number = 0 persist`. On the web that version throws at boot;
on native hosts it starts at `0`.

## What a fix is

Match the three spellings only: `Object.hasOwn(NONFINITE, v)`, or a `switch`, and test
that a stored `"constructor"` leaves the initial value standing.
