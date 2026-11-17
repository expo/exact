# A page-drawn macOS title row must preserve the title bar's mouse behaviour

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/TitleBarAreaMac.swift, TitleRowTapMacTests.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261009-titlebar-env-display-mode.md

Reproduced on macOS 27.0.1; three fix rounds did not preserve the authored action without the native action.

## What was wrong

On `ecdaf06fb`, Caltrain was given `window-controls-overlay` and an authored
52-point `header` in the title band. A real Quartz HID double click on its empty
ground changed the window frame from `(510, 137, 900, 640)` to the native zoomed
frame `(0, 30, 1920, 974)`. The same drive passed with no authored header and
with a glass root. Current main therefore preserves the person-visible title
bar action on macOS 27.0.1.

The same title-band ground can have an authored `dblclick`. A real Quartz HID
drive on that listener runs the authored action once but also performs the
person's configured title action. With Zoom selected, the window frame changes.
The authored action therefore does not have precedence.

## What was tried

Consuming the second down and up in the local event monitor still let AppKit
zoom. Marking the listener as outside the window's move region still let AppKit
zoom. Consuming both complete clicks still let AppKit zoom. The last drive in
each round reported `LISTENER Double 1 frameChanged=true`. These attempts were
removed rather than changing working empty-ground behaviour.

## What remains

Give an authored `dblclick` precedence without replacing AppKit's title action
on adjacent empty ground. Run that proof under Zoom, Minimize and None, then on
macOS 26.6, where the original report was made.
