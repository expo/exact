# AppKit editing shortcuts use the wrong keyboard layout with Command

**Status:** Closed
**Resolution:** A paragraph's, a reading region's and an editor's own Command chords read the key through `NodeView.commandKey`, which keeps the layout Command selects, as the menu bar and authored shortcuts do.
**Systems:** host/apple/Sources/ExactKit/Mac/NodeViewMac.swift, host/apple/Sources/ExactKit/Mac/TextAreaMac.swift, host/apple/Sources/ExactKit/Mac/RegionSurfaceMac.swift
**Severity:** P2
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

Command handlers read `charactersIgnoringModifiers` at `host/apple/Sources/ExactKit/Mac/NodeViewMac.swift:287,334`, `TextAreaMac.swift:262,267,272,719` and `RegionSurfaceMac.swift:19`. On Dvorak-QWERTY Command, that string names the Dvorak character rather than the QWERTY character selected while Command is down.

## The failure

Focus a paragraph. Send Command-C with `characters="c"`, `charactersIgnoringModifiers="j"`, key code 8. Copy is not handled. Send Command-I with `characters="i"`, `charactersIgnoringModifiers="c"`, key code 34: the paragraph handles Copy. The same lookup in the editor maps bold and undo to the wrong physical keys.

## Verification

Ran `bun host/apple/build.mjs --test --only Fix08MacTests,Fix08AlertMacTests,Fix08HeaderMacTests`. A temporary XCTest made a real window and paragraph first responder, then called its `performKeyEquivalent` with these NSEvents. It printed `FIX08 paragraph: Command-C handled=false, Command-I handled=true`; both assertions failed. The other named handlers were verified by reading; the runtime reproduction covers paragraph dispatch. The temporary tests were removed.

## Direction

Use a shared Command-layout key lookup for paragraph, region and editor commands, preserving the intended modifier rules.

## Related

[Dialog Command-period matched by key code](closed/20261110-dialog-command-period-matched-by-key-code.md) is fixed for dialogs and authored shortcuts. These editing handlers still use the old lookup. Reported in that lane's fourth review round.
