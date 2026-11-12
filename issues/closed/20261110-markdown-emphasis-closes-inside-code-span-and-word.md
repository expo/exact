# The Markdown reader's parser closes emphasis at a `*` or `_` inside a code span and at an underscore inside a word

**Status:** Closed
**Resolution:** Emphasis closes outside code spans and word-internal underscore runs, with URL recognition bounded by emphasis scopes.
**Systems:** apps/markdown/parse (inline.rs); read by apps/markdown, apps/llp, apps/harness, apps/exact-live, apps/markdown-stress
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

This is the readers' parser, the crate `markdown-parse` in `apps/markdown/parse`, not the shared `markdown/` crate.

`emphasis` finds an opener's closer in an index built once over the paragraph's raw bytes (`apps/markdown/parse/src/inline.rs:420-452,526-545` at `dd6f5dcd`). `emphasis_matches` takes as a closer any marker that is not escaped and whose previous byte is not a space. It does not know where code spans are, and for `_` it does not ask what follows. So:

- A `*`, `**` or `_` inside a later code span closes the emphasis. The span's opening backtick is then inside the emphasis without its partner, and the span is drawn as text with its backticks.
- A `_` between two letters closes `_emphasis_`. The opener has the intraword test (`inline.rs:425-431`); the closer does not.

CommonMark 0.31.2 gives code spans precedence over emphasis (§6.1: "Code span backticks have higher precedence than any other inline constructs except HTML tags and autolinks") and does not let a `_` between alphanumerics open or close emphasis (§6.2, rules 2, 4, 6 and 8). The crate is "deliberately a subset of CommonMark" in which constructs it reads (`lib.rs:9-16`), and emphasis and code spans are both in the subset.

## The failure

Runs as `[flags]text` (B bold, I italic, C code). The first five are the specification's own examples:

| Source | Observed | CommonMark |
|---|---|---|
| ``*foo`*` `` (example 341) | ``[I]foo` `` then `` ` `` | `*foo`, `[C]*` |
| ``*a `*`*`` (478) | ``[I]a ` `` then `` `* `` | `[I]a `, `[IC]*` |
| ``_a `_`_`` (479) | ``[I]a ` `` then `` `_ `` | `[I]a `, `[IC]_` |
| `_foo_bar` (374) | `[I]foo` then `bar` | `_foo_bar` |
| `_foo_bar_baz_` (376) | `[I]foo` then `bar_baz_` | `[I]foo_bar_baz` |
| ``**Pass `**kwargs` through**`` | ``[B]Pass ` `` then ``kwargs` through**`` | `[B]Pass `, `[BC]**kwargs`, `[B] through` |
| `_see snake_case names_` | `[I]see snake` then `case names_` | `[I]see snake_case names` |

The corpus the LLP reader exists for has the first form: `llp/1093-text-that-flows-across-boxes.rfc.md:128`, ``*violates `break-*: avoid`*``.

## Verification

Ran each source through `markdown_parse::parse` as an integration test of the crate; the Observed column is what it printed. `tests/it/support/inline_legacy.rs`, the oracle the suite compares with, matches closers the same way, which is why the suite passes.

## Direction

In `emphasis_matches`, skip the bytes of code spans (the `ticks` index already pairs them) and, for `_`, do not record a closer whose next byte is alphanumeric. Add the cases to `tests/it/parse.rs`; the legacy oracle changes with them.

## Related

Closed: `20260924-markdown-nested-link-stack-overflow`, `20260924-markdown-unclosed-destination-quadratic` (the same file, about cost, not about which delimiter closes).
