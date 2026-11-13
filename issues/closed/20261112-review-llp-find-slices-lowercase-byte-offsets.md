# Finding text in the LLP reader can panic on ordinary Unicode content

**Status:** Closed
**Resolution:** Find folds each character on its own and maps every match back to the whole original characters it touches, so a run whose lowercase changes length is searched without slicing inside a character.
**Systems:** LLP terminal reader, page search
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `apps/llp/data/src/page.rs:678–689` searches a lowercased
copy of each text run, then uses the matching byte offsets to slice the
original run. It checks that the two strings have equal total byte
lengths, but that does not establish equal character boundaries.
Lowercasing can expand one character and contract another in the same
run, leaving the total unchanged.

For example, the paragraph `İstanbul and STRAẞE.` has that property:
`İ` becomes `i` plus a combining dot, gaining one byte, while `ẞ` becomes
`ß`, losing one. Finding the ASCII letter `i` then slices the original
run at byte 1, inside `İ`, and panics. This is valid document text and a
normal find operation. It can terminate the reader's data call instead
of displaying a match or a refusal.

Reproduced through the actual `llp_data::Llp` source and its
`answer("page", ...)`, continuation, and `parse()` calls, linked against
the checkout's existing libraries. A temporary document contains the
paragraph above, and the find argument is `i`:

```text
actual Llp page find(i): panic=true
panicked at apps/llp/data/src/page.rs:689:23:
end byte index 1 is not a char boundary; it is inside 'İ' (bytes 0..2)
```

The probe catches the panic in its own process. Its source and output
are `/tmp/exact-review-20261112/llp-paths/unicode.rs` and
`unicode-result.txt`; no terminal reader was launched.

Map matches in normalized text back to the original character ranges,
or otherwise verify every offset before slicing. A total-length check
cannot provide that mapping. Cover expansions and contractions in one
run, matches after either character, and cached and uncached page
projections. A failed mapping must never panic.

Checked all open and closed filesystem issues and the full bodies of the
275-issue GitHub snapshot. QUEUE's find-across-a-run-boundary entry is a
separate missing-match limitation. The Markdown indentation report also
concerns a different byte-slicing path.
