# The one-scheme warning names white text on an accent fill when the fill comes from a `style`

**Status:** Closed
**Resolution:** The one-scheme warning reads a node's fill from its class's rows and its own, and drops a style's white `color` when that style's own `background-color` is `AccentColor`.
**Systems:** contract/lower/src/warnings.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

LLP 1116.020 D7 has the one-scheme warning allow white text on an `AccentColor` fill
(`contract/lower/src/warnings.rs:1332-1334`: "white text on an accent fill is a filled
button's label (Messages' bubble): the accent stays a fill in both schemes"). `one_scheme`
(`warnings.rs:1237`) applies that only where the fill is an attribute written on the element
or on an ancestor element:

- The view walk reads the fill from the node's own attributes (`warnings.rs:1327`). A fill
  the node's `class` supplies is not seen, so the node and its subtree are not on an accent.
- A `style`'s rows are collected before the walk (`warnings.rs:1309-1311`) with no exemption,
  so a style that holds both the fill and the white text is named at its `color`.

## The failure

Each of these is the same filled label.

```
component A
  view
    box background-color="AccentColor" color="#fff"
      text "Go"
```

No warning, as D7 says.

```
style Primary
  background-color="AccentColor"
  color="#fff"
component A
  view
    box class=Primary
      text "Go"
```

```
3:3 warning [lower-colour-one-scheme] `#fff` on `color` stays the same in dark mode while the platform's text and controls change: name a role (`-exact-label`, `-exact-secondary-label`, `AccentColor`) or leave it unsaid. …
```

```
style Fill
  background-color="AccentColor"
component A
  view
    box class=Fill
      text "Go" color="white"
```

`6:17 warning [lower-colour-one-scheme] `white` on `color` stays the same in dark mode …`;
with the fill written on the `box` instead of the class, nothing.

Expected: none of the three is named. Each repair the warning lists is wrong on an accent
fill: `-exact-label` and an unsaid colour are the page's text colour, dark on the accent in
light mode, and `AccentColor` is the fill's own colour. The role that fits, `AccentColorText`,
is not listed.

## Verification

Ran `target/debug/contract build` on the three sources; the output is quoted as printed. The
second source with `color="AccentColorText"` builds with no warning.
`contract/cli/tests/it/theme.rs:315-345` tests the exemption with inline rows only.
`apps/realworld/app.contract:95` keeps an accent fill in a style, with `AccentColorText`.

## Direction

Read the fill from the node's rows with its class's (`width::rows`,
`contract/lower/src/warnings/width.rs:43`, gives a node's class rows followed by its own), and
drop a style's white `color` site when that style's own `background-color` names
`AccentColor`.

## Related

`docs/recipes/chat-thread.md` writes `AccentColorText`, which no path warns of, so the recipe
is unaffected. `QUEUE.md`, "This repository's apps earn `lower-colour-one-scheme`", is about
the literal colours the repository's apps still write. Nothing in `issues/` is about this
exemption.
