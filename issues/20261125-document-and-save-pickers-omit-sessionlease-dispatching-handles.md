# Document and save pickers omit SessionLease (dispatching handles/events to replaced or destroyed sessions) and leak scratch directories on iOS showSaveFilePicker

**Status:** Open
**Systems:** ExactPickers (iOS, macOS)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `ExactPickers` (`DocumentPickers.swift` and `SaveFile.swift`) omits the `SessionLease` generation/lifetime guard that sibling `Picker.swift` enforces, and iOS `showSaveFilePicker` leaks its temporary staging directory on every invocation:

1. **Missing `SessionLease` in `DocumentPickers.swift` and `SaveFile.swift`:**
   - `ExactSession.picker` is a single `lazy var` (`host/apple/Sources/ExactKit/Session.swift:409`) that outlives hot reloads and live updates (`presentCommitted`, `Session.swift:1026-1051`). `ReplacementQueue.interactionHold` (`host/apple/Sources/ExactKit/Replacement.swift:288-308`) does not hold reloads while an `NSOpenPanel`, `NSSavePanel`, or `UIDocumentPickerViewController` is open.
   - `Picker.swift` (`host/apple/Sources/ExactPickers/Picker.swift:46, 77, 80-86, 306`) explicitly captures `SessionLease(session)` (`host/apple/Sources/ExactKit/Generation.swift:17-26`) on every `showPicker` request and drops completions once `r.issued.session !== session` (both after a reload/plan replacement via `runtime.completions.replace()` and after `session.destroy()` via `runtime.completions.close()`).
   - Neither `DocumentPickers.swift` nor `SaveFile.swift` captures a `SessionLease` or checks `session.state != .destroyed`:
     - iOS `presentDocument` (`host/apple/Sources/ExactPickers/DocumentPickers.swift:181, 207-212`; `Picker.swift:41`) stores only `(view, name)` in `documentRequests` and dispatches `documentsChosen(view, urls, name)` / `documentCancelled(view, name)` unconditionally.
     - macOS `presentDocument` (`DocumentPickers.swift:236-240`) captures raw `view: UInt32` and dispatches `documentsChosen` / `documentCancelled` unconditionally.
     - iOS `presentSave` (`host/apple/Sources/ExactPickers/SaveFile.swift:145, 150-155`; `Picker.swift:39`) stores only `(view, dir)` in `exports` and dispatches `saved(view, ...)` / `saveCancelled(view)` unconditionally.
     - macOS `presentSave` (`SaveFile.swift:169-176`) captures raw `view: UInt32` and runs `copyOut(view: view, file: file, to: url)` / `saveCancelled(view)` unconditionally.
   - When a reload or live plan replacement commits while any of these sheets/panels is open (note that retained views keep their numeric `view` ID across reloads, `SessionLeaseTests.swift:153`), confirming or cancelling the stale picker still mints security-scoped `doc:` handles in the new generation (`DocumentPickers.swift:124-126`), adds files to Open Recent (`:134`), copies out the old generation's file (`SaveFile.swift:174`), dispatches `valueChange(view, ...)` / `pickerCancel(view)` into the replacement generation, and (under iOS `--touch platform`) sends `exact_answer_ticket` (`DocumentPickers.swift:114-119`) with a stale numeric ticket from the previous runner (`documentTickets` is never cleared on reload). If `session.destroy()` ran while the panel/picker was open, the completion still calls into the destroyed runtime (`mintDocument`, `valueChange`, `pickerCancel`, `exact_answer_ticket`).
2. **Leaked scratch directory on iOS `showSaveFilePicker` (`DocumentPickers.swift:164-173, 181, 187-191, 207-212`):**
   - For `showSaveFilePicker` on iOS, `presentDocument` creates `$TMPDIR/exact-save-<UUID>/<suggestedName>` (`:167-170`) and passes it to `UIDocumentPickerViewController(forExporting: [staged], asCopy: false)` (`:173`).
   - Unlike sibling `SaveFile.swift:133-155` (which stores `dir` in `exports` and removes it in `exportFinished`), `dir` is a local variable that is never stored or removed — leaking the empty `exact-save-<UUID>` directory on every save, both directory and staged file on every cancel or `closeDocument(view)` (`:187-191`), and `dir` itself if `createDirectory` succeeds and `createFile` fails (`:169-171`).

