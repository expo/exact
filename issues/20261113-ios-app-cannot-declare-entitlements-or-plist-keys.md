# An iOS app cannot declare the entitlements, Info.plist keys or privacy manifest its own native code needs

**Status:** Open
**Systems:** host/apple/build.mjs (`entitlements`, `usageKeys`), bake/src/reach.rs, runner/src/device.rs, scripts/app.schema.json
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/20261113-ios-module-dynamic-framework-not-embedded.md

## What is wrong

An iOS build writes its entitlements and Info.plist keys only from what Exact derives: the
grants' rows (`reach`, `bake/src/reach.rs:195-225`; `usageKeys`, `host/apple/build.mjs:372`),
widgets, App Clips, associated domains, Sign in with Apple and `host.ios`'s fixed keys
(`entitlements`, `host/apple/build.mjs:322-363`). `host.ios` has no key for an arbitrary
entitlement or Info.plist key, and its one key for files, `resources`, copies them into the
bundle's `ExactResources/` folder, not its root (`scripts/app.schema.json`). An app whose own
Swift module calls a system API that Exact has no grant row for cannot sign or describe it:

- An entitlement no grant row names: nothing adds it to the signature.
- A usage description the module needs: these come only from grant rows. Saving to the photo
  library has one (the `photos-add` device grant gives `NSPhotoLibraryAddUsageDescription`,
  `runner/src/device.rs:281-288`), but a module with no matching grant line has no way to add
  the key.
- An app-level `PrivacyInfo.xcprivacy` at the bundle root, which App Store review requires of
  an app that uses a required-reason API: nothing writes or places one.

Each builder patches the signed bundle afterwards, and any build Exact runs for them (a drive's
rebuild, a plain `bun exact.mjs ios`) writes a bundle without the patch.

Push (`device.push` signs `aps-environment`, 69abb2e9) and Sign in with Apple on a device
(884c6d6a) are now derived.

## The failure

Three reports on macOS:

- An app ported from React Native, at `94b8671f7`: "Entitlements and Info.plist keys we couldn't
  declare: `aps-environment` (push), Sign in with Apple on device,
  `NSPhotoLibraryAddUsageDescription`. Patched in a post-build re-sign step."
- At `cf0606932`: no way to ship an app-level `PrivacyInfo.xcprivacy` at the bundle root; the
  builder added it to the signed IPA and re-signed before upload.
- A push diary: an ordinary `ios` build replaced the previously packaged push-capable app with a
  bare bundle, without the notification service extension the builder had added; about 15
  minutes tracing builds and signatures, then a wrapper script that builds, packages and
  reinstalls in one step.

## Fix

Under LLP 1069.008 D4 (amended 2026-11-11), entitlements remain derived from grant lines
and named manifest switches. `host.ios.infoPlist`, `host.macos.infoPlist`,
`host.ios.entitlements` and `host.macos.entitlements` refuse every supplied key,
giving tested grant advice, identifying a key Exact derives, or asking for a
feedback note when Exact never writes it. No dictionary value is merged into a bundle.

An app supplies `PrivacyInfo.xcprivacy` beside `app.json`; the build merges it
before signing. Admission of an unowned Info.plist key waits for a diary naming
one. The notification service extension remains a separate, open issue.

## Verified and still open

Upstream 40377f008 supports an app's `PrivacyInfo.xcprivacy` beside `app.json`,
merged before signing (`host/apple/privacy.mjs`, `host/apple/bundle.mjs`). The
report's push, Sign in with Apple and photo-add cases have grant-derived or
named-switch paths (`host/apple/build.mjs`, `host/apple/presence.mjs`,
`runner/src/device.rs`). Package plist declarations compose through
`host/apple/plist.mjs`.

LLP 1069.008 D4 keeps entitlements derived. `host.ios.infoPlist`,
`host.macos.infoPlist`, `host.ios.entitlements` and `host.macos.entitlements` now
refuse every supplied key in one diagnostic. Tested grant lines include any
companion grants. Other derived keys point to the bundle metadata reference;
a key Exact never writes asks for a feedback note naming it. The native-module and app-json
recipes explain the grant path, including `device.photos-add`.

Admission of unowned Info.plist keys still needs a decision informed by a diary
naming one, so this issue stays open. The notification service extension is tracked
separately in `issues/20261113-an-ios-build-cannot-package-a.md`; it remains outside
the visible-push admission (`rules/DEFERRED.md`, notifications).
