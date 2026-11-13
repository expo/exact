# On iOS `intersect` reports every element intersecting at mount and fires nothing while the content scrolls

**Status:** Open
**Systems:** host/apple/Sources/ExactKit/IOS/NavigationIOS.swift, host/apple/Sources/ExactKit/IOS/NodeViewIOS.swift, runner/src/runner/intersect.rs, runner/src/geometry/intersection.rs
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`intersect=` is documented on every host: "AppKit, UIKit and Linux use the shared native sampler after resize settlement and presented scrolling" (`docs/reference.md`, "Intersection visibility"). The sampler (`Runner::intersect_due`, `runner/src/runner/intersect.rs:25`) computes each target against its nearest scroll root from the kernel's frames and the offsets the presenter reports through `exact_scrolled` (`Runner::scrolled`). It never learns of a scroll the presenter does not report.

On iOS an element's own scroller reports its offset (`NodeViewIOS.scrollViewDidScroll`, `NodeViewIOS.swift:791`) and so does the document (`ScrollPumpIOS.swift:208`). A route's page scroller does not: `NavigationIOS.scrollViewDidScroll` (`NavigationIOS.swift:277-287`) updates stickies, transforms, video visibility, the scroll pump and the frame sampler, and never calls `presenter.onScrolled`. Content scrolled by that native page would then keep the offset it mounted at in the runner's view. That is a likely cause of what the reporter saw, not a confirmed one.

The tests cover only an element's own scroller: the Rust side with explicit `scrolled` calls (`host/apple/src/resize.rs:240`), and `IntersectionIOSTests` (`host/apple/tests/ExactKitTests/IntersectionIOSTests.swift`), which mounts `scripts/fixtures/intersect.contract` in a bare window and scrolls its `scroll` nodes' `UIScrollView`s. Nothing tests content in a route's page, or in a `list virtualized=true`.

## The failure

The port of a large React Native app (iOS, exact2 `94b8671f`) found that every observed element reported `isIntersecting` at mount and no enter or leave arrived while scrolling. The infinite-scroll pattern in the reference (a marker with `intersect=loadMore` after the content) loaded every page in turn, and gating media on `intersect` mounted a map and a playing video in every card of the list. The workaround was to compute visibility from `frame()` in a scroll handler. Which scroller held the content (a `scroll`, a route's page, or a `list virtualized=true`) was not reported.

Not run here: no simulator was started.

## Fix

Reproduce on the iOS simulator with `scripts/fixtures/intersect.contract` placed in a pushed route whose page scrolls natively, and with the reference's paging marker. If the route's page is the gap, report its offset from `NavigationIOS.scrollViewDidScroll` as the other scrollers do. Extend `IntersectionIOSTests` with the fixture inside a navigation route, scrolled by its page, expecting one leave and one enter.
