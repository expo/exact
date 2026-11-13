# A package picture that misses its budget is not asked again

**Status:** Open
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

After a package picture is shown, a larger box asks again and remembers that size as `asked` when the ask is issued. If the answer cannot be charged, `package_answer` clears the ticket and returns. `shown` stays true, and `asked` stays at the size that was not charged.

The next sync asks again only when the picture is not shown, or the box grows past `asked` by half. An upgrade that missed the budget therefore never retries at that size. The view keeps the smaller bitmap. The comment on the early return says the picture is asked again at a later sync. That happens for a first picture, whose `shown` is still false. It does not happen for a growth.

## Where

`host/linux/src/image/package.rs`, `want` and `package_answer`.

## How to reproduce

Show a package photo. Grow its box past one and a half times the decoded size while the raster session cannot reserve the new bitmap. After the budget has room again, leave the box at that size. No new ask is issued. The smaller bitmap stays.

## Direction

On a reserve or commit failure, keep `asked` at the size actually shown, so the next sync asks for the larger box again. Do not treat the uncharged size as satisfied.
