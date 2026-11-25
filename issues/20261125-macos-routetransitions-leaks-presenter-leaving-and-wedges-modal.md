# macOS `RouteTransitions` leaks `presenter.leaving` and wedges modal sheets on root unmount, leaves popped modal sheets blank, and survives `reset()`

**Status:** Open
**Systems:** ExactNavigation (macOS) — RouteTransitions, RouteSheetMac, NavigationHost
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Implicit state machines)

Verification: verified by reading.

At `9c0431fd5`, macOS `RouteTransitions` (`host/apple/Sources/ExactNavigation/Mac/RouteTransitionMac.swift`) has three state-lifecycle bugs involving `presenter.leaving`, modal route sheets (`RouteSheetMac.swift`), and `NavigationHost.reset()` (`NavigationMac.swift`):

1. **Popped child route inside a modal sheet never replaces/thaws the sheet (`RouteTransitionMac.swift:53-56`, `RouteSheetMac.swift:348`, `host/apple/src/route_parts.rs:31-35`):** `route_parts.rs:31` only excludes routes whose own `navigationPresentation == "modal"`. A pushed child route `C` inside a modal sheet `M` therefore returns a playing `RoutePart` and is stored in `RouteTransitions.top` (`RouteTransitionMac.swift:67`). When `C` is popped and destroyed, `PresenterMac.swift:1229` runs `blocks.destroyed(C.id)` (freezing `routeChrome` via `RouteSheetMac.swift:392`) and line `1236` calls `navigation.keepsForExit(C.id)` -> `transitions.keeps(C.id)` (`RouteTransitionMac.swift:35-41`) -> `beginExit(C.id)` (`PresenceMac.swift:16-55`), inserting `C` into `presenter.leaving`. During `syncRoutes()`, `RouteSheetMac.swift:348` skips `replace(entry, with: M, ...)` while `presenter.isLeaving(entry.dialog)` is true. When the CA animation completes, `RouteTransitionMac.swift:53-56` clears `exiting` and calls `presenter.endExit(kept.id)` (removing `C` from `sheet.content`), but unlike entrance completion (`RouteTransitionMac.swift:62-65`), never calls `navigationHost.resync()` or `dialogs.syncRoutes()`.
2. **Unmounting the navigation root leaks `presenter.leaving` and wedges modal sheets (`NavigationMac.swift:161-164, 176, 209` and `RouteSheetMac.swift:337`):** When `C` is destroyed in a batch that removes the navigation root (`roots.isEmpty`, `NavigationMac.swift:209`) or leaves no matching route (`found == nil`, `NavigationMac.swift:161-164`), `beginExit(C.id)` inserts `C` into `presenter.leaving` and `transitions.leaving`, but `NavigationMac.project` bypasses `transitions.changed(...)` (`line 176`). Consequently `endExit(C.id)` never runs: `C` leaks permanently in `presenter.leaving` as a frozen ghost, and if `C` was in a modal sheet, `RouteSheetMac.swift:337` sees `presenter.isLeaving(entry.dialog) == true` forever and permanently skips `close(entry.dialog)`.
3. **`RouteTransitions` is never reset across `NavigationHost.reset()` (`NavigationMac.swift:74-79`):** `NavigationHost.reset()` clears `refused`, `panels`, `shown`, `sheets`, `keptSheets`, `split`, and `grounds`, but omits `transitions`. Stale `top`, `held`, `exiting`, `leaving`, and active `playing` CA completions survive `Presenter.reset()` / hot reload and collide with the new session's view IDs starting from 1 (`NavigationMac.swift:202-205`, `PresenterMac.swift:1236`).

### Reachable triggers

On macOS, push a child route `C` declaring `::view-transition-old`:
- **Path A:** Push `C` inside a modal sheet route `M`, then pop `C` back to `M`.
- **Path B:** While `C` is the active top route (in a regular stack or inside a modal sheet), unmount the navigation root or remove all routes in a batch (e.g., sign-out to a login screen), yielding `roots.isEmpty` or `found == nil`.
- **Path C:** Trigger `Presenter.reset()` (e.g., hot reload or session restart) while `RouteTransitions` holds `top`, `held`, `exiting`, `leaving`, or an in-flight `playing` animation.

### Actual vs. expected

