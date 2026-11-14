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

## Decision needed

Obtain the extension's purpose and the notification behaviour the app needs
from its builder. Decide that capability's admission and packaging before
adding an extension target. The metadata refusals and recipe fixes in the
related issue leave this request unresolved.
