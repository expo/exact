# An Android disconnect drops a pad that was already connected

**Status:** Closed
**Resolution:** The Android Gamepad module records the identity of each pad already connected when it starts, so that pad's disconnect names it as one that connected later does.
**Systems:** packages/gamepad
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

`init` records connected pads in `told` and does not store their names. `shape`, which is what `getGamepads` calls, is what writes `names`. `gone` emits `id: ""` when that map has no entry for the device.

Foreground delivery creates the module before any list. Unplugging a pad that was already attached then emits a `gamepaddisconnected` with an empty id. Apple keeps the controller's identity from connect and emits that. The app cannot tell which pad left.

## Where

`packages/gamepad/android/Gamepad.kt`, `init`, `padsChanged`, and `gone`.

## How to reproduce

Connect a pad, launch the app, and unplug the pad before `getGamepads` runs. The disconnect event's `id` is empty.

## Direction

Store each pad's identity when it is first put in `told`, and emit that shape on disconnect.
