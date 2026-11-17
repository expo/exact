# Harness merges adjacent links with different destinations

**Status:** Closed
**Resolution:** A transcript line merges two runs only when their links match as well as their style, so adjacent links to different destinations stay separate runs.
**Systems:** Harness Markdown rendering
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `apps/harness/data/src/state.rs:45–53` compares a run's
visual attributes when deciding whether two runs can merge, but omits
`href`. `Line::push()` then concatenates their text and keeps the first
run's URL (`state.rs:66–76`). The Markdown mapper uses this helper for
links (`markdown.rs:15–36`), so consecutive links with the same visual
style lose every destination after the first.

For example, a reply containing
`[first](https://first.example/)[second](https://second.example/)`
renders both labels as a single link to `https://first.example/`. The
terminal view binds each run's URL directly (`terminal.contract:393`);
activating the second label therefore opens the wrong destination.

Reproduced by including the actual `markdown.rs` and `highlight.rs`,
linking the existing Markdown parser library, and using the actual
`Run`, `Line`, and `Block` helper implementations with plain transcript
records. Calling `markdown::blocks()` with the example produces:

```text
runs=[Run { text: "firstsecond", ... href: "https://first.example/" }]
```

The probe asserts that there is one run, that its text is `firstsecond`,
and that its URL is the first destination. Its source and output are
`/tmp/exact-review-20261112/markdown-indent/links.rs` and
`links-result.txt`. No terminal or external URL was opened.

Include link identity when deciding whether runs can merge. Cover
adjacent links with different destinations, links to the same
destination, and formatted links. Their visual style may be equal while
their interaction remains distinct.

Checked all open and closed filesystem issues and the full bodies of the
275-issue GitHub snapshot. The closed home-directory link issue concerns
tool-entry path resolution; no report covers merging Markdown links.
