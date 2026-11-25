# setDownloadBehavior on Apple resolves destinationFolder with context.file, rejecting every existing directory while admitting regular files

**Status:** Open
**Systems:** @exact/browser-automation (Apple iOS/macOS)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Trust boundaries)

Verification: verified by reading.

At `9c0431fd5`, `BrowserAutomationModule.later` in `packages/browser-automation/apple/BrowserAutomation.swift:82-86` resolves both `"path"` (for `captureScreenshot`) and `"destinationFolder"` (for `setDownloadBehavior`) through `context.file(path)`:

```swift
for option in ["path", "destinationFolder"] {
    guard let path = args[option] as? String else { continue }
    guard let file = context.file(path) else { return reply(["error": "SyntaxError", "message": "\(name): \(path) names no app file"]) }
    request["\(option)File"] = file.path
}
```

On Apple, `ExactPackageContext.file(_:)` (`host/apple/modules/ExactPackage.swift:465`) calls `ExactExtensions.file(_:_:)` (`host/apple/modules/ExactExtensions.swift:192-198, 478`) -> `AppFileWalk.destination(root, names)` (`host/apple/Sources/ExactCore/AppFileWalk.swift:48-74`), which checks `kind == S_IFREG` on any existing target component (`AppFileWalk.swift:60-61`), returning `nil` for existing directories (`S_IFDIR`) while accepting existing regular files (`S_IFREG`).

### Triggering sequence
1. On macOS or iOS, in an app with `browser.automation` and `fs.write app:/data` grants, call `setDownloadBehavior({ id, destinationFolder: "app:/data/downloads" })` when `app:/data/downloads` already exists as a directory (either created beforehand with `storage.fs.mkdir` or created automatically by an earlier download at `host/apple/webarm/FrameWindows.swift:765`). Runner admission (`runner/src/runner/modules/admit.rs:557-586`) checks only path syntax and `fs.write` grant coverage, so the call reaches `BrowserAutomationModule.later`.
2. Conversely, call `setDownloadBehavior({ id, destinationFolder: "app:/data/existing.txt" })` where `existing.txt` is an existing regular file.

### Actual vs. expected
- **Actual:** Every `setDownloadBehavior` call targeting an existing directory fails with `SyntaxError: setDownloadBehavior: <path> names no app file`. Even if the directory does not exist on the first call, once a single download creates it, every subsequent `setDownloadBehavior` call to that folder fails permanently. Conversely, an existing regular file is accepted by `context.file(path)` and forwarded in `destinationFolderFile`, which `FrameAutomation.swift:587` wraps as `URL(fileURLWithPath: $0, isDirectory: true)` and `FrameWindows.swift:765, 774-775` uses as a parent directory for downloads.
- **Expected:** Per `packages/browser-automation/exact.json:155-170`, `host/apple/modules/ExactPackage.swift:465-470`, `host/apple/Sources/ExactCore/AppFileWalk.swift:71-79`, `host/apple/webarm/FrameAutomation.swift:585-588`, and `host/apple/webarm/FrameWindows.swift:773-776`, `"destinationFolder"` must be resolved with `context.directory(path)` so missing and existing directories are admitted while existing regular files and symlinks are rejected, keeping `context.file(path)` for `"path"` (`captureScreenshot`) only.

### Verification & siblings checked
- **Verified by reading:** Traced `packages/browser-automation/exact.json:155-170` -> `runner/src/runner/modules/admit.rs:557-586` -> `BrowserAutomationModule.later` (`packages/browser-automation/apple/BrowserAutomation.swift:82-86`) -> `ExactExtensions.file` (`host/apple/modules/ExactExtensions.swift:192-198, 478`) -> `AppFileWalk.destination` (`host/apple/Sources/ExactCore/AppFileWalk.swift:48-74`, whose rejection of existing directories and acceptance of regular files is unit-tested in `host/apple/tests/ExactKitTests/AppFilesLinkTests.swift:72-107`) -> `FrameAutomation.swift:585-588` -> `FrameWindows.swift:765, 773-776`. Confirmed `host/apple/tests/ExactKitTests/IframeAutomationTests.swift:830` invokes `exact_web_automation` directly with a pre-populated `destinationFolderFile`, bypassing `BrowserAutomationModule.later`.
- **Siblings checked:**
  - `packages/browser-automation/android/BrowserAutomation.kt` / `host/android/canvas/src/main/java/dev/exact/canvas/FrameAutomation.kt:328-331` and `packages/browser-automation/web/index.js:74` — `setDownloadBehavior` returns `NotSupportedError` on both Android and Web.
  - `packages/background-transfer/apple/Transfers.swift:51` — **same defect class** (tracked as C151: passes directory `Self.kept(id)` to `context.file`).
  - `packages/speech-recognition/apple/SpeechRecognition.swift:212` — correctly uses `context.directory(path)`.
  - `packages/screen-capture/apple/ScreenCapture.swift:169-170` — constructs per-screen child file paths inside `directory` before calling `context.file`.
  - All other `context.file(...)` sites across `packages/*/apple/*.swift` resolve regular files (or use the `<dir>/x` workaround in `Contacts.swift:261` and `Photos.swift:42, 206`).

### Suggested fix & regression test
In `packages/browser-automation/apple/BrowserAutomation.swift:82-86`, resolve `"path"` with `context.file(path)` and `"destinationFolder"` with `context.directory(path)`:

```swift
if let path = args["path"] as? String {
    guard let file = context.file(path) else { return reply(["error": "SyntaxError", "message": "\(name): \(path) names no app file"]) }
    request["pathFile"] = file.path
}
if let folder = args["destinationFolder"] as? String {
    guard let directory = context.directory(folder) else { return reply(["error": "SyntaxError", "message": "\(name): \(folder) names no app folder"]) }
    request["destinationFolderFile"] = directory.path
}
```

Add an Apple test invoking `BrowserAutomationModule.later("setDownloadBehavior", ...)` with an `ExactPackageContext` wired to `AppFileWalk.destination`/`AppFileWalk.directory` verifying that an existing directory and a missing directory are accepted while an existing regular file and a symlink are rejected.

### Siblings checked

packages/browser-automation/android/BrowserAutomation.kt & FrameAutomation.kt:328-331 (NotSupportedError); packages/browser-automation/web/index.js:74 (NotSupportedError); packages/background-transfer/apple/Transfers.swift:51 (same bug, C151); packages/speech-recognition/apple/SpeechRecognition.swift:212 (uses context.directory); packages/screen-capture/apple/ScreenCapture.swift:169-170 (resolves child file paths)