- **Actual:**
  - Path A: After `C`'s exit finishes, `endExit(C.id)` removes `C` without resyncing `DialogHost`, leaving the AppKit sheet open with blank content (or the frozen `RouteChrome` snapshot from `blocks.destroyed`) and `entry.dialog` still referencing destroyed `C` with stale chrome/buttons, while `M` remains outside the sheet in the main window.
  - Path B: `C` remains permanently in `presenter.leaving` (visible as a frozen ghost when `found == nil`), and any modal sheet hosting `C` is permanently wedged open over the window blocking input.
  - Path C: After reset, new routes whose IDs match stale `held`/`exiting` become inert or wrongly visible (`NavigationMac.swift:202-205`), destroyed views matching stale `top.id` are spuriously kept in `presenter.leaving`, and an old `playing` CA completion can call `endExit` or `resync()` on the new session.
- **Expected:**
  - Path A: Exit completion triggers `navigationHost.resync()` / `dialogs.syncRoutes()` (mirroring entrance completion at `RouteTransitionMac.swift:62-65` and `SplitHost`'s retirement hook in `PresenceMac.swift:63`) so `replace(entry, with: M)` thaws and mounts `M`.
  - Path B: `NavigationMac.project` flushes `transitions` (`endExit` for any kept `leaving` route and clears `top`/`exiting`) whenever `roots.isEmpty` or `found == nil` (or `transitions` does not keep routes that are not popping).
  - Path C: `NavigationHost.reset()` cancels and clears all `RouteTransitions` state, matching `split.reset()` and the rest of `NavigationMac.swift:74-79`.

### Verification and siblings checked

- Verified by reading `RouteTransitionMac.swift`, `RouteSheetMac.swift`, `NavigationMac.swift`, `PresenceMac.swift`, `PresenterMac.swift`, `BlockHostMac.swift`, `DialogsMac.swift`, `SheetsMac.swift`, and `host/apple/src/route_parts.rs`.
- Siblings checked:
  - Entrance completion (`RouteTransitionMac.swift:62-65`) calls `if alone { self?.presenter.navigationHost.resync() }`.
  - `SplitHost` (`SplitMac.swift:316, 334, 510`) relies on `PresenceMac.swift:63` (`navigation.stackSplit.retire(member)`) inside `endExit`; `DialogHost` has no equivalent hook.
  - iOS `RouteTransitionIOS.swift` attaches `exitPart` to `RouteController` (cleared in `NavigationIOS.swift:1378` on `reset()`) and delegates transitions to UIKit rather than `presenter.leaving`.
  - Painted host `host/painted/src/presenter/stage/authored.rs` keys transitions by monotonic `NodeKey` and tracks `sheet_of(kinds, top)` explicitly.

### Suggested fix and regression test

- Add a `reset()` / `clear()` method on `RouteTransitions` that cancels `playing` (incrementing `generation` and removing `Self.key`), calls `presenter.endExit(...)` on any pending `leaving` or `exiting` ID, and clears `held`, `top`, `leaving`, `exiting`, and `playing`. Call it from `NavigationHost.reset()` (`NavigationMac.swift:74-79`) and from `NavigationMac.project` whenever `roots.isEmpty` (`line 209`) or `found == nil` for `roots.first` (`lines 161-164`).
- In `RouteTransitionMac.swift:53-56`, when `alone` is true (or whenever `endExit(kept.id)` retires a route), call `self?.presenter.navigationHost.resync()` (which schedules `dialogs.syncRoutes()` via `NavigationMac.swift:211`, or invoke `dialogs.syncRoutes()` directly after `endExit`) so a modal sheet holding `kept` replaces with `M` or closes cleanly.
- Add macOS navigation tests for all three paths: (1) popping a `::view-transition-old` child pushed inside a modal sheet verifies the sheet thaws and displays `M`; (2) unmounting the navigation root while a `::view-transition-old` child is active (stacked and in a modal sheet) verifies `presenter.leaving` is empty and the sheet closes; (3) calling `Presenter.reset()` mid-transition verifies `RouteTransitions` state is cleared and new routes with ID 1+ are neither inert nor retained.

### Siblings checked

Entrance completion (RouteTransitionMac.swift:62-65) calls resync(); SplitMac.swift:316,334,510 is retired by PresenceMac.swift:63 inside endExit while RouteSheetMac has no hook; iOS RouteTransitionIOS.swift stores exitPart on RouteController (cleared in NavigationIOS.swift:1378); painted host stage/authored.rs uses monotonic NodeKey and tracks sheets explicitly.
