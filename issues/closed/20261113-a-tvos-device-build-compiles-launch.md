# A tvOS device build compiles launch services as iOS

**Status:** Closed
**Resolution:** Every launch service compiles with `iosTripleFor(app, device, tv, compiled.ios)`, so a `--tvos --device` build embeds a tvOS service and iOS services follow `compiled.ios`.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`--device` and `--tvos` are independent. The launch-service compile chooses its target with `device` first: `arm64-apple-ios17.0` whenever `--device` is set, including `--tvos --device`, while the SDK is appletvos. swiftc rejects an iOS triple with the tvOS SDK, or the app embeds an iOS dylib that tvOS will not load. iOS device and simulator services are also fixed at the 17.0 triple, so they ignore `compiled.ios`, which is the app minimum whenever those services exist. The executable and the web arm already use `iosTripleFor(app, device, tv, compiled.ios)`.

## Where

`host/apple/build.mjs`, the `serviceDylibs` compile. `device` and `tv` are set from `--device` and `--tvos`.

## How to reproduce

Build an app that has a launch service with `--tvos --device`. The service is compiled as `arm64-apple-ios17.0` against the tvOS SDK.

## Direction

Compile every service with `iosTripleFor(app, device, tv, compiled.ios)`.
