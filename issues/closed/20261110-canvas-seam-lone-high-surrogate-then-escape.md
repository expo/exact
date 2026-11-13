# The canvas seam's reply reader pairs a lone high surrogate with whatever `\u` escape follows it

**Status:** Closed
**Resolution:** The reader pairs a high surrogate only with a following `\u` escape in `DC00..=DFFF`; otherwise it writes U+FFFD and reads the next escape on its own.
**Systems:** canvas/src/seam.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`exact_canvas::seam::reply` reads the JSON a TypeScript draw returns, with a string
reader of its own (`canvas/src/seam.rs:55-115`). After a `\u` escape in the high
surrogate range it takes any following `\u` escape as the low half, without checking
that it is one (`seam.rs:84-92`):

```rust
if (0xD800..0xDC00).contains(&code) && self.s[self.at..].starts_with(b"\\u") {
    …
    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
```

When the second escape is below `DC00` the subtraction underflows. A build with
overflow checks (the `dev` profile, which `cargo test` uses) panics. The `host-dev`
and `release` profiles wrap, so the two escapes become one unrelated character and
the second is lost.

`JSON.stringify` writes a lone surrogate as `\udXXX` and a control character as
`\u00XX`, so the input is a string with two lone high surrogates in a row
(`"😀".slice(0, 1) + "😀".slice(0, 1)`), or one followed by a control character. The
strings in a reply are the message a draw threw and its development notes
(`canvas/recorder.js:1133-1138`), which can hold text the app cut with `slice`.

## The failure

```rust
exact_canvas::seam::reply(
    r#"{"lists":[],"frame":false,"error":"Error: \ud83d\ud83d","notes":[]}"#,
)
```

Observed:

```
dev:       panicked at canvas/src/seam.rs:91:76: attempt to subtract with overflow
host-dev:  error = "Error: \u{1f03d}"
```

and with `\ud83d\u0001 tail` in place of the two surrogates, the same panic, or
`"Error: \u{11801} tail"`.

Expected: `"Error: \u{fffd}\u{fffd}"` and `"Error: \u{fffd}\u{1} tail"`. The reader
already writes U+FFFD for a lone high surrogate that a letter follows.

## Verification

Run: a temporary test in `canvas/tests` called `seam::reply` on four replies under
`catch_unwind`, once with `cargo test` and once with `cargo test --profile host-dev`,
and printed the lines above. In both, `\ud83dA` read as U+FFFD then `A`, and the pair
`😀` as U+1F600. No draw was run through a JavaScript engine.

## Direction

Combine only when the second escape is in `DC00..=DFFF`; otherwise write U+FFFD for
the first and leave the second to the loop.

## Related

`QUEUE.md`, "A native host refuses a data-module answer holding a lone UTF-16
surrogate", is serde_json refusing an answer; this is the canvas crate's own reader,
and it accepts the string and changes it.
