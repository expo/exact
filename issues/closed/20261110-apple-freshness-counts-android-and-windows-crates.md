# An edit to an app's Android or Windows crate makes its Apple build stale to the driver

**Status:** Closed
**Resolution:** `otherHosts` (`scripts/agent-launch.mjs`) skips the app's host crates from `HOST_CRATES` (`scripts/cargo-check.mjs`) but the check's own, and Android's Gradle lock and checksums, and `receiptChanges`, `webInputs` and `planSources` use it (the `windowsBuild` copy no longer exists).
**Systems:** scripts/agent-launch.mjs (`receiptChanges`, `webInputs`), scripts/agent-build.mjs (`planSources`, `windowsBuild`)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

Each freshness check walks the app's directory and leaves out the crates of the hosts it is not
checking. The rule is written four times, with four lists:

| where | leaves out |
|---|---|
| `scripts/agent-launch.mjs:950` (`receiptChanges`, the Apple receipt) | `apple ios macos linux web` |
| `scripts/agent-launch.mjs:996` (`webInputs`) | `apple ios macos linux` |
| `scripts/agent-build.mjs:184` (`planSources`) | `apple ios macos linux web` |
| `scripts/agent-build.mjs:392` (`windowsBuild`) | `apple ios macos linux web android` |

None of the first three names `android` or `windows`. Those directories hold a `Cargo.toml`, so
`notBuildInput` keeps every file in them as an input (`scripts/agent-launch.mjs:922-931`,
`inCrate`). The Apple check therefore counts the Android and Windows crates, and the Gradle files
beside `app.json`, as inputs of the macOS and iOS build. After an edit to one of them the
driver refuses that build as stale, and an `exact new` app's `agent` or `test` runs a native Apple
build that compiles nothing (`buildFirst`, `scripts/agent-build.mjs:347`).

## The failure

Shelf has `android/`, `windows/`, `linux/`, `web/` and `apple/`.

```
bun host/apple/build.mjs shelf-apple
bun scripts/agent.mjs macos --app shelf --storage v1 tree      # the tree, exit 0
echo '// x' >> apps/shelf/android/src/lib.rs
bun scripts/agent.mjs macos --app shelf --storage v1 tree
```

Observed, exit 3:

```
macos build is stale: apps/shelf/android/src/lib.rs changed since target/clients/…/receipt.json was built; run bun host/apple/build.mjs shelf-apple
```

The build it asks for reports `cargo 1.0 s`. Expected: the Apple check leaves out every other
host's crate, as it leaves out Linux's.

## Verification

Ran the commands above in a worktree at `dd6f5dcd`; the output is as shown. From a fresh build
each time, an edit to `apps/shelf/windows/src/main.rs` and one to `apps/shelf/gradle.lockfile` are
refused the same way, and an edit to `apps/shelf/linux/src/main.rs` is not: the drive prints the
tree. `receiptChanges`, called with a receipt older than every file, lists Shelf's `android/` and
`windows/` files, `gradle.lockfile` and `gradle-verification.xml`, and nothing from `linux/` or
`web/`. `planSources` keeps only plan sources, so its list shows in no drive today. The iOS build
and an `exact new` app's `buildFirst` were not run; they read the same `receiptChanges`.

## Direction

One list of the app's host crate directories (`APP_CRATES` in `scripts/cargo-check.mjs:68` already
names them), and each check leaves out all but its own.

## Related

`QUEUE.md`, "Conformance after the Shelf bisect, what is owed", item (5), records the web half
(`webInputs` and `apps/<app>/android`). This is the Apple half and the four copies. `QUEUE.md`,
"Editing a package's `test/` makes an app's Apple build stale", is a package's directory, not a
host crate.
