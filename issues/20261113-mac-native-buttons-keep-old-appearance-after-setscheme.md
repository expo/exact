# On macOS a native button keeps the appearance it was configured in after `setScheme` changes the app's

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/ButtonConfigurationMac.swift, host/apple/Sources/ExactKit/Mac/NativeButtonsMac.swift, host/apple/Sources/ExactKit/Mac/NodeViewMac.swift
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`ButtonConfigurationMac.apply` pins each `NSButton` to an explicit appearance:
`button.appearance = appearance` (`ButtonConfigurationMac.swift:67`), where the
caller passes `owner.effectiveAppearance` as it is at configuration
(`NativeButtonsMac.configureNative`, line 457). An explicit `appearance` on a
view stops it inheriting, so when `setScheme` later sets `NSApp.appearance`
(`ExactMac/main.swift:93` to `:97`), the button keeps the old one.

Nothing configures it again on the change. `configureNative` runs from the
controls pass (`ControlsMac.swift:182`) when a batch touches the node; it would
redraw, since `Written.appearance` is compared in `draws(as:)`, but an
appearance change sends no batch. `NodeView.viewDidChangeEffectiveAppearance`
(`NodeViewMac.swift:872`) re-resolves tables, the split, SVG paints and scheme
colours, and leaves native controls alone.

## The failure

With the system in Dark, an app calling `setScheme("light")`: the Contract-drawn
surfaces and text turned light, and a row of plain buttons (default
`buttonStyle`, `NSButton`) kept Dark's light label text on the now light
ground, nearly invisible. Seen on exact2 `9e53578f2`, macOS host.

Checked by reading the code at `49ae6357`; not run. The same holds for the other
direction (light system, `setScheme("dark")`) and for `color-scheme` on a
subtree (`NodeView.applyColorScheme`), by the same path.

## Fix

Leave the button's appearance unset so that it inherits, as AppKit's controls
do, and pass `effectiveAppearance` only to the colour resolution in `apply`; or,
keeping the explicit appearance, configure the native controls again from
`viewDidChangeEffectiveAppearance` on their owner (the `Written` comparison
already redraws on a new appearance name). An XCTest: configure a button under
`darkAqua`, set the window's appearance to `aqua`, and check the button's
`effectiveAppearance`.
