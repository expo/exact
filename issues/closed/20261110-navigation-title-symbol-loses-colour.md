# A coloured symbol beside an iOS navigation title is drawn in the title's colour after its first projection

**Status:** Closed
**Resolution:** `HeaderTitleView.update` takes the heading's colour and sets the label's `textColor` only as it builds the line, so a projection that changes nothing leaves a coloured symbol its own colour.
**Systems:** host/apple/Sources/ExactKit/IOS/NavigationTitleIOS.swift (richTitle, HeaderTitleView.update)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

A header whose heading shares a flex row with symbol images is drawn in the bar as a `HeaderTitleView`, the symbols inline in the title (`host/apple/Sources/ExactKit/IOS/NavigationTitleIOS.swift:71-87`). A symbol whose author wrote its `color` carries it (`contract/lower/src/symbols/ink.rs:6-10`): `titleLine` puts it on the symbol's range of the attributed title (lines 337-340) and `update` sets that as `title.attributedText` (line 368).

`richTitle` runs for every route at every navigation sync, which is each batch that is not rows only (`NavigationTabsIOS.swift:465`, `NavigationBarIOS.swift:557`). Each time it assigns the label's colour, then calls `update` (lines 534-535):

```swift
view.title.textColor = shape.heading?.ownUIColor("text_color") ?? .label
view.update(group, title: shape.title)
```

On a label that holds styled text, "assigning a new value to this property applies the color to the entirety of the string in the attributedText property" (Apple, `UILabel.textColor`), and UIKit does so when the colour assigned is the one the label has. `update` then returns early, because the group, the title and the ink are what it last showed (line 357), so the line is not built again. Nothing else writes the label.

## The failure

```
header display="flex" align-items="center" gap=4
  text "Ada" role="heading"
  image "symbol:sf/checkmark.seal.fill" color="-exact-system-blue"
```

as a route's header. Expected: a blue seal, as written and as the web paints it. By the reading above the seal is the title's colour from the second projection on, which can come in the stack's first sync (`NavigationIOS.swift:655`, then 805).

Few apps reach this. None in this repository writes the shape, no guide shows it, and `contract build` warns on it: `lower-header-unplaced` says the image "is not shown on iOS" (`contract/lower/src/warnings.rs:309`, `503-512`), which lines 71-87 contradict.

## Verification

Run: UIKit, as a bare Mac Catalyst process (the macOS 27.2 SDK; no iOS simulator runs on this machine). A `UILabel` was given a line built as `titleLine` builds it, then line 534's assignment again, and each range's colour was read back:

```
after the first projection:   ["Ada<thin>=label", "<symbol>=rgb(0,102,255)"]
after the second projection:  ["Ada<thin><symbol>=label"]   update returns early: true
```

The same with the symbol before the heading. Drawn into a bitmap, the label had 229 blue pixels after the first assignment and none after the second. `cargo test -p contract --test it symbol_ink` passes (the lowering).

Read, not run: the presenter (`richTitle`, `update`, `prepareRoutes`, `sync`), which does not build for the Mac test target. `NativeBarDetailsIOSTests.testAHeadingRowsSealIsDrawnBesideTheTitle` assigns the colour and calls `update` once, so it does not see the second projection.

The check that would settle it: on a simulator, a route with the header above; press anything and look at the seal's colour in a screenshot.

## Direction

Hand the ink to `update`, compare it with the ink last shown, and assign `title.textColor` only where the line is rebuilt, before building it. Compare the ink handed in, not `title.textColor` read back: with a symbol first in the line the label reports the symbol's colour (Apple, `UILabel.attributedText`: the style properties "reflect the style information starting at location 0").

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` for the title view's symbols.
