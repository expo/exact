# `rename-token` skips a `component` block's defaults and rewrites a kind word, then says the design does not check

**Status:** Closed
**Resolution:** Token renames update component defaults and pair references, preserve kinds and folded values, locate unresolved defaults, and keep range checks in combined transactions.
**Systems:** tamber/src/edit.rs (`Doc::apply`, `Op::RenameToken`), contract/cli/src/tamber_serve.rs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`Op::RenameToken` renames words in three blocks of a design: `theme`, `exports` and `params` (`tamber/src/edit.rs:964-1011`, at `dd6f5dcd`). Two cases are wrong.

1. **A `component` block is skipped.** The loop names a block by its unindented line. Inside `component MapCard` the block is `"component MapCard"`, so the test at line 972 passes over its `params` lines, and a default there that names the token keeps the old name.
2. **A token line is renamed whole.** For a `theme` line `keep` is 0 (line 989), so `rename_words` runs over `name: kind = value`, the kind included. LLP 1121 §10 (stage 3, commit 2) says a rename changes names "in values and exports only".

The proof that follows (`edit.rs:236-253`) then finds a design that no longer checks and refuses with "a rename needs a design that checks before and after it". The design checked before; the rename broke it.

## The failure

```
tamber t

theme
  accent.fill = #1083fe
  card = accent.fill

component MapCard
  params
    pin: color = accent.fill
```

Renaming `accent.fill` to `accent.main`:

- `exact_tamber::apply` returns `Ok` with a text that still reads `pin: color = accent.fill`, which `contract tamber check` refuses (`tamber-unknown-token`).
- The design server checks the app's design with its params, so its `propose` is refused in the words above.
- The server checks a second `.tamber` file beside the app's without params. With these blocks in one, the same `propose` answers `ok`, and its diff leaves the default.

```
tamber t

theme
  family = "Inter", system-ui
  mono: family = "Menlo", monospace
```

Renaming `family` to `text.face` is refused in the same words, by `apply` and by the server. With the second line removed it succeeds. Likewise `length = 4px` is refused beside `gap: length = 8px` and renamed beside `gap = 8px`.

Expected: the first rename also writes `pin: color = accent.main`. The second leaves `mono: family` alone and succeeds.

## Verification

A temporary test ran each design through `exact_tamber::apply`, and `contract tamber serve --stdio` answered each `propose`. The results are as quoted.

## Direction

In the rename loop, rename the value (after the first top-level `=`) of each line under a `component` block's `params`, and for a `theme` line skip the kind between `:` and `=`. Where the design stops checking only after the rename, say which name still reads the old one: a token that a Contract component's param default names is refused in the same false words today, and there refusing is right.

## Related

Nothing in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`. `docs/tamber-protocol.md` ("A token's rename renames the `params` values naming it") covers the app's `params` block, which works.
