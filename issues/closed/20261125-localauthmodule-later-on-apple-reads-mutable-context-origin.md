# LocalAuthModule.later on Apple reads mutable context.origin inside DispatchQueue.main.async, letting interleaved calls overwrite the origin check

**Status:** Closed
**Resolution:** Handle LocalAuthModule.later synchronously on the main thread so context.origin and context.request are captured at entry before any interleaved call can overwrite them.
**Systems:** @exact/local-auth (Apple iOS/macOS)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `LocalAuthModule.later` in `packages/local-auth/apple/LocalAuth.swift:57-65` wraps its dispatch switch in `DispatchQueue.main.async { [self] in ... }` and reads `context.origin` inside that deferred closure (`LocalAuth.swift:61`) rather than synchronously at entry:

```swift
public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
    DispatchQueue.main.async { [self] in
        switch name {
        case "localAuthStatus": reply(status())
        case "authenticate": authenticate(args, origin: context.origin, reply)
        default: reply(["error": "NotSupportedError"])
        }
    }
}
```

`ExactPackageContext.origin` is a single mutable property on the per-session context (`host/apple/modules/ExactPackage.swift:278-283`) overwritten synchronously before every `later` and `evaluate` call by `LoadedPackageAdapter` (`host/apple/modules/ExactPackageLibrary.swift:94-98,122-127`).

### Triggering sequence
On macOS or iOS with `@exact/local-auth` bound (`Route::Real`):
1. The runner forwards an `"app"`-origin `authenticate` send and a non-`"app"` (`"tool"`, `"wake"`, `"device"`, or `"action"`) `localAuthStatus` read (`packages/local-auth/exact.json:89-96` has no `origin` restriction, and `runner/src/runner/modules/bound.rs:251` always attaches `"origin"`).
2. `nativeLaterCallback` (`host/apple/Sources/ExactNativeViews/NativeModule.swift:381-391`) enqueues both requests on `DispatchQueue.main`.
3. On the main queue, the first block enters `LoadedPackageAdapter.later`, sets `context.origin = "app"`, and calls `LocalAuthModule.later("authenticate", ...)`, which enqueues its inner `DispatchQueue.main.async` block **behind** the already-queued `localAuthStatus` block.
4. The next main-queue block enters `LoadedPackageAdapter.later` for `localAuthStatus` (or `LoadedPackageAdapter.evaluate` from `PresenceHost.check` in `host/apple/Sources/ExactPresence/Presence.swift:177-207`) and overwrites `context.origin` to the non-`"app"` origin.
5. The deferred `authenticate` block then runs and reads the overwritten `context.origin`, passing it to `authenticate(_:origin:_:)` (`LocalAuth.swift:207-209`).

Symmetrically, if a non-`"app"` `authenticate` call reaches `LocalAuthModule.later` without the runner's `origin == "person"` gate (which `LocalAuth.swift:208` explicitly aims to hold independently), a subsequent `"app"` `localAuthStatus` or `evaluate` call overwrites `context.origin` to `"app"` before the deferred block executes, bypassing both `guard origin == "app"` (`LocalAuth.swift:209`) and the `origin == "app"` guard on `touchIDAuthenticationAllowableReuseDuration` (`LocalAuth.swift:161`).

### Actual vs. expected
- **Actual:** A legitimate `"app"`-origin `authenticate` call fails `guard origin == "app"` (`LocalAuth.swift:209`) with `{"error": "PERMISSION_DENIED", "message": "person-required"}` whenever a non-`"app"` `localAuthStatus` or `evaluate` call runs before the deferred block. Conversely, the module's own non-delegable presence guard and reuse-duration guard can be bypassed by an interleaved `"app"` call.
- **Expected:** Each `authenticate` call executes under the `origin` it arrived with. Established by `host/apple/modules/ExactPackage.swift:9,278-283` (every `ExactPackage` entry runs on the main thread; `origin` is set before each `later` and read at entry), `host/apple/modules/ExactPackageLibrary.swift:94-98,122-127`, `packages/local-auth/apple/LocalAuth.swift:160-161,208-209`, and LLP 1133.010 D12.

### Verification and siblings checked
Verified by reading the full dispatch path from `runner/src/runner/modules/bound.rs:251` through `NativeModule.swift:381-391`, `ExactExtensions.swift:301-302`, `ExactPackageLibrary.swift:94-98,122-127`, and `LocalAuth.swift:57-65,156-171,207-221`.

Audited all sibling Apple packages reading `context.origin` or `context.request` (`Alarms.swift:46`, `Calendar.swift:60`, `Camera.swift:230,331,374`, `Clipboard.swift:154`, `Compose.swift:153,211,263`, `Geocoding.swift:23`, `InAppBrowser.swift:99`, `LanguageModel.swift:334,461`, `MediaCapture.swift:117,121,131`, `Nfc.swift:272`, `Passkeys.swift:152,217,293,394`, `Payments.swift:208,243`, `Photos.swift:52`, `Recording.swift:125,145,169,282`, `ScreenCapture.swift:64`, `SignIn.swift:68,80`, `StoreReview.swift:52,63,71`, `StoreUpdate.swift:224`, `TrackingConsent.swift:43`, `Translation.swift:234`) and Android `packages/local-auth/android/LocalAuth.kt:63-69` (passes per-call immutable `Cause`). None of those siblings read `context.origin` or `context.request` inside a deferred async closure.

### Suggested fix and regression test
Remove the redundant `DispatchQueue.main.async` wrapper in `LocalAuthModule.later` (since `ExactPackage` guarantees main-thread delivery), or capture `let origin = context.origin` (and `let request = context.request` for cancellation) synchronously at the top of `later` before scheduling any async work. Add a Swift unit test calling `later("authenticate", ...)` with `context.origin = "app"` and immediately setting `context.origin = "tool"` and calling `later("localAuthStatus", ...)` before draining the main run loop (and the reverse ordering), asserting that the `"app"` call does not fail with `PERMISSION_DENIED` and a `"tool"` `authenticate` call still returns `PERMISSION_DENIED`.

### Siblings checked

Checked all Apple packages reading context.origin/context.request (Alarms, Calendar, Camera, Clipboard, Compose, Geocoding, InAppBrowser, LanguageModel, MediaCapture, Nfc, Passkeys, Payments, Photos, Recording, ScreenCapture, SignIn, StoreReview, StoreUpdate, TrackingConsent, Translation) and Android LocalAuth.kt; none share this defect.
