# The Hermes pin test treats Android as Linux

**Status:** Closed
**Resolution:** The pin test maps an -android triple to android before its -linux- arm and asserts a pinned target maps there, so dropping android from ENGINE_OS fails it.
**Systems:** js
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

`target_os` maps any triple that contains `-linux-` to `linux`. `aarch64-linux-android` takes that arm. `ENGINE_OS` lists `android` on its own, and `build.rs` returns before it builds the shim when the target OS is absent from that list. The pin test then treats the Android bundle as covered by the Linux shim.

Removing `android` from `ENGINE_OS` leaves the test green. An Android build of that tree loads no engine. The list's own comment says an OS left out refuses every TypeScript app at run time.

## Where

`js/tests/it/pinned_engines.rs`, `target_os` and `every_pinned_bundle_target_links_the_engine`. `js/engine_os.rs`, `ENGINE_OS`. `js/build.rs`, the early return. `vendor/ibex/crates/hermes-lean-sys/build_support.rs`, the `aarch64-linux-android` pin.

## How to reproduce

`target_os("aarch64-linux-android")` is `linux`. Drop `android` from `ENGINE_OS` and run `every_pinned_bundle_target_links_the_engine`. It passes. Build that tree for Android and the shim is skipped.

## Direction

Match an `-android` triple before the `-linux-` arm, and assert that `aarch64-linux-android` maps to `android`.
