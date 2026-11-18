# macOS: an alert opened by a context-menu pick lets VoiceOver re-read the old field first

**Status:** Closed
**Resolution:** not the host's: a hand-built AppKit window's NSAlert sheet re-reads the old field as often or more (Exact 1 of 8 and 2 of 8, AppKit 5 of 8 and 4 of 8 on macOS 26.6.2 and 27.2)
**Systems:** Apple host (MenuHost, NSAlert), VoiceOver
**Severity:** P3
**Author:** Daehyeon Mun
**Date:** 2026-10-18
**Related:** LLP 1021 (press timing), issues/closed/20261016-native-menu-pick-voiceover-old-focus.md, #719 (not merged)

With a long draft focused in a `textarea`, a `contextPopover` (`NSMenu`) pick that opens a
`dialog role="alertdialog"` (an `NSAlert` sheet) lets VoiceOver say "Closing menu Compose
message, edit text …", the draft, before "In alert, dialog": 2 of 18 runs on main, real
VoiceOver on macOS 26.6.2. #719's body has the repro (not committed) and each run's captions.

Likely fix: begin the `NSAlert` sheet and tell VoiceOver's landing once `popUp` returns, as
#719 tried on `daehyeon/fix-menu-pick-navigate-away` (`MenuHost.afterPopUp`; there
AlertsMac.swift:202, Accessibility.swift:689): 0 of 11 in its rounds 2 and 3. That branch
also pressed inside the item's action, where main presses as `popUp` returns, and made the
`<dialog>` case worse (3 of 18, against 0 of 16 on main), which this must not carry.

Acceptance: on the repro, 0 of 6, with the `<dialog>` and `aria-modal` picks staying at 0.

A page-drawn box whose field takes the focus re-reads 6 of 6 on main:
issues/closed/20261018-macos-menu-pick-page-modal-field-voiceover.md.

## Closed, 2026-11-18

Not the host's: an `NSAlert` sheet begun by a hand-built AppKit window's menu item lets
VoiceOver read the old field first as often or more. The sheet's `AXSheetCreated` comes about
300 ms after the menu's `AXMenuClosed`, as `beginSheetModal` brings it in, whether it is begun
in the item's action (which AppKit sends after `AXMenuClosed`) or after `popUp` returns, as the
host begins it; when VoiceOver reads before the sheet is there, it reads the old field ("Closing
menu Compose message, edit text, …"). The host posts what AppKit posts: `AXSheetCreated`,
`AXFocusedWindowChanged` and the button's `AXFocusedUIElementChanged`.

Measured on `scripts/fixtures/menu-pick` with `vo-trial.swift` against
`appkit-reference.swift`, interleaved in one VoiceOver session, eight rounds each, runs that
read the draft after the pick: macOS 26.6.2, Exact 1 of 8, AppKit 5 of 8; macOS 27.2, Exact 2
of 8, AppKit 4 of 8 (AppKit's alert begun after `popUp` returned, not interleaved: 4 of 6 and
3 of 3). The `<dialog>` and page-drawn box picks are in
issues/closed/20261018-macos-menu-pick-page-modal-field-voiceover.md. On main the `<dialog>`
pick does not stay at 0 in this harness, on either system (6 of 8 and 7 of 8; the hand-built
sheet 4 of 8 each): it counts a phrase in VoiceOver's caption panel or its last phrase, where
#719 counted the panel alone, filmed at 6 frames a second.
