# Apple button measurement caches retain every face and width for the session

**Status:** Open
**Systems:** host/apple
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, both Apple button measurement caches keep every distinct
serialized face, width, offer kind and trait combination until the session
is released. The key owns the complete face's `Data`, including its title.
Neither cache has an entry or byte limit, eviction, or a clear operation.

- AppKit inserts each miss into `entries` at
  `host/apple/Sources/ExactKit/Mac/ButtonMeasureMac.swift:27-39`.
- UIKit inserts each measured miss at
  `host/apple/Sources/ExactKit/IOS/ButtonMeasureIOS.swift:74-76`. Its numeral
  shortcut also inserts every distinct count into `entries` at lines 49-53,
  even though those results can be derived from one shared reference.
- `ExactSession` owns one cache for its lifetime (`Session.swift:358`) and
  installs it in both text engines (`Mac/SessionControlTextMac.swift:11-14`,
  `IOS/SessionControlTextIOS.swift:10-13`). Changing traits updates the key's
  inputs without discarding old entries. Removing or reusing a button does
  not release its cached face.

A long-running session with changing button labels or counts, a feed of
distinct button faces, or repeatedly changing width offers therefore retains
its entire measurement history. Virtualizing the views does not bound this
dictionary. UIKit also retains the separate numeral-reference dictionary,
including its attributed strings. This is avoidable memory growth in a cache
whose answers can be recomputed.

**Verification.** Read both complete cache implementations and their session
ownership and installation paths. Each new key follows the insertions above;
the only early return requires an already present key. Their private
dictionaries have no removal path. This is source-confirmed retention, with
no native measurement loop or heap-size claim. A regression can call `answer`
with successive titles at one offer, then one title at successive widths,
and observe the cache's retained entry and byte counts after each group.

**Direction.** Bound cached faces and numeral references by entries and bytes,
keeping recent results and releasing obsolete trait variants. Avoid storing
every derived numeral result when its shared reference is sufficient. Cover
changing numeric and nonnumeric titles, width changes, trait changes and view
removal, while checking that repeated live faces still reuse measurements.

**Duplicates checked.** Searched open and closed filesystem reports and all
275 cached GitHub issue titles and bodies. The closed
`20261110-apple-font-cache-keeps-every-size.md` bounds `TextEngine.fonts`, a
different dictionary. The closed
`20261110-count-button-shortcut-reuses-wrapped-height.md` corrects numeral
measurement geometry; it does not bound retained button faces.
