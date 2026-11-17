# An ATX heading that ends in # drops those characters

**Status:** Closed
**Resolution:** An ATX heading drops a closing run of # only when a space or tab separates it from the text (CommonMark 0.31.2 §4.2), so `# C#` is the heading `C#`.
**Systems:** apps/markdown parser
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`heading_of` trims the line and then strips every `#` from the end. CommonMark removes a closing sequence only when those `#` characters are preceded by a space and the rest of the line is spaces. A heading whose text ends in `#` loses them.

`# C#` is stored as `C`. The same happens for `F#` and for a heading that ends with a hash as part of the words.

## Where

`apps/markdown/parse/src/lib.rs` `heading_of`: `rest.trim().trim_end_matches('#').trim_end()`.

## How to reproduce

A document whose first line is `# C#`. The heading text is `C`.

## Direction

Strip a trailing run of `#` only when a space separates it from the heading text and nothing but spaces follows it.
