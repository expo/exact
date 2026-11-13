# On macOS, under `navigationHistory="none"`, Go ▸ Back and Forward show the app's ⌘[ ⌘] chords but stay disabled

**Status:** Closed
**Resolution:** Does not reproduce at 461760130: under `navigationHistory="none"` the Go menu lists the app's own two buttons with ⌘[ and ⌘] in the host items' place, and the chords press them, with and without `window-controls-overlay` and `navigationBar="none"`; a driven window is never key, so `ShortcutHost.validateMenuItem` shows those items disabled to anyone looking at a driven app's menu.
**Systems:** host/apple/Sources/ExactKit/Mac/ShortcutsMac.swift, host/apple/Sources/ExactKit/Mac/BackMac.swift, host/apple/Sources/ExactKit/Mac/DevMenuMac.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`docs/contract-grammar.md` ("The Mac's menu bar") files a button whose chord
is ⌘[ or ⌘] under Go, and LLP 1115.000 D5.1 has the app's own ⌘[ command
stand in for the host's Go ▸ Back. An app whose Back and Forward are its own
(an app-kept history) says `navigationHistory="none"` on its navigation root,
so the host's Back and Forward are withheld, and declares
`aria-keyshortcuts="Meta+["` and `"Meta+]"` on its two buttons. In that app the
Go menu shows Back and Forward with the chords, disabled, and the chords seem
not to reach the buttons.

By the code, this should not happen. `ShortcutHost.sync` makes each button an
item (`isNavigation`, lines 467 and 737, files ⌘[ ⌘] under Go); `claim` takes the
chord from the host's `GoMenuTarget` items and `stepAside` hides them while a
shown command holds it (lines 666 to 692); the app's items are enabled by
`enabled(_:key:)` (line 759), which does not read `navigationHistory`. The
host's items, by contrast, are disabled under `"none"`: `GoMenuTarget`'s
`validateMenuItem` asks `canGoBack` / `canGoForward` (`BackMac.swift:156`,
`:206`), and both need `offers(back:)` (`:149`). So what was seen looks like the host's
items, still holding their chords, meaning the buttons never became items:
`nodes()` (line 369) dropped them, or their chords did not parse. That is a
reading, not a reproduction.

Related, and not a bug in itself: no Contract command calls the host's
Forward, so an app with Back and Forward buttons in the page must keep its own
list of visits.

## The failure

Routes under a root with `navigationHistory="none"`, two buttons with `press`
handlers and `aria-keyshortcuts="Meta+["` and `"Meta+]"` driving the app's own
history; after a move, Go ▸ Back and Go ▸ Forward showed those chords and were
disabled. Seen on exact2 `5434f4325`, macOS host. In the same app the window
used `window-controls-overlay` with every route `navigationBar="none"`, so the
buttons may sit in the page-drawn title row.

Not reproduced here (no native build was run). `Shortcut.init` parses
`Meta+[` (a one-character key), so a parse failure is unlikely.

## Fix

Reproduce with an XCTest beside the existing menu tests: a root with
`navigationHistory="none"` and the two buttons, then read the Go menu's items,
their targets, chords and `validateMenuItem`. If the buttons are missing from
`nodes()`, find the filter that drops them (`toolbar.visible`, `segments`,
`sidebar`); if they are items but disabled, find which clause of `enabled`
fails. The grammar's promise is the expected result: the app's items in the
host items' place, with the chords, enabled while the buttons are.
