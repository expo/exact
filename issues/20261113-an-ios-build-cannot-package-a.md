# An iOS build cannot package a notification service extension

**Status:** Open
**Systems:** host/apple, notifications, native modules
**Author:** Codex, from the push diary reported in the related issue
**Date:** 2026-11-13
**Related:** issues/20261113-ios-app-cannot-declare-entitlements-or-plist-keys.md

## What is wrong

An ordinary iOS build replaces an app bundle to which its builder added a
notification service extension. The rebuilt bundle has no such extension.
The builder uses a wrapper that builds, packages and reinstalls the patched
app in one step. The diary does not say what the extension does.

`device.push` derives the app's APNs entitlement; it does not package a
notification service extension. `host/apple/extensions.mjs` and
`host/apple/widgets.mjs` package the admitted extensions, while
`rules/DEFERRED.md`'s notifications entry explicitly excludes a notification
service extension. Extra Info.plist keys or entitlements alone cannot create
its executable, bundle, provisioning or signing step.

## What is now true

The commit "An iOS build packages an app's notification service extension
from one recorded source inventory" fixes the build and receipt path described
below. The issue remains open for the device proof named at the end.

The 2026-11-14 panel, under Charlie's delegation, admitted the app's own Swift
notification service extension for Apple's defined purpose: changing a visible
remote notification before it appears. An app puts exactly one
`UNNotificationServiceExtension` subclass named `NotificationService` under
`modules/apple/notification-service/`. An ordinary iOS or iPadOS build packages
and signs it as `PlugIns/ExactNotificationService.appex`; a source edit keeps it
in the rebuilt bundle and moves binary compatibility. The target links no Exact
runtime. It inherits only the app's App Group and shared Keychain group, and a
device build requires a profile for `<app id>.notifications`.

The one source rule is: every non-dot `.swift` file below the folder and the
top-level `PrivacyInfo.xcprivacy` belong, `target` subtrees do not, symbolic
links in the folder or its `modules` and `modules/apple` ancestors and non-UTF-8
names are refused by path, and `/`-joined relative names are ordered by their
UTF-8 bytes. Other visible files are refused. The bake is the sole producer of
that ordered inventory and each member's digest.

A cyclic or otherwise unresolved module root with no service is absence. A
linked ancestor that resolves to a real service remains refused.

The app must grant `device.push` and `device.notifications`. APNs invokes the
extension only for a visible alert with `mutable-content: 1`. Exact still raises
no event and runs no Contract or TypeScript on arrival; a tap treats `navigate`
as an untrusted location. Push-to-start, data-only wake, notification content
extensions, filtering and critical-alert entitlements, and other Exact payload
keys remain refused.

No device result is claimed: no iPhone is connected to this machine.
`packages/push/test/checklist.md` names the real-APNs, locked-device and
distribution-signed checks still owed.

## Verification on 2026-11-14

The fixture's direct Swift reference passed changed-content success,
attachment failure and unavailable-key fallback by constructing
`UNNotificationRequest`s and calling `didReceive`. It then called
`serviceExtensionTimeWillExpire`, observed the controlled download cancel, and
released its callback late. Each case delivered exactly once, and the late
callback neither delivered again nor changed the fallback content. An ordinary
simulator build installed an app containing the service `.appex`; its plist,
App Group entitlement, privacy manifest, signature and dependency closure
matched the derived target.
Repeated ordinary builds and a service-source edit retained the `.appex`; the
edit changed the service tree and compatibility id. An unsigned device archive
contained the ad-hoc-signed extension and its receipt. An explicit
`--extensions none` build omitted it and recorded no service identity or
source input; the next ordinary build restored it. macOS, tvOS and watchOS
builds each refused the folder before compilation with the platform named.

Dot-prefixed entries and `target/` output trees are outside the extension
inventory. The folder is otherwise ordinary app source: freshness, evidence,
watches, captures and declaration scanners apply the same rules they use for
every directory under the app. An ignored file edit may therefore make one iOS
drive stale before it rebakes to the same compatibility id, and deploy captures
ignored files as it does elsewhere. A service-only edit makes a no-hatch Windows
build stale and rebuilds the web target, as any edit under `modules/` does. The
recorded inventory has two jobs: packaging verifies and snapshots exactly its
members, and a tool that already knows a binary is needed can name the changed
member. Packaging gives only the verified private snapshot to `swiftc`; either
verification failure removes it. An edit before the snapshot is refused, while
an edit from a later packaging callback cannot change the compiled or signed
executable. The share and widget packagers retain their separate live-source
window; `QUEUE.md` records that repair.

The app-owned extension receives no required-reason declaration audited for
Exact's runtime. A required-reason API use without the extension's own approved
declaration is refused by API category; a supplied declaration remains the only
reason in the extension's output, including file timestamps.

An iOS assembly-path test now covers the default convention and
`--extensions none`, including the nested identifier, principal class and deep
signature. The binary-input classifier preserves the extension selection from
both kinds of Apple receipt, including an excluded cohort whose source folder
still exists, so unchanged sources produce no binary change. A Cargo fixture
also shows that absent-service iOS and watch-companion bakes with no selection
stay warm, while the ordinary build's complete selection notices creation once.

On the lane's own iOS 27.0 simulator, `xcrun simctl push` accepted a visible
alert with `mutable-content: 1`, but the system did not launch the extension:
the App Group marker remained absent and the simulator log showed ordinary
notification servicing only. This is not counted as a handler failure or a
successful system invocation. No signed iPhone was connected, so real APNs,
locked-device fallback and a distribution-signed extension remain owed.

Production receipt construction pins current main's module-root behavior. An
already-linked `modules/apple` is fresh, including an absolute
link that deploy rewrites relative. Replacing a real directory with a link to
the same names is not detected. A cyclic root is recorded as absent and becomes
stale when the cycle resolves. Retargeting one dangling root to another remains
fresh. A root linked into `target/` records an empty local name set, so
retargeting it to ordinary source is stale locally; published evidence omits
the output alias and reports no change. No case records the module root's link
text. `QUEUE.md` records this general receipt limit.

The same production receipt path keeps a compiler dependency inside the
service folder in both local and published binary evidence, whether the
extension is selected or excluded. Creation of the service folder and a
changed inventory member remain covered. A mutation restoring the old folder
exclusion made the compiler-input regression test fail.
