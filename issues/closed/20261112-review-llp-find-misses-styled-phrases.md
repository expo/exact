# The LLP reader cannot find text that crosses a formatting boundary

**Status:** Closed
**Resolution:** Find searches a paragraph's, metadata field's or table cell's runs as one text and gives each run's part of a match its own span, with that run's style and link and the match's one ordinal.
**Systems:** LLP terminal reader, page search
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, `apps/llp/data/src/page.rs:136–143` calls `split()`
separately for each styled run. The page's find operation therefore
searches each run independently. A phrase visible as continuous text
cannot match when Markdown emphasis, a link, or inline code separates
its characters into multiple runs.

For example, `Hello **world**.` displays `Hello world.`, but finding
`hello world` reports no matches. The same failure occurs within a word
such as `some**thing**`. It makes ordinary document search depend on
formatting that the reader does not see as a text boundary.

Verified through the actual `llp_data::Llp` page source, its continuation,
and `parse()`, linked against the existing checkout libraries. A
temporary document contains `Hello **world**.`:

```text
actual Llp page find(hello world): hits=Number(0.0)
```

The source and output are
`/tmp/exact-review-20261112/llp-paths/styled.rs` and
`styled-result.txt`. No terminal app was launched.

Search the logical text of each paragraph or table cell, then map each
match back to its styled runs. Preserve each run's formatting and URL
while highlighting one match across those runs. Cover emphasis, links,
inline code, and a query that starts or ends inside a formatted span.

QUEUE.md:695 already lists “find across a run boundary” as an LLP reader
follow-up. Checked all open and closed filesystem issues and the full
bodies of the 275-issue GitHub snapshot; no issue tracks that entry. This
ticket records the verified failure separately from the Unicode
lowercase-offset panic.
