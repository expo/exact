# retireBackgroundFetch on Apple passes a directory to context.file, leaking retired transfer files and returning stale bodies on ID reuse

**Status:** Closed
**Resolution:** Resolve retired transfer directories via context.directory (guarded on non-empty id) in BackgroundTransferModule.later so retired bodies and .placed markers are removed on Apple.
**Systems:** @exact/background-transfer (Apple iOS/macOS)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Halfway failure)

Verification: verified by reading.

At `9c0431fd5`, `retireBackgroundFetch` in `packages/background-transfer/apple/Transfers.swift:49-54` calls `context.file(Self.kept(id))` to resolve the transfer directory before deleting it:

```swift
case "retireBackgroundFetch":
    context.transfers.retire(id: id)
    if let folder = context.file(Self.kept(id)) { try? FileManager.default.removeItem(at: folder) }
    context.log("background-transfer: \(id) retired")
    changed()
    reply([:])
```

`Self.kept(id)` (`Transfers.swift:109-111`) returns the directory path `app:/data/transfers/<encoded-id>`. On Apple, `ExactPackageContext.file(_:)` (`host/apple/modules/ExactPackage.swift:465`) delegates to `ExactExtensions.file(_:_:)` (`host/apple/modules/ExactExtensions.swift:192-198, 478`), which calls `AppFileWalk.destination(root, names)` (`host/apple/Sources/ExactCore/AppFileWalk.swift:48-74`). For an existing final path component, `AppFileWalk.destination(..., kind: S_IFREG)` checks `kind == expected` (`AppFileWalk.swift:60-61`), so any existing directory (`S_IFDIR != S_IFREG`) unconditionally returns `nil`.

### Triggering sequence
1. On macOS or iOS, run a `backgroundFetch` registration with `id` (with or without `saveTo`) to completion. When `registration(id)` runs (`Transfers.swift:115-128`), `place(id, x)` (`Transfers.swift:137-163`) creates the directory `app:/data/transfers/<encoded-id>` and writes `<index>.placed` inside it (plus `<index>` when `saveTo` is omitted).
2. Call `retireBackgroundFetch({ id })`. `context.transfers.retire(id: id)` (`host/apple/Sources/ExactWakeCarriers/Transfers.swift:160-164`) deletes only the carrier's private record under `Application Support`. Line 51 then calls `context.file(Self.kept(id))`, which returns `nil` because `app:/data/transfers/<encoded-id>` is an existing directory, so `FileManager.default.removeItem(at: folder)` never runs.
3. Optionally start a new transfer reusing the retired `id` (permitted by `TransferCarrier.start` at `host/apple/Sources/ExactWakeCarriers/Transfers.swift:174`, since `records[id]` is `nil`) that aborts or fails before any response bytes arrive. `TransferCarrier.shown` (`:305`) omits `x["path"]`, so `place(id, x)` skips lines 142–161 and evaluates line 162 (`fm.fileExists(atPath: mark.path) && fm.fileExists(atPath: dest.path) && !Self.isFolder(dest)`), which succeeds using the leftover `<index>.placed` marker and old file (either `app:/data/transfers/<id>/<index>` or a file at `saveTo`).

### Actual vs. expected
- **Actual:** `app:/data/transfers/<id>` (both kept response bodies and `<index>.placed` markers) is never deleted on retirement, leaking disk storage indefinitely and causing any reused `id` that finishes without a body to falsely report the previous transfer's file (or an existing `saveTo` file) as its response body.
- **Expected:** Per `packages/background-transfer/apple/Transfers.swift:11-14, 134-136`, `host/apple/modules/ExactPackage.swift:460-470`, `packages/background-transfer/test/apple.mjs:165`, and Android's `packages/background-transfer/android/Transfers.kt:489-499`, `retireBackgroundFetch` must remove `app:/data/transfers/<id>` so retired bodies and placement markers are reclaimed and cannot pollute reused IDs.

### Verification & siblings checked
- **Verified by reading:** Traced `Transfers.swift:49-54, 109-111, 137-163` -> `ExactPackageContext.file` (`ExactPackage.swift:465`) -> `ExactExtensions.file` (`ExactExtensions.swift:192-198, 478`) -> `AppFileWalk.destination` (`AppFileWalk.swift:48-74`, whose `nil` return for existing directories is directly asserted in `host/apple/tests/ExactKitTests/AppFilesLinkTests.swift:85-94`), and verified `TransferCarrier.retire`/`start`/`shown` in `host/apple/Sources/ExactWakeCarriers/Transfers.swift:160-175, 297-311`. Confirmed `packages/background-transfer/test/apple.mjs` never checks disk cleanup after `retireBackgroundFetch` and never reuses an `id`.
- **Siblings checked:**
  - `packages/background-transfer/android/Transfers.kt:489-499` — invokes `removePlaced` on `Registrations.kept(key.id)`, which recursively deletes the directory (`TransferFixture.kt:54`). Not affected.
  - `packages/background-transfer/web/index.js` — unsupported stub. Not affected.
  - `packages/browser-automation/apple/BrowserAutomation.swift:82-86` — **same defect class** (tracked as C152: passes directory option `destinationFolder` to `context.file`).
  - `packages/speech-recognition/apple/SpeechRecognition.swift:212` — correctly uses `context.directory(path)`.
  - `packages/contacts/apple/Contacts.swift:261` and `packages/photos/apple/Photos.swift:42, 206` — work around `context.file` by resolving `"<dir>/x"` and taking `.deletingLastPathComponent()` (safe unless a child directory/symlink named `x` exists, though `context.directory` is cleaner).

### Suggested fix & regression test
In `packages/background-transfer/apple/Transfers.swift:51`, resolve `Self.kept(id)` via `context.directory(Self.kept(id))` (guarded on `!id.isEmpty`) before removing it:

```swift
if !id.isEmpty, let folder = context.directory(Self.kept(id)) {
    try? FileManager.default.removeItem(at: folder)
}
```

Extend `packages/background-transfer/test/apple.mjs` (and/or add an Apple unit test) to verify that after `retireBackgroundFetch({ id })`, reading `app:/data/transfers/<id>/0` fails and starting/aborting a subsequent transfer with the same `id` reports no `path`.

### Siblings checked

packages/background-transfer/android/Transfers.kt:489-499 (deletes directory via removePlaced); packages/background-transfer/web/index.js (unsupported); packages/browser-automation/apple/BrowserAutomation.swift:82-86 (same bug, C152); packages/speech-recognition/apple/SpeechRecognition.swift:212 (uses context.directory); packages/contacts/apple/Contacts.swift:261 and packages/photos/apple/Photos.swift:42,206 (use "<dir>/x" deletingLastPathComponent workaround)
