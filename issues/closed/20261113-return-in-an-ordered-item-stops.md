# Return in an ordered item stops renumbering at a lazy line

**Status:** Closed
**Resolution:** Return's renumbering walk passes over a line the block machine counts as a continuation of a list item above it (a lazy line), and still stops at a blank line, a smaller indent, or a new block such as a column-0 fence or rule.
**Systems:** markdown
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Return in an ordered item inserts the next marker, then walks later lines to renumber them. A blank line or a smaller indent stops the walk. A same-indent line that is not an ordered marker also stops it.

A lazy continuation is that line. The block machine still treats it as part of the item. The walk stops there, so a later item keeps its old number. `1. hello` with the caret at the end of `hello`, then a lazy line `world`, then `2. next`, becomes two items numbered 2.

An indented continuation and a blank line are handled. A column-0 fence or rule is also not an ordered marker and must still stop the walk.

## Where

`markdown/src/edit.rs`, `newline`. `markdown/src/block.rs` treats a plain line at the same quote depth as a continuation.

## How to reproduce

In an ordered list, press Return at the end of `1. hello` when the next line is `world` and the line after that is `2. next`. The new item and `next` are both numbered 2.

## Direction

Skip a same-indent line that the block machine still counts as the open item. Stop only at a blank line, a smaller indent, or a new block that is not a continuation.
