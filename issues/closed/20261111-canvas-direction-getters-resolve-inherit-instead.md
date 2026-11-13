# Canvas direction getters resolve inherit instead of returning the stored token

**Status:** Closed
**Resolution:** Both recorders' `direction` getters return the stored token, `inherit` by default, and the shared cases expect it as Chrome answers.
**Systems:** canvas/src/context/text.rs, canvas/recorder.js, canvas/tests/cases.txt
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, both recorder getters return the effective writing direction: Rust calls `g.rtl()` (`canvas/src/context/text.rs:157`), and JavaScript calls `this._.rtl()` (`canvas/recorder.js:1014`). A new context stores `inherit`, but these getters return `ltr` or `rtl`. Chrome returns the stored `inherit` token. The shared fixture expects `ltr` at `canvas/tests/cases.txt:218`, so the browser conformance check fails on that first assertion.

## The failure

```js
const ctx = document.createElement('canvas').getContext('2d');
ctx.direction; // Chrome: "inherit"
```

The JavaScript recorder returns `ltr` both initially and after `direction = "inherit"`. Expected: `inherit` from the getter; resolve it only when drawing or measuring text.

## Verification

Ran `bun canvas/tests/cases.mjs --chrome` using the repository's headless Chrome launcher. It exited 1 and printed:

```
stage 2: the text attributes ignore unknown values:
  direction => inherit, want ltr
chrome: 32/33 cases agree with cases.txt
```

Also instantiated `Recorder` from `canvas/recorder.js` under Bun and read its direction before and after setting `inherit`; both were `ltr`. The Rust getter was checked by reading, not a separate Rust test. No source or test was changed.

## Direction

Return the stored direction token in both recorders and update the shared fixture to expect `inherit` initially. Keep effective-direction resolution in the text layout and drawing paths.

## Related

The other open Canvas 2D issues concern drawing, spacing and cache retention. No issue or QUEUE entry records this getter or the failing direction assertion.
