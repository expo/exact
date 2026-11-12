# On iOS a canvas never hears Space or Enter released, so the key stays held in its world

**Status:** Closed
**Resolution:** The iOS canvas leaves its held keys alone while it asks the surface controls about Space or Enter, so a release no control takes reaches the world, as on the Mac.
**Systems:** host/apple/Sources/ExactSurfaces/CanvasInputIOS.swift, GPU canvas input (LLP 1046.002 §3)
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `CanvasInputHost.presses` (`host/apple/Sources/ExactSurfaces/CanvasInputIOS.swift:155-177`) keeps the codes it sent down in `keys` and sends a release only for a code it holds. For Space, Enter and NumpadEnter every release first passes through the surface-control block, which removes the code from `keys` before it knows whether a control took the key:

```swift
if (source.isSurfaceControl || !down) && ["Space", "Enter", "NumpadEnter"].contains(code) {   // :162
    if down && keys.contains(code) { handled = true; continue }
    if down { keys.insert(code) } else { keys.remove(code) }                                    // :164
    if source.controlKey(code, down: down, timestamp: press.timestamp) { handled = true; continue }
}
if down { … } else if !keys.contains(code) { continue }                                         // :170
```

When no surface control holds the key, `controlKey(code, down: false)` answers false (`ExactKit/SurfaceInput.swift:78-89`: `pressedControlKey` returns false at `CanvasSeams.swift:128`, and `control("up")` finds no owner at `SurfaceGlue.swift:41-46`). Line 170 then finds the code gone and skips the release. The down went out at line 173, when the canvas itself held the focus: a down from the canvas does not enter the block, and line 172 put the code in `keys`.

The Mac host does not touch `keys` in that block and removes the code only where it sends the release (`CanvasInputMac.swift:90-98`).

## The failure

A canvas whose surface takes input has the focus on an iPad or an iPhone with a hardware keyboard, or in the simulator with the Mac's keyboard. Press and release Space.

By the lines above the surface receives `{"t":"key","code":"Space","down":true}` and no release. The game engine holds a key until its release or a blur (`game/engine/src/input.rs:1024-1029`) and drops a second down of a held key (`game/engine/src/sim/queue.rs:30-46`), so an action bound to Space stays active and the next press does nothing until the app resigns active.

Expected: `down:false` follows, as the Mac host sends it.

Every other key is unaffected: the block is entered only for the three codes. The agent's `type` on a canvas sends its key events itself (`ExactKit/IOS/AgentIOS.swift:1076-1080`), so a smoke does not take this path.

## Verification

By reading: `NodeView.pressesEnded` and `pressesCancelled` (`ExactKit/IOS/NodeViewIOS.swift:461-475`) are the only callers for a release, and the path through `presses` is the lines above with no other branch. Not run: the file compiles only for iOS and tvOS, and no simulator was available. No test under `host/apple/Tests` sends a canvas a key through `presses`.

To check on a simulator, write an iOS XCTest and run it with `bun host/apple/build.mjs --test --ios --only <Class>`: take the canvas of `SurfaceControlTests.fixture()` (`SurfaceControlTests.swift:40-65`, whose module records each input), make it first responder, and call `pressesBegan` and then `pressesEnded` on it with a `UIPress` subclass for Space, as `ReorderGroupIOSTests.swift:12-29` builds one. With the defect the module records a `key` event with `down: true` and none with `down: false`.

## Direction

Leave `keys` alone in the control block, as the Mac does: track the control's own held Space or Enter separately, or decide the repeat from the control's contact, so that only line 172 removes the code, where the release is sent.

## Related

`issues/20261110-checkbox-space-does-not-toggle.md` is a checkbox on the painted hosts. No filed issue names `CanvasInputHost`, and no entry in `QUEUE.md` or `docs/agent-pitfalls.md` names a canvas's key release.
