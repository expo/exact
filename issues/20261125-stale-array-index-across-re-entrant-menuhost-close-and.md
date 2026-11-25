# Stale array index across re-entrant `MenuHost.close` and `DialogHost.close` traps (`Index out of range`) or closes the wrong entry on macOS

**Status:** Open
**Systems:** ExactKit (macOS) — MenuHost, DialogHost, FocusEvents
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (The second call)

Verification: verified by reading.

At `9c0431fd5`, both `MenuHost.close` (`host/apple/Sources/ExactKit/Mac/MenusMac.swift:469-492`) and `DialogHost.close` (`host/apple/Sources/ExactKit/Mac/DialogsMac.swift:174-188`) cache an index into `entries`, invoke nested `close(...)` calls that synchronously change AppKit first responder and dispatch `blur`/`focus` handlers to the runner, and then index and remove from `entries` using the stale offset without re-looking up the target.

### Triggering sequence
**Path A (`MenuHost.close`):**
1. On macOS, open two stacked painted `popover="auto"` popovers (`outer` at index 0, `inner` at index 1 — e.g. with `position-try-fallbacks`, opened via `showPopover` with no invoker at `MenusMac.swift:388-391`, or under `--chrome agent` where `painted == true`), optionally above a `popover="manual"` at index 0. Focus a node inside `inner` that has a `blur` handler (or let `entry.previous` have a `focus` handler) which closes or unmounts `outer` (via `hidePopover("outer")` or toggling its rendering condition) or closes a `popover="manual"` below `outer`.
2. Close `outer` outside a batch (`presenter.applying == false && presenter.preparing == nil`), e.g. via a button with `popovertarget="outer" popovertargetaction="hide"`/`"toggle"` (`PresenterMac.swift:907` -> `MenusMac.swift:276`) or an action command `hidePopover("outer")`/`togglePopover("outer")` (`Session.swift:1255, 1312` -> `MenusMac.swift:316`).
3. `MenuHost.close(outer)` caches `index = entries.firstIndex(where: { $0.popover === pop })` (`MenusMac.swift:472`), then recursively calls `close(entry.popover, restoreFocus: restoreFocus)` for `inner` (`MenusMac.swift:474-478`).
4. Inside recursive `close(inner)`, because `entry.native == nil`, `window` is `presenter?.viewport.window` and `hadFocus` is true (`MenusMac.swift:481-482`). Lines `502-515` call `window.makeFirstResponder(previous)` (line 512) or — even when `restoreFocus == false` — `window.makeFirstResponder(presenter?.viewport)` (line 514).
5. AppKit synchronously invokes `NodeView.resignFirstResponder()` (`FocusMac.swift:46-50`) -> `focusLeft()` (`FocusMac.swift:79-87`) -> `presenter?.blur(id)` (`FocusEvents.swift:43-50`) -> `focusEvents.blur(id)` (`FocusEvents.swift:148-154`) -> `direct(id, entering: false)` (`FocusEvents.swift:126-132`) -> `presenter?.afterBatch { ... }`. Because `!applying && preparing == nil`, `Presenter.afterBatch` (`PresenterMac.swift:991`) runs synchronously -> `Session.swift:836` executes `admit(id) { apply(runtime.blur(id, now: now())) }` (`StagedApply.swift:601, 366` -> `Session.swift:1152`), whose batch runs `menus.sync()`/`menus.children()` or delivers `pendingCommands` synchronously after `applying = false` (`Session.swift:1255, 1312-1313`), mutating `entries`.
6. Unwinding back to `close(outer)` executes `let entry = entries[index]` (`MenusMac.swift:481`) and `entries.remove(at: index)` (`MenusMac.swift:492`) with the stale `index`.

