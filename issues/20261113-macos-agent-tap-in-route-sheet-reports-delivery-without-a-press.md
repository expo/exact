# A macOS agent tap on a button in a modal route's sheet replies `delivery: platform` but the button's action does not run

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/AgentMac.swift (`tap`), host/apple/Sources/ExactKit/Mac/RouteSheetMac.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261026-macos-agent-taps-in-the-title-bar-reach-nothing.md

## What is wrong

A modal route on macOS is an AppKit sheet (LLP 1115.002 D7.4). Its header's cancelling
and confirming buttons become the sheet's own push buttons, and the agent's tap on either
one presses that push button: `RouteSheetMac.swift:437` `activate`, reached from
`AgentMac.swift:684-690`, which replies `delivery: "host-activation"`, `native: "NSButton"`.
Any other button in the header stays in the page, at the top of the sheet's content. A tap
on one of those falls through to the platform click (`AgentMac.swift:861-890`): a
`leftMouseDown` and `leftMouseUp` through `NSApp.sendEvent` at the button's point in the
sheet's window. That path replies `tapped … delivery: platform` whether or not the click
reached a node. Here it reached none.

The reply shows the click took the platform path, so the button was not one of the two
the sheet's row stands for. Why the click is lost was not found. A sheet is a window of
its own, and the agent's app is an accessory that is never active. The closed issue on
title-bar taps had a similar symptom (a platform click answered with nothing pressed) and
did not reproduce later. Not reproduced here (this checkout runs on Linux).

## The failure

On Exact `e5ac601e`, `agent macos`: a tap on a button in a modal sheet's header replied
`tapped … delivery platform`, and the button's press action did not run. The reporter
took it for a toolbar item. The same tap passes on the web. Worked around with `submit=` on
the sheet's input and `type … key Enter`.

## Fix

A host test: a modal route whose header holds a third button (one `SheetHeader.read` does
not project) with a press that sets state, tapped by the agent. If the click is lost,
deliver it as a person's click on the sheet would be: make the sheet key first, as
`nativeType` does for a key. In every case, a platform tap that pressed nothing should not
reply as a delivered tap. `PartLanding` already records the hit view, and the reply could
say what the click landed on.
