# iOS `ModalHost` permanently freezes all input and navigation when UIKit refuses `owner.present(...)`, and `NavigationHost` drops modal routes on `canPresent` refusal

**Status:** Closed
**Resolution:** Validate presenter readiness in ModalHost.canPresent, roll back presentation state when UIKit declines owner.present, and retry waiting modal routes via NavigationHost.modalWaiting and presentationCleared().
**Systems:** ExactNavigation (iOS) — ModalHost, NavigationHost
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Implicit state machines)

Verification: verified by reading.

At `9c0431fd5`, `ModalHost` and `NavigationHost` on iOS have two presentation state-machine defects:

1. **Unrolled-back UIKit presentation refusal (`ModalIOS.swift:568-578, 588, 644, 665-690, 728-729` and `NavigationIOS.swift:654, 748`):**
   - `ModalHost.canPresent(from:route:)` (`ModalIOS.swift:568-578`) only checks `parent.presentedViewController == nil`; it does not check `!parent.isBeingPresented`, `!parent.isBeingDismissed`, or `parent.viewIfLoaded?.window != nil`.
   - In `ModalHost.present` (`ModalIOS.swift:580-700`), before calling `owner.present(controller, ...)`, line `588` calls `syncBackground(layer)` (setting `background.view.isUserInteractionEnabled = false` and `accessibilityElementsHidden = true`, lines `467-472`), line `644` appends `layer` to `layers` (with `layer.presenting == true`, line `267`), and `NavigationHost.syncProjection` (`NavigationIOS.swift:748`) has already appended `nav` to `presentedNavigations`.
   - Line `665` anticipates refusal (`defer { if owner.presentedViewController !== controller { endPress(layer) } } // UIKit refused it`), but only calls `endPress(layer)`: it leaves `layer` in `layers` with `layer.presenting == true`, leaves `background.view` non-interactive and accessibility-hidden, and leaves `nav` in `presentedNavigations`.
2. **Dropped modal route on transient `canPresent` refusal (`NavigationIOS.swift:735-758`):**
   - Line `735` sets `projectionHeld = held` (`false`), then line `744` does `guard presenter.modalHost.canPresent(from: owner, route: route) else { return }`. It returns without setting `projectionHeld = true`, and no alert/dialog dismissal path notifies `NavigationHost` (`MenusIOS.swift:570, 598, 624, 653-674` and `DialogSheetIOS.swift:715, 730` only invoke `menus.resumeAfterRouteDismissal()`).

### Reachable triggers

- **Path A (Permanent UI and navigation freeze):** In an iOS app (for example, an `ExactKit` session embedded in a host controller that is itself being presented or dismissed, where `owner = primaryOwner?.parent` at `NavigationIOS.swift:870` has `presentedViewController == nil` but `isBeingPresented == true` or `isBeingDismissed == true`), project a modal or fullscreen route. `canPresent` returns `true`, and UIKit refuses `owner.present(controller, ...)` without invoking its completion block.
- **Path B (Dropped modal route):** While a `UIAlertController` confirmation (`MenusIOS`) or `<dialog>` sheet (`DialogSheetIOS`) is presented or animating dismissal on `owner`, navigate to a modal route (e.g., from an async event or a dialog action that triggers both route navigation and dismissal), then let the alert/dialog finish dismissing with no subsequent Contract batch.

### Actual vs. expected

