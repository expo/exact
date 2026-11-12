# There is `swiperight` but no `swipeleft`

**Status:** Closed
**Resolution:** `swipeleft` (2aa560c2) now also passes on iOS: `SwipeScrollDragTests` on an iOS 27.0 simulator, and a `--touch platform` drive where a real 150 pt left drag ran the action once and a 30 pt one or a rightward one ran nothing.
**Systems:** gestures (runner, every host)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Excord's stage 1 transcript lane (`apps/excord/llp/0001-excord.rfc.md`)
**Date:** 2026-10-22

Status note (moved verbatim off the **Status:** line by `bun scripts/issue.mjs`; cdcstack issue statuses are exactly `Open` or `Closed`): an ask

## Symptom

Discord's iOS app replies to a message by swiping it left. Exact has `swiperight`;
Excord builds the left swipe from `pan` and `panrelease`, which runs one action per
drag frame where the platform's gesture would run one at its end.

## 2026-11-10: `swipeleft` landed; iOS not yet run

Commit 2aa560c2 adds `swipeleft` (Charlie, 2026-11-10), the mirror of
`swiperight`: the node follows the finger leftward, the 64 pt knee and the
resistance past it are `swiperight`'s, and one action runs as the finger lifts
past the knee. Direction is physical, as `swiperight`'s, in either writing
direction. It runs on the web (both targets), iOS, macOS and the presenter
Linux, Windows and Android share.

Verified: the runner and compiler tests (`contract/cli/tests/it/scroll_drag.rs`,
`swipeleft mirrors swiperight`); the web host's (`host/web/collection.test.mjs`,
`host/web/tests/it/holds.rs`) and the JS target's conformance step
(`host/web-js/conformance/swipeleft.steps`); the presenter's
(`host/linux/src/presenter/swipe_left_tests.rs`); a web drive on both targets,
where a 150 px leftward drag ran the action once and a 30 px one or a rightward
one ran nothing; and a macOS drive. Not run: iOS, whose XCTest
(`SwipeScrollDragTests`) and touch drive need this machine's CoreSimulator
updated (QUEUE). Close this once they pass.
