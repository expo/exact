# `excerpt` analyses the whole document to take its first lines, says it does not, and has no caller

**Status:** Closed
**Resolution:** `exact_markdown::excerpt`, which nothing called, is deleted with its tests; the apps that show an excerpt cut their own.
**Systems:** markdown/src/segment.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`exact_markdown::excerpt` is documented "Reads only as much source as it needs"
(`markdown/src/segment.rs:452` at `dd6f5dcd`). For a source longer than its budget
(`chars * 6 + 256` bytes) it finds where to cut by calling `analyze(source, None)` on the
whole source and walking the blocks until one ends past the budget (`segment.rs:457` to
`:466`). `analyze` runs the block pass and the inline pass over every block, so the cost
follows the document's length, not the excerpt's. Then it analyses the prefix a second time
through `plain`.

Nothing calls it. LLP 1045 D2 lists it "for feeds and search", and the apps that show an
excerpt cut their own (`apps/fieldnotes/app.ts:44`, `apps/agents/app.ts:283`).

## The failure

A 140-character excerpt of a document made of one repeated paragraph, release build, mean of
200 calls:

| source | `excerpt(source, 140)` |
| --- | --- |
| 1 KiB | 22 µs |
| 4 KiB | 98 µs |
| 16 KiB | 376 µs |
| 64 KiB | 1.14 ms |
| 256 KiB | 4.4 ms |
| 1 MiB | 17.2 ms |

`plain` of the first 1,080 bytes, which holds the excerpt, takes about 20 µs at every size.
Expected: about that for every row, as the doc says.

## Verification

A temporary release test of `exact-markdown` printed the table. With `excerpt` and its
re-export (`markdown/src/lib.rs:41`) removed as a trial, the four crates that depend on
`exact-markdown` built with every target (`cargo check --all-targets -p
exact-markdown-editor -p exact-web-capabilities -p exact-apple -p exact-linux`), and only
`markdown/tests/markdown.rs` (`:414`, `:415`, `:823` to `:830`) stopped compiling. No Swift,
Kotlin or JavaScript source and no wire command (`markdown/src/wire.rs`) names it. The trial
was reverted.

## Direction

If no app is meant to use the function, delete it and its tests
(`markdown/tests/markdown.rs:412` to `:415` and `:819` to `:831`). Otherwise find the cut with
the block pass alone (`block::scan`), stopping at the first block that ends past the budget.

## Related

`issues/closed/20260924-markdown-editor-copies-note-per-keystroke.md` and the two closed
quadratic issues of the same date are other costs in this crate; none is this one.
