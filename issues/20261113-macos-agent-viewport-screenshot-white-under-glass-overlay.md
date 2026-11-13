# A macOS agent `screenshot` of a glass page under the title bar is all white, while `screenshot … window` is right

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/Mac/AgentMac.swift (`screenshot`), host/apple/Sources/ExactKit/Mac/CaptureMac.swift, NodeViewMac.swift (`GlassBackground`)
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20260904-macos-offscreen-capture-can-be-transparent.md

## What is wrong

`Agent.screenshot` (`AgentMac.swift:1236`) has two paths. With `window` it asks the
window server for the window's picture (`ownWindowImage`). Without it, it draws
`presenter.navigation.split.agentRoot` itself: `Capture.picture(of:)` runs
`cacheDisplay` over the view tree (`AgentMac.swift:1274-1281`, `draw(_:to:)` at
`:1359`), with iframes and native views swapped for snapshots. In an app whose page
fills the window under the title bar (`viewport-fit="cover"`, every route
`navigationBar="none"`, `display_override: ["window-controls-overlay"]`) and has
`backgroundMaterial="glass"` in its tree, the second path writes an all-white picture.

Not reproduced here (this checkout runs on Linux). A likely cause, unconfirmed: a glass
node is an `NSGlassEffectView` whose `contentView` holds the node's children
(`NodeViewMac.swift:33`, `syncMaterialVisibility` at `:615`). The window server
composites the glass and its content; `cacheDisplay` may draw neither, so a glass box at
or near the root leaves only the window's background. Nothing in `CaptureMac.swift`
handles a glass material.

## The failure

On Exact `5434f432`, macOS 26.6, Apple silicon:

```
agent macos --size 1280x800 "clock +1500 real" "screenshot a.png" "screenshot b.png window"
```

`a.png` is 1280 × 800 and white (about 4 KB); `b.png` shows the app. The result was the
same with the driver window uncovered on a second display. The reply gave no sign that
the picture was empty.

## Fix

Reproduce with a host test: a root with `backgroundMaterial="glass"` and a coloured child,
captured by `Capture.picture(of:)`. If the glass's content is what goes missing, draw
`NSGlassEffectView.contentView` into the capture as `withGuests` does for iframes, or fill
the glass's area as `paintedContent` fills a covered canvas. Until then, a picture that
comes out one flat colour while the tree has visible nodes should say so in the reply and
point to `screenshot <png> window`.

## Verification

The blank viewport does not reproduce on macOS 27. `GlassGroupMacTests.testViewportCaptureKeepsTheContentOfAGlassRoot`
captures a full-size content view with controls overlay, a cover viewport, a glass
root and a red child. The child remains red in `Capture.picture(of:)`. A CLI drive
of the glass-root plan also captures its red child and text.

`NodeViewMac.swift` places the authored children in `NSGlassEffectView.contentView`,
and `Capture.draw` in `Mac/AgentMac.swift` captures those descendants through
`cacheDisplay`. No glass replacement is needed on this OS. The reported macOS 26.6
environment was unavailable; window-server comparison was also unavailable because
the current screen supplied no window image.

A macOS 26.6 run is owed. The macOS 27 regression test does not establish that
the reported capture failure is fixed; no capture implementation changed.
