# On Apple a second session's activation deletes the files the first one picked or was shared

**Status:** Closed
**Resolution:** `set_roots` empties `app:/tmp/picked/` and `app:/tmp/shared/` once a process for each `app:/tmp` root, on Apple and on Linux and Windows, so another session's activation or a restart with carry leaves the files this launch picked or was shared.
**Systems:** host/apple/src/picker.rs, host/apple/src/host.rs, host/apple/src/activation.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, `picker::set_roots` (`host/apple/src/picker.rs:183-189`) removes `app:/tmp/picked/` and `app:/tmp/shared/`, "the last launch's picks and received shares". It runs from `Host::configure_storage` (`host/apple/src/host.rs:757-768`), which each `Host` calls once when it activates its data after first pixel (`host/apple/src/activation.rs:10-13`). That is once for each session's runtime, not once a launch.

A process holds several sessions of one app, and they share the app's directories:

- on macOS, an app whose manifest sets `launch_handler.client_mode` to `navigate-new` opens a session for File ▸ New Window, the tab bar's + and each further document (`host/apple/Sources/ExactMac/main.swift:557-561`, `:761-767`); the Markdown and LLP readers are such apps;
- an embedder calls `ExactApp.makeSession` as often as it likes (LLP 1031), as the sample hosts do twice (`host/apple/Sources/ExactHostMac/main.swift:51-52`).

So a later session's activation deletes files an earlier one still names. LLP 1069.002 D4 says a picked path "must outlive" the picker's own URL, and that the directory "is emptied at launch".

By reading, a restart with carry does the same: `commit_plan` installs a new `Host` (`host/apple/src/abi.rs:911-943`), the session activates it for its new generation (`host/apple/Sources/ExactKit/SessionActivation.swift:14-15`), and the carried state still holds the paths. That is every development reload and an update applied while the app is open. This path was not run.

## The failure

In a `navigate-new` Mac app with a file input, pick a photo, so the window's state holds `app:/tmp/picked/<seed>-1.jpg`. Press ⌘N. When the new window has drawn, the file is gone: the first window's preview no longer loads and its upload's `readFile` fails. No app in the repository both opens windows this way and picks files.

Two sessions of one app, each booted in a window, in one process:

```
the first session's app:/tmp: …/Library/Caches/exact/com.exact.caltrain/temporary
the first session's pick exists: true
the second session's app:/tmp: …/Library/Caches/exact/com.exact.caltrain/temporary
after the second session activated, the first session's pick exists: false
```

## Verification

Run on macOS, twice. An XCTest (`bun host/apple/build.mjs --test --only <its class>`, with `HOME` pointed at a scratch directory) made two sessions with `ExactApp.shared.makeSession`, booted each in an `NSWindow`, waited for `dataGeneration == generation`, wrote `picked/7-1.jpg` under the first session's `appFileRoots["tmp"]` between the two boots, and printed the lines above. A Rust test beside `host/apple/src/storage_tests.rs` that boots two `Host`s of one app id and calls `activate_data` on each printed the same result.

## Direction

Empty the two directories once a process for each set of roots, as `empty_fresh_tree` already does with `EMPTIED` (`picker.rs:83-114`). `ROOTS` (`picker.rs:14`) is then only what `pickedPath`, `appRoots`, `appFile` and a landed share read; answering those from the asking session's own roots, as `file_op` does for `shareFile` (`picker.rs:240-249`), removes the last process-wide state.

## Related

`issues/20261110-picker-answer-after-reload-sets-another-node.md` is a date picker's node id, unrelated. Nothing in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md` covers `app:/tmp/picked` and a second session.