- **Actual (Path A):** `layer.presenting` stays `true` forever, keeping `defersFocus` (`ModalIOS.swift:353`) and `inTransition` (`ModalIOS.swift:333-335`) permanently `true`. `NavigationHost.syncProjection` (`NavigationIOS.swift:654`) blocks all future navigation syncs, `background.view` remains permanently non-interactive and hidden from VoiceOver, and even `ModalHost.reset()` (`ModalIOS.swift:821-830`) cannot recover because `drainRetired()` (`ModalIOS.swift:729`) guards `!layer.presenting`, leaving `retiring` non-empty and `inTransition == true` forever.
- **Actual (Path B):** `shownKeys` (`NavigationIOS.swift:388`) advances to the modal route while `canPresent` refuses it; when the alert/dialog finishes dismissing, nothing resyncs `NavigationHost` (and `pendingSync` is `false` so even `modalDidDismiss()` at line `920` would no-op), leaving the modal route unpresented until an unrelated future batch arrives.
- **Expected:** `canPresent` rejects controllers that are `isBeingPresented`, `isBeingDismissed`, or off-window (matching `DialogSheetIOS.swift:876, 882` and `MenusIOS.swift:1293-1295`); if UIKit still refuses `owner.present`, `ModalHost.present` rolls back `layers`, `layer.presenting`, `releaseBackground(layer)`, child containment, and `presentedNavigations` (returning `Bool` to `NavigationHost`); and when `canPresent` defers due to a transient presentation on `owner`, `NavigationHost` holds projection (`projectionHeld = true`) and resumes once `MenusIOS`/`DialogSheetIOS` finishes dismissing.

### Verification and siblings checked

- Verified by reading `ModalIOS.swift`, `NavigationIOS.swift`, `DialogSheetIOS.swift`, and `MenusIOS.swift`.
- Siblings checked:
  - `ExactDialogs/IOS/DialogSheetIOS.swift:876, 882` and `ExactKit/IOS/MenusIOS.swift:1293-1295, 1355, 1362` properly guard `isBeingPresented`/`isBeingDismissed` (and window presence in `DialogSheetIOS`). Note that `DialogSheetIOS.show` (`line 644`, after calling `layOut(dialog)` at line `608`) and `MenusIOS.openConfirmation` (`line 1452`) also lack post-`present` refusal rollback, though their pre-guards prevent the common `isBeingPresented`/`isBeingDismissed` case and neither permanently blocks `NavigationHost` across `reset()`.
  - macOS `SheetsMac.swift:272-273` stacks on `attachedSheet` and does not share this UIKit state machine.

### Suggested fix and regression test

- Guard `!parent.isBeingPresented, !parent.isBeingDismissed, parent.viewIfLoaded?.window != nil` in `ModalHost.canPresent(from:route:)`.
- Make `ModalHost.present(...)` return `Bool` (or roll back completely inside the `owner.presentedViewController !== controller` path): set `layer.presenting = false`, remove `layer` from `layers`, call `releaseBackground(layer)`, detach `navigation` from `controller`, call `endPress(layer)`, and have `NavigationHost` avoid appending `nav` to `presentedNavigations` (or remove/retire it) on failure. Also force `layer.presenting = false` in `ModalHost.reset()` for any unpresented layer (`layer.controller.presentingViewController == nil`).
- On `canPresent` refusal in `NavigationIOS.swift:744`, set `projectionHeld = true`, and have `MenusIOS.resumeAfterRouteDismissal()` (or confirmation/dialog dismissal completions) call `presenter.navigation.modalDidDismiss()` when `presenter.navigation.pendingSync` is true (or `syncOwed`).
- Add UIKit navigation tests covering: (1) presenting a modal route while `owner` is `isBeingPresented`/`isBeingDismissed` or refuses `present`, asserting `modalHost.inTransition == false`, background interaction remains enabled, and subsequent navigation works; (2) navigating to a modal route while a confirmation alert or `<dialog>` sheet is open and verifying the modal route presents automatically after the alert/dialog dismisses.

### Siblings checked

DialogSheetIOS.swift:876,882 and MenusIOS.swift:1293-1295,1355,1362 properly check isBeingPresented/isBeingDismissed before presenting (though DialogSheetIOS.swift:644 and MenusIOS.swift:1452 also lack post-present refusal rollback); macOS SheetsMac.swift:272-273 uses attachedSheet stacking and is unaffected.