### Trigger
1. Open `showOpenFilePicker`, `showDirectoryPicker`, `showSaveFilePicker`, or `saveFile` on macOS or iOS; trigger a dev reload or live plan replacement (`ExactSession.apply` / `ExactApp.applyGeneration`) — or destroy the session — while the panel/sheet is open, then confirm or cancel it.
2. Separately, call `showSaveFilePicker` on iOS and either save or cancel: `$TMPDIR/exact-save-<UUID>` remains in temporary storage afterward.

### Actual vs. Expected
- **Actual:** Stale document/save picker completions execute side effects (`copyOut`, `mintDocument`, `ExactDocuments.chosen`, `exact_answer_ticket`) and dispatch `change`/`cancel` events into the replacement generation or destroyed runtime; iOS `showSaveFilePicker` leaks `$TMPDIR/exact-save-<UUID>` on every call.
- **Expected:** Matching `Picker.swift:80-86`, `SaveFile.swift:32-48, 133-155`, `SessionLeaseTests.swift:147-173`, and LLP 1069.010, every `DocumentPickers` and `SaveFile` presentation captures `SessionLease(session)` and drops stale completions after cleanup (`done()` / scratch-dir removal), and iOS `showSaveFilePicker` removes its `exact-save-<UUID>` scratch directory on completion, cancellation, `closeDocument`, and setup failure.

### Verification & Siblings Checked
- **Verified by reading** `host/apple/Sources/ExactPickers/{Picker.swift,DocumentPickers.swift,SaveFile.swift}`, `host/apple/Sources/ExactKit/{Session.swift,Generation.swift,Replacement.swift}`, `host/apple/Sources/ExactCore/RuntimeCompletion.swift`, and `host/apple/tests/ExactKitTests/SessionLeaseTests.swift`.
- **Siblings checked:**
  - Apple `host/apple/Sources/ExactPickers/Picker.swift:46, 77, 80-86, 306`: uses `SessionLease(session)` properly.
  - Apple `host/apple/Sources/ExactPickers/SaveFile.swift:32-48, 133-155`: cleans up both `exact-text-<UUID>` and `exact-export-<UUID>` scratch directories properly.
  - Apple `host/apple/Sources/ExactSharing/Share.swift:38-43, 90`: cleans up `exact-share-<UUID>`; outside agent mode (`ticket == nil`) only logs, though under `--touch platform` (`ticket != nil`) its `outcome` closure also calls `runtime.ask(exact_answer_ticket, ...)` without a `SessionLease` check.
  - Web `host/web/documents-glue.js:593-594, 624, 641` and `host/web/picker-glue.js:156, 211, 230`: guard dispatches with `since()` (`host.generation?.() === g`).
  - Painted/Linux/Windows `host/painted/src/presenter/replace.rs:242` (`picks_replaced()`): drops all waiting document/import/export dialogs on reload.

### Suggested Fix & Regression Test
- Capture `let issued = SessionLease(session)` when presenting in `DocumentPickers.swift` and `SaveFile.swift` (storing `issued` alongside `(view, name, scratchDir?)` in `documentRequests` and `(view, dir, issued)` in `exports`, and capturing `issued` in macOS completion handlers); on completion, always run scratch cleanup (`try? FileManager.default.removeItem(at: dir)` / `done()`), then `guard issued.session === session else { return }` before calling `copyOut`, `documentsChosen`, `documentCancelled`, `saved`, `saveCancelled`, or `answerHold`.
- In iOS `DocumentPickers.swift`, remove `dir` in `createFile` failure, `closeDocument`, and `documentFinished`.
- Extend `host/apple/tests/ExactKitTests/SessionLeaseTests.swift` with tests covering pre-reload and post-destroy completions for `DocumentPickers` and `SaveFile` (verifying no `change`/`cancel` runs and `done()` / scratch dirs are cleaned up).

### Siblings checked

host/apple/Sources/ExactPickers/Picker.swift:46,77,80-86 (uses SessionLease properly); host/apple/Sources/ExactPickers/SaveFile.swift:32-48,133-155 (cleans up exact-text- and exact-export- scratch dirs); host/apple/Sources/ExactSharing/Share.swift:38-43,90 (cleans up scratch dir, minor sibling gap: exact_answer_ticket under --touch platform omits SessionLease); host/web/documents-glue.js:593-594 and host/web/picker-glue.js:156 (check generation via since()); host/painted/src/presenter/replace.rs:242 (picks_replaced drops all waiting file dialogs on reload)
