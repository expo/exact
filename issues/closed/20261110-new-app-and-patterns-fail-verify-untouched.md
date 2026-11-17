# `exact new groceries` and four of the six patterns report `verify` problems before any edit

**Status:** Closed
**Resolution:** On the web a route bar's unsaid title is one line ending in an ellipsis, as a sheet's and UIKit's are, the site pattern's Kind row wraps, white on a fill in a system hue is held to the platform's 3:1, and the async lane runs verify --strict on the new app and every pattern.
**Systems:** game/new.mjs (the template), docs/patterns, scripts/verify.mjs, host/web/neutral.css (a route bar's title), docs/start-here.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Line references are at `dd6f5dcd`.

`exact new` prints `verify … done when clean` (`game/new.mjs:826`), and
`docs/start-here.md:54` says "A clean run on the app's platform is done". The app the
guide's first command makes (`exact new ~/groceries`, `docs/start-here.md:42`) does not end
clean, and neither do four of the six patterns it says to start from, written into that
app.

## The failure

```sh
bun <exact2>/scripts/exact.mjs new ~/groceries && cd ~/groceries
bun exact.mjs verify
```

```
largest text size (root font size 53 px at 390x844, iOS's body at its largest), 5 screens: 1 finding (the app's own layout: …)
  (first screen, "the greeting loads", 2 more only) overflow root [-141,0 390×844]: runs 141 px past the 390-wide viewport's side, and nothing scrolls it; wrap the row: flex-wrap="wrap" …
verify: 1 problem (1 at the largest text size; the tests passed)        (exit 0)
```

The bar's title is the app's name (`game/new.mjs:660`; a pattern's too, `:712`). On the
web a route bar's title wraps only between words and is never shortened
(`host/web/neutral.css:413-415` gives the one-line ellipsis to a sheet's title alone), so one long
word pushes the bar's button off the screen: `layout like` puts it at x 411 of 390.
`workouts` does the same; `recipes`, `budget`, `notes` and `v1` end clean. The app said
nothing about the title's size, and the remedy printed does not apply to a bar.

Each pattern, written by `pattern <name>` and verified untouched:

| Pattern | App named `groceries` | App named `v1` |
|---|---|---|
| `one-screen` | 2: `undo's label "Undo"` cut by 9 px; `overflow root` 141 px | clean |
| `dashboard` | 1: `overflow root` 141 px | clean |
| `rest` | 3: `persona's label "Ada Lovelace"` cut by 88 px, and `"Grace Hopper"`; `overflow app` 260 px | 2: the same two labels (71 px) |
| `site` | 1: `overflow kind [125,293 361×115]`, 96 px past the viewport | the same 1 |
| `list-detail`, `tabs` | clean | clean |

All but `site`'s are in a route's `header`, the case `QUEUE.md:449` records: iOS draws that
header as its navigation bar, which truncates the title, so the phone has no such finding.
`site`'s is the pattern's own: a label and a `select` in a `row` that does not wrap
(`docs/patterns/site/app.contract:91-93`).

The guide says to run `verify` "when the features work" (`docs/start-here.md:59`), so a
builder meets these in a finished app that kept its pattern's bar.

Expected: an app nobody has edited ends `verify: clean`.

## Verification

Ran the commands above on the web host in new apps outside the repository, then `pattern
<name>` and `verify` for each pattern under both names, and `agent web --size 390x844
"prefer root-font-size 53" "clock settle" "layout like"`. Every run's `contract verify`
line said `done`. Not run: `verify ios`. The async lane's pattern test drives `test web`
only (`contract/cli/tests/it/docs.rs:425`), so nothing holds a starter to `verify`.

## Direction

Give a route bar's unsaid title the one line and ellipsis a sheet's has. The template,
`one-screen` and `dashboard` are clean once the title fits, as the `v1` column shows.
`rest`'s bar button goes with `QUEUE.md:449`. Wrap `site`'s Kind row.

## Related

`QUEUE.md:449` has the header cause, from builders' apps. It does not say that the template
and the patterns trip it, and `site`'s finding is outside it.