**Path B (`DialogHost.close`):**
1. Open a `<dialog>` containing a painted popover hosted in `presenter.viewport.window` (either a painted dialog under `--chrome agent`, or a native `DialogSheet` whose inner popover was opened with `showPopover(id)` where `source == nil` mounts it in `presenter.viewport` at `MenusMac.swift:334, PopoversMac.swift:125`), with focus on a node inside the popover whose `blur` handler closes or unmounts `dialog` (or closes an earlier dialog beneath `dialog`).
2. Close `dialog` outside a batch (Escape when the popover is `manual` at `DialogsMac.swift:382, 427`, `command="close"` button at `PresenterMac.swift:906` -> `DialogsMac.swift:120`, `outside()` click on `closedby="any"` at `DialogsMac.swift:420`, or `close("dialog")` command at `Session.swift:1313` -> `PresenterActionsMac.swift:17`).
3. `DialogHost.close` re-resolves `i` after `clear(above: dialog)` (`DialogsMac.swift:179-180`), then calls `presenter?.menus.close(popover, restoreFocus: false)` (`DialogsMac.swift:181-183`), which still runs `window.makeFirstResponder(presenter?.viewport)` (`MenusMac.swift:514`) and synchronously fires `blur`, mutating `dialogs.entries`.
4. Unwinding back to `DialogHost.close` executes `entries[i]` (`DialogsMac.swift:185`) and `entries.remove(at: i)` (`DialogsMac.swift:187`) with the stale `i`.

### Actual vs. expected result
- **Actual:** When `entries.count <= index` (e.g. `outer`/`dialog` itself was closed re-entrantly, or a lower entry was closed leaving fewer elements), Swift terminates the process with `Fatal error: Index out of range` at `MenusMac.swift:481` or `DialogsMac.swift:185`. When `entries.count > index` (e.g. a `popover="manual"` below `outer` was closed while another `popover="manual"` sits above `outer`), `entries[index]` and `entries.remove(at: index)` silently close and remove the wrong popover/dialog while leaving the target orphaned in `entries`.
- **Expected:** Both `MenuHost.close` and `DialogHost.close` must uphold Swift array safety and re-resolve `entries.firstIndex` after nested closes (just as `DialogsMac.swift:179-180` already does after `clear(above: dialog)`), returning early if the target was already closed re-entrantly.

### Verification & sibling check
- **Verified by reading** the full synchronous call chain through `MenusMac.swift`, `DialogsMac.swift`, `FocusMac.swift`, `FocusEvents.swift`, `PresenterMac.swift`, `PresenterActionsMac.swift`, `StagedApply.swift`, and `Session.swift`.
- **Siblings checked:**
  - `DialogsMac.swift:180-187`: affected (reported above).
  - `MenusMac.swift:169-174` (`apart`): removes before `body()` and clamps re-insert with `min(index, entries.count)` — safe.
  - `MenusMac.swift:525, 533, 562, 720`, `DialogsMac.swift:227, 234, 251`, `SheetsMac.swift:330`: iterate array copies or re-check `entries.last` — safe.
  - `IOS/MenusIOS.swift:867-869` (`drop`): uses dictionary `agentOpen.removeValue(forKey: name)` *after* recursive `drop(nested)` — safe.
  - `ExactDialogs/IOS/DialogSheetIOS.swift:674-725` (`close`, `dismiss`, `ended`): removes `at` immediately after `entries.firstIndex` before side effects — safe.
  - `Mac/AlertsMac.swift` and `IOS/ContextMenusIOS.swift`: single optional presentation, no cached array index — safe.

### Suggested fix & regression test
- Re-resolve the target index immediately after the nested-close loop in both methods:
  - In `MenusMac.swift:479-481`, insert `guard let index = entries.firstIndex(where: { $0.popover === pop }) else { return }` after the `if entries[index].value != "manual"` loop before indexing `entries[index]` and removing at `index`.
  - In `DialogsMac.swift:184`, move `guard let i = entries.firstIndex(where: { $0.dialog === dialog }) else { return }` to *after* the `for popover in ... { presenter?.menus.close(...) }` loop (keeping a `guard entries.contains(...)` before the loop if needed).
- **Regression test:** Add macOS XCTest cases in `host/apple/tests/ExactKitTests/` covering: (1) closing `outer` with nested painted `inner` whose focused node's `onBlur` calls `menus.close(outer)` (and a second case closing a `manual` popover beneath `outer` with another `manual` above it), asserting no trap and exact surviving `entries`; and (2) closing a `<dialog>` containing a painted popover whose focused node's `onBlur` calls `dialogs.close(dialog)`, asserting no trap and clean teardown.

### Siblings checked

Confirmed sibling defect in DialogsMac.swift:180-187; verified safe in MenusMac.swift (apart, reset, children, sync, release), SheetsMac.swift (clear(above:)), AlertsMac.swift, IOS/MenusIOS.swift (drop uses removeValue after recursion), IOS/ContextMenusIOS.swift, and ExactDialogs/IOS/DialogSheetIOS.swift (ended removes immediately after firstIndex).
