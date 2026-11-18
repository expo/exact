# macOS: VoiceOver re-reads the old field when a context-menu pick focuses a field in a page-drawn modal

**Status:** Closed
**Resolution:** not the host's: AppKit sends a menu item's action after AXMenuClosed, and a hand-built AppKit window re-reads the old field as often (8 of 8 each on macOS 26.6.2 and 27.2)
**Systems:** Apple host (MenuHost, accessibility), VoiceOver
**Severity:** P3
**Author:** Daehyeon Mun
**Date:** 2026-10-18
**Related:** issues/closed/20261018-macos-menu-pick-alert-voiceover-old-field.md, issues/closed/20261016-native-menu-pick-voiceover-old-focus.md, issues/closed/20261016-macos-voiceover-notifications-appkit-does-not-post.md, #719 (not merged)

With a long draft focused in a `textarea`, a `contextPopover` (`NSMenu`) pick whose action
opens a `column role="dialog" aria-modal` drawn in the page and calls `focus()` on its
`input` moves the keyboard focus into the box, but VoiceOver says "Closing menu Compose
message, edit text …", the draft, and does not follow the focus: 6 of 6 runs on main and
on #719, real VoiceOver on macOS 26.6.2. Opened with the focus left on the draft, the box is
read as "Box, dialog" (0 of 18 re-read); a `<dialog>` sheet whose field takes the focus is
read (0 of 16). #719's body has the repro (not committed) and each run's captions.

Cause: not found. Record with an `AXObserver` what the host posts after this pick and what
AppKit posts when an `NSMenu` item shows a group in its window and makes the group's field
the first responder; post what AppKit posts.

Acceptance: on the repro, VoiceOver reads the box's field, not the draft, in 6 of 6; the
`<dialog>`, unfocused-box and `alertdialog` picks read the draft no more than today.

## Closed, 2026-11-18

Not the host's: a hand-built AppKit window re-reads the old field after the same pick as
often. Its menu item adds a group to the window and makes the group's field first responder
(with the group's subrole `AXApplicationDialog` or none: the same; AppKit has no modal drawn
in a window, so the page's `aria-modal` has no counterpart there). It posts what the host
posts, one `AXFocusedUIElementChanged` for the field after the menu's `AXMenuClosed`, and
VoiceOver reads "Closing menu Compose message, edit text, …" and then the field, in one phrase,
its cursor ending on the field: it does follow the focus. In the AppKit window's log the
menu's `didEndTracking`, its delegate's `menuDidClose` and the item's action each came after
`AXMenuClosed`, so the action cannot move the focus before VoiceOver hears the menu close; the
likely reading is that VoiceOver names the focus it finds then.

The repro is `scripts/fixtures/menu-pick`: `vo-trial.swift` runs one pick with real input
and records what VoiceOver said (its caption panel and its last phrase) and what the app
posted (an `AXObserver`); `appkit-reference.swift` is the hand-built window. Runs that read the
draft after the pick, Exact and the AppKit window interleaved in one VoiceOver session, eight
rounds each:

| Pick | macOS 26.6.2: Exact | AppKit | macOS 27.2: Exact | AppKit |
|---|---|---|---|---|
| The box, its field focused | 8 of 8 | 8 of 8 | 8 of 8 | 8 of 8 |
| An `alertdialog` (`NSAlert` sheet; AppKit's begun in the action) | 1 of 8 | 5 of 8 | 2 of 8 | 4 of 8 |
| A `<dialog>` (a sheet; AppKit's begun in the action) | 6 of 8 | 4 of 8 | 7 of 8 | 4 of 8 |

The rate of the sheet and alert picks changes from one VoiceOver session to the next on the
same build (Exact's `<dialog>`: 1 of 3 in one session, 6 of 6 in the next), so only
interleaved runs compare. The AppKit window's group re-read also when shown after
`popUpContextMenu` returned, as the host presses (2 of 2 on 26.6.2, 3 of 3 on 27.2). The box
opened with the focus left on the draft, which has no AppKit counterpart, read the draft first
in 2 of 6 and 0 of 6 runs on main; the pick that navigates back, in 0 of 5 and 1 of 6.
