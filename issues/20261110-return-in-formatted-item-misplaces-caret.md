# Return beside a literal backtick in a list item's code span breaks the span

**Status:** Open
**Systems:** markdown/src/edit.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What remains

This issue was filed for Return inside a formatted list item destroying its formatting and
misplacing the caret. That is fixed: Return keeps bold, emphasis, code and link
destinations on both sides of the break, nested in order, and Backspace at the new item's
start removes only its block prefix. So does a selection that starts in the list marker. One
case is left.

**A literal backtick in a code span.** Return next to a backtick inside a code span copies
the span's delimiter beside it. The run no longer matches the opening delimiter, so the
first item's code becomes literal syntax.

```rust
let source = "- ``a`b``";
let change = exact_markdown::edit(source, exact_markdown::Range::caret(6), exact_markdown::Command::Newline);
let after = change.apply(source);
// after == "- ``a```\n- ``b``", plain(&after) == "• ``a```\n• b"
```

Expected plain text, with both halves still code:

| Source | Caret | Expected |
|---|---|---|
| ``- ``a`b`` `` | 6 | `• a` then a backtick, `• b` |
| ``- ``a`b`` `` | 5 | `• a`, `• ` then a backtick and `b` |
| ``- ```a``b``` `` | 7 | `• a` then a backtick, `• ` then a backtick and `b` |

LLP 1045 D2 and `markdown/src/lib.rs` specify code spans without edge-space normalization.
A span whose content starts or ends with a backtick needs padding to separate it from the
delimiter, and under this dialect the padding stays visible. So the fix needs a decision on
code-span padding first.

## Verification

Ran the table's three rows through `exact_markdown::edit` on 2026-11-12: the first gives the
result in the comment above, the second `"- ``a``\n- ```b``"` and the third
`"- ```a````\n- ````b```"`.

## Direction

Decide the padding rule, then serialize each half with valid delimiters and unchanged
displayed text.
