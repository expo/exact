# The recorder draws `fillText` with an infinite `maxWidth` and applies `clip()` under a singular matrix; Chrome does neither

**Status:** Closed
**Resolution:** Both recorders draw no text when `maxWidth` is not finite and record no `clip`, with or without a path, under a matrix that cannot be inverted, as Chrome does.
**Systems:** canvas/src/context/text.rs, canvas/src/context.rs, canvas/src/path2d.rs, canvas/recorder.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

Two argument rules in both recorders differ from Chrome's, which the recorder is held
to (LLP 1056 §4, `canvas/tests/cases.txt`). Neither has a case there.

1. **`fillText` and `strokeText` with a `maxWidth` of `Infinity`.** The recorder
   returns only for NaN or a width at most zero (`canvas/src/context/text.rs:83-87`,
   `canvas/recorder.js:626`), so positive infinity falls through and the text is
   recorded unsqueezed. HTML's first step is "If any of the arguments are infinite or
   NaN, then return"
   ([text drawing steps](https://html.spec.whatwg.org/multipage/canvas.html#dom-context-2d-filltext)),
   and Chrome draws nothing.
2. **`clip()` and `clip(path)` under a matrix that cannot be inverted.** Path
   building, fills and strokes are skipped under such a matrix, as Chrome skips them
   (`Inner::paints`, `canvas/src/context.rs:342-345`). `clip` is recorded regardless
   (`context.rs:877-881`; `path2d.rs:366-370` and `recorder.js:887`, where the check
   is skipped for a clip; `recorder.js:900-903`). The current path, built before the
   matrix went singular, becomes the clip and stays after the matrix is restored. A
   `Path2D` is transformed by the singular matrix first, so it clips everything away.
   HTML's `clip` steps do not mention the matrix; Chrome ignores the call.

## The failure

```js
ctx.fillText("X", 10, 60, Infinity);

ctx.beginPath(); ctx.rect(0, 0, 10, 10);
ctx.setTransform(0, 0, 0, 0, 0, 0);
ctx.clip();
ctx.setTransform(1, 0, 0, 1, 0, 0);
ctx.fillRect(50, 50, 10, 10);
```

Observed, the list both recorders write:

```
Font 10 400 0 100 …, FillText 10 60 1 0 "X"
… SetTransform 0 0 0 0 0 0, Clip 0, SetTransform 1 0 0 1 0 0, FillRect 50 50 10 10
```

so the text is drawn, and the square at (50, 50) is outside the clip and is not.

Expected, from Chrome on the same calls: no ink for the text, and the square painted.

## Verification

Run:

- The Rust recorder: a temporary test in `canvas/tests` printed the two lists with
  `list::describe`.
- The TypeScript recorder under Bun (`new Recorder()`, the same calls) wrote the same
  records. With `clip(path)` in place of `clip()` it wrote
  `PathMoveTo 0 0, PathLineTo 0 0 (three times), PathClose, ClipPath 0`. The Rust
  `clip(path)` was read, not run.
- Headless Chrome 156 on a 200 by 100 canvas: `fillText` and `strokeText` with
  `Infinity` left no pixel with alpha (the same text with no `maxWidth` did). After
  `clip()` or `clip(path)` under the singular matrix, `fillRect(50, 50, 10, 10)`
  painted its 100 pixels; after the same `clip()` under the identity it painted none.

## Direction

Return from `draw_text` when `maxWidth` is not finite, and from both `clip` forms when
the matrix is not invertible, in both recorders. Add the two cases to `cases.txt`.

## Related

None in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
