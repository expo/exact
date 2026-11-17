# Finding in the LLP reader can panic when casefold lengths cancel

**Status:** Closed
**Resolution:** Already fixed by 28c46a27b (LLP page find maps matches to whole characters): find folds each character on its own and maps a match back to the whole original characters, with no slicing at lowercase offsets; a test now pins the issue's U+2126 OHM SIGN and İ run.
**Systems:** apps/llp data, page find
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`text_matches` and `split` slice the original run with byte offsets taken from `to_lowercase` when the two strings have the same total length. Casefold is not length-preserving per character. U+2126 Ω is 3 bytes and lowercases to 2. U+0130 İ is 2 bytes and lowercases to 3. In a run such as `Ωİ` the totals match, so a find for `i` takes offset 2, which is inside Ω, and `run.text[start..end]` panics. A match that lands on a boundary highlights the wrong characters.

The length check was meant to skip a run whose indexes would not line up. Equal totals do not mean the indexes line up. The terminal reader calls `page` on every find keystroke.

## Where

`apps/llp/data/src/page.rs` `text_matches` and `split`.

## How to reproduce

Open a document whose text contains `Ωİ` in the LLP reader and search for `i`. The find panics on the slice.

## Direction

Do not slice the original at lowercase offsets unless each character's lowercase has the same UTF-8 length. Otherwise search without mapping indexes across the casefold.
