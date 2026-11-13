# One link atom stays mixed on the web toolbar facts

**Status:** Closed
**Resolution:** Editor::facts reports a selection of one link atom as not mixed, as wire::selection_with does.
**Systems:** markdown
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

`Editor::facts` reports one link atom's href and leaves `mixed` as `selection` computed it. `selection_with` forces `mixed` false for the same atom.

A label such as `[**bo**ld](scheme:)` is mixed on the web facts path and not on Apple. A uniform label stays unmixed, so the atom test does not catch it. The toolbar can offer mixed formatting for a selection that is one atom.

## Where

`markdown/editor/src/editor.rs`, `facts`. `markdown/src/wire.rs`, `selection_with`.

## How to reproduce

Select the atom `[**bo**ld](scheme:)` and read `facts` on the web. `mixed` is true and `link` is `scheme:`. The same selection through `selection_with` has `mixed` false.

## Direction

When the selection is one link atom, set `mixed` false, as `selection_with` does.
