# A Markdown closing fence with trailing space stays open

**Status:** Open
**Systems:** apps/markdown parser
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A fenced code block closes only when the trimmed line is nothing but the fence marker. `trim_start` leaves trailing spaces and tabs, and `trim_end_matches(marker)` then sees them, so the line is not a close. The marker is stored as code and the fence runs on to EOF or a later marker-only line.

CommonMark allows spaces or tabs after a closing fence. A fence written that way, which formatters and editors emit, swallows the rest of the document into the code block.

## Where

`apps/markdown/parse/src/lib.rs` `code_block`. The close test is `trimmed.trim_end_matches(marker).is_empty()` on a line that was only `trim_start`ed.

## How to reproduce

```
```
code
``` 
next
```

(The closing fence has a trailing space.) `next` is inside the code block.

## Direction

Treat a line as a close when, after the opening indent, it is the marker at least as long as the opener and the rest is only spaces or tabs.
