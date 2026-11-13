# Disabling an Apple GPU canvas discards releases of its held input

**Status:** Closed
**Resolution:** A disabled or inert canvas still receives the release of a key, button or touch its adapters hold, and the Mac pointer adapter clears its button on that release, so the next press is a press.
**Systems:** host/apple
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, disable a live GPU canvas while a mouse button or keyboard
key is held, release it while disabled, then enable the canvas again. The
surface receives the initial down but no up or cancellation. Its world can
keep the input held until a later blur. On macOS the pointer adapter also
keeps its own button bit and converts the next fresh down into a move.

The transition crosses two guards:

- `host/apple/Sources/ExactSurfaces/CanvasInputMac.swift:133` rejects every
  pointer phase on a disabled view before the up branch clears `buttons`
  at lines 149-152. After re-enabling, line 146 changes the next down into
  a move because `buttons` is still nonzero.
- `host/apple/Sources/ExactSurfaces/CanvasSeams.swift:478-483` rejects all
  input except blur on a disabled view. The native key-release paths in
  `CanvasInputMac.swift:100-108` and `CanvasInputIOS.swift:200-207`, and
  UIKit's pointer-release path at `CanvasInputIOS.swift:168-180`, call this
  guarded overload. Removing the adapter's local key or touch does not
  release the surface's input state.

Disabling the canvas supplies no compensating blur. The native props path
updates its disabled appearance (`ExactKit/Mac/PresenterMac.swift:45-54`),
and focus fixup leaves a canvas focused: `formDisabled` only includes
button, input, textarea and control (`ExactKit/KeyEvents.swift:20-29`;
`ExactKit/Mac/NavigationMac.swift:247-268`). `CanvasLifecycle` in
`CanvasSeams.swift:665-820` observes app visibility and audio lifecycle,
not this node's disabled state. The control-contact release path already
uses the overload with the original entry/module identity, bypassing the
new-input gate; raw pointer and key releases do not.

Verification is by tracing these production paths and the existing
`CanvasPointerMacTests.swift` fixture; no runtime or native UI test was run.
The deterministic branch sequence is down → disabled up returns before
clearing → enabled down becomes move.

Cancel owned raw input when the canvas stops accepting it, or deliver the
final releases to the original surface independently of whether it can
accept a new press. Cover mouse buttons, held keyboard keys, and touch
up/cancel across disable and re-enable, including a canvas that retains
keyboard focus throughout.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies. The closed
`20261110-ios-canvas-space-enter-release-never-sent.md` fixed removal of
Space/Enter from the local key set before surface-control dispatch. This
failure is the later disabled-state gate and also affects the Mac pointer
state and other held keys.
