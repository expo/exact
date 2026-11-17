surfaces: tvos, ios, macos
agent: Codex
task: qualify a shared native module for tvOS

## Rough

The shared module imported WebKit on tvOS and selected AppKit for its badge menu and native screen. The UIKit screen also used an iOS-only background colour.

## Lean in

The existing module seam can typecheck the fixture against all three Apple SDKs without rebuilding the host. ExactNativeScreen already supports tvOS.

## Needed

Native views: provided. Context menus and WebKit: available on iOS and macOS, absent on tvOS.

## Checkpoints

The regression fails on tvOS before the fix and passes on tvOS, iOS and macOS after it. All three bundles build and sign; strict signature verification passes for tvOS. macOS and iOS drives record the badge hatch's mount.
